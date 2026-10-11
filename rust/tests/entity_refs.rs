//! The one list of entity-reference fields (spec/world.md "Identity",
//! schema/entity-refs.json): this crate embeds a copy
//! (src/entity_refs.rs) and this test fails when the copy drifts from
//! the canonical file — and pins that the passes actually walk it.

use std::path::PathBuf;

use openworldformat::session::SessionOp;
use openworldformat::{
    AvatarDef, BehaviorDef, CreationDef, CreationId, EditOp, EntityId, EntityRef, ENTITY_REFS,
    OpLogEntry, WorldDoc, WorldEntity, WorldManifest, WorldPatch, ingest, merge_branch,
};
use serde_json::{Value, json};

fn repo(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../")
        .join(relative)
}

#[test]
fn the_embedded_list_is_the_canonical_list() {
    let canonical: Value = serde_json::from_str(
        &std::fs::read_to_string(repo("schema/entity-refs.json")).expect("the canonical file"),
    )
    .unwrap();
    let embedded: Vec<Value> = ENTITY_REFS
        .iter()
        .map(|field| {
            json!({
                "scope": field.scope.as_str(),
                "path": field.path,
                "kind": field.kind.as_str(),
            })
        })
        .collect();
    assert_eq!(json!(embedded), canonical["refs"]);
}

#[test]
fn the_list_holds_every_field_the_merge_table_rewrites() {
    let paths: Vec<String> = ENTITY_REFS
        .iter()
        .map(|field| format!("{}:{}", field.scope.as_str(), field.path.join("/")))
        .collect();
    for expected in [
        "entity:parent",
        "entity:behaviors/*/Orbit/center",
        "entity:behaviors/*/LookAt/target",
        "entity:ext-cinematography/camera/aim_entity",
        "entity:ext-cinematography/camera/focus_entity",
        "avatar:model_entity",
        "creation:entities/*",
    ] {
        assert!(paths.iter().any(|p| p == expected), "missing {expected}");
    }
}

#[test]
fn an_extensions_marked_refs_bind_at_ingestion_by_name() {
    let done = ingest(
        &base(json!([{"id": 1, "name": "maya"}])),
        &json!([
            {"SpawnEntity": {"entity": {"id": 2, "name": "4A",
                "ext-cinematography": {"camera": {"aim_entity": "maya"}}}}},
            // An `ext-*` patch replaces the block, so the patch carries
            // the whole camera.
            {"ModifyEntity": {"id": "4A", "patch":
                {"ext-cinematography": {"camera": {"aim_entity": "maya", "focus_entity": "maya"}}}}},
        ]),
    )
    .unwrap();
    let camera = &done.doc.get_by_name("4A").unwrap().extra["ext-cinematography"]["camera"];
    assert_eq!(camera["aim_entity"], 1, "the document holds the id");
    assert_eq!(camera["focus_entity"], 1, "a patch's references bind too");
    // Committed ops always carry ids (spec/session.md): the spawn's and
    // the patch's own fields are bound, not only the document.
    let EditOp::SpawnEntity { entity } = &done.ops[0] else {
        panic!("a spawn")
    };
    assert_eq!(entity.extra["ext-cinematography"]["camera"]["aim_entity"], 1);
    let EditOp::ModifyEntity { patch, .. } = &done.ops[1] else {
        panic!("a patch")
    };
    let block = patch.extra["ext-cinematography"].as_ref().unwrap();
    assert_eq!(block["camera"]["focus_entity"], 1);
    // A name nothing answers to is refused, like any marked ref.
    assert!(
        ingest(
            &base(json!([{"id": 1, "name": "maya"}])),
            &json!([{"SpawnEntity": {"entity": {"id": 3, "name": "4B",
                "ext-cinematography": {"camera": {"aim_entity": "nobody"}}}}}]),
        )
        .is_err()
    );
}

