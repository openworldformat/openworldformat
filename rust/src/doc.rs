//! The shared document: a world's entities by id plus its scene-wide
//! settings, changed only by applying world-types [`EditOp`]s.
//!
//! Application is validated (ids and names unique, parents present, no
//! cycles, finite transforms) and atomic: a [`EditOp::Batch`] applies every
//! op or none. Deleting an entity deletes its descendants, as despawning does
//! in Bevy and removing a node does in three.js.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use crate as wt;
use crate::entity_refs::{EntityRefKind, EntityRefScope, RefSlot, RefWalk, refs_of_kind};
use crate::identity::{EntityId, EntityRef};
use wt::{EditOp, WorldEntity};

/// Longest entity name the document accepts.
pub const MAX_ENTITY_NAME_LEN: usize = 256;

/// Why an op didn't apply. The document is unchanged when this is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    /// `SpawnEntity` with an id the document already has.
    DuplicateId(u64),
    /// A name another entity already uses.
    DuplicateName(String),
    /// The op names an entity id the document doesn't have.
    MissingEntity(u64),
    /// The op names an entity by a name the document doesn't have.
    MissingName(String),
    /// An entity's parent isn't in the document.
    MissingParent { id: u64, parent: u64 },
    /// The parent change would make an entity its own ancestor.
    ParentCycle(u64),
    /// Values the format can't hold (non-finite numbers, empty names, …).
    Invalid(String),
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId(id) => write!(f, "entity {id} already exists"),
            Self::DuplicateName(name) => write!(f, "an entity named '{name}' already exists"),
            Self::MissingEntity(id) => write!(f, "no entity {id}"),
            Self::MissingName(name) => write!(f, "no entity named '{name}'"),
            Self::MissingParent { id, parent } => {
                write!(f, "entity {id}'s parent {parent} doesn't exist")
            }
            Self::ParentCycle(id) => write!(f, "entity {id} can't be its own ancestor"),
            Self::Invalid(detail) => write!(f, "invalid: {detail}"),
        }
    }
}

impl std::error::Error for ApplyError {}

/// A world as a collaborative session shares it: every field a manifest
/// holds, so the fold's state is always a whole manifest
/// ([`WorldDoc::to_manifest`] round-trips [`WorldDoc::from_manifest`]).
#[derive(Debug, Clone, Default)]
pub struct WorldDoc {
    /// World name (the manifest's `meta.name`).
    pub name: String,
    entities: BTreeMap<u64, WorldEntity>,
    names: HashMap<String, u64>,
    /// The manifest schema version the world was read at.
    pub version: u32,
    /// The rest of `meta` (its `name` is [`WorldDoc::name`]).
    meta: Option<wt::WorldMeta>,
    pub environment: Option<wt::EnvironmentDef>,
    pub camera: Option<wt::CameraDef>,
    pub avatar: Option<wt::AvatarDef>,
    pub tours: Vec<wt::TourDef>,
    pub soundtrack: Option<wt::SoundtrackDef>,
    pub ambience: Vec<wt::AmbienceLayerDef>,
    pub creations: Vec<wt::CreationDef>,
    /// The lowest id the next spawn may take: ids are never reused, so
    /// this only grows — deleting the newest entity doesn't free its id.
    next_floor: u64,
}

