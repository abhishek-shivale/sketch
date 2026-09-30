use axum::{Json, extract::State};
use serde::Serialize;

use crate::{AppState, hub::RoomSummary};

pub async fn health() -> &'static str {
    "ok"
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    active_users: usize,
    active_rooms: usize,
}

/// Polled by the landing page for its live stats.
pub async fn count(State(app): State<AppState>) -> Json<Counts> {
    Json(Counts {
        active_users: app.hub.connection_count(),
        active_rooms: app.hub.room_count(),
    })
}

pub async fn rooms(State(app): State<AppState>) -> Json<Vec<RoomSummary>> {
    Json(app.hub.summaries())
}
