slint::include_modules!();

use luna_stream_proto::{ContainerFormat, QualityPreset};
use slint::VecModel;
use std::rc::Rc;
use tokio::sync::mpsc;

use crate::config::UiPreferences;
use crate::supervisor::{UiCommand, UiSnapshot};

pub mod config;
pub mod gallery;
pub mod keybind;
pub mod single_instance;
pub mod supervisor;
pub mod tray;

/// Aplica um snapshot do Supervisor aos estados e propriedades da janela Slint.
pub fn apply_snapshot_to_window(window: &AppWindow, snap: &UiSnapshot) {
    window.set_daemon_connected(snap.daemon_connected);
    window.set_encoder_label(snap.encoder_label.as_str().into());
    window.set_error_banner(snap.error_banner.as_str().into());

    window.set_replay_active(snap.replay_active);
    window.set_replay_progress(snap.replay_progress as i32);
    window.set_replay_buffer_label(snap.replay_buffer_label.as_str().into());
    window.set_replay_ram_label(snap.replay_ram_label.as_str().into());
    window.set_hotkey_label(snap.hotkey_label.as_str().into());
    window.set_can_save_clip(snap.can_save_clip);
    window.set_saving_clip(snap.saving_clip);
    window.set_last_saved_label(snap.last_saved_label.as_str().into());

    window.set_recording(snap.recording);
    window.set_record_status_label(snap.record_status_label.as_str().into());
    window.set_shared_session(snap.shared_session);
    window.set_restart_hint(snap.restart_hint.as_str().into());

    let clip_items: Vec<UiClipItem> = snap
        .clips
        .iter()
        .map(|c| UiClipItem {
            file_name: c.file_name.as_str().into(),
            path: c.path.to_string_lossy().to_string().into(),
            game: c.game.as_str().into(),
            recorded_at: c.recorded_at.as_str().into(),
            size_label: c.size_label.as_str().into(),
        })
        .collect();
    window.set_clips(Rc::new(VecModel::from(clip_items)).into());
    window.set_undo_banner_visible(snap.undo_banner_visible);

    window.set_pref_replay_duration_secs(snap.prefs.replay_duration_secs as i32);
    window.set_pref_replay_ram_mb(snap.prefs.replay_max_ram_mb as i32);
    window.set_pref_format(snap.prefs.format.extension().into());
    let q_str = match snap.prefs.quality {
        QualityPreset::Ultra => "ultra",
        QualityPreset::High => "high",
        QualityPreset::Medium => "medium",
        QualityPreset::Low => "low",
    };
    window.set_pref_quality(q_str.into());
    window.set_pref_fps(snap.prefs.fps as i32);
    window.set_pref_audio_game(snap.prefs.audio_game);
    window.set_pref_audio_call(snap.prefs.audio_call);
    window.set_pref_audio_mic(snap.prefs.audio_mic);
    window.set_pref_clear_after_save(snap.prefs.clear_after_save);
    window.set_pref_start_minimized(snap.prefs.start_minimized);
}

/// Extrai as preferências selecionadas na interface do usuário.
pub fn read_preferences_from_window(window: &AppWindow) -> UiPreferences {
    let fmt_str = window.get_pref_format();
    let format = if fmt_str.as_str() == "mkv" {
        ContainerFormat::Mkv
    } else {
        ContainerFormat::Mp4
    };

    let q_str = window.get_pref_quality();
    let quality = match q_str.as_str() {
        "ultra" => QualityPreset::Ultra,
        "medium" => QualityPreset::Medium,
        "low" => QualityPreset::Low,
        _ => QualityPreset::High,
    };

    UiPreferences {
        replay_duration_secs: window.get_pref_replay_duration_secs() as u32,
        replay_max_ram_mb: window.get_pref_replay_ram_mb() as u32,
        format,
        quality,
        fps: window.get_pref_fps() as u32,
        audio_game: window.get_pref_audio_game(),
        audio_call: window.get_pref_audio_call(),
        audio_mic: window.get_pref_audio_mic(),
        clear_after_save: window.get_pref_clear_after_save(),
        start_minimized: window.get_pref_start_minimized(),
        ..Default::default()
    }
}

/// Trata a solicitação de fechamento da janela:
/// Se a bandeja estiver ativa, oculta para a bandeja. Se não houver bandeja no ambiente,
/// encerra a aplicação com segurança para não prender processo órfão invisível.
pub fn handle_window_close_request(
    tray_available: bool,
    cmd_tx: &mpsc::UnboundedSender<UiCommand>,
) -> slint::CloseRequestResponse {
    if tray_available {
        let _ = cmd_tx.send(UiCommand::WindowVisibilityChanged(false));
        slint::CloseRequestResponse::HideWindow
    } else {
        let _ = cmd_tx.send(UiCommand::Quit);
        slint::CloseRequestResponse::HideWindow
    }
}

/// Determina se a aplicação deve iniciar com a janela oculta.
pub fn should_start_hidden(
    cli_minimized: bool,
    pref_start_minimized: bool,
    tray_available: bool,
) -> bool {
    (cli_minimized || pref_start_minimized) && tray_available
}
