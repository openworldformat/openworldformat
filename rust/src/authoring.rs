//! Authoring: what an authority does with a batch of ops an author sends
//! (spec/session.md, "Authoring"). The fold reads committed ops; this is
//! the step before — turning what an agent, a script or a person wrote
//! into ops worth committing, or refusing them with reasons.
//!
//! Each op, in order, against a trial document that already holds the
//! batch's earlier ops:
//!
//! 1. **Bind** — a string where an entity id goes is a name, resolved to
//!    the id it names now; a spawn without an `id` gets the next one.
//! 2. **Merge** — the object-valued struct fields of a patch
//!    (`ModifyEntity`'s `transform`, `material`, `light`; `SetEnvironment`'s
//!    `env`; `ModifyWorld`'s `meta`, `environment`, `camera`, `avatar`,
//!    `soundtrack`) merge into the current value as a JSON merge patch
//!    (RFC 7396). The committed op carries the merged whole.
//! 3. **Read strictly** — no key the format would drop.
//! 4. **Apply** — to the trial.
//!
//! A structural failure at any step refuses the whole batch. The world
//! the batch makes is then validated: budget limits (an entity's extent,
//! a chunk's entity or triangle count, an entity's behavior or
//! modulation count) are this authority's policy, not the format's, so
//! they come back on the result as warnings and never refuse a batch.
//! Asset files are the host's: the crate does no I/O.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::doc::WorldDoc;
use crate::entity_refs::{EntityRefKind, EntityRefScope, refs_of, refs_of_kind, walk_ref_path};
use crate::history::EditOp;
use crate::validation::{WorldLimits, validate_manifest};

/// A batch the world can take.
#[derive(Debug, Clone)]
pub struct Ingested {
    /// The ops to commit: names bound to ids, struct patches merged.
    pub ops: Vec<EditOp>,
    /// Entities the batch spawns, name → id.
    pub spawned: BTreeMap<String, u64>,
    /// The document after the batch.
    pub doc: WorldDoc,
    /// Validation warnings that didn't stop the batch.
    pub warnings: Vec<String>,
}

/// A batch refused whole; one reason per failing op (`op 2: …`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub errors: Vec<String>,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.errors.join("; "))
    }
}

impl std::error::Error for Refused {}

const OP_KINDS: &[&str] = crate::session::EDIT_KEYS;

/// Ingest a batch — `[op, …]` or `{"ops": [op, …], …}` — against `doc`.
pub fn ingest(doc: &WorldDoc, batch: &Value) -> Result<Ingested, Refused> {
    let ops = match batch {
        Value::Array(ops) => ops,
        Value::Object(body) => match body.get("ops") {
            Some(Value::Array(ops)) => ops,
            _ => return Err(refused("a batch is [op, …] or {\"ops\": [op, …]}")),
        },
        _ => return Err(refused("a batch is [op, …] or {\"ops\": [op, …]}")),
    };
    if ops.is_empty() {
        return Err(refused("the batch holds no ops"));
    }
    let mut state = State {
        trial: doc.clone(),
        next_id: doc.next_id(),
        spawned: BTreeMap::new(),
    };
    let mut committed = Vec::with_capacity(ops.len());
    let mut errors = Vec::new();
    for (i, raw) in ops.iter().enumerate() {
        match state.op(raw.clone()) {
            Ok(op) => committed.push(op),
            Err(e) => errors.push(format!("op {i}: {e}")),
        }
    }
    if !errors.is_empty() {
        return Err(Refused { errors });
    }
    let mut warnings = Vec::new();
    for issue in validate_manifest(&state.trial.to_manifest(), &WorldLimits::default()) {
        // Budget limits are this authority's policy, not the format's:
        // they are reported to the author, and never refuse a batch
        // (spec/session.md, "Authoring"). Structural failures already
        // refused above — at bind, at the strict read, or at apply.
        warnings.push(issue.message);
    }
    Ok(Ingested {
        ops: committed,
        spawned: state.spawned,
        doc: state.trial,
        warnings,
    })
}

