//! End-to-end tests: the real router on an ephemeral port, driven by
//! WebSocket clients speaking the frontend's protocol.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sketch::config::Config;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

const WAIT: Duration = Duration::from_secs(3);

struct Server {
    addr: SocketAddr,
    shutdown: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    async fn start(config: Config) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let shutdown = CancellationToken::new();
        let task = tokio::spawn({
            let shutdown = shutdown.clone();
            async move {
                sketch::serve(listener, Arc::new(config), shutdown)
                    .await
                    .unwrap()
            }
        });
        Self {
            addr,
            shutdown,
            task,
        }
    }

    async fn default() -> Self {
        Self::start(Config::default()).await
    }

    async fn client(&self) -> Client {
        let (ws, _) = connect_async(format!("ws://{}/ws", self.addr))
            .await
            .unwrap();
        let mut client = Client {
            ws,
            id: String::new(),
        };
        let hello = client.recv().await;
        assert_eq!(hello["key"], "connected");
        client.id = hello["user"]["id"].as_str().unwrap().to_string();
        client
    }

    async fn get(&self, path: &str) -> Value {
        let mut stream = TcpStream::connect(self.addr).await.unwrap();
        let request = format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        let body = response.split("\r\n\r\n").nth(1).unwrap();
        serde_json::from_str(body).unwrap_or(Value::String(body.to_string()))
    }
}

struct Client {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    id: String,
}

impl Client {
    async fn send(&mut self, value: Value) {
        self.ws
            .send(Message::text(value.to_string()))
            .await
            .unwrap();
    }

    /// Sends a regular event with the frontend's envelope.
    async fn event(&mut self, events: Value) {
        let id = self.id.clone();
        self.send(json!({
            "key": "message",
            "value": { "events": events },
            "user": { "id": id, "name": "tester", "color": "#123456" }
        }))
        .await;
    }

    /// Next JSON message, skipping transport-level frames.
    async fn recv(&mut self) -> Value {
        self.try_recv(WAIT).await.expect("expected a message")
    }

    async fn try_recv(&mut self, wait: Duration) -> Option<Value> {
        loop {
            match timeout(wait, self.ws.next()).await.ok()?? {
                Ok(Message::Text(text)) => return Some(serde_json::from_str(&text).unwrap()),
                Ok(Message::Close(_)) | Err(_) => return None,
                Ok(_) => continue,
            }
        }
    }

    /// Waits for an event with the given outbound variant name.
    async fn expect_event(&mut self, name: &str) -> Value {
        loop {
            let message = self.recv().await;
            if let Some(event) = message["value"]["events"].get(name) {
                let mut event = event.clone();
                event["_sender"] = message["user"]["id"].clone();
                return event;
            }
        }
    }

    /// Waits for the next control frame with the given key.
    async fn expect_key(&mut self, key: &str) -> Value {
        loop {
            let message = self.recv().await;
            if message["key"] == key {
                return message;
            }
        }
    }

    /// Round-trips a request so everything sent before it has been handled.
    async fn sync(&mut self, room: &str) {
        self.event(json!({ "playBack": { "room_id": room } })).await;
        self.expect_event("play_back").await;
    }

    async fn expect_silence(&mut self, name: &str) {
        while let Some(message) = self.try_recv(Duration::from_millis(300)).await {
            assert!(
                message["value"]["events"].get(name).is_none(),
                "unexpected {name}: {message}"
            );
        }
    }

    async fn join(&mut self, room: &str) -> Value {
        let id = self.id.clone();
        self.event(
            json!({ "roomJoined": { "room": { "id": room, "members": [id], "created_by": id } } }),
        )
        .await;
        self.expect_event("room_joined").await
    }

    async fn is_closed(&mut self) -> bool {
        loop {
            match timeout(WAIT, self.ws.next()).await {
                Err(_) => return false,
                Ok(None) | Ok(Some(Err(_))) | Ok(Some(Ok(Message::Close(_)))) => return true,
                Ok(Some(Ok(_))) => continue,
            }
        }
    }
}

