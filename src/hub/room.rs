use std::collections::HashMap;

use axum::extract::ws::Message;
use parking_lot::Mutex;
use tokio::{
    sync::mpsc::{self, error::TrySendError},
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use tracing::warn;
use uuid::Uuid;

use super::history::CanvasHistory;
use crate::{
    config::Config,
    protocol::{HistoryEvent, RoomId, RoomInfo, ServerEvent, ServerMessage, ShapeId, User, encode},
};

/// The outbound half of a connection, as seen by the rooms it is in.
#[derive(Clone, Debug)]
pub struct Peer {
    pub id: Uuid,
    pub tx: mpsc::Sender<Message>,
    /// Cancelling this closes the connection.
    pub kick: CancellationToken,
}

impl Peer {
    /// Queues a frame without waiting. A full queue means the client can't
    /// keep up; it is disconnected rather than allowed to hold up the room.
    pub fn deliver(&self, frame: &Message) {
        match self.tx.try_send(frame.clone()) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                warn!(peer = %self.id, "send queue full, disconnecting slow client");
                self.kick.cancel();
            }
            // The connection is already shutting down.
            Err(TrySendError::Closed(_)) => {}
        }
    }
}

/// What a published event does to the room's history.
pub enum HistoryChange {
    None,
    Record(Box<HistoryEvent>),
    Delete(Vec<ShapeId>),
}

/// A drawing room. All state sits behind one short, synchronous lock that is
/// never held across an `.await`.
#[derive(Debug)]
pub struct Room {
    id: RoomId,
    created_by: Uuid,
    state: Mutex<RoomState>,
}

#[derive(Debug)]
struct RoomState {
    members: HashMap<Uuid, Peer>,
    history: CanvasHistory,
    /// Set while the room has no members; used for idle eviction.
    empty_since: Option<Instant>,
}

impl RoomState {
    fn broadcast(&self, frame: &Message, except: Option<Uuid>) {
        for peer in self.members.values() {
            if Some(peer.id) != except {
                peer.deliver(frame);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoomStats {
    pub members: usize,
    pub shapes: usize,
    pub history_bytes: usize,
}

impl Room {
    pub fn new(id: RoomId, created_by: Uuid, config: &Config) -> Self {
        Self {
            id,
            created_by,
            state: Mutex::new(RoomState {
                members: HashMap::new(),
                history: CanvasHistory::new(config.room_max_shapes, config.room_max_bytes),
                empty_since: Some(Instant::now()),
            }),
        }
    }

    pub fn id(&self) -> &RoomId {
        &self.id
    }

    pub fn info(&self) -> RoomInfo {
        self.info_locked(&self.state.lock())
    }

    fn info_locked(&self, state: &RoomState) -> RoomInfo {
        RoomInfo {
            id: self.id.clone(),
            members: state.members.keys().copied().collect(),
            created_by: self.created_by,
        }
    }

    pub fn member_count(&self) -> usize {
        self.state.lock().members.len()
    }

    pub fn stats(&self) -> RoomStats {
        let state = self.state.lock();
        RoomStats {
            members: state.members.len(),
            shapes: state.history.len(),
            history_bytes: state.history.bytes(),
        }
    }

    pub fn history(&self) -> Vec<std::sync::Arc<HistoryEvent>> {
        self.state.lock().history.snapshot()
    }

    /// Adds `peer` and, atomically with that, sends it the room snapshot and
    /// tells everyone else. Doing both under one lock guarantees the joiner
    /// sees every canvas event exactly once: either in the snapshot or live.
    pub fn join(&self, user: &User, peer: Peer) -> Result<(), serde_json::Error> {
        let mut state = self.state.lock();
        state.members.insert(peer.id, peer.clone());
        state.empty_since = None;

        let info = self.info_locked(&state);
        let frames = (|| {
            let own = encode(&ServerMessage::event(
                user.clone(),
                ServerEvent::RoomJoined {
                    room: info.clone(),
                    history: Some(state.history.snapshot()),
                },
            ))?;
            let others = encode(&ServerMessage::event(
                user.clone(),
                ServerEvent::RoomJoined {
                    room: info.clone(),
                    history: None,
                },
            ))?;
            Ok((own, others, self.count_frame(user, info.members.len())?))
        })();

        let (own, others, count) = match frames {
            Ok(frames) => frames,
            Err(e) => {
                state.members.remove(&peer.id);
                if state.members.is_empty() {
                    state.empty_since = Some(Instant::now());
                }
                return Err(e);
            }
        };

        peer.deliver(&own);
        state.broadcast(&others, Some(peer.id));
        state.broadcast(&count, None);
        Ok(())
    }

    /// Removes a member and notifies the rest. Returns `true` if the room is
    /// now empty.
    pub fn leave(&self, user: &User) -> bool {
        let mut state = self.state.lock();
        if state.members.remove(&user.id).is_none() {
            return state.members.is_empty();
        }

        if state.members.is_empty() {
            state.empty_since = Some(Instant::now());
            return true;
        }

        let info = self.info_locked(&state);
        let count = info.members.len();
        let removed = encode(&ServerMessage::event(
            user.clone(),
            ServerEvent::RoomRemoved { room: info },
        ));
        match (removed, self.count_frame(user, count)) {
            (Ok(removed), Ok(count)) => {
                state.broadcast(&removed, None);
                state.broadcast(&count, None);
            }
            (Err(e), _) | (_, Err(e)) => warn!(room = %self.id, "failed to encode leave: {e}"),
        }
        false
    }

    /// Applies `change` to history and sends `frame` to every member except
    /// the sender, under a single lock so history and the live stream agree.
    pub fn publish(&self, sender: Uuid, frame: &Message, change: HistoryChange) {
        let mut state = self.state.lock();
        match change {
            HistoryChange::None => {}
            HistoryChange::Record(event) => state.history.record(*event),
            HistoryChange::Delete(ids) => state.history.delete(&ids),
        }
        state.broadcast(frame, Some(sender));
    }

    /// True if the room has had no members for at least `ttl`.
    pub fn is_idle(&self, now: Instant, ttl: std::time::Duration) -> bool {
        let state = self.state.lock();
        state.members.is_empty()
            && state
                .empty_since
                .is_some_and(|since| now.saturating_duration_since(since) >= ttl)
    }

    fn count_frame(&self, user: &User, count: usize) -> Result<Message, serde_json::Error> {
        encode(&ServerMessage::event(
            user.clone(),
            ServerEvent::RoomMembersCount {
                room_id: self.id.clone(),
                count: u32::try_from(count).unwrap_or(u32::MAX),
            },
        ))
    }
}
