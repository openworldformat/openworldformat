//! The package's derived parts: snapshot naming and compaction
//! (spec/package.md, spec/session.md "Snapshots"). The file moves
//! themselves stay with the host app — this crate is the format's
//! serde-only core — so what lives here are the decisions a host could
//! get wrong: which name a snapshot goes under, and what compaction
//! changes on paper.

use crate::session::SessionMeta;

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
/// the head the folded manifest now holds, the old log becomes
/// `ops.archive.jsonl` (or is deleted — both are legal), and a fresh
/// `ops.jsonl` starts from empty. Nothing observable about the current
/// state changes; structural replay is what gets truncated.
///
/// The routine, for the host that performs it:
/// 1. fold to head and write that document as the new `manifest.json`;
/// 2. write `package.json` with the [`SessionMeta`] this returns;
/// 3. rename `ops.jsonl` to `ops.archive.jsonl`;
/// 4. start a fresh, empty `ops.jsonl`.
pub fn compact_plan(meta: &SessionMeta, head_revision: u64) -> SessionMeta {
    let mut compacted = meta.clone();
    compacted.base_revision = head_revision;
    compacted
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
