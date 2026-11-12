//! Supervisor em segundo plano para ciclo de vida do motor, polling adaptativo e IPC.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use luna_stream_proto::{
    AudioDeviceEntry, AudioDevicesResult, EncoderListResult, HotkeysConfigureResult,
    MediaInspectResult, RecordStatusResult, ReplayStatusResult, SystemCapabilitiesResult, methods,
    paths,
};
use luna_streamctl::client::{ClientError, call};
#[cfg(unix)]
use nix::sys::signal::{Signal, kill};
#[cfg(unix)]
use nix::unistd::Pid;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::config::UiPreferences;
use crate::gallery::{
    ClipEntry, GalleryFacets, GalleryFilter, GalleryItem, GalleryManager, GallerySortField,
    SortOrder, format_size_mb,
};
use crate::keybind::{format_hotkey_label, validate_hotkey_pair};
use crate::overlay::{OverlayEvent, OverlayKind};
use crate::tray::{TrayController, TrayState};

#[derive(Debug, Clone)]
pub enum UiCommand {
    ShowWindow,
    WindowVisibilityChanged(bool),
    ToggleReplay,
    SaveClip,
    ToggleRecord,
    UpdatePreferences(UiPreferences),
    CaptureHotkey {
        for_record: bool,
        trigger: String,
    },
    ResetDefaultHotkeys,
    RefreshAudioDevices,
    RefreshGallery,
    UpdateGalleryFilter(GalleryFilter),
    UpdateGallerySort {
        field: GallerySortField,
        order: SortOrder,
    },
    RequestDeleteMedia(PathBuf),
    CancelDeleteMedia,
    ConfirmDeleteMedia(PathBuf),
    OpenMedia(PathBuf),
    OpenCapturesDir,
    DismissError,
    Quit,

    // Retrocompatibilidade
    OpenClip(PathBuf),
    OpenClipsDir,
    RefreshClips,
    DeleteClip(PathBuf),
    UndoDeleteClip,
}

/// Limite de itens recentes exibidos na aba Home.
pub const RECENT_ITEMS_CAPACITY: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UiSnapshot {
    pub daemon_connected: bool,
    pub encoder_label: String,
    pub replay_active: bool,
    pub replay_progress: u32,
    pub replay_buffer_label: String,
    pub replay_ram_label: String,
    pub hotkey_label: String,
    pub can_save_clip: bool,
    pub saving_clip: bool,
    pub last_saved_label: String,
    pub recording: bool,
    pub record_status_label: String,
    pub shared_session: bool,
    pub restart_hint: String,
    pub error_banner: String,
    pub clips: Vec<ClipEntry>,
    pub undo_banner_visible: bool,
    pub prefs: UiPreferences,

    // v2
    pub caps: SystemCapabilitiesResult,
    pub audio_inputs: Vec<AudioDeviceEntry>,
    pub audio_outputs: Vec<AudioDeviceEntry>,
    pub gallery_items: Vec<GalleryItem>,
    pub recent_items: Vec<GalleryItem>,
    pub facets: GalleryFacets,
    pub filter_date: String,
    pub sort_field: GallerySortField,
    pub sort_order: SortOrder,
    pub confirming_delete_path: String,
    pub profile_summary_label: String,
}

/// Traduz erros de transporte e códigos RPC do motor para mensagens claras em pt-BR.
pub fn format_rpc_error_pt_br(err: &ClientError) -> String {
    match err {
        ClientError::NotRunning => {
            "Motor luna-streamd não encontrado. Verifique a instalação.".to_string()
        }
        ClientError::Rpc(1004, _) => {
            "Sem espaço suficiente ou permissão de escrita na pasta Vídeos/Luna.".to_string()
        }
        ClientError::Rpc(1005, _) => "Compartilhamento de tela cancelado.".to_string(),
        ClientError::Rpc(1006, _) => {
            "Nenhum codificador H.264 compatível foi encontrado no GStreamer.".to_string()
        }
        ClientError::Rpc(1007, _) => "Falha no pipeline de captura.".to_string(),
        ClientError::Rpc(1010, _) => "Aguarde alguns segundos para preencher o buffer.".to_string(),
        ClientError::Rpc(code, msg) => format!("Erro ({code}): {msg}"),
        ClientError::Io(e) => format!("Erro de comunicação: {e}"),
        ClientError::Json(e) => format!("Erro de protocolo: {e}"),
    }
}

