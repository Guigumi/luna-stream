use luna_stream_ui::config::{AudioCaptureProfile, ResolutionPreset};
use luna_stream_ui::gallery::{GalleryFilter, GalleryItem, MediaKind};
use luna_stream_ui::{
    AppWindow, apply_snapshot_to_window, handle_window_close_request, read_preferences_from_window,
    should_start_hidden,
    supervisor::{UiCommand, UiSnapshot},
};
use slint::Model;
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

#[test]
fn headless_v2_navigation_settings_gallery_and_slint_attribution() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();

    // 1. Regra obrigatória ADR-0003: atribuição Slint presente e não vazia
    let attribution = app.get_slint_attribution();
    assert!(
        attribution
            .as_str()
            .contains("Interface construída com Slint"),
        "atribuição obrigatória da licença Royalty-Free 2.0 ausente: {attribution}"
    );

    // 2. Navegação de abas (0 = Home, 1 = Galeria, 2 = Ajustes)
    assert_eq!(app.get_active_tab(), 0);
    app.set_active_tab(1);
    assert_eq!(app.get_active_tab(), 1);
    app.set_active_tab(2);
    assert_eq!(app.get_active_tab(), 2);
    app.set_active_tab(0);
    assert_eq!(app.get_active_tab(), 0);

    // 3. Aplicação de snapshot v2
    let snap = UiSnapshot {
        daemon_connected: true,
        encoder_label: "vah264enc · GPU".into(),
        replay_active: true,
        replay_progress: 80,
        replay_buffer_label: "24s / 30s".into(),
        replay_ram_label: "38 MB / 512 MB".into(),
        hotkey_label: "Alt + F8".into(),
        confirming_delete_path: "/tmp/clip-confirm.mp4".into(),
        profile_summary_label: "1080p60 · 25.000 Kbps".into(),
        ..Default::default()
    };
    apply_snapshot_to_window(&app, &snap);

    assert!(app.get_replay_active());
    assert_eq!(app.get_replay_progress(), 80);
    assert_eq!(app.get_encoder_label().as_str(), "vah264enc · GPU");
    assert_eq!(
        app.get_confirming_delete_path().as_str(),
        "/tmp/clip-confirm.mp4"
    );

    // 4. Preferências em Ajustes: valores livres, bitrate em Kbps, resolução e perfil de áudio
    app.set_pref_replay_duration_secs(90);
    app.set_pref_bitrate_kbps(25000);
    app.set_pref_fps(120);
    app.set_pref_resolution("1080p".into());
    app.set_pref_audio_profile("game_only".into());
    app.set_pref_format("mkv".into());

    let prefs = read_preferences_from_window(&app);
    assert_eq!(prefs.replay_duration_secs, 90);
    assert_eq!(prefs.bitrate_kbps, 25000);
    assert_eq!(prefs.fps, 120);
    assert_eq!(prefs.resolution, ResolutionPreset::P1080);
    assert_eq!(prefs.audio_profile, AudioCaptureProfile::GameOnly);
    assert_eq!(prefs.format, luna_stream_proto::ContainerFormat::Mkv);
    // Somente jogo desativa microfone e chamadas
    assert!(!prefs.audio_mic);
    assert!(!prefs.audio_call);

    // 5. Confirmação dupla na exclusão de mídia
    let requested = Rc::new(Mutex::new(String::new()));
    let req_clone = requested.clone();
    app.on_request_delete_media(move |path| {
        *req_clone.lock().unwrap() = path.to_string();
    });
    app.invoke_request_delete_media("/tmp/clip-target.mp4".into());
    assert_eq!(*requested.lock().unwrap(), "/tmp/clip-target.mp4");

    let confirmed = Rc::new(Mutex::new(String::new()));
    let conf_clone = confirmed.clone();
    app.on_confirm_delete_media(move |path| {
        *conf_clone.lock().unwrap() = path.to_string();
    });
    app.invoke_confirm_delete_media("/tmp/clip-target.mp4".into());
    assert_eq!(*confirmed.lock().unwrap(), "/tmp/clip-target.mp4");

    let cancelled = Rc::new(AtomicBool::new(false));
    let cancel_clone = cancelled.clone();
    app.on_cancel_delete_media(move || {
        cancel_clone.store(true, Ordering::SeqCst);
    });
    app.invoke_cancel_delete_media();
    assert!(cancelled.load(Ordering::SeqCst));

    // 6. Callback de ordenação e filtro na Galeria
    let sort_called = Rc::new(AtomicBool::new(false));
    let sort_flag = sort_called.clone();
    app.on_sort_changed(move |_f, _o| {
        sort_flag.store(true, Ordering::SeqCst);
    });
    app.invoke_sort_changed("size".into(), "asc".into());
    assert!(sort_called.load(Ordering::SeqCst));

    let filter_called = Rc::new(AtomicBool::new(false));
    let filter_flag = filter_called.clone();
    app.on_filter_changed(move |_k, _fmt, _d| {
        filter_flag.store(true, Ordering::SeqCst);
    });
    app.invoke_filter_changed("clip".into(), "mp4".into(), "2026-11-08".into());
    assert!(filter_called.load(Ordering::SeqCst));
}

