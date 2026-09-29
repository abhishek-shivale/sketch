//! Compacted, size-bounded canvas history for one room.
//!
//! Instead of an append-only event log, history keeps at most two events per
//! shape: the one that created it (`canvas_add` / `canvas_duplicate`) and the
//! most recent mutation (`canvas_update` / `canvas_move`). A delete drops the
//! shape entirely. Replaying the snapshot in time order produces the same
//! canvas as replaying the full log, with memory bounded by the number of
//! live shapes rather than the number of edits.

use std::{collections::HashSet, sync::Arc};

use indexmap::IndexMap;

use crate::protocol::{EventKind, HistoryEvent, ShapeId};

/// Fixed per-event overhead on top of the action payload (ids, timestamps,
/// user fields, map entry).
const EVENT_OVERHEAD: usize = 256;

#[derive(Debug)]
pub struct CanvasHistory {
    /// Shapes in creation order, so the oldest is evicted first.
    shapes: IndexMap<ShapeId, ShapeLog>,
    bytes: usize,
    max_shapes: usize,
    max_bytes: usize,
}

#[derive(Debug, Default)]
struct ShapeLog {
    created: Option<Arc<HistoryEvent>>,
    latest: Option<Arc<HistoryEvent>>,
}

impl ShapeLog {
    fn bytes(&self) -> usize {
        [&self.created, &self.latest]
            .into_iter()
            .flatten()
            .map(|e| event_bytes(e))
            .sum()
    }
}

fn event_bytes(event: &HistoryEvent) -> usize {
    EVENT_OVERHEAD
        + event.action().map_or(0, |a| a.approx_bytes())
        + event.event_data.user.name.len()
        + event.event_data.user.color.len()
}

impl CanvasHistory {
    pub fn new(max_shapes: usize, max_bytes: usize) -> Self {
        Self {
            shapes: IndexMap::new(),
            bytes: 0,
            max_shapes,
            max_bytes,
        }
    }

    /// Records a canvas add/duplicate/update/move event. Other events are
    /// ignored: they carry no canvas state.
    pub fn record(&mut self, event: HistoryEvent) {
        let Some(shape_id) = event.action().map(|a| a.id.clone()) else {
            return;
        };
        let event = Arc::new(event);
        let added = event_bytes(&event);

        match event.event_type {
            EventKind::CanvasAdd | EventKind::CanvasDuplicate => {
                // A re-add with a known id starts that shape's log over.
                self.remove(&shape_id);
                self.shapes.insert(
                    shape_id,
                    ShapeLog {
                        created: Some(event),
                        latest: None,
                    },
                );
                self.bytes += added;
            }
            EventKind::CanvasUpdate | EventKind::CanvasMove => {
                let log = self.shapes.entry(shape_id).or_default();
                if let Some(previous) = log.latest.replace(event) {
                    self.bytes -= event_bytes(&previous);
                }
                self.bytes += added;
            }
            _ => return,
        }

        self.enforce_limits();
    }

