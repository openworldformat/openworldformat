//! Entity identity — dual ID + name system.
//!
//! Entities have a stable numeric [`EntityId`] (survives renames) and a
//! human-readable [`EntityName`] (LLM-friendly).  Cross-entity references
//! use [`EntityRef`] which can be either form and is resolved to an ID on
//! first use.

use serde::{Deserialize, Serialize};

/// The ceiling on entity ids: 2^53 − 1, the largest integer every JSON
/// number implementation round-trips exactly. Rust could hold `u64`,
/// but a world that spends ids past this line crashes the references
/// that read it — so every allocator enforces the same ceiling
/// (spec/versioning.md's "don't lie to a reader", as arithmetic).
pub const MAX_ENTITY_ID: u64 = 9007199254740991;

/// Cap an id's schema at the JSON-safe ceiling and drop the 64-bit
/// `format` hint: the wire contract is 53-bit, and advertising `uint64`
/// would invite exactly the overflow the ceiling exists to prevent.
#[cfg(feature = "schema")]
pub(crate) fn cap_json_safe(schema: &mut schemars::Schema) {
    if let Some(object) = schema.as_object_mut() {
        object.remove("format");
        object.insert(
            "maximum".to_string(),
            serde_json::Value::from(MAX_ENTITY_ID),
        );
    }
}

/// Stable, monotonically increasing entity identifier.
///
/// Bounded by [`MAX_ENTITY_ID`] (2^53 − 1) on the wire: the format's
/// contract with JSON-safe integers, so a world written by a 64-bit
/// allocator never overflows the references that read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(transform = cap_json_safe))]
pub struct EntityId(pub u64);

/// Human-readable entity name. Unique within a world but may be renamed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EntityName(pub String);

impl EntityName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EntityName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Cross-entity reference — used in behaviors, parenting, audio attachment.
///
/// LLMs produce `Name` references; these are resolved to `Id` on ingestion.
/// Saved worlds should only contain `Id` references.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum EntityRef {
    /// Stable reference by numeric ID (for persisted worlds).
    Id(EntityId),
    /// Human-readable reference by name (for LLM tool calls).
    Name(String),
}

impl EntityRef {
    pub fn name(name: impl Into<String>) -> Self {
        Self::Name(name.into())
    }

    pub fn id(id: u64) -> Self {
        Self::Id(EntityId(id))
    }
}

/// Unique identifier for a compound creation (group of entities).
///
/// Carries the same JSON-safe ceiling as [`EntityId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(transform = cap_json_safe))]
pub struct CreationId(pub u64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_ref_name_serializes_as_string() {
        let r = EntityRef::Name("campfire".to_string());
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, r#""campfire""#);
    }

    #[test]
    fn entity_ref_id_serializes_as_object() {
        let r = EntityRef::Id(EntityId(42));
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("42"));
    }

    #[test]
    fn entity_name_display() {
        let name = EntityName::new("sun_lamp");
        assert_eq!(format!("{}", name), "sun_lamp");
    }
}
