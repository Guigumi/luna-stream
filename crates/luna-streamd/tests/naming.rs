use luna_stream_proto::{ContainerFormat, ErrorCode};
use luna_streamd::audio::router::AudioGraph;
use luna_streamd::naming::{
    check_free_space, detect_active_game_name, generate_filename, resolve_output_path,
    sanitize_filename,
};
use tempfile::tempdir;

#[test]
fn sanitizes_complex_game_names() {
    assert_eq!(
        sanitize_filename("Counter-Strike: Global Offensive"),
        "Counter-Strike__Global_Offensive"
    );
    assert_eq!(
        sanitize_filename("Grand Theft Auto V / Online"),
        "Grand_Theft_Auto_V___Online"
    );
    assert_eq!(
        sanitize_filename("Game: *special?* <characters>|\"quotes\""),
        "Game___special____characters___quotes"
    );
    assert_eq!(sanitize_filename("   "), "recording");
    assert_eq!(sanitize_filename(""), "recording");

    let very_long = "a".repeat(100);
    let sanitized = sanitize_filename(&very_long);
    assert_eq!(sanitized.len(), 64);
}

#[test]
fn generates_automatic_filename_with_timestamp_and_game() {
    let name = generate_filename("DOOM_Eternal", ContainerFormat::Mp4);
    assert!(name.ends_with("_DOOM_Eternal.mp4"));
    // Format YYYY-MM-DD_HH-MM-SS_DOOM_Eternal.mp4
    let parts: Vec<&str> = name.split('_').collect();
    assert!(parts.len() >= 3);
    assert_eq!(parts[0].len(), 10); // YYYY-MM-DD
    assert_eq!(parts[1].len(), 8); // HH-MM-SS
}

#[test]
fn detects_game_from_audiograph() {
    let json = r#"[
        {
            "id": 42,
            "type": "PipeWire:Interface:Node",
            "info": {
                "props": {
                    "media.class": "Stream/Output/Audio",
                    "application.name": "Cyberpunk 2077",
                    "application.process.id": 1234
                }
            }
        },
        {
            "id": 43,
            "type": "PipeWire:Interface:Node",
            "info": {
                "props": {
                    "media.class": "Stream/Output/Audio",
                    "application.name": "Discord",
                    "application.process.id": 5678
                }
            }
        }
    ]"#;

    let graph = AudioGraph::from_json(json).expect("parse graph");
    let detected = detect_active_game_name(&graph, 9999);
    assert_eq!(detected, "Cyberpunk 2077");
}

#[test]
fn falls_back_to_desktop_when_no_game() {
    let json = r#"[
        {
            "id": 43,
            "type": "PipeWire:Interface:Node",
            "info": {
                "props": {
                    "media.class": "Stream/Output/Audio",
                    "application.name": "Discord",
                    "application.process.id": 5678
                }
            }
        }
    ]"#;

    let graph = AudioGraph::from_json(json).expect("parse graph");
    let detected = detect_active_game_name(&graph, 9999);
    assert_eq!(detected, "Desktop");

    let empty = AudioGraph::default();
    assert_eq!(detect_active_game_name(&empty, 9999), "Desktop");
}

#[test]
fn refuses_path_when_disk_is_full() {
    let dir = tempdir().unwrap();
    // 1024 bytes deve estar disponível
    let ok = check_free_space(dir.path(), 1024);
    assert!(ok.is_ok());

    // u64::MAX nunca está disponível
    let err = check_free_space(dir.path(), u64::MAX);
    assert_eq!(err, Err(ErrorCode::OutputNotWritable));
}

#[test]
fn resolves_output_path_explicit_or_automatic() {
    let dir = tempdir().unwrap();

    // Explícito
    let explicit_target = dir.path().join("my_custom_rec.mp4");
    let resolved = resolve_output_path(
        Some(&explicit_target),
        None,
        None,
        ContainerFormat::Mp4,
        None,
        9999,
    )
    .expect("resolve explicit");
    assert_eq!(resolved, explicit_target);

    // Automático com dir e game name
    let resolved_auto = resolve_output_path(
        None,
        Some(dir.path()),
        Some("Half-Life 2"),
        ContainerFormat::Mkv,
        None,
        9999,
    )
    .expect("resolve auto");
    assert_eq!(resolved_auto.parent().unwrap(), dir.path());
    assert!(
        resolved_auto
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("Half-Life_2")
    );
    assert!(
        resolved_auto
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with(".mkv")
    );
}

#[test]
fn clip_path_creates_clips_subdir_and_resolves_same_second_collisions() {
    let dir = tempfile::tempdir().unwrap();
    let p1 = luna_streamd::naming::resolve_clip_path(
        Some(dir.path()),
        Some("Counter Strike 2"),
        ContainerFormat::Mp4,
        None,
        1234,
        10 * 1024 * 1024,
    )
    .unwrap();

    let name1 = p1.file_name().unwrap().to_str().unwrap();
    assert!(name1.starts_with("Clip_"));
    assert!(name1.ends_with("_Counter_Strike_2.mp4"));

    // Simula arquivo já salvo no mesmo segundo
    std::fs::write(&p1, b"clip1").unwrap();

    let p2 = luna_streamd::naming::resolve_clip_path(
        Some(dir.path()),
        Some("Counter Strike 2"),
        ContainerFormat::Mp4,
        None,
        1234,
        10 * 1024 * 1024,
    )
    .unwrap();
    assert_ne!(p1, p2);
    assert!(p2
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .ends_with("_Counter_Strike_2_2.mp4"));
}

#[test]
fn clip_path_rejects_when_estimated_size_plus_margin_exceeds_disk() {
    let dir = tempfile::tempdir().unwrap();
    let err = luna_streamd::naming::resolve_clip_path(
        Some(dir.path()),
        Some("Desktop"),
        ContainerFormat::Mkv,
        None,
        1234,
        u64::MAX - 1024,
    )
    .unwrap_err();
    assert_eq!(err.code, luna_stream_proto::ErrorCode::OutputNotWritable as i32);
}
