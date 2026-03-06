use std::hash::{DefaultHasher, Hash, Hasher};

use crate::utils::{Data, StateType};
use axum::{Error, extract::{
    State,
    ws::{Message, Utf8Bytes, WebSocket},
}};
use futures_util::{SinkExt, StreamExt};
use uuid::Uuid;

pub async fn interact(socket: WebSocket, state: State<StateType>) -> Result<(), Error> {
    let (sender, mut receiver) = socket.split();
    let key = Uuid::new_v4();
    let hash = calculate_hash(&key);
    let mut map = state.lock().await;
    if !map.contains_key(&hash) {
        map.insert(hash, sender);
    }
    drop(map);

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(data)) => {
                match serde_json::from_str::<Data>(&data) {
                    Ok(parsed) => {
                        let send_data =
                            serde_json::to_string(&parsed).expect("there is error stringify!!!");
                        let utf8 = Utf8Bytes::from(send_data);
                        brodcast(Message::Text(utf8), &state).await?;
                    }
                    Err(e) => {
                        eprintln!("Failed to deserialize: {:?}", e);
                    }
                };
            }
            Ok(Message::Binary(_)) => println!("Got binary"),
            Ok(Message::Ping(_)) => println!("Got ping"),
            Ok(Message::Pong(_)) => println!("Got pong"),
            Ok(Message::Close(_)) => {
                state.lock().await.remove(&hash);
            },
            Err(e) => eprintln!("Somthing went wrong!!!, {}", e),
        }
    }
    Ok(())
}

async fn brodcast(message: Message, state: &State<StateType>) -> Result<(), axum::Error> {
    for (_key, sender) in state.clone().lock().await.iter_mut() {
        sender.send(message.clone()).await?;
    }
    Ok(())
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}
