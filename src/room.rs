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
                        GlobalEvents::Disconnected => {
                            clean_up(key, &state).await;
                        }
                        GlobalEvents::Message => {
                            if let Some(data) = parsed.value.clone() {
                                match data.events {
                                    MessageEvents::CanvasCursor { x, y, room_id } => {
                                        let send_data = Data::canvas_cursor(
                                            parsed.user.clone(),
                                            x,
                                            y,
                                            room_id.clone(),
                                        );
                                        broadcast_in_room(
                                            room_id,
                                            send_data,
                                            &state,
                                            Some(parsed.user.id),
                                        )
                                        .await;
                                    }
                                    MessageEvents::CanvasAdd { action } => {
                                        let send_data =
                                            Data::canvas_add(parsed.user.clone(), action.clone());
                                        add_event_history(
                                            parsed.clone(),
                                            EventKind::CanvasAdd,
                                            action.room_id.clone(),
                                            &state,
                                        )
                                        .await;
                                        broadcast_in_room(
                                            action.room_id,
                                            send_data,
                                            &state,
                                            Some(parsed.user.id),
                                        )
                                        .await;
                                    }
                                    MessageEvents::CanvasUpdate { action } => {
                                        let send_data = Data::canvas_update(
                                            parsed.user.clone(),
                                            action.clone(),
                                        );

                                        add_event_history(
                                            parsed.clone(),
                                            EventKind::CanvasUpdate,
                                            action.room_id.clone(),
                                            &state,
                                        )
                                        .await;

                                        broadcast_in_room(
                                            action.room_id,
                                            send_data,
                                            &state,
                                            Some(parsed.user.id),
                                        )
                                        .await;
                                    }

                                    MessageEvents::CanvasDuplicate { action } => {
                                        let send_data = Data::canvas_duplicate(
                                            parsed.user.clone(),
                                            action.clone(),
                                        );

                                        add_event_history(
                                            parsed.clone(),
                                            EventKind::CanvasDuplicate,
                                            action.room_id.clone(),
                                            &state,
                                        )
                                        .await;

                                        broadcast_in_room(
                                            action.room_id,
                                            send_data,
                                            &state,
                                            Some(parsed.user.id),
                                        )
                                        .await;
                                    }

                                    MessageEvents::CanvasMove { action } => {
                                        let send_data =
                                            Data::canvas_move(parsed.user.clone(), action.clone());

                                        add_event_history(
                                            parsed.clone(),
                                            EventKind::CanvasMove,
                                            action.room_id.clone(),
                                            &state,
                                        )
                                        .await;

                                        broadcast_in_room(
                                            action.room_id,
                                            send_data,
                                            &state,
                                            Some(parsed.user.id),
                                        )
                                        .await;
                                    }

                                    MessageEvents::CanvasDelete { id, ids, room_id } => {
                                        let send_data = Data::canvas_delete(
                                            parsed.user.clone(),
                                            id.clone(),
                                            ids.clone(),
                                            room_id.clone(),
                                        );

                                        add_event_history(
                                            parsed.clone(),
                                            EventKind::CanvasDelete,
                                            room_id.clone(),
                                            &state,
                                        )
                                        .await;

                                        broadcast_in_room(
                                            room_id,
                                            send_data,
                                            &state,
                                            Some(parsed.user.id),
                                        )
                                        .await;
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

                                        {
                                            let mut room_lock = state
                                                .0
                                                .rooms
                                                .lock()
                                                .await;
                                            
                                            let curr_room = room_lock.entry(room.id.clone()).or_insert(room.clone());

                                            if !curr_room.members.contains(&parsed.user.id) {
                                                curr_room.members.push(parsed.user.id.clone());
                                            }
                                        }

                                        add_event_history(
                                            parsed.clone(),
                                            EventKind::RoomJoined,
                                            room.id.clone(),
                                            &state,
                                        )
                                        .await;

                                        broadcast_in_room(
                                            room.id.clone(),
                                            send_data,
                                            &state,
                                            Some(parsed.user.id),
                                        )
                                        .await;
                                        
                                        let lock = state.0.history.lock().await;
                                        let send_data = Data::playback(
                                            parsed.user.clone(),
                                            room.id.clone(),
                                            lock.get(&room.id).cloned(),
                                        );
                                        drop(lock);
                                        broadcast_to_user(&parsed.user.id, send_data, &state).await;
                                    }

                                    MessageEvents::RoomRemoved { room } => {
                                        // User Left the room
                                        let send_data =
                                            Data::room_removed(parsed.user.clone(), room.clone());

                                        add_event_history(
                                            parsed.clone(),
                                            EventKind::RoomRemoved,
                                            room.id.clone(),
                                            &state,
                                        )
                                        .await;
                                        
                                        {
                                            let mut rooms = state.0.rooms.lock().await;
                                            let mut is_empty = false;

                                            if let Some(curr_room) = rooms.get_mut(&room.id) {
                                                curr_room
                                                    .members
                                                    .retain(|id| id != &parsed.user.id);
                                                is_empty = curr_room.members.is_empty();
                                            }

                                            if is_empty {
                                                rooms.remove(&room.id);
                                                drop(rooms);
                                                state
                                                    .0
                                                    .history
                                                    .lock()
                                                    .await
                                                    .remove(&room.id.clone());
                                            }
                                        }

                                        broadcast_in_room(
                                            room.id.clone(),
                                            send_data,
                                            &state,
                                            Some(parsed.user.id),
                                        )
                                        .await;
                                    }

                                    MessageEvents::ChatMessage { chat } => {
                                        let send_data =
                                            Data::chat_message(parsed.user.clone(), chat.clone());
                                        broadcast_in_room(
                                            chat.room_id,
                                            send_data,
                                            &state,
                                            Some(parsed.user.id),
                                        )
                                        .await;
                                    }

                                    MessageEvents::ChatReaction { reaction } => {
                                        let send_data = Data::chat_reaction(
                                            parsed.user.clone(),
                                            reaction.clone(),
                                        );
                                        broadcast_in_room(
                                            reaction.room_id,
                                            send_data,
                                            &state,
                                            Some(parsed.user.id),
                                        )
                                        .await;
                                    }

                                    MessageEvents::PlayBack { room_id, history: _history } => {
                                        let lock = state.0.history.lock().await;
                                        let send_data = Data::playback(
                                            parsed.user.clone(),
                                            room_id.clone(),
                                            lock.get(&room_id).cloned(),
                                        );
                                        drop(lock);
                                        broadcast_to_user(&parsed.user.id, send_data, &state).await;
                                    }
                                    
                                    MessageEvents::RoomMembersCount { room_id, count } => {
                                        let send_data = Data::room_members_count(
                                            parsed.user.clone(),
                                            room_id.clone(),
                                            count,
                                        );
                                        broadcast_in_room(room_id, send_data, &state, None).await;
                                    }                                    
                                }
                            }
                        }
                        _ => {}
                    },
                    Err(e) => {
                        eprintln!("Failed to deserialize: {}, {}", e, data);
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

    clean_up(key, &state).await;
}

