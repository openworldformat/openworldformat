//! Undo/redo history — edit operations and their inverses.

use serde::{Deserialize, Serialize};

use crate::audio::AudioDef;
use crate::entity::{EntityPatch, WorldEntity};
use crate::identity::EntityId;
use crate::world::{CameraDef, EnvironmentDef};

/// A recorded world edit with its inverse for undo support.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WorldEdit {
    /// Monotonically increasing sequence number.
    pub seq: u64,
    /// The operation that was performed.
    pub op: EditOp,
    /// The inverse operation (for undo).
    pub inverse: EditOp,
    /// Timestamp in milliseconds since epoch.
    pub timestamp_ms: u64,
    /// Who performed the edit (e.g., "user", "llm", agent ID).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

/// An atomic edit operation on the world.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum EditOp {
    /// Spawn a new entity.
    SpawnEntity { entity: WorldEntity },
    /// Delete an entity by ID.
    DeleteEntity { id: EntityId },
    /// Modify an entity with a patch.
    ModifyEntity { id: EntityId, patch: EntityPatch },
    /// Set environment (background color, ambient light, fog).
    SetEnvironment { env: EnvironmentDef },
    /// Set camera position, look-at target, and FOV.
    SetCamera { camera: CameraDef },
    /// Set ambient soundscape (replaces all layers).
    SetAmbience { ambience: Vec<AmbienceLayerDef> },
    /// Spawn an audio emitter attached to an entity or position.
    SpawnAudioEmitter { name: String, audio: AudioDef },
    /// Remove an audio emitter by name.
    RemoveAudioEmitter { name: String, audio: AudioDef },
    /// Change the world's scene-wide fields (meta, environment, camera,
    /// avatar, tours, soundtrack, ambience, creations): absent leaves a
    /// field alone, `null` clears it, a value sets it.
    ModifyWorld {
        patch: Box<crate::world_patch::WorldPatch>,
    },
    /// A batch of atomic operations (all-or-nothing).
    Batch { ops: Vec<EditOp> },
}

/// A single ambient audio layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AmbienceLayerDef {
    /// Layer name (e.g., "wind", "rain").
    pub name: String,
    /// The sound source.
    pub source: crate::audio::AudioSource,
    /// Volume (0.0–1.0).
    pub volume: f32,
}

impl EditOp {
    /// Compute the inverse of this operation.
    ///
    /// For `SpawnEntity`, the inverse is `DeleteEntity`.
    /// For `DeleteEntity`, the caller must provide the entity state to restore.
    /// For `ModifyEntity`, the caller must provide the previous state.
    /// For `Batch`, the inverse is a batch of inverses in reverse order.
    pub fn compute_inverse_spawn(entity: &WorldEntity) -> EditOp {
        EditOp::DeleteEntity { id: entity.id }
    }

