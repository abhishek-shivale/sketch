//! Messages sent by the server.
//!
//! Envelope: `{ "key": "message", "value": { "events": { "<snake_case_variant>": {...} } }, "user": {...} }`.

use std::sync::Arc;

use axum::extract::ws::{Message, Utf8Bytes};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::model::{Action, Chat, Reaction, RoomId, RoomInfo, ShapeId, User};

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServerKey {
    Connected,
    Message,
}

#[derive(Serialize, Debug, Clone)]
pub struct ServerMessage {
    pub key: ServerKey,
    pub value: Option<ServerValue>,
    pub user: User,
}

#[derive(Serialize, Debug, Clone)]
pub struct ServerValue {
    pub events: ServerEvent,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ServerEvent {
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
        id: Option<ShapeId>,
        ids: Option<Vec<ShapeId>>,
        room_id: RoomId,
    },
    RoomCreated {
        room: RoomInfo,
    },
    /// `history` is `Some` only in the joining user's own confirmation.
    RoomJoined {
        room: RoomInfo,
        history: Option<Vec<Arc<HistoryEvent>>>,
    },
    RoomRemoved {
        room: RoomInfo,
    },
    ChatMessage {
        chat: Chat,
    },
    ChatReaction {
        reaction: Reaction,
    },
    PlayBack {
        room_id: RoomId,
        history: Option<Vec<Arc<HistoryEvent>>>,
    },
    RoomMembersCount {
        room_id: RoomId,
        count: u32,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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
    RoomMembersCount,
}

impl ServerEvent {
    pub fn kind(&self) -> EventKind {
        match self {
            Self::CanvasCursor { .. } => EventKind::CanvasCursor,
            Self::CanvasAdd { .. } => EventKind::CanvasAdd,
            Self::CanvasUpdate { .. } => EventKind::CanvasUpdate,
            Self::CanvasDuplicate { .. } => EventKind::CanvasDuplicate,
            Self::CanvasMove { .. } => EventKind::CanvasMove,
            Self::CanvasDelete { .. } => EventKind::CanvasDelete,
            Self::RoomCreated { .. } => EventKind::RoomCreated,
            Self::RoomJoined { .. } => EventKind::RoomJoined,
            Self::RoomRemoved { .. } => EventKind::RoomRemoved,
            Self::ChatMessage { .. } => EventKind::ChatMessage,
            Self::ChatReaction { .. } => EventKind::ChatReaction,
            Self::PlayBack { .. } => EventKind::PlayBack,
            Self::RoomMembersCount { .. } => EventKind::RoomMembersCount,
        }
    }
}

/// One entry of a room's canvas history, replayed by clients on join.
#[derive(Serialize, Debug, Clone)]
pub struct HistoryEvent {
    pub event_id: Uuid,
    pub event_type: EventKind,
    pub event_time: DateTime<Utc>,
    pub event_data: ServerMessage,
    pub event_room: RoomId,
}

impl HistoryEvent {
    pub fn new(room: RoomId, user: User, event: ServerEvent) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            event_type: event.kind(),
            event_time: Utc::now(),
            event_data: ServerMessage::event(user, event),
            event_room: room,
        }
    }

    /// The canvas action carried by this event, if any.
    pub fn action(&self) -> Option<&Action> {
        match &self.event_data.value.as_ref()?.events {
            ServerEvent::CanvasAdd { action }
            | ServerEvent::CanvasUpdate { action }
            | ServerEvent::CanvasDuplicate { action }
            | ServerEvent::CanvasMove { action } => Some(action),
            _ => None,
        }
    }
}

impl ServerMessage {
    pub fn connected(id: Uuid) -> Self {
        Self {
            key: ServerKey::Connected,
            value: None,
            user: User::anonymous(id),
        }
    }

    pub fn event(user: User, events: ServerEvent) -> Self {
        Self {
            key: ServerKey::Message,
            value: Some(ServerValue { events }),
            user,
        }
    }
}

/// Control frames that sit outside the regular envelope.
#[derive(Serialize, Debug)]
#[serde(tag = "key", rename_all = "snake_case")]
pub enum ControlFrame<'a> {
    Pong,
    Error { message: &'a str },
}

/// Serialize once into a refcounted text frame that is cheap to clone into
/// every recipient's queue.
pub fn encode<T: Serialize>(value: &T) -> Result<Message, serde_json::Error> {
    let json = serde_json::to_string(value)?;
    Ok(Message::Text(Utf8Bytes::from(json)))
}
