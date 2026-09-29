//! In-memory registry of rooms and connections.
//!
//! Lock order: a registry shard (`DashMap`) is always taken before a room's
//! own mutex, never the other way around. Joining and idle eviction both run
//! under the shard lock, so a room can't be evicted while someone is joining.

mod history;
mod room;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use dashmap::DashMap;
use tokio::time::Instant;
use uuid::Uuid;

pub use history::CanvasHistory;
pub use room::{HistoryChange, Peer, Room, RoomStats};

use crate::{
    config::Config,
    error::ProtocolError,
    protocol::{RoomId, RoomInfo, User},
};

#[derive(Debug)]
pub struct Hub {
    config: Arc<Config>,
    rooms: DashMap<RoomId, Arc<Room>>,
    connections: AtomicUsize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HubStats {
    pub connections: usize,
    pub rooms: usize,
    pub members: usize,
    pub shapes: usize,
    pub history_bytes: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoomSummary {
    pub id: RoomId,
    pub member_count: usize,
}

impl Hub {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            rooms: DashMap::new(),
            connections: AtomicUsize::new(0),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Counts a connection for as long as the returned guard lives.
    pub fn track_connection(self: &Arc<Self>) -> ConnectionGuard {
        self.connections.fetch_add(1, Ordering::Relaxed);
        ConnectionGuard(Arc::clone(self))
    }

    pub fn connection_count(&self) -> usize {
        self.connections.load(Ordering::Relaxed)
    }

    pub fn room_count(&self) -> usize {
        self.rooms.len()
    }

    pub fn get(&self, id: &str) -> Option<Arc<Room>> {
        self.rooms.get(id).map(|r| Arc::clone(r.value()))
    }

    /// Creates the room if needed without joining it.
    pub fn create(&self, id: RoomId, creator: Uuid) -> Result<RoomInfo, ProtocolError> {
        self.ensure_capacity(&id)?;
        let room = self
            .rooms
            .entry(id.clone())
            .or_insert_with(|| Arc::new(Room::new(id, creator, &self.config)))
            .value()
            .clone();
        Ok(room.info())
    }

    /// Joins (creating if needed) the room `id`.
    pub fn join(&self, id: RoomId, user: &User, peer: Peer) -> Result<Arc<Room>, ProtocolError> {
        self.ensure_capacity(&id)?;
        let entry = self
            .rooms
            .entry(id.clone())
            .or_insert_with(|| Arc::new(Room::new(id, user.id, &self.config)));
        let room = Arc::clone(entry.value());
        // Still holding the shard lock: the janitor can't evict the room
        // between creating it and adding the member.
        room.join(user, peer).map_err(ProtocolError::Encode)?;
        drop(entry);
        Ok(room)
    }

    /// Removes `id` if it has been empty for at least `ttl`.
    pub fn evict_if_idle(&self, id: &str, now: Instant) -> bool {
        let ttl = self.config.room_idle_ttl;
        self.rooms
            .remove_if(id, |_, room| room.is_idle(now, ttl))
            .is_some()
    }

    /// Removes every room that has been empty for at least the idle TTL.
    /// Returns the number of rooms evicted.
    pub fn evict_idle(&self, now: Instant) -> usize {
        let ttl = self.config.room_idle_ttl;
        let before = self.rooms.len();
        self.rooms.retain(|_, room| !room.is_idle(now, ttl));
        before.saturating_sub(self.rooms.len())
    }

    pub fn summaries(&self) -> Vec<RoomSummary> {
        self.rooms
            .iter()
            .map(|r| RoomSummary {
                id: r.key().clone(),
                member_count: r.value().member_count(),
            })
            .collect()
    }

    pub fn stats(&self) -> HubStats {
        // Collect first so no shard lock is held while room locks are taken
        // one by one.
        let rooms: Vec<Arc<Room>> = self.rooms.iter().map(|r| Arc::clone(r.value())).collect();
        let mut stats = HubStats {
            connections: self.connection_count(),
            rooms: rooms.len(),
            ..HubStats::default()
        };
        for room in rooms {
            let s = room.stats();
            stats.members += s.members;
            stats.shapes += s.shapes;
            stats.history_bytes += s.history_bytes;
        }
        stats
    }

    fn ensure_capacity(&self, id: &str) -> Result<(), ProtocolError> {
        // Soft cap: checked before taking the entry lock, because
        // `DashMap::len` must not be called while a shard is locked.
        let max = self.config.max_rooms;
        if !self.rooms.contains_key(id) && self.rooms.len() >= max {
            return Err(ProtocolError::TooManyRooms(max));
        }
        Ok(())
    }
}

/// Decrements the live connection count when dropped.
#[derive(Debug)]
pub struct ConnectionGuard(Arc<Hub>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.connections.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::*;

    fn hub(ttl: Duration, max_rooms: usize) -> Hub {
        Hub::new(Arc::new(Config {
            room_idle_ttl: ttl,
            max_rooms,
            ..Config::default()
        }))
    }

    fn member(queue: usize) -> (User, Peer, mpsc::Receiver<axum::extract::ws::Message>) {
        let id = Uuid::new_v4();
        let (tx, rx) = mpsc::channel(queue);
        let peer = Peer {
            id,
            tx,
            kick: CancellationToken::new(),
        };
        (User::anonymous(id), peer, rx)
    }

    #[tokio::test]
    async fn empty_rooms_are_evicted_after_ttl() {
        let hub = hub(Duration::from_secs(60), 10);
        let (user, peer, _rx) = member(8);
        let room = hub.join("r".into(), &user, peer).unwrap();

        assert_eq!(hub.evict_idle(Instant::now()), 0, "occupied room kept");
        assert!(room.leave(&user));
        assert_eq!(hub.evict_idle(Instant::now()), 0, "within TTL");
        assert_eq!(hub.evict_idle(Instant::now() + Duration::from_secs(61)), 1);
        assert_eq!(hub.room_count(), 0);
    }

    #[tokio::test]
    async fn rejoining_cancels_eviction() {
        let hub = hub(Duration::from_secs(60), 10);
        let (user, peer, _rx) = member(8);
        let room = hub.join("r".into(), &user, peer.clone()).unwrap();
        room.leave(&user);
        hub.join("r".into(), &user, peer).unwrap();
        assert_eq!(hub.evict_idle(Instant::now() + Duration::from_secs(600)), 0);
    }

    #[tokio::test]
    async fn room_cap_is_enforced() {
        let hub = hub(Duration::from_secs(60), 1);
        let (user, peer, _rx) = member(8);
        hub.join("a".into(), &user, peer.clone()).unwrap();
        assert!(matches!(
            hub.join("b".into(), &user, peer.clone()),
            Err(ProtocolError::TooManyRooms(1))
        ));
        // Existing rooms stay joinable at capacity.
        hub.join("a".into(), &user, peer).unwrap();
    }

    #[tokio::test]
    async fn slow_consumers_are_kicked() {
        let hub = hub(Duration::from_secs(60), 10);
        let (slow_user, slow, _slow_rx) = member(1);
        let kick = slow.kick.clone();
        let (fast_user, fast, _fast_rx) = member(64);

        let room = hub.join("r".into(), &slow_user, slow).unwrap();
        hub.join("r".into(), &fast_user, fast).unwrap();
        assert!(kick.is_cancelled(), "slow peer's single slot overflowed");
        assert_eq!(room.member_count(), 2, "removal is left to the connection");
    }
}
