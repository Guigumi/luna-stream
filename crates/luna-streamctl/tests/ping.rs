use luna_streamctl::client::{ClientError, call};
use luna_streamd::handlers::Daemon;
use luna_streamd::server::serve;
use luna_streamd::socket::bind;
use serde_json::Value;
use std::sync::Arc;
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn call_ping() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("luna-stream").join("daemon.sock");
    let listener = bind(&sock).unwrap();
    let handler = Arc::new(Daemon::new());
    let token = CancellationToken::new();
    let token_clone = token.clone();

    let server_task = tokio::spawn(async move {
        serve(listener, handler, token_clone).await.unwrap();
    });

    let res = call(&sock, "daemon.ping", Value::Null).await.unwrap();
    assert_eq!(res["protocol"], 1);
    assert_eq!(res["version"], env!("CARGO_PKG_VERSION"));

    token.cancel();
    let _ = server_task.await;
}

#[tokio::test]
async fn not_running() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("nonexistent.sock");

    let err = call(&sock, "daemon.ping", Value::Null).await.unwrap_err();
    assert!(matches!(err, ClientError::NotRunning));
}
