use std::sync::Arc;

use axum::{
    extract::{
        State, WebSocketUpgrade
        ,
    },
    response::Response,
    routing::any,
    Router,
};
use futures_util::StreamExt;
use tokio::{self, sync::Mutex};
use tower_http::services::ServeDir;
mod room;
mod utils;
use crate::utils::StateType;

#[tokio::main]
async fn main() {
    let mut state: StateType = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/ws", any(handler))
        .with_state(state)
        .fallback_service(ServeDir::new("public").append_index_html_on_directories(true));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap()
}

async fn handler(state: State<StateType>, socket: WebSocketUpgrade) -> Response {
    socket.on_upgrade(|socket| async move {
        room::interact(socket, state).await;
    })
}
