//! Stub de atalhos globais para Windows.
//! O Windows usa um mecanismo diferente (Win32 RegisterHotKey) que será
//! implementado no marco W2. Por enquanto, este stub retorna "não implementado".

use luna_stream_proto::HotkeysConfigureResult;
use std::path::Path;
use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    SaveClip,
    ToggleRecord,
}

pub struct HotkeyHandle;

impl HotkeyHandle {
    pub fn new() -> Self {
        Self
    }

    pub fn close(self) {}
}

pub fn shortcut_id_for_trigger(_prefix: &str, _trigger: &str) -> String {
    String::new()
}

pub async fn bind_global_hotkeys(
    _replay_trigger: &str,
    _record_trigger: &str,
    _action_tx: UnboundedSender<HotkeyAction>,
) -> (Option<HotkeyHandle>, HotkeysConfigureResult) {
    warn!("Atalhos globais não implementados no Windows ainda (marco W2).");
    (
        None,
        HotkeysConfigureResult {
            replay_bound: false,
            replay_trigger: String::new(),
            record_bound: false,
            record_trigger: String::new(),
        },
    )
}

pub async fn dispatch_overlay_to_socket(_sock_path: &Path, _json: &str) {
    // No Windows, o IPC usa Named Pipe/TCP (marco W1/W2). Stub vazio por enquanto.
}

pub async fn dispatch_overlay_notification(_json: &str) {
    // Stub: notificações via socket ainda não implementadas no Windows.
}

pub async fn notify_clip_saved(_path: &Path) {
    // Stub.
}

pub async fn notify_recording_state(_started: bool, _path: &Path) {
    // Stub.
}

pub async fn notify_error(_title: &str, _message: &str) {
    // Stub.
}
