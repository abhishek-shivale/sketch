//! Payload types shared by inbound and outbound messages.
//!
//! Struct fields are never renamed: they are `snake_case` on the wire in both
//! directions, exactly as the frontend's `wsTypes.ts` expects.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ProtocolError;

pub type RoomId = Arc<str>;
pub type ShapeId = Arc<str>;

pub const MAX_ID_LEN: usize = 128;
pub const MAX_NAME_LEN: usize = 64;
pub const MAX_COLOR_LEN: usize = 64;
pub const MAX_POINTS: usize = 50_000;
pub const MAX_TEXT_LEN: usize = 20_000;
pub const MAX_IDS_PER_DELETE: usize = 10_000;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
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

/// One shape on the canvas, as sent by the frontend.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Action {
    pub id: ShapeId,
    pub room_id: RoomId,
    pub tool: Tool,
    pub points: Vec<Point>,
    pub color: String,
    pub fill_color: String,
    pub size: f64,
    pub opacity: f64,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub image_data: Option<String>,
    #[serde(default)]
    pub image_height: Option<f64>,
    #[serde(default)]
    pub image_width: Option<f64>,
}

impl Action {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_id("action.id", &self.id)?;
        validate_id("action.room_id", &self.room_id)?;
        if self.points.len() > MAX_POINTS {
            return Err(ProtocolError::invalid("action.points", "too many points"));
        }
        if !self
            .points
            .iter()
            .all(|p| p.x.is_finite() && p.y.is_finite())
        {
            return Err(ProtocolError::invalid(
                "action.points",
                "not a finite number",
            ));
        }
        if self.color.len() > MAX_COLOR_LEN || self.fill_color.len() > MAX_COLOR_LEN {
            return Err(ProtocolError::invalid("action.color", "too long"));
        }
        if self.text.as_ref().is_some_and(|t| t.len() > MAX_TEXT_LEN) {
            return Err(ProtocolError::invalid("action.text", "too long"));
        }
        Ok(())
    }

    /// Rough heap footprint, used to keep room history under its byte budget.
    pub fn approx_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.id.len()
            + self.room_id.len()
            + self.points.len() * std::mem::size_of::<Point>()
            + self.color.len()
            + self.fill_color.len()
            + self.text.as_ref().map_or(0, String::len)
            + self.image_data.as_ref().map_or(0, String::len)
    }
}

/// A user's display identity. The `id` is always the server-assigned
/// connection id; see [`crate::protocol::ClientUser`] for the inbound form.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub color: String,
}

impl User {
    pub fn anonymous(id: Uuid) -> Self {
        Self {
            id,
            name: String::new(),
            color: String::new(),
        }
    }
}

/// Room snapshot as the frontend sees it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RoomInfo {
    pub id: RoomId,
    pub members: Vec<Uuid>,
    pub created_by: Uuid,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub message_id: String,
    pub text: String,
    #[serde(default)]
    pub reaction_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Chat {
    pub room_id: RoomId,
    #[serde(default)]
    pub message: Option<Vec<ChatMessage>>,
}

impl Chat {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_id("chat.room_id", &self.room_id)?;
        for message in self.message.iter().flatten() {
            validate_id("chat.message_id", &message.message_id)?;
            if message.text.len() > MAX_TEXT_LEN {
                return Err(ProtocolError::invalid("chat.text", "too long"));
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Reaction {
    pub room_id: RoomId,
    pub message_id: String,
    pub reaction_id: String,
}

impl Reaction {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_id("reaction.room_id", &self.room_id)?;
        validate_id("reaction.message_id", &self.message_id)?;
        validate_id("reaction.reaction_id", &self.reaction_id)
    }
}

pub fn validate_id(field: &'static str, id: &str) -> Result<(), ProtocolError> {
    if id.is_empty() {
        return Err(ProtocolError::invalid(field, "must not be empty"));
    }
    if id.len() > MAX_ID_LEN {
        return Err(ProtocolError::invalid(field, "too long"));
    }
    Ok(())
}
