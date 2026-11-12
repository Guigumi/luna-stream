//! Gerenciamento unificado da sessão de captura, roteamento de pacotes e coexistência.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSink;
use luna_stream_proto::{
    ErrorCode, HotkeysConfigureParams, HotkeysConfigureResult, RecordStartParams,
    RecordStartResult, RecordStatusResult, RecordStopResult, ReplaySaveParams, ReplaySaveResult,
    ReplayStartParams, ReplayStartResult, ReplayStatusResult, ReplayStopResult, RpcError,
    SourceKind, SystemAudioMode,
};
#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as TokioMutex;
use tracing::error;

use crate::audio::router::{AudioGraph, AudioRouter};
use crate::encoder;
use crate::hotkey::{
    HotkeyAction, HotkeyHandle, notify_clip_saved, notify_error, notify_recording_state,
};
use crate::muxer::{LiveRecordMuxer, write_clip_offline};
use crate::naming::{resolve_clip_path, resolve_output_path};
use crate::pipeline::{AudioSourceKind, VideoSource, describe_producer};
use crate::portal::{self, PortalSession};
use crate::replay_buffer::{EncodedPacket, ReplayRingBuffer, TrackId};
use crate::token_store::TokenStore;

pub struct CaptureSession {
    pipeline: gst::Pipeline,
    _portal_session: Option<PortalSession>,
    audio_router: Option<AudioRouter>,
    encoder_name: String,
    tracks: Vec<TrackId>,
    replay_buffer: Arc<Mutex<Option<ReplayRingBuffer>>>,
    record_muxer: Arc<Mutex<Option<LiveRecordMuxer>>>,
    hotkey_handle: Option<HotkeyHandle>,
    hotkey_bound: bool,
    hotkey_trigger: Option<String>,
    replay_duration_secs: u32,
    replay_max_ram_mb: u32,
    last_error: Arc<Mutex<Option<String>>>,
    bus_abort: tokio::task::AbortHandle,
}

impl CaptureSession {
    pub fn force_keyunit(&self) {
        let event = gstreamer_video::UpstreamForceKeyUnitEvent::builder()
            .all_headers(true)
            .build();
        if let Some(vsink) = self.pipeline.by_name("vsink") {
            let _ = vsink.send_event(event);
        }
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.bus_abort.abort();
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[derive(Clone)]
pub struct SessionManager {
    session: Arc<TokioMutex<Option<CaptureSession>>>,
    save_mutex: Arc<TokioMutex<()>>,
    record_defaults: Arc<TokioMutex<RecordStartParams>>,
    global_hotkey: Arc<TokioMutex<Option<HotkeyHandle>>>,
    hotkey_config: Arc<TokioMutex<HotkeysConfigureResult>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            session: Arc::new(TokioMutex::new(None)),
            save_mutex: Arc::new(TokioMutex::new(())),
            record_defaults: Arc::new(TokioMutex::new(RecordStartParams::default())),
            global_hotkey: Arc::new(TokioMutex::new(None)),
            hotkey_config: Arc::new(TokioMutex::new(HotkeysConfigureResult {
                replay_bound: false,
                replay_trigger: "Alt+F8".into(),
                record_bound: false,
                record_trigger: "Alt+F9".into(),
            })),
        }
    }

    pub async fn configure_hotkeys(
        &self,
        params: HotkeysConfigureParams,
        token_store: Arc<TokenStore>,
    ) -> HotkeysConfigureResult {
        *self.record_defaults.lock().await = params.record_defaults;

        let mut hotkey_lock = self.global_hotkey.lock().await;
        if let Some(old_handle) = hotkey_lock.take() {
            old_handle.close();
        }

        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let (handle_opt, result) = crate::hotkey::bind_global_hotkeys(
            &params.replay_trigger,
            &params.record_trigger,
            action_tx,
        )
        .await;

        *hotkey_lock = handle_opt;
        *self.hotkey_config.lock().await = result.clone();

        let this = self.clone();
        tokio::spawn(async move {
            while let Some(action) = action_rx.recv().await {
                this.trigger_hotkey_action(action, &token_store).await;
            }
        });

        result
    }

