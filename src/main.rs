use axum::{
    Router,
    extract::{State, WebSocketUpgrade},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use tokio::{self};
use tower_http::services::ServeDir;
mod room;
mod state;
mod utils;
use crate::state::AppState;

#[tokio::main]
async fn main() {
    let state: AppState = AppState::new();
    let app = Router::new()
        .route("/ws", any(handler))
        .route("/rooms", get(list_rooms))
        .route("/health", get(|| async { "ok" }))
        .with_state(state)
        .fallback_service(ServeDir::new("public").append_index_html_on_directories(true));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    println!("Server Started");
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
