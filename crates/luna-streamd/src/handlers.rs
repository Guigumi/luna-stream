use async_trait::async_trait;
use luna_stream_proto::{ErrorCode, PROTOCOL_VERSION, PingResult, RpcError, methods};
use serde_json::Value;

#[async_trait]
pub trait Handler: Send + Sync {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcError>;
}

pub struct Daemon;

impl Daemon {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Daemon {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Handler for Daemon {
    async fn handle(&self, method: &str, _params: Value) -> Result<Value, RpcError> {
        match method {
            methods::PING => {
                let res = PingResult {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    protocol: PROTOCOL_VERSION,
                };
                serde_json::to_value(res)
                    .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))
            }
            _ => Err(ErrorCode::MethodNotFound.into()),
        }
    }
}