fn refused(message: &str) -> Refused {
    Refused {
        errors: vec![message.to_string()],
    }
}

struct State {
    trial: WorldDoc,
    next_id: u64,
    spawned: BTreeMap<String, u64>,
}

impl State {
    fn op(&mut self, mut raw: Value) -> Result<EditOp, String> {
        self.bind(&mut raw)?;
        let op: EditOp = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
        let mut unknown = Vec::new();
        dropped_keys(
            &raw,
            &serde_json::to_value(&op).unwrap_or(Value::Null),
            String::new(),
            &mut unknown,
        );
        if !unknown.is_empty() {
            return Err(unknown
                .iter()
                .map(|p| format!("{p} is not a field of the format"))
                .collect::<Vec<_>>()
                .join("; "));
        }
        self.trial
            .apply_entry(std::slice::from_ref(&op))
            .map_err(|e| format!("doesn't apply: {e}"))?;
        Ok(op)
    }

    /// Names to ids, ids for spawns that left theirs out, merged struct
    /// patches — on the raw JSON, before it is read.
    fn bind(&mut self, raw: &mut Value) -> Result<(), String> {
        let Some(object) = raw.as_object_mut() else {
            return Err("an op is an object like {\"SpawnEntity\": {…}}".into());
        };
        if object.len() != 1 {
            return Err(format!(
                "an op holds exactly one kind, one of: {}",
                OP_KINDS.join(", ")
            ));
        }
        let (kind, body) = object.iter_mut().next().expect("one key");
        match kind.as_str() {
            "SpawnEntity" => {
                let entity = body
                    .get_mut("entity")
                    .filter(|entity| entity.is_object())
                    .ok_or("SpawnEntity needs an \"entity\" object")?;
                match entity.get("id") {
                    None | Some(Value::Null) => {
                        entity["id"] = json!(self.next_id);
                    }
                    Some(Value::Number(_)) => {}
                    Some(_) => {
                        return Err("a new entity's id is a number, or left out to get one".into());
                    }
                }
                let id = entity
                    .get("id")
                    .and_then(Value::as_u64)
                    .unwrap_or(self.next_id);
                self.next_id = self.next_id.max(id.saturating_add(1));
                self.bind_top_level_refs(entity)?;
                if let Some(name) = entity.get("name").and_then(Value::as_str) {
                    self.spawned.insert(name.to_string(), id);
                }
            }
            "ModifyEntity" => {
                let id = body
                    .get_mut("id")
                    .ok_or("ModifyEntity needs an \"id\" (or a name)")?;
                self.resolve(id)?;
                let id = id.as_u64().unwrap_or_default();
                let current = self
                    .trial
                    .get(id)
                    .and_then(|e| serde_json::to_value(e).ok());
                if let Some(patch) = body.get_mut("patch").filter(|patch| patch.is_object()) {
                    self.bind_top_level_refs(patch)?;
                    if let (Some(current), Some(slots)) = (current, patch.as_object_mut()) {
                        merge_fields(slots, &current, &["transform", "material", "light"]);
                    }
                }
            }
            "DeleteEntity" => {
                let id = body
                    .get_mut("id")
                    .ok_or("DeleteEntity needs an \"id\" (or a name)")?;
                self.resolve(id)?;
            }
            "SetEnvironment" => {
                if let (Some(change @ Value::Object(_)), Some(now)) =
                    (body.get_mut("env"), self.trial.environment.as_ref())
                {
                    *change =
                        merge_patch(&serde_json::to_value(now).unwrap_or(Value::Null), change);
                }
            }
            "ModifyWorld" => {
                if let Some(patch) = body.get_mut("patch").filter(|patch| patch.is_object()) {
                    // The avatar's marked refs (today: `model_entity`)
                    // bind like any other — a name where an entity id
                    // goes resolves at intake (spec/world.md,
                    // "Identity": refs MUST resolve at ingestion).
                    if let Some(avatar) = patch
                        .get_mut("avatar")
                        .filter(|avatar| avatar.is_object())
                    {
                        for field in refs_of(EntityRefScope::Avatar) {
                            walk_ref_path(avatar, field.path, &mut |reference| {
                                self.resolve(reference)
                            })?;
                        }
                    }
                    let current =
                        serde_json::to_value(self.trial.to_manifest()).unwrap_or(Value::Null);
                    if let Some(slots) = patch.as_object_mut() {
                        merge_fields(
                            slots,
                            &current,
                            &["meta", "environment", "camera", "avatar", "soundtrack"],
                        );
                    }
                }
            }
            "Batch" => {
                let ops = body
                    .get_mut("ops")
                    .and_then(Value::as_array_mut)
                    .ok_or("Batch needs an \"ops\" array")?;
                for op in ops {
                    self.bind(op)?;
                }
            }
            known if OP_KINDS.contains(&known) => {}
            other => {
                return Err(format!(
                    "\"{other}\" isn't an op kind; the format has: {}",
                    OP_KINDS.join(", ")
                ));
            }
        }
        Ok(())
    }

