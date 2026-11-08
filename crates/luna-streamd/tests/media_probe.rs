use luna_stream_proto::{AudioConfig, Codec, EncoderInfo, EncoderKind, MediaInspectParams};
use luna_streamd::media_probe::MediaProber;
use luna_streamd::pipeline::{AudioSourceKind, VideoSource};
use luna_streamd::recorder::Recorder;
use std::fs;
use std::time::Duration;
use tempfile::tempdir;

fn test_encoder() -> EncoderInfo {
    EncoderInfo {
        element: "openh264enc".into(),
        codec: Codec::H264,
        kind: EncoderKind::Software,
    }
}

#[tokio::test]
async fn inspects_valid_video_and_extracts_png_thumbnail() {
    gstreamer::init().expect("gst init");
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("sample.mp4");
    let thumb_path = dir.path().join("thumbs").join("sample.png");

    let mut recorder = Recorder::new();
    let enc = test_encoder();
    let audio = AudioConfig::default();
    recorder
        .start(
            VideoSource::Test,
            &enc,
            30,
            &audio,
            AudioSourceKind::Test,
            video_path.clone(),
        )
        .await
        .expect("start recorder");

    tokio::time::sleep(Duration::from_millis(1500)).await;
    let stop_res = recorder.stop().await.expect("stop recorder");
    assert!(stop_res.duration_ms >= 1000);

    let prober = MediaProber::new();
    let inspect_res = prober
        .inspect(MediaInspectParams {
            path: video_path.clone(),
            thumb_output: thumb_path.clone(),
        })
        .await
        .expect("inspect should succeed");

    assert!(
        inspect_res.duration_ms >= 1000,
        "duration was {}",
        inspect_res.duration_ms
    );
    assert!(inspect_res.thumb_written, "thumb_written should be true");
    assert!(thumb_path.exists(), "thumbnail file must exist");

    let thumb_bytes = fs::read(&thumb_path).expect("read thumb");
    assert!(thumb_bytes.len() > 8);
    assert_eq!(
        &thumb_bytes[0..8],
        b"\x89PNG\r\n\x1a\n",
        "must be valid PNG header"
    );
}

#[tokio::test]
async fn inspects_corrupted_file_without_panic_and_keeps_queue_functional() {
    gstreamer::init().expect("gst init");
    let dir = tempdir().unwrap();
    let corrupt_path = dir.path().join("corrupted.mp4");
    let corrupt_thumb = dir.path().join("corrupted.png");
    fs::write(&corrupt_path, b"nao-e-um-video-invalido").unwrap();

    let prober = MediaProber::new();

    let start = std::time::Instant::now();
    let inspect_corrupt = prober
        .inspect(MediaInspectParams {
            path: corrupt_path.clone(),
            thumb_output: corrupt_thumb.clone(),
        })
        .await
        .expect("inspect on corrupted file should return Ok with zeros");

    assert!(
        start.elapsed() < Duration::from_secs(3),
        "must not timeout beyond 3s"
    );
    assert_eq!(inspect_corrupt.duration_ms, 0);
    assert!(!inspect_corrupt.thumb_written);
    assert!(!corrupt_thumb.exists());

    // Fila subsequente funciona normalmente
    let non_existent = dir.path().join("does_not_exist.mp4");
    let res2 = prober
        .inspect(MediaInspectParams {
            path: non_existent,
            thumb_output: corrupt_thumb.clone(),
        })
        .await
        .expect("inspect non existent should return zeros");
    assert_eq!(res2.duration_ms, 0);
    assert!(!res2.thumb_written);
}