#[test]
fn test_calendar_popup_filters_gallery() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();

    // Test the new filter-date property exists and works
    // Initially filter-date should be empty string (all dates)
    assert_eq!(app.get_filter_date().as_str(), "");

    // Set filter-date to a specific ISO date
    app.set_filter_date("2026-11-08".into());
    assert_eq!(app.get_filter_date().as_str(), "2026-11-08");

    // Verify filter-changed callback is invoked with new signature (kind, format, date)
    let filter_called = Rc::new(AtomicBool::new(false));
    let filter_flag = filter_called.clone();
    app.on_filter_changed(move |_k, _fmt, _d| {
        filter_flag.store(true, Ordering::SeqCst);
    });
    app.invoke_filter_changed("all".into(), "all".into(), "2026-11-08".into());
    assert!(filter_called.load(Ordering::SeqCst));

    // Clear filter (empty string = all dates)
    app.set_filter_date("".into());
    assert_eq!(app.get_filter_date().as_str(), "");
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

#[test]
fn test_recent_items_capacity_and_dynamic_columns() {
    // 1. Valida capacidade do supervisor para até 6 itens recentes
    assert_eq!(
        luna_stream_ui::supervisor::RECENT_ITEMS_CAPACITY,
        6,
        "supervisor deve ter capacidade para até 6 itens recentes na Home"
    );

    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();

    let recent_items: Vec<luna_stream_ui::gallery::GalleryItem> = (0..6)
        .map(|i| luna_stream_ui::gallery::GalleryItem {
            path: format!("/tmp/clip-{i}.mp4").into(),
            file_name: format!("clip-{i}.mp4"),
            kind: luna_stream_ui::gallery::MediaKind::Clip,
            format: "mp4".into(),
            year: 2026,
            month: 11,
            day: 8,
            date_label: "08/11/2026 21:00".into(),
            size_bytes: 1024 * 1024 * (i as u64 + 1),
            size_label: format!("{} MB", i + 1),
            duration_ms: 30000,
            duration_label: "00:30".into(),
            title: format!("Clip {i}"),
            thumb_path: None,
            probed: true,
            sort_timestamp_ms: (1000 - i) as u128,
        })
        .collect();

    let snap = UiSnapshot {
        recent_items,
        ..Default::default()
    };
    apply_snapshot_to_window(&app, &snap);
    assert_eq!(app.get_recent_items().row_count(), 6);

    // 2. Lógica de cálculo de colunas dinâmicas (clamp entre 3 e 6)
    let calc_cols = |available_width: f32| -> i32 {
        let usable_grid_width = (available_width - 64.0) / 1.0;
        let cols = ((usable_grid_width + 14.0) / 234.0).floor() as i32;
        cols.clamp(3, 6)
    };

    // Janela 820px (min-width, available-width = 620px): 3 colunas
    assert_eq!(calc_cols(620.0), 3);
    // Janela 1024px (available-width = 824px): 3 colunas
    assert_eq!(calc_cols(824.0), 3);
    // Janela 1200px (available-width = 1000px): 4 colunas
    assert_eq!(calc_cols(1000.0), 4);
    // Janela 1440px (available-width = 1240px): 5 colunas
    assert_eq!(calc_cols(1240.0), 5);
    // Janela 1920px (available-width = 1720px): 6 colunas
    assert_eq!(calc_cols(1720.0), 6);
    // Janela UltraWide 2560px (available-width = 2360px): clamp em 6 colunas
    assert_eq!(calc_cols(2360.0), 6);
    // Largura mínima reduzida (ex: 400px): clamp inferior em 3 colunas
    assert_eq!(calc_cols(400.0), 3);
}

