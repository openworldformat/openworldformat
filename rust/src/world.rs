//! World manifest — the top-level world definition.
//!
//! Schema-versioned and designed for RON serialization.  Small worlds
//! store entities inline; large worlds split into per-chunk files.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::avatar::AvatarDef;
use crate::creation::CreationDef;
use crate::entity::WorldEntity;
use crate::soundtrack::SoundtrackDef;
use crate::tour::TourDef;

/// Current schema version. Increment when making breaking changes.
///
/// - 2: multi-file worlds (regions, libraries).
/// - 3: reusable creations and their instances (`CreationDef::parts`,
///   `WorldEntity::instance_of`), triggers, and mesh node overrides and
///   hashes.
pub const WORLD_SCHEMA_VERSION: u32 = 3;

/// Minimum supported version for loading. Update when dropping old format support.
///
/// 3, the current version: before launch the format carries no
/// compatibility with older files; the repo's own worlds are kept current.
pub const MIN_SUPPORTED_VERSION: u32 = 3;

/// Version compatibility error.
#[derive(Debug, Clone)]
pub enum VersionError {
    /// World file is too old to load.
    TooOld { found: u32, min: u32 },
    /// World file is from a newer version of the software.
    TooNew { found: u32, current: u32 },
}

/// The id space is exhausted: [`MAX_ENTITY_ID`](crate::MAX_ENTITY_ID)
/// ids spent, none left to give.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdCeilingError {
    /// What the allocator tried to hand out.
    pub next: u64,
}

impl fmt::Display for IdCeilingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "entity id ceiling reached: next would be {}, but ids stop at {} \
             (2^53 - 1, the JSON-safe integers)",
            self.next,
            crate::identity::MAX_ENTITY_ID
        )
    }
}

impl std::error::Error for IdCeilingError {}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionError::TooOld { found, min } => {
                write!(
                    f,
                    "version {} is too old (minimum supported: {})",
                    found, min
                )
            }
            VersionError::TooNew { found, current } => {
                write!(
                    f,
                    "version {} is from a newer localgpt-gen (current: {})",
                    found, current
                )
            }
        }
    }
}

/// Top-level world manifest — everything needed to save/load a world.
///
/// Conventions every renderer follows (Bevy through `localgpt-world-bevy`,
/// three.js through the web viewer):
///
/// - positions are world units, Y up;
/// - rotations are XYZ Euler angles in degrees;
/// - colours are RGBA in `0..=1`, sRGB-encoded, except `emissive`, which is
///   linear (values above 1 glow);
/// - directional light intensity is lux, point and spot lights are lumens,
///   spot angles are radians;
/// - asset paths are relative to the world's `assets/` folder.
///
/// The format names no engine: which renderer or engine version drew a
/// world is not part of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WorldManifest {
    /// Schema version for forward/backward migration. Required: a
    /// world that doesn't say which schema it speaks is refused, not
    /// guessed at (spec/versioning.md — the version gates parsing).
    pub version: u32,
    /// World metadata (name, description, tags, …).
    pub meta: WorldMeta,
    /// Environment settings (background, ambient light, fog).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<EnvironmentDef>,
    /// Default camera position/orientation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraDef>,
    /// Avatar configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<AvatarDef>,
    /// Guided tours.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tours: Vec<TourDef>,
    /// The song this world performs to, with the analysis that drives
    /// entity modulations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soundtrack: Option<SoundtrackDef>,
    /// Entities (inline for small worlds).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<WorldEntity>,
    /// Compound creations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub creations: Vec<CreationDef>,
    /// Next entity ID to allocate, bounded by
    /// [`MAX_ENTITY_ID`](crate::MAX_ENTITY_ID) (2^53 − 1) so ids stay
    /// JSON-safe in every reader.
    #[serde(default = "default_next_id")]
    #[cfg_attr(
        feature = "schema",
        schemars(transform = crate::identity::cap_json_safe)
    )]
    pub next_entity_id: u64,
}

