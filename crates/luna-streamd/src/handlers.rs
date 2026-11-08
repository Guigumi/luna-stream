use async_trait::async_trait;
use serde_json::Value;
use std::env;
use std::sync::Arc;

use crate::encoder;
use crate::media_probe::MediaProber;
use crate::session::SessionManager;
use crate::token_store::TokenStore;
use luna_stream_proto::{
    EncoderListResult, ErrorCode, HotkeysConfigureParams, MediaInspectParams, PROTOCOL_VERSION,
    PingResult, RecordStartParams, ReplaySaveParams, ReplayStartParams, RpcError, methods,
};

#[async_trait]
pub trait Handler: Send + Sync {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcError>;
}

pub struct Daemon {
    session_mgr: SessionManager,
    token_store: Arc<TokenStore>,
    media_prober: MediaProber,
}

impl Daemon {
    pub fn new() -> Self {
        let _ = gstreamer::init();
        let token_dir = TokenStore::default_dir();
        Self {
            session_mgr: SessionManager::new(),
            token_store: Arc::new(TokenStore::new(token_dir)),
            media_prober: MediaProber::new(),
        }
    }

    pub async fn shutdown(&self) -> Option<luna_stream_proto::RecordStopResult> {
        self.session_mgr.shutdown().await
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
            methods::SYSTEM_CAPABILITIES => {
                let caps = crate::capabilities::probe_system_capabilities();
                serde_json::to_value(caps)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            methods::AUDIO_DEVICES => {
                let devices = crate::audio::router::query_pipewire_devices();
                serde_json::to_value(devices)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            methods::RECORD_START => {
                let start_params: RecordStartParams = serde_json::from_value(params)
                    .map_err(|e| RpcError::new(ErrorCode::InvalidParams as i32, e.to_string()))?;
                let res = self
                    .session_mgr
                    .record_start(start_params, &self.token_store)
                    .await?;
                serde_json::to_value(res)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            methods::RECORD_STATUS => {
                let status = self.session_mgr.record_status().await;
                serde_json::to_value(status)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            methods::RECORD_STOP => {
                let res = self.session_mgr.record_stop().await?;
                serde_json::to_value(res)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            methods::REPLAY_START => {
                let start_params: ReplayStartParams = serde_json::from_value(params)
                    .map_err(|e| RpcError::new(ErrorCode::InvalidParams as i32, e.to_string()))?;
                let res = self
                    .session_mgr
                    .replay_start(start_params, &self.token_store)
                    .await?;
                serde_json::to_value(res)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            methods::REPLAY_SAVE => {
                let save_params: ReplaySaveParams = serde_json::from_value(params)
                    .map_err(|e| RpcError::new(ErrorCode::InvalidParams as i32, e.to_string()))?;
                let res = self.session_mgr.replay_save(save_params).await?;
                serde_json::to_value(res)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            methods::REPLAY_STATUS => {
                let status = self.session_mgr.replay_status().await;
                serde_json::to_value(status)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            methods::REPLAY_STOP => {
                let res = self.session_mgr.replay_stop().await?;
                serde_json::to_value(res)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            methods::HOTKEYS_CONFIGURE => {
                let configure_params: HotkeysConfigureParams = serde_json::from_value(params)
                    .map_err(|e| RpcError::new(ErrorCode::InvalidParams as i32, e.to_string()))?;
                let res = self
                    .session_mgr
                    .configure_hotkeys(configure_params, self.token_store.clone())
                    .await;
                serde_json::to_value(res)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            methods::MEDIA_INSPECT => {
                let inspect_params: MediaInspectParams = serde_json::from_value(params)
                    .map_err(|e| RpcError::new(ErrorCode::InvalidParams as i32, e.to_string()))?;
                let res = self.media_prober.inspect(inspect_params).await?;
                serde_json::to_value(res)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            _ => Err(ErrorCode::MethodNotFound.into()),
        }
    }
}