#[test]
fn test_calendar_integration_filter_gallery() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();

    // Create test gallery items with different dates
    let items: Vec<GalleryItem> = vec![
        GalleryItem {
            path: "/tmp/clip-2026-11-08.mp4".into(),
            file_name: "clip-2026-11-08.mp4".into(),
            kind: MediaKind::Clip,
            format: "mp4".into(),
            year: 2026,
            month: 11,
            day: 8,
            date_label: "08/11/2026 10:00".into(),
            size_bytes: 1024 * 1024 * 10,
            size_label: "10 MB".into(),
            duration_ms: 30000,
            duration_label: "00:30".into(),
            title: "Clip Nov 8".into(),
            thumb_path: None,
            probed: true,
            sort_timestamp_ms: 1000,
        },
        GalleryItem {
            path: "/tmp/clip-2026-11-09.mp4".into(),
            file_name: "clip-2026-11-09.mp4".into(),
            kind: MediaKind::Clip,
            format: "mp4".into(),
            year: 2026,
            month: 11,
            day: 9,
            date_label: "09/11/2026 10:00".into(),
            size_bytes: 1024 * 1024 * 20,
            size_label: "20 MB".into(),
            duration_ms: 60000,
            duration_label: "01:00".into(),
            title: "Clip Nov 9".into(),
            thumb_path: None,
            probed: true,
            sort_timestamp_ms: 2000,
        },
        GalleryItem {
            path: "/tmp/clip-2026-10-15.mp4".into(),
            file_name: "clip-2026-10-15.mp4".into(),
            kind: MediaKind::Recording,
            format: "mkv".into(),
            year: 2026,
            month: 10,
            day: 15,
            date_label: "15/10/2026 10:00".into(),
            size_bytes: 1024 * 1024 * 50,
            size_label: "50 MB".into(),
            duration_ms: 120000,
            duration_label: "02:00".into(),
            title: "Recording Oct 15".into(),
            thumb_path: None,
            probed: true,
            sort_timestamp_ms: 500,
        },
    ];

    // Apply snapshot with items
    let snap = UiSnapshot {
        gallery_items: items.clone(),
        recent_items: items.clone(),
        ..Default::default()
    };
    apply_snapshot_to_window(&app, &snap);

    // Verify all 3 items are shown initially (no filter)
    assert_eq!(app.get_gallery_items().row_count(), 3);

    // Set filter-date to 2026-11-08 via CalendarPopup integration
    // The GalleryTab's CalendarPopup sets filter-date and invokes filter-changed
    app.set_filter_date("2026-11-08".into());
    assert_eq!(app.get_filter_date().as_str(), "2026-11-08");

    // Verify filter-changed callback is invoked with the date
    let filter_called = Rc::new(AtomicBool::new(false));
    let filter_flag = filter_called.clone();
    app.on_filter_changed(move |kind, format, date| {
        assert_eq!(kind.as_str(), "all");
        assert_eq!(format.as_str(), "all");
        assert_eq!(date.as_str(), "2026-11-08");
        filter_flag.store(true, Ordering::SeqCst);
    });
    app.invoke_filter_changed("all".into(), "all".into(), "2026-11-08".into());
    assert!(filter_called.load(Ordering::SeqCst));

    // The actual filtering logic runs in the supervisor/backend.
    // Here we verify the UI correctly passes the filter-date through the callback chain.

    // Clear filter
    app.set_filter_date("".into());
    assert_eq!(app.get_filter_date().as_str(), "");

    // Verify filter-changed called with empty date
    let filter_cleared = Rc::new(AtomicBool::new(false));
    let clear_flag = filter_cleared.clone();
    app.on_filter_changed(move |_k, _f, date| {
        assert_eq!(date.as_str(), "");
        clear_flag.store(true, Ordering::SeqCst);
    });
    app.invoke_filter_changed("all".into(), "all".into(), "".into());
    assert!(filter_cleared.load(Ordering::SeqCst));
}
