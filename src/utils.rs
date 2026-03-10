use crate::state::HistoryEvent;
use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use chrono::{DateTime, Utc};
use futures_util::stream::SplitSink;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(serialize = "snake_case", deserialize = "camelCase"))]
pub enum GlobalEvents {
    Connected,
    Disconnected,
    Message,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Point {
    x: f64,
    y: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Tools {
    Pencil,
    Text,
    Image,
    Line,
    Arrow,
    Rectangle,
    Circle,
    Diamond,
    Eraser,
    Select,
}

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
    pub room_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Room {
    pub id: String,
    pub members: Vec<Uuid>,
    pub created_by: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    message_id: String,
    text: String,
    reaction_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Chat {
    pub room_id: String,
    message: Option<Vec<ChatMessage>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reaction {
    pub room_id: String,
    message_id: String,
    reaction_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(serialize = "snake_case", deserialize = "camelCase"))]
pub enum MessageEvents {
    CanvasCursor {
        x: i16,
        y: i16,
        room_id: String,
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
        ids: Option<Vec<String>>,
        room_id: String,
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
        history: Option<Vec<HistoryEvent>>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    pub id: Uuid,
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
    pub fn connected(user_id: Uuid) -> Self {
        Self {
            key: GlobalEvents::Connected,
            value: None,
            user: User {
                id: user_id,
                name: "".to_string(),
                color: "".to_string(),
            },
        }
    }

    pub fn _disconnected(user: User) -> Self {
        Self {
            key: GlobalEvents::Disconnected,
            value: None,
            user,
        }
    }

    pub fn canvas_cursor(user: User, x: i16, y: i16, room_id: String) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::CanvasCursor { x, y, room_id },
            }),
            user,
        }
    }

    pub fn canvas_add(user: User, action: Action) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::CanvasAdd { action },
            }),
            user,
        }
    }

    pub fn canvas_update(user: User, action: Action) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::CanvasUpdate { action },
            }),
            user,
        }
    }

    pub fn canvas_duplicate(user: User, action: Action) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::CanvasDuplicate { action },
            }),
            user,
        }
    }

    pub fn canvas_move(user: User, action: Action) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::CanvasMove { action },
            }),
            user,
        }
    }

    pub fn canvas_delete(
        user: User,
        id: Option<String>,
        ids: Option<Vec<String>>,
        room_id: String,
    ) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::CanvasDelete { id, ids, room_id },
            }),
            user,
        }
    }

    pub fn room_created(user: User, room: Room) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::RoomCreated { room },
            }),
            user,
        }
    }

    pub fn room_joined(user: User, room: Room) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::RoomJoined { room },
            }),
            user,
        }
    }

    pub fn room_removed(user: User, room: Room) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::RoomRemoved { room },
            }),
            user,
        }
    }

    pub fn chat_message(user: User, chat: Chat) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::ChatMessage { chat },
            }),
            user,
        }
    }

    pub fn chat_reaction(user: User, reaction: Reaction) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::ChatReaction { reaction },
            }),
            user,
        }
    }

    pub fn playback(user: User, room_id: String, history: Option<Vec<HistoryEvent>>) -> Self {
        Self {
            key: GlobalEvents::Message,
            value: Some(DataValue {
                events: MessageEvents::PlayBack { room_id, history },
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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(serialize = "snake_case", deserialize = "camelCase"))]
pub enum EventKind {
    CanvasCursor,
    CanvasAdd,
    CanvasUpdate,
    CanvasDuplicate,
    CanvasMove,
    CanvasDelete,
    RoomCreated,
    RoomJoined,
    RoomRemoved,
    ChatMessage,
    ChatReaction,
    PlayBack,
}
