use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
    routing::any,
};
use futures_util::stream::SplitSink;
use std::{collections::HashMap, sync::Arc};
use tokio::{self, sync::Mutex};
use tower_http::services::ServeDir;
mod room;
mod utils;
use crate::utils::StateType;

#[tokio::main]
async fn main() {
    let state: StateType = Arc::new(Mutex::new(
        HashMap::<u64, SplitSink<WebSocket, Message>>::new(),
    ));
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
    // AsyncFn(Websocket) will be type if we add async move before clouser also it will run mutiple times because state is not moved to body
    // Future(Websocket)<Output=()> it will run once bcause state is moving into body.
    socket.on_upgrade(|socket| async move {
       match room::interact(socket, state).await {
           Err(_e) => eprintln!("Somthing went wrong"),
           Ok(_s) => {}
       }
    })
}
