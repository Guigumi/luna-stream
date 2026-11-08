use luna_stream_proto::{ContainerFormat, QualityPreset};
use luna_stream_ui::config::UiPreferences;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn defaults_roundtrip_and_atomic_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("luna-stream").join("ui.json");

    let mut prefs = UiPreferences::load_from(&cfg_path);
    assert_eq!(prefs.replay_duration_secs, 30);
    assert_eq!(prefs.replay_max_ram_mb, 512);
    assert_eq!(prefs.format, ContainerFormat::Mp4);
    assert_eq!(prefs.quality, QualityPreset::High);
    assert_eq!(prefs.fps, 60);

    prefs.replay_duration_secs = 120;
    prefs.format = ContainerFormat::Mkv;
    prefs.audio_mic = false;
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
    assert_eq!(loaded, prefs);
    assert!(loaded.to_replay_start_params().validate().is_ok());
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

    fs::write(
        &cfg_path,
        r#"{"replay_duration_secs":0,"replay_max_ram_mb":8,"fps":999}"#,
    )
    .unwrap();
    let sanitized = UiPreferences::load_from(&cfg_path);
    assert_eq!(sanitized.replay_duration_secs, 30);
    assert_eq!(sanitized.replay_max_ram_mb, 512);
    assert_eq!(sanitized.fps, 60);
    assert!(sanitized.to_replay_start_params().validate().is_ok());
}
