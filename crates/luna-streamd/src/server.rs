use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::handlers::Handler;
use luna_stream_proto::{ErrorCode, MAX_LINE_BYTES, Request, Response, RpcError};

pub async fn serve(
    listener: UnixListener,
    handler: Arc<dyn Handler>,
    shutdown: CancellationToken,
) -> Result<(), std::io::Error> {
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                info!("Encerrando loop de conexões do servidor...");
                break;
            }
            res = listener.accept() => {
                match res {
                    Ok((stream, _)) => {
                        let handler = Arc::clone(&handler);
                        let shutdown = shutdown.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_client(stream, handler, shutdown).await {
                                warn!("Cliente desconectado com erro: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        error!("Erro ao aceitar conexão UnixListener: {e}");
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_client(
    stream: UnixStream,
    handler: Arc<dyn Handler>,
    shutdown: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();

    loop {
        buf.clear();
        tokio::select! {
            _ = shutdown.cancelled() => break,
            res = reader.read_until(b'\n', &mut buf) => {
                let n = res?;
                if n == 0 {
                    break;
                }

                if n > MAX_LINE_BYTES {
                    let err = RpcError::from(ErrorCode::Parse);
                    let resp = Response::err(None, err);
                    let mut data = serde_json::to_vec(&resp)?;
                    data.push(b'\n');
                    let _ = writer.write_all(&data).await;
                    break;
                }

                let line_str = match std::str::from_utf8(&buf) {
                    Ok(s) => s.trim_end(),
                    Err(_) => {
                        let err = RpcError::from(ErrorCode::Parse);
                        let resp = Response::err(None, err);
                        let mut data = serde_json::to_vec(&resp)?;
                        data.push(b'\n');
                        writer.write_all(&data).await?;
                        continue;
                    }
                };

                if line_str.is_empty() {
                    continue;
                }

                let req: Request = match serde_json::from_str(line_str) {
                    Ok(r) => r,
                    Err(_) => {
                        let err = RpcError::from(ErrorCode::Parse);
                        let resp = Response::err(None, err);
                        let mut data = serde_json::to_vec(&resp)?;
                        data.push(b'\n');
                        writer.write_all(&data).await?;
                        continue;
                    }
                };

                let resp = match handler.handle(&req.method, req.params).await {
                    Ok(res_val) => Response::ok(req.id, res_val),
                    Err(rpc_err) => Response::err(req.id, rpc_err),
                };

                let mut data = serde_json::to_vec(&resp)?;
                data.push(b'\n');
                writer.write_all(&data).await?;
            }
        }
    }
    Ok(())
}
