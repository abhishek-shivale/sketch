use crate::utils::Data;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{
    StreamExt,
    stream::{SplitSink, SplitStream},
};

pub async fn read(mut receiver: SplitStream<WebSocket>) {
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(data)) => {
                let parsed_data = match serde_json::from_str::<Data>(&data) {
                    Ok(parsed) => {
                        println!("Parsed struct: {:?}", parsed);
                    }
                    Err(e) => {
                        eprintln!("Failed to deserialize: {:?}", e);
                    }
                };
            }
            Ok(Message::Binary(_)) => println!("Got binary"),
            Ok(Message::Ping(_)) => println!("Got ping"),
            Ok(Message::Pong(_)) => println!("Got pong"),
            Ok(Message::Close(_)) => println!("Got Close"),
            Err(e) => eprintln!("Somthing went wrong!!!"),
        }
    }
}

pub async fn write(sender: SplitSink<WebSocket, Message>) {}
