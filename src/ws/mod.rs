mod connection;
mod rate_limit;
mod session;

use axum::{
    extract::{State, WebSocketUpgrade},
    response::Response,
};
use tracing::warn;

use crate::AppState;

pub async fn upgrade(State(app): State<AppState>, ws: WebSocketUpgrade) -> Response {
    let max = app.hub.config().max_message_bytes;
    ws.max_message_size(max)
        .max_frame_size(max)
        .on_failed_upgrade(|e| warn!("websocket upgrade failed: {e}"))
        .on_upgrade(move |socket| connection::run(socket, app.hub, app.shutdown))
}