    /// Compute the inverse of this operation against the document it is
    /// about to change — **undo is appending the inverse** (the log
    /// never rewinds), so the inverse must be computed at the time of
    /// the edit, while the before-state is still in hand.
    ///
    /// The rule the spec states and this implements
    /// (spec/session.md, "The op kinds"):
    /// - `SpawnEntity` inverses to `DeleteEntity`;
    /// - `DeleteEntity` inverses to a `Batch` of `SpawnEntity` ops
    ///   holding a **deep copy of the deleted tree** (subtree,
    ///   parents before children, so re-spawning applies);
    /// - `ModifyEntity` inverses to a `ModifyEntity` restoring the old
    ///   values — a patch that sets back exactly what the forward patch
    ///   touched;
    /// - the scene-wide sets inverse to the scene they replace
    ///   (defaults where none existed);
    /// - audio emitters swap roles symmetrically;
    /// - `Batch` inverses to a batch of inverses in reverse order, each
    ///   taken against the state its op was about to change.
    pub fn compute_inverse(
        &self,
        doc: &crate::doc::WorldDoc,
    ) -> Result<EditOp, crate::doc::ApplyError> {
        use crate::doc::ApplyError;
        match self {
            EditOp::SpawnEntity { entity } => Ok(EditOp::DeleteEntity { id: entity.id }),
            EditOp::DeleteEntity { id } => {
                if !doc.contains(id.0) {
                    return Err(ApplyError::MissingEntity(id.0));
                }
                // The deleted tree, deep-copied, parents first: applying
                // the batch re-spawns exactly what deleting removed
                // (descendants go with a delete, so they come back with it).
                let ops = doc
                    .subtree_entities_parent_first(id.0)
                    .into_iter()
                    .cloned()
                    .map(EditOp::spawn)
                    .collect();
                Ok(EditOp::Batch { ops })
            }
            EditOp::ModifyEntity { id, patch } => {
                let current = doc.get(id.0).ok_or(ApplyError::MissingEntity(id.0))?;
                Ok(EditOp::modify(*id, inverse_patch(patch, current)))
            }
            // A scene setting that didn't exist comes back as absent,
            // not as a default one.
            EditOp::SetEnvironment { .. } => Ok(match &doc.environment {
                Some(env) => EditOp::SetEnvironment { env: env.clone() },
                None => EditOp::ModifyWorld {
                    patch: Box::new(crate::world_patch::WorldPatch {
                        environment: Some(None),
                        ..Default::default()
                    }),
                },
            }),
            EditOp::SetCamera { .. } => Ok(match &doc.camera {
                Some(camera) => EditOp::SetCamera {
                    camera: camera.clone(),
                },
                None => EditOp::ModifyWorld {
                    patch: Box::new(crate::world_patch::WorldPatch {
                        camera: Some(None),
                        ..Default::default()
                    }),
                },
            }),
            EditOp::ModifyWorld { patch } => Ok(EditOp::ModifyWorld {
                patch: Box::new(inverse_world_patch(patch, doc)),
            }),
            EditOp::SetAmbience { .. } => Ok(EditOp::SetAmbience {
                ambience: doc.ambience.clone(),
            }),
            EditOp::SpawnAudioEmitter { name, audio } => Ok(EditOp::RemoveAudioEmitter {
                name: name.clone(),
                audio: audio.clone(),
            }),
            EditOp::RemoveAudioEmitter { name, .. } => {
                let id = doc
                    .entity_id_by_name(name)
                    .ok_or_else(|| ApplyError::MissingName(name.clone()))?;
                let audio = doc
                    .get(id)
                    .and_then(|e| e.audio.clone())
                    .ok_or_else(|| ApplyError::MissingName(name.clone()))?;
                Ok(EditOp::SpawnAudioEmitter {
                    name: name.clone(),
                    audio,
                })
            }
            EditOp::Batch { ops } => {
                // Each op's inverse sees the state its op was about to
                // change: walk forward over a trial, collecting inverses,
                // then reverse — the undo of a batch replays it backwards.
                let mut trial = doc.clone();
                let mut inverses = Vec::with_capacity(ops.len());
                for op in ops {
                    inverses.push(op.compute_inverse(&trial)?);
                    trial.apply(op)?;
                }
                inverses.reverse();
                Ok(EditOp::Batch { ops: inverses })
            }
        }
    }

    /// Create a spawn operation for an entity.
    pub fn spawn(entity: WorldEntity) -> EditOp {
        EditOp::SpawnEntity { entity }
    }

    /// Create a delete operation.
    pub fn delete(id: EntityId) -> EditOp {
        EditOp::DeleteEntity { id }
    }

    /// Create a modify operation.
    pub fn modify(id: EntityId, patch: EntityPatch) -> EditOp {
        EditOp::ModifyEntity { id, patch }
    }
}

/// The patch that undoes `patch` against `current`: every slot the
/// forward patch sets, the inverse sets back to the entity's value —
/// or clears, where the entity never had the field.
fn inverse_patch(patch: &EntityPatch, current: &WorldEntity) -> EntityPatch {
    let mut inverse = EntityPatch::default();
    if patch.name.is_some() {
        inverse.name = Some(current.name.clone());
    }
    if patch.transform.is_some() {
        inverse.transform = Some(current.transform.clone());
    }
    if patch.parent.is_some() {
        inverse.parent = Some(current.parent);
    }
    if patch.shape.is_some() {
        inverse.shape = Some(current.shape.clone());
    }
    if patch.material.is_some() {
        inverse.material = Some(current.material.clone());
    }
    if patch.light.is_some() {
        inverse.light = Some(current.light.clone());
    }
    if patch.behaviors.is_some() {
        inverse.behaviors = Some(current.behaviors.clone());
    }
    if patch.audio.is_some() {
        inverse.audio = Some(current.audio.clone());
    }
    if patch.mesh_asset.is_some() {
        inverse.mesh_asset = Some(current.mesh_asset.clone());
    }
    if patch.modulations.is_some() {
        inverse.modulations = Some(current.modulations.clone());
    }
    if patch.instance_of.is_some() {
        inverse.instance_of = Some(current.instance_of.clone());
    }
    if patch.triggers.is_some() {
        inverse.triggers = Some(current.triggers.clone());
    }
    // Extension fields set by the forward patch go back to whatever the
    // entity carried — absent extension fields clear.
    for key in patch.extra.keys() {
        inverse
            .extra
            .insert(key.clone(), current.extra.get(key).cloned());
    }
    inverse
}

