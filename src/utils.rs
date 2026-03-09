use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use chrono::{DateTime, Utc};
use futures_util::stream::SplitSink;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(serialize = "snake_case", deserialize = "camelCase"))]
pub enum GlobalEvents {
    Connected,
    Disconnected,
    Message,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Point {
    x: i16,
    y: i16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Tools {}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Action {
    color: String,
    fill_color: String,
    id: String,
    opacity: i8,
    points: Vec<Point>,
    size: i8,
    timestamp: DateTime<Utc>,
    tool: Tools,
    text: Option<String>,
    image_data: Option<String>,
    image_height: Option<i16>,
    image_width: Option<i16>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(serialize = "snake_case", deserialize = "camelCase"))]
pub enum MessageEvents {
    CanvasCursor {
        user: User,
        x: i16,
        y: i16,
    },
    CanvasAdd {
        user: User,
        action: Action,
    },
    CanvasUpdate {
        user: User,
        action: Action,
    },
    CanvasDuplicate {
        user: User,
        action: Action,
    },
    CanvasMove {
        user: User,
        action: Action,
    },
    CanvasDelete {
        user: User,
        id: Option<String>,
        ids: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub color: String,
}

// #[derive(Serialize, Deserialize, Debug, Clone)]
// pub struct Room {
//     pub id: u64,
//     pub name: String,
//     pub admin: u64,
// }

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DataValue {
    pub events: MessageEvents,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(serialize = "snake_case", deserialize = "camelCase"))]
pub struct Data {
    pub key: GlobalEvents,
    pub value: Option<DataValue>,
    pub user: User,
}

impl Data {
    pub fn connected(user_id: u64) -> Self {
        Self {
            key: GlobalEvents::Connected,
            value: None,
            user: {
                User {
                    id: user_id,
                    name: "".to_string(),
                    color: "Red".to_string(),
                }
            },
        }
    }

    // pub fn user_is_drawing(x: i32, y: i32, user: User) -> Self {
    //     Self {
    //         key: GlobalEvents::Message,
    //         value: Some(DataValue {
    //             events: MessageEvents::UserIsDrawing { x, y },
    //         }),
    //         user,
    //     }
    // }

    // pub fn user_started_drawing(x: i32, y: i32, user: User) -> Self {
    //     Self {
    //         key: GlobalEvents::Message,
    //         value: Some(DataValue {
    //             events: MessageEvents::UserStartedDrawing { x, y },
    //         }),
    //         user,
    //     }
    // }

    // pub fn user_stopped_drawing(user: User) -> Self {
    //     Self {
    //         key: GlobalEvents::Message,
    //         value: Some(DataValue {
    //             events: MessageEvents::UserStoppedDrawing,
    //         }),
    //         user,
    //     }
    // }

    pub fn convert(self) -> Message {
        let utf8 = Utf8Bytes::from(serde_json::to_string(&self).expect("Parsing Fail"));
        Message::Text(utf8)
    }
}

pub type SenderType = SplitSink<WebSocket, Message>;
