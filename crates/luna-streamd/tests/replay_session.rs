use gstreamer as gst;
use gstreamer_pbutils::Discoverer;
use luna_stream_proto::{
    ErrorCode, RecordStartResult, RecordStatusResult, RecordStopResult, ReplaySaveParams,
    ReplaySaveResult, ReplayStartParams, ReplayStartResult, ReplayStatusResult, ReplayStopResult,
    SourceKind, methods,
};
use luna_streamd::handlers::{Daemon, Handler};
use serde_json::json;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn replay_lifecycle_saves_valid_multitrack_clip_under_one_second() {
    tokio::time::timeout(Duration::from_secs(25), async {
        gst::init().unwrap();
        let dir = tempdir().unwrap();
        let daemon = Daemon::new();

        let start_params = ReplayStartParams {
            source: SourceKind::Test,
            hotkey: false,
            ..Default::default()
        };

        let start_val = daemon
            .handle(
                methods::REPLAY_START,
                serde_json::to_value(&start_params).unwrap(),
            )
            .await
            .expect("replay start");
        let start_res: ReplayStartResult = serde_json::from_value(start_val).unwrap();
        assert!(!start_res.shared_session);
        assert!(!start_res.hotkey_bound);

        tokio::time::sleep(Duration::from_millis(3200)).await;

        let save_params = ReplaySaveParams {
            output_dir: Some(dir.path().to_path_buf()),
            ..Default::default()
        };

        let save_val = daemon
            .handle(
                methods::REPLAY_SAVE,
                serde_json::to_value(&save_params).unwrap(),
            )
            .await
            .expect("replay save");
        let save_res: ReplaySaveResult = serde_json::from_value(save_val).unwrap();
        assert!(save_res.elapsed_ms < 1000, "elapsed_ms deve ser < 1000");
        assert!(save_res.path.exists());
        assert!(save_res.bytes_written > 0);

        let discoverer = Discoverer::new(gst::ClockTime::from_seconds(5)).unwrap();
        let uri = format!("file://{}", save_res.path.display());
        let info = discoverer.discover_uri(&uri).unwrap();
        assert_eq!(info.video_streams().len(), 1);
        assert_eq!(info.audio_streams().len(), 4);

        let stop_val = daemon
            .handle(methods::REPLAY_STOP, serde_json::Value::Null)
            .await
            .expect("replay stop");
        let stop_res: ReplayStopResult = serde_json::from_value(stop_val).unwrap();
        assert!(stop_res.discarded_ms > 0);

        daemon.shutdown().await;
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn shared_session_coexistence_between_replay_and_record() {
    tokio::time::timeout(Duration::from_secs(25), async {
        gst::init().unwrap();
        let dir = tempdir().unwrap();
        let daemon = Daemon::new();

        // 1. Inicia Replay
        let replay_start = daemon
            .handle(
                methods::REPLAY_START,
                json!({ "source": "test", "hotkey": false }),
            )
            .await
            .expect("replay start");
        let r_res: ReplayStartResult = serde_json::from_value(replay_start).unwrap();
        assert!(!r_res.shared_session);

        // 2. Inicia Gravação na sessão compartilhada
        let rec_path = dir.path().join("coexist.mp4");
        let record_start = daemon
            .handle(
                methods::RECORD_START,
                json!({ "source": "test", "output": rec_path }),
            )
            .await
            .expect("record start");
        let rec_res: RecordStartResult = serde_json::from_value(record_start).unwrap();
        assert!(
            rec_res.shared_session,
            "Gravação deve compartilhar a sessão do replay"
        );

        tokio::time::sleep(Duration::from_millis(2500)).await;

        // 3. Para a gravação; Replay continua ativo
        let rec_stop = daemon
            .handle(methods::RECORD_STOP, serde_json::Value::Null)
            .await
            .expect("record stop");
        let stop_res: RecordStopResult = serde_json::from_value(rec_stop).unwrap();
        assert!(stop_res.bytes_written > 0);

        let r_status_val = daemon
            .handle(methods::REPLAY_STATUS, serde_json::Value::Null)
            .await
            .expect("replay status");
        let r_status: ReplayStatusResult = serde_json::from_value(r_status_val).unwrap();
        assert!(r_status.active);
        assert!(r_status.buffered_ms > 0);

        // 4. Salva clipe pelo replay
        let save_val = daemon
            .handle(methods::REPLAY_SAVE, json!({ "output_dir": dir.path() }))
            .await
            .expect("replay save");
        let save_res: ReplaySaveResult = serde_json::from_value(save_val).unwrap();
        assert!(save_res.path.exists());

        // 5. Encerra replay
        let _ = daemon
            .handle(methods::REPLAY_STOP, serde_json::Value::Null)
            .await
            .expect("replay stop");

        // 6. Ordem inversa: Record primeiro, depois Replay
        let rec2_path = dir.path().join("rec_first.mp4");
        let start_rec2 = daemon
            .handle(
                methods::RECORD_START,
                json!({ "source": "test", "output": rec2_path }),
            )
            .await
            .expect("record start");
        let rec2_res: RecordStartResult = serde_json::from_value(start_rec2).unwrap();
        assert!(!rec2_res.shared_session);

        let start_rep2 = daemon
            .handle(
                methods::REPLAY_START,
                json!({ "source": "test", "hotkey": false }),
            )
            .await
            .expect("replay start");
        let rep2_res: ReplayStartResult = serde_json::from_value(start_rep2).unwrap();
        assert!(
            rep2_res.shared_session,
            "Replay deve compartilhar a sessão da gravação ativa"
        );

        // Encerra Replay, Record continua
        let _ = daemon
            .handle(methods::REPLAY_STOP, serde_json::Value::Null)
            .await
            .expect("replay stop");
        let rec_status_val = daemon
            .handle(methods::RECORD_STATUS, serde_json::Value::Null)
            .await
            .expect("record status");
        let rec_status: RecordStatusResult = serde_json::from_value(rec_status_val).unwrap();
        assert!(rec_status.recording);

        let _ = daemon
            .handle(methods::RECORD_STOP, serde_json::Value::Null)
            .await
            .expect("record stop");

        daemon.shutdown().await;
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn failed_save_preserves_replay_buffer_even_with_clear_after_save() {
    tokio::time::timeout(Duration::from_secs(20), async {
        gst::init().unwrap();
        let dir = tempdir().unwrap();
        let daemon = Daemon::new();

        daemon
            .handle(
                methods::REPLAY_START,
                json!({ "source": "test", "hotkey": false }),
            )
            .await
            .expect("replay start");

        tokio::time::sleep(Duration::from_millis(2500)).await;

        // Salvar com pasta inválida e clear_after_save=true deve falhar e PRESERVAR o buffer
        let bad_save = daemon
            .handle(
                methods::REPLAY_SAVE,
                json!({ "output_dir": "/proc/luna_non_writable_dir", "clear_after_save": true }),
            )
            .await;
        assert!(bad_save.is_err());
        assert_eq!(
            bad_save.unwrap_err().code,
            ErrorCode::OutputNotWritable as i32
        );

        let status_val = daemon
            .handle(methods::REPLAY_STATUS, serde_json::Value::Null)
            .await
            .unwrap();
        let status: ReplayStatusResult = serde_json::from_value(status_val).unwrap();
        assert!(
            status.buffered_ms >= 2000,
            "Buffer não deve ter sido limpo em caso de erro"
        );

        // Salvar com pasta válida e clear_after_save=true deve ter sucesso e limpar o buffer
        let ok_save = daemon
            .handle(
                methods::REPLAY_SAVE,
                json!({ "output_dir": dir.path(), "clear_after_save": true }),
            )
            .await
            .expect("replay save ok");
        let save_res: ReplaySaveResult = serde_json::from_value(ok_save).unwrap();
        assert!(save_res.path.exists());

        let after_status_val = daemon
            .handle(methods::REPLAY_STATUS, serde_json::Value::Null)
            .await
            .unwrap();
        let after_status: ReplayStatusResult = serde_json::from_value(after_status_val).unwrap();
        assert!(
            after_status.buffered_ms < status.buffered_ms,
            "Buffer deve ter sido reduzido após clear_after_save"
        );

        daemon.shutdown().await;
    })
    .await
    .unwrap();
}
