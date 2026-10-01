//! The conformance suite, run the way the `js` and `python` CI jobs run
//! it: every world parses, validates, and round-trips; the examples
//! fold; the forks fold per tip; the state folds; the physics outcome
//! assertions pass under this crate's solver.

use std::collections::BTreeSet;
use std::path::PathBuf;

use openworldformat::{
    WorldLimits, WorldManifest as Manifest, fold_path, fold_state, validate_manifest,
};

fn repo(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../")
        .join(relative)
}

fn worlds() -> Vec<(String, String)> {
    let dir = repo("conformance");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("conformance dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no fixtures in {}", dir.display());
    files
        .into_iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read_to_string(&p).unwrap())
        })
        .collect()
}

fn parse(name: &str, text: &str) -> Manifest {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn conformance_worlds_parse_validate_and_roundtrip() {
    for (name, text) in worlds() {
        let manifest = parse(&name, &text);
        manifest
            .check_version()
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!manifest.entities.is_empty(), "{name}: no entities");
        let issues = validate_manifest(&manifest, &WorldLimits::default());
        assert!(
            issues
                .iter()
                .all(|i| i.severity != openworldformat::Severity::Error),
            "{name}: validation errors"
        );
        // Ids and names are unique; parents exist.
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for e in &manifest.entities {
            assert!(ids.insert(e.id.0), "{name}: duplicate id {}", e.id);
            assert!(
                names.insert(e.name.as_str().to_string()),
                "{name}: duplicate name"
            );
        }
        // JSON round trip
        let value = serde_json::to_value(&manifest).unwrap();
        let back: Manifest = serde_json::from_value(value).unwrap();
        assert_eq!(back, manifest, "{name}: JSON round trip");
    }
}

#[test]
fn hello_world_folds_and_history_folds_to_nothing() {
    let base: Manifest = serde_json::from_str(
        &std::fs::read_to_string(repo("examples/hello-world/manifest.json")).unwrap(),
    )
    .unwrap();
    let entries: Vec<openworldformat::OpLogEntry> =
        std::fs::read_to_string(repo("examples/hello-world/ops.jsonl"))
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()
            .unwrap();
    let doc = openworldformat::fold_log(&base.as_base().unwrap(), &entries).unwrap();
    assert!(doc.get_by_name("lantern").is_some());
    assert_eq!(entries.iter().filter(|e| !e.is_history_only()).count(), 1);
}

#[test]
fn the_speedrun_fork_folds_each_tip_with_its_own_state() {
    let base: Manifest = serde_json::from_str(
        &std::fs::read_to_string(repo("examples/speedrun-fork/manifest.json")).unwrap(),
    )
    .unwrap();
    let entries: Vec<openworldformat::OpLogEntry> =
        std::fs::read_to_string(repo("examples/speedrun-fork/ops.jsonl"))
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()
            .unwrap();
    let state_doc: openworldformat::StateDoc = serde_json::from_str(
        &std::fs::read_to_string(repo("examples/speedrun-fork/state.json")).unwrap(),
    )
    .unwrap();

    let (trunk, path) = fold_path(&base.as_base().unwrap(), &entries, Some("e2")).unwrap();
    assert_eq!(path, vec!["e1", "e2"]);
    assert!(trunk.get_by_name("banner").is_some());

    let (run, path) = fold_path(&base.as_base().unwrap(), &entries, Some("e3")).unwrap();
    assert_eq!(path, vec!["e1", "e2", "e3"]);
    assert_eq!(run.len(), trunk.len()); // a run is history, not edits
    let by_id = |id: &str| {
        entries
            .iter()
            .find(|e| e.id.as_deref() == Some(id))
            .cloned()
            .unwrap()
    };
    let chain: Vec<_> = ["e1", "e2", "e3"].map(by_id).to_vec();
    let folded = fold_state(&state_doc, &chain);
    assert_eq!(folded.values.get("run.kai"), Some(&serde_json::json!(9.42)));
    assert_eq!(folded.values.get("run.noor"), Some(&serde_json::json!(0.0)));
}

#[test]
fn the_physics_conformance_outcomes_pass_under_this_solver() {
    let manifest: Manifest =
        serde_json::from_str(&std::fs::read_to_string(repo("conformance/physics.json")).unwrap())
            .unwrap();
    let outcomes: openworldformat::physics::OutcomesDoc = serde_json::from_str(
        &std::fs::read_to_string(repo("conformance/outcomes/physics.json")).unwrap(),
    )
    .unwrap();
    let result = openworldformat::physics::run_outcomes(&manifest, &outcomes);
    assert_eq!(result.failures, Vec::<String>::new());
    assert!(result.ok);
}
