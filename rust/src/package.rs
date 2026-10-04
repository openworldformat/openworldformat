//! The package's derived parts: snapshot naming and compaction
//! (spec/package.md, spec/session.md "Snapshots"). The file moves
//! themselves stay with the host app — this crate is the format's
//! serde-only core — so what lives here are the decisions a host could
//! get wrong: which name a snapshot goes under, and what compaction
//! changes on paper.

use crate::session::SessionMeta;

/// The world now — the state at the tip of `main`. Viewers read only this.
pub const MANIFEST: &str = "manifest.json";

/// The oldest state the log folds from. Present whenever `ops.jsonl`
/// holds entries; a package without history is `manifest.json` alone.
pub const BASE_SNAPSHOT: &str = "snapshots/base.json";

/// The package format version this crate writes: 2, head-first.
pub const PACKAGE_FORMAT_VERSION: u32 = crate::session::SESSION_FORMAT_VERSION;

/// A snapshot's path inside the package: `snapshots/entry-<id>.json`
/// for logs whose entries carry identity — the branch-aware name, one
/// file per entry regardless of which branch it tips — falling back to
/// `snapshots/rev-<N>.json` for linear logs without explicit ids.
///
/// Entry ids are opaque strings; a filename has to stay a filename, so
/// anything outside `[A-Za-z0-9._-]` folds to `_` (two ids that sanitize
/// alike were alike enough to collide anyway — the fold from the base
/// reaches the same state, which is the snapshot's own disclaimer).
pub fn snapshot_filename(entry_id: Option<&str>, revision: u64) -> String {
    match entry_id {
        Some(id) => format!("snapshots/entry-{}.json", sanitize(id)),
        None => format!("snapshots/rev-{revision}.json"),
    }
}

/// Keep a filename a filename.
fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// What compaction does to a package, as data: `base_revision` moves to
/// the head, the old log becomes `ops.archive.jsonl` (or is deleted —
/// both are legal), and a fresh `ops.jsonl` starts from empty. Nothing
/// observable about the current state changes — `manifest.json` already
/// is the head — and structural replay is what gets truncated.
///
/// The routine, for the host that performs it:
/// 1. copy `manifest.json` (the head) to `snapshots/base.json`;
/// 2. write `package.json` with the [`SessionMeta`] this returns;
/// 3. rename `ops.jsonl` to `ops.archive.jsonl`;
/// 4. start a fresh, empty `ops.jsonl`, and drop snapshots older than
///    the new base.
pub fn compact_plan(meta: &SessionMeta, head_revision: u64) -> SessionMeta {
    let mut compacted = meta.clone();
    compacted.base_revision = head_revision;
    compacted
}

/// A manifest's canonical text (spec/package.md, "Canonical text"): what
/// an authority writes to `manifest.json`, so that one world is always
/// the same bytes — small diffs, ordinary git merges, a `world_sha256`
/// that means something. Members sorted by code point, null members left
/// out, entities in id order, two-space indentation, arrays of plain
/// values on one line, integral numbers without a fraction, a trailing
/// newline. The JS reference's `manifestText` writes the same bytes.
pub fn manifest_text(manifest: &serde_json::Value) -> String {
    let mut world = manifest.clone();
    if let Some(entities) = world.get_mut("entities").and_then(|e| e.as_array_mut()) {
        entities.sort_by_key(|e| e.get("id").and_then(|id| id.as_u64()).unwrap_or(u64::MAX));
    }
    let mut out = String::new();
    pretty(&world, 0, &mut out);
    out.push('\n');
    out
}

/// [`manifest_text`] of a typed manifest. It goes through its own JSON
/// text first, so a float prints the way it was written (`0.1`, not the
/// widened `0.10000000149011612`).
pub fn manifest_text_of(manifest: &crate::WorldManifest) -> String {
    let text = serde_json::to_string(manifest).expect("a manifest serializes");
    manifest_text(&serde_json::from_str(&text).expect("its own JSON parses"))
}

fn pretty(value: &serde_json::Value, depth: usize, out: &mut String) {
    use serde_json::Value;
    let pad = "  ".repeat(depth);
    let inner = "  ".repeat(depth + 1);
    match value {
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Array(items) if items.iter().all(|x| !x.is_array() && !x.is_object()) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                scalar(item, out);
            }
            out.push(']');
        }
        Value::Array(items) => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                out.push_str(&inner);
                pretty(item, depth + 1, out);
            }
            out.push('\n');
            out.push_str(&pad);
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map
                .iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, _)| k)
                .collect();
            if keys.is_empty() {
                out.push_str("{}");
                return;
            }
            // Code-point order: Rust strings compare as UTF-8 bytes, which
            // orders the same as code points.
            keys.sort();
            out.push_str("{\n");
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                out.push_str(&inner);
                out.push_str(&serde_json::to_string(key).expect("a string serializes"));
                out.push_str(": ");
                pretty(&map[key.as_str()], depth + 1, out);
            }
            out.push('\n');
            out.push_str(&pad);
            out.push('}');
        }
        other => scalar(other, out),
    }
}

/// A plain value: integral numbers without a fraction (`2`, never `2.0`),
/// others in their shortest round-trip form.
fn scalar(value: &serde_json::Value, out: &mut String) {
    if let Some(f) = value.as_f64()
        && value.is_f64()
        && f.fract() == 0.0
        && f.abs() <= crate::identity::MAX_ENTITY_ID as f64
    {
        out.push_str(&format!("{}", f as i64));
        return;
    }
    out.push_str(&serde_json::to_string(value).expect("a value serializes"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_name_by_entry_when_there_is_one() {
        assert_eq!(
            snapshot_filename(Some("e42"), 7),
            "snapshots/entry-e42.json"
        );
        // Opaque ids sanitize into filename-safe ones.
        assert_eq!(
            snapshot_filename(Some("sha256:4b754af+/odd"), 7),
            "snapshots/entry-sha256_4b754af__odd.json"
        );
        // Linear logs keep the revision name.
        assert_eq!(snapshot_filename(None, 7), "snapshots/rev-7.json");
    }

    #[test]
    fn compaction_moves_the_base_to_head() {
        let meta = SessionMeta {
            base_revision: 0,
            head_revision: 41,
            ..SessionMeta::new("castle")
        };
        let plan = compact_plan(&meta, 41);
        assert_eq!(plan.base_revision, 41);
        assert_eq!(plan.head_revision, 41);
        assert_eq!(plan.name, "castle");
    }
}
