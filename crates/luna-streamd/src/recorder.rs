use gstreamer as gst;
use gstreamer::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{error, info};

use crate::pipeline::{VideoSource, describe};
use luna_stream_proto::{EncoderInfo, ErrorCode, RecordStopResult, RpcError};

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
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            pipeline: None,
            start_time: None,
            output_path: None,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.pipeline.is_some()
    }

    pub fn start(
        &mut self,
        src: VideoSource,
        enc: &EncoderInfo,
        fps: u32,
        output: PathBuf,
    ) -> Result<(), RpcError> {
        if self.is_recording() {
            return Err(ErrorCode::AlreadyRecording.into());
        }

        validate_output(&output)?;

        let desc = describe(&src, enc, fps, &output);
        info!("Iniciando pipeline de gravação: {desc}");

        let element = gst::parse::launch(&desc).map_err(|e| {
            error!("Falha ao compilar pipeline GStreamer: {e}");
            RpcError::with_data(
                ErrorCode::PipelineFailed as i32,
                ErrorCode::PipelineFailed.message(),
                serde_json::json!({ "error": e.to_string(), "pipeline": desc }),
            )
        })?;

        let pipeline = element
            .dynamic_cast::<gst::Pipeline>()
            .map_err(|_| RpcError::from(ErrorCode::PipelineFailed))?;

        pipeline.set_state(gst::State::Playing).map_err(|e| {
            error!("Falha ao iniciar reprodução do pipeline: {:?}", e);
            RpcError::with_data(
                ErrorCode::PipelineFailed as i32,
                ErrorCode::PipelineFailed.message(),
                serde_json::json!({ "error": format!("{e:?}") }),
            )
        })?;

        self.pipeline = Some(pipeline);
        self.start_time = Some(Instant::now());
        self.output_path = Some(output);

        Ok(())
    }

    pub fn stop(&mut self) -> Result<RecordStopResult, RpcError> {
        let pipeline = self.pipeline.take().ok_or(ErrorCode::NotRecording)?;
        let start_time = self.start_time.take().ok_or(ErrorCode::NotRecording)?;
        let output = self.output_path.take().ok_or(ErrorCode::NotRecording)?;

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
        Ok(RecordStopResult {
            path: output,
            duration_ms,
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
