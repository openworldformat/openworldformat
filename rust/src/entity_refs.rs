//! The one list of entity-reference fields (spec/world.md "Identity",
//! schema/entity-refs.json) — the crate's embedded copy. The canonical
//! list is generated from world.schema.json's `x-entity-ref` markers by
//! schema/generate-entity-refs.mjs, but a published crate can't read
//! the repo's schema/ at runtime, so the passes read this table
//! instead, and tests/entity_refs.rs fails when it drifts from the
//! canonical file.
//!
//! [`EntityRefKind::Bindable`] — a name is accepted at intake and MUST
//! bind to an id at ingestion; [`EntityRefKind::Id`] — numeric only.
//! The scope is the object the path walks from: an entity (or its
//! patch, the same fields), the manifest's avatar, or one creation in
//! `creations[]`. `"*"` walks every array element.
//!
//! Two walks read the table: [`walk_ref_path`] over the raw op JSON
//! (the authoring pass binds names before the typed read) and
//! [`RefWalk`] over the typed model (name binding after apply, merge
//! rewriting) — the same list either way, so the three passes can
//! never drift apart over hand-written field mentions.

use std::collections::BTreeMap;
use std::convert::Infallible;

use serde_json::Value;

use crate::avatar::AvatarDef;
use crate::behavior::BehaviorDef;
use crate::creation::CreationDef;
use crate::entity::{EntityPatch, WorldEntity};
use crate::identity::{EntityId, EntityRef};

/// The object a marked path walks from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityRefScope {
    /// An entity (or its patch — the same fields).
    Entity,
    /// The manifest's avatar.
    Avatar,
    /// One creation in `creations[]`.
    Creation,
}

impl EntityRefScope {
    /// The scope's name in the canonical list.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Entity => "entity",
            Self::Avatar => "avatar",
            Self::Creation => "creation",
        }
    }
}

/// What a marked field accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityRefKind {
    /// A name is accepted at intake; it MUST bind to an id at ingestion.
    Bindable,
    /// Numeric only.
    Id,
}

impl EntityRefKind {
    /// The kind's name in the canonical list.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bindable => "bindable",
            Self::Id => "id",
        }
    }
}

/// One marked entity-reference field: a scope, the path that walks it
/// (`"*"` walks every array element), and what it accepts.
pub struct EntityRefField {
    pub scope: EntityRefScope,
    pub path: &'static [&'static str],
    pub kind: EntityRefKind,
}

/// The one list of entity-reference fields, in the canonical file's
/// order — the embedded copy of schema/entity-refs.json's `refs`.
pub const ENTITY_REFS: &[EntityRefField] = &[
    EntityRefField {
        scope: EntityRefScope::Entity,
        path: &["behaviors", "*", "LookAt", "target"],
        kind: EntityRefKind::Bindable,
    },
    EntityRefField {
        scope: EntityRefScope::Entity,
        path: &["behaviors", "*", "Orbit", "center"],
        kind: EntityRefKind::Bindable,
    },
    EntityRefField {
        scope: EntityRefScope::Entity,
        path: &["parent"],
        kind: EntityRefKind::Bindable,
    },
    EntityRefField {
        scope: EntityRefScope::Avatar,
        path: &["model_entity"],
        kind: EntityRefKind::Bindable,
    },
    EntityRefField {
        scope: EntityRefScope::Creation,
        path: &["entities", "*"],
        kind: EntityRefKind::Id,
    },
];

/// The marked refs of one scope.
pub fn refs_of(scope: EntityRefScope) -> impl Iterator<Item = &'static EntityRefField> {
    ENTITY_REFS.iter().filter(move |field| field.scope == scope)
}

/// The marked refs of one scope and kind.
pub fn refs_of_kind(
    scope: EntityRefScope,
    kind: EntityRefKind,
) -> impl Iterator<Item = &'static EntityRefField> {
    refs_of(scope).filter(move |field| field.kind == kind)
}

