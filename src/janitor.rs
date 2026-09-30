//! Background cleanup cycle.

use std::sync::Arc;

use tokio::time::{self, Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::hub::Hub;

/// Periodically evicts rooms that have been empty for longer than the idle
/// TTL and logs registry statistics. Returns when `shutdown` fires.
pub async fn run(hub: Arc<Hub>, shutdown: CancellationToken) {
    let period = hub.config().janitor_interval;
    let mut tick = time::interval_at(Instant::now() + period, period);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = tick.tick() => { sweep(&hub); }
        }
    }
    debug!("janitor stopped");
}

pub fn sweep(hub: &Hub) -> usize {
    let evicted = hub.evict_idle(Instant::now());
    let stats = hub.stats();
    if evicted > 0 {
        info!(evicted, "evicted idle rooms");
    }
    debug!(
        connections = stats.connections,
        rooms = stats.rooms,
        members = stats.members,
        shapes = stats.shapes,
        history_bytes = stats.history_bytes,
        "janitor sweep"
    );
    evicted
}
