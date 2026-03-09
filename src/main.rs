use axum::{
    Router,
    extract::{
         State, WebSocketUpgrade
    },
    response::Response,
    routing::any,
};
use tokio::{self};
use tower_http::services::ServeDir;
mod room;
mod state;
mod utils;
use crate::{state::AppState};

#[tokio::main]
async fn main() {
    let state: AppState = AppState::new();
    let app = Router::new()
        .route("/ws", any(handler))
        .with_state(state)
        .fallback_service(ServeDir::new("public").append_index_html_on_directories(true));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap()
}

async fn handler(state: State<AppState>, socket: WebSocketUpgrade) -> Response {
    // AsyncFn(Websocket) will be type if we add async move before clouser also it will run mutiple times because state is not moved to body
    // Future(Websocket)<Output=()> it will run once bcause state is moving into body.
    socket.on_upgrade(|socket| async move {
        room::interact(socket, state).await;
    })
}
