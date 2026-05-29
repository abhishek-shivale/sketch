# Make `sketch` a codebase to be proud of

## Context

`sketch` is a real-time collaborative drawing server (Axum + Tokio + WebSocket,
in-memory state, ~1000 LOC over 4 files). It works, but it carries correctness
bugs, zero tests, heavy duplication, an identity-spoofing security hole, and
unbounded memory growth. The goal is to take it from "works" to
"production-grade and idiomatic" — efficient memory, solid security, clean
abstractions, and real test coverage. This document sequences that work into
independently-shippable phases.

### Honest scope notes (read first)
- **`unsafe`: deliberately excluded.** Nothing here needs it. Adding it would
  reduce safety with zero performance gain (the hot paths are network + lock
  bound, not CPU). Mastery = not using it here.
- **`macro_rules!`: real fit.** Collapses the 12 near-identical handler arms.
- **Proc-macro: optional learning only.** Needs a second crate; overkill for one
  binary. Listed at the end as a stretch, not a requirement.
- **Broadcast clones are already cheap.** `Message::Text(Utf8Bytes)` is backed by
  refcounted `bytes::Bytes` — per-recipient `.clone()` is O(1), not a copy. The
  real memory wins are bounding history and `Arc<str>` room ids, NOT chasing
  these clones.

---

## Phase 0 — Tooling foundation (do first)
Make the project enforce its own quality bar.

- Add `rustfmt.toml` (default is fine) and a `clippy.toml`; run
  `cargo clippy --all-targets -- -D warnings` clean.
- Add `[dev-dependencies]`: `tokio-tungstenite` (WS client for integration
  tests), `serde_json` already present.
- Add `[profile.release]` with `lto = "thin"`, `codegen-units = 1`.
- Update `.github/workflows/deploy.yml`: add `cargo fmt --check`,
  `cargo clippy -D warnings`, `cargo test` **before** the release build.
- Files: `Cargo.toml`, new `rustfmt.toml`, `.github/workflows/deploy.yml`.

## Phase 1 — Correctness / concurrency bugs
These are real, verified-in-source defects.

1. **Heartbeat timeout leaks room state** (`room.rs:46`). On 60s no-pong the
   spawned task removes the user from `users` only — room membership + history
   are never cleaned, and the receiver loop keeps running on a dead socket.
   Fix: have the heartbeat signal the main loop to exit and run the *full*
   `clean_up`, instead of doing a partial `users`-only remove. Reuse the
   existing `watch` channel (`room.rs:24`) in the other direction, or have
   timeout call `clean_up(key, &state)`.
2. **Unbounded history growth** (`room.rs:122,144,166`). `CanvasUpdate`/`Move`/
   `Duplicate` are appended forever; only `CanvasAdd` is GC'd on delete. Bound
   it: per-room cap (e.g. keep latest state per shape id, or cap Vec length with
   compaction). Decide policy in Phase 2 refactor.
3. **Fragile lock ordering** (`clean_up`, `room.rs:484-511`). The None-branch
   holds the `rooms` guard across `users.lock().await`. Establish ONE global
   lock order (recommend `rooms -> history -> users`) and make every nested
   acquisition follow it; drop guards before acquiring the next where possible.
   Document the order in a comment at the top of `room.rs`.
4. **Panic points**: `convert()` `.expect("Parsing Fail")` (`utils.rs:310`) ->
   return `Result` or log+skip; startup `.unwrap()` (`main.rs:102,105`) -> log
   and exit cleanly.
5. **Swallowed send errors**: `send_message` `match { _ => {} }`
   (`room.rs:411`) -> handle `Err` (log + mark for cleanup) like the other
   broadcasters do.

## Phase 2 — Refactor & dedup (macros land here)
- **Collapse the 12 match arms** in `room.rs:56-360`. Introduce a
  `macro_rules! canvas_action_handler` for the Add/Update/Move/Duplicate group
  (identical except constructor + `EventKind`):
  ```rust
  macro_rules! canvas_action_handler {
      ($parsed:expr, $state:expr, $action:expr, $ctor:path, $kind:expr) => {{
          let user_id = $parsed.user.id;
          let room_id = $action.room_id.clone();
          let send_data = $ctor($parsed.user, $action);
          broadcast_in_room(&room_id, &send_data, $state, Some(user_id)).await;
          add_event_history(send_data, $kind, room_id, $state).await;
      }};
  }
  ```
  Arm becomes one line:
  `CanvasAdd { action } => canvas_action_handler!(parsed, &state, action, Data::canvas_add, EventKind::CanvasAdd),`
