//! Sketch: a real-time collaborative whiteboard server.
//!
//! See `ARCHITECTURE.md` for how the pieces fit together.

pub mod config;
pub mod error;
pub mod http;
pub mod hub;
pub mod janitor;
pub mod protocol;
pub mod ws;

use std::sync::Arc;

use axum::{
    Router,
    http::HeaderValue,
    routing::{any, get},
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::info;

use crate::{config::Config, error::AppError, hub::Hub};

/// Shared handle given to every request handler. Cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub hub: Arc<Hub>,
    /// Cancelled when the server shuts down.
    pub shutdown: CancellationToken,
}

pub fn router(state: AppState) -> Router {
    let config = state.hub.config();
    let public = &config.public_dir;
    let cors = if config.cors_origins.is_empty() {
        CorsLayer::permissive()
    } else {
        let origins = config
            .cors_origins
            .iter()
            .filter_map(|o| HeaderValue::from_str(o).ok());
        CorsLayer::new().allow_origin(AllowOrigin::list(origins))
    };

    Router::new()
        .route("/ws", any(ws::upgrade))
        .route("/rooms", get(http::rooms))
        .route("/count", get(http::count))
        .route("/health", get(http::health))
        .fallback_service(
            ServeDir::new(public).not_found_service(ServeFile::new(public.join("index.html"))),
        )
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Serves until `shutdown` is cancelled, then stops accepting, closes every
/// WebSocket and waits for background tasks.
pub async fn serve(
    listener: TcpListener,
    config: Arc<Config>,
    shutdown: CancellationToken,
) -> Result<(), AppError> {
    let hub = Arc::new(Hub::new(config));
    let janitor = tokio::spawn(janitor::run(Arc::clone(&hub), shutdown.clone()));
    let app = router(AppState {
        hub,
        shutdown: shutdown.clone(),
    });

    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown.clone().cancelled_owned())
        .await;

    // Also covers the case where the server stopped on its own.
    shutdown.cancel();
    let _ = janitor.await;
    info!("server stopped");
    result.map_err(AppError::Io)
}
