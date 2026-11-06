use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_pbutils::Discoverer;
use gstreamer_pbutils::prelude::*;
use luna_stream_proto::{
    AudioConfig, Codec, ContainerFormat, EncoderInfo, EncoderKind, QualityPreset, SystemAudioMode,
    VideoRecordConfig,
};
use luna_streamd::pipeline::{AudioSourceKind, VideoSource, describe_record, select_aac_encoder};
use std::time::Duration;
use tempfile::tempdir;

fn test_encoder() -> EncoderInfo {
    EncoderInfo {
        element: "openh264enc".to_string(),
        codec: Codec::H264,
        kind: EncoderKind::Software,
    }
}

#[tokio::test]
async fn generates_fmp4_with_obs_audio_layout() {
    gst::init().expect("gst init");
    let dir = tempdir().unwrap();
    let out = dir.path().join("obs_fmp4.mp4");

    let enc = test_encoder();
    let video_cfg = VideoRecordConfig::default();
    let audio_cfg = AudioConfig {
        mode: SystemAudioMode::Isolated,
        game: true,
        call: true,
        microphone: true,
        mic_device: None,
        separate_tracks: true,
    };

    let desc = describe_record(
        &VideoSource::Test,
        &enc,
        30,
        &video_cfg,
        &audio_cfg,
        AudioSourceKind::Test,
        ContainerFormat::Mp4,
        &out,
    );

    assert!(desc.contains("mp4mux fragment-duration=1000"));
    assert!(desc.contains("audiomixer name=mix"));

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
    assert_eq!(
        audio_streams.len(),
        4,
        "deve conter 4 faixas: Trilha 1 Mix + 3 isoladas"
    );
    for s in audio_streams {
        assert_eq!(s.caps().unwrap().structure(0).unwrap().name(), "audio/mpeg");
    }
}

#[tokio::test]
async fn omits_separate_tracks_when_disabled() {
    gst::init().expect("gst init");
    let dir = tempdir().unwrap();
    let out = dir.path().join("single_mix.mp4");

    let enc = test_encoder();
    let video_cfg = VideoRecordConfig::default();
    let audio_cfg = AudioConfig {
        mode: SystemAudioMode::Isolated,
        game: true,
        call: true,
        microphone: true,
        mic_device: None,
        separate_tracks: false,
    };

    let desc = describe_record(
        &VideoSource::Test,
        &enc,
        30,
        &video_cfg,
        &audio_cfg,
        AudioSourceKind::Test,
        ContainerFormat::Mp4,
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

    let discoverer = Discoverer::new(gst::ClockTime::from_seconds(5)).expect("discoverer");
    let uri = format!("file://{}", out.to_str().unwrap());
    let info = discoverer.discover_uri(&uri).expect("discover uri");

    let audio_streams = info.audio_streams();
    assert_eq!(
        audio_streams.len(),
        1,
        "quando separate_tracks=false deve conter apenas o mix"
    );
}

#[tokio::test]
async fn generates_clusterized_mkv_with_opus() {
    gst::init().expect("gst init");
    let dir = tempdir().unwrap();
    let out = dir.path().join("cluster_mkv.mkv");

    let enc = test_encoder();
    let video_cfg = VideoRecordConfig::default();
    let audio_cfg = AudioConfig {
        mode: SystemAudioMode::Isolated,
        game: true,
        call: false,
        microphone: true,
        mic_device: None,
        separate_tracks: true,
    };

    let desc = describe_record(
        &VideoSource::Test,
        &enc,
        30,
        &video_cfg,
        &audio_cfg,
        AudioSourceKind::Test,
        ContainerFormat::Mkv,
        &out,
    );

    assert!(desc.contains("matroskamux min-cluster-duration=500000000 offset-to-zero=true"));
    assert!(desc.contains("opusenc"));

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

    let discoverer = Discoverer::new(gst::ClockTime::from_seconds(5)).expect("discoverer");
    let uri = format!("file://{}", out.to_str().unwrap());
    let info = discoverer.discover_uri(&uri).expect("discover uri");

    let audio_streams = info.audio_streams();
    assert_eq!(
        audio_streams.len(),
        3,
        "deve conter 3 faixas: Trilha 1 Mix + Game + Mic"
    );
    for s in audio_streams {
        assert_eq!(
            s.caps().unwrap().structure(0).unwrap().name(),
            "audio/x-opus"
        );
    }
}

#[test]
fn applies_custom_cqp_and_resolution_scale() {
    gst::init().expect("gst init");
    let out = std::path::Path::new("/tmp/test.mp4");
    let enc_vaapi = EncoderInfo {
        element: "vah264enc".to_string(),
        codec: Codec::H264,
        kind: EncoderKind::Hardware,
    };

    let video_cfg = VideoRecordConfig {
        width: Some(1920),
        height: Some(1080),
        quality: QualityPreset::Ultra,
        bitrate_kbps: None,
    };
    let audio_cfg = AudioConfig::default();

    let desc = describe_record(
        &VideoSource::Test,
        &enc_vaapi,
        60,
        &video_cfg,
        &audio_cfg,
        AudioSourceKind::Test,
        ContainerFormat::Mp4,
        out,
    );

    assert!(desc.contains("rate-control=cqp qpi=18"));
    assert!(desc.contains("width=1920,height=1080"));

    // Bitrate explícito
    let video_cfg_bitrate = VideoRecordConfig {
        width: None,
        height: None,
        quality: QualityPreset::High,
        bitrate_kbps: Some(12000),
    };
    let desc_bitrate = describe_record(
        &VideoSource::Test,
        &enc_vaapi,
        60,
        &video_cfg_bitrate,
        &audio_cfg,
        AudioSourceKind::Test,
        ContainerFormat::Mp4,
        out,
    );
    assert!(desc_bitrate.contains("bitrate=12000"));

    let aac = select_aac_encoder();
    assert!(aac == "fdkaacenc" || aac == "avenc_aac");
}
