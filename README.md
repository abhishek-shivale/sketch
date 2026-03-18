# SketchSync

Real-time collaborative drawing server built with Rust, Axum, and Tokio. The frontend is a static build served from the `public/` directory.

## Stack

- **[Axum](https://github.com/tokio-rs/axum)** — HTTP + WebSocket server
- **[Tokio](https://tokio.rs)** — async runtime
- **[Serde / serde_json](https://serde.rs)** — JSON serialisation
- **[chrono](https://docs.rs/chrono)** — UTC timestamps on history events
- **[uuid](https://docs.rs/uuid)** — connection and event IDs
- **[tower-http](https://docs.rs/tower-http)** — static file serving (`ServeDir`)

## Running

```bash
# Build the frontend first, output to public/
# (see frontend repo — builds to public/ via vite build --outDir ../public)

cargo run
# Listens on http://127.0.0.1:3000
```

The server binds to `127.0.0.1:3000`. All routes not matching `/ws` or `/count` fall through to `ServeDir::new("public")` which serves `index.html` for directory requests (SPA fallback included).

## Routes

| Method | Path | Description |
|--------|------|-------------|
| `GET / any` | `/ws` | WebSocket upgrade — all real-time traffic |
| `GET` | `/count` | Returns `{ activeUsers, activeRooms }` — polled by the landing page every 10 s |
| `*` | `/*` | Static files from `public/` with index.html fallback |

## State

All state lives in `AppState` — three `Arc<Mutex<HashMap>>` shared across every connection:

```
users   →  Uuid        → SplitSink<WebSocket>   (one entry per open connection)
rooms   →  room_id     → Room { id, members, created_by }
history →  room_id     → Vec<HistoryEvent>
```

No database. Everything is in memory and is dropped when the process exits or when the last member leaves a room.

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

The `MessageEvents` enum has asymmetric rename rules:

```rust
#[serde(rename_all(serialize = "snake_case", deserialize = "camelCase"))]
```

This means:

| Direction | Variant key format | Field format |
|-----------|--------------------|--------------|
| Server → Client (serialize) | `snake_case` — `canvas_add`, `room_joined` | `snake_case` — `room_id`, `fill_color` |
| Client → Server (deserialize) | `camelCase` — `canvasAdd`, `roomJoined` | `snake_case` — `room_id`, `fill_color` |

**Only variant names are renamed. Fields inside each variant are never renamed** — they stay exactly as declared in Rust (`snake_case`) in both directions. `Action`, `Room`, `Chat`, and `Reaction` structs have no `rename_all` so they are `snake_case` in both directions.

### Connection lifecycle

```
Client connects
  ← { key: "connected", user: { id: "<server-assigned-uuid>", ... } }

Client sends RoomJoined
  → { key: "message", value: { events: { roomJoined: { room: { id, members, created_by } } } }, user: ... }

Server responds with two messages:
  ← broadcast_in_room (peers):  room_joined with history: null
  ← broadcast_to_user (self):   room_joined with history: [HistoryEvent, ...]

Client unmounts / tab closes
  → { key: "message", value: { events: { roomRemoved: { room: ... } } }, user: ... }
  → { key: "disconnected", user: ... }
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
| `roomRemoved` | `room_removed` | Remaining members |
| `roomCreated` | `room_created` | Creating user only |

**Utility events:**

| Client → Server | Server → Client | Behaviour |
|-----------------|-----------------|-----------|
| `roomMembersCount` `{ room_id, count }` | `room_members_count` `{ room_id, count }` | Echoed to all room members including sender — used for member count sync polling |
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

Every canvas event (`CanvasAdd`, `CanvasUpdate`, `CanvasMove`, `CanvasDuplicate`, `CanvasDelete`) is appended to an in-memory history log keyed by `room_id`. History is returned to a user when they join a room so the canvas state is restored. History is deleted when the last member leaves the room.

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
  main.rs     — server setup, route registration, ServeDir
  state.rs    — AppState, HistoryEvent, type aliases
  utils.rs    — all message types (Data, MessageEvents, Action, Room, Chat, Reaction, ...)
  room.rs     — WebSocket handler, broadcast helpers, clean_up
public/       — built frontend static files (not in this repo)
```

## Known limitations / pending

- **No persistence** — state is lost on restart
- **No heartbeat** — dead sockets are only detected on the next failed write
- **History grows unboundedly** — no rolling time window implemented yet
- **`RoomCreated` does not authoritatively add the creator as a member** — the creator must also send `RoomJoined`