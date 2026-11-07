//! Muxers assíncronos e desacoplados: escrita rápida de clipes e gravação contínua.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;
use luna_stream_proto::{ContainerFormat, ErrorCode, RecordStopResult, RpcError};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;
use tracing::{error, info, warn};

use crate::recorder::validate_output;
use crate::replay_buffer::{ClipSnapshot, EncodedPacket, TrackId};

fn rebase_buffer(packet: &EncodedPacket, base_pts: gst::ClockTime) -> Option<gst::Buffer> {
    let pts = packet.pts()?;
    if pts < base_pts {
        return None;
    }
    let rebased_pts = pts.saturating_sub(base_pts);
    let mut buf = packet.buffer.copy();
    let b = buf.get_mut()?;
    b.set_pts(rebased_pts);
    if let Some(dts) = packet.dts() {
        b.set_dts(dts.saturating_sub(base_pts));
    }
    Some(buf)
}

/// Escreve um clipe para disco de forma não-live na velocidade do SSD (< 1 s).
pub fn write_clip_offline(
    snapshot: ClipSnapshot,
    format: ContainerFormat,
    output: &Path,
) -> Result<u64, RpcError> {
    validate_output(output)?;

    if snapshot.segments.is_empty() {
        return Err(ErrorCode::ReplayEmpty.into());
    }

    let mut packets_by_track: BTreeMap<TrackId, Vec<gst::Buffer>> = BTreeMap::new();
    let mut caps_by_track: HashMap<TrackId, gst::Caps> = HashMap::new();

    for seg in &snapshot.segments {
        for pkt in &seg.packets {
            if let Some(buf) = rebase_buffer(pkt, snapshot.start_pts) {
                caps_by_track
                    .entry(pkt.track)
                    .or_insert_with(|| pkt.caps.clone());
                packets_by_track.entry(pkt.track).or_default().push(buf);
            }
        }
    }

    // Deve conter obrigatoriamente a trilha de vídeo com pelo menos um pacote
    let video_buffers = match packets_by_track.get(&TrackId::Video) {
        Some(bufs) if !bufs.is_empty() => bufs,
        _ => return Err(ErrorCode::ReplayEmpty.into()),
    };

    let out_str = output.to_string_lossy();
    let mux_and_sink = match format {
        ContainerFormat::Mp4 => {
            format!(
                "mp4mux fragment-duration=1000 name=mux ! filesink buffer-mode=unbuffered location=\"{out_str}\""
            )
        }
        ContainerFormat::Mkv => {
            format!(
                "matroskamux min-cluster-duration=500000000 offset-to-zero=true name=mux ! filesink buffer-mode=unbuffered location=\"{out_str}\""
            )
        }
    };

    // Monta appsrc apenas para as trilhas que possuem buffers presentes
    let mut parts = Vec::new();
    let mut active_tracks = Vec::new();

    for (idx, (track, _)) in packets_by_track.iter().enumerate() {
        let parser = match track {
            TrackId::Video => "h264parse",
            _ => {
                if let Some(caps) = caps_by_track.get(track) {
                    let name = caps.structure(0).map(|s| s.name().as_str()).unwrap_or("");
                    if name.contains("opus") {
                        "opusparse"
                    } else {
                        "aacparse"
                    }
                } else {
                    "aacparse"
                }
            }
        };

        parts.push(format!(
            "appsrc name=src_{idx} format=time is-live=false ! {parser} ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 ! mux."
        ));
        active_tracks.push(*track);
    }
    parts.push(mux_and_sink);

    let desc = parts.join(" ");
    let elem = gst::parse::launch(&desc).map_err(|e| {
        error!("Erro compilando pipeline de clipe offline: {e}");
        RpcError::with_data(
            ErrorCode::PipelineFailed as i32,
            ErrorCode::PipelineFailed.message(),
            serde_json::json!({ "error": e.to_string() }),
        )
    })?;

    let pipeline = elem
        .dynamic_cast::<gst::Pipeline>()
        .map_err(|_| RpcError::from(ErrorCode::PipelineFailed))?;

    // Configura caps nos appsrc antes de iniciar reprodução
    for (idx, track) in active_tracks.iter().enumerate() {
        let src_elem = pipeline
            .by_name(&format!("src_{idx}"))
            .ok_or(ErrorCode::PipelineFailed)?;
        let appsrc = src_elem
            .dynamic_cast::<AppSrc>()
            .map_err(|_| RpcError::from(ErrorCode::PipelineFailed))?;
        if let Some(caps) = caps_by_track.get(track) {
            appsrc.set_caps(Some(caps));
        }
    }

    pipeline
        .set_state(gst::State::Playing)
        .map_err(|e| RpcError::new(ErrorCode::PipelineFailed as i32, format!("{e:?}")))?;

    // Empurra os buffers e emite EOS
    for (idx, track) in active_tracks.iter().enumerate() {
        let src_elem = pipeline
            .by_name(&format!("src_{idx}"))
            .ok_or(ErrorCode::PipelineFailed)?;
        let appsrc = src_elem
            .dynamic_cast::<AppSrc>()
            .map_err(|_| RpcError::from(ErrorCode::PipelineFailed))?;

        if let Some(bufs) = packets_by_track.get(track) {
            for buf in bufs {
                let _ = appsrc.push_buffer(buf.clone());
            }
        }
        let _ = appsrc.end_of_stream();
    }

    // Aguarda o término da muxagem
    if let Some(bus) = pipeline.bus() {
        let _ = bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(5),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        );
    }

    let _ = pipeline.set_state(gst::State::Null);

    let bytes = fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    let _ = video_buffers; // Mantém referência viva
    Ok(bytes)
}

