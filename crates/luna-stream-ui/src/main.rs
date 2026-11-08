use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use luna_stream_ui::config::UiPreferences;
use luna_stream_ui::single_instance::{InstanceRole, acquire_or_notify};
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

    let (show_tx, mut show_rx) = mpsc::unbounded_channel();
    let _instance_guard = match runtime.block_on(acquire_or_notify(ui_sock, show_tx)) {
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

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (tray_action_tx, mut tray_action_rx) = mpsc::unbounded_channel();

    let tray = runtime.block_on(spawn_tray(tray_action_tx));
    let tray_available = tray.is_some();

    // Encaminha sinais de instância secundária para o supervisor
    let tx_for_show = cmd_tx.clone();
    runtime.spawn(async move {
        while show_rx.recv().await.is_some() {
            let _ = tx_for_show.send(UiCommand::ShowWindow);
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

    app.on_open_slint_website(|| {
        let _ = std::process::Command::new("xdg-open")
            .arg("https://slint.dev")
            .spawn();
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
