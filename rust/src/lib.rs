//! # openworldformat (Rust)
//!
//! The Open World Format's Rust reference, extracted from LocalGPT's
//! `world-types`, `world-sync` and `world-physics` crates — the same
//! code that implemented the format since its origin, now in the
//! format's own home. The document (types + edits), the session fold
//! (log, branches, state), and the `ext-physics` extension's executable
//! half, in one serde-only crate: no Bevy, no async, no sockets.
//!
//! ```no_run
//! use openworldformat::{WorldManifest, OpLogEntry};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let manifest: WorldManifest = serde_json::from_str(
//!     std::fs::read_to_string("world/manifest.json")?.as_str(),
//! )?;
//! let entries: Vec<OpLogEntry> = std::fs::read_to_string("world/ops.jsonl")?
//!     .lines()
//!     .filter(|l| !l.trim().is_empty())
//!     .map(serde_json::from_str)
//!     .collect::<Result<_, _>>()?;
//! let doc = openworldformat::fold_log(&manifest.as_base()?, &entries)?;
//! println!("{} entities at head", doc.len());
//! # Ok(())
//! # }
//! ```
//!
//! Provenance and plan: LocalGPT remains the app-side consumer; its
//! crates flip to depend on this one (a re-export shim) when they're
//! quiet, ending the dual maintenance. Until then, both sides generate
//! the same `world.schema.json` — the schema snapshot test on each side
//! is the drift guard.

// ---- The document (L0): types and edits ----
pub mod asset;
pub mod audio;
pub mod avatar;
pub mod behavior;
pub mod creation;
pub mod entity;
pub mod history;
pub mod identity;
pub mod instance;
pub mod light;
pub mod material;
pub mod modulation;
pub mod shape;
pub mod soundtrack;
pub mod spatial;
pub mod tour;
pub mod trigger;
pub mod validation;
pub mod world;

// ---- The session (L1): the log and its folds ----
pub mod author;
pub mod doc;
pub mod oplog;
pub mod physics;
pub mod session;
pub mod state;

// The document's data model, at the root the way the format's readers
// expect it (mirrors the npm and PyPI packages' flat surface).
pub use asset::{MeshAssetRef, NodeOverride};
pub use audio::{AudioDef, AudioKind, AudioSource, FilterType, Rolloff, WaveformType};
pub use avatar::{AvatarDef, PointOfView};
pub use behavior::{BehaviorDef, PathMode};
pub use creation::{CreationDef, SemanticCategory};
pub use doc::{ApplyError, WorldDoc};
pub use entity::{EntityPatch, WorldEntity, WorldTransform, values_close};
pub use history::{AmbienceLayerDef, EditHistory, EditOp, WorldEdit};
pub use identity::{CreationId, EntityId, EntityName, EntityRef};
pub use instance::{
    InstanceOf, PartLink, PartOverride, expand_instances, part_links, validate_instances,
};
pub use light::{LightDef, LightType};
pub use material::{AlphaModeDef, MaterialDef, TextureSlot};
pub use modulation::{ModulationDef, ModulationTarget, SignalSource, StemKind};
pub use oplog::{OpLogEntry, decode_line, encode_line};
pub use session::{fold_log, fold_path};
pub use shape::{PrimitiveShapeKind, Shape};
pub use soundtrack::{SoundtrackDef, StemCurves, curve_at};
pub use spatial::ChunkCoord;
pub use state::{StateDoc, StateField, fold_state};
pub use tour::{TourDef, TourMode, TourWaypoint};
pub use trigger::{TriggerActionDef, TriggerDef, TriggerEvent, TriggerVolume};
pub use validation::{
    Severity, ValidationIssue, WorldLimits, validate_entities, validate_manifest,
};
pub use world::{CameraDef, ComplianceMeta, EnvironmentDef, WorldManifest, WorldMeta};

impl WorldManifest {
    /// The base document as a [`WorldDoc`]: entities spawned through
    /// the fold's own path so id/name maps stay honest.
    pub fn as_base(&self) -> Result<WorldDoc, ApplyError> {
        let mut doc = WorldDoc::new(self.meta.name.as_str());
        doc.environment = self.environment.clone();
        doc.camera = self.camera.clone();
        doc.avatar = self.avatar.clone();
        doc.tours = self.tours.clone();
        for entity in &self.entities {
            doc.apply(&EditOp::spawn(entity.clone()))?;
        }
        Ok(doc)
    }
}

/// The manifest schema version this crate reads (spec: schema v3).
pub const WORLD_SCHEMA_VERSION: u32 = crate::world::WORLD_SCHEMA_VERSION;
