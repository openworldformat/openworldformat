//! The op log: one JSON object per line, `ops.jsonl` — the room's durable,
//! replayable history (spec phase 5; the session package's one log — see
//! `docs/rfcs/multiplayer/session-package-format.md`).
//!
//! world-sync stays I/O-free: this module is only the serde shape and
//! line codec. The host appends every committed `ops` message; a later
//! session replays the file to rebuild the document, and a time-lapse
//! player steps through it.
//!
//! An entry's `ops` are [`SessionOp`]s: edits (which change the document)
//! plus tool, input, state and clock records (history that folds to
//! nothing). Logs written before those kinds existed hold plain `EditOp`
//! JSON and parse unchanged.

use serde::{Deserialize, Serialize};

use crate::author::Author;
use crate::session::SessionOp;

/// One committed batch, in the order it committed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpLogEntry {
    /// The document revision after these ops applied. Only edits bump it;
    /// history-only entries carry the current revision.
    pub revision: u64,
    /// Who or what wrote the entry. The spec's own example logs omit it
    /// on some lines (the JS fold reads them); a missing author is an
    /// unnamed one, never a broken log.
    #[serde(default)]
    pub author: Author,
    pub ops: Vec<SessionOp>,
    /// Milliseconds since the Unix epoch (0 when the writer didn't clock).
    #[serde(default)]
    pub timestamp_ms: u64,
    /// The entry's identity, for branching histories (a reader treats it
    /// as opaque). Absent on linear logs, where file order is the chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The entry this one builds on; absent means the previous line (or
    /// the base, for the first). A branch is a second child of one parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

/// One log line (no trailing newline).
pub fn encode_line(entry: &OpLogEntry) -> Result<String, serde_json::Error> {
    serde_json::to_string(entry)
}

/// Parse one log line. Blank lines are skipped by callers before this.
pub fn decode_line(line: &str) -> Result<OpLogEntry, serde_json::Error> {
    serde_json::from_str(line)
}

/// Canonical JSON — the one serialization writers hash
/// (spec/session.md): no whitespace, object keys sorted, arrays in
/// order. Two writers that agree on content agree on these bytes, so
/// entries minted on different forks of the same parent get the same
/// identity. Integer-valued JSON is byte-identical across the format's
/// five references; float formatting beyond that is each language's
/// own — hash content that stays on the integers (timestamps already
/// are, in ms).
pub fn canonical_json(value: &serde_json::Value) -> String {
    // serde_json's Map is a BTreeMap unless `preserve_order` is on, and
    // this crate does not turn it on: `to_string` on a Value is already
    // whitespace-free and key-sorted. The function exists so the
    // contract has a name, and so the invariant is asserted, not assumed.
    debug_assert!(canonical_by_construction(value));
    serde_json::to_string(value).expect("a Value always serializes")
}

/// Recursively check: every object's keys in sorted order (a BTreeMap
/// guarantees it; this guards the guarantee against a future
/// `preserve_order` sneaking in through a feature flag elsewhere).
fn canonical_by_construction(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.keys().zip(map.keys().skip(1)).all(|(a, b)| a <= b)
                && map.values().all(canonical_by_construction)
        }
        serde_json::Value::Array(items) => items.iter().all(canonical_by_construction),
        _ => true,
    }
}

/// The entry's content identity: `sha256:<hex>` over the canonical JSON
/// of the entry *without* its `id` (an id cannot contain itself; the
/// `parent` it builds on is part of the content, so the same edit on
/// two forks of one parent mints one id — the point of the rule).
pub fn compute_entry_id(entry: &OpLogEntry) -> Result<String, serde_json::Error> {
    let mut value = serde_json::to_value(entry)?;
    if let Some(object) = value.as_object_mut() {
        object.remove("id");
    }
    Ok(format!(
        "sha256:{}",
        crate::hash::sha256_hex(canonical_json(&value).as_bytes(),)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as wt;

    #[test]
    fn entry_roundtrip() {
        let entry = OpLogEntry {
            revision: 7,
            author: Author {
                peer: Some(3),
                name: "maya".into(),
            },
            ops: vec![SessionOp::Edit(Box::new(wt::EditOp::spawn(
                wt::WorldEntity::new(1, "lighthouse"),
            )))],
            timestamp_ms: 1_700_000_000_000,
            id: None,
            parent: None,
        };
        let line = encode_line(&entry).unwrap();
        assert!(!line.contains('\n'));
        let back = decode_line(&line).unwrap();
        assert_eq!(back.revision, 7);
        assert_eq!(back.author.name, "maya");
        assert_eq!(back.ops.len(), 1);
    }

    /// The cross-language golden: the same bytes, and the same digest,
    /// in every one of the five references.
    #[test]
    fn canonical_json_matches_the_cross_language_golden() {
        let entry: serde_json::Value = serde_json::json!({
            "revision": 7,
            "timestamp_ms": 1790000000123u64,
            "author": {"peer": 3, "name": "maya"},
            "ops": [{"SpawnEntity": {"entity": {"id": 1, "name": "beacon"}}}],
            "parent": "e6"
        });
        assert_eq!(
            canonical_json(&entry),
            "{\"author\":{\"name\":\"maya\",\"peer\":3},\
             \"ops\":[{\"SpawnEntity\":{\"entity\":{\"id\":1,\"name\":\"beacon\"}}}],\
             \"parent\":\"e6\",\"revision\":7,\"timestamp_ms\":1790000000123}"
        );
        assert_eq!(
            format!(
                "sha256:{}",
                crate::hash::sha256_hex(canonical_json(&entry).as_bytes())
            ),
            "sha256:4b754af615abd0b36a11f6bec2753eedb7be6492080d8ed8ba6e50464dfaffa2"
        );
    }

    #[test]
    fn entry_identity_excludes_the_id_and_includes_the_parent() {
        let entry = OpLogEntry {
            revision: 7,
            timestamp_ms: 1_790_000_000_123,
            author: Author {
                peer: Some(3),
                name: "maya".into(),
            },
            ops: vec![SessionOp::Edit(Box::new(wt::EditOp::spawn(
                wt::WorldEntity::new(1, "beacon"),
            )))],
            id: Some("anything at all".into()),
            parent: Some("e6".into()),
        };
        // The typed entity carries its default transform, so this
        // fixture has a digest of its own — the cross-language golden
        // above covers the bare shape. What matters here: the id field
        // contributes nothing, and the parent contributes everything
        // it should.
        let id_here = compute_entry_id(&entry).unwrap();
        assert_eq!(
            id_here,
            "sha256:51a49c473ba75e925dcccf1b9578fb163bba841833d7d1d578cc77388c0102b0"
        );
        let id_other = compute_entry_id(&OpLogEntry {
            id: Some("a different one entirely".into()),
            ..entry.clone()
        })
        .unwrap();
        assert_eq!(id_other, id_here);
        let forked = OpLogEntry {
            parent: Some("e9".into()),
            ..entry
        };
        assert_ne!(compute_entry_id(&forked).unwrap(), id_here);
    }
}
