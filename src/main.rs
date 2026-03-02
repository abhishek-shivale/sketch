use axum::{
    Router,
    extract::ws::{WebSocket, WebSocketUpgrade},
    response::{IntoResponse, Response},
    routing::any,
};
use tokio::runtime::Handle;
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/ws", any(handler))
        .fallback_service(ServeDir::new("public").append_index_html_on_directories(true));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Running on http://localhost:3000");

    axum::serve(listener, app).await.unwrap();
}

async fn handler(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    while let Some(msg) = socket.recv().await {
        let msg = if let Ok(msg) = msg {
            msg
        } else {
            // client disconnected
            return;
        };

        if socket.send(msg).await.is_err() {
            // client disconnected
            return;
        }
    }
}