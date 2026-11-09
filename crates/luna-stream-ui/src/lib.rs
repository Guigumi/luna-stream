slint::include_modules!();

use luna_stream_proto::ContainerFormat;
use slint::VecModel;
use std::rc::Rc;
use tokio::sync::mpsc;

use crate::config::{AudioCaptureProfile, ResolutionPreset, UiPreferences};
use crate::gallery::GalleryItem;
use crate::keybind::format_hotkey_label;
use crate::supervisor::{UiCommand, UiSnapshot};

pub mod config;
pub mod gallery;
pub mod keybind;
pub mod overlay;
pub mod single_instance;
pub mod supervisor;
pub mod tray;

fn to_ui_media_item(item: &GalleryItem) -> UiMediaItem {
    let (thumbnail, has_thumbnail) = if let Some(thumb_path) = &item.thumb_path {
        match slint::Image::load_from_path(thumb_path) {
            Ok(img) => (img, true),
            Err(_) => (slint::Image::default(), false),
        }
    } else {
        (slint::Image::default(), false)
    };

    UiMediaItem {
        path: item.path.to_string_lossy().to_string().into(),
        title: item.title.as_str().into(),
        is_clip: item.kind == crate::gallery::MediaKind::Clip,
        kind_label: item.kind.label().into(),
        format_label: item.format.to_ascii_uppercase().into(),
        date_label: item.date_label.as_str().into(),
        size_label: item.size_label.as_str().into(),
        duration_label: item.duration_label.as_str().into(),
        thumbnail,
        has_thumbnail,
    }
}

/// Aplica um snapshot do Supervisor aos estados e propriedades da janela Slint.
pub fn apply_snapshot_to_window(window: &AppWindow, snap: &UiSnapshot) {
    window.set_daemon_connected(snap.daemon_connected);
    window.set_encoder_label(snap.encoder_label.as_str().into());
    window.set_error_banner(snap.error_banner.as_str().into());
    window.set_profile_summary_label(snap.profile_summary_label.as_str().into());

    window.set_replay_active(snap.replay_active);
    window.set_replay_progress(snap.replay_progress as i32);
    window.set_replay_buffer_label(snap.replay_buffer_label.as_str().into());
    window.set_replay_ram_label(snap.replay_ram_label.as_str().into());
    let rep_bind_label = format_hotkey_label(&snap.prefs.replay_hotkey);
    window.set_replay_hotkey_label(rep_bind_label.as_str().into());
    window.set_pref_replay_hotkey(rep_bind_label.as_str().into());
    window.set_can_save_clip(snap.can_save_clip);
    window.set_saving_clip(snap.saving_clip);
    window.set_last_saved_label(snap.last_saved_label.as_str().into());

    window.set_recording(snap.recording);
    window.set_record_status_label(snap.record_status_label.as_str().into());
    let rec_bind_label = format_hotkey_label(&snap.prefs.record_hotkey);
    window.set_record_hotkey_label(rec_bind_label.as_str().into());
    window.set_pref_record_hotkey(rec_bind_label.as_str().into());
    window.set_shared_session(snap.shared_session);
    window.set_restart_hint(snap.restart_hint.as_str().into());

    window.set_confirming_delete_path(snap.confirming_delete_path.as_str().into());

    // Mídias recentes da Home (até 3 itens)
    let recent_ui: Vec<UiMediaItem> = snap.recent_items.iter().map(to_ui_media_item).collect();
    window.set_recent_items(Rc::new(VecModel::from(recent_ui)).into());

    // Mídias da Galeria completa com filtros aplicados
    let gallery_ui: Vec<UiMediaItem> = snap.gallery_items.iter().map(to_ui_media_item).collect();
    window.set_gallery_items(Rc::new(VecModel::from(gallery_ui)).into());

    let years: Vec<slint::SharedString> = snap
        .facets
        .years
        .iter()
        .map(|y| y.as_str().into())
        .collect();
    window.set_facet_years(Rc::new(VecModel::from(years)).into());

    // Dispositivos de Áudio
    let out_devices: Vec<UiDeviceOption> = snap
        .audio_outputs
        .iter()
        .map(|d| UiDeviceOption {
            id: d.id.as_str().into(),
            label: d.label.as_str().into(),
        })
        .collect();
    window.set_audio_outputs(Rc::new(VecModel::from(out_devices)).into());

    let in_devices: Vec<UiDeviceOption> = snap
        .audio_inputs
        .iter()
        .map(|d| UiDeviceOption {
            id: d.id.as_str().into(),
            label: d.label.as_str().into(),
        })
        .collect();
    window.set_audio_inputs(Rc::new(VecModel::from(in_devices)).into());

    // Preferências em Ajustes
    window.set_pref_replay_duration_secs(snap.prefs.replay_duration_secs as i32);
    window.set_pref_replay_ram_mb(snap.prefs.replay_max_ram_mb as i32);
    window.set_pref_bitrate_kbps(snap.prefs.bitrate_kbps as i32);
    window.set_pref_fps(snap.prefs.fps as i32);

    let res_str = match snap.prefs.resolution {
        ResolutionPreset::Native => "native",
        ResolutionPreset::P1440 => "1440p",
        ResolutionPreset::P1080 => "1080p",
        ResolutionPreset::P720 => "720p",
    };
    window.set_pref_resolution(res_str.into());
    window.set_pref_format(snap.prefs.format.extension().into());

    let profile_str = match snap.prefs.audio_profile {
        AudioCaptureProfile::GameOnly => "game_only",
        AudioCaptureProfile::SystemAndMic => "system",
    };
    window.set_pref_audio_profile(profile_str.into());
    window.set_pref_audio_game(snap.prefs.audio_game);
    window.set_pref_audio_mic(snap.prefs.audio_mic);
    window.set_pref_audio_call(snap.prefs.audio_call);
    window.set_pref_output_device(snap.prefs.output_device.as_str().into());
    window.set_pref_mic_device(snap.prefs.mic_device.as_str().into());
    window.set_pref_clear_after_save(snap.prefs.clear_after_save);
    window.set_pref_start_minimized(snap.prefs.start_minimized);
    window.set_pref_overlay_enabled(snap.prefs.overlay_enabled);

    // Limites de Hardware da máquina
    if snap.caps.max_replay_secs > 0 {
        window.set_cap_max_replay_secs(snap.caps.max_replay_secs as i32);
    }
    if snap.caps.min_bitrate_kbps > 0 {
        window.set_cap_min_bitrate_kbps(snap.caps.min_bitrate_kbps as i32);
    }
    if snap.caps.max_bitrate_kbps > 0 {
        window.set_cap_max_bitrate_kbps(snap.caps.max_bitrate_kbps as i32);
    }
    if snap.caps.min_fps > 0 {
        window.set_cap_min_fps(snap.caps.min_fps as i32);
    }
    if snap.caps.max_fps > 0 {
        window.set_cap_max_fps(snap.caps.max_fps as i32);
    }
    if snap.caps.max_buffer_ram_mb > 0 {
        window.set_cap_max_ram_mb(snap.caps.max_buffer_ram_mb as i32);
    }
}