/// World metadata.
///
/// The LLM lineage fields (`prompt`, `model`,
/// `generation_duration_ms`, `biome`, `semantic_category`) moved to
/// the [`ext-provenance`](crate::ext_provenance) extension — the core
/// schema is governed independently of any single producer
/// (spec/extensions/provenance.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WorldMeta {
    /// World name (used as skill name / directory name).
    pub name: String,
    /// Human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Time of day (0.0-24.0, for lighting presets).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_of_day: Option<f32>,

    // --- Gallery & experiment metadata (added for headless pipeline) ---
    /// Free-form style tags for gallery filtering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Generation source: "interactive", "headless", "experiment", "mcp".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// If part of a variation experiment, the group ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variation_group: Option<String>,
    /// Variation axis and value (e.g., ("lighting", "sunset")).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variation: Option<(String, String)>,
    /// Style name from memory (if applied).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_ref: Option<String>,
    // --- Regulatory compliance metadata ---
    /// Compliance metadata for distribution and regulatory classification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compliance: Option<ComplianceMeta>,
    /// LLM lineage, under the `ext-provenance` extension's namespace —
    /// the one extension the core types carry, because generation tools
    /// were already writing these fields and must not silently lose
    /// them in the move. In the generated core schema the block is
    /// free-form (`true`): an extension's shape belongs to its registry
    /// entry, not the core.
    #[serde(
        rename = "ext-provenance",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(
        feature = "schema",
        schemars(schema_with = "crate::ext_provenance::core_schema")
    )]
    pub ext_provenance: Option<crate::ext_provenance::ExtProvenance>,
}

/// Regulatory and distribution compliance metadata.
///
/// Records classification signals for storefronts, regulatory frameworks,
/// and content-origin seals so that exported worlds carry machine-readable
/// provenance alongside the creative data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ComplianceMeta {
    /// Steam "code tool" exemption flag.
    ///
    /// `true` indicates the output is compilable/editable source code (RON scene
    /// definitions, parametric shapes) rather than pre-made binary assets.  Under
    /// Valve's AI content policy, tools whose output is code that the developer
    /// compiles or modifies are treated as code tools, not AI-generated asset
    /// generators.
    #[serde(default = "default_true")]
    pub steam_code_tool_exempt: bool,

    /// EU AI Act risk-level classification.
    ///
    /// LocalGPT Gen is a code-generation tool: the LLM produces scene definition
    /// code (RON) that the user compiles into 3D geometry via Bevy.  Under the
    /// EU AI Act (Regulation 2024/1689), general-purpose code-generation tools
    /// with a human in the loop fall under the "minimal risk" tier, requiring
    /// only transparency obligations (Art. 52) -- no conformity assessment.
    #[serde(default = "default_risk_level")]
    pub eu_ai_act_risk_level: String,

    /// "No Gen AI" seal compatibility flag.
    ///
    /// `true` indicates the output is human-editable source code (not opaque
    /// binary blobs) and the creative direction is human-driven.  The scene
    /// definition can be fully read, understood, and modified by a person,
    /// making the output compatible with "No Gen AI" asset provenance
    /// requirements that focus on human authorship of the final artifact.
    #[serde(default = "default_true")]
    pub no_gen_ai_compatible: bool,

    /// Tool name and version that produced this world (e.g. "LocalGPT Gen v0.3.5").
    #[serde(default = "default_generation_tool")]
    pub generation_tool: String,

    /// How the output was produced: "code-generation" (LLM writes scene code
    /// compiled by the engine) vs "asset-generation" (LLM directly produces
    /// binary mesh/texture data).
    #[serde(default = "default_generation_method")]
    pub generation_method: String,

    /// Whether the output can be meaningfully edited by a human.
    ///
    /// `true` for LocalGPT Gen because the canonical format is RON text with
    /// parametric shapes -- users can open, read, and modify every dimension,
    /// material, and behavior by hand.
    #[serde(default = "default_true")]
    pub human_modifiable: bool,
}