    /// The entity scope's top-level bindable refs (today: `parent`)
    /// bind here, at intake, before the op applies; the behavior refs
    /// bind after apply, against the spawned world — the same marked
    /// list, walked per pass (spec/world.md, "Identity").
    fn bind_top_level_refs(&self, value: &mut Value) -> Result<(), String> {
        for field in refs_of_kind(EntityRefScope::Entity, EntityRefKind::Bindable) {
            if field.path.len() != 1 {
                continue;
            }
            walk_ref_path(value, field.path, &mut |reference| self.resolve(reference))?;
        }
        Ok(())
    }

    /// A name becomes the id it names now; ids pass through.
    fn resolve(&self, reference: &mut Value) -> Result<(), String> {
        if let Value::String(name) = reference {
            let id = self
                .spawned
                .get(name.as_str())
                .copied()
                .or_else(|| self.trial.entity_id_by_name(name))
                .ok_or_else(|| format!("no entity is named \"{name}\""))?;
            *reference = json!(id);
        }
        Ok(())
    }
}

/// Merge each named object field of `patch` into `current`'s value.
fn merge_fields(patch: &mut serde_json::Map<String, Value>, current: &Value, fields: &[&str]) {
    for field in fields {
        if let (Some(change @ Value::Object(_)), Some(now @ Value::Object(_))) =
            (patch.get_mut(*field), current.get(*field))
        {
            *change = merge_patch(now, change);
        }
    }
}

/// JSON merge patch (RFC 7396): objects merge key by key, `null`
/// removes, anything else replaces.
pub fn merge_patch(current: &Value, change: &Value) -> Value {
    match (current, change) {
        (Value::Object(now), Value::Object(delta)) => {
            let mut out = now.clone();
            for (key, value) in delta {
                if value.is_null() {
                    out.remove(key);
                } else {
                    let next = out
                        .get(key)
                        .map_or_else(|| value.clone(), |old| merge_patch(old, value));
                    out.insert(key.clone(), next);
                }
            }
            Value::Object(out)
        }
        _ => change.clone(),
    }
}