    pub async fn trigger_hotkey_action(&self, action: HotkeyAction, token_store: &TokenStore) {
        match action {
            HotkeyAction::SaveClip => {
                let _guard = self.save_mutex.lock().await;
                let (format, output_dir) = {
                    let defs = self.record_defaults.lock().await;
                    (defs.format, defs.output_dir.clone())
                };
                let snap_opt = {
                    let lock = self.session.lock().await;
                    lock.as_ref().and_then(|s| {
                        s.replay_buffer
                            .lock()
                            .unwrap()
                            .as_ref()
                            .and_then(|rb| rb.snapshot(None).ok())
                    })
                };
                if let Some(snapshot) = snap_opt {
                    if let Ok(path) = resolve_clip_path(
                        output_dir.as_deref(),
                        None,
                        format,
                        None,
                        std::process::id(),
                        snapshot.total_bytes,
                    ) {
                        let res = tokio::task::spawn_blocking(move || {
                            write_clip_offline(snapshot, format, &path).map(|_| path)
                        })
                        .await;
                        if let Ok(Ok(saved_path)) = res {
                            notify_clip_saved(&saved_path).await;
                        }
                    }
                }
            }
            HotkeyAction::ToggleRecord => {
                let is_recording = self.record_status().await.recording;
                if is_recording {
                    if let Ok(res) = self.record_stop().await {
                        notify_recording_state(false, &res.path).await;
                    }
                } else {
                    let defaults = self.record_defaults.lock().await.clone();
                    if let Ok(res) = self.record_start(defaults, token_store).await {
                        notify_recording_state(true, &res.path).await;
                    }
                }
            }
        }
    }

