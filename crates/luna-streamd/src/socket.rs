use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream as StdUnixStream;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::net::UnixListener;

#[derive(Debug, Error)]
pub enum SocketError {
    #[error("XDG_RUNTIME_DIR não está definido")]
    NoRuntimeDir,
    #[error("Outra instância do luna-streamd já está em execução")]
    AlreadyRunning,
    #[error("Erro de I/O no socket: {0}")]
    Io(#[from] std::io::Error),
}

pub fn socket_path(runtime_dir: Option<&OsStr>) -> Result<PathBuf, SocketError> {
    let base = runtime_dir.ok_or(SocketError::NoRuntimeDir)?;
    Ok(PathBuf::from(base).join("luna-stream").join("daemon.sock"))
}

pub fn bind(path: &Path) -> Result<UnixListener, SocketError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        let mut perms = fs::metadata(parent)?.permissions();
        perms.set_mode(0o700);
        fs::set_permissions(parent, perms)?;
    }

    if path.exists() {
        match StdUnixStream::connect(path) {
            Ok(_) => return Err(SocketError::AlreadyRunning),
            Err(_) => {
                let _ = fs::remove_file(path);
            }
        }
    }

    let std_listener = std::os::unix::net::UnixListener::bind(path)?;
    std_listener.set_nonblocking(true)?;
    let listener = UnixListener::from_std(std_listener)?;
    Ok(listener)
}