enum MuxMsg {
    Packet(EncodedPacket),
    Stop,
}

pub struct LiveRecordMuxer {
    tx: Mutex<Option<SyncSender<MuxMsg>>>,
    worker: Mutex<Option<JoinHandle<Result<RecordStopResult, RpcError>>>>,
    start_time: Instant,
    output: PathBuf,
    encoder_name: String,
    format: ContainerFormat,
    bytes_written: Arc<AtomicU64>,
    last_error: Arc<Mutex<Option<String>>>,
    active: Arc<AtomicBool>,
}

impl LiveRecordMuxer {
    pub fn start(
        tracks: &[TrackId],
        format: ContainerFormat,
        output: PathBuf,
        encoder_name: String,
    ) -> Result<Self, RpcError> {
        validate_output(&output)?;

        let out_str = output.to_string_lossy();
        let mux_and_sink = match format {
            ContainerFormat::Mp4 => {
                format!(
                    "mp4mux fragment-duration=1000 name=mux ! filesink buffer-mode=unbuffered location=\"{out_str}\""
                )
            }
            ContainerFormat::Mkv => {
                format!(
                    "matroskamux min-cluster-duration=500000000 offset-to-zero=true name=mux ! filesink buffer-mode=unbuffered location=\"{out_str}\""
                )
            }
        };

        let mut parts = Vec::new();
        for track in tracks {
            let parser = match track {
                TrackId::Video => "h264parse",
                _ => match format {
                    ContainerFormat::Mp4 => "aacparse",
                    ContainerFormat::Mkv => "opusparse",
                },
            };
            parts.push(format!(
                "appsrc name={} is-live=true do-timestamp=false format=time max-bytes=4194304 ! {parser} ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 ! mux.",
                track.sink_name()
            ));
        }
        parts.push(mux_and_sink);

        let desc = parts.join(" ");
        let elem = gst::parse::launch(&desc).map_err(|e| {
            error!("Erro compilando pipeline de gravação contínua: {e}");
            RpcError::with_data(
                ErrorCode::PipelineFailed as i32,
                ErrorCode::PipelineFailed.message(),
                serde_json::json!({ "error": e.to_string() }),
            )
        })?;

        let pipeline = elem
            .dynamic_cast::<gst::Pipeline>()
            .map_err(|_| RpcError::from(ErrorCode::PipelineFailed))?;

        let (tx, rx) = sync_channel::<MuxMsg>(512);
        let bytes_written = Arc::new(AtomicU64::new(0));
        let last_error = Arc::new(Mutex::new(None));
        let active = Arc::new(AtomicBool::new(true));

        let bw_clone = Arc::clone(&bytes_written);
        let err_clone = Arc::clone(&last_error);
        let act_clone = Arc::clone(&active);
        let out_clone = output.clone();
        let track_list = tracks.to_vec();

        pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| RpcError::new(ErrorCode::PipelineFailed as i32, format!("{e:?}")))?;

        let worker = std::thread::spawn(move || {
            Self::worker_loop(
                pipeline, rx, track_list, out_clone, bw_clone, err_clone, act_clone,
            )
        });

