use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::Parser;
use luna_stream_ui::config::UiPreferences;
use luna_stream_ui::gallery::{GalleryFilter, GallerySortField, MediaKind, SortOrder};
use luna_stream_ui::keybind;
use luna_stream_ui::overlay::{spawn_overlay_controller, OverlayEvent, OverlayKind};
use luna_stream_ui::single_instance::{InstanceRole, UiIncomingEvent, acquire_or_notify};
use luna_stream_ui::supervisor::{
    SupervisorCallbacks, SupervisorConfig, UiCommand, run_supervisor,
};
use luna_stream_ui::tray::{TrayAction, spawn_tray};
use luna_stream_ui::{
    AppWindow, apply_snapshot_to_window, handle_window_close_request, read_preferences_from_window,
    should_start_hidden,
};
use slint::ComponentHandle;
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
#[command(
    name = "luna-stream",
    about = "Aplicativo nativo desktop e de bandeja do Luna Stream"
)]
struct Cli {
    /// Inicia o aplicativo minimizado diretamente na bandeja do sistema
    #[arg(long)]
    minimized: bool,

    /// Caminho explícito para o socket do motor (padrão: $XDG_RUNTIME_DIR/luna-stream/daemon.sock)
    #[arg(long)]
    socket: Option<PathBuf>,
}

