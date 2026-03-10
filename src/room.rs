use crate::{
    state::{AppState, HistoryEvent},
    utils::{Data, EventKind, GlobalEvents, MessageEvents, SenderType},
};
use axum::extract::{
    State,
    ws::{Message, WebSocket},
};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use uuid::Uuid;

pub async fn interact(socket: WebSocket, state: State<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let key = Uuid::new_v4();
    let mut map = state.0.users.lock().await;
    if !map.contains_key(&key) {
        {
            let send_data = Data::connected(key);
            send_message(send_data, &mut sender).await;
        }
        map.insert(key, sender);
    }
    drop(map);

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(data)) => {
                match serde_json::from_str::<Data>(&data) {
                    Ok(parsed) => match &parsed.key {
                        GlobalEvents::Disconnected => {}
                        GlobalEvents::Message => {
                            if let Some(data) = parsed.value.clone() {
                                match data.events {
                                    MessageEvents::CanvasCursor { x, y, room_id } => {
                                        let send_data = Data::canvas_cursor(parsed.user, x, y, room_id.clone());
                                        broadcast_in_room(room_id, send_data, &state).await;
                                    }
                                    MessageEvents::CanvasAdd { action } => {
                                        let send_data =
                                            Data::canvas_add(parsed.user.clone(), action.clone());
                                        add_event_history(
                                            parsed,
                                            EventKind::CanvasAdd,
                                            action.room_id.clone(),
                                            &state,
                                        )
                                        .await;
                                        broadcast_in_room(action.room_id, send_data, &state).await;
                                    }
                                    MessageEvents::CanvasUpdate { action } => {
                                        let send_data = Data::canvas_update(
                                            parsed.user.clone(),
                                            action.clone(),
                                        );

                                        add_event_history(
                                            parsed,
                                            EventKind::CanvasUpdate,
                                            action.room_id.clone(),
                                            &state,
                                        )
                                        .await;

                                        broadcast_in_room(action.room_id, send_data, &state).await;
                                    }

                                    MessageEvents::CanvasDuplicate { action } => {
                                        let send_data = Data::canvas_duplicate(
                                            parsed.user.clone(),
                                            action.clone(),
                                        );

                                        add_event_history(
                                            parsed,
                                            EventKind::CanvasDuplicate,
                                            action.room_id.clone(),
                                            &state,
                                        )
                                        .await;

                                        broadcast_in_room(action.room_id, send_data, &state).await;
                                    }

                                    MessageEvents::CanvasMove { action } => {
                                        let send_data =
                                            Data::canvas_move(parsed.user.clone(), action.clone());

                                        add_event_history(
                                            parsed,
                                            EventKind::CanvasMove,
                                            action.room_id.clone(),
                                            &state,
                                        )
                                        .await;

                                        broadcast_in_room(action.room_id, send_data, &state).await;
                                    }

                                    MessageEvents::CanvasDelete { id, ids, room_id } => {
                                        let send_data = Data::canvas_delete(
                                            parsed.user.clone(),
                                            id.clone(),
                                            ids.clone(),
                                            room_id.clone(),
                                        );
                                        // todo!
                                        add_event_history(
                                            parsed,
                                            EventKind::CanvasDelete,
                                            room_id.clone(),
                                            &state,
                                        )
                                        .await;

                                        broadcast_in_room(room_id, send_data, &state).await;
                                    }

                                    MessageEvents::RoomCreated { room } => {
                                        let send_data =
                                            Data::room_created(parsed.user.clone(), room.clone());

                                        add_event_history(
                                            parsed.clone(),
                                            EventKind::RoomCreated,
                                            room.id.clone(),
                                            &state,
                                        )
                                        .await;

                                        {
                                            state
                                                .0
                                                .rooms
                                                .lock()
                                                .await
                                                .insert(room.id.clone(), room);
                                        };
                                        broadcast_to_user(&parsed.user.id, send_data, &state).await;
                                    }

                                    MessageEvents::RoomJoined { room } => {
                                        // User Join the room

                                        let send_data =
                                            Data::room_joined(parsed.user.clone(), room.clone());

                                        add_event_history(
                                            parsed,
                                            EventKind::RoomJoined,
                                            room.id.clone(),
                                            &state,
                                        )
                                        .await;

                                        broadcast_in_room(room.id.clone(), send_data, &state).await;
                                    }

                                    MessageEvents::RoomRemoved { room } => {
                                        // User Left the room
                                        let send_data =
                                            Data::room_removed(parsed.user.clone(), room.clone());

                                        add_event_history(
                                            parsed,
                                            EventKind::RoomRemoved,
                                            room.id.clone(),
                                            &state,
                                        )
                                        .await;

                                        broadcast_in_room(room.id.clone(), send_data, &state).await;
                                    }

                                    MessageEvents::ChatMessage { chat } => {
                                        let send_data =
                                            Data::chat_message(parsed.user.clone(), chat.clone());
                                        broadcast_in_room(chat.room_id, send_data, &state).await;
                                    }

                                    MessageEvents::ChatReaction { reaction } => {
                                        let send_data = Data::chat_reaction(
                                            parsed.user.clone(),
                                            reaction.clone(),
                                        );
                                        broadcast_in_room(reaction.room_id, send_data, &state)
                                            .await;
                                    }

                                    MessageEvents::PlayBack { room_id, history } => {
                                        let lock = state.0.history.lock().await;
                                        let send_data = Data::playback(
                                            parsed.user.clone(),
                                            room_id.clone(),
                                            lock.get(&room_id).cloned(),
                                        );
                                        drop(lock);
                                        broadcast(send_data, &state).await;
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
                state.0.users.lock().await.remove(&key);
            }
            Err(e) => eprintln!("Somthing went wrong!!!, {}", e),
            _ => {}
        }
    }
}

async fn broadcast(message: Data, state: &State<AppState>) {
    let data = message.convert();
    let mut failed_keys: Vec<Uuid> = vec![];
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
async fn send_message(send_data: Data, sender: &mut SenderType) {
    match sender.send(send_data.convert()).await {
        _ => {}
    }
}

async fn broadcast_in_room(room_id: String, message: Data, state: &State<AppState>) {
    let data = message.convert();
    let mut failed_keys: Vec<Uuid> = vec![];
    let map = state.0.rooms.lock().await;
    let mut user = state.0.users.lock().await;
    if let Some(room) = map.get(&room_id) {
        for member in room.members.iter() {
            if let Some(socket) = user.get_mut(member) {
                match socket.send(data.clone()).await {
                    Err(_) => {
                        eprintln!("Error while sending message for: {}", member);
                        failed_keys.push(*member);
                    }
                    Ok(()) => {}
                };
            }
        }
    }
}

async fn add_event_history(
    message: Data,
    event_type: EventKind,
    event_room: String,
    state: &State<AppState>,
) {
    let event = HistoryEvent {
        event_data: message,
        event_id: Uuid::new_v4(),
        event_time: Utc::now(),
        event_type,
        event_room: event_room.clone(),
    };

    match state.0.history.lock().await.get_mut(&event_room) {
        Some(x) => x.push(event),
        None => {
            let vec: Vec<HistoryEvent> = vec![event];
            state.0.history.lock().await.insert(event_room, vec);
        }
    }
}

async fn broadcast_to_user(user_id: &Uuid, message: Data, state: &State<AppState>) {
    let data = message.convert();
    let mut failed_keys: Vec<Uuid> = vec![];
    if let Some(sender) = state.0.users.lock().await.get_mut(user_id) {
        match sender.send(data.clone()).await {
            Err(_) => {
                eprintln!("Error while sending message for:");
                failed_keys.push(user_id.clone());
            }
            Ok(()) => {}
        };
    }
}
