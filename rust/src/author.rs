//! Who or what wrote a log entry — a person, a model, a visitor, a
//! host. Authorship is what makes the log an audit trail: "what did
//! the model do here, versus the people?" is a filter on one field.
//!
//! The filter has two levels. `name` says who; `source` says how the
//! batch came to be — by hand, from a library of ready-made content,
//! or with a model's help — so an exporter whose histories must carry
//! a disclosure label (spec/package.md, "Exports from other systems")
//! writes one word per entry and never a prompt.

use serde::{Deserialize, Serialize};

/// A peer's numeric id, when the author is connected.
pub type PeerId = u64;

/// The `source` labels the spec defines (spec/session.md, "The entry").
/// A reader treats any other label as no label.
pub const AUTHOR_SOURCES: &[&str] = &[SOURCE_HAND, SOURCE_LIBRARY, SOURCE_AI];

/// A person acting directly.
pub const SOURCE_HAND: &str = "hand";
/// Ready-made content placed from a catalogue or template.
pub const SOURCE_LIBRARY: &str = "library";
/// A model's output, whether or not a person accepted it.
pub const SOURCE_AI: &str = "ai";

/// One entry's author. Defaults to an unnamed author (the JS fold's
/// behavior for entries that omit the field).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Author {
    /// The peer, when the author is connected (None for the host app itself).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer: Option<PeerId>,
    /// The author's name (a person, a model, a visitor id, "host").
    pub name: String,
    /// How the batch came to be: `hand`, `library` or `ai`
    /// ([`AUTHOR_SOURCES`]). Optional; absent is unlabelled. Other
    /// labels ride through unchanged (the field is part of the entry's
    /// identity) and read as unlabelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl Author {
    /// A named author with no source label.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            peer: None,
            name: name.into(),
            source: None,
        }
    }

    /// The source label when it is one the spec defines, else `None`
    /// — the must-ignore rule applied to a label.
    pub fn known_source(&self) -> Option<&'static str> {
        let source = self.source.as_deref()?;
        AUTHOR_SOURCES.iter().copied().find(|s| *s == source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_rides_the_wire_only_when_set() {
        let plain = Author::named("maya");
        assert_eq!(serde_json::to_string(&plain).unwrap(), r#"{"name":"maya"}"#);
        let labelled = Author {
            source: Some(SOURCE_AI.into()),
            ..Author::named("blueprint")
        };
        let json = serde_json::to_string(&labelled).unwrap();
        assert_eq!(json, r#"{"name":"blueprint","source":"ai"}"#);
        let back: Author = serde_json::from_str(&json).unwrap();
        assert_eq!(back, labelled);
        assert_eq!(back.known_source(), Some("ai"));
    }

    #[test]
    fn an_unknown_label_is_kept_but_reads_as_none() {
        let back: Author = serde_json::from_str(r#"{"name":"x","source":"template"}"#).unwrap();
        assert_eq!(back.source.as_deref(), Some("template"));
        assert_eq!(back.known_source(), None);
        // Still on the wire: the label is part of the entry's identity.
        assert!(serde_json::to_string(&back).unwrap().contains("template"));
    }
}
