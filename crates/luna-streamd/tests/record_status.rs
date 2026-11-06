use gstreamer as gst;
use luna_stream_proto::{
    AudioConfig, ContainerFormat, QualityPreset, RecordStartParams, RecordStartResult,
    RecordStatusResult, RecordStopResult, SourceKind, SystemAudioMode, VideoRecordConfig, methods,
};
use luna_streamd::handlers::{Daemon, Handler};
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_record_status_lifecycle() {
    gst::init().expect("gst init");
    let dir = tempdir().unwrap();
    let out = dir.path().join("status_test.mp4");

    let daemon = Daemon::new();

    // 1. Status inicial deve ser idle (não gravando)
    let idle_val = daemon
        .handle(methods::RECORD_STATUS, serde_json::Value::Null)
        .await
        .expect("handle status idle");
    let idle_status: RecordStatusResult = serde_json::from_value(idle_val).expect("parse status");
    assert!(!idle_status.recording);
    assert_eq!(idle_status.duration_ms, 0);
    assert_eq!(idle_status.bytes_written, 0);
    assert_eq!(idle_status.path, None);
    assert_eq!(idle_status.format, None);

    // 2. Inicia gravação com source de teste
    let params = RecordStartParams {
        output: Some(out.clone()),
        output_dir: None,
        game_name: None,
        format: ContainerFormat::Mp4,
        source: SourceKind::Test,
        fps: 30,
        video: VideoRecordConfig {
            width: None,
            height: None,
            quality: QualityPreset::High,
            bitrate_kbps: None,
        },
        audio: AudioConfig {
            mode: SystemAudioMode::Isolated,
            game: true,
            call: false,
            microphone: false,
            mic_device: None,
            separate_tracks: true,
        },
    };

    let start_val = daemon
        .handle(
            methods::RECORD_START,
            serde_json::to_value(&params).unwrap(),
        )
        .await
        .expect("handle start");
    let start_res: RecordStartResult =
        serde_json::from_value(start_val).expect("parse start result");
    assert_eq!(start_res.path, out);
    assert_eq!(start_res.format, ContainerFormat::Mp4);

    // Aguarda 1.2 segundos para progresso de gravação e escrita de blocos
    tokio::time::sleep(Duration::from_millis(1200)).await;

    // 3. Status durante gravação ativa
    let active_val = daemon
        .handle(methods::RECORD_STATUS, serde_json::Value::Null)
        .await
        .expect("handle status active");
    let active_status: RecordStatusResult =
        serde_json::from_value(active_val).expect("parse active status");
    assert!(active_status.recording);
    assert!(
        active_status.duration_ms >= 800,
        "duração deve ser >= 800ms, obtido: {}",
        active_status.duration_ms
    );
    assert!(
        active_status.bytes_written > 0,
        "bytes_written deve ser > 0 durante gravação ativa"
    );
    assert_eq!(active_status.path, Some(out.clone()));
    assert_eq!(active_status.format, Some(ContainerFormat::Mp4));

    // 4. Finaliza a gravação e valida bytes_written final
    let stop_val = daemon
        .handle(methods::RECORD_STOP, serde_json::Value::Null)
        .await
        .expect("handle stop");
    let stop_res: RecordStopResult = serde_json::from_value(stop_val).expect("parse stop result");
    assert_eq!(stop_res.path, out);
    assert!(stop_res.duration_ms >= 1000);
    assert!(stop_res.bytes_written >= active_status.bytes_written);

    // 5. Após stop, status volta a ser idle
    let post_stop_val = daemon
        .handle(methods::RECORD_STATUS, serde_json::Value::Null)
        .await
        .expect("handle post-stop status");
    let post_stop_status: RecordStatusResult =
        serde_json::from_value(post_stop_val).expect("parse post-stop status");
    assert!(!post_stop_status.recording);
}
