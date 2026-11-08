use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use luna_stream_ui::supervisor::{
    SupervisorCallbacks, SupervisorConfig, UiCommand, UiSnapshot, format_rpc_error_pt_br,
    run_supervisor,
};

#[tokio::test]
async fn supervisor_lifecycle_ipc_and_error_handling() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let tmp = tempfile::tempdir().unwrap();
        let sock_path = tmp.path().join("daemon.sock");
        let cfg_path = tmp.path().join("ui.json");
        let clips_dir = tmp.path().join("Clips");

        let listener = UnixListener::bind(&sock_path).unwrap();

        // Servidor Unix socket simulando respostas do luna-streamd
        let server_task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let (reader, mut writer) = stream.into_split();
                    let mut reader = BufReader::new(reader);
                    let mut line = String::new();

                    while let Ok(n) = reader.read_line(&mut line).await {
                        if n == 0 {
                            break;
                        }
                        let req: serde_json::Value = serde_json::from_str(&line).unwrap();
                        let id = req.get("id").cloned().unwrap_or(serde_json::json!(1));
                        let method = req["method"].as_str().unwrap_or("");

                        let resp = match method {
                            "daemon.ping" => {
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": { "version": "0.4.0", "protocol": 1 }
                                })
                            }
                            "encoder.list" => {
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "encoders": [{ "element": "vah264enc", "codec": "h264", "kind": "hardware" }],
                                        "selected": "vah264enc"
                                    }
                                })
                            }
                            "replay.status" => {
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "active": true,
                                        "buffered_ms": 20000,
                                        "ram_bytes": 100 * 1024 * 1024,
                                        "duration_secs": 30,
                                        "max_ram_mb": 512,
                                        "encoder": "vah264enc",
                                        "hotkey_bound": true,
                                        "last_error": null
                                    }
                                })
                            }
                            "record.status" => {
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "recording": false,
                                        "duration_ms": 0,
                                        "bytes_written": 0
                                    }
                                })
                            }
                            "replay.start" => {
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "encoder": "vah264enc",
                                        "duration_secs": 30,
                                        "max_ram_mb": 512,
                                        "shared_session": false,
                                        "hotkey_bound": true,
                                        "hotkey_trigger": "ALT+F10"
                                    }
                                })
                            }
                            "replay.save" => {
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "path": "/tmp/Clip_2026-11-07_20-00-00_Valorant.mp4",
                                        "duration_ms": 20000,
                                        "bytes_written": 5000000,
                                        "elapsed_ms": 250
                                    }
                                })
                            }
                            "replay.stop" => {
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": { "discarded_ms": 20000 }
                                })
                            }
                            _ => {
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": { "code": -32601, "message": "Method not found" }
                                })
                            }
                        };

                        let mut bytes = serde_json::to_vec(&resp).unwrap();
                        bytes.push(b'\n');
                        let _ = writer.write_all(&bytes).await;
                        line.clear();
                    }
                });
            }
        });

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let last_snapshot = Arc::new(Mutex::new(UiSnapshot::default()));
        let snap_ref = last_snapshot.clone();

        let quit_called = Arc::new(AtomicBool::new(false));
        let quit_ref = quit_called.clone();

        let callbacks = SupervisorCallbacks {
            on_snapshot: Box::new(move |snap| {
                let mut guard = snap_ref.lock().unwrap();
                *guard = snap;
            }),
            on_show_window: Box::new(|| {}),
            on_quit: Box::new(move || {
                quit_ref.store(true, Ordering::SeqCst);
            }),
        };

        let clips_dir_for_test = clips_dir.clone();
        let cfg = SupervisorConfig {
            socket_path: sock_path,
            config_path: cfg_path,
            clips_dir,
            daemon_bin_override: None,
            auto_spawn: false,
        };

        let sup_task = tokio::spawn(run_supervisor(cfg, cmd_rx, None, callbacks));

        // Aguarda conexão e primeiro tick de status
        tokio::time::sleep(Duration::from_millis(500)).await;

        {
            let snap = last_snapshot.lock().unwrap().clone();
            assert!(snap.daemon_connected);
            assert_eq!(snap.encoder_label, "vah264enc · GPU");
            assert!(snap.replay_active);
            assert_eq!(snap.replay_buffer_label, "20s / 30s");
        }

        // Testa envio de comando SaveClip
        cmd_tx.send(UiCommand::SaveClip).unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        {
            let snap = last_snapshot.lock().unwrap().clone();
            assert!(snap.last_saved_label.contains("Clip_2026-11-07_20-00-00_Valorant.mp4"));
        }

        // Simula clipe salvo externamente (ex: atalho ALT+F10 enquanto oculto na bandeja)
        cmd_tx.send(UiCommand::WindowVisibilityChanged(false)).unwrap();
        std::fs::create_dir_all(&clips_dir_for_test).unwrap();
        let external_clip = clips_dir_for_test.join("Clip_2026-11-07_20-10-00_Apex.mp4");
        std::fs::write(&external_clip, vec![0u8; 1024]).unwrap();

        // Ao abrir a janela via ShowWindow, a lista de clipes deve ser atualizada imediatamente
        cmd_tx.send(UiCommand::ShowWindow).unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        {
            let snap = last_snapshot.lock().unwrap().clone();
            assert!(
                snap.clips.iter().any(|c| c.game == "Apex"),
                "clipe gravado por ALT+F10 deve refletir na UI ao abrir a janela"
            );
        }

        // Testa encerramento
        cmd_tx.send(UiCommand::Quit).unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert!(quit_called.load(Ordering::SeqCst));

        server_task.abort();
        sup_task.abort();
    })
    .await
    .unwrap();
}

#[test]
fn format_rpc_error_pt_br_messages() {
    use luna_streamctl::client::ClientError;

    assert_eq!(
        format_rpc_error_pt_br(&ClientError::Rpc(1005, "permission".into())),
        "Compartilhamento de tela cancelado."
    );
    assert_eq!(
        format_rpc_error_pt_br(&ClientError::Rpc(1006, "no enc".into())),
        "Nenhum codificador H.264 compatível foi encontrado no GStreamer."
    );
    assert_eq!(
        format_rpc_error_pt_br(&ClientError::NotRunning),
        "Motor luna-streamd não encontrado. Verifique a instalação."
    );
}
