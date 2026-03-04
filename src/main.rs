use axum::{
    Router,
    extract::{
        WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response, routing::any,
};
use futures_util::{
    StreamExt,
    stream::{SplitSink, SplitStream},
};
use tokio;
use tower_http::services::ServeDir;
mod room;
mod utils;
use crate::room::interact;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/ws", any(handler))
        .fallback_service(ServeDir::new("public").append_index_html_on_directories(true));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap()
}

async fn handler(socket: WebSocketUpgrade) -> Response {
    socket.on_upgrade(handel_socket)
}

async fn handel_socket(socket: WebSocket) {
    room::interact(socket).await;
}