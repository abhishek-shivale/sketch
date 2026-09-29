# SketchSync

Real-time collaborative drawing server built with Rust, Axum, and Tokio. The frontend is a static build served from the `public/` directory.

![SketchSync preview](https://res.cloudinary.com/dygubvmg6/image/upload/v1785202553/ChatGPT_Image_Jul_28_2026_07_05_44_AM_pf8dei.png)

## Stack

- **[Axum](https://github.com/tokio-rs/axum)** — HTTP + WebSocket server
- **[Tokio](https://tokio.rs)** — async runtime
- **[Serde / serde_json](https://serde.rs)** — JSON serialisation
- **[chrono](https://docs.rs/chrono)** — UTC timestamps on history events
- **[uuid](https://docs.rs/uuid)** — connection and event IDs
- **[tower-http](https://docs.rs/tower-http)** — static file serving (`ServeDir`), CORS, request tracing
- **[DashMap](https://docs.rs/dashmap) / [parking_lot](https://docs.rs/parking_lot)** — sharded room registry, short per-room locks
- **[tracing](https://docs.rs/tracing)** — structured logs (`RUST_LOG=sketch=debug`)
- **[thiserror](https://docs.rs/thiserror)** — typed errors

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the design: concurrency model,
connection lifecycle, history compaction and the cleanup cycle.

## Running

```bash
# Rebuild public/ from the frontend repo (optional, a build is committed)
scripts/build-frontend.sh                    # clones doodle-duo-space
scripts/build-frontend.sh ../doodle-duo-space  # or use a local checkout

cargo run
# Listens on http://127.0.0.1:3000
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test          # unit tests + WebSocket integration tests in tests/
```

CI runs the same three checks, and deploys only when they pass.

## Configuration

Everything is optional and read from the environment at startup. Invalid
values stop the server with an error naming the variable.

| Variable | Default | Meaning |
|----------|---------|---------|
| `SKETCH_ADDR` | `127.0.0.1:3000` | Listen address |
| `SKETCH_PUBLIC_DIR` | `public` | Built frontend directory |
| `SKETCH_WORKER_THREADS` | one per core | Tokio worker threads |
| `SKETCH_CORS_ORIGINS` | any origin | Comma separated allow-list |
| `SKETCH_HEARTBEAT_INTERVAL_SECS` | `30` | Server ping interval |
| `SKETCH_HEARTBEAT_TIMEOUT_SECS` | `75` | Silence before a client is dropped |
| `SKETCH_SEND_QUEUE` | `512` | Outbound messages buffered per client before it is dropped as too slow |
| `SKETCH_MAX_MESSAGE_BYTES` | `16777216` | Largest accepted WebSocket message (images are sent inline as data URLs) |
| `SKETCH_RATE_PER_SEC` / `SKETCH_RATE_BURST` | `120` / `240` | Per-connection message rate limit |
| `SKETCH_MAX_ROOMS` | `10000` | Rooms held in memory |
| `SKETCH_ROOM_MAX_SHAPES` | `5000` | Shapes kept in one room's history |
| `SKETCH_ROOM_MAX_BYTES` | `33554432` | Approximate history bytes per room |
| `SKETCH_ROOM_IDLE_TTL_SECS` | `300` | How long an empty room (and its canvas) is kept |
| `SKETCH_JANITOR_INTERVAL_SECS` | `30` | Cleanup cycle period |
| `RUST_LOG` | `sketch=info` | Log filter |

## Routes

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/ws` | WebSocket upgrade — all real-time traffic |
| `GET` | `/count` | `{ activeUsers, activeRooms }` — polled by the landing page |
| `GET` | `/rooms` | `[{ id, member_count }]` |
| `GET` | `/health` | `ok` |
| `*` | `/*` | Static files from `public/`, `index.html` fallback for the SPA |

## State

All state is in memory, owned by a `Hub`:

```
Hub
  rooms: DashMap<RoomId, Arc<Room>>
    Room { Mutex<{ members: conn id -> send queue, history, empty_since }> }
  connections: AtomicUsize
```

Each connection has its own writer task and bounded send queue; broadcasting
never waits on another client's network. There is no database: state is lost
on restart, and an empty room is dropped after `SKETCH_ROOM_IDLE_TTL_SECS`.

## WebSocket Protocol

Every message is a JSON object with this envelope:

```json
{
  "key":   "message" | "connected" | "disconnected",
  "user":  { "id": "<uuid>", "name": "...", "color": "..." },
  "value": { "events": { "<variantKey>": { ...fields } } }
}
```

### Serde rename rules — important

The two directions use different variant naming, so they are separate types:
`protocol::ClientEvent` (`rename_all = "camelCase"`) for what clients send and
`protocol::ServerEvent` (`rename_all = "snake_case"`) for what the server sends.

| Direction | Variant key format | Field format |
|-----------|--------------------|--------------|
| Server → Client (serialize) | `snake_case` — `canvas_add`, `room_joined` | `snake_case` — `room_id`, `fill_color` |
| Client → Server (deserialize) | `camelCase` — `canvasAdd`, `roomJoined` | `snake_case` — `room_id`, `fill_color` |

**Only variant names are renamed. Fields inside each variant are never renamed** — they stay exactly as declared in Rust (`snake_case`) in both directions. `Action`, `RoomInfo`, `Chat`, and `Reaction` structs have no `rename_all` so they are `snake_case` in both directions.

### Server-owned fields

- `user.id` sent by the client is ignored; every event is attributed to the
  id the server assigned in `connected`.
- Events can only target the room the connection joined. Others are
  rejected with an error frame.
- `room_members_count` is computed by the server; the client's `count` is
  ignored and the reply goes to the requester. The server also pushes the
  count to the whole room on every join and leave.

### Control frames

| Direction | Frame | Meaning |
|-----------|-------|---------|
| Client → Server | `{ "key": "ping" }` | Keepalive, answered with `{ "key": "pong" }` |
| Server → Client | `{ "key": "error", "message": "..." }` | A message was rejected (bad JSON, validation, wrong room, not joined) |

### Connection lifecycle

```
Client connects
  ← { key: "connected", user: { id: "<server-assigned-uuid>", ... } }

Client sends RoomJoined
  → { key: "message", value: { events: { roomJoined: { room: { id, members, created_by } } } }, user: ... }

Server responds (atomically, under the room lock):
  ← self:  room_joined with history: [HistoryEvent, ...]
  ← peers: room_joined with history: null
  ← all:   room_members_count

Client unmounts / tab closes
  → { key: "message", value: { events: { roomRemoved: { room: ... } } }, user: ... }
  → { key: "disconnected", user: ... }

Client leaves, disconnects, or misses heartbeats
  ← remaining peers: room_removed + room_members_count
```

### Event reference

**Canvas events** — broadcast to all room members except sender:

| Client → Server | Server → Client | Payload |
|-----------------|-----------------|---------|
| `canvasAdd` | `canvas_add` | `{ action: Action }` |
| `canvasUpdate` | `canvas_update` | `{ action: Action }` |
| `canvasDuplicate` | `canvas_duplicate` | `{ action: Action }` |
| `canvasMove` | `canvas_move` | `{ action: Action }` |
| `canvasDelete` | `canvas_delete` | `{ id: string\|null, ids: string[]\|null, room_id }` |
| `canvasCursor` | `canvas_cursor` | `{ x, y, room_id }` |

**Room events:**

| Client → Server | Server → Client | Who receives |
|-----------------|-----------------|--------------|
| `roomJoined` | `room_joined` (history: null) | All existing members |
| `roomJoined` | `room_joined` (history: [...]) | Joining user only |
| `roomRemoved` / disconnect | `room_removed` | Remaining members |
| `roomCreated` | `room_created` | Creating user only |

**Utility events:**

| Client → Server | Server → Client | Behaviour |
|-----------------|-----------------|-----------|
| `roomMembersCount` `{ room_id, count }` | `room_members_count` `{ room_id, count }` | Server's own count, sent to the requester |
| `playBack` `{ room_id }` | `play_back` `{ room_id, history }` | Returns full room history to requesting user only |
| `chatMessage` | `chat_message` | Broadcast to room, sender excluded |
| `chatReaction` | `chat_reaction` | Broadcast to room, sender excluded |

### Action shape

All canvas actions share this structure (no rename — always `snake_case`):

```json
{
  "id":           "<uuid-string>",
  "room_id":      "<room-id>",
  "tool":         "pencil|text|image|line|arrow|rectangle|circle|diamond|eraser|select",
  "points":       [{ "x": 0, "y": 0 }],
  "color":        "#rrggbb",
  "fill_color":   "#rrggbb | transparent",
  "size":         1,
  "opacity":      100,
  "timestamp":    "2025-01-01T00:00:00Z",
  "text":         null,
  "image_data":   null,
  "image_height": null,
  "image_width":  null
}
```

## History

Canvas events are recorded per room and sent to users when they join (and
on `playBack`), so the canvas can be rebuilt. History is compacted as it is
written:

- per shape: the creating `canvas_add` / `canvas_duplicate`, plus only the
  **latest** `canvas_update` / `canvas_move`;
- `canvas_delete` removes everything recorded for those shapes;
- cursor, chat and room events are not recorded;
- per-room caps on shapes and bytes evict the oldest shapes first.

Replaying the compacted history gives the same canvas as replaying every
event.

`HistoryEvent` shape (serialised `snake_case`):

```json
{
  "event_id":   "<uuid>",
  "event_type": "canvas_add | canvas_update | ...",
  "event_time": "2025-01-01T00:00:00Z",
  "event_room": "<room-id>",
  "event_data": { ...full original Data envelope... }
}
```

## Project structure

```
src/
  main.rs         bootstrap: config, logging, runtime, signals
  lib.rs          router() and serve(), shared by main and the tests
  config.rs       SKETCH_* environment config
  error.rs        AppError (startup), ProtocolError (per message)
  http.rs         /health, /count, /rooms
  janitor.rs      cleanup cycle: evicts idle rooms, logs stats
  protocol/       wire types: model, inbound (camelCase), outbound (snake_case)
  hub/            room registry, Room, compacted CanvasHistory
  ws/             connection lifecycle, session dispatch, rate limiter
tests/ws.rs       end-to-end WebSocket tests
scripts/          build-frontend.sh
public/           built frontend (doodle-duo-space)
```

## Known limitations

- **No persistence**: state is lost on restart.
- **No room access control**: anyone who knows a room id can join it.
