use std::env;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use luna_streamd::handlers::Daemon;
use luna_streamd::server::serve;
use luna_streamd::socket::{bind, socket_path};

#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("LUNA_STREAM_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let runtime_dir = env::var_os("XDG_RUNTIME_DIR");
    let path = match socket_path(runtime_dir.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            error!("{e}");
            std::process::exit(1);
        }
    };

    info!("Iniciando luna-streamd em {}", path.display());
    let listener = match bind(&path) {
        Ok(l) => l,
        Err(e) => {
            error!("{e}");
            std::process::exit(1);
        }
    };

    let handler = Arc::new(Daemon::new());
    let handler_dyn: Arc<dyn luna_streamd::handlers::Handler> =
        Arc::clone(&handler) as Arc<dyn luna_streamd::handlers::Handler>;
    let shutdown = CancellationToken::new();

    let shutdown_clone = shutdown.clone();
    #[cfg(unix)]
    {
        tokio::spawn(async move {
            let mut sigterm =
                signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");
            let mut sigint =
                signal(SignalKind::interrupt()).expect("failed to register SIGINT handler");

            tokio::select! {
                _ = sigterm.recv() => {
                    info!("Sinal SIGTERM recebido, iniciando encerramento gracioso...");
                }
                _ = sigint.recv() => {
                    info!("Sinal SIGINT (Ctrl+C) recebido, iniciando encerramento gracioso...");
                }
            }
            shutdown_clone.cancel();
        });
    }
    #[cfg(windows)]
    {
        let _ = shutdown_clone;
        warn!("Sinais Unix não disponíveis no Windows. Use Ctrl+C para encerrar.");
    }

    serve(listener, handler_dyn, shutdown).await?;

    if let Some(res) = handler.shutdown().await {
        info!(
            "Gravação ativa finalizada com sucesso no encerramento: {} ({}ms)",
            res.path.display(),
            res.duration_ms
        );
    }

    let _ = std::fs::remove_file(&path);
    info!("luna-streamd finalizado com sucesso.");
    Ok(())
}
