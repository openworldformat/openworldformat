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
        &std::fs::read_to_string(repo("examples/hello-world/snapshots/base.json")).unwrap(),
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
        &std::fs::read_to_string(repo("examples/speedrun-fork/snapshots/base.json")).unwrap(),
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

#[test]
fn the_cinematography_conformance_outcomes_pass_under_the_reference_math() {
    let manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(repo("conformance/cinematography.json")).unwrap(),
    )
    .unwrap();
    let outcomes: openworldformat::cinematography::OutcomesDoc = serde_json::from_str(
        &std::fs::read_to_string(repo("conformance/outcomes/cinematography.json")).unwrap(),
    )
    .unwrap();
    let result = openworldformat::cinematography::run_outcomes(&manifest, &outcomes);
    assert_eq!(result.failures, Vec::<String>::new());
    assert!(result.ok);
}

#[test]
fn hand_authored_ron_named_structs_parse() {
    // The regression this pins: serde(flatten) forces map-form
    // deserialization, which rejects RON's named-struct syntax — the
    // form every hand-written world.ron uses. The hand-written serde
    // on WorldEntity/EnvironmentDef/EntityPatch enters through
    // deserialize_struct and reads both forms plus JSON objects.
    //
    // One authoring constraint RON itself imposes: a named struct's
    // keys are identifiers, so `ext-physics` (the dash) cannot be
    // spelled there — hand-authored RON carries extension fields by
    // writing that one value in map form, quoted keys and all, or by
    // shipping JSON. Serialization always round-trips either way.
    let text = r#"
(
    version: 3,
    meta: (name: "ron-form"),
    environment: Some((
        background_color: Some((0.1, 0.2, 0.3, 1.0)),
        ambient_intensity: Some(0.5),
    )),
    entities: [
        (
            id: (1),
            name: ("ball"),
            transform: (position: (0.0, 5.0, 0.0)),
            shape: Some(Sphere(radius: 0.3)),
        ),
    ],
    next_entity_id: 2,
)
"#;
    let manifest: Manifest = ron::from_str(text).expect("named-struct RON parses");
    assert_eq!(manifest.entities.len(), 1);
    assert_eq!(
        manifest.environment.as_ref().unwrap().ambient_intensity,
        Some(0.5)
    );

    // Extension fields survive RON's own output (the map form
    // serialization emits) — round-trip, not folk syntax.
    let mut entity = openworldformat::WorldEntity::new(2, "late");
    entity.extra.insert(
        "ext-physics".into(),
        serde_json::json!({ "body": "dynamic" }),
    );
    let ron_text = ron::to_string(&entity).unwrap();
    let back: openworldformat::WorldEntity = ron::from_str(&ron_text).expect(&ron_text);
    assert_eq!(back, entity);
    assert_eq!(back.extra["ext-physics"]["body"], "dynamic");

    // …and everywhere in JSON.
    let json = serde_json::to_string(&entity).unwrap();
    assert!(json.contains("\"ext-physics\""));
    let back: openworldformat::WorldEntity = serde_json::from_str(&json).unwrap();
    assert_eq!(back, entity);

    // The full round trip keeps working in both dialects.
    let ron_back = ron::to_string(&manifest).unwrap();
    let _: Manifest = ron::from_str(&ron_back).expect(&ron_back);
    let json_manifest = serde_json::to_string(&manifest).unwrap();
    let _: Manifest = serde_json::from_str(&json_manifest).unwrap();
}

/// A package's base, log and head, read the head-first way.
fn package(
    name: &str,
) -> (
    Manifest,
    Vec<openworldformat::OpLogEntry>,
    Manifest,
    serde_json::Value,
) {
    let read =
        |file: &str| std::fs::read_to_string(repo(&format!("examples/{name}/{file}"))).unwrap();
    let base: Manifest = serde_json::from_str(&read("snapshots/base.json")).unwrap();
    let head: Manifest = serde_json::from_str(&read("manifest.json")).unwrap();
    let entries = read("ops.jsonl")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{name}: {e}: {l}")))
        .collect();
    let package: serde_json::Value = serde_json::from_str(&read("package.json")).unwrap();
    (base, entries, head, package)
}

