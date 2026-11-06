use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_pbutils::Discoverer;
use gstreamer_pbutils::prelude::*;
use luna_stream_proto::{AudioConfig, Codec, EncoderInfo, EncoderKind, SystemAudioMode};
use luna_streamd::pipeline::{AudioSourceKind, VideoSource, describe_with_audio};
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
async fn generates_three_audio_tracks_in_mkv() {
    gst::init().expect("gst init");
    let dir = tempdir().unwrap();
    let out = dir.path().join("multi_audio.mkv");

    let enc = test_encoder();
    let audio_cfg = AudioConfig {
        mode: SystemAudioMode::Isolated,
        game: true,
        call: true,
        microphone: true,
        mic_device: None,
        separate_tracks: true,
    };

    let desc = describe_with_audio(
        &VideoSource::Test,
        &enc,
        30,
        &audio_cfg,
        AudioSourceKind::Test,
        &out,
    );

    let element = gst::parse::launch(&desc).expect("parse pipeline");
    let pipeline = element.dynamic_cast::<gst::Pipeline>().unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();

    tokio::time::sleep(Duration::from_millis(2000)).await;

    pipeline.send_event(gst::event::Eos::new());
    if let Some(bus) = pipeline.bus() {
        let _ = bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(5),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        );
    }
    let _ = pipeline.set_state(gst::State::Null);

    assert!(out.exists());
    let discoverer = Discoverer::new(gst::ClockTime::from_seconds(5)).expect("discoverer");
    let uri = format!("file://{}", out.to_str().unwrap());
    let info = discoverer.discover_uri(&uri).expect("discover uri");

    let video_streams = info.video_streams();
    assert_eq!(video_streams.len(), 1);

    let audio_streams = info.audio_streams();
    assert_eq!(audio_streams.len(), 3, "expected 3 audio streams");
    for s in audio_streams {
        assert_eq!(
            s.caps().unwrap().structure(0).unwrap().name(),
            "audio/x-opus"
        );
    }
}

#[tokio::test]
async fn omits_disabled_tracks() {
    gst::init().expect("gst init");
    let dir = tempdir().unwrap();
    let out = dir.path().join("single_audio.mkv");

    let enc = test_encoder();
    let audio_cfg = AudioConfig {
        mode: SystemAudioMode::Isolated,
        game: true,
        call: false,
        microphone: false,
        mic_device: None,
        separate_tracks: true,
    };

    let desc = describe_with_audio(
        &VideoSource::Test,
        &enc,
        30,
        &audio_cfg,
        AudioSourceKind::Test,
        &out,
    );

    let element = gst::parse::launch(&desc).expect("parse pipeline");
    let pipeline = element.dynamic_cast::<gst::Pipeline>().unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();

    tokio::time::sleep(Duration::from_millis(2000)).await;

    pipeline.send_event(gst::event::Eos::new());
    if let Some(bus) = pipeline.bus() {
        let _ = bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(5),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        );
    }
    let _ = pipeline.set_state(gst::State::Null);

    assert!(out.exists());
    let discoverer = Discoverer::new(gst::ClockTime::from_seconds(5)).expect("discoverer");
    let uri = format!("file://{}", out.to_str().unwrap());
    let info = discoverer.discover_uri(&uri).expect("discover uri");

    let audio_streams = info.audio_streams();
    assert_eq!(audio_streams.len(), 1, "expected only 1 audio stream");
}
