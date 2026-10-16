//! The authoring outcome suite (conformance/authoring/cases.json) through
//! this crate's `authoring::ingest`: each batch over the base world, and
//! what must come back.

use std::path::PathBuf;

use openworldformat::authoring::{ingest, Refused};
use openworldformat::{WorldDoc, WorldManifest};
use serde_json::Value;

fn suite() -> Value {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/authoring/cases.json");
    serde_json::from_str(&std::fs::read_to_string(&path).expect("the authoring suite"))
        .expect("suite json")
}

/// A JSON pointer into a value (`/a/0/b`), or `None` where it leads nowhere.
fn at<'a>(value: &'a Value, pointer: &str) -> Option<&'a Value> {
    value.pointer(pointer)
}

/// Numbers compare by value — the suite writes `2`, an f32 field reads back `2.0`.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => (x - y).abs() <= 1e-6 * x.abs().max(y.abs()).max(1.0),
            _ => false,
        },
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

fn check_world(name: &str, doc: &WorldDoc, predicates: &[Value]) {
    for p in predicates {
        let entity = p["entity"].as_str().expect("entity name");
        let found = doc.get_by_name(entity);
        if p["absent"].as_bool() == Some(true) {
            assert!(
                found.is_none(),
                "{name}: entity '{entity}' should be absent"
            );
            continue;
        }
        let found = found.unwrap_or_else(|| panic!("{name}: no entity '{entity}'"));
        let json = serde_json::to_value(found).unwrap();
        let path = p["path"].as_str().expect("path");
        let got = at(&json, path)
            .unwrap_or_else(|| panic!("{name}: '{entity}' has nothing at {path}: {json}"));
        assert!(
            same(got, &p["equals"]),
            "{name}: '{entity}' at {path} is {got}, expected {}",
            p["equals"]
        );
    }
}

#[test]
fn authoring_outcomes_hold() {
    let suite = suite();
    let base: WorldManifest = serde_json::from_value(suite["base"].clone()).expect("base manifest");
    let base = WorldDoc::from_manifest(&base).expect("base loads");
    let cases = suite["cases"].as_array().expect("cases");
    assert!(!cases.is_empty());
    for case in cases {
        let name = case["name"].as_str().unwrap_or("?");
        let expect = &case["expect"];
        let world_predicates: Vec<Value> = expect["world"].as_array().cloned().unwrap_or_default();
        match (ingest(&base, &case["batch"]), expect["result"].as_str()) {
            (Ok(done), Some("committed")) => {
                if let Some(want) = expect["errors_contain"].as_array().and_then(|a| a.first()) {
                    panic!("{name}: committed, but the suite expects an error containing {want}");
                }
                for want in expect["warnings_contain"].as_array().into_iter().flatten() {
                    let want = want.as_str().unwrap();
                    assert!(
                        done.warnings.iter().any(|w| w.contains(want)),
                        "{name}: no warning contains {want:?}: {:?}",
                        done.warnings
                    );
                }
                if let Some(spawned) = expect["spawned"].as_object() {
                    for (n, id) in spawned {
                        assert_eq!(
                            done.spawned.get(n).copied(),
                            id.as_u64(),
                            "{name}: spawned {n} -> {:?}",
                            done.spawned
                        );
                    }
                }
                for p in expect["ops"].as_array().into_iter().flatten() {
                    let i = p["op"].as_u64().expect("op index") as usize;
                    let op = serde_json::to_value(&done.ops[i]).unwrap();
                    let path = p["path"].as_str().expect("path");
                    let got = at(&op, path)
                        .unwrap_or_else(|| panic!("{name}: op {i} has nothing at {path}: {op}"));
                    assert!(
                        same(got, &p["equals"]),
                        "{name}: op {i} at {path} is {got}, expected {}",
                        p["equals"]
                    );
                }
                check_world(name, &done.doc, &world_predicates);
            }
            (Err(Refused { errors }), Some("refused")) => {
                for want in expect["errors_contain"].as_array().into_iter().flatten() {
                    let want = want.as_str().unwrap();
                    assert!(
                        errors.iter().any(|e| e.contains(want)),
                        "{name}: no error contains {want:?}: {errors:?}"
                    );
                }
                check_world(name, &base, &world_predicates);
            }
            (Ok(_), other) => panic!("{name}: committed, expected {other:?}"),
            (Err(refused), other) => panic!("{name}: refused ({refused}), expected {other:?}"),
        }
    }
}
