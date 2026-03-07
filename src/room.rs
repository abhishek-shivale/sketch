use std::hash::{DefaultHasher, Hash, Hasher};

use crate::utils::{Data, GlobalEvents, MessageEvents, SenderType, StateType};
use axum::extract::{
    State,
    ws::{Message, WebSocket},
};
use futures_util::{SinkExt, StreamExt};
use uuid::Uuid;

pub async fn interact(socket: WebSocket, state: State<StateType>) {
    let (mut sender, mut receiver) = socket.split();
    let key = Uuid::new_v4();
    let hash = calculate_hash(&key);
    let mut map = state.lock().await;
    if !map.contains_key(&hash) {
        {
            let send_data = Data::connected(hash);
            send_message(send_data, &mut sender).await;
        }
        map.insert(hash, sender);
    }
    drop(map);

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(data)) => {
                match serde_json::from_str::<Data>(&data) {
                    Ok(parsed) => match &parsed.key {
                        GlobalEvents::Disconnected => {}
                        GlobalEvents::Message => {
                            if let Some(data) = parsed.value {
                                match data.events {
                                    MessageEvents::UserStartedDrawing { x, y } => {
                                        let send_data = Data::user_started_drawing(x, y, parsed.user);
                                        brodcast(send_data, &state).await;
                                    }
                                    MessageEvents::UserIsDrawing { x, y } => {
                                        let send_data = Data::user_is_drawing(x, y, parsed.user);
                                        brodcast(send_data, &state).await;
                                    }
                                    MessageEvents::UserStoppedDrawing => {
                                        let send_data = Data::user_stopped_drawing(parsed.user);
                                        brodcast(send_data, &state).await;
                                    }
                                }
                            }
                        }
                        _ => {}
                    },
                    Err(e) => {
                        eprintln!("Failed to deserialize: {}", e);
                    }
                };
            }
            Ok(Message::Close(_)) => {
                state.lock().await.remove(&hash);
            }
            Err(e) => eprintln!("Somthing went wrong!!!, {}", e),
            _ => {}
        }
    }
}

async fn brodcast(message: Data, state: &State<StateType>) {
    let data = message.convert();
    let mut failed_keys: Vec<u64> = vec![];
    let mut map = state.lock().await;
    for (key, sender) in map.iter_mut() {
        match sender.send(data.clone()).await {
            Err(_) => {
                eprintln!("Error while sending message for: {}", key);
                failed_keys.push(*key);
            }
            Ok(()) => {}
        };
    }

    for key in failed_keys {
        map.remove(&key);
    }

    drop(map)
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

async fn send_message(send_data: Data, sender: &mut SenderType) {
    match sender.send(send_data.convert()).await {
        _ => {}
    }
}