    pub async fn record_start(
        &self,
        params: RecordStartParams,
        token_store: &TokenStore,
    ) -> Result<RecordStartResult, RpcError> {
        let dump_graph = #[cfg(unix)]
        {
            if params.output.is_none() && params.game_name.is_none() {
                tokio::process::Command::new("pw-dump")
                    .output()
                    .await
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .and_then(|s| AudioGraph::from_json(&s).ok())
            } else {
                None
            }
        };
        #[cfg(windows)]
        let dump_graph = None;

        let output_path = resolve_output_path(
            params.output.as_deref(),
            params.output_dir.as_deref(),
            params.game_name.as_deref(),
            params.format,
            dump_graph.as_ref(),
            std::process::id(),
        )?;

        let mut lock = self.session.lock().await;

        if let Some(ref mut session) = *lock {
            if session.record_muxer.lock().unwrap().is_some() {
                return Err(ErrorCode::AlreadyRecording.into());
            }

            let muxer = LiveRecordMuxer::start(
                &session.tracks,
                params.format,
                output_path.clone(),
                session.encoder_name.clone(),
            )?;
            *session.record_muxer.lock().unwrap() = Some(muxer);
            session.force_keyunit();

            return Ok(RecordStartResult {
                encoder: session.encoder_name.clone(),
                path: output_path,
                format: params.format,
                shared_session: true,
            });
        }

        // Sessão não existe: cria nova sessão de captura com encode único
        let probed = encoder::probe();
        let enc = if let Ok(override_enc) = std::env::var("LUNA_STREAM_ENCODER") {
            probed
                .into_iter()
                .find(|e| e.element == override_enc)
                .ok_or(ErrorCode::NoEncoder)?
        } else {
            probed.into_iter().next().ok_or(ErrorCode::NoEncoder)?
        };

        let (video_src, portal_session, audio_src) = match params.source {
            SourceKind::Test => (VideoSource::Test, None, AudioSourceKind::Test),
            SourceKind::Screen => {
                #[cfg(unix)]
                let result = {
                    let portal = portal::open_screen(token_store).await?;
                    let raw_fd = portal.fd.as_raw_fd();
                    let node_id = portal.node_id;
                    Ok((
                        VideoSource::PipeWire {
                            fd: raw_fd,
                            node_id,
                        },
                        Some(portal),
                        AudioSourceKind::PipeWire,
                    ))
                };
                #[cfg(windows)]
                let result = Err(RpcError::new(
                    ErrorCode::Internal as i32,
                    "Captura de tela não implementada no Windows (marco W2)".to_string(),
                ));
                result?
            }
        };

        let audio_router = if audio_src == AudioSourceKind::PipeWire
            && params.audio.mode == SystemAudioMode::Isolated
        {
            let game = if params.audio.game {
                Some("luna_game".to_string())
            } else {
                None
            };
            let call = if params.audio.call {
                Some("luna_call".to_string())
            } else {
                None
            };
            if game.is_some() || call.is_some() {
                let mut router = AudioRouter::new(game, call);
                let _ = router.sync().await;
                router.start_periodic_sync(Duration::from_secs(2));
                Some(router)
            } else {
                None
            }
        } else {
            None
        };

        let (desc, tracks) = describe_producer(
            &video_src,
            &enc,
            params.fps,
            &params.video,
            &params.audio,
            audio_src,
            params.format,
        );

        let elem = gst::parse::launch(&desc).map_err(|e| {
            error!("Erro compilando pipeline produtor: {e}");
            RpcError::with_data(
                ErrorCode::PipelineFailed as i32,
                ErrorCode::PipelineFailed.message(),
                serde_json::json!({ "error": e.to_string(), "pipeline": desc }),
            )
        })?;

        let pipeline = elem
            .dynamic_cast::<gst::Pipeline>()
            .map_err(|_| RpcError::from(ErrorCode::PipelineFailed))?;

        let replay_buffer: Arc<Mutex<Option<ReplayRingBuffer>>> = Arc::new(Mutex::new(None));
        let record_muxer: Arc<Mutex<Option<LiveRecordMuxer>>> = Arc::new(Mutex::new(None));
        let last_error = Arc::new(Mutex::new(None));

        for track in &tracks {
            let sink_elem = pipeline
                .by_name(track.sink_name())
                .ok_or(ErrorCode::PipelineFailed)?;
            let appsink = sink_elem
                .dynamic_cast::<AppSink>()
                .map_err(|_| RpcError::from(ErrorCode::PipelineFailed))?;

            let rb_clone = Arc::clone(&replay_buffer);
            let rm_clone = Arc::clone(&record_muxer);
            let trk = *track;

            appsink.set_callbacks(
                gstreamer_app::AppSinkCallbacks::builder()
                    .new_sample(move |sink| {
                        if let Ok(sample) = sink.pull_sample() {
                            if let (Some(buffer), Some(caps)) = (sample.buffer(), sample.caps()) {
                                let pkt = EncodedPacket {
                                    track: trk,
                                    buffer: buffer.to_owned(),
                                    caps: caps.to_owned(),
                                };
                                if let Some(ref mut rb) = *rb_clone.lock().unwrap() {
                                    rb.push(pkt.clone());
                                }
                                if let Some(ref rm) = *rm_clone.lock().unwrap() {
                                    rm.push(&pkt);
                                }
                            }
                        }
                        Ok(gst::FlowSuccess::Ok)
                    })
                    .build(),
            );
        }

        pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| RpcError::new(ErrorCode::PipelineFailed as i32, format!("{e:?}")))?;

        let muxer = LiveRecordMuxer::start(
            &tracks,
            params.format,
            output_path.clone(),
            enc.element.clone(),
        )?;
        *record_muxer.lock().unwrap() = Some(muxer);

        let bus_err_clone = Arc::clone(&last_error);
        let bus_rm_clone = Arc::clone(&record_muxer);
        let bus_rb_clone = Arc::clone(&replay_buffer);
        let bus = pipeline.bus().ok_or(ErrorCode::PipelineFailed)?;

