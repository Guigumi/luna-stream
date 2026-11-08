use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSink;
use gstreamer_pbutils::Discoverer;
use luna_stream_proto::{
    AudioConfig, Codec, ContainerFormat, EncoderInfo, EncoderKind, SystemAudioMode,
    VideoRecordConfig,
};
use luna_streamd::muxer::{LiveRecordMuxer, write_clip_offline};
use luna_streamd::pipeline::{AudioSourceKind, VideoSource, describe_producer};
use luna_streamd::replay_buffer::{EncodedPacket, GopSegment, ReplayRingBuffer, TrackId};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn sw_enc() -> EncoderInfo {
    EncoderInfo {
        element: "openh264enc".into(),
        codec: Codec::H264,
        kind: EncoderKind::Software,
    }
}

#[tokio::test]
async fn test_write_clip_offline_under_one_second() {
    tokio::time::timeout(Duration::from_secs(20), async {
        gst::init().unwrap();
        let dir = tempdir().unwrap();
        let clip_path = dir.path().join("clip_fast.mp4");

        let audio = AudioConfig {
            mode: SystemAudioMode::Isolated,
            game: true,
            call: true,
            microphone: true,
            mic_device: None,
            output_device: None,
            separate_tracks: true,
        };

        let (desc, tracks) = describe_producer(
            &VideoSource::Test,
            &sw_enc(),
            30,
            &VideoRecordConfig::default(),
            &audio,
            AudioSourceKind::Test,
            ContainerFormat::Mp4,
        );

        let elem = gst::parse::launch(&desc).unwrap();
        let pipeline = elem.dynamic_cast::<gst::Pipeline>().unwrap();

        let ring_buffer = Arc::new(Mutex::new(ReplayRingBuffer::new(
            Duration::from_secs(10),
            50 * 1024 * 1024,
        )));

        for track in &tracks {
            let sink_elem = pipeline.by_name(track.sink_name()).unwrap();
            let appsink = sink_elem.dynamic_cast::<AppSink>().unwrap();
            let rb = Arc::clone(&ring_buffer);
            let trk = *track;

            appsink.set_callbacks(
                gstreamer_app::AppSinkCallbacks::builder()
                    .new_sample(move |sink| {
                        if let Ok(sample) = sink.pull_sample() {
                            if let (Some(buffer), Some(caps)) = (sample.buffer(), sample.caps()) {
                                let pkt = EncodedPacket {
                                    track: trk,
                                    buffer: buffer.to_owned(),
                                    caps: caps.to_owned(),
                                };
                                rb.lock().unwrap().push(pkt);
                            }
                        }
                        Ok(gst::FlowSuccess::Ok)
                    })
                    .build(),
            );
        }

        pipeline.set_state(gst::State::Playing).unwrap();
        tokio::time::sleep(Duration::from_millis(2500)).await;

        let snapshot = ring_buffer.lock().unwrap().snapshot(None).unwrap();

        let t0 = Instant::now();
        let bytes_written = tokio::task::spawn_blocking(move || {
            write_clip_offline(snapshot, ContainerFormat::Mp4, &clip_path)
        })
        .await
        .unwrap()
        .unwrap();

        let elapsed = t0.elapsed();
        assert!(
            elapsed < Duration::from_millis(1000),
            "Escrita de clipe demorou {elapsed:?}, deve ser < 1s"
        );
        assert!(bytes_written > 0);

        let _ = pipeline.set_state(gst::State::Null);

        let discoverer = Discoverer::new(gst::ClockTime::from_seconds(5)).unwrap();
        let uri = format!("file://{}", dir.path().join("clip_fast.mp4").display());
        let info = discoverer.discover_uri(&uri).unwrap();
        assert_eq!(info.video_streams().len(), 1);
        assert_eq!(info.audio_streams().len(), 4);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_write_clip_offline_with_empty_track_does_not_hang() {
    tokio::time::timeout(Duration::from_secs(10), async {
        gst::init().unwrap();
        let dir = tempdir().unwrap();
        let clip_path = dir.path().join("clip_partial.mp4");

        // Monta um snapshot artificial onde AudioCall está ausente
        let mut vbuf = gst::Buffer::with_size(1000).unwrap();
        {
            let b = vbuf.get_mut().unwrap();
            b.set_pts(gst::ClockTime::from_mseconds(0));
        }
        let vcaps = gst::Caps::builder("video/x-h264").build();
        let vpkt = EncodedPacket {
            track: TrackId::Video,
            buffer: vbuf,
            caps: vcaps,
        };

        let mut abuf = gst::Buffer::with_size(200).unwrap();
        {
            let b = abuf.get_mut().unwrap();
            b.set_pts(gst::ClockTime::from_mseconds(0));
        }
        let acaps = gst::Caps::builder("audio/mpeg")
            .field("mpegversion", 4i32)
            .field("stream-format", "raw")
            .build();
        let apkt = EncodedPacket {
            track: TrackId::AudioMix,
            buffer: abuf,
            caps: acaps,
        };

        let seg = GopSegment {
            seq: 1,
            start_pts: gst::ClockTime::from_mseconds(0),
            end_pts: gst::ClockTime::from_mseconds(1000),
            packets: vec![vpkt, apkt],
            bytes: 1200,
        };

        let snap = luna_streamd::replay_buffer::ClipSnapshot {
            segments: vec![seg],
            start_pts: gst::ClockTime::from_mseconds(0),
            end_pts: gst::ClockTime::from_mseconds(1000),
            total_bytes: 1200,
            end_gop_seq: 1,
        };

        let res = tokio::task::spawn_blocking(move || {
            write_clip_offline(snap, ContainerFormat::Mp4, &clip_path)
        })
        .await
        .unwrap();

        assert!(res.is_ok());
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_live_record_muxer_lifecycle() {
    tokio::time::timeout(Duration::from_secs(20), async {
        gst::init().unwrap();
        let dir = tempdir().unwrap();
        let out_path = dir.path().join("live_record.mp4");

        let audio = AudioConfig {
            mode: SystemAudioMode::Isolated,
            game: true,
            call: false,
            microphone: false,
            mic_device: None,
            output_device: None,
            separate_tracks: false,
        };

        let (desc, tracks) = describe_producer(
            &VideoSource::Test,
            &sw_enc(),
            30,
            &VideoRecordConfig::default(),
            &audio,
            AudioSourceKind::Test,
            ContainerFormat::Mp4,
        );

        let elem = gst::parse::launch(&desc).unwrap();
        let pipeline = elem.dynamic_cast::<gst::Pipeline>().unwrap();

        let muxer = Arc::new(
            LiveRecordMuxer::start(
                &tracks,
                ContainerFormat::Mp4,
                out_path.clone(),
                "openh264enc".into(),
            )
            .unwrap(),
        );

        for track in &tracks {
            let sink_elem = pipeline.by_name(track.sink_name()).unwrap();
            let appsink = sink_elem.dynamic_cast::<AppSink>().unwrap();
            let mux = Arc::clone(&muxer);
            let trk = *track;

            appsink.set_callbacks(
                gstreamer_app::AppSinkCallbacks::builder()
                    .new_sample(move |sink| {
                        if let Ok(sample) = sink.pull_sample() {
                            if let (Some(buffer), Some(caps)) = (sample.buffer(), sample.caps()) {
                                let pkt = EncodedPacket {
                                    track: trk,
                                    buffer: buffer.to_owned(),
                                    caps: caps.to_owned(),
                                };
                                mux.push(&pkt);
                            }
                        }
                        Ok(gst::FlowSuccess::Ok)
                    })
                    .build(),
            );
        }

        pipeline.set_state(gst::State::Playing).unwrap();
        tokio::time::sleep(Duration::from_millis(2200)).await;

        let status = muxer.status();
        assert!(status.0 >= 1000, "duration_ms deve ser >= 1000");

        let stop_res = muxer.stop().await.unwrap();

        assert_eq!(stop_res.path, out_path);
        assert!(stop_res.bytes_written > 0);

        let _ = pipeline.set_state(gst::State::Null);

        let discoverer = Discoverer::new(gst::ClockTime::from_seconds(5)).unwrap();
        let uri = format!("file://{}", out_path.display());
        let info = discoverer.discover_uri(&uri).unwrap();
        assert_eq!(info.video_streams().len(), 1);
        assert_eq!(info.audio_streams().len(), 1);
    })
    .await
    .unwrap();
}