impl WorldDoc {
    /// An empty world.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: crate::world::WORLD_SCHEMA_VERSION,
            next_floor: 1,
            ..Self::default()
        }
    }

    /// A document holding a manifest's inline entities and scene settings.
    ///
    /// An entity whose parent isn't in the manifest becomes a root rather
    /// than failing the whole world.
    pub fn from_manifest(manifest: &wt::WorldManifest) -> Result<Self, ApplyError> {
        let mut doc = Self::new(manifest.meta.name.clone());
        doc.set_scene(manifest);
        doc.load_entities(manifest.entities.iter().cloned())?;
        Ok(doc)
    }

    /// Take every field of `manifest` but its entities.
    pub fn set_scene(&mut self, manifest: &wt::WorldManifest) {
        self.name = manifest.meta.name.clone();
        self.version = manifest.version;
        self.meta = Some(manifest.meta.clone());
        self.environment = manifest.environment.clone();
        self.camera = manifest.camera.clone();
        self.avatar = manifest.avatar.clone();
        self.tours = manifest.tours.clone();
        self.soundtrack = manifest.soundtrack.clone();
        self.ambience = manifest.ambience.clone();
        self.creations = manifest.creations.clone();
        self.next_floor = self.next_floor.max(manifest.next_entity_id);
    }

    /// The world's metadata, its name included.
    pub fn meta(&self) -> wt::WorldMeta {
        let mut meta = self
            .meta
            .clone()
            .unwrap_or_else(|| wt::WorldManifest::new("").meta);
        meta.name = self.name.clone();
        meta
    }

    /// A document holding exactly `entities` (a scene projection), with the
    /// given scene settings. Used to recover when a diff doesn't apply.
    pub fn from_entities(
        name: impl Into<String>,
        entities: &[WorldEntity],
        environment: Option<wt::EnvironmentDef>,
    ) -> Result<Self, ApplyError> {
        let mut doc = Self::new(name);
        doc.environment = environment;
        doc.load_entities(entities.iter().cloned())?;
        Ok(doc)
    }

    fn load_entities(
        &mut self,
        entities: impl IntoIterator<Item = WorldEntity>,
    ) -> Result<(), ApplyError> {
        for entity in entities {
            validate_entity(&entity)?;
            let id = entity.id.0;
            if self.entities.contains_key(&id) {
                return Err(ApplyError::DuplicateId(id));
            }
            if self.names.contains_key(entity.name.as_str()) {
                return Err(ApplyError::DuplicateName(entity.name.0.clone()));
            }
            self.names.insert(entity.name.0.clone(), id);
            self.entities.insert(id, entity);
        }
        // Dangling parents become roots.
        let dangling: Vec<u64> = self
            .entities
            .values()
            .filter(|e| e.parent.is_some_and(|p| !self.entities.contains_key(&p.0)))
            .map(|e| e.id.0)
            .collect();
        for id in dangling {
            if let Some(entity) = self.entities.get_mut(&id) {
                entity.parent = None;
            }
        }
        // A cycle can't be repaired without guessing; refuse it.
        for &id in self.entities.keys() {
            if self.ancestors_contain(id, id) {
                return Err(ApplyError::ParentCycle(id));
            }
        }
        Ok(())
    }

    /// The document as a manifest: entities inline, parents before children.
    ///
    /// Every field is the document's: a world read with
    /// [`WorldDoc::from_manifest`] and folded with an empty log comes back
    /// as it was, up to entity order (spec/session.md, "The fold is
    /// total").
    pub fn to_manifest(&self) -> wt::WorldManifest {
        let mut manifest = wt::WorldManifest::new(self.name.clone());
        manifest.version = self.version;
        manifest.meta = self.meta();
        manifest.environment = self.environment.clone();
        manifest.camera = self.camera.clone();
        manifest.avatar = self.avatar.clone();
        manifest.tours = self.tours.clone();
        manifest.soundtrack = self.soundtrack.clone();
        manifest.ambience = self.ambience.clone();
        manifest.creations = self.creations.clone();
        manifest.entities = self.entities_parent_first().into_iter().cloned().collect();
        manifest.next_entity_id = self.next_id();
        manifest
    }

    /// Number of entities.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// No entities.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Entity by id.
    pub fn get(&self, id: u64) -> Option<&WorldEntity> {
        self.entities.get(&id)
    }

    /// Entity by name.
    pub fn get_by_name(&self, name: &str) -> Option<&WorldEntity> {
        self.names.get(name).and_then(|id| self.entities.get(id))
    }

    /// The id a name resolves to, if the document has it.
    pub fn entity_id_by_name(&self, name: &str) -> Option<u64> {
        self.names.get(name).copied()
    }

    /// Whether an entity id is present.
    pub fn contains(&self, id: u64) -> bool {
        self.entities.contains_key(&id)
    }

    /// Entities in id order.
    pub fn entities(&self) -> impl Iterator<Item = &WorldEntity> {
        self.entities.values()
    }

    /// One more than the largest entity id (1 for an empty world),
    /// never past [`MAX_ENTITY_ID`](crate::MAX_ENTITY_ID): a document
    /// at the ceiling has nothing left to hand out, and says so by
    /// sitting on it rather than overflowing past it.
    pub fn next_id(&self) -> u64 {
        self.entities
            .keys()
            .next_back()
            .map_or(1, |id| id + 1)
            .max(self.next_floor)
            .min(crate::identity::MAX_ENTITY_ID)
    }

    /// Entities ordered so every parent comes before its children; roots and
    /// siblings in id order.
    pub fn entities_parent_first(&self) -> Vec<&WorldEntity> {
        let mut children: HashMap<u64, Vec<u64>> = HashMap::new();
        let mut roots = Vec::new();
        for entity in self.entities.values() {
            match entity.parent {
                Some(parent) => children.entry(parent.0).or_default().push(entity.id.0),
                None => roots.push(entity.id.0),
            }
        }
        let mut out = Vec::with_capacity(self.entities.len());
        let mut stack: Vec<u64> = roots.into_iter().rev().collect();
        while let Some(id) = stack.pop() {
            if let Some(entity) = self.entities.get(&id) {
                out.push(entity);
            }
            if let Some(kids) = children.get(&id) {
                stack.extend(kids.iter().rev());
            }
        }
        out
    }

    /// Nesting depth of an entity: 0 for a root.
    pub fn depth(&self, id: u64) -> usize {
        let mut depth = 0;
        let mut current = self.entities.get(&id).and_then(|e| e.parent);
        while let Some(parent) = current {
            depth += 1;
            if depth > self.entities.len() {
                break;
            }
            current = self.entities.get(&parent.0).and_then(|e| e.parent);
        }
        depth
    }

    /// Whether `target` is `id` or one of its ancestors' ids, walking up from
    /// `id`'s parent.
    fn ancestors_contain(&self, id: u64, target: u64) -> bool {
        let mut current = self.entities.get(&id).and_then(|e| e.parent);
        let mut steps = 0;
        while let Some(parent) = current {
            if parent.0 == target {
                return true;
            }
            steps += 1;
            if steps > self.entities.len() {
                return true;
            }
            current = self.entities.get(&parent.0).and_then(|e| e.parent);
        }
        false
    }

    /// `id` and all of its descendants.
    fn subtree(&self, id: u64) -> Vec<u64> {
        let mut children: HashMap<u64, Vec<u64>> = HashMap::new();
        for entity in self.entities.values() {
            if let Some(parent) = entity.parent {
                children.entry(parent.0).or_default().push(entity.id.0);
            }
        }
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = vec![id];
        while let Some(next) = stack.pop() {
            if !seen.insert(next) {
                continue;
            }
            out.push(next);
            if let Some(kids) = children.get(&next) {
                stack.extend(kids);
            }
        }
        out
    }

    /// An entity and all its descendants, parents before children — the
    /// order needed to re-spawn a deleted subtree.
    pub fn subtree_entities_parent_first(&self, id: u64) -> Vec<&WorldEntity> {
        let ids: HashSet<u64> = self.subtree(id).into_iter().collect();
        self.entities_parent_first()
            .into_iter()
            .filter(|e| ids.contains(&e.id.0))
            .collect()
    }

    /// Apply one op. A `Batch` applies all of its ops or none of them; on
    /// error the document is unchanged.
    ///
    /// This is the op-level primitive; name references bind one level
    /// up, at entry ingestion ([`WorldDoc::apply_entry`]) — the fold's
    /// unit of commitment.
    pub fn apply(&mut self, op: &EditOp) -> Result<(), ApplyError> {
        match op {
            EditOp::Batch { ops } => {
                let mut scratch = self.clone();
                for op in ops {
                    scratch.apply_in_place(op)?;
                }
                *self = scratch;
                Ok(())
            }
            other => self.apply_in_place(other),
        }
    }

    /// Apply a sequence of ops atomically (as one batch).
    pub fn apply_all(&mut self, ops: &[EditOp]) -> Result<(), ApplyError> {
        let mut scratch = self.clone();
        for op in ops {
            scratch.apply_in_place(op)?;
        }
        *self = scratch;
        Ok(())
    }

    /// Apply one log entry's edits and bind the names it introduced:
    /// the ingest unit of the fold (spec/world.md, "Identity"). Names
    /// in entities the entry spawned or modified resolve to ids
    /// against the fold-so-far *including this entry* — an entry is
    /// atomic, so same-entry references bind — and a name nothing owns
    /// refuses the entry, exactly as an op that no longer applies
    /// would. Delaying resolution to fold time is forbidden: a later
    /// rename must not retroactively rebind what an entry meant.
    pub fn apply_entry(&mut self, ops: &[EditOp]) -> Result<(), ApplyError> {
        // One transactional copy, not two. Calling `apply_all` here would
        // make its own, and this scratch is already exclusive and already
        // discarded on error, so that copy bought nothing and cost O(n) per
        // entry.
        let mut scratch = self.clone();
        scratch.apply_entry_in_place(ops)?;
        *self = scratch;
        Ok(())
    }

    /// Apply an entry *without* the transactional copy, leaving a partially
    /// applied document behind if an op fails.
    ///
    /// For callers that own the document and discard it on error — which is
    /// what folding a log is: `fold_log` clones the base once and returns
    /// `Err` without the document if an entry no longer applies, so a copy
    /// per entry protects nothing. That copy is O(document), so paying it
    /// per entry makes a fold quadratic in the log's length: 8,000 entries
    /// cost 7.6 s before this existed and 0.19 s after.
    ///
    /// Name binding is unchanged: the names an entry introduces resolve
    /// against the fold-so-far *including this entry*, because an entry is
    /// still the unit of ingestion even when it is not the unit of rollback.
    ///
    /// Prefer [`WorldDoc::apply_entry`] on a document anything else can
    /// observe. The contract here is "you keep the pieces".
    pub fn apply_entry_in_place(&mut self, ops: &[EditOp]) -> Result<(), ApplyError> {
        let touched = touched_ids(ops);
        for op in ops {
            self.apply_in_place(op)?;
        }
        self.resolve_refs(&touched)
    }

    /// Resolve name references to ids in the given entities, against
    /// this document as it stands (the fold-so-far).
    pub fn resolve_refs(&mut self, ids: &[u64]) -> Result<(), ApplyError> {
        // Disjoint field borrows, not a copy of the name map: `entities` and
        // `names` are different fields, so the borrow checker allows one
        // mutably and the other shared. Cloning here cost O(n) on every
        // entry of a fold.
        let names = &self.names;
        let entities = &mut self.entities;
        for &id in ids {
            let Some(entity) = entities.get_mut(&id) else {
                continue;
            };
            // The schema's marked entity refs (spec/world.md, "Identity"),
            // except the top-level ones (`parent`): those bind at op
            // intake, before the op applies — a string there never
            // reaches the typed model — so only the deeper refs (the
            // behavior refs) resolve here.
            for field in refs_of_kind(EntityRefScope::Entity, EntityRefKind::Bindable) {
                if field.path.len() == 1 {
                    continue;
                }
                entity.walk(field.path, &mut |slot| bind_slot(slot, names))?;
            }
        }
        Ok(())
    }

    /// Resolve every name reference in the document against the
    /// document: what loading a base manifest does. Two-phase by
    /// necessity — a manifest may list a referencing entity before the
    /// entity it references, so binding waits until all of them are in.
    pub fn resolve_all_refs(&mut self) -> Result<(), ApplyError> {
        let ids: Vec<u64> = self.entities.keys().copied().collect();
        self.resolve_refs(&ids)?;
        let names = &self.names;
        if let Some(avatar) = &mut self.avatar {
            for field in refs_of_kind(EntityRefScope::Avatar, EntityRefKind::Bindable) {
                avatar.walk(field.path, &mut |slot| bind_slot(slot, names))?;
            }
        }
        Ok(())
    }

    /// Apply without the batch copy. Only atomic for single, non-batch ops,
    /// which validate before they change anything.
    fn apply_in_place(&mut self, op: &EditOp) -> Result<(), ApplyError> {
        match op {
            EditOp::SpawnEntity { entity } => {
                validate_entity(entity)?;
                let id = entity.id.0;
                if self.entities.contains_key(&id) {
                    return Err(ApplyError::DuplicateId(id));
                }
                if self.names.contains_key(entity.name.as_str()) {
                    return Err(ApplyError::DuplicateName(entity.name.0.clone()));
                }
                if let Some(parent) = entity.parent
                    && !self.entities.contains_key(&parent.0)
                {
                    return Err(ApplyError::MissingParent {
                        id,
                        parent: parent.0,
                    });
                }
                self.names.insert(entity.name.0.clone(), id);
                self.entities.insert(id, entity.clone());
                self.next_floor = self.next_floor.max(id.saturating_add(1));
                Ok(())
            }
            EditOp::DeleteEntity { id } => {
                if !self.entities.contains_key(&id.0) {
                    return Err(ApplyError::MissingEntity(id.0));
                }
                for gone in self.subtree(id.0) {
                    if let Some(entity) = self.entities.remove(&gone) {
                        self.names.remove(entity.name.as_str());
                    }
                }
                Ok(())
            }
            EditOp::ModifyEntity { id, patch } => {
                let current = self
                    .entities
                    .get(&id.0)
                    .ok_or(ApplyError::MissingEntity(id.0))?;
                let mut next = current.clone();
                patch.apply(&mut next);
                next.id = *id;
                validate_entity(&next)?;
                if next.name != current.name
                    && let Some(&holder) = self.names.get(next.name.as_str())
                    && holder != id.0
                {
                    return Err(ApplyError::DuplicateName(next.name.0.clone()));
                }
                if next.parent != current.parent
                    && let Some(parent) = next.parent
                {
                    if !self.entities.contains_key(&parent.0) {
                        return Err(ApplyError::MissingParent {
                            id: id.0,
                            parent: parent.0,
                        });
                    }
                    if parent.0 == id.0 || self.ancestors_contain(parent.0, id.0) {
                        return Err(ApplyError::ParentCycle(id.0));
                    }
                }
                let old_name = current.name.0.clone();
                if old_name != next.name.0 {
                    self.names.remove(&old_name);
                    self.names.insert(next.name.0.clone(), id.0);
                }
                self.entities.insert(id.0, next);
                Ok(())
            }
            EditOp::SetEnvironment { env } => {
                self.environment = Some(env.clone());
                Ok(())
            }
            EditOp::SetCamera { camera } => {
                self.camera = Some(camera.clone());
                Ok(())
            }
            EditOp::SetAmbience { ambience } => {
                self.ambience = ambience.clone();
                Ok(())
            }
            EditOp::SpawnAudioEmitter { name, audio } => {
                let id = *self
                    .names
                    .get(name)
                    .ok_or_else(|| ApplyError::MissingName(name.clone()))?;
                if let Some(entity) = self.entities.get_mut(&id) {
                    entity.audio = Some(audio.clone());
                }
                Ok(())
            }
            EditOp::RemoveAudioEmitter { name, .. } => {
                let id = *self
                    .names
                    .get(name)
                    .ok_or_else(|| ApplyError::MissingName(name.clone()))?;
                if let Some(entity) = self.entities.get_mut(&id) {
                    entity.audio = None;
                }
                Ok(())
            }
            EditOp::ModifyWorld { patch } => {
                if let Some(meta) = &patch.meta
                    && meta.name.trim().is_empty()
                {
                    return Err(ApplyError::Invalid("a world's name can't be empty".into()));
                }
                if let Some(meta) = &patch.meta {
                    self.name = meta.name.clone();
                    self.meta = Some(meta.clone());
                }
                if let Some(env) = &patch.environment {
                    self.environment = env.clone();
                }
                if let Some(camera) = &patch.camera {
                    self.camera = camera.clone();
                }
                if let Some(avatar) = &patch.avatar {
                    self.avatar = avatar.clone();
                }
                if let Some(tours) = &patch.tours {
                    self.tours = tours.clone();
                }
                if let Some(soundtrack) = &patch.soundtrack {
                    self.soundtrack = soundtrack.clone();
                }
                if let Some(ambience) = &patch.ambience {
                    self.ambience = ambience.clone();
                }
                if let Some(creations) = &patch.creations {
                    self.creations = creations.clone();
                }
                Ok(())
            }
            EditOp::Batch { ops } => {
                for op in ops {
                    self.apply_in_place(op)?;
                }
                Ok(())
            }
        }
    }
}

