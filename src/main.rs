use std::time::Duration;

use axum::{
    Router,
    extract::{State, WebSocketUpgrade},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use chrono::{TimeDelta, Utc};
use tokio::{self};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;
mod room;
mod state;
mod utils;
use crate::{
    state::AppState,
    utils::{EventKind, MessageEvents},
};

#[tokio::main]
async fn main() {
    let state: AppState = AppState::new();
    //we use clone here because there here are just cloning arc values not the actual state
    let state_clone = state.clone();
    let app = Router::new()
        .route("/ws", any(handler))
        .route("/rooms", get(list_rooms))
        .route("/count", get(handler_count))
        .route("/health", get(|| async { "ok" }))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .fallback_service(
            ServeDir::new("public").not_found_service(ServeFile::new("public/index.html")),
        );

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::new(60, 0)).await;
            let mut h_lock = state_clone.history.lock().await;
            let work: Vec<(String, Vec<Uuid>)> = h_lock
                .iter()
                .filter_map(|(room_id, events)| {
                    let idle = Utc::now();
                    let last = events.last()?;
                    if (idle - last.event_time.to_utc()) <= TimeDelta::seconds(60) {
                        return None;
                    }

                    let deleted_ids: Vec<Uuid> = events
                        .iter()
                        .filter(|t| (idle - t.event_time.to_utc()) > TimeDelta::seconds(60))
                        .filter(|x| matches!(x.event_type, EventKind::CanvasDelete))
                        .flat_map(|h| {
                            if let Some(value) = h.event_data.value.as_ref() {
                                match &value.events {
                                    MessageEvents::CanvasDelete { id, ids, .. } => id
                                        .iter()
                                        .cloned()
                                        .chain(ids.iter().flat_map(|v| v.iter().cloned()))
                                        .filter_map(|s| Uuid::parse_str(&s).ok())
                                        .collect::<Vec<_>>(),
                                    _ => vec![],
                                }
                            } else {
                                vec![]
                            }
                        })
                        .collect();

                    if deleted_ids.is_empty() {
                        None
                    } else {
                        Some((room_id.clone(), deleted_ids))
                    }
                })
                .collect();

            for (room_id, deleted_ids) in work {
                for id in &deleted_ids {
                    let id_str = id.to_string();
                    if let Some(value) = h_lock.get_mut(&room_id) {
                        let to_remove: Vec<usize> = value
                            .iter()
                            .enumerate()
                            .filter_map(|(i, x)| {
                                x.event_data
                                    .value
                                    .as_ref()
                                    .and_then(|data| match &data.events {
                                        MessageEvents::CanvasAdd { action }
                                            if action.id == id_str =>
                                        {
                                            Some(i)
                                        }
                                        _ => None,
                                    })
                            })
                            .collect();

                        for &i in to_remove.iter().rev() {
                            value.remove(i);
                        }
                    }
                }
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    println!("Server Started on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn handler(state: State<AppState>, socket: WebSocketUpgrade) -> Response {
    // AsyncFn(Websocket) will be type if we add async move before clouser also it will run mutiple times because state is not moved to body
    // Future(Websocket)<Output=()> it will run once bcause state is moving into body.
    socket.on_upgrade(|socket| async move {
        room::interact(socket, state).await;
    })
}

async fn list_rooms(State(state): State<AppState>) -> impl IntoResponse {
    let rooms = state.rooms.lock().await;
    let body: Vec<_> = rooms
        .values()
        .map(|r| serde_json::json!({ "id": r.id, "member_count": r.members.len() }))
        .collect();
    axum::Json(body)
}

async fn handler_count(State(state): State<AppState>) -> impl IntoResponse {
    let room = state.rooms.lock().await;
    let room_count = room.len();
    drop(room);
    let users = state.users.lock().await;
    let user_count = users.len();
    drop(users);
    axum::Json(serde_json::json!({ "activeUsers": user_count, "activeRooms": room_count }))
}
