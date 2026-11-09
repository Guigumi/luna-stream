use luna_stream_ui::overlay::{OverlayEvent, OverlayKind};
use luna_stream_ui::single_instance::{InstanceRole, UiIncomingEvent, acquire_or_notify};
use std::fs;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
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

        let event = rx1
            .recv()
            .await
            .expect("instância primária deve receber sinal show");
        assert_eq!(event, UiIncomingEvent::ShowWindow);
        drop(guard);
        assert!(!sock.exists(), "Drop do guard deve limpar ui.sock");
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_ui_socket_parses_show_and_overlay_events() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("luna-stream").join("ui.sock");

        let (tx, mut rx) = mpsc::unbounded_channel();
        let role = acquire_or_notify(sock.clone(), tx).await.unwrap();
        let _guard = match role {
            InstanceRole::Primary(g) => g,
            InstanceRole::SecondaryNotified => panic!("esperava Primary"),
        };

        // 1. Envia linha "show\n"
        {
            let mut stream = UnixStream::connect(&sock).await.unwrap();
            stream.write_all(b"show\n").await.unwrap();
            stream.flush().await.unwrap();
        }

        let ev1 = rx.recv().await.expect("deve receber ShowWindow");
        assert_eq!(ev1, UiIncomingEvent::ShowWindow);

        // 2. Envia linha "overlay:<json>\n" para ClipSaved
        {
            let mut stream = UnixStream::connect(&sock).await.unwrap();
            let line = "overlay:{\"kind\":\"ClipSaved\",\"title\":\"Clipe salvo\",\"subtitle\":\"clipe_01.mp4\"}\n";
            stream.write_all(line.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
        }

        let ev2 = rx.recv().await.expect("deve receber Overlay ClipSaved");
        assert_eq!(
            ev2,
            UiIncomingEvent::Overlay(OverlayEvent {
                kind: OverlayKind::ClipSaved,
                title: "Clipe salvo".to_string(),
                subtitle: "clipe_01.mp4".to_string(),
            })
        );

        // 3. Envia linha "overlay:<json>\n" para RecordStarted
        {
            let mut stream = UnixStream::connect(&sock).await.unwrap();
            let line = "overlay:{\"kind\":\"RecordStarted\",\"title\":\"Gravação iniciada\",\"subtitle\":\"Capturando vídeo e áudio\"}\n";
            stream.write_all(line.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
        }

        let ev3 = rx.recv().await.expect("deve receber Overlay RecordStarted");
        assert_eq!(
            ev3,
            UiIncomingEvent::Overlay(OverlayEvent {
                kind: OverlayKind::RecordStarted,
                title: "Gravação iniciada".to_string(),
                subtitle: "Capturando vídeo e áudio".to_string(),
            })
        );
    })
    .await
    .unwrap();
}