/// Walk a marked path over raw JSON, calling `f` on each leaf value in
/// place; `"*"` walks every array element. Missing keys walk to
/// nothing. This is the JS reference's `walkRefPath`, on the raw op
/// JSON the authoring pass binds before the typed read.
pub(crate) fn walk_ref_path<E>(
    value: &mut Value,
    path: &[&str],
    f: &mut impl FnMut(&mut Value) -> Result<(), E>,
) -> Result<(), E> {
    let Some((key, rest)) = path.split_first() else {
        return Ok(());
    };
    if *key == "*" {
        if let Value::Array(elements) = value {
            for element in elements {
                if rest.is_empty() {
                    f(element)?;
                } else {
                    walk_ref_path(element, rest, f)?;
                }
            }
        }
        return Ok(());
    }
    let Some(next) = value.get_mut(*key) else {
        return Ok(());
    };
    if rest.is_empty() {
        f(next)
    } else {
        walk_ref_path(next, rest, f)
    }
}

/// A marked leaf in the typed model: a bare id slot, or a classed
/// (name-or-id) reference. Name binding touches only the classed kind;
/// the merge remaps the ids of both.
pub(crate) enum RefSlot<'a> {
    /// A numeric-only slot (an entity's `parent`, a creation's member).
    Id(&'a mut EntityId),
    /// A name-or-id reference (a behavior's target, the avatar's model).
    Ref(&'a mut EntityRef),
}

impl RefSlot<'_> {
    /// Move the id this slot holds through the merge's remap, when it
    /// holds one: a `Ref` still holding a name is left alone — names
    /// bind at ingestion, a merge only moves ids.
    pub(crate) fn remap(self, remapped: &BTreeMap<u64, u64>) {
        let held = match self {
            Self::Id(id) => Some(id),
            Self::Ref(EntityRef::Id(id)) => Some(id),
            Self::Ref(EntityRef::Name(_)) => None,
        };
        if let Some(id) = held
            && let Some(&to) = remapped.get(&id.0)
        {
            *id = EntityId(to);
        }
    }
}

