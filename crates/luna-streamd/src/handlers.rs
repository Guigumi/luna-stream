use async_trait::async_trait;
use serde_json::Value;
use std::env;
use std::os::fd::AsRawFd;
use tokio::sync::Mutex;

use crate::encoder;
use crate::pipeline::VideoSource;
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
        let res = recorder.stop().ok();
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

                match start_params.source {
                    SourceKind::Test => {
                        let mut recorder = self.recorder.lock().await;
                        recorder.start(
                            VideoSource::Test,
                            &enc,
                            start_params.fps,
                            start_params.output,
                        )?;
                        let res = RecordStartResult {
                            encoder: enc.element,
                        };
                        serde_json::to_value(res)
                            .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
                    }
                    SourceKind::Screen => {
                        let mut recorder = self.recorder.lock().await;
                        // Valida output antes de abrir o portal
                        crate::recorder::validate_output(&start_params.output)?;

                        let session = portal::open_screen(&self.token_store).await?;
                        let raw_fd = session.fd.as_raw_fd();
                        let node_id = session.node_id;

                        recorder.start(
                            VideoSource::PipeWire {
                                fd: raw_fd,
                                node_id,
                            },
                            &enc,
                            start_params.fps,
                            start_params.output,
                        )?;

                        *self.portal_session.lock().await = Some(session);

                        let res = RecordStartResult {
                            encoder: enc.element,
                        };
                        serde_json::to_value(res)
                            .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
                    }
                }
            }
            methods::RECORD_STOP => {
                let mut recorder = self.recorder.lock().await;
                let res = recorder.stop()?;
                self.portal_session.lock().await.take();
                serde_json::to_value(res)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            _ => Err(ErrorCode::MethodNotFound.into()),
        }
    }
}
