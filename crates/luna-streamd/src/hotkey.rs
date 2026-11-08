//! Gerenciamento de atalhos globais via portal XDG e notificações do sistema.

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use ashpd::desktop::notification::{Notification, NotificationProxy};
use luna_stream_proto::HotkeysConfigureResult;
use std::collections::HashMap;
use std::path::Path;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    SaveClip,
    ToggleRecord,
}

pub struct HotkeyHandle {
    abort_handle: tokio::task::AbortHandle,
}

impl HotkeyHandle {
    pub fn new(abort_handle: tokio::task::AbortHandle) -> Self {
        Self { abort_handle }
    }

    pub fn close(self) {
        self.abort_handle.abort();
    }
}

pub fn shortcut_id_for_trigger(prefix: &str, trigger: &str) -> String {
    let mut clean = String::with_capacity(trigger.len());
    for ch in trigger.chars() {
        if ch.is_ascii_alphanumeric() {
            clean.push(ch.to_ascii_lowercase());
        } else if ch == '+' || ch == '_' || ch == '-' {
            clean.push('_');
        }
    }
    format!("{prefix}_{clean}")
}

pub async fn bind_global_hotkeys(
    replay_trigger: &str,
    record_trigger: &str,
    action_tx: UnboundedSender<HotkeyAction>,
) -> (Option<HotkeyHandle>, HotkeysConfigureResult) {
    // Tenta registrar app ID no host portal registry quando fora de sandbox
    if !ashpd::is_sandboxed().await {
        if let Ok(connection) = ashpd::zbus::Connection::session().await {
            let _ = connection
                .call_method(
                    Some("org.freedesktop.portal.Desktop"),
                    "/org/freedesktop/portal/desktop",
                    Some("org.freedesktop.host.portal.Registry"),
                    "Register",
                    &(
                        "br.com.lunagg.LunaStream",
                        HashMap::<String, ashpd::zbus::zvariant::Value>::new(),
                    ),
                )
                .await;
        }
    }

    let shortcuts_proxy = match GlobalShortcuts::new().await {
        Ok(p) => p,
        Err(e) => {
            warn!("Portal GlobalShortcuts indisponível: {e}");
            return (
                None,
                HotkeysConfigureResult {
                    replay_bound: false,
                    replay_trigger: replay_trigger.to_string(),
                    record_bound: false,
                    record_trigger: record_trigger.to_string(),
                },
            );
        }
    };

    let session = match shortcuts_proxy.create_session().await {
        Ok(s) => s,
        Err(e) => {
            warn!("Não foi possível criar sessão no GlobalShortcuts: {e}");
            return (
                None,
                HotkeysConfigureResult {
                    replay_bound: false,
                    replay_trigger: replay_trigger.to_string(),
                    record_bound: false,
                    record_trigger: record_trigger.to_string(),
                },
            );
        }
    };

    let replay_id = shortcut_id_for_trigger("save_clip", replay_trigger);
    let record_id = shortcut_id_for_trigger("toggle_record", record_trigger);

    let sc_replay = NewShortcut::new(&replay_id, "Salvar clipe Luna (Replay)")
        .preferred_trigger(replay_trigger);
    let sc_record = NewShortcut::new(&record_id, "Iniciar/Parar gravação Luna")
        .preferred_trigger(record_trigger);

    let bind_res = match shortcuts_proxy
        .bind_shortcuts(&session, &[sc_replay, sc_record], None)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("Falha ao vincular atalhos no GlobalShortcuts: {e}");
            return (
                None,
                HotkeysConfigureResult {
                    replay_bound: false,
                    replay_trigger: replay_trigger.to_string(),
                    record_bound: false,
                    record_trigger: record_trigger.to_string(),
                },
            );
        }
    };

    let mut replay_bound = false;
    let mut replay_actual = replay_trigger.to_string();
    let mut record_bound = false;
    let mut record_actual = record_trigger.to_string();

    if let Ok(response) = bind_res.response() {
        for sc in response.shortcuts() {
            if sc.id() == replay_id {
                replay_bound = true;
                replay_actual = sc.trigger_description().to_string();
            } else if sc.id() == record_id {
                record_bound = true;
                record_actual = sc.trigger_description().to_string();
            }
        }
    }

    let stream = match shortcuts_proxy.receive_activated().await {
        Ok(s) => s,
        Err(e) => {
            warn!("Falha ao escutar ativação de atalhos: {e}");
            return (
                None,
                HotkeysConfigureResult {
                    replay_bound,
                    replay_trigger: replay_actual,
                    record_bound,
                    record_trigger: record_actual,
                },
            );
        }
    };

    let replay_id_cloned = replay_id.clone();
    let record_id_cloned = record_id.clone();
    let handle = tokio::spawn(async move {
        use futures_util::StreamExt;
        let mut stream = stream;
        while let Some(activated) = stream.next().await {
            let id = activated.shortcut_id();
            if id == replay_id_cloned {
                info!("Atalho global de Replay acionado ({id})!");
                let _ = action_tx.send(HotkeyAction::SaveClip);
            } else if id == record_id_cloned {
                info!("Atalho global de Gravação acionado ({id})!");
                let _ = action_tx.send(HotkeyAction::ToggleRecord);
            }
        }
    });

    (
        Some(HotkeyHandle::new(handle.abort_handle())),
        HotkeysConfigureResult {
            replay_bound,
            replay_trigger: replay_actual,
            record_bound,
            record_trigger: record_actual,
        },
    )
}

pub async fn notify_clip_saved(path: &Path) {
    if let Ok(proxy) = NotificationProxy::new().await {
        let file_name = path.file_name().and_then(|f| f.to_str()).unwrap_or("clipe");
        let body = format!("Arquivo salvo em: {file_name}");
        let notif = Notification::new("Clipe salvo com sucesso!").body(body.as_str());
        let _ = proxy
            .add_notification("br.com.lunagg.LunaStream.clip", notif)
            .await;
    }
}

pub async fn notify_recording_state(started: bool, path: &Path) {
    if let Ok(proxy) = NotificationProxy::new().await {
        let (title, body) = if started {
            (
                "Gravação iniciada".to_string(),
                "A captura contínua foi iniciada.".to_string(),
            )
        } else {
            let file_name = path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("gravação");
            (
                "Gravação finalizada".to_string(),
                format!("Arquivo salvo em: {file_name}"),
            )
        };
        let notif = Notification::new(&title).body(body.as_str());
        let _ = proxy
            .add_notification("br.com.lunagg.LunaStream.record", notif)
            .await;
    }
}

pub async fn notify_error(title: &str, message: &str) {
    if let Ok(proxy) = NotificationProxy::new().await {
        let notif = Notification::new(title).body(message);
        let _ = proxy
            .add_notification("br.com.lunagg.LunaStream.error", notif)
            .await;
    }
}
