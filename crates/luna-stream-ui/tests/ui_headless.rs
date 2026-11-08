use luna_stream_ui::{
    AppWindow, apply_snapshot_to_window, handle_window_close_request, read_preferences_from_window,
    should_start_hidden,
    supervisor::{UiCommand, UiSnapshot},
};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

#[test]
fn headless_ui_bindings_callbacks_and_slint_attribution_compliance() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();

    // Regra obrigatória ADR-0003: atribuição Slint presente e não vazia
    let attribution = app.get_slint_attribution();
    assert!(
        attribution
            .as_str()
            .contains("Interface construída com Slint"),
        "atribuição obrigatória da licença Royalty-Free 2.0 ausente: {attribution}"
    );

    // Navegação de abas e aplicação de snapshot
    assert_eq!(app.get_active_tab(), 0);
    let snap = UiSnapshot {
        daemon_connected: true,
        encoder_label: "vah264enc · GPU".into(),
        replay_active: true,
        replay_progress: 80,
        replay_buffer_label: "24s / 30s".into(),
        ..Default::default()
    };
    apply_snapshot_to_window(&app, &snap);

    assert!(app.get_replay_active());
    assert_eq!(app.get_replay_progress(), 80);
    assert_eq!(app.get_encoder_label().as_str(), "vah264enc · GPU");

    // Callback de salvar clipe e leitura de preferências
    let saved_clicked = Rc::new(AtomicBool::new(false));
    let flag = saved_clicked.clone();
    app.on_save_clip(move || flag.store(true, Ordering::SeqCst));
    app.invoke_save_clip();
    assert!(saved_clicked.load(Ordering::SeqCst));

    app.set_pref_replay_duration_secs(60);
    app.set_pref_audio_call(false);
    let prefs = read_preferences_from_window(&app);
    assert_eq!(prefs.replay_duration_secs, 60);
    assert!(!prefs.audio_call);
}

#[test]
fn close_and_minimized_safeguards_respect_tray_availability() {
    assert!(should_start_hidden(true, false, true));
    assert!(should_start_hidden(false, true, true));
    assert!(!should_start_hidden(true, true, false));

    let (tx, mut rx) = mpsc::unbounded_channel();
    let resp_with_tray = handle_window_close_request(true, &tx);
    assert!(matches!(
        resp_with_tray,
        slint::CloseRequestResponse::HideWindow
    ));
    assert!(matches!(
        rx.try_recv().unwrap(),
        UiCommand::WindowVisibilityChanged(false)
    ));

    let _resp_no_tray = handle_window_close_request(false, &tx);
    assert!(matches!(rx.try_recv().unwrap(), UiCommand::Quit));
}