    pub fn delete<'a>(&mut self, ids: impl IntoIterator<Item = &'a ShapeId>) {
        let ids: HashSet<&ShapeId> = ids.into_iter().collect();
        match ids.len() {
            0 => {}
            1 => {
                if let Some(id) = ids.into_iter().next() {
                    self.remove(id);
                }
            }
            _ => {
                let mut freed = 0;
                self.shapes.retain(|id, log| {
                    let keep = !ids.contains(id);
                    if !keep {
                        freed += log.bytes();
                    }
                    keep
                });
                self.bytes -= freed;
            }
        }
    }

    /// All retained events in the order they happened.
    pub fn snapshot(&self) -> Vec<Arc<HistoryEvent>> {
        let mut events: Vec<Arc<HistoryEvent>> = self
            .shapes
            .values()
            .flat_map(|log| [&log.created, &log.latest])
            .flatten()
            .cloned()
            .collect();
        events.sort_by_key(|e| e.event_time);
        events
    }

    pub fn len(&self) -> usize {
        self.shapes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    fn remove(&mut self, id: &ShapeId) {
        if let Some(log) = self.shapes.shift_remove(id) {
            self.bytes -= log.bytes();
        }
    }

    fn enforce_limits(&mut self) {
        while self.shapes.len() > self.max_shapes || self.bytes > self.max_bytes {
            let Some((_, log)) = self.shapes.shift_remove_index(0) else {
                break;
            };
            self.bytes -= log.bytes();
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::protocol::{Action, Point, ServerEvent, Tool, User};

    fn action(id: &str, x: f64) -> Action {
        Action {
            id: id.into(),
            room_id: "room".into(),
            tool: Tool::Rectangle,
            points: vec![Point { x, y: 0.0 }],
            color: "#000".into(),
            fill_color: "transparent".into(),
            size: 2.0,
            opacity: 100.0,
            timestamp: Utc::now(),
            text: None,
            image_data: None,
            image_height: None,
            image_width: None,
        }
    }

    fn event(event: ServerEvent) -> HistoryEvent {
        HistoryEvent::new("room".into(), User::anonymous(Uuid::nil()), event)
    }

    fn add(id: &str) -> HistoryEvent {
        event(ServerEvent::CanvasAdd {
            action: action(id, 0.0),
        })
    }

    fn update(id: &str, x: f64) -> HistoryEvent {
        event(ServerEvent::CanvasUpdate {
            action: action(id, x),
        })
    }

    fn kinds(history: &CanvasHistory) -> Vec<(EventKind, String, f64)> {
        history
            .snapshot()
            .iter()
            .map(|e| {
                let a = e.action().unwrap();
                (e.event_type, a.id.to_string(), a.points[0].x)
            })
            .collect()
    }

    #[test]
    fn keeps_only_latest_mutation_per_shape() {
        let mut h = CanvasHistory::new(100, usize::MAX);
        h.record(add("a"));
        for x in 1..=50 {
            h.record(update("a", x as f64));
        }
        assert_eq!(
            kinds(&h),
            [
                (EventKind::CanvasAdd, "a".into(), 0.0),
                (EventKind::CanvasUpdate, "a".into(), 50.0)
            ]
        );
    }

    #[test]
    fn delete_drops_every_event_for_the_shape() {
        let mut h = CanvasHistory::new(100, usize::MAX);
        h.record(add("a"));
        h.record(update("a", 1.0));
        h.record(add("b"));
        h.record(add("c"));

        h.delete([&ShapeId::from("a")]);
        assert_eq!(h.len(), 2);

        h.delete([&ShapeId::from("b"), &ShapeId::from("c")]);
        assert!(h.is_empty());
        assert_eq!(h.bytes(), 0);
    }

    #[test]
    fn byte_accounting_stays_consistent() {
        let mut h = CanvasHistory::new(100, usize::MAX);
        h.record(add("a"));
        h.record(update("a", 1.0));
        h.record(update("a", 2.0));
        h.record(add("a"));
        let expected: usize = h.snapshot().iter().map(|e| event_bytes(e)).sum();
        assert_eq!(h.bytes(), expected);
    }

    #[test]
    fn evicts_oldest_shapes_over_the_shape_cap() {
        let mut h = CanvasHistory::new(2, usize::MAX);
        h.record(add("a"));
        h.record(add("b"));
        h.record(add("c"));
        let ids: Vec<String> = kinds(&h).into_iter().map(|(_, id, _)| id).collect();
        assert_eq!(ids, ["b", "c"]);
    }

    #[test]
    fn evicts_oldest_shapes_over_the_byte_cap() {
        let one = event_bytes(&add("a"));
        let mut h = CanvasHistory::new(100, one * 2);
        h.record(add("a"));
        h.record(add("b"));
        h.record(add("c"));
        assert_eq!(h.len(), 2);
        assert!(h.bytes() <= one * 2);
    }

    #[test]
    fn ignores_non_canvas_events() {
        let mut h = CanvasHistory::new(100, usize::MAX);
        h.record(event(ServerEvent::RoomMembersCount {
            room_id: "room".into(),
            count: 3,
        }));
        assert!(h.is_empty());
    }
}
