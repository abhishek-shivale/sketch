use axum::extract::ws::{Message, WebSocket};
use chrono::{DateTime, Utc};
use futures_util::stream::SplitSink;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::utils::Data;

pub type User = Arc<Mutex<HashMap<u64, SplitSink<WebSocket, Message>>>>;

pub type History = Arc<Mutex<Vec<HistoryEvent>>>;

#[derive(Clone)]
pub struct AppState {
    pub user: User,
    pub history: History,
}

#[derive(Clone)]
pub struct HistoryEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub event_time: DateTime<Utc>,
    pub event_data: Data,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            user: Arc::new(Mutex::new(
                HashMap::<u64, SplitSink<WebSocket, Message>>::new(),
            )),
            history: Arc::new(Mutex::new(Vec::<HistoryEvent>::new())),
        }
    }
}
