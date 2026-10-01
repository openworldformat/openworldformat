//! `world.schema.json` is the published JSON Schema of the format, and
//! this crate must generate exactly what LocalGPT's world-types
//! generates — same file, two crates, one schema. When they disagree,
//! one of them has drifted; this test is the guard on this side (the
//! world-types snapshot test is the other).

#![cfg(feature = "schema")]

#[test]
fn generated_schema_is_the_published_one() {
    let schema = schemars::schema_for!(openworldformat::WorldManifest);
    let generated = serde_json::to_string_pretty(&schema).unwrap();
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../schema/world.schema.json");
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        committed.trim_end(),
        generated.trim_end(),
        "world.schema.json is stale or drifted from this crate's types; \
         regenerate (cargo run --example schema --features schema) and \
         reconcile with LocalGPT's world-types"
    );
}
