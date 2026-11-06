use gstreamer_pbutils::Discoverer;
use luna_stream_proto::{AudioConfig, Codec, EncoderInfo, EncoderKind};
use luna_streamd::pipeline::{AudioSourceKind, VideoSource};
use luna_streamd::recorder::Recorder;
use std::time::Duration;
use tempfile::NamedTempFile;

#[tokio::test]
async fn records_screen_and_three_audio_tracks() {
    let _ = gstreamer::init();

    let tmp = NamedTempFile::new().expect("temp file");
    let output_path = tmp.path().to_path_buf();
    let _ = tmp.close(); // permite que o recorder crie o arquivo

    let enc = EncoderInfo {
        kind: EncoderKind::Software,
        element: "openh264enc".to_string(),
        codec: Codec::H264,
    };

    let audio_cfg = AudioConfig::default();
    let mut recorder = Recorder::new();

    recorder
        .start(
            VideoSource::Test,
            &enc,
            30,
            &audio_cfg,
            AudioSourceKind::Test,
            output_path.clone(),
        )
        .await
        .expect("start failed");

    tokio::time::sleep(Duration::from_millis(1500)).await;

    let res = recorder.stop().await.expect("stop failed");
    assert_eq!(res.path, output_path);
    assert!(res.duration_ms >= 1000);

    let discoverer = Discoverer::new(gstreamer::ClockTime::from_seconds(5)).expect("discoverer");
    let uri = format!("file://{}", output_path.display());
    let info = discoverer.discover_uri(&uri).expect("discover uri");

    let video_streams = info.video_streams();
    assert_eq!(video_streams.len(), 1, "deve conter 1 stream de vídeo");

    let audio_streams = info.audio_streams();
    assert_eq!(
        audio_streams.len(),
        4,
        "deve conter 4 streams de áudio (mix, jogo, call, mic)"
    );

    let _ = std::fs::remove_file(output_path);
}

#[tokio::test]
async fn records_with_disabled_audio() {
    let _ = gstreamer::init();

    let tmp = NamedTempFile::new().expect("temp file");
    let output_path = tmp.path().to_path_buf();
    let _ = tmp.close();

    let enc = EncoderInfo {
        kind: EncoderKind::Software,
        element: "openh264enc".to_string(),
        codec: Codec::H264,
    };

    let audio_cfg = AudioConfig {
        game: false,
        call: false,
        microphone: false,
        ..Default::default()
    };
    let mut recorder = Recorder::new();

    recorder
        .start(
            VideoSource::Test,
            &enc,
            30,
            &audio_cfg,
            AudioSourceKind::Test,
            output_path.clone(),
        )
        .await
        .expect("start failed");

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let res = recorder.stop().await.expect("stop failed");
    assert_eq!(res.path, output_path);

    let discoverer = Discoverer::new(gstreamer::ClockTime::from_seconds(5)).expect("discoverer");
    let uri = format!("file://{}", output_path.display());
    let info = discoverer.discover_uri(&uri).expect("discover uri");

    assert_eq!(info.video_streams().len(), 1);
    assert_eq!(info.audio_streams().len(), 0);

    let _ = std::fs::remove_file(output_path);
}