/// Localiza o binário `luna-streamd` no mesmo diretório do executável atual ou no PATH.
pub fn resolve_daemon_binary(current_exe: Option<&Path>) -> Option<PathBuf> {
    if let Some(exe) = current_exe
        && let Some(parent) = exe.parent()
    {
        let candidate = parent.join("luna-streamd");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    if let Some(path_var) = env::var_os("PATH") {
        for dir in env::split_paths(&path_var) {
            let candidate = dir.join("luna-streamd");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

pub struct SupervisorConfig {
    pub socket_path: PathBuf,
    pub config_path: PathBuf,
    pub clips_dir: PathBuf,
    pub daemon_bin_override: Option<PathBuf>,
    pub auto_spawn: bool,
}

pub struct SupervisorCallbacks {
    pub on_snapshot: Box<dyn FnMut(UiSnapshot) + Send>,
    pub on_show_window: Box<dyn FnMut() + Send>,
    pub on_quit: Box<dyn FnMut() + Send>,
    pub on_overlay: Box<dyn FnMut(OverlayEvent) + Send>,
}

fn format_profile_summary(prefs: &UiPreferences) -> String {
    format!(
        "{} · {} fps · {} Kbps",
        prefs.resolution.label(),
        prefs.fps,
        prefs.bitrate_kbps
    )
}

pub async fn run_supervisor(
    cfg: SupervisorConfig,
    mut cmd_rx: mpsc::UnboundedReceiver<UiCommand>,
    tray: Option<TrayController>,
    mut callbacks: SupervisorCallbacks,
) {
    let mut prefs = UiPreferences::load_from(&cfg.config_path);

    let recordings_dir = cfg
        .clips_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| cfg.clips_dir.clone());
    let cache_dir = paths::default_cache_dir().join("gallery");
    let mut gallery_mgr = GalleryManager::new(recordings_dir, cfg.clips_dir.clone(), cache_dir);
    let _ = gallery_mgr.refresh();

    let mut snapshot = UiSnapshot {
        daemon_connected: false,
        encoder_label: "Detectando encoder...".to_string(),
        replay_active: false,
        replay_progress: 0,
        replay_buffer_label: "0s / 30s".to_string(),
        replay_ram_label: "0 MB / 512 MB".to_string(),
        hotkey_label: format_hotkey_label(&prefs.replay_hotkey),
        can_save_clip: false,
        saving_clip: false,
        last_saved_label: String::new(),
        recording: false,
        record_status_label: String::new(),
        shared_session: false,
        restart_hint: String::new(),
        error_banner: String::new(),
        clips: Vec::new(),
        undo_banner_visible: false,
        prefs: prefs.clone(),
        caps: SystemCapabilitiesResult::default(),
        audio_inputs: Vec::new(),
        audio_outputs: Vec::new(),
        gallery_items: gallery_mgr.visible_items(),
        recent_items: gallery_mgr.recent_items(RECENT_ITEMS_CAPACITY),
        facets: gallery_mgr.facets(),
        filter_date: String::new(),
        sort_field: GallerySortField::Recent,
        sort_order: SortOrder::Desc,
        confirming_delete_path: String::new(),
        profile_summary_label: format_profile_summary(&prefs),
    };

    let mut window_visible = true;
    let mut spawned_child: Option<Child> = None;
    let current_exe = env::current_exe().ok();

    // Emite o snapshot inicial
    (callbacks.on_snapshot)(snapshot.clone());

    let mut poll_timer = tokio::time::interval(Duration::from_millis(1000));
    poll_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else { break; };
                match cmd {
                    UiCommand::ShowWindow => {
                        window_visible = true;
                        poll_timer = tokio::time::interval(Duration::from_millis(1000));
                        poll_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                        let _ = gallery_mgr.refresh();
                        snapshot.gallery_items = gallery_mgr.visible_items();
                        snapshot.recent_items = gallery_mgr.recent_items(RECENT_ITEMS_CAPACITY);
                        snapshot.facets = gallery_mgr.facets();
                        snapshot.confirming_delete_path.clear();
                        (callbacks.on_show_window)();
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::WindowVisibilityChanged(v) => {
                        window_visible = v;
                        let interval_ms = if window_visible { 1000 } else { 3000 };
                        poll_timer = tokio::time::interval(Duration::from_millis(interval_ms));
                        poll_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                        if window_visible {
                            let _ = gallery_mgr.refresh();
                            snapshot.gallery_items = gallery_mgr.visible_items();
                            snapshot.recent_items = gallery_mgr.recent_items(RECENT_ITEMS_CAPACITY);
                            snapshot.facets = gallery_mgr.facets();
                            snapshot.confirming_delete_path.clear();
                            (callbacks.on_snapshot)(snapshot.clone());
                        }
                    }
                    UiCommand::ToggleReplay => {
                        let mut overlay_event: Option<OverlayEvent> = None;
                        if snapshot.replay_active {
                            match call(&cfg.socket_path, methods::REPLAY_STOP, serde_json::Value::Null).await {
                                Ok(_) => {
                                    snapshot.replay_active = false;
                                    snapshot.can_save_clip = false;
                                    snapshot.error_banner.clear();
                                    overlay_event = Some(OverlayEvent {
                                        kind: OverlayKind::ReplayStopped,
                                        title: "Replay desativado".into(),
                                        subtitle: "Buffer encerrado".into(),
                                    });
                                }
                                Err(e) => {
                                    snapshot.error_banner = format_rpc_error_pt_br(&e);
                                }
                            }
                        } else {
                            let params = serde_json::to_value(prefs.to_replay_start_params()).unwrap_or_default();
                            match call(&cfg.socket_path, methods::REPLAY_START, params).await {
                                Ok(res) => {
                                    snapshot.replay_active = true;
                                    snapshot.error_banner.clear();
                                    snapshot.restart_hint.clear();
                                    if let Some(trigger) = res.get("hotkey_trigger").and_then(|v| v.as_str()) {
                                        snapshot.hotkey_label = format_hotkey_label(trigger);
                                    }
                                    overlay_event = Some(OverlayEvent {
                                        kind: OverlayKind::ReplayStarted,
                                        title: "Replay ativado".into(),
                                        subtitle: format!("Buffer de {}s ativo", prefs.replay_duration_secs),
                                    });
                                }
                                Err(e) => {
                                    snapshot.replay_active = false;
                                    snapshot.error_banner = format_rpc_error_pt_br(&e);
                                }
                            }
                        }
                        if let Some(event) = overlay_event {
                            (callbacks.on_overlay)(event);
                        }
                        sync_tray(&tray, &snapshot, &prefs).await;
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::SaveClip => {
                        let mut overlay_event: Option<OverlayEvent> = None;
                        if snapshot.replay_active && !snapshot.saving_clip {
                            snapshot.saving_clip = true;
                            (callbacks.on_snapshot)(snapshot.clone());

                            let params = serde_json::to_value(prefs.to_replay_save_params()).unwrap_or_default();
                            match call(&cfg.socket_path, methods::REPLAY_SAVE, params).await {
                                Ok(res) => {
                                    if let Some(p) = res.get("path").and_then(|v| v.as_str()) {
                                        let file_name = Path::new(p).file_name().and_then(|n| n.to_str()).unwrap_or("clipe");
                                        snapshot.last_saved_label = format!("Salvo: {file_name}");
                                        overlay_event = Some(OverlayEvent {
                                            kind: OverlayKind::ClipSaved,
                                            title: "Clipe salvo".into(),
                                            subtitle: file_name.into(),
                                        });
                                    }
                                    let _ = gallery_mgr.refresh();
                                    snapshot.gallery_items = gallery_mgr.visible_items();
                                    snapshot.recent_items = gallery_mgr.recent_items(RECENT_ITEMS_CAPACITY);
                                    snapshot.facets = gallery_mgr.facets();
                                    snapshot.error_banner.clear();
                                }
                                Err(e) => {
                                    snapshot.error_banner = format_rpc_error_pt_br(&e);
                                }
                            }
                            snapshot.saving_clip = false;
                            (callbacks.on_snapshot)(snapshot.clone());
                        }
                        if let Some(event) = overlay_event {
                            (callbacks.on_overlay)(event);
                        }
                    }
                    UiCommand::ToggleRecord => {
                        let mut overlay_event: Option<OverlayEvent> = None;
                        if snapshot.recording {
                            match call(&cfg.socket_path, methods::RECORD_STOP, serde_json::Value::Null).await {
                                Ok(res) => {
                                    snapshot.recording = false;
                                    snapshot.error_banner.clear();
                                    if let Some(p) = res.get("path").and_then(|v| v.as_str()) {
                                        let file_name = Path::new(p).file_name().and_then(|n| n.to_str()).unwrap_or("gravação");
                                        overlay_event = Some(OverlayEvent {
                                            kind: OverlayKind::RecordStopped,
                                            title: "Gravação salva".into(),
                                            subtitle: file_name.into(),
                                        });
                                    }
                                }
                                Err(e) => {
                                    snapshot.error_banner = format_rpc_error_pt_br(&e);
                                }
                            }
                        } else {
                            let params = serde_json::to_value(prefs.to_record_start_params()).unwrap_or_default();
                            match call(&cfg.socket_path, methods::RECORD_START, params).await {
                                Ok(_) => {
                                    snapshot.recording = true;
                                    snapshot.error_banner.clear();
                                    overlay_event = Some(OverlayEvent {
                                        kind: OverlayKind::RecordStarted,
                                        title: "Gravação iniciada".into(),
                                        subtitle: "Capturando vídeo e áudio".into(),
                                    });
                                }
                                Err(e) => {
                                    snapshot.recording = false;
                                    snapshot.error_banner = format_rpc_error_pt_br(&e);
                                }
                            }
                        }
                        if let Some(event) = overlay_event {
                            (callbacks.on_overlay)(event);
                        }
                        sync_tray(&tray, &snapshot, &prefs).await;
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::UpdatePreferences(mut new_prefs) => {
                        if snapshot.replay_active && prefs.requires_session_restart(&new_prefs) {
                            snapshot.restart_hint = "Reinicie o Replay para aplicar na sessão atual".to_string();
                        } else {
                            snapshot.restart_hint.clear();
                        }
                        // Preserva os atalhos vigentes para não sobrescrever customizações feitas via CaptureHotkey
                        new_prefs.replay_hotkey = prefs.replay_hotkey.clone();
                        new_prefs.record_hotkey = prefs.record_hotkey.clone();
                        prefs = new_prefs;
                        // Salva preferências persistidas no disco (incluindo overlay_enabled)
                        let _ = prefs.save_to(&cfg.config_path);
                        snapshot.prefs = prefs.clone();
                        snapshot.profile_summary_label = format_profile_summary(&prefs);
                        let hk_val = serde_json::to_value(prefs.to_hotkeys_configure_params()).unwrap_or_default();
                        let _ = call(&cfg.socket_path, methods::HOTKEYS_CONFIGURE, hk_val).await;
                        sync_tray(&tray, &snapshot, &prefs).await;
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::CaptureHotkey { for_record, trigger } => {
                        let other = if for_record {
                            &prefs.replay_hotkey
                        } else {
                            &prefs.record_hotkey
                        };
                        match validate_hotkey_pair(&trigger, other) {
                            Ok(valid) => {
                                if for_record {
                                    prefs.record_hotkey = valid;
                                } else {
                                    prefs.replay_hotkey = valid;
                                }
                                let _ = prefs.save_to(&cfg.config_path);
                                snapshot.prefs = prefs.clone();
                                let hk_val = serde_json::to_value(prefs.to_hotkeys_configure_params()).unwrap_or_default();
                                let _ = call(&cfg.socket_path, methods::HOTKEYS_CONFIGURE, hk_val).await;
                                snapshot.hotkey_label = format_hotkey_label(&prefs.replay_hotkey);
                                snapshot.error_banner.clear();
                                sync_tray(&tray, &snapshot, &prefs).await;
                            }
                            Err(err_msg) => {
                                snapshot.error_banner = err_msg.to_string();
                            }
                        }
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::ResetDefaultHotkeys => {
                        prefs.replay_hotkey = crate::keybind::DEFAULT_REPLAY_HOTKEY.to_string();
                        prefs.record_hotkey = crate::keybind::DEFAULT_RECORD_HOTKEY.to_string();
                        let _ = prefs.save_to(&cfg.config_path);
                        snapshot.prefs = prefs.clone();
                        let hk_val = serde_json::to_value(prefs.to_hotkeys_configure_params()).unwrap_or_default();
                        let _ = call(&cfg.socket_path, methods::HOTKEYS_CONFIGURE, hk_val).await;
                        snapshot.hotkey_label = format_hotkey_label(&prefs.replay_hotkey);
                        snapshot.error_banner.clear();
                        sync_tray(&tray, &snapshot, &prefs).await;
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::RefreshAudioDevices => {
                        if let Ok(res) = call(&cfg.socket_path, methods::AUDIO_DEVICES, serde_json::Value::Null).await
                            && let Ok(devs) = serde_json::from_value::<AudioDevicesResult>(res)
                        {
                            snapshot.audio_inputs = devs.inputs;
                            snapshot.audio_outputs = devs.outputs;
                        }
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::RefreshGallery | UiCommand::RefreshClips => {
                        let _ = gallery_mgr.refresh();
                        snapshot.gallery_items = gallery_mgr.visible_items();
                        snapshot.recent_items = gallery_mgr.recent_items(RECENT_ITEMS_CAPACITY);
                        snapshot.facets = gallery_mgr.facets();
                        snapshot.confirming_delete_path.clear();
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::UpdateGalleryFilter(f) => {
                        gallery_mgr.set_filter(f.clone());
                        snapshot.filter_date = f.date.unwrap_or_default();
                        snapshot.gallery_items = gallery_mgr.visible_items();
                        snapshot.confirming_delete_path.clear();
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::UpdateGallerySort { field, order } => {
                        gallery_mgr.set_sort(field, order);
                        snapshot.sort_field = field;
                        snapshot.sort_order = order;
                        snapshot.gallery_items = gallery_mgr.visible_items();
                        snapshot.confirming_delete_path.clear();
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::RequestDeleteMedia(path) | UiCommand::DeleteClip(path) => {
                        gallery_mgr.request_delete(&path);
                        snapshot.confirming_delete_path = path.to_string_lossy().to_string();
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::CancelDeleteMedia => {
                        gallery_mgr.cancel_delete();
                        snapshot.confirming_delete_path.clear();
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::ConfirmDeleteMedia(path) => {
                        if let Ok(deleted) = gallery_mgr.confirm_delete(&path)
                            && deleted
                        {
                            snapshot.confirming_delete_path.clear();
                            snapshot.gallery_items = gallery_mgr.visible_items();
                            snapshot.recent_items = gallery_mgr.recent_items(RECENT_ITEMS_CAPACITY);
                            snapshot.facets = gallery_mgr.facets();
                        }
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::UndoDeleteClip => {
                        // Não utilizado no modo confirmação dupla inline
                    }
                    UiCommand::OpenMedia(path) | UiCommand::OpenClip(path) => {
                        #[cfg(unix)]
                        {
                            let _ = Command::new("xdg-open").arg(path).spawn();
                        }
                    }
                    UiCommand::OpenCapturesDir | UiCommand::OpenClipsDir => {
                        let target = cfg.clips_dir.parent().unwrap_or(&cfg.clips_dir);
                        #[cfg(unix)]
                        {
                            let _ = Command::new("xdg-open").arg(target).spawn();
                        }
                    }
                    UiCommand::DismissError => {
                        snapshot.error_banner.clear();
                        (callbacks.on_snapshot)(snapshot.clone());
                    }
                    UiCommand::Quit => {
                        if snapshot.recording {
                            let _ = tokio::time::timeout(
                                Duration::from_secs(3),
                                call(&cfg.socket_path, methods::RECORD_STOP, serde_json::Value::Null)
                            ).await;
                        }
                        if snapshot.replay_active {
                            let _ = tokio::time::timeout(
                                Duration::from_secs(2),
                                call(&cfg.socket_path, methods::REPLAY_STOP, serde_json::Value::Null)
                            ).await;
                        }
                        if let Some(mut child) = spawned_child.take() {
                            if let Some(pid) = child.id() {
                                #[cfg(unix)]
                                {
                                    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                                }
                            }
                            let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
                        }
                        (callbacks.on_quit)();
                        break;
                    }
                }
            }
            _ = poll_timer.tick() => {
                if let Some(child) = spawned_child.as_mut()
                    && let Ok(Some(_)) = child.try_wait()
                {
                    spawned_child = None;
                }

                // Conexão e inicialização com o daemon
                if !snapshot.daemon_connected {
                    match call(&cfg.socket_path, methods::PING, serde_json::Value::Null).await {
                        Ok(_) => {
                            snapshot.daemon_connected = true;

                            if let Ok(res) = call(&cfg.socket_path, methods::ENCODER_LIST, serde_json::Value::Null).await
                                && let Ok(info) = serde_json::from_value::<EncoderListResult>(res)
                                && let Some(selected) = info.selected
                            {
                                let kind = info.encoders.iter().find(|e| e.element == selected)
                                    .map(|e| match e.kind {
                                        luna_stream_proto::EncoderKind::Hardware => "GPU",
                                        luna_stream_proto::EncoderKind::Software => "Software",
                                    }).unwrap_or("GPU");
                                snapshot.encoder_label = format!("{selected} · {kind}");
                            }

                            if let Ok(res) = call(&cfg.socket_path, methods::SYSTEM_CAPABILITIES, serde_json::Value::Null).await
                                && let Ok(c) = serde_json::from_value::<SystemCapabilitiesResult>(res)
                            {
                                prefs = prefs.sanitize_with_caps(&c);
                                snapshot.caps = c;
                                snapshot.prefs = prefs.clone();
                                snapshot.profile_summary_label = format_profile_summary(&prefs);
                            }

                            if let Ok(res) = call(&cfg.socket_path, methods::AUDIO_DEVICES, serde_json::Value::Null).await
                                && let Ok(devs) = serde_json::from_value::<AudioDevicesResult>(res)
                            {
                                snapshot.audio_inputs = devs.inputs;
                                snapshot.audio_outputs = devs.outputs;
                            }

                            let hk_val = serde_json::to_value(prefs.to_hotkeys_configure_params()).unwrap_or_default();
                            if let Ok(res) = call(&cfg.socket_path, methods::HOTKEYS_CONFIGURE, hk_val).await
                                && let Ok(hk_res) = serde_json::from_value::<HotkeysConfigureResult>(res)
                            {
                                snapshot.hotkey_label = format_hotkey_label(&hk_res.replay_trigger);
                            }
                        }
                        Err(ClientError::NotRunning) => {
                            if cfg.auto_spawn && spawned_child.is_none() {
                                let bin = cfg.daemon_bin_override.clone()
                                    .or_else(|| resolve_daemon_binary(current_exe.as_deref()));
                                if let Some(bin_path) = bin {
                                    match Command::new(&bin_path)
                                        .stdin(Stdio::null())
                                        .stdout(Stdio::null())
                                        .stderr(Stdio::null())
                                        .spawn()
                                    {
                                        Ok(c) => spawned_child = Some(c),
                                        Err(e) => {
                                            tracing::warn!("Falha ao auto-spawnar luna-streamd em {}: {e}", bin_path.display());
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => {}
                    }
                }

                if snapshot.daemon_connected {
                    // Consulta replay.status
                    match call(&cfg.socket_path, methods::REPLAY_STATUS, serde_json::Value::Null).await {
                        Ok(res) => {
                            if let Ok(st) = serde_json::from_value::<ReplayStatusResult>(res) {
                                snapshot.replay_active = st.active;
                                snapshot.can_save_clip = st.active && st.buffered_ms > 0 && !snapshot.saving_clip;
                                if st.active {
                                    let target_ms = (st.duration_secs * 1000).max(1) as u64;
                                    snapshot.replay_progress = ((st.buffered_ms * 100) / target_ms).min(100) as u32;
                                    let secs = st.buffered_ms / 1000;
                                    snapshot.replay_buffer_label = format!("{secs}s / {}s", st.duration_secs);
                                    let ram_formatted = format_size_mb(st.ram_bytes);
                                    snapshot.replay_ram_label = format!("{ram_formatted} / {} MB", st.max_ram_mb);
                                    snapshot.hotkey_label = if st.hotkey_bound {
                                        format_hotkey_label(&prefs.replay_hotkey)
                                    } else {
                                        "Atalho indisponível".to_string()
                                    };
                                }
                                if let Some(err) = st.last_error {
                                    snapshot.error_banner = err;
                                }
                            }
                        }
                        Err(ClientError::NotRunning) => {
                            spawned_child = None;
                            snapshot.daemon_connected = false;
                            snapshot.replay_active = false;
                            snapshot.recording = false;
                            snapshot.can_save_clip = false;
                            snapshot.error_banner = "O motor reiniciou inesperadamente. Reative a captura se desejar.".to_string();
                        }
                        Err(_) => {}
                    }

                    // Consulta record.status
                    match call(&cfg.socket_path, methods::RECORD_STATUS, serde_json::Value::Null).await {
                        Ok(res) => {
                            if let Ok(st) = serde_json::from_value::<RecordStatusResult>(res) {
                                snapshot.recording = st.recording;
                                if st.recording {
                                    let total_secs = st.duration_ms / 1000;
                                    let h = total_secs / 3600;
                                    let m = (total_secs % 3600) / 60;
                                    let s = total_secs % 60;
                                    let size_str = format_size_mb(st.bytes_written);
                                    snapshot.record_status_label = format!("{h:02}:{m:02}:{s:02} · {size_str}");
                                } else {
                                    snapshot.record_status_label.clear();
                                }
                                if let Some(err) = st.last_error {
                                    snapshot.error_banner = err;
                                }
                            }
                        }
                        Err(ClientError::NotRunning) => {
                            spawned_child = None;
                            snapshot.daemon_connected = false;
                            snapshot.replay_active = false;
                            snapshot.recording = false;
                        }
                        Err(_) => {}
                    }

                    snapshot.shared_session = snapshot.replay_active && snapshot.recording;

                    // Fila de extração sequencial de thumbnails e duração (1 por tick)
                    if window_visible
                        && let Some((video_path, thumb_target)) = gallery_mgr.next_unprobed()
                    {
                        let params = serde_json::json!({
                            "path": video_path,
                            "thumb_output": thumb_target,
                        });
                        match call(&cfg.socket_path, methods::MEDIA_INSPECT, params).await {
                            Ok(res) => {
                                if let Ok(insp) = serde_json::from_value::<MediaInspectResult>(res) {
                                    gallery_mgr.record_probe_result(&video_path, insp.duration_ms, insp.thumb_written);
                                } else {
                                    gallery_mgr.record_probe_result(&video_path, 0, false);
                                }
                            }
                            Err(_) => {
                                gallery_mgr.record_probe_result(&video_path, 0, false);
                            }
                        }
                        snapshot.gallery_items = gallery_mgr.visible_items();
                        snapshot.recent_items = gallery_mgr.recent_items(RECENT_ITEMS_CAPACITY);
                        snapshot.facets = gallery_mgr.facets();
                    }

                    sync_tray(&tray, &snapshot, &prefs).await;
                }

                if window_visible {
                    (callbacks.on_snapshot)(snapshot.clone());
                }
            }
        }
    }
}

async fn sync_tray(tray: &Option<TrayController>, snapshot: &UiSnapshot, prefs: &UiPreferences) {
    if let Some(t) = tray {
        t.update(TrayState {
            replay_active: snapshot.replay_active,
            can_save_clip: snapshot.can_save_clip,
            recording: snapshot.recording,
            replay_hotkey_label: format_hotkey_label(&prefs.replay_hotkey),
            record_hotkey_label: format_hotkey_label(&prefs.record_hotkey),
        })
        .await;
    }
}
