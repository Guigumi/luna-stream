use gstreamer as gst;
use gstreamer::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tracing::{error, info};

use crate::audio::router::AudioRouter;
use crate::pipeline::{AudioSourceKind, VideoSource, describe_record};
use luna_stream_proto::{
    AudioConfig, ContainerFormat, EncoderInfo, ErrorCode, RecordStatusResult, RecordStopResult,
    RpcError, SystemAudioMode, VideoRecordConfig,
};

pub fn validate_output(path: &Path) -> Result<(), RpcError> {
    if path.exists() {
        return Err(ErrorCode::OutputExists.into());
    }

    let parent = path.parent().ok_or(ErrorCode::OutputNotWritable)?;
    if !parent.exists() {
        return Err(ErrorCode::OutputNotWritable.into());
    }

    // Testa permissão de escrita criando e removendo arquivo temporário
    let test_file = parent.join(format!(".luna_write_test_{}", std::process::id()));
    match fs::write(&test_file, b"") {
        Ok(_) => {
            let _ = fs::remove_file(test_file);
            Ok(())
        }
        Err(_) => Err(ErrorCode::OutputNotWritable.into()),
    }
}

pub struct Recorder {
    pipeline: Option<gst::Pipeline>,
    start_time: Option<Instant>,
    output_path: Option<PathBuf>,
    audio_router: Option<AudioRouter>,
    encoder: Option<String>,
    format: Option<ContainerFormat>,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            pipeline: None,
            start_time: None,
            output_path: None,
            audio_router: None,
            encoder: None,
            format: None,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.pipeline.is_some()
    }

    pub fn status(&self) -> RecordStatusResult {
        if !self.is_recording() {
            return RecordStatusResult {
                recording: false,
                path: None,
                duration_ms: 0,
                bytes_written: 0,
                encoder: None,
                format: None,
                last_error: None,
            };
        }

        let duration_ms = self
            .start_time
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);

        let bytes_written = self
            .output_path
            .as_ref()
            .and_then(|p| fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0);

        RecordStatusResult {
            recording: true,
            path: self.output_path.clone(),
            duration_ms,
            bytes_written,
            encoder: self.encoder.clone(),
            format: self.format,
            last_error: None,
        }
    }

    pub async fn start(
        &mut self,
        src: VideoSource,
        enc: &EncoderInfo,
        fps: u32,
        audio: &AudioConfig,
        audio_src: AudioSourceKind,
        output: PathBuf,
    ) -> Result<(), RpcError> {
        let video_cfg = VideoRecordConfig::default();
        self.start_record(
            src,
            enc,
            fps,
            &video_cfg,
            audio,
            audio_src,
            ContainerFormat::Mkv,
            output,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn start_record(
        &mut self,
        src: VideoSource,
        enc: &EncoderInfo,
        fps: u32,
        video: &VideoRecordConfig,
        audio: &AudioConfig,
        audio_src: AudioSourceKind,
        format: ContainerFormat,
        output: PathBuf,
    ) -> Result<(), RpcError> {
        if self.is_recording() {
            return Err(ErrorCode::AlreadyRecording.into());
        }

        validate_output(&output)?;

        // Inicializa AudioRouter antes de iniciar o pipeline (para instanciar os virtual sinks se necessário)
        let audio_router =
            if audio_src == AudioSourceKind::PipeWire && audio.mode == SystemAudioMode::Isolated {
                let game_target = if audio.game {
                    Some("luna_game".to_string())
                } else {
                    None
                };
                let call_target = if audio.call {
                    Some("luna_call".to_string())
                } else {
                    None
                };

                if game_target.is_some() || call_target.is_some() {
                    let mut router = AudioRouter::new(game_target, call_target);
                    let _ = router.sync().await;
                    router.start_periodic_sync(Duration::from_secs(2));
                    Some(router)
                } else {
                    None
                }
            } else {
                None
            };

        let desc = describe_record(&src, enc, fps, video, audio, audio_src, format, &output);
        info!("Iniciando pipeline de gravação: {desc}");

        let element = match gst::parse::launch(&desc) {
            Ok(elem) => elem,
            Err(e) => {
                error!("Falha ao compilar pipeline GStreamer: {e}");
                if let Some(mut r) = audio_router {
                    r.stop().await;
                }
                return Err(RpcError::with_data(
                    ErrorCode::PipelineFailed as i32,
                    ErrorCode::PipelineFailed.message(),
                    serde_json::json!({ "error": e.to_string(), "pipeline": desc }),
                ));
            }
        };

        let pipeline = match element.dynamic_cast::<gst::Pipeline>() {
            Ok(p) => p,
            Err(_) => {
                if let Some(mut r) = audio_router {
                    r.stop().await;
                }
                return Err(RpcError::from(ErrorCode::PipelineFailed));
            }
        };

        if let Err(e) = pipeline.set_state(gst::State::Playing) {
            error!("Falha ao iniciar reprodução do pipeline: {:?}", e);
            if let Some(mut r) = audio_router {
                r.stop().await;
            }
            return Err(RpcError::with_data(
                ErrorCode::PipelineFailed as i32,
                ErrorCode::PipelineFailed.message(),
                serde_json::json!({ "error": format!("{e:?}") }),
            ));
        }

        self.audio_router = audio_router;
        self.pipeline = Some(pipeline);
        self.start_time = Some(Instant::now());
        self.output_path = Some(output);
        self.encoder = Some(enc.element.clone());
        self.format = Some(format);

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<RecordStopResult, RpcError> {
        let pipeline = self.pipeline.take().ok_or(ErrorCode::NotRecording)?;
        let start_time = self.start_time.take().ok_or(ErrorCode::NotRecording)?;
        let output = self.output_path.take().ok_or(ErrorCode::NotRecording)?;
        self.encoder = None;
        self.format = None;

        // Encerra roteador de áudio e desfaz links
        if let Some(mut router) = self.audio_router.take() {
            router.stop().await;
        }

        info!("Finalizando gravação com envio de EOS...");
        pipeline.send_event(gst::event::Eos::new());

        if let Some(bus) = pipeline.bus() {
            let msg = bus.timed_pop_filtered(
                gst::ClockTime::from_seconds(5),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            );
            if let Some(msg) = msg {
                match msg.view() {
                    gst::MessageView::Error(err) => {
                        error!("Erro recebido no bus ao finalizar gravação: {err}");
                    }
                    gst::MessageView::Eos(_) => {
                        info!("EOS processado com sucesso pelo pipeline.");
                    }
                    _ => {}
                }
            } else {
                error!("Timeout aguardando EOS do pipeline.");
            }
        }

        let _ = pipeline.set_state(gst::State::Null);

        let duration_ms = start_time.elapsed().as_millis() as u64;
        let bytes_written = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
        Ok(RecordStopResult {
            path: output,
            duration_ms,
            bytes_written,
        })
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if let Some(pipeline) = self.pipeline.take() {
            let _ = pipeline.set_state(gst::State::Null);
        }
    }
}
