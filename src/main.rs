use std::{process::ExitCode, sync::Arc};

use sketch::{config::Config, error::AppError};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("sketch=info")),
        )
        .init();

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), AppError> {
    let config = Arc::new(Config::from_env()?);

    let mut runtime = tokio::runtime::Builder::new_multi_thread();
    runtime.enable_all().thread_name("sketch-worker");
    if let Some(threads) = config.worker_threads {
        runtime.worker_threads(threads);
    }
    let runtime = runtime.build()?;

    runtime.block_on(async {
        let listener = TcpListener::bind(config.addr)
            .await
            .map_err(|source| AppError::Bind {
                addr: config.addr.to_string(),
                source,
            })?;
        info!(
            addr = %config.addr,
            public_dir = %config.public_dir.display(),
            "listening on http://{}",
            config.addr
        );

        let shutdown = CancellationToken::new();
        tokio::spawn(cancel_on_signal(shutdown.clone()));
        sketch::serve(listener, config, shutdown).await
    })
}

async fn cancel_on_signal(shutdown: CancellationToken) {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            warn!("failed to listen for ctrl-c: {e}");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                term.recv().await;
            }
            Err(e) => {
                warn!("failed to listen for SIGTERM: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    info!("shutdown signal received");
    shutdown.cancel();
}
