use gstreamer_pbutils::Discoverer;
use gstreamer_pbutils::prelude::*;
use luna_stream_proto::{Codec, EncoderInfo, EncoderKind};
use luna_streamd::pipeline::VideoSource;
use luna_streamd::recorder::Recorder;
use std::path::PathBuf;
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
async fn records_two_seconds() {
    gstreamer::init().expect("gst init");
    let dir = tempdir().unwrap();
    let out = dir.path().join("rec.mkv");

    let mut recorder = Recorder::new();
    let enc = test_encoder();
    recorder
        .start(VideoSource::Test, &enc, 30, out.clone())
        .expect("start recorder");

    tokio::time::sleep(Duration::from_millis(2000)).await;

    let res = recorder.stop().expect("stop recorder");
    assert_eq!(res.path, out);
    assert!(
        res.duration_ms >= 1500 && res.duration_ms <= 4000,
        "duration_ms was {}",
        res.duration_ms
    );
    assert!(out.exists());

    let discoverer = Discoverer::new(gstreamer::ClockTime::from_seconds(5)).expect("discoverer");
    let uri = format!("file://{}", out.to_str().unwrap());
    let info = discoverer.discover_uri(&uri).expect("discover uri");
    let video_streams = info.video_streams();
    assert_eq!(video_streams.len(), 1);
    assert_eq!(
        video_streams[0]
            .caps()
            .unwrap()
            .structure(0)
            .unwrap()
            .name(),
        "video/x-h264"
    );
}

#[test]
fn refuses_existing_output() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("exists.mkv");
    std::fs::write(&out, b"content").unwrap();

    let mut recorder = Recorder::new();
    let enc = test_encoder();
    let err = recorder
        .start(VideoSource::Test, &enc, 30, out.clone())
        .unwrap_err();
    assert_eq!(err.code, 1003); // OutputExists
    assert_eq!(std::fs::read(&out).unwrap(), b"content");
}

#[test]
fn refuses_missing_dir() {
    let out = PathBuf::from("/nao/existe_mesmo/rec.mkv");
    let mut recorder = Recorder::new();
    let enc = test_encoder();
    let err = recorder
        .start(VideoSource::Test, &enc, 30, out)
        .unwrap_err();
    assert_eq!(err.code, 1004); // OutputNotWritable
}

#[test]
fn double_start() {
    gstreamer::init().expect("gst init");
    let dir = tempdir().unwrap();
    let out1 = dir.path().join("r1.mkv");
    let out2 = dir.path().join("r2.mkv");

    let mut recorder = Recorder::new();
    let enc = test_encoder();
    recorder
        .start(VideoSource::Test, &enc, 30, out1)
        .expect("first start");

    let err = recorder
        .start(VideoSource::Test, &enc, 30, out2)
        .unwrap_err();
    assert_eq!(err.code, 1001); // AlreadyRecording

    recorder.stop().expect("stop");

    let stop_err = recorder.stop().unwrap_err();
    assert_eq!(stop_err.code, 1002); // NotRecording
}
