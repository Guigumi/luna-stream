use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use luna_stream_ui::gallery::MediaKind;
use luna_stream_ui::supervisor::{
    SupervisorCallbacks, SupervisorConfig, UiCommand, UiSnapshot, format_rpc_error_pt_br,
    run_supervisor,
};

#[tokio::test]
async fn supervisor_lifecycle_ipc_and_gallery_v2() {
    tokio::time::timeout(Duration::from_secs(12), async {
        let tmp = tempfile::tempdir().unwrap();
        let sock_path = tmp.path().join("daemon.sock");
        let cfg_path = tmp.path().join("ui.json");
        let recordings_dir = tmp.path().join("Luna");
        let clips_dir = recordings_dir.join("Clips");
        let cache_dir = tmp.path().join("cache");

        std::fs::create_dir_all(&clips_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let listener = UnixListener::bind(&sock_path).unwrap();

        // Servidor Unix socket simulando respostas v2 do daemon
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
                                    "result": { "version": "0.6.0", "protocol": 1 }
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
                            "system.capabilities" => {
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "total_ram_mb": 16384,
                                        "max_buffer_ram_mb": 8192,
                                        "encoder_kind": "hardware",
                                        "min_bitrate_kbps": 1000,
                                        "max_bitrate_kbps": 150000,
                                        "min_fps": 15,
                                        "max_fps": 240,
                                        "max_replay_secs": 1800
                                    }
                                })
                            }
                            "audio.devices" => {
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "inputs": [{ "id": "alsa_input.usb-Fifine", "label": "Fifine Mic" }],
                                        "outputs": [{ "id": "alsa_output.usb-HyperX", "label": "HyperX Cloud" }]
                                    }
                                })
                            }
                            "hotkeys.configure" => {
                                let p = &req["params"];
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "replay_bound": true,
                                        "replay_trigger": p["replay_trigger"].as_str().unwrap_or("ALT+F8"),
                                        "record_bound": true,
                                        "record_trigger": p["record_trigger"].as_str().unwrap_or("ALT+F9")
                                    }
                                })
                            }
                            "replay.status" => {
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "active": true,
                                        "buffered_ms": 15000,
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
                            "media.inspect" => {
                                let out = req["params"]["thumb_output"].as_str().unwrap_or("");
                                if !out.is_empty() {
                                    let p = std::path::Path::new(out);
                                    if let Some(parent) = p.parent() {
                                        let _ = std::fs::create_dir_all(parent);
                                    }
                                    let _ = std::fs::write(p, b"mock-png");
                                }
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "duration_ms": 25000,
                                        "thumb_written": true
                                    }
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

        // Cria previamente um arquivo de clipe
        let test_clip = clips_dir.join("Clip_2026-11-08_14-30-00_Valorant.mp4");
        std::fs::write(&test_clip, vec![0u8; 10000]).unwrap();

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
            on_overlay: Box::new(move |_event| {}),
        };

        let cfg = SupervisorConfig {
            socket_path: sock_path,
            config_path: cfg_path,
            clips_dir: clips_dir.clone(),
            daemon_bin_override: None,
            auto_spawn: false,
        };

        let sup_task = tokio::spawn(run_supervisor(cfg, cmd_rx, None, callbacks));

        // Aguarda conexão e consulta inicial de capabilities, devices e media.inspect
        tokio::time::sleep(Duration::from_millis(800)).await;

        {
            let snap = last_snapshot.lock().unwrap().clone();
            assert!(snap.daemon_connected);
            assert_eq!(snap.caps.max_bitrate_kbps, 150000);
            assert_eq!(snap.audio_inputs.len(), 1);
            assert_eq!(snap.audio_outputs.len(), 1);
            assert_eq!(snap.audio_inputs[0].label, "Fifine Mic");

            // Verifica galeria
            assert_eq!(snap.gallery_items.len(), 1);
            assert_eq!(snap.gallery_items[0].title, "Valorant");
            assert_eq!(snap.gallery_items[0].duration_ms, 25000);
            assert_eq!(snap.gallery_items[0].kind, MediaKind::Clip);
            assert!(snap.gallery_items[0].probed);
            assert_eq!(snap.recent_items.len(), 1);
        }

        // Teste de rejeição de atalho duplicado
        cmd_tx
            .send(UiCommand::CaptureHotkey {
                for_record: true,
                trigger: "ALT+F8".into(), // mesmo do replay -> conflito!
            })
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        {
            let snap = last_snapshot.lock().unwrap().clone();
            assert!(
                snap.error_banner.contains("já está em uso"),
                "deve alertar sobre atalho em uso: {}",
                snap.error_banner
            );
        }

        // Teste de captura válida de atalho e garantia de preservação no UpdatePreferences
        cmd_tx
            .send(UiCommand::CaptureHotkey {
                for_record: true,
                trigger: "CTRL+F12".into(),
            })
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        {
            let snap = last_snapshot.lock().unwrap().clone();
            assert_eq!(snap.prefs.record_hotkey, "CTRL+F12");
        }

        let mut prefs_update = last_snapshot.lock().unwrap().prefs.clone();
        prefs_update.bitrate_kbps = 28000;
        prefs_update.record_hotkey = "ALT+F9".into(); // simula default do read_preferences_from_window
        cmd_tx
            .send(UiCommand::UpdatePreferences(prefs_update))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        {
            let snap = last_snapshot.lock().unwrap().clone();
            assert_eq!(snap.prefs.bitrate_kbps, 28000);
            assert_eq!(
                snap.prefs.record_hotkey, "CTRL+F12",
                "UpdatePreferences deve preservar binds customizados"
            );
        }

        // Teste de exclusão em duas etapas
        // 1. Request
        cmd_tx
            .send(UiCommand::RequestDeleteMedia(test_clip.clone()))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        {
            let snap = last_snapshot.lock().unwrap().clone();
            assert_eq!(snap.confirming_delete_path, test_clip.to_string_lossy());
            assert!(test_clip.exists(), "arquivo ainda deve existir no 1º passo");
        }

        // 2. Confirm
        cmd_tx
            .send(UiCommand::ConfirmDeleteMedia(test_clip.clone()))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        {
            let snap = last_snapshot.lock().unwrap().clone();
            assert!(snap.confirming_delete_path.is_empty());
            assert!(!test_clip.exists(), "arquivo deve ter sido apagado após confirmação");
            assert_eq!(snap.gallery_items.len(), 0);
        }

        // Encerramento
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
