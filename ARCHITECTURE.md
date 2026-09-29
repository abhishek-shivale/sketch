# Sketch server architecture

Sketch is a real-time, Excalidraw-style collaborative whiteboard. This
document describes how the Rust server is put together and why. The frontend
([doodle-duo-space](https://github.com/abhishek-shivale/doodle-duo-space)) is
a static Vite/React build served from `public/`. The wire protocol it speaks
is a hard compatibility constraint: every change on the server side keeps the
JSON shapes the frontend already sends and expects.

## Goals

1. **No global lock on the hot path.** One slow or dead client must never
   stall broadcasts for anyone else.
2. **Bounded memory.** Every in-memory structure has a cap: per-connection
   send queues, per-room history, message size, number of rooms.
3. **Errors are values.** No `unwrap`/`expect` on runtime paths. Protocol
   errors are typed, logged, and reported back to the client; they never
   take the task down.
4. **Clean lifecycle.** Every connection, room and background task has a
   defined start, an owner, and a cleanup path (disconnect, heartbeat
   timeout, slow consumer, idle-room eviction, process shutdown).
5. **Server-owned identity.** The server assigns a connection id and never
   trusts ids sent by the client.

## Module layout

```
src/
  main.rs            process bootstrap: config -> tracing -> runtime -> serve
  lib.rs             build_router() / serve(): used by main and by tests
  config.rs          Config loaded from SKETCH_* environment variables
  error.rs           AppError (startup) and ProtocolError (per message)
  http.rs            REST endpoints: /health, /count, /rooms
  janitor.rs         background cleanup cycle
  protocol/
    model.rs         shared payload types: Action, Point, Tool, RoomInfo, Chat...
    inbound.rs       what clients send   (variant names camelCase)
    outbound.rs      what the server sends (variant names snake_case), HistoryEvent
  hub/
    mod.rs           Hub: the room registry and connection counters
    room.rs          Room: members, broadcast, history, all behind one short lock
    history.rs       CanvasHistory: compacted, size-bounded canvas log
  ws/
    mod.rs           WebSocket upgrade handler
    connection.rs    per-connection lifecycle: reader, writer task, heartbeat
    session.rs       event dispatch for one connection (join/leave/draw/chat)
    rate_limit.rs    token bucket used to throttle abusive clients
```

## Concurrency model

The server runs on Tokio's multi-threaded runtime (worker count is
configurable with `SKETCH_WORKER_THREADS`, defaulting to one per core).

```
                    ┌──────────────────────────── Hub ───────────────────────────┐
                    │ rooms: DashMap<RoomId, Arc<Room>>   (sharded, lock-free-ish) │
                    │   Room { Mutex<RoomState { members, history, last_active }> }│
                    └─────────────────────────────────────────────────────────────┘
                           ▲ join/leave/broadcast (sync, never held across .await)
                           │
 socket ──► reader task ──► Session::handle(event) ──► room.broadcast(msg)
                                                          │ try_send (non-blocking)
                                                          ▼
                             mpsc (bounded) ──► writer task ──► socket
```

* **One writer task per connection** owns the socket sink and drains a
  bounded `mpsc` queue. Broadcasting is `try_send` into each member's queue:
  O(members), non-blocking, never awaits another client's network I/O.
* **Slow consumers are disconnected.** If a member's queue is full, the
  broadcaster cancels that member's `CancellationToken`; its connection
  shuts down and the normal cleanup path runs.
* **Rooms are sharded.** The registry is a `DashMap`; each room has its own
  `parking_lot::Mutex`. Traffic in one room never contends with another.
* **Locks are synchronous and short.** No lock is held across an `.await`.
  Lock order is always *registry shard -> room*, which rules out deadlock.
* **Messages are serialized once per broadcast.** The JSON is encoded to a
  `Utf8Bytes` (refcounted) and cloned into each queue for free.

## Connection lifecycle

1. Upgrade (`/ws`) with frame and message size limits from config.
2. Server assigns a `Uuid`, spawns the writer, sends `{key: "connected"}`.
3. Reader loop `select!`s over: incoming frames, heartbeat tick, the
   connection's cancel token (slow consumer / server shutdown).
4. Heartbeat: a `Ping` goes out every `heartbeat_interval`; any inbound
   frame (pong, text, client `{key:"ping"}`) refreshes liveness. No traffic
   for `heartbeat_timeout` means the connection is dead and gets closed.
5. Exit, whatever the cause: leave the room (peers get `room_removed` and an
   updated `room_members_count`), cancel the token, drop the queue, and let
   the writer flush and close the socket.

## Rooms and history

* A room is created on the first `roomJoined` and has one member set.
* The client-sent `user.id` and `room_id` are **not** trusted: events are
  attributed to the server-assigned id and routed to the room the connection
  actually joined. Events for any other room are rejected.
* `room_members_count` is computed by the server; the client's number is
  treated only as a request for the current count.
* **History is compacted**, not an append-only log:
  * per shape we keep the creating event (`canvas_add` / `canvas_duplicate`)
    and only the latest mutation (`canvas_update` / `canvas_move`);
  * `canvas_delete` drops everything recorded for those shapes;
  * room lifecycle events are not recorded (the old server stored each
    `room_joined` *including the full history* inside history, which grew
    quadratically);
  * hard caps on shapes per room and approximate bytes per room: the oldest
    shapes are evicted first.
  Replaying the compacted log on the frontend gives the same canvas as
  replaying the full log.
* When the last member leaves, the room is kept for `room_idle_ttl` so a page
  refresh doesn't wipe the canvas. After that the janitor evicts it.

## Cleanup cycle (janitor)

A background task runs every `janitor_interval`:

* evicts rooms that have been empty longer than `room_idle_ttl`
  (`DashMap::remove_if`, so a concurrent join cannot be lost);
* logs registry statistics (rooms, connections, retained history bytes).

Dead connections are handled by their own heartbeat, not the janitor, so the
cleanup for each resource lives next to whatever owns it.

## Error handling

* `AppError`: startup failures (bad config, bind error, I/O). `main` logs it
  and exits with a non-zero status.
* `ProtocolError`: malformed JSON, oversized payloads, validation failures,
  acting before joining a room, room mismatch, rate limit, server full.
  Returned from `Session::handle`, logged with the connection id, and sent
  back as `{"key":"error","message":"..."}`. The current frontend ignores
  unknown keys, so this is backward compatible.

## Shutdown

`SIGINT`/`SIGTERM` cancel a root `CancellationToken`. Axum stops accepting,
every connection token (a child of the root) fires so sockets get a close
frame, and the janitor exits.

## Configuration

All settings are environment variables with defaults. See `README.md`.

## Testing

* Unit tests: serde contracts for every event (camelCase in, snake_case out),
  history compaction, rate limiter, config parsing.
* Integration tests (`tests/`): the real router on an ephemeral port, driven
  by `tokio-tungstenite` clients: join/draw/broadcast, history on join,
  identity spoofing, cross-room injection, leave notifications.
