//! Event handling for one connection.
//!
//! Everything here is synchronous: sends are non-blocking `try_send`s into
//! bounded queues, so handling an event never waits on another client.

use std::sync::Arc;

use axum::extract::ws::Message;
use tokio::time::Instant;
use tracing::{debug, info};
use uuid::Uuid;

use super::rate_limit::TokenBucket;
use crate::{
    error::ProtocolError,
    hub::{HistoryChange, Hub, Peer, Room},
    protocol::{
        Action, ClientEnvelope, ClientEvent, ClientKey, ClientUser, ControlFrame, HistoryEvent,
        MAX_COLOR_LEN, MAX_IDS_PER_DELETE, MAX_NAME_LEN, RoomId, ServerEvent, ServerMessage,
        ShapeId, User, encode, validate_id,
    },
};

/// Whether the connection should keep going after a message.
#[derive(Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Close,
}

pub struct Session {
    user: User,
    hub: Arc<Hub>,
    peer: Peer,
    room: Option<Arc<Room>>,
    limiter: TokenBucket,
}

impl Session {
    pub fn new(hub: Arc<Hub>, peer: Peer) -> Self {
        let config = hub.config();
        let limiter = TokenBucket::new(config.rate_per_sec, config.rate_burst);
        Self {
            user: User::anonymous(peer.id),
            hub,
            peer,
            room: None,
            limiter,
        }
    }

    pub fn id(&self) -> Uuid {
        self.peer.id
    }

    /// Tells the client its server-assigned id.
    pub fn greet(&self) -> Result<(), ProtocolError> {
        self.send(&ServerMessage::connected(self.id()))
    }

    pub fn handle_text(&mut self, text: &str) -> Result<Flow, ProtocolError> {
        let envelope = ClientEnvelope::parse(text)?;
        match envelope.key {
            ClientKey::Ping => {
                self.send(&ControlFrame::Pong)?;
                Ok(Flow::Continue)
            }
            ClientKey::Pong | ClientKey::Connected => Ok(Flow::Continue),
            ClientKey::Disconnected => Ok(Flow::Close),
            ClientKey::Message => {
                if !self.limiter.try_acquire() {
                    return Err(ProtocolError::RateLimited);
                }
                if let Some(user) = envelope.user {
                    self.update_user(user);
                }
                match envelope.value {
                    Some(value) => self.handle_event(value.events)?,
                    None => return Err(ProtocolError::invalid("value", "missing")),
                }
                Ok(Flow::Continue)
            }
        }
    }

    /// Reports a protocol error back to the client.
    pub fn report(&self, error: &ProtocolError) {
        debug!(conn = %self.id(), "rejected message: {error}");
        // Answering every dropped message would amplify a flood.
        if matches!(error, ProtocolError::RateLimited) {
            return;
        }
        let message = error.to_string();
        if let Ok(frame) = encode(&ControlFrame::Error { message: &message }) {
            self.peer.deliver(&frame);
        }
    }

    pub fn ping(&self) {
        self.peer.deliver(&Message::Ping(Default::default()));
    }

    /// Leaves the current room, if any.
    pub fn leave(&mut self) {
        let Some(room) = self.room.take() else {
            return;
        };
        let empty = room.leave(&self.user);
        info!(conn = %self.id(), room = %room.id(), "left room");
        if empty && self.hub.config().room_idle_ttl.is_zero() {
            self.hub.evict_if_idle(room.id(), Instant::now());
        }
    }

