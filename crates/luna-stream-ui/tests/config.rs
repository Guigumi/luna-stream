use luna_stream_proto::{ContainerFormat, EncoderKind, SystemAudioMode, SystemCapabilitiesResult};
use luna_stream_ui::config::{AudioCaptureProfile, ResolutionPreset, UiPreferences};
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn defaults_roundtrip_and_atomic_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("luna-stream").join("ui.json");

    let mut prefs = UiPreferences::load_from(&cfg_path);
    assert_eq!(prefs.replay_duration_secs, 30);
    assert_eq!(prefs.bitrate_kbps, 15_000);
    assert_eq!(prefs.fps, 60);
    assert_eq!(prefs.resolution, ResolutionPreset::Native);
    assert_eq!(prefs.format, ContainerFormat::Mp4);
    assert_eq!(prefs.audio_profile, AudioCaptureProfile::SystemAndMic);
    assert_eq!(prefs.output_device, "default");
    assert_eq!(prefs.mic_device, "default");
    assert_eq!(prefs.replay_hotkey, "ALT+F8");
    assert_eq!(prefs.record_hotkey, "ALT+F9");

    prefs.replay_duration_secs = 45;
    prefs.bitrate_kbps = 25_000;
    prefs.resolution = ResolutionPreset::P1080;
    prefs.format = ContainerFormat::Mkv;
    prefs.save_to(&cfg_path).unwrap();

    let dir_mode = fs::metadata(cfg_path.parent().unwrap())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    let file_mode = fs::metadata(&cfg_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(dir_mode, 0o700);
    assert_eq!(file_mode, 0o600);

    let loaded = UiPreferences::load_from(&cfg_path);
    assert_eq!(loaded, prefs.sanitize());
    assert!(loaded.to_replay_start_params().validate().is_ok());
}

#[test]
fn sanitize_with_caps_clamps_bitrate_fps_and_duration_to_ram_budget() {
    let caps = SystemCapabilitiesResult {
        total_ram_mb: 16_384,
        max_buffer_ram_mb: 4_096,
        encoder_kind: Some(EncoderKind::Hardware),
        max_bitrate_kbps: 80_000,
        min_bitrate_kbps: 1_000,
        min_fps: 15,
        max_fps: 240,
        max_replay_secs: 600,
    };

    let prefs = UiPreferences {
        bitrate_kbps: 999_999,
        fps: 5,
        replay_duration_secs: 10_000,
        ..Default::default()
    };

    let sanitized = prefs.sanitize_with_caps(&caps);
    assert_eq!(sanitized.bitrate_kbps, 80_000);
    assert_eq!(sanitized.fps, 15);
    // Budget: (4096 * 8192) / 80_000 = 419s
    assert_eq!(sanitized.replay_duration_secs, 419);
    assert!(sanitized.to_replay_start_params().validate().is_ok());
}

#[test]
fn audio_profile_game_only_isolates_and_ignores_mic_and_output() {
    let prefs = UiPreferences {
        audio_profile: AudioCaptureProfile::GameOnly,
        mic_device: "alsa_input.usb-Fifine".into(),
        output_device: "alsa_output.usb-HyperX".into(),
        ..Default::default()
    };

    let audio = prefs.to_audio_config();
    assert_eq!(audio.mode, SystemAudioMode::Isolated);
    assert!(audio.game);
    assert!(!audio.call);
    assert!(!audio.microphone);
    assert_eq!(audio.mic_device, None);
    assert_eq!(audio.output_device, None);
    assert!(!audio.separate_tracks);
}

#[test]
fn audio_profile_system_and_mic_routes_properly() {
    let prefs = UiPreferences {
        audio_profile: AudioCaptureProfile::SystemAndMic,
        output_device: "alsa_output.usb-HyperX".into(),
        mic_device: "off".into(),
        ..Default::default()
    };

    let audio = prefs.to_audio_config();
    assert_eq!(audio.mode, SystemAudioMode::DesktopLoopback);
    assert!(audio.game);
    assert_eq!(audio.output_device, Some("alsa_output.usb-HyperX".into()));
    assert!(!audio.microphone);
    assert_eq!(audio.mic_device, None);
    assert!(audio.separate_tracks);
}

#[test]
fn legacy_v050_json_migration_defaults_new_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("ui.json");

    let legacy_json = r#"{
        "replay_duration_secs": 60,
        "replay_max_ram_mb": 512,
        "format": "mp4",
        "quality": "high",
        "fps": 60,
        "audio_game": true,
        "audio_call": true,
        "audio_mic": true,
        "clear_after_save": true,
        "start_minimized": false
    }"#;
    fs::write(&cfg_path, legacy_json.as_bytes()).unwrap();

    let loaded = UiPreferences::load_from(&cfg_path);
    assert_eq!(loaded.replay_duration_secs, 60);
    assert_eq!(loaded.bitrate_kbps, 15_000);
    assert_eq!(loaded.resolution, ResolutionPreset::Native);
    assert_eq!(loaded.audio_profile, AudioCaptureProfile::SystemAndMic);
    assert_eq!(loaded.replay_hotkey, "ALT+F8");
    assert_eq!(loaded.record_hotkey, "ALT+F9");
    assert!(loaded.clear_after_save);
}

#[test]
fn corrupt_or_out_of_bounds_json_recovers_to_valid_params() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("ui.json");

    fs::write(&cfg_path, b"{not-json").unwrap();
    assert_eq!(
        UiPreferences::load_from(&cfg_path),
        UiPreferences::default()
    );
}

#[test]
fn test_overlay_enabled_default_and_roundtrip() {
    let mut prefs = UiPreferences::default();
    assert!(prefs.overlay_enabled);

    prefs.overlay_enabled = false;
    let json = serde_json::to_string(&prefs).unwrap();
    let loaded: UiPreferences = serde_json::from_str(&json).unwrap();
    assert!(!loaded.overlay_enabled);

    // Migração de JSON legado sem o campo overlay_enabled
    let legacy_json = r#"{"bitrate_kbps": 12000}"#;
    let loaded: UiPreferences = serde_json::from_str(legacy_json).unwrap();
    assert!(loaded.overlay_enabled);
}