/// The entities an entry's edits touch — the ones whose names bind at
/// ingestion. Spawns introduce references; modifies replace them.
fn touched_ids(ops: &[EditOp]) -> Vec<u64> {
    let mut out = Vec::new();
    fn walk(ops: &[EditOp], out: &mut Vec<u64>) {
        for op in ops {
            match op {
                EditOp::SpawnEntity { entity } => out.push(entity.id.0),
                EditOp::ModifyEntity { id, .. } => out.push(id.0),
                EditOp::Batch { ops } => walk(ops, out),
                _ => {}
            }
        }
    }
    walk(ops, &mut out);
    out
}

/// Bind a marked slot's name to the id it names now, against the
/// fold-so-far's names — the ingestion rule (spec/world.md, "Identity").
/// An id slot can't hold a name; a reference already holding an id is
/// left alone.
fn bind_slot(slot: RefSlot<'_>, names: &HashMap<String, u64>) -> Result<(), ApplyError> {
    if let RefSlot::Ref(reference) = slot
        && let EntityRef::Name(name) = &*reference
    {
        let id = names
            .get(name.as_str())
            .copied()
            .ok_or_else(|| ApplyError::MissingName(name.clone()))?;
        *reference = EntityRef::Id(EntityId(id));
    }
    Ok(())
}