#[test]
fn merge_rewriting_walks_the_extensions_marked_refs() {
    let mut main = WorldDoc::new("t");
    main.apply(&EditOp::spawn(WorldEntity::new(1, "maya")))
        .unwrap();
    let mut tracked = WorldEntity::new(2, "4A");
    tracked.extra.insert(
        "ext-cinematography".into(),
        json!({"camera": {"aim_entity": 1, "focus_entity": 1}}),
    );
    let patch: openworldformat::EntityPatch = serde_json::from_value(
        json!({"ext-cinematography": {"camera": {"aim_entity": 1}}}),
    )
    .unwrap();
    let branch = vec![entry(
        2,
        vec![
            SessionOp::Edit(Box::new(EditOp::spawn(WorldEntity::new(1, "kai")))),
            SessionOp::Edit(Box::new(EditOp::spawn(tracked))),
            SessionOp::Edit(Box::new(EditOp::modify(EntityId(2), patch))),
        ],
    )];
    let merged = merge_branch(&main, &branch).unwrap();
    assert_eq!(
        merged.remapped.iter().collect::<Vec<_>>(),
        vec![(&1, &3)]
    );
    let SessionOp::Edit(spawn) = &merged.entries[0].ops[1] else {
        panic!("an edit")
    };
    let EditOp::SpawnEntity { entity } = &**spawn else {
        panic!("a spawn")
    };
    assert_eq!(entity.extra["ext-cinematography"]["camera"]["aim_entity"], 3);
    assert_eq!(entity.extra["ext-cinematography"]["camera"]["focus_entity"], 3);
    let SessionOp::Edit(modify) = &merged.entries[0].ops[2] else {
        panic!("an edit")
    };
    let EditOp::ModifyEntity { patch, .. } = &**modify else {
        panic!("a patch")
    };
    let block = patch.extra["ext-cinematography"].as_ref().unwrap();
    assert_eq!(block["camera"]["aim_entity"], 3);
}

fn base(entities: Value) -> WorldDoc {
    let manifest: WorldManifest = serde_json::from_value(json!({
        "version": 3,
        "meta": {"name": "t"},
        "entities": entities,
    }))
    .unwrap();
    manifest.as_base().unwrap()
}

#[test]
fn name_binding_walks_the_list_a_string_parent_binds_at_intake() {
    let done = ingest(
        &base(json!([{"id": 1, "name": "sun"}])),
        &json!([
            {"SpawnEntity": {"entity": {"id": 2, "name": "planet", "parent": "sun"}}},
            {"SpawnEntity": {"entity": {"name": "moon", "parent": "planet"}}},
        ]),
    )
    .unwrap();
    let EditOp::SpawnEntity { entity } = &done.ops[0] else {
        panic!("a spawn")
    };
    assert_eq!(entity.parent, Some(EntityId(1)));
    let EditOp::SpawnEntity { entity } = &done.ops[1] else {
        panic!("a spawn")
    };
    assert_eq!(entity.parent, Some(EntityId(2)));
    assert_eq!(done.spawned["moon"], 3);
    // A raw log's string parent is not a committed form: the typed read
    // refuses it (committed ops carry ids).
    assert!(
        serde_json::from_value::<OpLogEntry>(json!({
            "revision": 1,
            "ops": [{"SpawnEntity": {"entity": {"id": 4, "name": "x", "parent": "sun"}}}],
        }))
        .is_err()
    );
}

#[test]
fn ingest_binds_the_avatars_marked_refs_model_entity_by_name() {
    let done = ingest(
        &base(json!([{"id": 1, "name": "hero"}])),
        &json!([{"ModifyWorld": {"patch": {"avatar": {"model_entity": "hero"}}}}]),
    )
    .unwrap();
    let EditOp::ModifyWorld { patch } = &done.ops[0] else {
        panic!("a world edit")
    };
    let avatar = patch
        .avatar
        .as_ref()
        .and_then(|avatar| avatar.as_ref())
        .expect("the patch sets an avatar");
    assert_eq!(
        avatar.model_entity,
        Some(EntityRef::id(1)),
        "the committed op carries the id"
    );
    assert_eq!(
        done.doc
            .avatar
            .as_ref()
            .and_then(|avatar| avatar.model_entity.clone()),
        Some(EntityRef::id(1)),
        "and the world does too"
    );
}

