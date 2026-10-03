//! Strict mode — must-ignore's off switch (spec/profiles.md).
//!
//! The default reader is tolerant by rule: unknown fields ride along or
//! drop, unregistered extensions fold to nothing, a torn line loses at
//! most itself. That is the right posture for *reading the world*; it
//! is the wrong posture for an authoring tool, which wants to hear
//! about every typo the moment it makes one — silent skipping is how
//! custom content gets erased on the next save (spec/package.md's Safe
//! Mode warning, parser edition).
//!
//! The checks run on the raw JSON before the typed parse, because the
//! typed parse is where unknown fields politely disappear.

use std::collections::BTreeSet;
use std::fmt;

use crate::oplog::{self, OpLogEntry};
use crate::world::WorldManifest;

/// The extensions the registry knows
/// ([spec/extensions/registry.json](https://openworldformat.org)). A
/// strict reader refuses `ext-*` namespaces not in this list — an
/// unregistered extension is a typo wearing a namespace.
pub const REGISTERED_EXTENSIONS: &[&str] = &[
    "ext-physics",
    "ext-strict-determinism",
    "ext-visibility",
    "ext-cinematography",
    "ext-provenance",
];

/// The keys a manifest's top level may carry (schema v3, post-pruning:
/// the v2 multi-file references are gone and now say so).
const MANIFEST_KEYS: &[&str] = &[
    "version",
    "meta",
    "entities",
    "environment",
    "camera",
    "avatar",
    "tours",
    "soundtrack",
    "creations",
    "next_entity_id",
];

/// The keys `meta` may carry. The LLM lineage fields are deliberately
/// absent — they moved to `ext-provenance`, and strict mode says where.
const META_KEYS: &[&str] = &[
    "name",
    "description",
    "time_of_day",
    "tags",
    "source",
    "variation_group",
    "variation",
    "style_ref",
    "compliance",
];

/// The lineage fields that moved to the `ext-provenance` extension.
const LEGACY_LINEAGE_KEYS: &[&str] = &[
    "prompt",
    "model",
    "generation_duration_ms",
    "biome",
    "semantic_category",
];

/// The keys an entity may carry.
const ENTITY_KEYS: &[&str] = &[
    "id",
    "name",
    "parent",
    "transform",
    "chunk",
    "shape",
    "material",
    "light",
    "audio",
    "behaviors",
    "modulations",
    "triggers",
    "mesh_asset",
    "instance_of",
    "creation_id",
];

/// Why strict mode refused. The message is the diagnosis; the path is
/// where to look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrictError {
    /// A key the level doesn't know (path names it).
    UnknownField { path: String, key: String },
    /// One of the LLM lineage keys at `meta`'s top level — moved to
    /// `meta["ext-provenance"]` (spec/extensions/provenance.md).
    LegacyLineage { key: String },
    /// An `ext-*` namespace the registry doesn't list.
    UnregisteredExtension { key: String },
    /// A log op recognized by no rule — in strict mode, a refusal.
    UnknownOpKind,
    /// The underlying JSON wouldn't parse at all.
    Parse(String),
}

impl fmt::Display for StrictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownField { path, key } => {
                write!(f, "strict: unknown field '{key}' at {path}")
            }
            Self::LegacyLineage { key } => write!(
                f,
                "strict: meta.{key} moved to the ext-provenance extension \
                 (meta[\"ext-provenance\"].{key})"
            ),
            Self::UnregisteredExtension { key } => write!(
                f,
                "strict: '{key}' is not in the extension registry \
                 (known: {})",
                REGISTERED_EXTENSIONS.join(", ")
            ),
            Self::UnknownOpKind => {
                write!(f, "strict: an op recognized by no rule")
            }
            Self::Parse(detail) => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for StrictError {}

/// Parse a manifest strictly: unknown fields, legacy lineage keys and
/// unregistered extensions are refusals, not silent skips.
pub fn parse_manifest_strict(text: &str) -> Result<WorldManifest, StrictError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| StrictError::Parse(e.to_string()))?;
    check_manifest(&value)?;
    serde_json::from_value(value).map_err(|e| StrictError::Parse(e.to_string()))
}

/// Parse one log line strictly: every op must classify, and extension
/// ops must be registered.
pub fn decode_line_strict(line: &str) -> Result<OpLogEntry, StrictError> {
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|e| StrictError::Parse(e.to_string()))?;
    check_entry(&value)?;
    oplog::decode_line(line).map_err(|e| StrictError::Parse(e.to_string()))
}

/// The manifest's own checks, over the raw JSON.
fn check_manifest(value: &serde_json::Value) -> Result<(), StrictError> {
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    check_keys(object, "manifest", MANIFEST_KEYS)?;
    if let Some(meta) = object.get("meta").filter(|m| m.is_object()) {
        check_meta_keys(meta.as_object().expect("checked"))?;
    }
    if let Some(entities) = object.get("entities").and_then(|e| e.as_array()) {
        for (n, entity) in entities.iter().enumerate() {
            if let Some(keys) = entity.as_object() {
                check_keys(keys, &format!("entities[{n}]"), ENTITY_KEYS)?;
            }
        }
    }
    Ok(())
}

