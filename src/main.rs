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
    let (sender, receiver) = socket.split();

    tokio::spawn(write(sender));
    tokio::spawn(read(receiver));
}

async fn read(receiver: SplitStream<WebSocket>) {
    // ...
}

async fn write(sender: SplitSink<WebSocket, Message>) {
    // ...
}