- Remove dead code: `_broadcast` (`room.rs:391`), `_disconnected`
  (`utils.rs:166`), commented `// drop(rooms_state)` (`room.rs:491`).
- De-duplicate `EventKind` vs `MessageEvents` discriminants — derive `EventKind`
  from the event or keep one source of truth.
- Split `room.rs` into `handlers.rs` (match arms) + `broadcast.rs` (transport)
  for altitude/readability.

## Phase 3 — Memory efficiency
- **Bound history** (policy from Phase 1.2): store the *current* shape state in a
  `HashMap<shape_id, Action>` per room instead of an ever-growing event log, OR
  cap + compact. This is the single biggest memory win.
- **Room ids as `Arc<str>`** instead of `String` cloned at ~20 call sites — keys
  in `Rooms`/`History` maps become `Arc<str>`; clone is a refcount bump.
- **Cap `image_data`** (base64) size — currently unbounded `Option<String>` can
  balloon per-event memory (ties into Phase 5 limits).
- Skip micro-optimizing broadcast clones (already cheap, see scope note).

## Phase 4 — Production hardening
- Replace `println!`/`eprintln!` with `tracing` + `tracing-subscriber`
  (structured spans per connection, env-filtered levels).
- Config via env (`figment` or plain `std::env`): bind addr/port, timeouts,
  limits, allowed origins. Stop hardcoding `127.0.0.1:3000` (`main.rs:100`).
- Graceful shutdown: `axum::serve(...).with_graceful_shutdown(...)` on
  SIGINT/SIGTERM; drain connections.
- WebSocket message/frame size limits via axum `WebSocketUpgrade` config.

## Phase 5 — Security
1. **Identity spoofing (important).** Handlers use client-supplied
   `parsed.user.id` (`room.rs:77,89,...`) as identity. A client can set any id /
   name / color and impersonate. Fix: bind identity to the server-assigned
   `key` (`room.rs:22`); ignore/override client-sent id, or validate it equals
   `key`. Carry `key` into handlers rather than trusting the payload.
2. **CORS lockdown**: replace `CorsLayer::permissive()` (`main.rs:32`) with an
   explicit allowed-origin list from config.
3. **Input validation**: bound `points.len()`, string lengths, `image_data`
   size; reject oversized/malformed messages instead of broadcasting them.
4. **Rate limiting**: per-connection cap on canvas/cursor events to stop flood
   DoS.
5. **Room access**: optional room tokens/passwords so arbitrary clients can't
   join/observe any room id.

## Phase 6 — Tests (cross-cutting, build alongside each phase)
- **Unit**: serde round-trips for every `MessageEvents` variant (lock in the
  asymmetric camelCase-in / snake_case-out contract); history GC logic
  (`main.rs:41-97`) extracted into a pure, testable fn; `clean_up` state
  transitions.
- **Integration**: spin the server on an ephemeral port, connect 2+
  `tokio-tungstenite` clients, assert join/draw/broadcast/leave + heartbeat
  timeout cleanup actually removes room membership (regression for Phase 1.1).
- Target: every Phase 1 bug gets a failing test first (TDD), then the fix.

## Stretch (optional, learning) — proc-macro
Only if wanted for its own sake: a `#[derive(EventKind)]` proc-macro (new
`sketch-macros` crate) to auto-generate `EventKind` from `MessageEvents`,
eliminating the parallel enum. Not required; `macro_rules!` + manual mapping is
enough for production.

---

## Suggested order
Phase 0 -> 1 (+tests per bug) -> 2 -> 5.1 (spoofing) -> 3 -> 4 -> 5 rest -> 6 fill-in.
Each phase compiles, passes `clippy -D warnings`, and is independently
commitable.

## Verification
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
  green after each phase.
- Manual: `cargo run`, open two browser tabs / two `tokio-tungstenite` clients,
  draw in one, confirm it appears in the other; kill one client's network and
  confirm (via `/count` + `/rooms`) the member is removed after timeout
  (Phase 1.1 regression).
- Memory: run a long draw+update session, watch RSS stays bounded (Phase 3).
- Security: attempt to send a forged `user.id`, confirm server uses its own key
  (Phase 5.1).
