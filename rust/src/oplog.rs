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
    /// Milliseconds since the Unix epoch (0 when the writer didn't
    /// clock). The format requires no precision: a writer MAY coarsen
    /// it — to the day, for a public history (spec/session.md, "The
    /// entry") — and a reader never assumes it is finer than written.
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
    /// What the author says the batch is for — a commit message. History
    /// for people and for the git commit a batch becomes; it folds to
    /// nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// One log line (no trailing newline).
pub fn encode_line(entry: &OpLogEntry) -> Result<String, serde_json::Error> {
    serde_json::to_string(entry)
}

/// Parse one log line. Blank lines are skipped by callers before this.
pub fn decode_line(line: &str) -> Result<OpLogEntry, serde_json::Error> {
    serde_json::from_str(line)
}

/// A session log as a reader takes it: its entries in file order, and the
/// torn last line when one was skipped.
#[derive(Debug, Clone, Default)]
pub struct DecodedLog {
    /// Every entry, in file order. Blank lines hold none.
    pub entries: Vec<OpLogEntry>,
    /// The last line, numbered from 1, when it didn't parse and was
    /// skipped: a writer that crashed mid-append, not a broken log.
    pub torn_line: Option<usize>,
}

/// A line before the last that doesn't parse. The last line may be torn
/// mid-write; anything earlier is real corruption, and the entries after
/// it can't be placed — their identity and their parents run through it.
#[derive(Debug, Clone)]
pub struct CorruptLine {
    /// The line, numbered from 1 as an editor numbers it.
    pub line: usize,
    /// Why it doesn't parse.
    pub error: String,
    /// The entries above it, which did: what a reader can still show. An
    /// authority refuses the log rather than append after it.
    pub before: Vec<OpLogEntry>,
}

impl std::fmt::Display for CorruptLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line {} of the log doesn't parse: {}",
            self.line, self.error
        )
    }
}

impl std::error::Error for CorruptLine {}

/// Read a whole `ops.jsonl` as the spec says a reader must
/// (spec/package.md, "Integrity and tolerance"): blank lines are skipped, a
/// torn last line loses at most itself and is counted, and any earlier
/// line that doesn't parse is refused — the rule the Kotlin and Swift
/// loaders apply.
pub fn decode_log(text: &str) -> Result<DecodedLog, CorruptLine> {
    let lines: Vec<(usize, &str)> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .collect();
    let mut log = DecodedLog::default();
    for (k, &(n, line)) in lines.iter().enumerate() {
        match decode_line(line) {
            Ok(entry) => log.entries.push(entry),
            Err(_) if k + 1 == lines.len() => log.torn_line = Some(n + 1),
            Err(error) => {
                return Err(CorruptLine {
                    line: n + 1,
                    error: describe(&error),
                    before: log.entries,
                });
            }
        }
    }
    Ok(log)
}

/// The parser's message with its position put the log's way: the line is
/// the log's to name, so only the column is the parser's.
fn describe(error: &serde_json::Error) -> String {
    let message = error.to_string();
    let position = format!(" at line {} column {}", error.line(), error.column());
    match message.strip_suffix(&position) {
        Some(what) => format!("{what} at column {}", error.column()),
        None => message,
    }
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
                source: None,
            },
            ops: vec![SessionOp::Edit(Box::new(wt::EditOp::spawn(
                wt::WorldEntity::new(1, "lighthouse"),
            )))],
            timestamp_ms: 1_700_000_000_000,
            id: None,
            parent: None,
            message: None,
        };
        let line = encode_line(&entry).unwrap();
        assert!(!line.contains('\n'));
        let back = decode_line(&line).unwrap();
        assert_eq!(back.revision, 7);
        assert_eq!(back.author.name, "maya");
        assert_eq!(back.ops.len(), 1);
    }

    fn line(revision: u64, id: &str) -> String {
        format!(r#"{{"revision":{revision},"ops":[],"id":"{id}"}}"#)
    }

    #[test]
    fn a_torn_last_line_loses_only_itself() {
        let torn = r#"{"revision":3,"ops":[{"Spa"#;
        let text = format!("{}\n\n{}\n{torn}", line(1, "e1"), line(2, "e2"));
        let log = decode_log(&text).unwrap();
        assert_eq!(log.entries.len(), 2);
        assert_eq!(
            log.torn_line,
            Some(4),
            "lines count as an editor counts them"
        );
        // Blank lines after it don't move the end: it is still the last.
        assert_eq!(
            decode_log(&format!("{text}\n\n")).unwrap().torn_line,
            Some(4)
        );
        // A whole log tears nothing; CRLF and blank lines read as nothing.
        let log = decode_log(&format!("{}\r\n\r\n{}\r\n", line(1, "e1"), line(2, "e2"))).unwrap();
        assert_eq!(log.entries.len(), 2);
        assert_eq!(log.torn_line, None);
        assert!(decode_log("").unwrap().entries.is_empty());
    }

    #[test]
    fn a_bad_line_before_the_last_is_corruption() {
        let text = format!(
            "{}\n{{\"revision\":2,\"op\n{}\n",
            line(1, "e1"),
            line(3, "e3")
        );
        let corrupt = decode_log(&text).unwrap_err();
        assert_eq!(corrupt.line, 2);
        assert_eq!(corrupt.before.len(), 1);
        assert_eq!(corrupt.before[0].id.as_deref(), Some("e1"));
        // The line is the log's to name; the parser only knows the column.
        assert!(corrupt.error.contains("at column"), "{}", corrupt.error);
        assert!(!corrupt.error.contains("line 1"), "{}", corrupt.error);
        assert!(
            corrupt
                .to_string()
                .starts_with("line 2 of the log doesn't parse: "),
            "{corrupt}"
        );
        // JSON that isn't an entry is corruption too, not a torn write.
        let corrupt = decode_log(&format!("{{\"revision\":1}}\n{}", line(2, "e2"))).unwrap_err();
        assert_eq!(corrupt.line, 1);
        assert!(corrupt.before.is_empty());
        assert!(corrupt.error.contains("ops"), "{}", corrupt.error);
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
                source: None,
            },
            ops: vec![SessionOp::Edit(Box::new(wt::EditOp::spawn(
                wt::WorldEntity::new(1, "beacon"),
            )))],
            id: Some("anything at all".into()),
            parent: Some("e6".into()),
            message: None,
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
