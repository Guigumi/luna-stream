//! Stub do socket IPC para Windows.
//! No Windows, o socket será Named Pipe ou TCP (marco W1/W2).
//! Por enquanto, este stub retorna erro "não implementado".

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SocketError {
    #[error("Socket IPC não implementado no Windows ainda (marco W1/W2)")]
    NotImplemented,
    #[error("Erro de I/O no socket: {0}")]
    Io(#[from] std::io::Error),
}

pub fn socket_path(_runtime_dir: Option<&OsStr>) -> Result<PathBuf, SocketError> {
    Err(SocketError::NotImplemented)
}

pub fn bind(_path: &Path) -> Result<WindowsListener, SocketError> {
    Err(SocketError::NotImplemented)
}

pub struct WindowsListener;

impl WindowsListener {
    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Não implementado no Windows",
        ))
    }
}