/// Check what the format can't hold: empty or over-long names,
/// non-finite transforms, and ids past the JSON-safe ceiling.
fn validate_entity(entity: &WorldEntity) -> Result<(), ApplyError> {
    if entity.id.0 > crate::identity::MAX_ENTITY_ID {
        return Err(ApplyError::Invalid(format!(
            "entity {} exceeds the id ceiling {} (2^53-1)",
            entity.id.0,
            crate::identity::MAX_ENTITY_ID
        )));
    }
    let name = entity.name.as_str();
    if name.trim().is_empty() {
        return Err(ApplyError::Invalid(format!(
            "entity {} has an empty name",
            entity.id.0
        )));
    }
    if name.len() > MAX_ENTITY_NAME_LEN {
        return Err(ApplyError::Invalid(format!(
            "entity {}'s name is longer than {MAX_ENTITY_NAME_LEN} bytes",
            entity.id.0
        )));
    }
    let t = &entity.transform;
    let finite = t
        .position
        .iter()
        .chain(&t.rotation_degrees)
        .chain(&t.scale)
        .all(|v| v.is_finite());
    if !finite {
        return Err(ApplyError::Invalid(format!(
            "entity '{name}' has a non-finite transform"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wt::{EntityId, EntityName, EntityPatch};

    fn entity(id: u64, name: &str, parent: Option<u64>) -> WorldEntity {
        let mut e = WorldEntity::new(id, name);
        e.parent = parent.map(EntityId);
        e
    }

    fn spawn(id: u64, name: &str, parent: Option<u64>) -> EditOp {
        EditOp::spawn(entity(id, name, parent))
    }

    #[test]
    fn an_entry_applied_in_place_reaches_the_same_document() {
        let ops = [spawn(1, "a", None), spawn(2, "b", Some(1))];

        let mut transactional = WorldDoc::new("w");
        transactional.apply_entry(&ops).unwrap();

        let mut in_place = WorldDoc::new("w");
        in_place.apply_entry_in_place(&ops).unwrap();

        assert_eq!(transactional.len(), in_place.len());
        assert_eq!(transactional.next_id(), in_place.next_id());
        for entity in transactional.entities() {
            assert_eq!(in_place.get(entity.id.0), Some(entity));
        }
    }

    #[test]
    fn a_failed_entry_leaves_the_document_alone_but_in_place_keeps_the_pieces() {
        // The second op names a parent nothing owns, so the entry fails.
        let ops = [spawn(1, "a", None), spawn(2, "b", Some(99))];

        let mut transactional = WorldDoc::new("w");
        assert!(transactional.apply_entry(&ops).is_err());
        assert_eq!(
            transactional.len(),
            0,
            "apply_entry is atomic: the first spawn must be rolled back"
        );

        let mut in_place = WorldDoc::new("w");
        assert!(in_place.apply_entry_in_place(&ops).is_err());
        assert_eq!(
            in_place.len(),
            1,
            "apply_entry_in_place is not atomic, and says so: the caller \
             owns the pieces"
        );
    }

    #[test]
    fn in_place_still_binds_the_names_the_entry_introduces() {
        // A behavior referencing a sibling by name must bind against the
        // fold-so-far including this entry — the entry is still the unit of
        // ingestion even when it is not the unit of rollback.
        let mut target = entity(1, "target", None);
        target.transform.position = [1.0, 0.0, 0.0];
        let mut watcher = entity(2, "watcher", None);
        watcher.behaviors = vec![wt::BehaviorDef::LookAt {
            target: EntityRef::Name("target".into()),
        }];

        let ops = [EditOp::spawn(target), EditOp::spawn(watcher)];
        let mut doc = WorldDoc::new("w");
        doc.apply_entry_in_place(&ops).unwrap();

        let bound = doc.get(2).expect("the watcher is there");
        assert!(
            matches!(
                bound.behaviors.first(),
                Some(wt::BehaviorDef::LookAt {
                    target: EntityRef::Id(EntityId(1))
                })
            ),
            "the name should have bound to the id: {:?}",
            bound.behaviors.first()
        );
    }

    #[test]
    fn spawn_refuses_ids_past_the_json_safe_ceiling() {
        let mut doc = WorldDoc::new("w");
        let mut beyond = entity(crate::identity::MAX_ENTITY_ID + 1, "big", None);
        beyond.transform.position = [0.0; 3];
        assert!(matches!(
            doc.apply(&EditOp::spawn(beyond)),
            Err(ApplyError::Invalid(_))
        ));
        // The ceiling itself is fine.
        doc.apply(&spawn(crate::identity::MAX_ENTITY_ID, "cap", None))
            .unwrap();
        assert!(doc.contains(crate::identity::MAX_ENTITY_ID));
    }

    #[test]
    fn names_bind_at_ingestion_not_at_fold_time() {
        // spec/world.md: names resolve against the fold-so-far, at the
        // moment the entry that carries them is ingested. A rename
        // afterwards must not rebind what an earlier entry meant.
        let mut doc = WorldDoc::new("w");
        doc.apply(&spawn(1, "anchor", None)).unwrap();

        let mut watcher = entity(2, "watcher", None);
        watcher.behaviors = vec![crate::behavior::BehaviorDef::LookAt {
            target: EntityRef::name("anchor"),
        }];
        doc.apply_entry(&[EditOp::spawn(watcher)]).unwrap();

        // Now the rename — the behavior already holds the id.
        doc.apply(&EditOp::modify(
            EntityId(1),
            EntityPatch {
                name: Some(EntityName::new("moved")),
                ..Default::default()
            },
        ))
        .unwrap();

        let watcher = doc.get(2).unwrap();
        match &watcher.behaviors[0] {
            crate::behavior::BehaviorDef::LookAt { target } => {
                assert_eq!(*target, EntityRef::id(1));
            }
            other => panic!("unexpected behavior {other:?}"),
        }
        // And the base document resolves too: a manifest's entities bind
        // against the manifest as a whole.
        let mut manifest = wt::WorldManifest::new("m");
        let mut orbiter = entity(4, "orbiter", None);
        orbiter.behaviors = vec![crate::behavior::BehaviorDef::Orbit {
            center: Some(EntityRef::name("hub")),
            center_point: None,
            radius: 2.0,
            speed: 10.0,
            axis: [0.0, 1.0, 0.0],
            phase: 0.0,
            tilt: 0.0,
        }];
        manifest.entities = vec![entity(3, "hub", None), orbiter];
        let base = manifest.as_base().unwrap();
        match &base.get(4).unwrap().behaviors[0] {
            crate::behavior::BehaviorDef::Orbit { center, .. } => {
                assert_eq!(*center, Some(EntityRef::id(3)));
            }
            other => panic!("unexpected behavior {other:?}"),
        }
    }

    #[test]
    fn a_name_nothing_owns_refuses_the_entry() {
        let mut doc = WorldDoc::new("w");
        doc.apply(&spawn(1, "a", None)).unwrap();
        let mut stray = entity(2, "stray", None);
        stray.behaviors = vec![crate::behavior::BehaviorDef::LookAt {
            target: EntityRef::name("ghost"),
        }];
        assert!(matches!(
            doc.apply_entry(&[EditOp::spawn(stray.clone())]),
            Err(ApplyError::MissingName(name)) if name == "ghost"
        ));
        // The entry is atomic: nothing applied, not even the valid parts.
        assert_eq!(doc.len(), 1);
    }

    #[test]
    fn same_entry_references_bind() {
        // An entry is atomic, so a behavior may reference an entity the
        // same entry spawns — order within the entry is not causality.
        let mut doc = WorldDoc::new("w");
        let mut watcher = entity(2, "watcher", None);
        watcher.behaviors = vec![crate::behavior::BehaviorDef::LookAt {
            target: EntityRef::name("anchor"),
        }];
        doc.apply_entry(&[spawn(1, "anchor", None), EditOp::spawn(watcher)])
            .unwrap();
        match &doc.get(2).unwrap().behaviors[0] {
            crate::behavior::BehaviorDef::LookAt { target } => {
                assert_eq!(*target, EntityRef::id(1));
            }
            other => panic!("unexpected behavior {other:?}"),
        }
    }

    #[test]
    fn spawn_then_lookup_by_id_and_name() {
        let mut doc = WorldDoc::new("w");
        doc.apply(&spawn(1, "tower", None)).unwrap();
        assert_eq!(doc.len(), 1);
        assert_eq!(doc.get(1).unwrap().name.as_str(), "tower");
        assert_eq!(doc.get_by_name("tower").unwrap().id, EntityId(1));
        assert_eq!(doc.next_id(), 2);
    }

    #[test]
    fn spawn_rejects_duplicates_and_missing_parents() {
        let mut doc = WorldDoc::new("w");
        doc.apply(&spawn(1, "a", None)).unwrap();
        assert_eq!(
            doc.apply(&spawn(1, "b", None)),
            Err(ApplyError::DuplicateId(1))
        );
        assert_eq!(
            doc.apply(&spawn(2, "a", None)),
            Err(ApplyError::DuplicateName("a".into()))
        );
        assert_eq!(
            doc.apply(&spawn(3, "c", Some(9))),
            Err(ApplyError::MissingParent { id: 3, parent: 9 })
        );
    }

    #[test]
    fn spawn_rejects_non_finite_transforms_and_empty_names() {
        let mut doc = WorldDoc::new("w");
        let mut bad = entity(1, "nan", None);
        bad.transform.position[1] = f32::NAN;
        assert!(matches!(
            doc.apply(&EditOp::spawn(bad)),
            Err(ApplyError::Invalid(_))
        ));
        assert!(matches!(
            doc.apply(&spawn(2, "  ", None)),
            Err(ApplyError::Invalid(_))
        ));
        assert!(doc.is_empty());
    }

    #[test]
    fn delete_takes_the_subtree_and_frees_names() {
        let mut doc = WorldDoc::new("w");
        doc.apply(&spawn(1, "house", None)).unwrap();
        doc.apply(&spawn(2, "roof", Some(1))).unwrap();
        doc.apply(&spawn(3, "chimney", Some(2))).unwrap();
        doc.apply(&spawn(4, "tree", None)).unwrap();
        doc.apply(&EditOp::delete(EntityId(1))).unwrap();
        assert_eq!(doc.len(), 1);
        assert!(doc.get_by_name("roof").is_none());
        doc.apply(&spawn(5, "roof", None)).unwrap();
        assert_eq!(
            doc.apply(&EditOp::delete(EntityId(1))),
            Err(ApplyError::MissingEntity(1))
        );
    }

    #[test]
    fn modify_renames_and_reparents_with_checks() {
        let mut doc = WorldDoc::new("w");
        doc.apply(&spawn(1, "a", None)).unwrap();
        doc.apply(&spawn(2, "b", Some(1))).unwrap();
        doc.apply(&spawn(3, "c", None)).unwrap();

        let rename = EditOp::modify(
            EntityId(3),
            EntityPatch {
                name: Some(EntityName::new("c2")),
                ..Default::default()
            },
        );
        doc.apply(&rename).unwrap();
        assert!(doc.get_by_name("c").is_none());
        assert_eq!(doc.get_by_name("c2").unwrap().id, EntityId(3));

        let taken = EditOp::modify(
            EntityId(3),
            EntityPatch {
                name: Some(EntityName::new("a")),
                ..Default::default()
            },
        );
        assert_eq!(
            doc.apply(&taken),
            Err(ApplyError::DuplicateName("a".into()))
        );

        // a under b, while b is a's child: a cycle.
        let cycle = EditOp::modify(
            EntityId(1),
            EntityPatch {
                parent: Some(Some(EntityId(2))),
                ..Default::default()
            },
        );
        assert_eq!(doc.apply(&cycle), Err(ApplyError::ParentCycle(1)));
        let own = EditOp::modify(
            EntityId(3),
            EntityPatch {
                parent: Some(Some(EntityId(3))),
                ..Default::default()
            },
        );
        assert_eq!(doc.apply(&own), Err(ApplyError::ParentCycle(3)));

        let move_under = EditOp::modify(
            EntityId(3),
            EntityPatch {
                parent: Some(Some(EntityId(2))),
                ..Default::default()
            },
        );
        doc.apply(&move_under).unwrap();
        assert_eq!(doc.depth(3), 2);
    }

    #[test]
    fn batch_is_all_or_nothing() {
        let mut doc = WorldDoc::new("w");
        doc.apply(&spawn(1, "a", None)).unwrap();
        let batch = EditOp::Batch {
            ops: vec![spawn(2, "b", None), spawn(3, "a", None)],
        };
        assert_eq!(
            doc.apply(&batch),
            Err(ApplyError::DuplicateName("a".into()))
        );
        assert_eq!(doc.len(), 1);
        assert!(doc.get(2).is_none());

        let ok = EditOp::Batch {
            ops: vec![spawn(2, "b", None), EditOp::delete(EntityId(1))],
        };
        doc.apply(&ok).unwrap();
        assert_eq!(doc.len(), 1);
        assert!(doc.get(2).is_some());
    }

    #[test]
    fn audio_emitter_ops_target_named_entities() {
        let mut doc = WorldDoc::new("w");
        doc.apply(&spawn(1, "campfire", None)).unwrap();
        let audio = wt::AudioDef {
            kind: wt::AudioKind::Sfx,
            source: wt::AudioSource::Fire {
                intensity: 0.5,
                crackle: 0.5,
            },
            volume: 0.6,
            radius: Some(8.0),
            rolloff: wt::Rolloff::default(),
        };
        doc.apply(&EditOp::SpawnAudioEmitter {
            name: "campfire".into(),
            audio: audio.clone(),
        })
        .unwrap();
        assert!(doc.get(1).unwrap().audio.is_some());
        doc.apply(&EditOp::RemoveAudioEmitter {
            name: "campfire".into(),
            audio,
        })
        .unwrap();
        assert!(doc.get(1).unwrap().audio.is_none());
    }

    #[test]
    fn manifest_round_trip_orders_parents_first() {
        let mut doc = WorldDoc::new("village");
        doc.apply(&spawn(5, "house", None)).unwrap();
        doc.apply(&spawn(2, "door", Some(5))).unwrap();
        doc.apply(&spawn(9, "well", None)).unwrap();
        let manifest = doc.to_manifest();
        let names: Vec<&str> = manifest.entities.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["house", "door", "well"]);
        assert_eq!(manifest.next_entity_id, 10);
        assert_eq!(manifest.meta.name, "village");

        let back = WorldDoc::from_manifest(&manifest).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(back.get(2).unwrap().parent, Some(EntityId(5)));
    }

    #[test]
    fn from_manifest_turns_dangling_parents_into_roots() {
        let mut manifest = wt::WorldManifest::new("w");
        manifest.entities = vec![entity(1, "orphan", Some(42))];
        let doc = WorldDoc::from_manifest(&manifest).unwrap();
        assert_eq!(doc.get(1).unwrap().parent, None);
    }
}
