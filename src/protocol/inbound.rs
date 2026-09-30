//! Messages sent by clients.
//!
//! Envelope: `{ "key": "message", "value": { "events": { "<camelCaseVariant>": {...} } }, "user": {...} }`.
//! The client's `user.id` and room `members`/`created_by` are deliberately not
//! deserialized: the server owns identity and membership.

use serde::Deserialize;

use super::model::{Action, Chat, Reaction, RoomId, ShapeId};

#[derive(Deserialize, Debug)]
pub struct ClientEnvelope {
    pub key: ClientKey,
    #[serde(default)]
    pub value: Option<ClientValue>,
    #[serde(default)]
    pub user: Option<ClientUser>,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClientKey {
    Connected,
    Disconnected,
    Message,
    /// Application-level keepalive sent by the frontend every 30s.
    Ping,
    Pong,
}

#[derive(Deserialize, Debug)]
pub struct ClientValue {
    pub events: ClientEvent,
}

/// Display fields the client may choose for itself.
#[derive(Deserialize, Debug, Default, Clone)]
pub struct ClientUser {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub color: String,
}

#[derive(Deserialize, Debug)]
pub struct ClientRoom {
    pub id: RoomId,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum ClientEvent {
    CanvasCursor {
        x: f64,
        y: f64,
        room_id: RoomId,
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
        #[serde(default)]
        id: Option<ShapeId>,
        #[serde(default)]
        ids: Option<Vec<ShapeId>>,
        room_id: RoomId,
    },
    RoomCreated {
        room: ClientRoom,
    },
    RoomJoined {
        room: ClientRoom,
    },
    RoomRemoved {
        room: ClientRoom,
    },
    ChatMessage {
        chat: Chat,
    },
    ChatReaction {
        reaction: Reaction,
    },
    PlayBack {
        room_id: RoomId,
    },
    RoomMembersCount {
        room_id: RoomId,
        /// Ignored: the server replies with the real count.
        #[serde(default)]
        #[allow(dead_code)]
        count: u32,
    },
}

impl ClientEnvelope {
    pub fn parse(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}
