//! Stub do portal de captura para Windows.
//! O Windows usa Windows Graphics Capture (marco W2). Por enquanto, este stub
//! retorna erro "não implementado".

use luna_stream_proto::{ErrorCode, RpcError};
use std::path::Path;

pub struct PortalSession;

impl PortalSession {
    pub fn fd(&self) -> std::io::Result<std::fs::File> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "PipeWire FD não disponível no Windows",
        ))
    }
}

pub async fn open_screen(_store: &crate::token_store::TokenStore) -> Result<PortalSession, RpcError> {
    Err(RpcError::new(
        ErrorCode::Internal as i32,
        "Captura de tela via portal não implementada no Windows (marco W2)".to_string(),
    ))
}
