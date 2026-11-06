use serde_json::Value;
use std::path::Path;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use luna_stream_proto::{Request, Response};

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("luna-streamd não está rodando")]
    NotRunning,
    #[error("Erro RPC ({0}): {1}")]
    Rpc(i32, String),
    #[error("Erro de I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("Erro de JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub async fn call(socket_path: &Path, method: &str, params: Value) -> Result<Value, ClientError> {
    let stream = match UnixStream::connect(socket_path).await {
        Ok(s) => s,
        Err(_) => return Err(ClientError::NotRunning),
    };

    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let req = Request {
        jsonrpc: "2.0".to_string(),
        id: Some(1),
        method: method.to_string(),
        params,
    };

    let mut data = serde_json::to_vec(&req)?;
    data.push(b'\n');
    writer.write_all(&data).await?;

    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Err(ClientError::NotRunning);
    }

    let resp: Response = serde_json::from_str(&line)?;
    if let Some(err) = resp.error {
        return Err(ClientError::Rpc(err.code, err.message));
    }

    resp.result
        .ok_or_else(|| ClientError::Rpc(-32603, "resposta sem resultado".to_string()))
}
