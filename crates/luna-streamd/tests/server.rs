use luna_stream_proto::MAX_LINE_BYTES;
use luna_streamd::handlers::Daemon;
use luna_streamd::server::serve;
use luna_streamd::socket::{SocketError, bind, socket_path};
use serde_json::Value;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio_util::sync::CancellationToken;

#[test]
fn socket_path_requires_runtime_dir() {
    assert!(matches!(socket_path(None), Err(SocketError::NoRuntimeDir)));

    let path = socket_path(Some(OsStr::new("/run/user/1000"))).unwrap();
    assert_eq!(
        path.to_str().unwrap(),
        "/run/user/1000/luna-stream/daemon.sock"
    );
}

#[tokio::test]
async fn ping_roundtrip() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("luna-stream").join("daemon.sock");
    let listener = bind(&sock).unwrap();
    let handler = Arc::new(Daemon::new());
    let token = CancellationToken::new();
    let token_clone = token.clone();

    let server_task = tokio::spawn(async move {
        serve(listener, handler, token_clone).await.unwrap();
    });

    let stream = UnixStream::connect(&sock).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    writer
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"daemon.ping\"}\n")
        .await
        .unwrap();

    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    let res: Value = serde_json::from_str(&line).unwrap();

    assert_eq!(res["id"], 7);
    assert_eq!(res["result"]["protocol"], 1);

    token.cancel();
    let _ = server_task.await;
}

#[tokio::test]
async fn unknown_method() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("luna-stream").join("daemon.sock");
    let listener = bind(&sock).unwrap();
    let handler = Arc::new(Daemon::new());
    let token = CancellationToken::new();
    let token_clone = token.clone();

    let server_task = tokio::spawn(async move {
        serve(listener, handler, token_clone).await.unwrap();
    });

    let stream = UnixStream::connect(&sock).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    writer
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"nope\"}\n")
        .await
        .unwrap();

    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    let res: Value = serde_json::from_str(&line).unwrap();

    assert_eq!(res["id"], 1);
    assert_eq!(res["error"]["code"], -32601);

    token.cancel();
    let _ = server_task.await;
}

#[tokio::test]
async fn malformed_json_keeps_connection() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("luna-stream").join("daemon.sock");
    let listener = bind(&sock).unwrap();
    let handler = Arc::new(Daemon::new());
    let token = CancellationToken::new();
    let token_clone = token.clone();

    let server_task = tokio::spawn(async move {
        serve(listener, handler, token_clone).await.unwrap();
    });

    let stream = UnixStream::connect(&sock).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Envia JSON quebrado
    writer.write_all(b"{oops\n").await.unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    let res: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(res["error"]["code"], -32700);

    // Na mesma conexão, envia um ping válido
    writer
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"daemon.ping\"}\n")
        .await
        .unwrap();
    line.clear();
    reader.read_line(&mut line).await.unwrap();
    let res: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(res["id"], 2);
    assert_eq!(res["result"]["protocol"], 1);

    token.cancel();
    let _ = server_task.await;
}

#[tokio::test]
async fn oversized_line_closes_only_that_client() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("luna-stream").join("daemon.sock");
    let listener = bind(&sock).unwrap();
    let handler = Arc::new(Daemon::new());
    let token = CancellationToken::new();
    let token_clone = token.clone();

    let server_task = tokio::spawn(async move {
        serve(listener, handler, token_clone).await.unwrap();
    });

    // Cliente 1 envia linha maior que 64 KiB
    let stream1 = UnixStream::connect(&sock).await.unwrap();
    let (reader1, mut writer1) = stream1.into_split();
    let mut reader1 = BufReader::new(reader1);

    let huge = vec![b'a'; MAX_LINE_BYTES + 1000];
    writer1.write_all(&huge).await.unwrap();
    writer1.write_all(b"\n").await.unwrap();

    let mut line = String::new();
    let read_bytes = reader1.read_line(&mut line).await.unwrap();
    // A conexão deve ter sido fechada pelo servidor (0 bytes lidos ou resposta de erro seguida de EOF)
    assert!(read_bytes == 0 || line.contains("-32700"));

    // Cliente 2 conecta e envia ping com sucesso
    let stream2 = UnixStream::connect(&sock).await.unwrap();
    let (reader2, mut writer2) = stream2.into_split();
    let mut reader2 = BufReader::new(reader2);

    writer2
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"daemon.ping\"}\n")
        .await
        .unwrap();
    line.clear();
    reader2.read_line(&mut line).await.unwrap();
    let res: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(res["id"], 99);
    assert_eq!(res["result"]["protocol"], 1);

    token.cancel();
    let _ = server_task.await;
}

#[tokio::test]
async fn second_instance_rejected() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("luna-stream").join("daemon.sock");
    let _l1 = bind(&sock).unwrap();

    let err = bind(&sock).unwrap_err();
    assert!(matches!(err, SocketError::AlreadyRunning));
}

#[tokio::test]
async fn stale_socket_replaced() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("luna-stream").join("daemon.sock");
    std::fs::create_dir_all(sock.parent().unwrap()).unwrap();
    {
        let _stale = std::os::unix::net::UnixListener::bind(&sock).unwrap();
    } // drop fecha o listener mas deixa o arquivo no fs

    let l = bind(&sock);
    assert!(l.is_ok(), "stale socket should have been replaced");
}

#[tokio::test]
async fn parent_dir_is_private() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("luna-stream").join("daemon.sock");
    let _l = bind(&sock).unwrap();

    let metadata = std::fs::metadata(sock.parent().unwrap()).unwrap();
    let permissions = metadata.permissions().mode() & 0o777;
    assert_eq!(permissions, 0o700);
}
