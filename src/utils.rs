use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use chrono::{DateTime, Utc};
use futures_util::stream::SplitSink;
use serde::{Deserialize, Serialize};

use crate::state::HistoryEvent;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(serialize = "snake_case", deserialize = "camelCase"))]
pub enum GlobalEvents {
    Connected,
    Disconnected,
    Message,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Point {
    x: i16,
    y: i16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Tools {}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Action {
    color: String,
    fill_color: String,
    id: String,
    opacity: i8,
    points: Vec<Point>,
    size: i8,
    timestamp: DateTime<Utc>,
    tool: Tools,
    text: Option<String>,
    image_data: Option<String>,
    image_height: Option<i16>,
    image_width: Option<i16>,
    room_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Room {
    id: String,
    members: Vec<String>,
    created_by: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    message_id: String,
    text: String,
    reaction_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Chat {
    room_id: String,
    message: Option<Vec<ChatMessage>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reaction {
    room_id: String,
    message_id: String,
    reaction_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(serialize = "snake_case", deserialize = "camelCase"))]
pub enum MessageEvents {
    CanvasCursor {
        x: i16,
        y: i16,
    },
    CanvasAdd {
        action: Action,
    },
    CanvasUpdate {
        action: Action,
    },
    CanvasDuplicate {
        action: Action,
    },
    CanvasMove {
        action: Action,
    },
    CanvasDelete {
        id: Option<String>,
        ids: Vec<String>,
    },
    RoomCreated {
        room: Room,
    },
    RoomJoined {
        room: Room,
    },
    RoomRemoved {
        room: Room,
    },
    ChatMessage {
        chat: Chat,
    },
    ChatReaction {
        reaction: Reaction,
    },
    PlayBack {
        room_id: String,
        history: Vec<HistoryEvent>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub color: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DataValue {
    pub events: MessageEvents,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(serialize = "snake_case", deserialize = "camelCase"))]
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
                    color: "".to_string(),
                }
            },
        }
    }

    pub fn canvas_cursor(user: User, action: Action) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::CanvasAdd { action },
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
