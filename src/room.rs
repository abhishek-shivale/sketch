use std::hash::{DefaultHasher, Hash, Hasher};

use crate::{
    state::AppState,
    utils::{Data, GlobalEvents, MessageEvents, SenderType},
};
use axum::extract::{
    State,
    ws::{Message, WebSocket},
};
use futures_util::{SinkExt, StreamExt};
use uuid::Uuid;

pub async fn interact(socket: WebSocket, state: State<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let key = Uuid::new_v4();
    let hash = calculate_hash(&key);
    let mut map = state.0.users.lock().await;
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
                                    MessageEvents::CanvasCursor { x, y } => {
                                        let send_data = Data::canvas_cursor(parsed.user, x, y);
                                        brodcast(send_data, &state).await;
                                    }
                                    MessageEvents::CanvasAdd { action } => {
                                        let send_data = Data::canvas_add(parsed.user, action);
                                        brodcast(send_data, &state).await;
                                    }
                                    MessageEvents::CanvasUpdate { action } => {
                                        let send_data = Data::canvas_update(parsed.user, action);
                                        brodcast(send_data, &state).await;
                                    }
                                    MessageEvents::CanvasDuplicate { action } => {
                                        let send_data = Data::canvas_duplicate(parsed.user, action);
                                        brodcast(send_data, &state).await;
                                    }
                                    MessageEvents::CanvasMove { action } => {
                                        let send_data = Data::canvas_move(parsed.user, action);
                                        brodcast(send_data, &state).await;
                                    }
                                    MessageEvents::CanvasDelete { id, ids } => {
                                        let send_data = Data::canvas_delete(parsed.user, id, ids);
                                        brodcast(send_data, &state).await;
                                    }
                                    MessageEvents::RoomCreated { room } => {
                                        let send_data =
                                            Data::room_created(parsed.user, room.clone());
                                        {
                                            state.0.rooms.lock().await.insert(room.id.clone(), room);
                                        }
                                        brodcast(send_data, &state).await;
                                    }
                                    MessageEvents::RoomJoined { room } => {
                                        let send_data = Data::room_joined(parsed.user, room.clone());
                                        brodcast(send_data, &state).await;
                                    }
                                    MessageEvents::RoomRemoved { room } => {
                                        let send_data = Data::room_removed(parsed.user, room);
                                        brodcast(send_data, &state).await;
                                    }
                                    MessageEvents::ChatMessage { chat } => {
                                        let send_data = Data::chat_message(parsed.user, chat);
                                        brodcast(send_data, &state).await;
                                    }
                                    MessageEvents::ChatReaction { reaction } => {
                                        let send_data = Data::chat_reaction(parsed.user, reaction);
                                        brodcast(send_data, &state).await;
                                    }
                                    MessageEvents::PlayBack { room_id, history } => {
                                        let send_data =
                                            Data::playback(parsed.user, room_id, history);
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
                state.0.users.lock().await.remove(&hash);
            }
            Err(e) => eprintln!("Somthing went wrong!!!, {}", e),
            _ => {}
        }
    }
}

async fn brodcast(message: Data, state: &State<AppState>) {
    let data = message.convert();
    let mut failed_keys: Vec<u64> = vec![];
    let mut map = state.0.users.lock().await;
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
