//! The shared merge corpus (conformance/merge/cases/, spec/session.md
//! "The merge rules, exactly"): every case runs — the remap table and
//! the rewritten entries against the committed expected ones, then the
//! fold of base + main + merged entries against the expected head. The
//! other four references run the same cases in their own suites, so the
//! five merges cannot drift apart.
//!
//! One comparison is normalized, not raw: the committed heads are the
//! JS fold's raw-JSON output (a struct field holds only what an op
//! carried), while this crate's fold is typed and materializes defaults
//! (a bare entity gains its `transform`; an avatar its spawn position
//! and POV; a creation its `bbox_half`). Byte-matching the raw text is
//! therefore impossible for this reference, so the expected head is
//! read through the same typed model and both sides are compared as
//! canonical text — every field the format models, byte for byte.

use std::path::PathBuf;

use openworldformat::{OpLogEntry, WorldManifest as Manifest, fold_log, manifest_text_of, merge_branch};

fn repo(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../")
        .join(relative)
}

struct Case {
    name: String,
    base: Manifest,
    main: Vec<OpLogEntry>,
    branch: Vec<OpLogEntry>,
    remapped: Vec<(u64, u64)>,
    entries: Vec<OpLogEntry>,
    head: Manifest,
}

fn cases() -> Vec<Case> {
    fn parse<T: serde::de::DeserializeOwned>(
        name: &str,
        value: serde_json::Value,
        what: &str,
    ) -> T {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("{name}: {what}: {e}"))
    }
    let dir = repo("conformance/merge/cases");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("merge corpus dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no cases in {}", dir.display());
    files
        .into_iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let case: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&p).unwrap())
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
            let at = |pointer: &str| {
                case.pointer(pointer)
                    .unwrap_or_else(|| panic!("{name}: no {pointer}"))
                    .clone()
            };
            Case {
                name: name.clone(),
                base: parse(&name, at("/base"), "base"),
                main: parse(&name, at("/main"), "main"),
                branch: parse(&name, at("/branch"), "branch"),
                remapped: parse(&name, at("/expected/remapped"), "expected.remapped"),
                entries: parse(&name, at("/expected/entries"), "expected.entries"),
                head: parse(
                    &name,
                    serde_json::from_str(at("/expected/head").as_str().unwrap())
                        .unwrap_or_else(|e| panic!("{name}: expected.head: {e}")),
                    "expected.head",
                ),
            }
        })
        .collect()
}

#[test]
fn the_merge_corpus_is_present_and_covers_the_hand_written_rules() {
    let cases = cases();
    for required in ["spent-id", "modify-world", "batch", "names"] {
        let required = format!("{required}.json");
        assert!(
            cases.iter().any(|c| c.name == required),
            "missing hand-written case {required}"
        );
    }
    assert!(cases.len() >= 100, "the corpus holds the generated cases too");
}

#[test]
fn every_merge_case_matches_the_committed_expected_results() {
    for case in cases() {
        let name = &case.name;
        let base = case.base.as_base().unwrap_or_else(|e| panic!("{name}: base: {e}"));
        let main = fold_log(&base, &case.main).unwrap_or_else(|e| panic!("{name}: main: {e}"));

        let merged = merge_branch(&main, &case.branch).unwrap_or_else(|e| panic!("{name}: {e}"));

        // The remap table, ascending by old id (BTreeMap's order).
        let remapped: Vec<(u64, u64)> = merged.remapped.into_iter().collect();
        assert_eq!(remapped, case.remapped, "{name}: the remap table");

        // The rewritten entries. Both sides are typed and serialize the
        // same way, so the values compare directly — identity fields
        // (revision, author, id, parent, message, timestamp) included.
        assert_eq!(
            serde_json::to_value(&merged.entries).unwrap(),
            serde_json::to_value(&case.entries).unwrap(),
            "{name}: the rewritten entries"
        );

        // The merged head: fold base + main + merged entries and compare
        // canonical text. The comparison happens before anything else
        // could alias the merged entries — this crate's fold clones what
        // it applies, so the JS generator's fold-mutates-the-ops bug has
        // no foothold here, but the order keeps it that way by
        // construction. Entries were already compared above, so what
        // fails here is the head alone.
        let log: Vec<OpLogEntry> = case
            .main
            .iter()
            .chain(merged.entries.iter())
            .cloned()
            .collect();
        let head = fold_log(&base, &log).unwrap_or_else(|e| panic!("{name}: head fold: {e}"));
        assert_eq!(
            manifest_text_of(&head.to_manifest()),
            manifest_text_of(&case.head),
            "{name}: the merged head, as canonical text"
        );
    }
}