        Ok(Self {
            tx: Mutex::new(Some(tx)),
            worker: Mutex::new(Some(worker)),
            start_time: Instant::now(),
            output,
            encoder_name,
            format,
            bytes_written,
            last_error,
            active,
        })
    }

    fn worker_loop(
        pipeline: gst::Pipeline,
        rx: Receiver<MuxMsg>,
        tracks: Vec<TrackId>,
        output: PathBuf,
        bytes_written: Arc<AtomicU64>,
        last_error: Arc<Mutex<Option<String>>>,
        active: Arc<AtomicBool>,
    ) -> Result<RecordStopResult, RpcError> {
        let mut appsrc_map = HashMap::new();
        let mut caps_set = HashSet::new();
        for track in &tracks {
            if let Some(elem) = pipeline.by_name(track.sink_name()) {
                if let Ok(appsrc) = elem.dynamic_cast::<AppSrc>() {
                    appsrc_map.insert(*track, appsrc);
                }
            }
        }

        let mut origin_pts: Option<gst::ClockTime> = None;
        let start_instant = Instant::now();

        while let Ok(msg) = rx.recv() {
            match msg {
                MuxMsg::Packet(pkt) => {
                    if origin_pts.is_none() {
                        if pkt.is_video_keyframe() {
                            origin_pts = pkt.pts();
                            info!("Gravação contínua alinhada em keyframe (origin={origin_pts:?})");
                        } else {
                            continue;
                        }
                    }

                    let base = origin_pts.unwrap_or(gst::ClockTime::ZERO);
                    if let Some(appsrc) = appsrc_map.get(&pkt.track) {
                        if caps_set.insert(pkt.track) {
                            appsrc.set_caps(Some(&pkt.caps));
                        }

                        if let Some(buf) = rebase_buffer(&pkt, base) {
                            let _ = appsrc.push_buffer(buf);
                        }
                    }

                    if let Ok(meta) = fs::metadata(&output) {
                        bytes_written.store(meta.len(), Ordering::Relaxed);
                    }
                }
                MuxMsg::Stop => {
                    break;
                }
            }
        }

        // Finaliza gravação enviando EOS em cada appsrc
        for appsrc in appsrc_map.values() {
            let _ = appsrc.end_of_stream();
        }

        if let Some(bus) = pipeline.bus() {
            let msg = bus.timed_pop_filtered(
                gst::ClockTime::from_seconds(5),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            );
            if let Some(msg) = msg {
                if let gst::MessageView::Error(err) = msg.view() {
                    error!("Erro no bus durante encerramento de gravação: {err}");
                    *last_error.lock().unwrap() = Some(err.error().to_string());
                }
            }
        }

        let _ = pipeline.set_state(gst::State::Null);
        active.store(false, Ordering::Relaxed);

        let final_bytes = fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
        bytes_written.store(final_bytes, Ordering::Relaxed);
        let duration_ms = start_instant.elapsed().as_millis() as u64;

        Ok(RecordStopResult {
            path: output,
            duration_ms,
            bytes_written: final_bytes,
        })
    }

    pub fn push(&self, packet: &EncodedPacket) {
        if !self.active.load(Ordering::Relaxed) {
            return;
        }
        if let Some(ref tx) = *self.tx.lock().unwrap() {
            match tx.try_send(MuxMsg::Packet(packet.clone())) {
                Ok(_) => {}
                Err(TrySendError::Full(_)) => {
                    warn!("Fila de gravação cheia (disco lento); sinalizando parada");
                    *self.last_error.lock().unwrap() =
                        Some("Fila de gravação cheia (disco lento)".into());
                    self.active.store(false, Ordering::Relaxed);
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.active.store(false, Ordering::Relaxed);
                }
            }
        }
    }

    pub fn status(&self) -> (u64, u64, PathBuf, String, ContainerFormat, Option<String>) {
        let duration_ms = self.start_time.elapsed().as_millis() as u64;
        let bytes = self.bytes_written.load(Ordering::Relaxed);
        let err = self.last_error.lock().unwrap().clone();
        (
            duration_ms,
            bytes,
            self.output.clone(),
            self.encoder_name.clone(),
            self.format,
            err,
        )
    }

    pub async fn stop(&self) -> Result<RecordStopResult, RpcError> {
        if let Some(tx) = self.tx.lock().unwrap().take() {
            let _ = tx.send(MuxMsg::Stop);
        }
        let worker = self.worker.lock().unwrap().take();
        if let Some(worker) = worker {
            tokio::task::spawn_blocking(move || worker.join().map_err(|_| ErrorCode::Internal)?)
                .await
                .map_err(|_| RpcError::from(ErrorCode::Internal))?
        } else {
            Err(ErrorCode::NotRecording.into())
        }
    }
}