async fn _broadcast(message: Data, state: &State<AppState>) {
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

    drop(map);
    for key in failed_keys {
        clean_up(key, state).await;
    }
}
async fn send_message(send_data: Data, sender: &mut SenderType) {
    match sender.send(send_data.convert()).await {
        _ => {}
    }
}

async fn broadcast_in_room(
    room_id: String,
    message: Data,
    state: &State<AppState>,
    exclude: Option<Uuid>,
) {
    let data = message.convert();
    let mut failed_keys: Vec<Uuid> = vec![];
    let map = state.0.rooms.lock().await;
    let members = &map.get(&room_id).cloned();
    drop(map);
    let mut user = state.0.users.lock().await;
    if let Some(room) = members {
        for member in room.members.iter() {
            if let Some(socket) = user.get_mut(member) {
                if Some(*member) == exclude {
                    continue;
                }

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

    drop(user);

    for key in failed_keys {
        clean_up(key, state).await;
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

    let mut lock = state.0.history.lock().await;
    lock.entry(event_room).or_insert_with(Vec::new).push(event);
}

async fn broadcast_to_user(user_id: &Uuid, message: Data, state: &State<AppState>) {
    let data = message.convert();
    if let Some(sender) = state.0.users.lock().await.get_mut(user_id) {
        match sender.send(data.clone()).await {
            Err(_) => {
                eprintln!("Error while sending message for:");
            }
            Ok(()) => {}
        };
    }
}

async fn clean_up(user_id: Uuid, state: &State<AppState>) {
    let rooms_state = state.0.rooms.lock().await;

    let room_id = rooms_state
        .iter()
        .find(|(_, room)| room.members.contains(&user_id))
        .map(|(id, _)| id.clone());

    // drop(rooms_state);

    if let Some(room_id) = room_id {
        let mut rooms = rooms_state;
        let mut is_empty = false;

        if let Some(room) = rooms.get_mut(&room_id) {
            room.members.retain_mut(|id| id != &user_id);
            is_empty = room.members.is_empty();
        }

        if is_empty {
            rooms.remove(&room_id);
            drop(rooms);
            state.0.history.lock().await.remove(&room_id);
        } else {
            drop(rooms);
        }
    }

    state.0.users.lock().await.remove(&user_id);
}
