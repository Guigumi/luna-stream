use gstreamer as gst;
use luna_stream_proto::{
    AudioConfig, Codec, ContainerFormat, EncoderInfo, EncoderKind, SystemAudioMode,
    VideoRecordConfig,
};
use luna_streamd::pipeline::{AudioSourceKind, VideoSource, describe_producer};
use luna_streamd::replay_buffer::TrackId;

fn sw_enc() -> EncoderInfo {
    EncoderInfo {
        element: "openh264enc".into(),
        codec: Codec::H264,
        kind: EncoderKind::Software,
    }
}

#[test]
fn describe_producer_builds_parsable_mp4_and_mkv_appsink_pipelines() {
    gst::init().unwrap();
    let audio = AudioConfig {
        mode: SystemAudioMode::Isolated,
        game: true,
        call: true,
        microphone: true,
        mic_device: None,
        output_device: None,
        separate_tracks: true,
    };

    let (desc_mp4, tracks_mp4) = describe_producer(
        &VideoSource::Test,
        &sw_enc(),
        60,
        &VideoRecordConfig::default(),
        &audio,
        AudioSourceKind::Test,
        ContainerFormat::Mp4,
    );
    assert_eq!(
        tracks_mp4,
        vec![
            TrackId::Video,
            TrackId::AudioMix,
            TrackId::AudioGame,
            TrackId::AudioCall,
            TrackId::AudioMic
        ]
    );
    assert!(desc_mp4.contains("h264parse config-interval=-1 ! video/x-h264,stream-format=avc,alignment=au ! appsink name=vsink"));
    assert!(
        desc_mp4.contains("audio/mpeg,mpegversion=4,stream-format=raw ! appsink name=asink_mix")
    );
    assert!(!desc_mp4.contains("mp4mux"));
    gst::parse::launch(&desc_mp4).expect("pipeline produtor MP4 deve compilar no GStreamer");

    let (desc_mkv, tracks_mkv) = describe_producer(
        &VideoSource::Test,
        &sw_enc(),
        30,
        &VideoRecordConfig::default(),
        &AudioConfig {
            separate_tracks: false,
            ..audio
        },
        AudioSourceKind::Test,
        ContainerFormat::Mkv,
    );
    assert_eq!(tracks_mkv, vec![TrackId::Video, TrackId::AudioMix]);
    assert!(desc_mkv.contains("opusparse ! audio/x-opus ! appsink name=asink_mix"));
    gst::parse::launch(&desc_mkv).expect("pipeline produtor MKV deve compilar no GStreamer");
}
