use std::{collections::HashMap, sync::Arc};

use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use futures_util::stream::SplitSink;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum GlobalEvents {
    Connected,
    Disconnected,
    Message,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MessageEvents {
    // UserJoinedRoom,
    // UserUpdateConfig,
    // UserLeaveRoom,
    // ClearCanvas,
    UserStartedDrawing { x: i32, y: i32 },
    UserIsDrawing { x: i32, y: i32 },
    UserStoppedDrawing,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub color: String,
}

// #[derive(Serialize, Deserialize, Debug)]
// pub struct Room {
//     pub id: u64,
//     pub name: String,
//     pub admin: u64,
// }

#[derive(Serialize, Deserialize, Debug)]
pub struct DataValue {
    pub events: MessageEvents,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Data {
    pub key: GlobalEvents,
    pub value: Option<DataValue>,
    pub user: User,
}

impl Data {
    pub fn connected(user_id: u64) -> Self {
        Self {
            key: GlobalEvents::Connected,
            value: None,
            user: {
                User {
                    id: user_id,
                    name: "".to_string(),
                    color: "Red".to_string(),
                }
            },
        }
    }

    pub fn user_is_drawing(x: i32, y: i32, user: User) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::UserIsDrawing { x, y },
            }),
            user,
        }
    }

    pub fn user_started_drawing(x: i32, y: i32, user: User) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::UserStartedDrawing { x, y },
            }),
            user,
        }
    }

    pub fn user_stopped_drawing(user: User) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::UserStoppedDrawing,
            }),
            user,
        }
    }

    pub fn convert(self) -> Message {
        let utf8 = Utf8Bytes::from(serde_json::to_string(&self).expect("Parsing Fail"));
        Message::Text(utf8)
    }
}

pub type SenderType = SplitSink<WebSocket, Message>;

pub type StateType = Arc<Mutex<HashMap<u64, SplitSink<WebSocket, Message>>>>;
