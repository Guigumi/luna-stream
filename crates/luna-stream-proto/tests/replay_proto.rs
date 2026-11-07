use luna_stream_proto::{
    ContainerFormat, ErrorCode, RecordStartResult, RecordStatusResult, ReplaySaveParams,
    ReplayStartParams, SourceKind, methods,
};

#[test]
fn replay_start_defaults_and_validation() {
    assert_eq!(methods::REPLAY_START, "replay.start");
    assert_eq!(methods::REPLAY_SAVE, "replay.save");
    assert_eq!(methods::REPLAY_STATUS, "replay.status");
    assert_eq!(methods::REPLAY_STOP, "replay.stop");

    let p: ReplayStartParams = serde_json::from_str("{}").unwrap();
    assert_eq!(p.duration_secs, 30);
    assert_eq!(p.max_ram_mb, 512);
    assert_eq!(p.fps, 60);
    assert!(p.hotkey);
    assert_eq!(p.format, ContainerFormat::Mp4);
    assert_eq!(p.source, SourceKind::Screen);
    assert!(p.validate().is_ok());

    let too_short = ReplayStartParams {
        duration_secs: 4,
        ..p.clone()
    };
    assert_eq!(
        too_short.validate().unwrap_err().code,
        ErrorCode::InvalidParams as i32
    );

    let too_long = ReplayStartParams {
        duration_secs: 601,
        ..p.clone()
    };
    assert_eq!(
        too_long.validate().unwrap_err().code,
        ErrorCode::InvalidParams as i32
    );

    let low_ram = ReplayStartParams {
        max_ram_mb: 32,
        ..p
    };
    assert_eq!(
        low_ram.validate().unwrap_err().code,
        ErrorCode::InvalidParams as i32
    );
}

#[test]
fn replay_save_validation_and_error_codes() {
    let ok: ReplaySaveParams = serde_json::from_str("{}").unwrap();
    assert!(!ok.clear_after_save);
    assert_eq!(ok.last_secs, None);
    assert!(ok.validate().is_ok());

    let zero_window = ReplaySaveParams {
        last_secs: Some(0),
        ..ok
    };
    assert_eq!(
        zero_window.validate().unwrap_err().code,
        ErrorCode::InvalidParams as i32
    );

    assert_eq!(ErrorCode::AlreadyReplaying as i32, 1008);
    assert_eq!(ErrorCode::NotReplaying as i32, 1009);
    assert_eq!(ErrorCode::ReplayEmpty as i32, 1010);
}

#[test]
fn record_results_keep_json_backward_compatibility() {
    let start: RecordStartResult =
        serde_json::from_str(r#"{"encoder":"vah264enc","path":"/tmp/a.mp4","format":"mp4"}"#)
            .unwrap();
    assert!(!start.shared_session);

    let status: RecordStatusResult = serde_json::from_str(r#"{"recording":false}"#).unwrap();
    assert_eq!(status.last_error, None);
}
