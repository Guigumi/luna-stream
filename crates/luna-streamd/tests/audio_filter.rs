use luna_streamd::audio::filter::{StreamCategory, StreamProps, classify};

#[test]
fn identifies_discord_as_call() {
    let own_pid = 1000;

    let p1 = StreamProps {
        application_name: Some("Discord".into()),
        binary_name: Some("discord".into()),
        node_name: Some("WEBRTC VoiceEngine".into()),
        media_name: Some("Playback".into()),
        process_id: Some(5432),
    };
    assert_eq!(classify(&p1, own_pid), StreamCategory::Call);

    let p2 = StreamProps {
        application_name: Some("Vesktop".into()),
        binary_name: Some("vesktop".into()),
        node_name: None,
        media_name: None,
        process_id: Some(5433),
    };
    assert_eq!(classify(&p2, own_pid), StreamCategory::Call);

    let p3 = StreamProps {
        application_name: None,
        binary_name: None,
        node_name: Some("WebRTC VoiceEngine".into()),
        media_name: None,
        process_id: Some(5434),
    };
    assert_eq!(classify(&p3, own_pid), StreamCategory::Call);
}

#[test]
fn identifies_game_as_game() {
    let own_pid = 1000;

    let p1 = StreamProps {
        application_name: Some("Counter-Strike 2".into()),
        binary_name: Some("cs2".into()),
        node_name: Some("ALSA Playback".into()),
        media_name: Some("Audio Stream".into()),
        process_id: Some(9999),
    };
    assert_eq!(classify(&p1, own_pid), StreamCategory::Game);

    let p2 = StreamProps {
        application_name: Some("Google Chrome".into()),
        binary_name: Some("chrome".into()),
        node_name: Some("Playback".into()),
        media_name: Some("YouTube Video".into()),
        process_id: Some(8888),
    };
    assert_eq!(classify(&p2, own_pid), StreamCategory::Game);

    let p3 = StreamProps {
        application_name: Some("Spotify".into()),
        binary_name: Some("spotify".into()),
        node_name: None,
        media_name: None,
        process_id: Some(7777),
    };
    assert_eq!(classify(&p3, own_pid), StreamCategory::Game);
}

#[test]
fn ignores_self() {
    let own_pid = 12345;

    let p1 = StreamProps {
        application_name: Some("luna-streamd".into()),
        binary_name: Some("luna-streamd".into()),
        node_name: Some("luna-audio-game".into()),
        media_name: None,
        process_id: Some(own_pid),
    };
    assert_eq!(classify(&p1, own_pid), StreamCategory::Ignored);

    let p2 = StreamProps {
        application_name: None,
        binary_name: None,
        node_name: Some("luna-audio-call".into()),
        media_name: None,
        process_id: Some(99999),
    };
    assert_eq!(classify(&p2, own_pid), StreamCategory::Ignored);
}
