use async_trait::async_trait;
use serde_json::Value;
use std::env;
use std::os::fd::AsRawFd;
use tokio::sync::Mutex;

use crate::encoder;
use crate::pipeline::{AudioSourceKind, VideoSource};
use crate::portal::{self, PortalSession};
use crate::recorder::Recorder;
use crate::token_store::TokenStore;
use luna_stream_proto::{
    EncoderListResult, ErrorCode, PROTOCOL_VERSION, PingResult, RecordStartParams,
    RecordStartResult, RpcError, SourceKind, methods,
};

#[async_trait]
pub trait Handler: Send + Sync {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcError>;
}

pub struct Daemon {
    recorder: Mutex<Recorder>,
    portal_session: Mutex<Option<PortalSession>>,
    token_store: TokenStore,
}

impl Daemon {
    pub fn new() -> Self {
        let _ = gstreamer::init();
        let token_dir = TokenStore::default_dir();
        Self {
            recorder: Mutex::new(Recorder::new()),
            portal_session: Mutex::new(None),
            token_store: TokenStore::new(token_dir),
        }
    }

    pub async fn shutdown(&self) -> Option<luna_stream_proto::RecordStopResult> {
        let mut recorder = self.recorder.lock().await;
        let res = recorder.stop().await.ok();
        self.portal_session.lock().await.take();
        res
    }
}

impl Default for Daemon {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Handler for Daemon {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            methods::PING => {
                let res = PingResult {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    protocol: PROTOCOL_VERSION,
                };
                serde_json::to_value(res)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            methods::ENCODER_LIST => {
                let probed = encoder::probe();
                let selected = if let Ok(override_enc) = env::var("LUNA_STREAM_ENCODER") {
                    if probed.iter().any(|e| e.element == override_enc) {
                        Some(override_enc)
                    } else {
                        probed.first().map(|e| e.element.clone())
                    }
                } else {
                    probed.first().map(|e| e.element.clone())
                };

                let res = EncoderListResult {
                    encoders: probed,
                    selected,
                };
                serde_json::to_value(res)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            methods::RECORD_START => {
                let start_params: RecordStartParams = serde_json::from_value(params)
                    .map_err(|e| RpcError::new(ErrorCode::InvalidParams as i32, e.to_string()))?;

                let probed = encoder::probe();
                let enc = if let Ok(override_enc) = env::var("LUNA_STREAM_ENCODER") {
                    probed
                        .into_iter()
                        .find(|e| e.element == override_enc)
                        .ok_or(ErrorCode::NoEncoder)?
                } else {
                    probed.into_iter().next().ok_or(ErrorCode::NoEncoder)?
                };

                let dump_graph =
                    if start_params.output.is_none() && start_params.game_name.is_none() {
                        tokio::process::Command::new("pw-dump")
                            .output()
                            .await
                            .ok()
                            .and_then(|o| String::from_utf8(o.stdout).ok())
                            .and_then(|s| crate::audio::router::AudioGraph::from_json(&s).ok())
                    } else {
                        None
                    };

                let output_path = crate::naming::resolve_output_path(
                    start_params.output.as_deref(),
                    start_params.output_dir.as_deref(),
                    start_params.game_name.as_deref(),
                    start_params.format,
                    dump_graph.as_ref(),
                    std::process::id(),
                )?;

                match start_params.source {
                    SourceKind::Test => {
                        let mut recorder = self.recorder.lock().await;
                        recorder
                            .start_record(
                                VideoSource::Test,
                                &enc,
                                start_params.fps,
                                &start_params.video,
                                &start_params.audio,
                                AudioSourceKind::Test,
                                start_params.format,
                                output_path.clone(),
                            )
                            .await?;
                        let res = RecordStartResult {
                            encoder: enc.element,
                            path: output_path,
                            format: start_params.format,
                            shared_session: false,
                        };
                        serde_json::to_value(res)
                            .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
                    }
                    SourceKind::Screen => {
                        let mut recorder = self.recorder.lock().await;

                        let session = portal::open_screen(&self.token_store).await?;
                        let raw_fd = session.fd.as_raw_fd();
                        let node_id = session.node_id;

                        recorder
                            .start_record(
                                VideoSource::PipeWire {
                                    fd: raw_fd,
                                    node_id,
                                },
                                &enc,
                                start_params.fps,
                                &start_params.video,
                                &start_params.audio,
                                AudioSourceKind::PipeWire,
                                start_params.format,
                                output_path.clone(),
                            )
                            .await?;

                        *self.portal_session.lock().await = Some(session);

                        let res = RecordStartResult {
                            encoder: enc.element,
                            path: output_path,
                            format: start_params.format,
                            shared_session: false,
                        };
                        serde_json::to_value(res)
                            .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
                    }
                }
            }
            methods::RECORD_STATUS => {
                let recorder = self.recorder.lock().await;
                let status = recorder.status();
                serde_json::to_value(status)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            methods::RECORD_STOP => {
                let mut recorder = self.recorder.lock().await;
                let res = recorder.stop().await?;
                self.portal_session.lock().await.take();
                serde_json::to_value(res)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            _ => Err(ErrorCode::MethodNotFound.into()),
        }
    }
}
