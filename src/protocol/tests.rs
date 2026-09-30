//! Locks in the wire contract with the frontend (`wsTypes.ts`).

use serde_json::{Value, json};
use uuid::Uuid;

use super::*;

fn action_json() -> Value {
    json!({
        "id": "shape-1",
        "room_id": "room-1",
        "tool": "pencil",
        "points": [{ "x": 1, "y": 2.5 }],
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

fn inbound(events: Value) -> ClientEnvelope {
    let raw = json!({
        "key": "message",
        "value": { "events": events },
        "user": { "id": "not-a-uuid", "name": "Ada", "color": "#f00" }
    });
    ClientEnvelope::parse(&raw.to_string()).expect("valid inbound message")
}

#[test]
fn inbound_variants_are_camel_case() {
    let cases = [
        json!({ "canvasCursor": { "x": 1.5, "y": 2, "room_id": "r" } }),
        json!({ "canvasAdd": { "action": action_json() } }),
        json!({ "canvasUpdate": { "action": action_json() } }),
        json!({ "canvasDuplicate": { "action": action_json() } }),
        json!({ "canvasMove": { "action": action_json() } }),
        json!({ "canvasDelete": { "id": "a", "ids": null, "room_id": "r" } }),
        json!({ "canvasDelete": { "ids": ["a", "b"], "room_id": "r" } }),
        json!({ "roomCreated": { "room": { "id": "r", "members": [], "created_by": "x" } } }),
        json!({ "roomJoined": { "room": { "id": "r", "members": ["x"], "created_by": "x" } } }),
        json!({ "roomRemoved": { "room": { "id": "r", "members": [], "created_by": "x" } } }),
        json!({ "chatMessage": { "chat": { "room_id": "r", "message": [
            { "message_id": "m", "text": "hi", "reaction_ids": [] }
        ] } } }),
        json!({ "chatReaction": { "reaction": { "room_id": "r", "message_id": "m", "reaction_id": "👍" } } }),
        json!({ "playBack": { "room_id": "r" } }),
        json!({ "roomMembersCount": { "room_id": "r", "count": 3 } }),
    ];
    for case in cases {
        let envelope = inbound(case.clone());
        assert_eq!(envelope.key, ClientKey::Message, "{case}");
        assert!(envelope.value.is_some(), "{case}");
    }
}

#[test]
fn inbound_rejects_snake_case_variants() {
    let raw = json!({ "key": "message", "value": { "events": { "canvas_add": { "action": action_json() } } } });
    assert!(ClientEnvelope::parse(&raw.to_string()).is_err());
}

#[test]
fn inbound_keepalive_and_disconnect_need_no_body() {
    let ping = ClientEnvelope::parse(r#"{"key":"ping"}"#).unwrap();
    assert_eq!(ping.key, ClientKey::Ping);

    let bye =
        ClientEnvelope::parse(r#"{"key":"disconnected","user":{"id":"x","name":"","color":""}}"#)
            .unwrap();
    assert_eq!(bye.key, ClientKey::Disconnected);
}

#[test]
fn action_round_trips_with_snake_case_fields() {
    let action: Action = serde_json::from_value(action_json()).unwrap();
    let back = serde_json::to_value(&action).unwrap();
    for field in [
        "id",
        "room_id",
        "fill_color",
        "image_data",
        "image_height",
        "image_width",
    ] {
        assert!(back.get(field).is_some(), "missing {field}");
    }
    assert_eq!(back["tool"], "pencil");
    assert_eq!(back["points"][0]["y"], 2.5);
}

#[test]
fn outbound_variants_are_snake_case() {
    let user = User {
        id: Uuid::nil(),
        name: "Ada".into(),
        color: "#f00".into(),
    };
    let action: Action = serde_json::from_value(action_json()).unwrap();
    let message = ServerMessage::event(user, ServerEvent::CanvasAdd { action });
    let value = serde_json::to_value(&message).unwrap();

    assert_eq!(value["key"], "message");
    assert_eq!(value["user"]["id"], Uuid::nil().to_string());
    assert!(value["value"]["events"]["canvas_add"]["action"].is_object());
}

#[test]
fn connected_message_carries_the_server_id() {
    let id = Uuid::new_v4();
    let value = serde_json::to_value(ServerMessage::connected(id)).unwrap();
    assert_eq!(
        value,
        json!({ "key": "connected", "value": null, "user": { "id": id, "name": "", "color": "" } })
    );
}

#[test]
fn history_event_shape_matches_frontend() {
    let action: Action = serde_json::from_value(action_json()).unwrap();
    let event = HistoryEvent::new(
        "room-1".into(),
        User::anonymous(Uuid::nil()),
        ServerEvent::CanvasMove { action },
    );
    let value = serde_json::to_value(&event).unwrap();
    assert_eq!(value["event_type"], "canvas_move");
    assert_eq!(value["event_room"], "room-1");
    assert!(value["event_time"].is_string());
    assert!(value["event_data"]["value"]["events"]["canvas_move"].is_object());
}

#[test]
fn control_frames() {
    assert_eq!(
        serde_json::to_value(ControlFrame::Pong).unwrap(),
        json!({ "key": "pong" })
    );
    assert_eq!(
        serde_json::to_value(ControlFrame::Error { message: "nope" }).unwrap(),
        json!({ "key": "error", "message": "nope" })
    );
}

#[test]
fn action_validation() {
    let mut action: Action = serde_json::from_value(action_json()).unwrap();
    assert!(action.validate().is_ok());

    action.id = "".into();
    assert!(action.validate().is_err());

    action.id = "x".repeat(MAX_ID_LEN + 1).into();
    assert!(action.validate().is_err());

    action.id = "ok".into();
    action.points = vec![Point { x: 0.0, y: 0.0 }; MAX_POINTS + 1];
    assert!(action.validate().is_err());
}