    fn handle_event(&mut self, event: ClientEvent) -> Result<(), ProtocolError> {
        match event {
            ClientEvent::CanvasCursor { x, y, room_id } => {
                if !x.is_finite() || !y.is_finite() {
                    return Err(ProtocolError::invalid("cursor", "not a finite number"));
                }
                let room = self.room_for(&room_id)?;
                let room_id = room.id().clone();
                self.publish(&room, ServerEvent::CanvasCursor { x, y, room_id }, false)
            }
            ClientEvent::CanvasAdd { action } => {
                self.canvas(action, |action| ServerEvent::CanvasAdd { action })
            }
            ClientEvent::CanvasUpdate { action } => {
                self.canvas(action, |action| ServerEvent::CanvasUpdate { action })
            }
            ClientEvent::CanvasDuplicate { action } => {
                self.canvas(action, |action| ServerEvent::CanvasDuplicate { action })
            }
            ClientEvent::CanvasMove { action } => {
                self.canvas(action, |action| ServerEvent::CanvasMove { action })
            }
            ClientEvent::CanvasDelete { id, ids, room_id } => self.delete(id, ids, &room_id),
            ClientEvent::RoomCreated { room } => {
                validate_id("room.id", &room.id)?;
                let info = self.hub.create(room.id, self.id())?;
                self.send(&ServerMessage::event(
                    self.user.clone(),
                    ServerEvent::RoomCreated { room: info },
                ))
            }
            ClientEvent::RoomJoined { room } => self.join(room.id),
            ClientEvent::RoomRemoved { room } => {
                self.room_for(&room.id)?;
                self.leave();
                Ok(())
            }
            ClientEvent::ChatMessage { chat } => {
                chat.validate()?;
                let room = self.room_for(&chat.room_id)?;
                self.publish(&room, ServerEvent::ChatMessage { chat }, false)
            }
            ClientEvent::ChatReaction { reaction } => {
                reaction.validate()?;
                let room = self.room_for(&reaction.room_id)?;
                self.publish(&room, ServerEvent::ChatReaction { reaction }, false)
            }
            ClientEvent::PlayBack { room_id } => {
                let room = self.room_for(&room_id)?;
                self.send(&ServerMessage::event(
                    self.user.clone(),
                    ServerEvent::PlayBack {
                        room_id: room.id().clone(),
                        history: Some(room.history()),
                    },
                ))
            }
            ClientEvent::RoomMembersCount { room_id, .. } => {
                // The client's own count is only a hint; answer with ours.
                let room = self.room_for(&room_id)?;
                let count = u32::try_from(room.member_count()).unwrap_or(u32::MAX);
                self.send(&ServerMessage::event(
                    self.user.clone(),
                    ServerEvent::RoomMembersCount {
                        room_id: room.id().clone(),
                        count,
                    },
                ))
            }
        }
    }

    fn join(&mut self, room_id: RoomId) -> Result<(), ProtocolError> {
        validate_id("room.id", &room_id)?;
        // Joining again (e.g. after a reconnect race) just re-syncs.
        self.leave();
        let room = self.hub.join(room_id, &self.user, self.peer.clone())?;
        info!(conn = %self.id(), room = %room.id(), members = room.member_count(), "joined room");
        self.room = Some(room);
        Ok(())
    }

    fn canvas(
        &mut self,
        action: Action,
        make: fn(Action) -> ServerEvent,
    ) -> Result<(), ProtocolError> {
        action.validate()?;
        let room = self.room_for(&action.room_id)?;
        self.publish(&room, make(action), true)
    }

    fn delete(
        &mut self,
        id: Option<ShapeId>,
        ids: Option<Vec<ShapeId>>,
        room_id: &str,
    ) -> Result<(), ProtocolError> {
        let room = self.room_for(room_id)?;
        if ids
            .as_ref()
            .is_some_and(|ids| ids.len() > MAX_IDS_PER_DELETE)
        {
            return Err(ProtocolError::invalid("ids", "too many ids"));
        }
        let targets: Vec<ShapeId> = id.iter().chain(ids.iter().flatten()).cloned().collect();
        for target in &targets {
            validate_id("id", target)?;
        }

        let event = ServerEvent::CanvasDelete {
            id,
            ids,
            room_id: room.id().clone(),
        };
        let frame = encode(&ServerMessage::event(self.user.clone(), event))
            .map_err(ProtocolError::Encode)?;
        room.publish(self.id(), &frame, HistoryChange::Delete(targets));
        Ok(())
    }

    /// Broadcasts `event` to the rest of the room, optionally recording it.
    fn publish(&self, room: &Room, event: ServerEvent, record: bool) -> Result<(), ProtocolError> {
        let message = ServerMessage::event(self.user.clone(), event);
        let frame = encode(&message).map_err(ProtocolError::Encode)?;
        let change = match (record, message.value) {
            (true, Some(value)) => HistoryChange::Record(Box::new(HistoryEvent::new(
                room.id().clone(),
                message.user,
                value.events,
            ))),
            _ => HistoryChange::None,
        };
        room.publish(self.id(), &frame, change);
        Ok(())
    }

    /// The joined room, provided the client is addressing it. Events can
    /// only target the room this connection actually joined.
    fn room_for(&self, room_id: &str) -> Result<Arc<Room>, ProtocolError> {
        let room = self.room.as_ref().ok_or(ProtocolError::NotInRoom)?;
        if room.id().as_ref() != room_id {
            return Err(ProtocolError::RoomMismatch {
                joined: room.id().to_string(),
                got: truncate(room_id, 64),
            });
        }
        Ok(Arc::clone(room))
    }

    fn update_user(&mut self, user: ClientUser) {
        self.user.name = truncate(&user.name, MAX_NAME_LEN);
        self.user.color = truncate(&user.color, MAX_COLOR_LEN);
    }

    fn send<T: serde::Serialize>(&self, value: &T) -> Result<(), ProtocolError> {
        let frame = encode(value).map_err(ProtocolError::Encode)?;
        self.peer.deliver(&frame);
        Ok(())
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}
