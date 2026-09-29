//! Wire protocol spoken with the frontend.
//!
//! Inbound variant names are `camelCase` (`canvasAdd`), outbound variant names
//! are `snake_case` (`canvas_add`). Fields are `snake_case` both ways. The two
//! directions are separate types so neither can drift into the other.

mod inbound;
mod model;
mod outbound;

pub use inbound::{ClientEnvelope, ClientEvent, ClientKey, ClientRoom, ClientUser, ClientValue};
pub use model::*;
pub use outbound::{
    ControlFrame, EventKind, HistoryEvent, ServerEvent, ServerKey, ServerMessage, ServerValue,
    encode,
};

#[cfg(test)]
mod tests;
