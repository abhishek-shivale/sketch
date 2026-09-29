use std::io;

use thiserror::Error;

/// Failures that stop the process from starting or serving.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid configuration: {0}")]
    Config(#[from] ConfigError),

    #[error("failed to bind {addr}: {source}")]
    Bind { addr: String, source: io::Error },

    #[error("server error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Error)]
#[error("{var}: {reason}")]
pub struct ConfigError {
    pub var: &'static str,
    pub reason: String,
}

/// Failures while handling a single client message.
///
/// None of these are fatal to the connection: they are logged and reported
/// back to the client as `{"key":"error","message":...}`.
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("malformed message: {0}")]
    Malformed(#[from] serde_json::Error),

    #[error("binary frames are not supported")]
    BinaryFrame,

    #[error("invalid {field}: {reason}")]
    Invalid {
        field: &'static str,
        reason: &'static str,
    },

    #[error("join a room before sending room events")]
    NotInRoom,

    #[error("event targets room `{got}` but connection is in `{joined}`")]
    RoomMismatch { joined: String, got: String },

    #[error("server is at capacity ({0} rooms)")]
    TooManyRooms(usize),

    #[error("rate limit exceeded")]
    RateLimited,

    #[error("failed to encode response: {0}")]
    Encode(serde_json::Error),
}

impl ProtocolError {
    pub fn invalid(field: &'static str, reason: &'static str) -> Self {
        Self::Invalid { field, reason }
    }
}
