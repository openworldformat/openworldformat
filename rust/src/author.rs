//! Who or what wrote a log entry — a person, a model, a visitor, a
//! host. Authorship is what makes the log an audit trail: "what did
//! the model do here, versus the people?" is a filter on one field.

use serde::{Deserialize, Serialize};

/// A peer's numeric id, when the author is connected.
pub type PeerId = u64;

/// One entry's author. Defaults to an unnamed author (the JS fold's
/// behavior for entries that omit the field).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Author {
    /// The peer, when the author is connected (None for the host app itself).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer: Option<PeerId>,
    /// The author's name (a person, a model, a visitor id, "host").
    pub name: String,
}
