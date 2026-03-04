use crate::utils::Data;
use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use futures_util::{
    StreamExt,
    stream::{SplitSink, SplitStream},
};

pub async fn interact(mut socket: WebSocket) {
    while let Some(msg) = socket.next().await {
        match msg {
            Ok(Message::Text(data)) => {
                match serde_json::from_str::<Data>(&data) {
                    Ok(parsed) => {
                        let send_data = serde_json::to_string(&parsed).expect("there is error stringify!!!");
                        let utf8 = Utf8Bytes::from(send_data);
                        
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