/// A typed root a marked scope's paths walk. An entity and its patch
/// share the `entity` scope (a patch sets the entity's fields, so the
/// same paths read both); the avatar and one creation walk theirs.
pub(crate) trait RefWalk {
    /// The scope whose paths this root walks.
    fn scope() -> EntityRefScope;
    /// Walk one marked path, `f` on each leaf it lands on; pieces the
    /// root doesn't have walk to nothing.
    fn walk<E>(
        &mut self,
        path: &[&str],
        f: &mut dyn FnMut(RefSlot<'_>) -> Result<(), E>,
    ) -> Result<(), E>;
}

/// Every marked entity-reference field of a typed root, `f` on each
/// leaf in place — the typed `eachRef`, the merge's one walk.
pub(crate) fn each_ref<R: RefWalk>(root: &mut R, mut f: impl FnMut(RefSlot<'_>)) {
    for field in refs_of(R::scope()) {
        root.walk(field.path, &mut |slot| {
            f(slot);
            Ok::<(), Infallible>(())
        })
        .unwrap_or_else(|never| match never {});
    }
}

impl RefWalk for WorldEntity {
    fn scope() -> EntityRefScope {
        EntityRefScope::Entity
    }

    fn walk<E>(
        &mut self,
        path: &[&str],
        f: &mut dyn FnMut(RefSlot<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        match path {
            ["parent"] => {
                if let Some(id) = &mut self.parent {
                    f(RefSlot::Id(id))?;
                }
            }
            ["behaviors", "*", variant, leaf] => {
                for behavior in &mut self.behaviors {
                    if let Some(slot) = behavior_slot(behavior, variant, leaf) {
                        f(slot)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl RefWalk for EntityPatch {
    fn scope() -> EntityRefScope {
        EntityRefScope::Entity
    }

    fn walk<E>(
        &mut self,
        path: &[&str],
        f: &mut dyn FnMut(RefSlot<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        match path {
            ["parent"] => {
                if let Some(Some(id)) = &mut self.parent {
                    f(RefSlot::Id(id))?;
                }
            }
            ["behaviors", "*", variant, leaf] => {
                for behavior in self.behaviors.iter_mut().flatten() {
                    if let Some(slot) = behavior_slot(behavior, variant, leaf) {
                        f(slot)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl RefWalk for AvatarDef {
    fn scope() -> EntityRefScope {
        EntityRefScope::Avatar
    }

    fn walk<E>(
        &mut self,
        path: &[&str],
        f: &mut dyn FnMut(RefSlot<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        if let ["model_entity"] = path
            && let Some(model) = &mut self.model_entity
        {
            f(RefSlot::Ref(model))?;
        }
        Ok(())
    }
}

impl RefWalk for CreationDef {
    fn scope() -> EntityRefScope {
        EntityRefScope::Creation
    }

    fn walk<E>(
        &mut self,
        path: &[&str],
        f: &mut dyn FnMut(RefSlot<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        if let ["entities", "*"] = path {
            for id in &mut self.entities {
                f(RefSlot::Id(id))?;
            }
        }
        Ok(())
    }
}

/// The leaf one behavior's marked field holds, by variant and field
/// name — the last two keys of the entity scope's `behaviors/*/…`
/// paths.
fn behavior_slot<'a>(
    behavior: &'a mut BehaviorDef,
    variant: &str,
    leaf: &str,
) -> Option<RefSlot<'a>> {
    match (variant, leaf, behavior) {
        ("Orbit", "center", BehaviorDef::Orbit { center: Some(center), .. }) => {
            Some(RefSlot::Ref(center))
        }
        ("LookAt", "target", BehaviorDef::LookAt { target }) => Some(RefSlot::Ref(target)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every path the table names must land on a slot in a
    /// fully-populated root: the guard against a walk silently
    /// ignoring a listed field.
    #[test]
    fn every_listed_path_visits_a_slot_in_a_full_root() {
        fn visits<R: RefWalk>(root: &mut R) -> Vec<(String, usize)> {
            refs_of(R::scope())
                .map(|field| {
                    let mut visited = 0;
                    root.walk(field.path, &mut |_| {
                        visited += 1;
                        Ok::<(), Infallible>(())
                    })
                    .unwrap_or_else(|never| match never {});
                    (field.path.join("/"), visited)
                })
                .collect()
        }

        let mut entity = WorldEntity::new(1, "e");
        entity.parent = Some(EntityId(2));
        entity.behaviors = vec![
            BehaviorDef::LookAt {
                target: EntityRef::id(2),
            },
            BehaviorDef::Orbit {
                center: Some(EntityRef::id(2)),
                center_point: None,
                radius: 2.0,
                speed: 10.0,
                axis: [0.0, 1.0, 0.0],
                phase: 0.0,
                tilt: 0.0,
            },
        ];
        for (path, visited) in visits(&mut entity) {
            assert!(visited > 0, "entity path {path} visited nothing");
        }

        let mut patch = EntityPatch {
            parent: Some(Some(EntityId(2))),
            behaviors: Some(vec![
                BehaviorDef::LookAt {
                    target: EntityRef::id(2),
                },
                BehaviorDef::Orbit {
                    center: Some(EntityRef::id(2)),
                    center_point: None,
                    radius: 2.0,
                    speed: 10.0,
                    axis: [0.0, 1.0, 0.0],
                    phase: 0.0,
                    tilt: 0.0,
                },
            ]),
            ..Default::default()
        };
        for (path, visited) in visits(&mut patch) {
            assert!(visited > 0, "patch path {path} visited nothing");
        }

        let mut avatar = AvatarDef {
            model_entity: Some(EntityRef::id(2)),
            ..Default::default()
        };
        for (path, visited) in visits(&mut avatar) {
            assert!(visited > 0, "avatar path {path} visited nothing");
        }

        let mut creation = CreationDef {
            id: crate::identity::CreationId(1),
            name: "c".into(),
            semantic_category: None,
            bbox_half: [0.0; 3],
            entities: vec![EntityId(2)],
            parts: Vec::new(),
        };
        for (path, visited) in visits(&mut creation) {
            assert!(visited > 0, "creation path {path} visited nothing");
        }
    }

    #[test]
    fn the_json_walk_matches_walk_ref_path_semantics() {
        // "*" walks every array element, missing keys walk to nothing,
        // the leaf visit sets in place.
        let mut value = serde_json::json!({
            "behaviors": [{"Orbit": {"center": "sun"}}, {"Spin": {}}],
            "parent": "sun",
        });
        walk_ref_path(&mut value, &["behaviors", "*", "Orbit", "center"], &mut |leaf| {
            *leaf = serde_json::json!(1);
            Ok::<(), Infallible>(())
        })
        .unwrap_or_else(|never| match never {});
        walk_ref_path(&mut value, &["missing", "key"], &mut |_| -> Result<(), Infallible> {
            panic!("a missing key walks to nothing")
        })
        .unwrap_or_else(|never| match never {});
        assert_eq!(value["behaviors"][0]["Orbit"]["center"], 1);
        assert_eq!(value["parent"], "sun", "a path not walked is untouched");
    }
}
