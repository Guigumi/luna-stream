//! Gerenciamento de atalhos globais via portal XDG e notificações do sistema.

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use ashpd::desktop::notification::{Notification, NotificationProxy};
use std::collections::HashMap;
use std::path::Path;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{info, warn};

pub struct HotkeyHandle {
    abort_handle: tokio::task::AbortHandle,
}

impl HotkeyHandle {
    pub async fn bind(trigger_tx: UnboundedSender<()>) -> Option<(Self, Option<String>)> {
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
                return None;
            }
        };

        let session = match shortcuts_proxy.create_session().await {
            Ok(s) => s,
            Err(e) => {
                warn!("Não foi possível criar sessão no GlobalShortcuts: {e}");
                return None;
            }
        };

        let shortcut =
            NewShortcut::new("save_clip", "Salvar clipe Luna").preferred_trigger("ALT+F10");
        let bind_res = match shortcuts_proxy
            .bind_shortcuts(&session, &[shortcut], None)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!("Falha ao vincular atalho GlobalShortcuts: {e}");
                return None;
            }
        };

        let trigger_desc = bind_res.response().ok().and_then(|r| {
            r.shortcuts()
                .first()
                .map(|s| s.trigger_description().to_string())
        });

        let stream = match shortcuts_proxy.receive_activated().await {
            Ok(s) => s,
            Err(e) => {
                warn!("Falha ao escutar ativação de atalhos: {e}");
                return None;
            }
        };

        let handle = tokio::spawn(async move {
            use futures_util::StreamExt;
            let mut stream = stream;
            while let Some(activated) = stream.next().await {
                if activated.shortcut_id() == "save_clip" {
                    info!("Atalho global save_clip acionado!");
                    let _ = trigger_tx.send(());
                }
            }
        });

        Some((
            Self {
                abort_handle: handle.abort_handle(),
            },
            trigger_desc,
        ))
    }

    pub fn close(self) {
        self.abort_handle.abort();
    }
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

pub async fn notify_error(title: &str, message: &str) {
    if let Ok(proxy) = NotificationProxy::new().await {
        let notif = Notification::new(title).body(message);
        let _ = proxy
            .add_notification("br.com.lunagg.LunaStream.error", notif)
            .await;
    }
}
