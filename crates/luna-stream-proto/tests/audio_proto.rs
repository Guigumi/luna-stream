use luna_stream_proto::{AudioTrackKind, RecordStartParams, SystemAudioMode};

#[test]
fn audio_config_defaults() {
    let raw = r#"{"output":"/x.mkv"}"#;
    let params: RecordStartParams = serde_json::from_str(raw).expect("deserialize params");
    assert_eq!(params.audio.mode, SystemAudioMode::Isolated);
    assert!(params.audio.game);
    assert!(params.audio.call);
    assert!(params.audio.microphone);
    assert_eq!(params.audio.mic_device, None);
}

#[test]
fn audio_config_custom() {
    let raw = r#"{
        "output": "/x.mkv",
        "audio": {
            "mode": "desktop_loopback",
            "game": true,
            "call": false,
            "microphone": true,
            "mic_device": "Fifine Mic"
        }
    }"#;
    let params: RecordStartParams = serde_json::from_str(raw).expect("deserialize custom params");
    assert_eq!(params.audio.mode, SystemAudioMode::DesktopLoopback);
    assert!(params.audio.game);
    assert!(!params.audio.call);
    assert!(params.audio.microphone);
    assert_eq!(params.audio.mic_device.as_deref(), Some("Fifine Mic"));
}

#[test]
fn audio_track_kind_serde() {
    let kind = AudioTrackKind::Call;
    let serialized = serde_json::to_string(&kind).expect("serialize AudioTrackKind");
    assert_eq!(serialized, r#""call""#);

    let deserialized: AudioTrackKind =
        serde_json::from_str(r#""microphone""#).expect("deserialize AudioTrackKind");
    assert_eq!(deserialized, AudioTrackKind::Microphone);
}