/// `meta`'s keys, with the lineage move called out by name.
fn check_meta_keys(meta: &serde_json::Map<String, serde_json::Value>) -> Result<(), StrictError> {
    for key in meta.keys() {
        if let Some(field) = LEGACY_LINEAGE_KEYS.iter().find(|k| *k == key) {
            return Err(StrictError::LegacyLineage {
                key: field.to_string(),
            });
        }
    }
    check_keys(meta, "meta", META_KEYS)
}

/// The entry's ops: each must classify; extensions must be registered.
fn check_entry(value: &serde_json::Value) -> Result<(), StrictError> {
    let Some(ops) = value.get("ops").and_then(|o| o.as_array()) else {
        return Ok(());
    };
    for op in ops {
        let Some(keys) = op.as_object() else {
            return Err(StrictError::UnknownOpKind);
        };
        // The shape rules, in the classifier's own order: an edit is
        // exactly one PascalCase key; an extension is one `ext-*` key
        // the registry knows; everything else must parse as a history
        // kind, or it is a refusal.
        if keys.len() == 1 {
            let name = keys.keys().next().expect("one key");
            if crate::session::is_edit_key(name) {
                continue;
            }
            if name.starts_with("ext-") {
                if !REGISTERED_EXTENSIONS.contains(&name.as_str()) {
                    return Err(StrictError::UnregisteredExtension { key: name.clone() });
                }
                continue;
            }
        }
        let parsed: Result<crate::session::SessionOp, _> = serde_json::from_value(op.clone());
        if parsed.is_err() {
            return Err(StrictError::UnknownOpKind);
        }
    }
    Ok(())
}

/// One level's unknown-key check: known keys plus registered `ext-*`.
fn check_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    path: &str,
    allowed: &[&str],
) -> Result<(), StrictError> {
    for key in object.keys() {
        if allowed.contains(&key.as_str()) {
            continue;
        }
        if key.starts_with("ext-") {
            if !REGISTERED_EXTENSIONS.contains(&key.as_str()) {
                return Err(StrictError::UnregisteredExtension { key: key.clone() });
            }
            continue;
        }
        return Err(StrictError::UnknownField {
            path: path.to_string(),
            key: key.clone(),
        });
    }
    Ok(())
}

/// The set of registered extensions, as a set (for tools that want one).
pub fn registered_extensions() -> BTreeSet<&'static str> {
    REGISTERED_EXTENSIONS.iter().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_json(extra: &str) -> String {
        format!(r#"{{"version": 3, "meta": {{"name": "w"{extra}}}}}"#)
    }

    #[test]
    fn strict_refuses_unknown_manifest_fields() {
        // The ghost that the schema already dropped, said out loud.
        let err = parse_manifest_strict(
            r#"{"version": 3, "meta": {"name": "w"}, "layout_file": "l.json"}"#,
        )
        .unwrap_err();
        assert_eq!(
            err,
            StrictError::UnknownField {
                path: "manifest".into(),
                key: "layout_file".into()
            }
        );
        assert!(parse_manifest_strict(&manifest_json("")).is_ok());
    }

    #[test]
    fn strict_points_lineage_at_the_extension() {
        let err = parse_manifest_strict(&manifest_json(r#", "prompt": "a bay""#)).unwrap_err();
        assert_eq!(
            err,
            StrictError::LegacyLineage {
                key: "prompt".into()
            }
        );
        // Under the extension, the same field is fine.
        assert!(
            parse_manifest_strict(&manifest_json(r#", "ext-provenance": {"prompt": "a bay"}"#))
                .is_ok()
        );
    }

    #[test]
    fn strict_refuses_unregistered_extensions() {
        let err =
            parse_manifest_strict(r#"{"version": 3, "meta": {"name": "w"}, "ext-mystery": {}}"#)
                .unwrap_err();
        assert_eq!(
            err,
            StrictError::UnregisteredExtension {
                key: "ext-mystery".into()
            }
        );
        // Registered ones pass.
        assert!(
            parse_manifest_strict(r#"{"version": 3, "meta": {"name": "w"}, "ext-visibility": {}}"#)
                .is_ok()
        );
    }

    #[test]
    fn strict_logs_refuse_unknown_kinds_and_unregistered_extensions() {
        let good = r#"{"revision":1,"ops":[{"SpawnEntity":{"entity":{"id":1,"name":"a"}}}]}"#;
        assert!(decode_line_strict(good).is_ok());
        assert!(decode_line_strict(r#"{"revision":1,"ops":[{"tool":"t","args":{}}]}"#).is_ok());
        assert_eq!(
            decode_line_strict(r#"{"revision":1,"ops":[{"mystery": 1}]}"#).unwrap_err(),
            StrictError::UnknownOpKind
        );
        assert_eq!(
            decode_line_strict(r#"{"revision":1,"ops":[{"ext-mystery": {}}]}"#).unwrap_err(),
            StrictError::UnregisteredExtension {
                key: "ext-mystery".into()
            }
        );
        assert!(decode_line_strict(r#"{"revision":1,"ops":[{"ext-physics": {}}]}"#).is_ok());
    }
}