#[test]
fn behavior_refs_bind_against_the_fold_so_far_at_ingestion() {
    let done = ingest(
        &base(json!([{"id": 1, "name": "sun"}])),
        &json!([{"SpawnEntity": {"entity": {"name": "planet", "behaviors": [
            {"Orbit": {"center": "sun", "radius": 2.0, "speed": 10.0}},
            {"LookAt": {"target": "sun"}}
        ]}}}]),
    )
    .unwrap();
    let planet = done.doc.get_by_name("planet").unwrap();
    match &planet.behaviors[0] {
        BehaviorDef::Orbit { center, .. } => assert_eq!(*center, Some(EntityRef::id(1))),
        other => panic!("unexpected behavior {other:?}"),
    }
    match &planet.behaviors[1] {
        BehaviorDef::LookAt { target } => assert_eq!(*target, EntityRef::id(1)),
        other => panic!("unexpected behavior {other:?}"),
    }
    // And the committed op carries the ids, not the names it was sent
    // with: a log never holds a name (spec/session.md).
    let EditOp::SpawnEntity { entity } = &done.ops[0] else {
        panic!("a spawn")
    };
    match &entity.behaviors[1] {
        BehaviorDef::LookAt { target } => assert_eq!(*target, EntityRef::id(1)),
        other => panic!("unexpected behavior {other:?}"),
    }
}

fn entry(revision: u64, ops: Vec<SessionOp>) -> OpLogEntry {
    OpLogEntry {
        revision,
        author: Default::default(),
        ops,
        timestamp_ms: revision,
        id: None,
        parent: None,
        message: None,
    }
}

#[test]
fn merge_rewriting_walks_the_list_for_every_scope() {
    let mut main = WorldDoc::new("t");
    main.apply(&EditOp::spawn(WorldEntity::new(1, "main-one")))
        .unwrap();

    let mut branch_spawn = WorldEntity::new(1, "branch-one");
    branch_spawn.parent = Some(EntityId(1));
    branch_spawn.behaviors = vec![
        BehaviorDef::Orbit {
            center: Some(EntityRef::id(1)),
            center_point: None,
            radius: 2.0,
            speed: 10.0,
            axis: [0.0, 1.0, 0.0],
            phase: 0.0,
            tilt: 0.0,
        },
        BehaviorDef::LookAt {
            target: EntityRef::id(1),
        },
    ];
    let branch = vec![entry(
        2,
        vec![
            SessionOp::Edit(Box::new(EditOp::spawn(branch_spawn))),
            SessionOp::Edit(Box::new(EditOp::ModifyWorld {
                patch: Box::new(WorldPatch {
                    avatar: Some(Some(AvatarDef {
                        model_entity: Some(EntityRef::id(1)),
                        ..Default::default()
                    })),
                    creations: Some(vec![CreationDef {
                        id: CreationId(1),
                        name: "c".into(),
                        semantic_category: None,
                        bbox_half: [0.0; 3],
                        entities: vec![EntityId(1)],
                        parts: Vec::new(),
                    }]),
                    ..Default::default()
                }),
            })),
        ],
    )];

    let merged = merge_branch(&main, &branch).unwrap();
    assert_eq!(
        merged.remapped.iter().collect::<Vec<_>>(),
        vec![(&1, &2)]
    );

    let SessionOp::Edit(spawn) = &merged.entries[0].ops[0] else {
        panic!("an edit")
    };
    let EditOp::SpawnEntity { entity } = &**spawn else {
        panic!("a spawn")
    };
    assert_eq!(entity.id, EntityId(2));
    assert_eq!(entity.parent, Some(EntityId(2)));
    match &entity.behaviors[0] {
        BehaviorDef::Orbit { center, .. } => assert_eq!(*center, Some(EntityRef::id(2))),
        other => panic!("unexpected behavior {other:?}"),
    }
    match &entity.behaviors[1] {
        BehaviorDef::LookAt { target } => assert_eq!(*target, EntityRef::id(2)),
        other => panic!("unexpected behavior {other:?}"),
    }

    let SessionOp::Edit(world) = &merged.entries[0].ops[1] else {
        panic!("an edit")
    };
    let EditOp::ModifyWorld { patch } = &**world else {
        panic!("a world edit")
    };
    let avatar = patch
        .avatar
        .as_ref()
        .and_then(|avatar| avatar.as_ref())
        .unwrap();
    assert_eq!(avatar.model_entity, Some(EntityRef::id(2)));
    assert_eq!(
        patch.creations.as_ref().unwrap()[0].entities,
        vec![EntityId(2)]
    );
}
