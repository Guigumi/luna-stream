//! Controle de instância única via Unix socket ($XDG_RUNTIME_DIR/luna-stream/ui.sock).

use std::fs;
use std::io;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use crate::overlay::OverlayEvent;

/// Eventos recebidos via socket IPC da interface (`ui.sock`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiIncomingEvent {
    /// Solicitação para exibir e focar a janela principal.
    ShowWindow,
    /// Solicitação para exibir aviso visual na HUD overlay.
    Overlay(OverlayEvent),
}

pub struct SingleInstanceGuard {
    sock_path: PathBuf,
    abort_handle: AbortHandle,
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        self.abort_handle.abort();
        let _ = fs::remove_file(&self.sock_path);
    }
}

pub enum InstanceRole {
    Primary(SingleInstanceGuard),
    SecondaryNotified,
}

pub async fn acquire_or_notify(
    sock_path: PathBuf,
    incoming_tx: mpsc::UnboundedSender<UiIncomingEvent>,
) -> io::Result<InstanceRole> {
    if sock_path.exists() {
        if let Ok(mut stream) = UnixStream::connect(&sock_path).await {
            let _ = stream.write_all(b"show\n").await;
            let _ = stream.flush().await;
            return Ok(InstanceRole::SecondaryNotified);
        }
        // Conexão falhou: socket órfão de crash anterior
        let _ = fs::remove_file(&sock_path);
    }

    if let Some(parent) = sock_path.parent() {
        fs::create_dir_all(parent)?;
        let mut parent_perms = fs::metadata(parent)?.permissions();
        parent_perms.set_mode(0o700);
        fs::set_permissions(parent, parent_perms)?;
    }

    let std_listener = std::os::unix::net::UnixListener::bind(&sock_path)?;
    std_listener.set_nonblocking(true)?;
    let mut file_perms = fs::metadata(&sock_path)?.permissions();
    file_perms.set_mode(0o600);
    fs::set_permissions(&sock_path, file_perms)?;

    let listener = UnixListener::from_std(std_listener)?;
    let path_clone = sock_path.clone();

    let task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let tx = incoming_tx.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stream.take(4096));
                let mut line = String::new();
                while let Ok(n) = reader.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    let trimmed = line.trim();
                    if trimmed == "show" {
                        let _ = tx.send(UiIncomingEvent::ShowWindow);
                    } else if let Some(json_str) = trimmed.strip_prefix("overlay:") {
                        if let Ok(event) = serde_json::from_str::<OverlayEvent>(json_str) {
                            eprintln!("[OVERLAY_SOCKET] recebido: kind={:?} title={}", event.kind, event.title);
                            std::io::stderr().flush().ok();
                            let _ = tx.send(UiIncomingEvent::Overlay(event));
                        } else {
                            eprintln!("[OVERLAY_SOCKET] falha ao parsear: {}", json_str);
                            std::io::stderr().flush().ok();
                        }
                    }
                    line.clear();
                }
            });
        }
    });

    Ok(InstanceRole::Primary(SingleInstanceGuard {
        sock_path: path_clone,
        abort_handle: task.abort_handle(),
    }))
}
