//! LLM provenance — the `ext-provenance` extension
//! ([spec/extensions/provenance.md](https://openworldformat.org)).
//!
//! The core schema is governed independently of any single producer, so
//! it omits fields that track how a world was generated. Generative
//! authoring tools opt into this extension and write their lineage
//! under `meta["ext-provenance"]`; readers that don't know the
//! extension must-ignore it, like every `ext-*` namespace.

use serde::{Deserialize, Serialize};

/// The extension's namespace key, as it appears in `meta`.
pub const EXT_PROVENANCE_KEY: &str = "ext-provenance";

/// The fields this extension defines, under `meta["ext-provenance"]`.
pub const EXT_PROVENANCE_FIELDS: &[&str] = &[
    "prompt",
    "model",
    "generation_duration_ms",
    "biome",
    "semantic_category",
];

/// LLM lineage for generated content (`meta["ext-provenance"]`).
///
/// Every field optional: a tool writes what it knows. `prompt` carries
/// whatever the author typed, so it is preserved into every distributed
/// copy — tools circulating worlds MUST warn or scrub it before
/// publishing (the extension's privacy clause).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExtProvenance {
    /// The text the author typed to generate the content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// The LLM or generative model used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// How long generation took, milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_duration_ms: Option<u64>,
    /// A procedural generation hint: the environmental type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub biome: Option<String>,
    /// A tag aiding LLM reasoning and search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_category: Option<String>,
}

/// The core schema's view of the extension block: *anything*. The
/// namespace's shape is governed by the extension's registry entry
/// (spec/extensions/registry.json), not by `world.schema.json` — the
/// core schema names the door, the extension says what's behind it.
#[cfg(feature = "schema")]
pub(crate) fn core_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_under_the_extension_key() {
        let provenance = ExtProvenance {
            prompt: Some("a lighthouse over a foggy bay".into()),
            model: Some("claude-fable-5-1".into()),
            generation_duration_ms: Some(41_000),
            biome: Some("coastal".into()),
            semantic_category: Some("landmark".into()),
        };
        let json = serde_json::to_string(&provenance).unwrap();
        let back: ExtProvenance = serde_json::from_str(&json).unwrap();
        assert_eq!(back, provenance);
        // Untouched fields never appear on the wire.
        assert!(!json.contains("prompt\":null"));
    }

    #[test]
    fn empty_serializes_to_an_empty_object() {
        let json = serde_json::to_string(&ExtProvenance::default()).unwrap();
        assert_eq!(json, "{}");
    }
}
