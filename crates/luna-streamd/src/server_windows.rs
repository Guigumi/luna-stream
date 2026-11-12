//! Stub do servidor IPC para Windows.
//! No Windows, o IPC será via Named Pipe ou TCP (marco W1/W2).
//! Por enquanto, este stub aceita a estrutura mas não escuta conexões reais.

use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::handlers::Handler;
use luna_stream_proto::{ErrorCode, MAX_LINE_BYTES, Request, Response, RpcError};

pub struct WindowsListener;

impl WindowsListener {
    pub fn new() -> Result<Self, std::io::Error> {
        warn!("Servidor IPC Unix socket não disponível no Windows. Usando stub.");
        Ok(Self)
    }
}

pub async fn serve(
    _listener: WindowsListener,
    _handler: Arc<dyn Handler>,
    _shutdown: CancellationToken,
) -> Result<(), std::io::Error> {
    warn!("serve() não implementado no Windows ainda (marco W1/W2).");
    Ok(())
}

pub async fn handle_client(
    _stream: std::io::Empty,
    _handler: Arc<dyn Handler>,
    _shutdown: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "handle_client não implementado no Windows",
    )))
}

fn _assert_types() {
    let _ = MAX_LINE_BYTES;
    let _ = ErrorCode::Internal as i32;
    let _ = RpcError::new(ErrorCode::Internal as i32, "x".to_string());
    fn _check_request(_r: Request) {}
    fn _check_response(_r: Response) {}
}