/// The world patch that undoes `patch`: every field it touches set back
/// to the document's value (cleared where the document had none).
fn inverse_world_patch(
    patch: &crate::world_patch::WorldPatch,
    doc: &crate::doc::WorldDoc,
) -> crate::world_patch::WorldPatch {
    use crate::world_patch::WorldPatch;
    WorldPatch {
        meta: patch.meta.as_ref().map(|_| doc.meta()),
        environment: patch.environment.as_ref().map(|_| doc.environment.clone()),
        camera: patch.camera.as_ref().map(|_| doc.camera.clone()),
        avatar: patch.avatar.as_ref().map(|_| doc.avatar.clone()),
        tours: patch.tours.as_ref().map(|_| doc.tours.clone()),
        soundtrack: patch.soundtrack.as_ref().map(|_| doc.soundtrack.clone()),
        ambience: patch.ambience.as_ref().map(|_| doc.ambience.clone()),
        creations: patch.creations.as_ref().map(|_| doc.creations.clone()),
    }
}

/// Edit history — append-only log of world edits.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EditHistory {
    /// All edits in chronological order.
    pub edits: Vec<WorldEdit>,
    /// Current position in the edit log (for undo/redo).
    /// Points to the next edit to be undone.
    pub cursor: usize,
}

impl EditHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new edit. Truncates any redo history.
    pub fn push(&mut self, op: EditOp, inverse: EditOp, author: Option<String>) {
        // Truncate redo history
        self.edits.truncate(self.cursor);

        let seq = self.edits.len() as u64;
        self.edits.push(WorldEdit {
            seq,
            op,
            inverse,
            timestamp_ms: 0, // Caller should set this
            author,
        });
        self.cursor = self.edits.len();
    }

    /// Get the next operation to undo, if any.
    pub fn undo(&mut self) -> Option<&EditOp> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        Some(&self.edits[self.cursor].inverse)
    }

    /// Get the next operation to redo, if any.
    pub fn redo(&mut self) -> Option<&EditOp> {
        if self.cursor >= self.edits.len() {
            return None;
        }
        let op = &self.edits[self.cursor].op;
        self.cursor += 1;
        Some(op)
    }

    /// Number of operations that can be undone.
    pub fn undo_count(&self) -> usize {
        self.cursor
    }

    /// Number of operations that can be redone.
    pub fn redo_count(&self) -> usize {
        self.edits.len() - self.cursor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::WorldDoc;
    use crate::entity::WorldEntity;

    #[test]
    fn undoing_a_delete_restores_the_whole_tree() {
        // The rule: DeleteEntity inverses to a Batch of SpawnEntity ops
        // containing a deep copy of the deleted tree — descendants and
        // all, parents before children.
        let mut doc = WorldDoc::new("w");
        doc.apply(&EditOp::spawn(WorldEntity::new(1, "house")))
            .unwrap();
        let mut roof = WorldEntity::new(2, "roof");
        roof.parent = Some(EntityId(1));
        doc.apply(&EditOp::spawn(roof)).unwrap();
        let mut chimney = WorldEntity::new(3, "chimney");
        chimney.parent = Some(EntityId(2));
        doc.apply(&EditOp::spawn(chimney)).unwrap();
        doc.apply(&EditOp::spawn(WorldEntity::new(4, "tree")))
            .unwrap();

        let delete = EditOp::delete(EntityId(1));
        let inverse = delete.compute_inverse(&doc).unwrap();
        let EditOp::Batch { ops } = &inverse else {
            panic!("a delete's inverse is a batch of spawns");
        };
        assert_eq!(ops.len(), 3, "the subtree, one spawn each");
        // Parents before children, so the batch applies.
        doc.apply(&delete).unwrap();
        assert_eq!(doc.len(), 1);
        doc.apply(&inverse).unwrap();
        assert_eq!(doc.len(), 4);
        assert_eq!(doc.get(3).unwrap().parent, Some(EntityId(2)));
        assert_eq!(doc.get_by_name("chimney").unwrap().id, EntityId(3));
    }

    #[test]
    fn every_edit_has_a_computable_inverse() {
        let mut doc = WorldDoc::new("w");
        doc.apply(&EditOp::spawn(WorldEntity::new(1, "lamp")))
            .unwrap();

        let spawn = EditOp::spawn(WorldEntity::new(9, "beacon"));
        let spawn_inverse = spawn.compute_inverse(&doc).unwrap();
        doc.apply(&spawn).unwrap();
        doc.apply(&spawn_inverse).unwrap();
        assert!(doc.get(9).is_none());

        let modify = EditOp::modify(
            EntityId(1),
            EntityPatch {
                name: Some(crate::identity::EntityName::new("torch")),
                light: Some(None),
                ..Default::default()
            },
        );
        let modify_inverse = modify.compute_inverse(&doc).unwrap();
        doc.apply(&modify).unwrap();
        assert!(doc.get(1).unwrap().light.is_none());
        doc.apply(&modify_inverse).unwrap();
        assert_eq!(doc.get_by_name("lamp").unwrap().id, EntityId(1));

        let set_env = EditOp::SetEnvironment {
            env: crate::world::EnvironmentDef::default(),
        };
        let env_inverse = set_env.compute_inverse(&doc).unwrap();
        doc.apply(&set_env).unwrap();
        doc.apply(&env_inverse).unwrap();

        // Batches inverse in reverse, each against the state its op changed.
        let batch = EditOp::Batch {
            ops: vec![
                EditOp::spawn(WorldEntity::new(5, "a")),
                EditOp::spawn(WorldEntity::new(6, "b")),
            ],
        };
        let batch_inverse = batch.compute_inverse(&doc).unwrap();
        doc.apply(&batch).unwrap();
        assert_eq!(doc.len(), 3);
        doc.apply(&batch_inverse).unwrap();
        assert_eq!(doc.len(), 1);
        assert!(doc.get(5).is_none() && doc.get(6).is_none());
    }

    #[test]
    fn undo_redo_basic() {
        let mut history = EditHistory::new();

        let entity = WorldEntity::new(1, "cube");
        let op = EditOp::spawn(entity.clone());
        let inverse = EditOp::delete(entity.id);

        history.push(op, inverse, None);
        assert_eq!(history.undo_count(), 1);
        assert_eq!(history.redo_count(), 0);

        // Undo
        let undo_op = history.undo().unwrap();
        assert!(matches!(undo_op, EditOp::DeleteEntity { .. }));
        assert_eq!(history.undo_count(), 0);
        assert_eq!(history.redo_count(), 1);

        // Redo
        let redo_op = history.redo().unwrap();
        assert!(matches!(redo_op, EditOp::SpawnEntity { .. }));
        assert_eq!(history.undo_count(), 1);
        assert_eq!(history.redo_count(), 0);
    }

    #[test]
    fn new_edit_truncates_redo() {
        let mut history = EditHistory::new();

        // Push two edits
        for i in 0..2 {
            let entity = WorldEntity::new(i, format!("e{i}"));
            history.push(
                EditOp::spawn(entity.clone()),
                EditOp::delete(entity.id),
                None,
            );
        }

        // Undo one
        history.undo();
        assert_eq!(history.redo_count(), 1);

        // Push a new edit — should truncate redo
        let entity = WorldEntity::new(99, "new");
        history.push(
            EditOp::spawn(entity.clone()),
            EditOp::delete(entity.id),
            None,
        );
        assert_eq!(history.redo_count(), 0);
        assert_eq!(history.undo_count(), 2); // first + new
    }

    #[test]
    fn edit_op_set_environment_roundtrip() {
        let op = EditOp::SetEnvironment {
            env: crate::world::EnvironmentDef {
                background_color: Some([0.1, 0.2, 0.3, 1.0]),
                ambient_intensity: Some(0.5),
                ambient_color: Some([1.0, 1.0, 0.9, 1.0]),
                fog_density: Some(0.02),
                fog_color: None,

                extra: ::std::collections::BTreeMap::new(),
            },
        };
        let json = serde_json::to_string(&op).unwrap();
        let back: EditOp = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, EditOp::SetEnvironment { .. }));
    }

    #[test]
    fn edit_op_set_camera_roundtrip() {
        let op = EditOp::SetCamera {
            camera: crate::world::CameraDef {
                position: [10.0, 5.0, 10.0],
                look_at: [0.0, 0.0, 0.0],
                fov_degrees: 60.0,
            },
        };
        let json = serde_json::to_string(&op).unwrap();
        let back: EditOp = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, EditOp::SetCamera { .. }));
    }

    #[test]
    fn edit_op_batch_roundtrip() {
        let entity = WorldEntity::new(5, "test_batch");
        let op = EditOp::Batch {
            ops: vec![
                EditOp::delete(EntityId(1)),
                EditOp::spawn(entity),
                EditOp::SetEnvironment {
                    env: crate::world::EnvironmentDef {
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        ambient_intensity: None,
                        ambient_color: None,
                        fog_density: None,
                        fog_color: None,

                        extra: ::std::collections::BTreeMap::new(),
                    },
                },
            ],
        };
        let json = serde_json::to_string(&op).unwrap();
        let back: EditOp = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, EditOp::Batch { ops } if ops.len() == 3));
    }

    #[test]
    fn edit_op_modify_entity_roundtrip() {
        let op = EditOp::ModifyEntity {
            id: EntityId(42),
            patch: EntityPatch {
                name: Some(crate::identity::EntityName::new("renamed")),
                shape: Some(Some(crate::shape::Shape::Sphere { radius: 3.0 })),
                light: Some(None), // Clearing the light
                ..Default::default()
            },
        };
        let json = serde_json::to_string(&op).unwrap();
        let back: EditOp = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, EditOp::ModifyEntity { id, .. } if id.0 == 42));
    }
}
