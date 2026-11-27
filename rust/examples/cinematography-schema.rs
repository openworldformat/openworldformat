//! Print the JSON Schema of `ext-cinematography`.
//!
//! ```bash
//! cargo run --example cinematography-schema --features schema > ../schema/extensions/ext-cinematography.schema.json
//! ```

fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&openworldformat::cinematography::extension_schema()).unwrap()
    );
}
