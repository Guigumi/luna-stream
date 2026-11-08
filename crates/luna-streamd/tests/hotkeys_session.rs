use luna_stream_proto::{HotkeysConfigureParams, RecordStartParams, SourceKind};
use luna_streamd::hotkey::{HotkeyAction, shortcut_id_for_trigger};
use luna_streamd::session::SessionManager;
use luna_streamd::token_store::TokenStore;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn test_shortcut_id_generation() {
    assert_eq!(
        shortcut_id_for_trigger("save_clip", "ALT+F8"),
        "save_clip_alt_f8"
    );
    assert_eq!(
        shortcut_id_for_trigger("toggle_record", "CTRL+SHIFT+R"),
        "toggle_record_ctrl_shift_r"
    );
    assert_eq!(
        shortcut_id_for_trigger("save_clip", "Alt + F8"),
        "save_clip_alt_f8"
    );
}

#[tokio::test]
async fn test_session_manager_toggle_record_cycle() {
    gstreamer::init().expect("gst init");
    let dir = tempdir().unwrap();
    let token_store = Arc::new(TokenStore::new(dir.path().join("tokens")));
    let output_file = dir.path().join("toggle_test.mp4");

    let mgr = SessionManager::new();

    // Configura defaults de gravação apontando para SourceKind::Test e arquivo de saída
    let defaults = RecordStartParams {
        source: SourceKind::Test,
        output: Some(output_file.clone()),
        ..Default::default()
    };

    mgr.configure_hotkeys(
        HotkeysConfigureParams {
            replay_trigger: "Alt+F8".into(),
            record_trigger: "Alt+F9".into(),
            record_defaults: defaults,
        },
        token_store.clone(),
    )
    .await;

    // Estado inicial: não gravando
    let initial_status = mgr.record_status().await;
    assert!(!initial_status.recording);

    // 1º Toggle: inicia gravação
    mgr.trigger_hotkey_action(HotkeyAction::ToggleRecord, &token_store)
        .await;

    let recording_status = mgr.record_status().await;
    assert!(recording_status.recording);
    assert_eq!(recording_status.path, Some(output_file.clone()));

    // Aguarda 1200ms para ter dados no pipeline
    tokio::time::sleep(Duration::from_millis(1200)).await;

    // 2º Toggle: para a gravação
    mgr.trigger_hotkey_action(HotkeyAction::ToggleRecord, &token_store)
        .await;

    let stopped_status = mgr.record_status().await;
    assert!(!stopped_status.recording);
    assert!(
        output_file.exists(),
        "O arquivo gravado deve existir no disco"
    );
    let metadata = std::fs::metadata(&output_file).expect("metadata");
    assert!(metadata.len() > 1024, "Arquivo deve ter conteúdo gravado");
}