fn examples() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(repo("examples"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("manifest.json").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Two manifests mean the same world: every field equal, entities
/// matched by id (their order is a serialization detail).
///
/// `next_entity_id` compares by its effective value — the larger of the
/// declared one and one past the largest id — since an absent or low
/// declaration can't hand out an id the world already holds.
fn same_world(a: &Manifest, b: &Manifest) -> Result<(), String> {
    let effective = |m: &Manifest| {
        let mut m = m.clone();
        let past = m.entities.iter().map(|e| e.id.0 + 1).max().unwrap_or(1);
        m.next_entity_id = m.next_entity_id.max(past);
        m
    };
    let (a, b) = (&effective(a), &effective(b));
    // JSON has one number type: 2 and 2.0 are the same value, so values
    // compare numerically (the crate's own `values_close`).
    let by_id = |m: &Manifest| -> std::collections::BTreeMap<u64, serde_json::Value> {
        m.entities
            .iter()
            .map(|e| (e.id.0, serde_json::to_value(e).unwrap()))
            .collect()
    };
    let (ea, eb) = (by_id(a), by_id(b));
    if ea.keys().ne(eb.keys()) {
        return Err("entity ids differ".into());
    }
    for (id, value) in &ea {
        if !openworldformat::values_close(value, &eb[id]) {
            return Err(format!("entity {id} differs"));
        }
    }
    let (mut a, mut b) = (
        serde_json::to_value(a).unwrap(),
        serde_json::to_value(b).unwrap(),
    );
    a.as_object_mut().unwrap().remove("entities");
    b.as_object_mut().unwrap().remove("entities");
    if a.as_object()
        .unwrap()
        .keys()
        .ne(b.as_object().unwrap().keys())
    {
        return Err("a field only one side has".into());
    }
    for (key, value) in a.as_object().unwrap() {
        if !openworldformat::values_close(value, &b[key]) {
            return Err(format!("{key} differs"));
        }
    }
    Ok(())
}

#[test]
fn the_fold_is_total_every_world_survives_an_empty_fold() {
    let mut worlds: Vec<(String, Manifest)> = worlds()
        .into_iter()
        .map(|(name, text)| {
            let m = parse(&name, &text);
            (name, m)
        })
        .collect();
    for name in examples() {
        let (base, _, head, _) = package(&name);
        worlds.push((format!("{name}/snapshots/base.json"), base));
        worlds.push((format!("{name}/manifest.json"), head));
    }
    for (name, manifest) in worlds {
        // Names bind at ingestion, so compare against the bound base.
        let base = manifest.as_base().unwrap_or_else(|e| panic!("{name}: {e}"));
        let folded = openworldformat::fold_log(&base, &[]).unwrap().to_manifest();
        let mut bound = manifest.clone();
        bound.entities = base.entities().cloned().collect();
        same_world(&folded, &bound).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn every_example_is_head_first_its_manifest_is_the_fold_to_main() {
    for name in examples() {
        let (base, entries, head, package) = package(&name);
        assert_eq!(package["format_version"], 2, "{name}: package format 2");
        let tip = package["refs"]["main"].as_str();
        let (doc, _) = fold_path(&base.as_base().unwrap(), &entries, tip)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        same_world(&doc.to_manifest(), &head).unwrap_or_else(|e| panic!("{name}: head: {e}"));
        let bytes = std::fs::read(repo(&format!("examples/{name}/manifest.json"))).unwrap();
        assert_eq!(
            package["world_sha256"].as_str(),
            Some(openworldformat::sha256_hex(&bytes).as_str()),
            "{name}: world_sha256 names manifest.json's bytes"
        );
    }
}

#[test]
fn modify_world_reaches_every_scene_field_and_undoes() {
    let manifest = parse(
        "hello",
        &std::fs::read_to_string(repo("examples/hello-world/manifest.json")).unwrap(),
    );
    let doc = manifest.as_base().unwrap();
    let op: openworldformat::EditOp =
        serde_json::from_value(serde_json::json!({"ModifyWorld": {"patch": {
            "meta": {"name": "hello-again", "description": "renamed"},
            "environment": null,
            "tours": [{"name": "walk", "waypoints": []}],
            "soundtrack": null
        }}}))
        .unwrap();
    let inverse = op.compute_inverse(&doc).unwrap();
    let mut changed = doc.clone();
    changed.apply(&op).unwrap();
    let m = changed.to_manifest();
    assert_eq!(m.meta.name, "hello-again");
    assert!(m.environment.is_none(), "null clears");
    assert_eq!(m.tours.len(), 1);
    changed.apply(&inverse).unwrap();
    same_world(&changed.to_manifest(), &doc.to_manifest()).unwrap();
}

#[test]
fn every_example_head_is_in_canonical_text() {
    for name in examples() {
        let path = repo(&format!("examples/{name}/manifest.json"));
        let text = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            openworldformat::manifest_text(&value),
            text,
            "{name}: canonical text"
        );
    }
    // A typed manifest writes its floats as written, not widened.
    let mut m = Manifest::new("t");
    let mut e = openworldformat::WorldEntity::new(1, "a");
    e.transform.position = [0.1, 2.0, -3.5];
    m.entities.push(e);
    let text = openworldformat::manifest_text_of(&m);
    assert!(text.contains("\"position\": [0.1, 2, -3.5]"), "{text}");
}

#[test]
fn an_entry_message_is_part_of_its_identity_in_every_reference() {
    // The golden vector the JS, Python, Swift and Kotlin references pin too.
    let line = r#"{"id":"x","parent":"e6","revision":7,"author":{"name":"claude"},"timestamp_ms":1790000000123,"message":"a lantern by the gate","ops":[{"DeleteEntity":{"id":21}}]}"#;
    let entry: openworldformat::OpLogEntry = serde_json::from_str(line).unwrap();
    assert_eq!(entry.message.as_deref(), Some("a lantern by the gate"));
    assert_eq!(
        openworldformat::compute_entry_id(&entry).unwrap(),
        "sha256:a7cd0955d35a2ff15b16ad7cc3440fb064d675ceceeca865a7ace463a5d177f2"
    );
}