/// Environment settings.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EnvironmentDef {
    /// Background/sky color, sRGB-encoded RGBA in `0..=1`.
    pub background_color: Option<[f32; 4]>,
    /// Ambient light brightness in Bevy's `GlobalAmbientLight` units
    /// (default 80; the web viewer scales it with `AMBIENT_SCALE`).
    pub ambient_intensity: Option<f32>,
    /// Ambient light color, sRGB-encoded RGBA in `0..=1`.
    pub ambient_color: Option<[f32; 4]>,
    /// Exponential fog density (0.0 = no fog).
    pub fog_density: Option<f32>,
    /// Fog color, sRGB-encoded RGBA in `0..=1`; the background color when
    /// unset.
    pub fog_color: Option<[f32; 4]>,
    /// Extension fields (`ext-*`), namespaced and must-ignored: what a
    /// reader doesn't understand rides along unchanged (the physics
    /// extension's gravity lives here today). An empty map serializes to
    /// nothing.
    #[cfg_attr(feature = "schema", schemars(default))]
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}

// Hand-written for the same reason as WorldEntity's: flatten can't read
// RON's named-struct form, and world.ron is full of it.
impl Serialize for EnvironmentDef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // See WorldEntity: struct-form without extension fields (RON
        // named, readable), map-form with them; JSON is identical
        // either way, and `Some` is written explicitly so RON's strict
        // `deserialize_option` round-trips (JSON ignores the markers).
        use crate::entity::{SomeRef, interned};
        use serde::ser::{SerializeMap, SerializeStruct};
        fn opt<M, T: Serialize>(
            store: &mut M,
            key: &'static str,
            value: &Option<T>,
        ) -> Result<(), M::Error>
        where
            M: Fields,
        {
            if value.is_some() {
                store.field(key, &SomeRef(value))
            } else {
                Ok(())
            }
        }
        // One trait over both serde stores so `opt` serves both forms.
        trait Fields {
            type Error;
            fn field<T: Serialize + ?Sized>(
                &mut self,
                key: &'static str,
                value: &T,
            ) -> Result<(), Self::Error>;
        }
        struct StructForm<'a, S>(&'a mut S);
        impl<S: SerializeStruct> Fields for StructForm<'_, S> {
            type Error = S::Error;
            fn field<T: Serialize + ?Sized>(
                &mut self,
                key: &'static str,
                value: &T,
            ) -> Result<(), S::Error> {
                self.0.serialize_field(key, value)
            }
        }
        struct MapForm<'a, S>(&'a mut S);
        impl<S: SerializeMap> Fields for MapForm<'_, S> {
            type Error = S::Error;
            fn field<T: Serialize + ?Sized>(
                &mut self,
                key: &'static str,
                value: &T,
            ) -> Result<(), S::Error> {
                self.0.serialize_entry(key, value)
            }
        }
        if self.extra.is_empty() {
            let mut s = serializer.serialize_struct("EnvironmentDef", 5)?;
            let mut form = StructForm(&mut s);
            opt(&mut form, "background_color", &self.background_color)?;
            opt(&mut form, "ambient_intensity", &self.ambient_intensity)?;
            opt(&mut form, "ambient_color", &self.ambient_color)?;
            opt(&mut form, "fog_density", &self.fog_density)?;
            opt(&mut form, "fog_color", &self.fog_color)?;
            s.end()
        } else {
            let mut m = serializer.serialize_map(Some(5 + self.extra.len()))?;
            let mut form = MapForm(&mut m);
            opt(&mut form, "background_color", &self.background_color)?;
            opt(&mut form, "ambient_intensity", &self.ambient_intensity)?;
            opt(&mut form, "ambient_color", &self.ambient_color)?;
            opt(&mut form, "fog_density", &self.fog_density)?;
            opt(&mut form, "fog_color", &self.fog_color)?;
            for (key, value) in &self.extra {
                m.serialize_entry(interned(key), value)?;
            }
            m.end()
        }
    }
}

