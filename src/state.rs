use axum::extract::ws::{Message, WebSocket};
use chrono::{DateTime, Utc};
use futures_util::stream::SplitSink;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use std::{
    collections::HashMap,
    sync::{Arc},
};
use uuid::Uuid;

use crate::utils::{Data, EventKind, MessageEvents, Room};

pub type User = Arc<Mutex<HashMap<Uuid, SplitSink<WebSocket, Message>>>>;

pub type History = Arc<Mutex<HashMap<String, Vec<HistoryEvent>>>>;

pub type Rooms = Arc<Mutex<HashMap<String, Room>>>;

#[derive(Clone)]
pub struct AppState {
    pub users: User,
    pub history: History,
    pub rooms: Rooms,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(serialize = "snake_case", deserialize = "camelCase"))]
pub struct HistoryEvent {
    pub event_id: Uuid,
    pub event_type: EventKind,
    pub event_time: DateTime<Utc>,
    pub event_data: Data,
    pub event_room: String,
}



impl AppState {
    pub fn new() -> Self {
        Self {
            users: Arc::new(Mutex::new(HashMap::new())),
            history: Arc::new(Mutex::new(HashMap::new())),
            rooms: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