/// Extrai as preferências selecionadas na interface do usuário.
pub fn read_preferences_from_window(window: &AppWindow) -> UiPreferences {
    let fmt_str = window.get_pref_format();
    let format = if fmt_str.as_str() == "mkv" {
        ContainerFormat::Mkv
    } else {
        ContainerFormat::Mp4
    };

    let res_str = window.get_pref_resolution();
    let resolution = match res_str.as_str() {
        "1440p" => ResolutionPreset::P1440,
        "1080p" => ResolutionPreset::P1080,
        "720p" => ResolutionPreset::P720,
        _ => ResolutionPreset::Native,
    };

    let profile_str = window.get_pref_audio_profile();
    let audio_profile = match profile_str.as_str() {
        "game_only" => AudioCaptureProfile::GameOnly,
        _ => AudioCaptureProfile::SystemAndMic,
    };

    let (audio_mic, audio_call, mic_device) = match audio_profile {
        AudioCaptureProfile::GameOnly => (false, false, String::new()),
        AudioCaptureProfile::SystemAndMic => (
            window.get_pref_audio_mic(),
            true,
            window.get_pref_mic_device().to_string(),
        ),
    };

    let output_device = window.get_pref_output_device().to_string();

    UiPreferences {
        replay_duration_secs: window.get_pref_replay_duration_secs() as u32,
        replay_max_ram_mb: window.get_pref_replay_ram_mb() as u32,
        format,
        resolution,
        bitrate_kbps: window.get_pref_bitrate_kbps() as u32,
        fps: window.get_pref_fps() as u32,
        audio_profile,
        audio_game: true,
        audio_mic,
        audio_call,
        mic_device,
        output_device,
        clear_after_save: window.get_pref_clear_after_save(),
        start_minimized: window.get_pref_start_minimized(),
        overlay_enabled: window.get_pref_overlay_enabled(),
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