impl<'de> Deserialize<'de> for EnvironmentDef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = EnvironmentDef;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an environment block")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<EnvironmentDef, A::Error> {
                let mut env = EnvironmentDef::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "background_color" => env.background_color = map.next_value()?,
                        "ambient_intensity" => env.ambient_intensity = map.next_value()?,
                        "ambient_color" => env.ambient_color = map.next_value()?,
                        "fog_density" => env.fog_density = map.next_value()?,
                        "fog_color" => env.fog_color = map.next_value()?,
                        _ => {
                            let value: serde_json::Value = map.next_value()?;
                            env.extra.insert(key, value);
                        }
                    }
                }
                Ok(env)
            }
        }
        // `deserialize_any`: the one entry that accepts RON named-struct
        // syntax AND map syntax AND JSON objects — see the impl note above.
        deserializer.deserialize_any(Visitor)
    }
}

/// Camera definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CameraDef {
    /// Camera position [x, y, z].
    #[serde(default = "default_camera_pos")]
    pub position: [f32; 3],
    /// Camera look-at target [x, y, z].
    #[serde(default)]
    pub look_at: [f32; 3],
    /// Vertical field of view in degrees.
    #[serde(default = "default_fov")]
    pub fov_degrees: f32,
}

impl Default for CameraDef {
    fn default() -> Self {
        Self {
            position: default_camera_pos(),
            look_at: [0.0, 0.0, 0.0],
            fov_degrees: default_fov(),
        }
    }
}

fn default_version() -> u32 {
    WORLD_SCHEMA_VERSION
}
fn default_next_id() -> u64 {
    1
}
fn default_camera_pos() -> [f32; 3] {
    [5.0, 5.0, 5.0]
}
fn default_fov() -> f32 {
    45.0
}
fn default_true() -> bool {
    true
}
fn default_risk_level() -> String {
    "minimal".to_string()
}
fn default_generation_tool() -> String {
    // A literal, deliberately: the published schema's default must not
    // churn with the generating crate's version (the snapshot tests on
    // both sides hold the two generators to one schema).
    "LocalGPT Gen".to_string()
}
fn default_generation_method() -> String {
    "code-generation".to_string()
}

impl Default for ComplianceMeta {
    fn default() -> Self {
        Self {
            steam_code_tool_exempt: true,
            eu_ai_act_risk_level: default_risk_level(),
            no_gen_ai_compatible: true,
            generation_tool: default_generation_tool(),
            generation_method: default_generation_method(),
            human_modifiable: true,
        }
    }
}

