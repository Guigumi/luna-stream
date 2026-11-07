use luna_stream_proto::{ContainerFormat, QualityPreset, RecordStartParams, RecordStatusResult};
use std::path::PathBuf;

#[test]
fn test_container_format_serde() {
    let mp4_json = serde_json::to_string(&ContainerFormat::Mp4).expect("serialize mp4");
    assert_eq!(mp4_json, "\"mp4\"");
    let parsed_mp4: ContainerFormat = serde_json::from_str("\"mp4\"").expect("deserialize mp4");
    assert_eq!(parsed_mp4, ContainerFormat::Mp4);

    let mkv_json = serde_json::to_string(&ContainerFormat::Mkv).expect("serialize mkv");
    assert_eq!(mkv_json, "\"mkv\"");
    let parsed_mkv: ContainerFormat = serde_json::from_str("\"mkv\"").expect("deserialize mkv");
    assert_eq!(parsed_mkv, ContainerFormat::Mkv);

    assert_eq!(ContainerFormat::default(), ContainerFormat::Mp4);
}

#[test]
fn test_quality_preset_serde() {
    let pairs = [
        (QualityPreset::Ultra, "\"ultra\""),
        (QualityPreset::High, "\"high\""),
        (QualityPreset::Medium, "\"medium\""),
        (QualityPreset::Low, "\"low\""),
    ];

    for (preset, json_str) in pairs {
        let serialized = serde_json::to_string(&preset).expect("serialize preset");
        assert_eq!(serialized, json_str);
        let deserialized: QualityPreset =
            serde_json::from_str(json_str).expect("deserialize preset");
        assert_eq!(deserialized, preset);
    }

    assert_eq!(QualityPreset::default(), QualityPreset::High);
}

#[test]
fn test_record_start_params_defaults() {
    let raw = "{}";
    let params: RecordStartParams = serde_json::from_str(raw).expect("deserialize empty params");

    assert_eq!(params.output, None);
    assert_eq!(params.output_dir, None);
    assert_eq!(params.game_name, None);
    assert_eq!(params.format, ContainerFormat::Mp4);
    assert_eq!(params.video.quality, QualityPreset::High);
    assert_eq!(params.video.width, None);
    assert_eq!(params.video.height, None);
    assert_eq!(params.video.bitrate_kbps, None);
    assert!(params.audio.separate_tracks);
    assert_eq!(params.fps, 60);
}

#[test]
fn test_record_status_result_serde() {
    let status = RecordStatusResult {
        recording: true,
        path: Some(PathBuf::from("/videos/Luna/2026-11-06_game.mp4")),
        duration_ms: 5432,
        bytes_written: 10_485_760,
        encoder: Some("vah264enc".to_string()),
        format: Some(ContainerFormat::Mp4),
        last_error: None,
    };

    let serialized = serde_json::to_string(&status).expect("serialize status");
    let deserialized: RecordStatusResult =
        serde_json::from_str(&serialized).expect("deserialize status");

    assert_eq!(deserialized, status);
}
