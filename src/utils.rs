use std::{collections::HashMap, sync::Arc};

use axum::extract::ws::{Message, WebSocket};
use futures_util::stream::SplitSink;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Debug)]
pub struct DataValue {
    pub x: i32,
    pub y: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Data {
    pub key: i32,
    pub value: DataValue,
    pub data_type: String
}

pub type StateType = Arc<Mutex<HashMap<u64, SplitSink<WebSocket, Message>>>>;