/// Keys in `raw` the typed value didn't keep — fields every reader would
/// drop. Explicit nulls are fine (they clear).
fn dropped_keys(raw: &Value, back: &Value, path: String, out: &mut Vec<String>) {
    match (raw, back) {
        (Value::Object(raw), Value::Object(back)) => {
            for (key, value) in raw {
                let here = format!("{path}/{key}");
                match back.get(key) {
                    Some(kept) => dropped_keys(value, kept, here, out),
                    None if !value.is_null() => out.push(here),
                    None => {}
                }
            }
        }
        (Value::Array(raw), Value::Array(back)) => {
            for (i, (value, kept)) in raw.iter().zip(back).enumerate() {
                dropped_keys(value, kept, format!("{path}/{i}"), out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::WorldManifest;

    fn yard() -> WorldDoc {
        let manifest: WorldManifest = serde_json::from_value(json!({
            "version": 3,
            "meta": {"name": "yard"},
            "entities": [
                {"id": 1, "name": "ground", "shape": {"Plane": {"x": 20.0, "z": 20.0}}},
                {"id": 2, "name": "crate", "transform": {"position": [0.0, 0.5, 0.0], "scale": [2.0, 2.0, 2.0]},
                 "material": {"color": [0.6, 0.4, 0.2, 1.0], "roughness": 0.8}}
            ],
            "next_entity_id": 3
        }))
        .unwrap();
        manifest.as_base().unwrap()
    }

    #[test]
    fn names_bind_ids_allocate_and_partial_patches_merge() {
        let done = ingest(
            &yard(),
            &json!([
                {"SpawnEntity": {"entity": {"name": "lamp", "parent": "crate"}}},
                {"ModifyEntity": {"id": "crate", "patch": {"transform": {"position": [3.0, 0.5, 0.0]},
                                                          "material": {"base_color_texture": "brick.png"}}}},
                {"ModifyWorld": {"patch": {"meta": {"description": "a yard"}}}}
            ]),
        )
        .unwrap();
        assert_eq!(done.spawned["lamp"], 3);
        assert_eq!(done.doc.get(3).unwrap().parent.unwrap().0, 2);
        let crate_ = done.doc.get(2).unwrap();
        assert_eq!(crate_.transform.scale, [2.0, 2.0, 2.0], "scale kept");
        assert_eq!(
            crate_.material.as_ref().unwrap().roughness,
            0.8,
            "roughness kept"
        );
        assert_eq!(done.doc.meta().description.as_deref(), Some("a yard"));
        assert_eq!(done.doc.name, "yard", "meta merges, the name stays");
        // What commits is the whole value, so the fold needs no merging.
        let EditOp::ModifyEntity { patch, .. } = &done.ops[1] else {
            panic!()
        };
        assert_eq!(patch.transform.as_ref().unwrap().scale, [2.0, 2.0, 2.0]);
    }

    #[test]
    fn one_bad_op_refuses_the_batch_with_a_reason_per_op() {
        let refused = ingest(
            &yard(),
            &json!([
                {"ModifyEntity": {"id": "crate", "patch": {"material": {"colour": [1.0, 0.0, 0.0, 1.0]}}}},
                {"DeleteEntity": {"id": "nobody"}},
                {"MoveEntity": {"id": 2}},
                {"SpawnEntity": {"entity": {"id": 1, "name": "again"}}},
                {"ModifyEntity": {"id": "ground", "patch": {"transform": {"position": [0.0, 1.0, 0.0]}}}}
            ]),
        )
        .unwrap_err();
        assert_eq!(refused.errors.len(), 4, "{refused}");
        assert!(refused.errors[0].starts_with("op 0: /ModifyEntity/patch/material/colour"));
        assert!(refused.errors[1].contains("no entity is named \"nobody\""));
        assert!(refused.errors[2].contains("isn't an op kind"));
        assert!(refused.errors[3].contains("already exists"));
    }

    #[test]
    fn a_world_over_budget_commits_with_warnings_never_a_refusal() {
        // The 500 m slab breaks the extent limit, but budget limits are
        // the authority's policy: the batch commits and the author hears
        // about it as a warning (spec/session.md, "Authoring").
        let done = ingest(
            &yard(),
            &json!([{"SpawnEntity": {"entity": {"name": "wall",
                "shape": {"Cuboid": {"x": 1e6, "y": 1.0, "z": 1.0}}}}}]),
        )
        .unwrap();
        assert!(done.doc.get_by_name("wall").is_some());
        assert!(
            done.warnings.iter().any(|w| w.contains("exceeds limit")),
            "{:?}",
            done.warnings
        );
    }

    #[test]
    fn merge_patch_is_rfc_7396() {
        let merged = merge_patch(
            &json!({"a": 1, "b": {"c": 2, "d": 3}}),
            &json!({"b": {"c": null, "e": 4}, "f": 5}),
        );
        assert_eq!(merged, json!({"a": 1, "b": {"d": 3, "e": 4}, "f": 5}));
    }
}
