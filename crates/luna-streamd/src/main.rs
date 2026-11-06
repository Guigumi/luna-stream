use std::env;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use luna_streamd::handlers::Daemon;
use luna_streamd::server::serve;
use luna_streamd::socket::{bind, socket_path};

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
    let shutdown = CancellationToken::new();

    let shutdown_clone = shutdown.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Sinal recebido, encerrando daemon...");
        shutdown_clone.cancel();
    });

    serve(listener, handler, shutdown).await?;
    let _ = std::fs::remove_file(&path);
    info!("luna-streamd finalizado com sucesso.");
    Ok(())
}
