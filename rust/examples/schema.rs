//! Print the JSON Schema of `WorldManifest`.
//!
//! ```bash
//! cargo run --example schema --features schema > ../schema/world.schema.json
//! ```
//!
//! `tests/schema_snapshot.rs` fails when the committed file is stale —
//! and LocalGPT's world-types snapshot must agree with it.

fn main() {
    let schema = schemars::schema_for!(openworldformat::WorldManifest);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