        let bus_task = tokio::spawn(async move {
            let bus = bus;
            tokio::task::spawn_blocking(move || {
                while let Some(msg) = bus.timed_pop_filtered(
                    gst::ClockTime::from_mseconds(500),
                    &[gst::MessageType::Error, gst::MessageType::Eos],
                ) {
                    if let gst::MessageView::Error(err) = msg.view() {
                        let err_msg = err.error().to_string();
                        error!("Erro fatal no barramento do pipeline produtor: {err_msg}");
                        *bus_err_clone.lock().unwrap() = Some(err_msg.clone());
                        let rm = bus_rm_clone.lock().unwrap().take();
                        if let Some(muxer) = rm {
                            tokio::spawn(async move {
                                let _ = muxer.stop().await;
                            });
                        }
                        *bus_rb_clone.lock().unwrap() = None;
                        tokio::spawn(async move {
                            notify_error("Erro de captura", &err_msg).await;
                        });
                        break;
                    }
                }
            })
            .await
        });

        *lock = Some(CaptureSession {
            pipeline,
            _portal_session: portal_session,
            audio_router,
            encoder_name: enc.element.clone(),
            tracks,
            replay_buffer,
            record_muxer,
            hotkey_handle: None,
            hotkey_bound: false,
            hotkey_trigger: None,
            replay_duration_secs: 30,
            replay_max_ram_mb: 512,
            last_error,
            bus_abort: bus_task.abort_handle(),
        });

