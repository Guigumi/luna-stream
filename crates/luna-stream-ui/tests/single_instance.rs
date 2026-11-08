use luna_stream_ui::single_instance::{InstanceRole, acquire_or_notify};
use std::fs;
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::test]
async fn primary_receives_show_from_secondary_and_recovers_stale_socket() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("luna-stream").join("ui.sock");

        // Simula arquivo órfão deixado por crash anterior
        fs::create_dir_all(sock.parent().unwrap()).unwrap();
        let stale = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        drop(stale);
        assert!(sock.exists());

        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let role1 = acquire_or_notify(sock.clone(), tx1).await.unwrap();
        let guard = match role1 {
            InstanceRole::Primary(g) => g,
            InstanceRole::SecondaryNotified => panic!("esperava Primary sobre socket órfão"),
        };

        let (tx2, _rx2) = mpsc::unbounded_channel();
        let role2 = acquire_or_notify(sock.clone(), tx2).await.unwrap();
        assert!(matches!(role2, InstanceRole::SecondaryNotified));

        rx1.recv()
            .await
            .expect("instância primária deve receber sinal show");
        drop(guard);
        assert!(!sock.exists(), "Drop do guard deve limpar ui.sock");
    })
    .await
    .unwrap();
}
