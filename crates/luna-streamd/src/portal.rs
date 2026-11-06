use ashpd::Error as AshpdError;
use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
use ashpd::desktop::{PersistMode, ResponseError, Session};
use std::os::fd::OwnedFd;
use tracing::{error, info, warn};

use crate::token_store::TokenStore;
use luna_stream_proto::{ErrorCode, RpcError};

pub struct PortalSession {
    pub fd: OwnedFd,
    pub node_id: u32,
    session: Option<Session<'static, Screencast<'static>>>,
}

impl Drop for PortalSession {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            tokio::spawn(async move {
                let _ = session.close().await;
            });
        }
    }
}

pub async fn open_screen(store: &TokenStore) -> Result<PortalSession, RpcError> {
    let screencast = Screencast::new().await.map_err(|e| {
        error!("Erro conectando ao portal de Screencast: {e}");
        RpcError::new(
            ErrorCode::Internal as i32,
            format!("portal_init_failed: {e}"),
        )
    })?;

    let session = screencast.create_session().await.map_err(|e| {
        error!("Erro criando sessão no portal de Screencast: {e}");
        RpcError::new(
            ErrorCode::Internal as i32,
            format!("portal_session_failed: {e}"),
        )
    })?;

    let restore_token = store.load();
    if restore_token.is_some() {
        info!("Tentando reutilizar restore token do portal ScreenCast...");
    }

    let select_req = screencast
        .select_sources(
            &session,
            CursorMode::Embedded,
            SourceType::Monitor.into(),
            false,
            restore_token.as_deref(),
            PersistMode::ExplicitlyRevoked,
        )
        .await
        .map_err(map_portal_error)?;

    select_req.response().map_err(map_portal_error)?;

    let start_req = screencast
        .start(&session, None)
        .await
        .map_err(map_portal_error)?;

    let streams_res = start_req.response().map_err(map_portal_error)?;

    if let Some(token) = streams_res.restore_token() {
        info!("Salvando novo restore token do portal...");
        if let Err(e) = store.save(token) {
            warn!("Não foi possível persistir restore token: {e}");
        }
    }

    let stream = streams_res.streams().first().ok_or_else(|| {
        RpcError::new(
            ErrorCode::Internal as i32,
            "Nenhum monitor selecionado no portal",
        )
    })?;

    let node_id = stream.pipe_wire_node_id();
    let fd = screencast
        .open_pipe_wire_remote(&session)
        .await
        .map_err(|e| {
            error!("Erro ao abrir fd do PipeWire via portal: {e}");
            RpcError::new(
                ErrorCode::Internal as i32,
                format!("pipewire_remote_failed: {e}"),
            )
        })?;

    info!("Captura de tela obtida com sucesso via portal (Node ID: {node_id})");

    Ok(PortalSession {
        fd,
        node_id,
        session: Some(session),
    })
}

fn map_portal_error(err: AshpdError) -> RpcError {
    match err {
        AshpdError::Response(ResponseError::Cancelled) => {
            RpcError::from(ErrorCode::PermissionDenied)
        }
        other => {
            error!("Erro retornado pelo portal ScreenCast: {other}");
            RpcError::new(ErrorCode::Internal as i32, format!("portal_error: {other}"))
        }
    }
}
