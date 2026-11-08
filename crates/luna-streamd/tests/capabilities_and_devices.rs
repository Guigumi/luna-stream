use luna_stream_proto::{
    AudioConfig, Codec, ContainerFormat, EncoderInfo, EncoderKind, SystemAudioMode,
    VideoRecordConfig,
};
use luna_streamd::audio::router::AudioGraph;
use luna_streamd::capabilities::parse_meminfo_total_mb;
use luna_streamd::pipeline::{AudioSourceKind, VideoSource, describe_record};
use std::path::PathBuf;

#[test]
fn parse_meminfo_and_compute_caps() {
    let sample = r#"
MemTotal:       16384000 kB
MemFree:         4123456 kB
MemAvailable:    8901234 kB
"#;
    assert_eq!(parse_meminfo_total_mb(sample), Some(16000));
}

#[test]
fn audio_graph_lists_real_sinks_and_sources_excluding_virtual_luna_nodes() {
    let json = r#"[
        {
            "id": 40,
            "type": "PipeWire:Interface:Node",
            "info": {
                "props": {
                    "node.name": "alsa_output.usb-HyperX",
                    "media.class": "Audio/Sink",
                    "node.description": "HyperX Cloud II"
                }
            }
        },
        {
            "id": 41,
            "type": "PipeWire:Interface:Node",
            "info": {
                "props": {
                    "node.name": "luna_game",
                    "media.class": "Audio/Sink",
                    "node.description": "Luna Game Sink"
                }
            }
        },
        {
            "id": 42,
            "type": "PipeWire:Interface:Node",
            "info": {
                "props": {
                    "node.name": "luna_call",
                    "media.class": "Audio/Sink",
                    "node.description": "Luna Call Sink"
                }
            }
        },
        {
            "id": 50,
            "type": "PipeWire:Interface:Node",
            "info": {
                "props": {
                    "node.name": "alsa_input.usb-Fifine",
                    "media.class": "Audio/Source",
                    "node.description": "Fifine K669"
                }
            }
        }
    ]"#;

    let graph = AudioGraph::from_json(json).unwrap();
    let devices = graph.list_audio_devices();

    assert_eq!(devices.outputs.len(), 1);
    assert_eq!(devices.outputs[0].id, "alsa_output.usb-HyperX");
    assert_eq!(devices.outputs[0].label, "HyperX Cloud II");

    assert_eq!(devices.inputs.len(), 1);
    assert_eq!(devices.inputs[0].id, "alsa_input.usb-Fifine");
    assert_eq!(devices.inputs[0].label, "Fifine K669");
}

#[test]
fn desktop_loopback_pipeline_targets_selected_output_monitor() {
    gstreamer::init().ok();
    let enc = EncoderInfo {
        element: "openh264enc".to_string(),
        codec: Codec::H264,
        kind: EncoderKind::Software,
    };
    let video_cfg = VideoRecordConfig::default();
    let audio_cfg = AudioConfig {
        mode: SystemAudioMode::DesktopLoopback,
        game: true,
        call: false,
        microphone: false,
        mic_device: None,
        output_device: Some("alsa_output.usb-HyperX".to_string()),
        separate_tracks: false,
    };

    let desc = describe_record(
        &VideoSource::Test,
        &enc,
        30,
        &video_cfg,
        &audio_cfg,
        AudioSourceKind::PipeWire,
        ContainerFormat::Mp4,
        &PathBuf::from("/tmp/test.mp4"),
    );

    assert!(
        desc.contains(r#"device="alsa_output.usb-HyperX.monitor""#),
        "pipeline deve monitorar o sink selecionado: {desc}"
    );
}