fn action(id: &str, room: &str, x: f64) -> Value {
    json!({
        "id": id,
        "room_id": room,
        "tool": "rectangle",
        "points": [{ "x": x, "y": 0 }],
        "color": "#000000",
        "fill_color": "transparent",
        "size": 2,
        "opacity": 100,
        "timestamp": "2026-01-01T00:00:00.000Z",
        "text": null,
        "image_data": null,
        "image_height": null,
        "image_width": null
    })
}

#[tokio::test]
async fn drawing_reaches_peers_but_not_the_sender() {
    let server = Server::default().await;
    let mut alice = server.client().await;
    let mut bob = server.client().await;

    let joined = alice.join("r1").await;
    assert_eq!(joined["history"], json!([]));
    bob.join("r1").await;
    alice.expect_event("room_joined").await; // bob arrived

    alice
        .event(json!({ "canvasAdd": { "action": action("s1", "r1", 1.0) } }))
        .await;
    let added = bob.expect_event("canvas_add").await;
    assert_eq!(added["action"]["id"], "s1");
    assert_eq!(added["_sender"], alice.id.as_str());

    alice.expect_silence("canvas_add").await;
}

#[tokio::test]
async fn late_joiners_get_compacted_history() {
    let server = Server::default().await;
    let mut alice = server.client().await;
    alice.join("r1").await;

    alice
        .event(json!({ "canvasAdd": { "action": action("keep", "r1", 0.0) } }))
        .await;
    for x in 1..=20 {
        alice
            .event(json!({ "canvasMove": { "action": action("keep", "r1", f64::from(x)) } }))
            .await;
    }
    alice
        .event(json!({ "canvasAdd": { "action": action("gone", "r1", 0.0) } }))
        .await;
    alice
        .event(json!({ "canvasDelete": { "id": "gone", "ids": null, "room_id": "r1" } }))
        .await;

    // Round-trip through the server so everything above has been applied.
    alice.sync("r1").await;

    let mut carol = server.client().await;
    let joined = carol.join("r1").await;
    let history = joined["history"].as_array().unwrap();
    let summary: Vec<(String, String, f64)> = history
        .iter()
        .map(|e| {
            let kind = e["event_type"].as_str().unwrap().to_string();
            let action = &e["event_data"]["value"]["events"][&kind]["action"];
            (
                kind,
                action["id"].as_str().unwrap().to_string(),
                action["points"][0]["x"].as_f64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("canvas_add".into(), "keep".into(), 0.0),
            ("canvas_move".into(), "keep".into(), 20.0)
        ]
    );
}

#[tokio::test]
async fn client_cannot_spoof_another_users_id() {
    let server = Server::default().await;
    let mut alice = server.client().await;
    let mut bob = server.client().await;
    alice.join("r1").await;
    bob.join("r1").await;

    let bob_id = bob.id.clone();
    alice
        .send(json!({
            "key": "message",
            "value": { "events": { "canvasAdd": { "action": action("s1", "r1", 0.0) } } },
            "user": { "id": bob_id, "name": "definitely bob", "color": "#000" }
        }))
        .await;

    let added = bob.expect_event("canvas_add").await;
    assert_eq!(added["_sender"], alice.id.as_str());
}

#[tokio::test]
async fn events_for_other_rooms_are_rejected() {
    let server = Server::default().await;
    let mut alice = server.client().await;
    let mut eve = server.client().await;
    alice.join("r1").await;
    eve.join("r2").await;

    eve.event(json!({ "canvasAdd": { "action": action("x", "r1", 0.0) } }))
        .await;
    eve.expect_key("error").await;
    alice.expect_silence("canvas_add").await;
}

#[tokio::test]
async fn events_before_joining_are_rejected() {
    let server = Server::default().await;
    let mut alice = server.client().await;
    alice
        .event(json!({ "canvasCursor": { "x": 1, "y": 2, "room_id": "r1" } }))
        .await;
    let error = alice.expect_key("error").await;
    assert!(error["message"].as_str().unwrap().contains("join"));
}

#[tokio::test]
async fn malformed_json_is_reported_and_connection_survives() {
    let server = Server::default().await;
    let mut alice = server.client().await;
    alice.ws.send(Message::text("{not json")).await.unwrap();
    assert_eq!(alice.recv().await["key"], "error");

    alice.send(json!({ "key": "ping" })).await;
    assert_eq!(alice.recv().await, json!({ "key": "pong" }));
}

#[tokio::test]
async fn member_count_is_computed_by_the_server() {
    let server = Server::default().await;
    let mut alice = server.client().await;
    let mut bob = server.client().await;
    alice.join("r1").await;
    bob.join("r1").await;

    alice
        .event(json!({ "roomMembersCount": { "room_id": "r1", "count": 99 } }))
        .await;
    loop {
        let count = alice.expect_event("room_members_count").await;
        if count["count"] == 2 {
            break;
        }
        assert_ne!(count["count"], 99);
    }
}

#[tokio::test]
async fn disconnecting_notifies_remaining_members() {
    let server = Server::default().await;
    let mut alice = server.client().await;
    let mut bob = server.client().await;
    alice.join("r1").await;
    bob.join("r1").await;

    let bob_id = bob.id.clone();
    drop(bob);

    let removed = alice.expect_event("room_removed").await;
    assert_eq!(removed["_sender"], bob_id.as_str());
    assert_eq!(removed["room"]["members"], json!([alice.id]));
    loop {
        if alice.expect_event("room_members_count").await["count"] == 1 {
            break;
        }
    }
}

#[tokio::test]
async fn room_survives_a_refresh_within_the_idle_ttl() {
    let server = Server::default().await;
    let mut alice = server.client().await;
    alice.join("r1").await;
    alice
        .event(json!({ "canvasAdd": { "action": action("s1", "r1", 0.0) } }))
        .await;
    alice
        .event(
            json!({ "roomRemoved": { "room": { "id": "r1", "members": [], "created_by": "" } } }),
        )
        .await;
    alice.send(json!({ "key": "disconnected" })).await;
    assert!(alice.is_closed().await);

    let mut again = server.client().await;
    let joined = again.join("r1").await;
    assert_eq!(joined["history"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn rooms_are_dropped_immediately_with_zero_ttl() {
    let server = Server::start(Config {
        room_idle_ttl: Duration::ZERO,
        ..Config::default()
    })
    .await;
    let mut alice = server.client().await;
    alice.join("r1").await;
    alice
        .event(json!({ "canvasAdd": { "action": action("s1", "r1", 0.0) } }))
        .await;
    alice
        .event(
            json!({ "roomRemoved": { "room": { "id": "r1", "members": [], "created_by": "" } } }),
        )
        .await;
    alice
        .event(json!({ "roomMembersCount": { "room_id": "r1", "count": 0 } }))
        .await;
    alice.expect_key("error").await; // no longer in a room

    assert_eq!(server.get("/count").await["activeRooms"], 0);
}

#[tokio::test]
async fn silent_clients_are_dropped_by_the_heartbeat() {
    let server = Server::start(Config {
        heartbeat_interval: Duration::from_millis(200),
        heartbeat_timeout: Duration::from_millis(500),
        ..Config::default()
    })
    .await;
    let mut alice = server.client().await;
    alice.join("r1").await;
    // Not polling the socket means pings go unanswered.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(alice.is_closed().await);
    assert_eq!(server.get("/count").await["activeUsers"], 0);
}

#[tokio::test]
async fn http_endpoints() {
    let server = Server::default().await;
    assert_eq!(server.get("/health").await, Value::String("ok".into()));

    let mut alice = server.client().await;
    alice.join("r1").await;
    assert_eq!(
        server.get("/count").await,
        json!({ "activeUsers": 1, "activeRooms": 1 })
    );
    assert_eq!(
        server.get("/rooms").await,
        json!([{ "id": "r1", "member_count": 1 }])
    );
}

#[tokio::test]
async fn shutdown_closes_open_sockets() {
    let server = Server::default().await;
    let mut alice = server.client().await;
    alice.join("r1").await;

    server.shutdown.cancel();
    assert!(alice.is_closed().await);
    timeout(WAIT, server.task).await.unwrap().unwrap();
}