        Ok(RecordStartResult {
            encoder: enc.element,
            path: output_path,
            format: params.format,
            shared_session: false,
        })
    }

    pub async fn record_status(&self) -> RecordStatusResult {
        let lock = self.session.lock().await;
        if let Some(ref session) = *lock {
            if let Some(ref muxer) = *session.record_muxer.lock().unwrap() {
                let (duration_ms, bytes_written, path, encoder, format, last_err) = muxer.status();
                return RecordStatusResult {
                    recording: true,
                    path: Some(path),
                    duration_ms,
                    bytes_written,
                    encoder: Some(encoder),
                    format: Some(format),
                    last_error: last_err.or_else(|| session.last_error.lock().unwrap().clone()),
                };
            }
        }

        RecordStatusResult {
            recording: false,
            path: None,
            duration_ms: 0,
            bytes_written: 0,
            encoder: None,
            format: None,
            last_error: None,
        }
    }

    pub async fn record_stop(&self) -> Result<RecordStopResult, RpcError> {
        let mut lock = self.session.lock().await;
        let session = lock.as_mut().ok_or(ErrorCode::NotRecording)?;

        let muxer = session
            .record_muxer
            .lock()
            .unwrap()
            .take()
            .ok_or(ErrorCode::NotRecording)?;
        let res = muxer.stop().await?;

        // Se replay não estiver ativo, encerra a sessão de captura
        let replay_active = session.replay_buffer.lock().unwrap().is_some();
        if !replay_active {
            if let Some(mut s) = lock.take() {
                s.bus_abort.abort();
                if let Some(mut router) = s.audio_router.take() {
                    router.stop().await;
                }
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = s.pipeline.set_state(gst::State::Null);
                })
                .await;
            }
        }

        Ok(res)
    }

    pub async fn replay_start(
        &self,
        params: ReplayStartParams,
        token_store: &TokenStore,
    ) -> Result<ReplayStartResult, RpcError> {
        params.validate()?;

        let mut lock = self.session.lock().await;

        if let Some(ref mut session) = *lock {
            if session.replay_buffer.lock().unwrap().is_some() {
                return Err(ErrorCode::AlreadyReplaying.into());
            }

            let rb = ReplayRingBuffer::new(
                Duration::from_secs(params.duration_secs as u64),
                (params.max_ram_mb as u64) * 1024 * 1024,
            );
            *session.replay_buffer.lock().unwrap() = Some(rb);
            session.replay_duration_secs = params.duration_secs;
            session.replay_max_ram_mb = params.max_ram_mb;

            return Ok(ReplayStartResult {
                encoder: session.encoder_name.clone(),
                duration_secs: params.duration_secs,
                max_ram_mb: params.max_ram_mb,
                shared_session: true,
                hotkey_bound: session.hotkey_bound,
                hotkey_trigger: session.hotkey_trigger.clone(),
            });
        }

        // Sessão não existe: cria nova sessão de captura para o Replay
        let probed = encoder::probe();
        let enc = if let Ok(override_enc) = std::env::var("LUNA_STREAM_ENCODER") {
            probed
                .into_iter()
                .find(|e| e.element == override_enc)
                .ok_or(ErrorCode::NoEncoder)?
        } else {
            probed.into_iter().next().ok_or(ErrorCode::NoEncoder)?
        };

        let (video_src, portal_session, audio_src) = match params.source {
            SourceKind::Test => (VideoSource::Test, None, AudioSourceKind::Test),
            SourceKind::Screen => {
                #[cfg(unix)]
                let result = {
                    let portal = portal::open_screen(token_store).await?;
                    let raw_fd = portal.fd.as_raw_fd();
                    let node_id = portal.node_id;
                    Ok((
                        VideoSource::PipeWire {
                            fd: raw_fd,
                            node_id,
                        },
                        Some(portal),
                        AudioSourceKind::PipeWire,
                    ))
                };
                #[cfg(windows)]
                let result = Err(RpcError::new(
                    ErrorCode::Internal as i32,
                    "Captura de tela não implementada no Windows (marco W2)".to_string(),
                ));
                result?
            }
        };

        let audio_router = if audio_src == AudioSourceKind::PipeWire
            && params.audio.mode == SystemAudioMode::Isolated
        {
            let game = if params.audio.game {
                Some("luna_game".to_string())
            } else {
                None
            };
            let call = if params.audio.call {
                Some("luna_call".to_string())
            } else {
                None
            };
            if game.is_some() || call.is_some() {
                let mut router = AudioRouter::new(game, call);
                let _ = router.sync().await;
                router.start_periodic_sync(Duration::from_secs(2));
                Some(router)
            } else {
                None
            }
        } else {
            None
        };

        let (desc, tracks) = describe_producer(
            &video_src,
            &enc,
            params.fps,
            &params.video,
            &params.audio,
            audio_src,
            params.format,
        );

        let elem = gst::parse::launch(&desc).map_err(|e| {
            error!("Erro compilando pipeline produtor: {e}");
            RpcError::with_data(
                ErrorCode::PipelineFailed as i32,
                ErrorCode::PipelineFailed.message(),
                serde_json::json!({ "error": e.to_string(), "pipeline": desc }),
            )
        })?;

        let pipeline = elem
            .dynamic_cast::<gst::Pipeline>()
            .map_err(|_| RpcError::from(ErrorCode::PipelineFailed))?;

        let rb = ReplayRingBuffer::new(
            Duration::from_secs(params.duration_secs as u64),
            (params.max_ram_mb as u64) * 1024 * 1024,
        );
        let replay_buffer: Arc<Mutex<Option<ReplayRingBuffer>>> = Arc::new(Mutex::new(Some(rb)));
        let record_muxer: Arc<Mutex<Option<LiveRecordMuxer>>> = Arc::new(Mutex::new(None));
        let last_error = Arc::new(Mutex::new(None));

        for track in &tracks {
            let sink_elem = pipeline
                .by_name(track.sink_name())
                .ok_or(ErrorCode::PipelineFailed)?;
            let appsink = sink_elem
                .dynamic_cast::<AppSink>()
                .map_err(|_| RpcError::from(ErrorCode::PipelineFailed))?;

            let rb_clone = Arc::clone(&replay_buffer);
            let rm_clone = Arc::clone(&record_muxer);
            let trk = *track;

            appsink.set_callbacks(
                gstreamer_app::AppSinkCallbacks::builder()
                    .new_sample(move |sink| {
                        if let Ok(sample) = sink.pull_sample() {
                            if let (Some(buffer), Some(caps)) = (sample.buffer(), sample.caps()) {
                                let pkt = EncodedPacket {
                                    track: trk,
                                    buffer: buffer.to_owned(),
                                    caps: caps.to_owned(),
                                };
                                if let Some(ref mut rb) = *rb_clone.lock().unwrap() {
                                    rb.push(pkt.clone());
                                }
                                if let Some(ref rm) = *rm_clone.lock().unwrap() {
                                    rm.push(&pkt);
                                }
                            }
                        }
                        Ok(gst::FlowSuccess::Ok)
                    })
                    .build(),
            );
        }

        pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| RpcError::new(ErrorCode::PipelineFailed as i32, format!("{e:?}")))?;

        let bus_err_clone = Arc::clone(&last_error);
        let bus_rm_clone = Arc::clone(&record_muxer);
        let bus_rb_clone = Arc::clone(&replay_buffer);
        let bus = pipeline.bus().ok_or(ErrorCode::PipelineFailed)?;

        let bus_task = tokio::spawn(async move {
            let bus = bus;
            tokio::task::spawn_blocking(move || {
                while let Some(msg) = bus.timed_pop_filtered(
                    gst::ClockTime::from_mseconds(500),
                    &[gst::MessageType::Error, gst::MessageType::Eos],
                ) {
                    if let gst::MessageView::Error(err) = msg.view() {
                        let err_msg = err.error().to_string();
                        error!("Erro fatal no barramento do pipeline produtor: {err_msg}");
                        *bus_err_clone.lock().unwrap() = Some(err_msg.clone());
                        let rm = bus_rm_clone.lock().unwrap().take();
                        if let Some(muxer) = rm {
                            tokio::spawn(async move {
                                let _ = muxer.stop().await;
                            });
                        }
                        *bus_rb_clone.lock().unwrap() = None;
                        tokio::spawn(async move {
                            notify_error("Erro de captura", &err_msg).await;
                        });
                        break;
                    }
                }
            })
            .await
        });

        // Configuração de hotkey via gerenciador global de atalhos
        let (hotkey_bound, hotkey_trigger) = if params.hotkey {
            let cfg = self.hotkey_config.lock().await;
            (cfg.replay_bound, Some(cfg.replay_trigger.clone()))
        } else {
            (false, None)
        };

        *lock = Some(CaptureSession {
            pipeline,
            _portal_session: portal_session,
            audio_router,
            encoder_name: enc.element.clone(),
            tracks,
            replay_buffer,
            record_muxer,
            hotkey_handle: None,
            hotkey_bound,
            hotkey_trigger: hotkey_trigger.clone(),
            replay_duration_secs: params.duration_secs,
            replay_max_ram_mb: params.max_ram_mb,
            last_error,
            bus_abort: bus_task.abort_handle(),
        });

        Ok(ReplayStartResult {
            encoder: enc.element,
            duration_secs: params.duration_secs,
            max_ram_mb: params.max_ram_mb,
            shared_session: false,
            hotkey_bound,
            hotkey_trigger,
        })
    }

    pub async fn replay_save(
        &self,
        params: ReplaySaveParams,
    ) -> Result<ReplaySaveResult, RpcError> {
        params.validate()?;
        let _guard = self.save_mutex.lock().await;

        let (snapshot, game_name_auto) = {
            let lock = self.session.lock().await;
            let session = lock.as_ref().ok_or(ErrorCode::NotReplaying)?;
            let mut rb_lock = session.replay_buffer.lock().unwrap();
            let rb = rb_lock.as_mut().ok_or(ErrorCode::NotReplaying)?;

            let window = params.last_secs.map(|s| Duration::from_secs(s as u64));
            let snap = rb.snapshot(window).map_err(RpcError::from)?;
            (snap, None::<String>)
        };

        let dump_graph = if params.game_name.is_none() {
            tokio::time::timeout(
                Duration::from_millis(200),
                tokio::process::Command::new("pw-dump").output(),
            )
            .await
            .ok()
            .and_then(|res| res.ok())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| AudioGraph::from_json(&s).ok())
        } else {
            None
        };

        let clip_path = resolve_clip_path(
            params.output_dir.as_deref(),
            params.game_name.as_deref().or(game_name_auto.as_deref()),
            params.format,
            dump_graph.as_ref(),
            std::process::id(),
            snapshot.total_bytes,
        )?;

        let end_gop_seq = snapshot.end_gop_seq;
        let snap_duration_ms = snapshot
            .end_pts
            .saturating_sub(snapshot.start_pts)
            .mseconds();

        let t0 = Instant::now();
        let path_clone = clip_path.clone();
        let format = params.format;

        let bytes_written =
            tokio::task::spawn_blocking(move || write_clip_offline(snapshot, format, &path_clone))
                .await
                .map_err(|_| RpcError::from(ErrorCode::Internal))??;

        let elapsed_ms = t0.elapsed().as_millis() as u64;

        if params.clear_after_save {
            let lock = self.session.lock().await;
            if let Some(ref session) = *lock {
                if let Some(ref mut rb) = *session.replay_buffer.lock().unwrap() {
                    rb.clear_through(end_gop_seq);
                }
            }
        }

        let notif_path = clip_path.clone();
        tokio::spawn(async move {
            notify_clip_saved(&notif_path).await;
        });

        Ok(ReplaySaveResult {
            path: clip_path,
            duration_ms: snap_duration_ms,
            bytes_written,
            elapsed_ms,
        })
    }

    pub async fn replay_status(&self) -> ReplayStatusResult {
        let lock = self.session.lock().await;
        let cfg = self.hotkey_config.lock().await;
        if let Some(ref session) = *lock {
            if let Some(ref rb) = *session.replay_buffer.lock().unwrap() {
                let (buffered_ms, ram_bytes) = rb.status();
                return ReplayStatusResult {
                    active: true,
                    buffered_ms,
                    ram_bytes,
                    duration_secs: session.replay_duration_secs,
                    max_ram_mb: session.replay_max_ram_mb,
                    encoder: Some(session.encoder_name.clone()),
                    hotkey_bound: cfg.replay_bound,
                    last_error: session.last_error.lock().unwrap().clone(),
                };
            }
        }

        ReplayStatusResult {
            active: false,
            buffered_ms: 0,
            ram_bytes: 0,
            duration_secs: 0,
            max_ram_mb: 0,
            encoder: None,
            hotkey_bound: cfg.replay_bound,
            last_error: None,
        }
    }

    pub async fn replay_stop(&self) -> Result<ReplayStopResult, RpcError> {
        let mut lock = self.session.lock().await;
        let session = lock.as_mut().ok_or(ErrorCode::NotReplaying)?;

        let mut rb_opt = session.replay_buffer.lock().unwrap().take();
        let rb = rb_opt.take().ok_or(ErrorCode::NotReplaying)?;
        let (buffered_ms, _) = rb.status();

        if let Some(h) = session.hotkey_handle.take() {
            h.close();
            session.hotkey_bound = false;
        }

        let rec_active = session.record_muxer.lock().unwrap().is_some();
        if !rec_active {
            if let Some(mut s) = lock.take() {
                s.bus_abort.abort();
                if let Some(mut router) = s.audio_router.take() {
                    router.stop().await;
                }
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = s.pipeline.set_state(gst::State::Null);
                })
                .await;
            }
        }

        Ok(ReplayStopResult {
            discarded_ms: buffered_ms,
        })
    }

    pub async fn shutdown(&self) -> Option<RecordStopResult> {
        if let Some(h) = self.global_hotkey.lock().await.take() {
            h.close();
        }
        let mut lock = self.session.lock().await;
        if let Some(mut s) = lock.take() {
            s.bus_abort.abort();
            let mut stop_res = None;
            let maybe_muxer = s.record_muxer.lock().unwrap().take();
            if let Some(muxer) = maybe_muxer {
                stop_res = muxer.stop().await.ok();
            }
            if let Some(h) = s.hotkey_handle.take() {
                h.close();
            }
            if let Some(mut router) = s.audio_router.take() {
                router.stop().await;
            }
            let _ = tokio::task::spawn_blocking(move || {
                let _ = s.pipeline.set_state(gst::State::Null);
            })
            .await;
            return stop_res;
        }
        None
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