fn main() -> ExitCode {
    // Zero VRAM por padrão enquanto o usuário joga: renderização software sob demanda
    if env::var_os("SLINT_BACKEND").is_none() {
        unsafe {
            env::set_var("SLINT_BACKEND", "winit-software");
        }
    }

    let cli = Cli::parse();

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Falha ao inicializar runtime assíncrono: {e}");
            return ExitCode::FAILURE;
        }
    };

    let runtime_dir = env::var_os("XDG_RUNTIME_DIR");
    let ui_sock = match luna_stream_proto::paths::ui_socket_path(runtime_dir.as_deref()) {
        Some(p) => p,
        None => {
            eprintln!("Erro: XDG_RUNTIME_DIR não está definido.");
            return ExitCode::FAILURE;
        }
    };

    let (incoming_tx, mut incoming_rx) = mpsc::unbounded_channel();
    let _instance_guard = match runtime.block_on(acquire_or_notify(ui_sock, incoming_tx)) {
        Ok(InstanceRole::Primary(guard)) => guard,
        Ok(InstanceRole::SecondaryNotified) => {
            // Outra instância já está em execução e recebeu o sinal para exibir a janela
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("Falha no controle de instância única: {e}");
            return ExitCode::FAILURE;
        }
    };

    let app = match AppWindow::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Falha ao criar janela da interface Slint: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Estado compartilhado do overlay entre a thread tokio (supervisor) e a thread Slint (UI)
    let shared_overlay: Arc<Mutex<Option<OverlayEvent>>> = Arc::new(Mutex::new(None));
    let shared_overlay_for_callback = shared_overlay.clone();

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (tray_action_tx, mut tray_action_rx) = mpsc::unbounded_channel();

    let tray = runtime.block_on(spawn_tray(tray_action_tx));
    let tray_available = tray.is_some();

    // Carrega ícone para overlay
    let icon_png = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/icon.png")
    ).unwrap_or_default();

    // Spawn do OverlayController
    let overlay_sender = spawn_overlay_controller(icon_png, true);

    // Encaminha sinais de socket (show / overlay) para o supervisor
    let tx_for_incoming = cmd_tx.clone();
    let overlay_sender_for_socket = overlay_sender.clone();
    runtime.spawn(async move {
        while let Some(event) = incoming_rx.recv().await {
            match event {
                UiIncomingEvent::ShowWindow => {
                    let _ = tx_for_incoming.send(UiCommand::ShowWindow);
                }
                UiIncomingEvent::Overlay(event) => {
                    // Dispara overlay via supervisor (exibe notificação na UI + refresh da galeria se clipe)
                    let _ = tx_for_incoming.send(UiCommand::OverlayEvent(event.clone()));
                    // Mantém envio ao stub do controller (compatibilidade futura com Wayland layer-shell)
                    let _ = overlay_sender_for_socket.send(luna_stream_ui::overlay::OverlayCommand::Show(event));
                }
            }
        }
    });

    // Encaminha ações do menu da bandeja para o supervisor
    let tx_for_tray = cmd_tx.clone();
    runtime.spawn(async move {
        while let Some(action) = tray_action_rx.recv().await {
            match action {
                TrayAction::ShowWindow => {
                    let _ = tx_for_tray.send(UiCommand::ShowWindow);
                }
                TrayAction::SaveClip => {
                    let _ = tx_for_tray.send(UiCommand::SaveClip);
                }
                TrayAction::ToggleReplay => {
                    let _ = tx_for_tray.send(UiCommand::ToggleReplay);
                }
                TrayAction::ToggleRecord => {
                    let _ = tx_for_tray.send(UiCommand::ToggleRecord);
                }
                TrayAction::OpenClipsDir => {
                    let _ = tx_for_tray.send(UiCommand::OpenClipsDir);
                }
                TrayAction::Quit => {
                    let _ = tx_for_tray.send(UiCommand::Quit);
                }
            }
        }
    });

    // Conecta callbacks da interface Slint
    {
        let tx = cmd_tx.clone();
        app.on_toggle_replay(move || {
            let _ = tx.send(UiCommand::ToggleReplay);
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_save_clip(move || {
            let _ = tx.send(UiCommand::SaveClip);
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_toggle_record(move || {
            let _ = tx.send(UiCommand::ToggleRecord);
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_dismiss_error(move || {
            let _ = tx.send(UiCommand::DismissError);
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_open_clip(move |path_str| {
            let _ = tx.send(UiCommand::OpenClip(PathBuf::from(path_str.as_str())));
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_delete_clip(move |path_str| {
            let _ = tx.send(UiCommand::DeleteClip(PathBuf::from(path_str.as_str())));
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_undo_delete_clip(move || {
            let _ = tx.send(UiCommand::UndoDeleteClip);
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_open_clips_dir(move || {
            let _ = tx.send(UiCommand::OpenClipsDir);
        });
    }

    {
        let tx = cmd_tx.clone();
        let app_weak = app.as_weak();
        app.on_preferences_changed(move || {
            if let Some(window) = app_weak.upgrade() {
                let prefs = read_preferences_from_window(&window);
                let _ = tx.send(UiCommand::UpdatePreferences(prefs));
            }
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_open_media(move |path_str| {
            let _ = tx.send(UiCommand::OpenMedia(PathBuf::from(path_str.as_str())));
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_request_delete_media(move |path_str| {
            let _ = tx.send(UiCommand::RequestDeleteMedia(PathBuf::from(
                path_str.as_str(),
            )));
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_cancel_delete_media(move || {
            let _ = tx.send(UiCommand::CancelDeleteMedia);
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_confirm_delete_media(move |path_str| {
            let _ = tx.send(UiCommand::ConfirmDeleteMedia(PathBuf::from(
                path_str.as_str(),
            )));
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_open_captures_dir(move || {
            let _ = tx.send(UiCommand::OpenCapturesDir);
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_refresh_gallery(move || {
            let _ = tx.send(UiCommand::RefreshGallery);
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_reset_default_hotkeys(move || {
            let _ = tx.send(UiCommand::ResetDefaultHotkeys);
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_refresh_audio_devices(move || {
            let _ = tx.send(UiCommand::RefreshAudioDevices);
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_filter_changed(move |kind, format, date| {
            let (year, month, day) = if date.is_empty() {
                (None, None, None)
            } else {
                let parts: Vec<&str> = date.split('-').collect();
                (
                    parts.get(0).and_then(|s| s.parse().ok()),
                    parts.get(1).and_then(|s| s.parse().ok()),
                    parts.get(2).and_then(|s| s.parse().ok()),
                )
            };
            let filter = GalleryFilter {
                kind: match kind.as_str() {
                    "clip" => Some(MediaKind::Clip),
                    "recording" => Some(MediaKind::Recording),
                    _ => None,
                },
                format: match format.as_str() {
                    "mp4" => Some("mp4".to_string()),
                    "mkv" => Some("mkv".to_string()),
                    _ => None,
                },
                year,
                month,
                day,
                date: if date.is_empty() { None } else { Some(date.to_string()) },
            };
            let _ = tx.send(UiCommand::UpdateGalleryFilter(filter));
        });
    }

    {
        let tx = cmd_tx.clone();
        app.on_sort_changed(move |field, order| {
            let sort_field = match field.as_str() {
                "size" => GallerySortField::Size,
                "duration" => GallerySortField::Duration,
                _ => GallerySortField::Recent,
            };
            let sort_order = match order.as_str() {
                "asc" => SortOrder::Asc,
                _ => SortOrder::Desc,
            };
            let _ = tx.send(UiCommand::UpdateGallerySort {
                field: sort_field,
                order: sort_order,
            });
        });
    }

    {
        let tx = cmd_tx.clone();
        let app_weak = app.as_weak();
        app.on_key_pressed_for_capture(move |for_record, text, alt, ctrl, shift, meta| {
            match keybind::parse_key_event(&text, alt, ctrl, shift, meta) {
                keybind::KeyCaptureOutcome::Captured(trigger) => {
                    let _ = tx.send(UiCommand::CaptureHotkey {
                        for_record,
                        trigger,
                    });
                    if let Some(w) = app_weak.upgrade() {
                        w.set_capturing_replay_keybind(false);
                        w.set_capturing_record_keybind(false);
                    }
                }
                keybind::KeyCaptureOutcome::ResetDefault => {
                    let def = if for_record {
                        keybind::DEFAULT_RECORD_HOTKEY
                    } else {
                        keybind::DEFAULT_REPLAY_HOTKEY
                    };
                    let _ = tx.send(UiCommand::CaptureHotkey {
                        for_record,
                        trigger: def.to_string(),
                    });
                    if let Some(w) = app_weak.upgrade() {
                        w.set_capturing_replay_keybind(false);
                        w.set_capturing_record_keybind(false);
                    }
                }
                keybind::KeyCaptureOutcome::Cancelled => {
                    if let Some(w) = app_weak.upgrade() {
                        w.set_capturing_replay_keybind(false);
                        w.set_capturing_record_keybind(false);
                    }
                }
                keybind::KeyCaptureOutcome::IgnoredModifierOnly => {}
            }
        });
    }

    {
        let app_weak = app.as_weak();
        app.on_cancel_capture_hotkey(move || {
            if let Some(w) = app_weak.upgrade() {
                w.set_capturing_replay_keybind(false);
                w.set_capturing_record_keybind(false);
            }
        });
    }

    app.on_open_slint_website(|| {
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("xdg-open")
                .arg("https://slint.dev")
                .spawn();
        }
    });

    // Trata pedido de fechamento da janela (X)
    {
        let tx = cmd_tx.clone();
        app.window()
            .on_close_requested(move || handle_window_close_request(tray_available, &tx));
    }

    // Callbacks do Supervisor para a thread principal de UI
    let app_weak_for_snap = app.as_weak();
    let app_weak_for_show = app.as_weak();
    let app_weak_for_overlay = app.as_weak();
    let overlay_sender_for_callback = overlay_sender.clone();
    let runtime_handle = runtime.handle().clone();

    {
        let shared = shared_overlay.clone();
        let weak = app_weak_for_overlay.clone();
        app.on_check_overlay(move || {
            let event = {
                let mut guard = shared.lock().unwrap();
                guard.take()
            };
            if let Some(event) = event {
                eprintln!(
                    "[OVERLAY_TIMER] processando evento: kind={:?} title={}",
                    event.kind, event.title
                );
                std::io::stderr().flush().ok();

                let kind = match event.kind {
                    OverlayKind::ReplayStarted => 0,
                    OverlayKind::ReplayStopped => 1,
                    OverlayKind::ClipSaved => 2,
                    OverlayKind::RecordStarted => 3,
                    OverlayKind::RecordStopped => 4,
                };
                let title = event.title.clone();
                let subtitle = event.subtitle.clone();

                if let Some(window) = weak.upgrade() {
                    window.set_overlay_visible(true);
                    window.set_overlay_title(title.as_str().into());
                    window.set_overlay_subtitle(subtitle.as_str().into());
                    window.set_overlay_kind(kind);
                    eprintln!(
                        "[OVERLAY_TIMER] overlay definido: visible={}",
                        window.get_overlay_visible()
                    );
                    std::io::stderr().flush().ok();

                    // Auto-fade: esconde após 3 segundos
                    let weak_fade = weak.clone();
                    let handle = runtime_handle.clone();
                    handle.spawn(async move {
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = weak_fade.upgrade() {
                                w.set_overlay_visible(false);
                            }
                        });
                    });
                }
            }
        });
    }

    let callbacks = SupervisorCallbacks {
        on_snapshot: Box::new(move |snap| {
            let weak = app_weak_for_snap.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(window) = weak.upgrade() {
                    apply_snapshot_to_window(&window, &snap);
                }
            });
        }),
        on_show_window: Box::new(move || {
            let weak = app_weak_for_show.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(window) = weak.upgrade() {
                    let _ = window.show();
                    window.window().request_redraw();
                }
            });
        }),
        on_quit: Box::new(|| {
            let _ = slint::invoke_from_event_loop(|| {
                slint::quit_event_loop().ok();
            });
        }),
        on_overlay: Box::new(move |event| {
            eprintln!(
                "[OVERLAY] evento recebido: kind={:?} title={}",
                event.kind, event.title
            );
            std::io::stderr().flush().ok();

            // Escreve no mutex compartilhado para a thread Slint processar
            {
                let mut guard = shared_overlay_for_callback.lock().unwrap();
                *guard = Some(event.clone());
            }
            eprintln!("[OVERLAY] evento escrito no mutex compartilhado");
            std::io::stderr().flush().ok();

            // Mantém envio ao stub do controller (compatibilidade futura com Wayland layer-shell)
            let _ = overlay_sender_for_callback.send(luna_stream_ui::overlay::OverlayCommand::Show(event));
        }),
    };

    let daemon_sock = cli.socket.unwrap_or_else(|| {
        luna_stream_proto::paths::daemon_socket_path(runtime_dir.as_deref())
            .unwrap_or_else(|| PathBuf::from("/tmp/daemon.sock"))
    });

    let sup_cfg = SupervisorConfig {
        socket_path: daemon_sock,
        config_path: UiPreferences::default_path(),
        clips_dir: luna_stream_proto::paths::default_clips_dir(),
        daemon_bin_override: None,
        auto_spawn: true,
    };

    runtime.spawn(run_supervisor(sup_cfg, cmd_rx, tray, callbacks));

    let initial_prefs = UiPreferences::load_from(&UiPreferences::default_path());
    let start_hidden =
        should_start_hidden(cli.minimized, initial_prefs.start_minimized, tray_available);

    if !start_hidden {
        let _ = app.show();
    } else {
        let _ = cmd_tx.send(UiCommand::WindowVisibilityChanged(false));
    }

    let _ = slint::run_event_loop_until_quit();

    ExitCode::SUCCESS
}