impl WorldManifest {
    /// Create a new empty world with a given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: default_version(),
            meta: WorldMeta {
                name: name.into(),
                description: None,
                time_of_day: None,
                tags: None,
                source: None,
                variation_group: None,
                variation: None,
                style_ref: None,
                compliance: Some(ComplianceMeta::default()),
                ext_provenance: None,
            },
            environment: None,
            camera: None,
            avatar: None,
            tours: Vec::new(),
            soundtrack: None,
            entities: Vec::new(),
            creations: Vec::new(),
            next_entity_id: default_next_id(),
        }
    }

    /// Check if this manifest's version is compatible with current code.
    pub fn check_version(&self) -> Result<(), VersionError> {
        if self.version < MIN_SUPPORTED_VERSION {
            Err(VersionError::TooOld {
                found: self.version,
                min: MIN_SUPPORTED_VERSION,
            })
        } else if self.version > WORLD_SCHEMA_VERSION {
            Err(VersionError::TooNew {
                found: self.version,
                current: WORLD_SCHEMA_VERSION,
            })
        } else {
            Ok(())
        }
    }

    /// Allocate and return the next entity ID, incrementing the counter.
    ///
    /// Refuses past [`MAX_ENTITY_ID`](crate::MAX_ENTITY_ID) — the
    /// allocator enforces the JSON-safe ceiling even where the language
    /// could count higher, so the worlds it generates never crash the
    /// references that read them.
    pub fn alloc_entity_id(&mut self) -> Result<crate::identity::EntityId, IdCeilingError> {
        if self.next_entity_id > crate::identity::MAX_ENTITY_ID {
            return Err(IdCeilingError {
                next: self.next_entity_id,
            });
        }
        let id = crate::identity::EntityId(self.next_entity_id);
        self.next_entity_id += 1;
        Ok(id)
    }

    /// Total entity count.
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Total triangle budget estimate.
    pub fn estimate_triangles(&self) -> usize {
        self.entities
            .iter()
            .filter_map(|e| e.shape.as_ref())
            .map(|s| s.estimate_triangles())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worlds_before_version_3_are_refused() {
        let json = r#"{"version": 2, "meta": {"name": "old"}}"#;
        let m: WorldManifest = serde_json::from_str(json).unwrap();
        assert!(matches!(
            m.check_version(),
            Err(VersionError::TooOld { found: 2, min: 3 })
        ));
    }
    use crate::entity::WorldEntity;
    use crate::shape::Shape;

    #[test]
    fn manifest_new() {
        let m = WorldManifest::new("test_world");
        assert_eq!(m.meta.name, "test_world");
        assert_eq!(m.version, WORLD_SCHEMA_VERSION);
        assert_eq!(m.next_entity_id, 1);
        assert!(m.entities.is_empty());
    }

    #[test]
    fn alloc_entity_id() {
        let mut m = WorldManifest::new("test");
        let id1 = m.alloc_entity_id().unwrap();
        let id2 = m.alloc_entity_id().unwrap();
        assert_eq!(id1.0, 1);
        assert_eq!(id2.0, 2);
        assert_eq!(m.next_entity_id, 3);
    }

    #[test]
    fn alloc_entity_id_stops_at_the_json_safe_ceiling() {
        let mut m = WorldManifest::new("test");
        m.next_entity_id = crate::identity::MAX_ENTITY_ID;
        assert_eq!(
            m.alloc_entity_id().unwrap().0,
            crate::identity::MAX_ENTITY_ID
        );
        assert_eq!(
            m.alloc_entity_id(),
            Err(IdCeilingError {
                next: crate::identity::MAX_ENTITY_ID + 1
            })
        );
        // The counter never crosses the line, even after refusing.
        assert_eq!(m.next_entity_id, crate::identity::MAX_ENTITY_ID + 1);
    }

    #[test]
    fn version_is_required_not_defaulted() {
        // A manifest that doesn't say which schema it speaks is refused,
        // not guessed at (the version gates parsing).
        let json = r#"{"meta": {"name": "silent"}}"#;
        assert!(serde_json::from_str::<WorldManifest>(json).is_err());
    }

    #[test]
    fn llm_lineage_lives_under_ext_provenance() {
        let json = r#"{
            "version": 3,
            "meta": {
                "name": "generated",
                "ext-provenance": {
                    "prompt": "a lighthouse over a foggy bay",
                    "model": "claude-fable-5-1",
                    "generation_duration_ms": 41000,
                    "biome": "coastal",
                    "semantic_category": "landmark"
                }
            }
        }"#;
        let m: WorldManifest = serde_json::from_str(json).unwrap();
        let provenance = m.meta.ext_provenance.clone().expect("carried");
        assert_eq!(
            provenance.prompt.as_deref(),
            Some("a lighthouse over a foggy bay")
        );
        assert_eq!(provenance.biome.as_deref(), Some("coastal"));
        assert_eq!(provenance.semantic_category.as_deref(), Some("landmark"));
        // Round trip keeps it under the extension's key, not the core's.
        let back = serde_json::to_value(&m).unwrap();
        assert!(back["meta"]["ext-provenance"]["model"].is_string());
        assert!(back["meta"]["prompt"].is_null());
    }

    #[test]
    fn the_v2_multi_file_references_are_gone() {
        // The ghost fields: nothing wrote them, nothing read them — the
        // schema dropped them, and the type finally agrees.
        let json = r#"{"version": 3, "meta": {"name": "w"},
            "layout_file": "layout.json", "region_files": ["r1.json"],
            "behavior_files": [], "audio_files": [], "avatar_file": null}"#;
        let m: WorldManifest = serde_json::from_str(json).unwrap();
        let back = serde_json::to_value(&m).unwrap();
        for ghost in [
            "layout_file",
            "region_files",
            "behavior_files",
            "audio_files",
            "avatar_file",
        ] {
            assert!(back[ghost].is_null(), "{ghost} must not round-trip");
        }
        // Strict mode flags them (see strict.rs) — here they are merely
        // must-ignored, like any unknown field.
    }

    #[test]
    fn manifest_roundtrip_json() {
        let mut m = WorldManifest::new("roundtrip_test");
        m.meta.description = Some("A test world".to_string());
        m.environment = Some(EnvironmentDef {
            background_color: Some([0.1, 0.1, 0.2, 1.0]),
            ambient_intensity: Some(0.3),
            ambient_color: None,
            fog_density: None,
            fog_color: None,
            extra: std::collections::BTreeMap::new(),
        });
        m.camera = Some(CameraDef::default());
        m.avatar = Some(AvatarDef::default());
        m.entities
            .push(WorldEntity::new(1, "cube").with_shape(Shape::Cuboid {
                x: 2.0,
                y: 2.0,
                z: 2.0,
            }));
        m.next_entity_id = 2;

        let json = serde_json::to_string_pretty(&m).unwrap();
        let back: WorldManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn triangle_estimate() {
        let mut m = WorldManifest::new("budget_test");
        m.entities
            .push(WorldEntity::new(1, "cube").with_shape(Shape::Cuboid {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            }));
        m.entities
            .push(WorldEntity::new(2, "sphere").with_shape(Shape::Sphere { radius: 1.0 }));
        assert!(m.estimate_triangles() > 0);
    }

    #[test]
    fn version_check_current() {
        let m = WorldManifest::new("test");
        assert!(m.check_version().is_ok());
    }

    #[test]
    fn version_check_too_old() {
        let mut m = WorldManifest::new("test");
        m.version = 0; // Below MIN_SUPPORTED_VERSION
        let err = m.check_version().unwrap_err();
        match err {
            VersionError::TooOld { found, min } => {
                assert_eq!(found, 0);
                assert_eq!(min, MIN_SUPPORTED_VERSION);
            }
            _ => panic!("Expected TooOld error"),
        }
    }

    #[test]
    fn version_check_too_new() {
        let mut m = WorldManifest::new("test");
        m.version = 99; // Above WORLD_SCHEMA_VERSION
        let err = m.check_version().unwrap_err();
        match err {
            VersionError::TooNew { found, current } => {
                assert_eq!(found, 99);
                assert_eq!(current, WORLD_SCHEMA_VERSION);
            }
            _ => panic!("Expected TooNew error"),
        }
    }

    #[test]
    fn compliance_meta_default() {
        let c = ComplianceMeta::default();
        assert!(c.steam_code_tool_exempt);
        assert_eq!(c.eu_ai_act_risk_level, "minimal");
        assert!(c.no_gen_ai_compatible);
        assert!(c.generation_tool.starts_with("LocalGPT Gen")); // a literal now — the schema must not churn with crate versions
        assert_eq!(c.generation_method, "code-generation");
        assert!(c.human_modifiable);
    }

    #[test]
    fn compliance_roundtrip_json() {
        let c = ComplianceMeta::default();
        let json = serde_json::to_string_pretty(&c).unwrap();
        let back: ComplianceMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn manifest_new_includes_compliance() {
        let m = WorldManifest::new("compliance_test");
        assert!(m.meta.compliance.is_some());
        let c = m.meta.compliance.unwrap();
        assert!(c.steam_code_tool_exempt);
        assert_eq!(c.eu_ai_act_risk_level, "minimal");
        assert!(c.no_gen_ai_compatible);
        assert!(c.human_modifiable);
        assert_eq!(c.generation_method, "code-generation");
    }
}
