//! The session log's op vocabulary — what a line in `ops.jsonl` can hold.
//!
//! An op log entry is a committed batch by one author. Its `ops` are
//! [`SessionOp`]s, of which only [`SessionOp::Edit`] changes the shared
//! document; the others are history that folds to nothing — the model's
//! tool calls, a visitor's sampled input, host game state, a performance
//! clock. See `docs/rfcs/multiplayer/session-package-format.md`.
//!
//! `SessionOp` is untagged with `Edit` first, so a log written before the
//! other kinds existed (plain `EditOp` JSON) parses unchanged, and an
//! `Edit` written today serializes exactly as the old format did.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::doc::{ApplyError, WorldDoc};
use crate::entity_refs::each_ref;
use crate::history::EditOp;
use crate::oplog::OpLogEntry;

/// The session package format's version (`session.json`'s `format_version`).
/// Version 2 is head-first: `manifest.json` holds the world at the tip of
/// `main` and the base lives in `snapshots/base.json` (spec/package.md).
pub const SESSION_FORMAT_VERSION: u32 = 2;

/// The edit op kinds, as the shape collision rule spells them:
/// PascalCase, always — history kinds are lowercase, always, and the
/// two forms never collide (spec/session.md, "Compatibility").
pub const EDIT_KEYS: &[&str] = &[
    "SpawnEntity",
    "DeleteEntity",
    "ModifyEntity",
    "SetEnvironment",
    "SetCamera",
    "SetAmbience",
    "SpawnAudioEmitter",
    "RemoveAudioEmitter",
    "ModifyWorld",
    "Batch",
];

/// Whether `key` names an edit op kind (a serializer's guard: edits go
/// out PascalCase, history goes out lowercase, and an op that breaks
/// the rule is a bug in the writer, not a new kind).
pub fn is_edit_key(key: &str) -> bool {
    EDIT_KEYS.contains(&key)
}

/// The history op kinds, lowercase by the same rule.
pub const HISTORY_KEYS: &[&str] = &["tool", "input", "state", "clock", "merge"];

/// Whether a kind follows its case rule: edits PascalCase, history
/// lowercase. New kinds MUST pick a side (spec/session.md).
pub fn op_kind_shape_ok(kind: &str) -> bool {
    (is_edit_key(kind) && kind.chars().next().is_some_and(char::is_uppercase))
        || (HISTORY_KEYS.contains(&kind) && kind.chars().next().is_some_and(char::is_lowercase))
}

/// One op in a session log entry. Only `Edit` changes the document; the
/// rest is history that folds to nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SessionOp {
    /// A world edit, exactly as the room has always committed. Boxed so
    /// the history variants don't pay for an entity's size.
    Edit(Box<EditOp>),
    /// A tool call that ran during the session — the model's (or a
    /// pipeline's) intent, recorded next to the edits it caused.
    Tool(ToolRecord),
    /// A visitor's sampled state, for playthrough replay.
    Input(InputRecord),
    /// Host game state the document doesn't hold (score, inventory, …).
    State(StateRecord),
    /// A performance clock: Verse's song transport, a tour's clock.
    Clock(ClockRecord),
    /// Merge provenance: this batch came from a branch. Folds to
    /// nothing, like `Tool`; the merged edits are ordinary edit ops.
    Merge(MergeRecord),
    /// An extension op (`{"ext-physics": {...}}`): namespaced history
    /// owned by an extension, not the core. Folds to nothing for the
    /// document, like every history kind; readers that don't know the
    /// extension keep reading the log (the must-ignore rule).
    Extension(ExtensionRecord),
}

/// One extension op: the namespace key and its body, verbatim.
///
/// Serialize/Deserialize are hand-written because the shape is "one key
/// from the `ext-*` namespace" — a derived struct can't express that,
/// and a failed match must fall through (untagged) rather than reject
/// the line.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionRecord {
    /// The extension's namespace (`"ext-physics"`).
    pub name: String,
    /// The op's body, verbatim.
    pub body: serde_json::Value,
}

impl Serialize for ExtensionRecord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serde_json::Map::new();
        map.insert(self.name.clone(), self.body.clone());
        map.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ExtensionRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let map = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let mut iter = map.into_iter();
        let (name, body) = iter.next().ok_or_else(|| {
            <D::Error as serde::de::Error>::custom("an extension op holds one key")
        })?;
        if iter.next().is_some() {
            return Err(<D::Error as serde::de::Error>::custom(
                "an extension op holds exactly one key",
            ));
        }
        if !name.starts_with("ext-") {
            return Err(<D::Error as serde::de::Error>::custom(format!(
                "'{name}' is not an extension namespace (ext-…)"
            )));
        }
        Ok(ExtensionRecord { name, body })
    }
}

/// Where a merged batch came from: `{"merge": {"branch": "…"}}`, the
/// history kind's key wrapping its body, as `state` and `clock` do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeRecord {
    pub merge: MergeSource,
}

/// The body of a merge record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeSource {
    /// The merged branch's name (a ref in the source package).
    pub branch: String,
}

/// A tool invocation, as the old generation log recorded it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRecord {
    /// Tool name (e.g., `gen_spawn_primitive`).
    pub tool: String,
    /// The arguments the caller passed, verbatim.
    pub args: serde_json::Value,
    /// Hash of the result, for change detection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_hash: Option<String>,
    /// Pipeline phase (e.g., "blockout", "populate").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    /// Milliseconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_ms: Option<u64>,
}

/// A visitor's sampled state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputRecord {
    /// Who was sampled (a visitor id).
    pub input: InputSample,
}

/// One input sample: where the visitor was, what they looked at, what they
/// clicked. Position and look are sampled (~10 Hz); a click is discrete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSample {
    /// The visitor this sample belongs to.
    pub actor: String,
    /// Position in world space.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<[f32; 3]>,
    /// Yaw and pitch, degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<[f32; 2]>,
    /// The entity a click hit, by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub click: Option<u64>,
}

/// Host game state, as a flat map (`"score.chest": 10`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateRecord {
    /// The state that changed, keyed by name.
    pub state: BTreeMap<String, serde_json::Value>,
}

/// A clock event: transport for a timed performance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockRecord {
    /// The clock's state at the event.
    pub clock: ClockState,
}

/// Where a performance clock is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockState {
    /// Whether the clock is running.
    pub playing: bool,
    /// The clock's position, seconds.
    pub position_s: f64,
}

/// `session.json` — the package's metadata and integrity record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    /// The session package format version ([`SESSION_FORMAT_VERSION`]).
    pub format_version: u32,
    /// The session's name.
    pub name: String,
    /// Which app wrote the package ("gen", "md", "verse").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// The revision `snapshots/base.json` holds.
    pub base_revision: u64,
    /// The newest revision the log reaches.
    pub head_revision: u64,
    /// The session's seed, for deterministic replay. Reserved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// SHA-256 of `manifest.json` (the head) as last written, hex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_sha256: Option<String>,
    /// SHA-256 of `ops.jsonl` as of `head_revision`, hex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_sha256: Option<String>,
    /// When this package began as a fork: which package, at which entry
    /// or revision (a string like `castle-build@42`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forked_from: Option<String>,
    /// When the package was last written, milliseconds since the epoch.
    #[serde(default)]
    pub updated_ms: u64,
    /// Named tips of the history — `"main": "<entry id>"` and any others.
    ///
    /// spec/package.md: refs MUST name an entry `id`, never a line number,
    /// so they survive compaction and branching. `main` decides what
    /// `manifest.json` is (see [`main_tip`]). Absent means none are named.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub refs: std::collections::BTreeMap<String, String>,
    /// Every member this version doesn't model, kept as read.
    ///
    /// The must-ignore rule as it applies to a *writer*: a reader skips what
    /// it doesn't know, but an authority rewrites `package.json` on every
    /// commit, and one that dropped unknown members would destroy whatever a
    /// newer writer — or a person — put there. Before this field existed,
    /// that is exactly what happened to `refs`.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl SessionMeta {
    /// A fresh meta for a base at revision 0.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            format_version: SESSION_FORMAT_VERSION,
            name: name.into(),
            app: None,
            base_revision: 0,
            head_revision: 0,
            seed: None,
            world_sha256: None,
            log_sha256: None,
            forked_from: None,
            updated_ms: 0,
            refs: std::collections::BTreeMap::new(),
            extra: serde_json::Map::new(),
        }
    }
}

/// The tip whose fold is `manifest.json`: spec/package.md's head rule —
/// `refs.main` when the package names one, else the log's last entry (its
/// own id, or the synthesized `line-<n>` for an entry without one), else
/// `None`, meaning the base is the head.
///
/// This is the one place the rule lives in Rust, so an authority deciding
/// whether a commit extends `main`, and a reader deciding what the head is,
/// cannot disagree. The Swift and Kotlin references implement the same rule
/// over raw `package.json`.
pub fn main_tip(meta: &SessionMeta, entries: &[OpLogEntry]) -> Option<String> {
    if let Some(main) = meta.refs.get("main").filter(|m| !m.is_empty()) {
        return Some(main.clone());
    }
    let n = entries.len().checked_sub(1)?;
    Some(entries[n].id.clone().unwrap_or_else(|| format!("line-{n}")))
}

impl SessionOp {
    /// The edit this op carries, if it is one.
    pub fn as_edit(&self) -> Option<&EditOp> {
        match self {
            SessionOp::Edit(op) => Some(op),
            _ => None,
        }
    }
}

impl OpLogEntry {
    /// The entry's edits, in order — the ops that change a document.
    /// Tool, input, state and clock records are skipped.
    pub fn edit_ops(&self) -> Vec<EditOp> {
        self.ops
            .iter()
            .filter_map(|op| match op {
                SessionOp::Edit(edit) => Some((**edit).clone()),
                _ => None,
            })
            .collect()
    }

    /// True when the entry carries no edits (history only).
    pub fn is_history_only(&self) -> bool {
        self.ops.iter().all(|op| !matches!(op, SessionOp::Edit(_)))
    }
}

/// The history of a log with entry identity applied: every entry gets an
/// id (its own, or `line-<n>`) and a parent (its own, or the previous
/// entry). A log with no ids is a chain in file order.
/// One entry of a log with its identity applied.
struct Identified<'a> {
    entry: &'a OpLogEntry,
    id: String,
    parent: Option<String>,
}

fn with_identity(entries: &[OpLogEntry]) -> Result<Vec<Identified<'_>>, ApplyError> {
    let mut out: Vec<Identified<'_>> = Vec::with_capacity(entries.len());
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut previous: Option<String> = None;
    for (n, entry) in entries.iter().enumerate() {
        let id = entry.id.clone().unwrap_or_else(|| format!("line-{n}"));
        if !seen.insert(id.clone()) {
            return Err(ApplyError::Invalid(format!("duplicate entry id '{id}'")));
        }
        if let Some(parent) = entry.parent.clone().or_else(|| previous.clone())
            && !seen.contains(&parent)
        {
            return Err(ApplyError::Invalid(format!(
                "entry '{id}' names parent '{parent}', which isn't in the log yet"
            )));
        }
        let parent = entry.parent.clone().or_else(|| previous.clone());
        out.push(Identified {
            entry,
            id: id.clone(),
            parent,
        });
        previous = Some(id);
    }
    Ok(out)
}

/// Fold one path of a branching history: the document at `tip` (an entry
/// id; `None` folds the last entry in file order), plus the ids of the
/// path folded. Entries without ids chain in file order, so a linear log
/// is the degenerate branch.
pub fn fold_path(
    base: &WorldDoc,
    entries: &[OpLogEntry],
    tip: Option<&str>,
) -> Result<(WorldDoc, Vec<String>), ApplyError> {
    let identified = with_identity(entries)?;
    let target = match tip {
        Some(id) => identified
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.id.clone())
            .ok_or_else(|| ApplyError::Invalid(format!("no entry '{id}' in this log")))?,
        None => identified
            .last()
            .ok_or_else(|| ApplyError::Invalid("the log is empty".into()))?
            .id
            .clone(),
    };
    // Walk parent links tip → base, then fold the chain forward.
    let by_id: std::collections::HashMap<&str, usize> = identified
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i))
        .collect();
    let mut chain_idx = Vec::new();
    let mut cursor = Some(target);
    while let Some(id) = cursor {
        let idx = *by_id
            .get(id.as_str())
            .ok_or_else(|| ApplyError::Invalid(format!("no entry '{}' in this log", id)))?;
        let parent = identified[idx].parent.clone();
        chain_idx.push(idx);
        cursor = parent;
    }
    chain_idx.reverse();
    let chain: Vec<OpLogEntry> = chain_idx
        .iter()
        .map(|&i| identified[i].entry.clone())
        .collect();
    let path: Vec<String> = chain_idx
        .iter()
        .map(|&i| identified[i].id.clone())
        .collect();
    let doc = fold_log(base, &chain)?;
    Ok((doc, path))
}

/// Fold log entries onto a base document: the state at the last entry.
///
/// Each entry's edits apply atomically, as the room applied them, and
/// the names the entry introduces bind against the fold-so-far at
/// ingestion (spec/world.md); the fold stops at the first entry that
/// no longer applies.
pub fn fold_log(base: &WorldDoc, entries: &[OpLogEntry]) -> Result<WorldDoc, ApplyError> {
    let mut doc = base.clone();
    for entry in entries {
        // The fold owns `doc` and returns `Err` without it, so the
        // per-entry transactional copy protects nothing and would make
        // folding quadratic in the log's length.
        doc.apply_entry_in_place(&entry.edit_ops())?;
    }
    Ok(doc)
}

/// What [`merge_branch`] did: the entries to append, and the ids it had
/// to move out of the way.
#[derive(Debug, Clone)]
pub struct MergedBranch {
    /// The branch's entries with colliding ids reallocated and every
    /// reference to them rewritten — ready to append to the main log
    /// (with a [`SessionOp::Merge`] record saying where they came from).
    pub entries: Vec<OpLogEntry>,
    /// Old id → new id, for the entities the branch spawned under ids
    /// the main branch had concurrently allocated.
    pub remapped: std::collections::BTreeMap<u64, u64>,
}

/// Merge a branch's entries into a main line, by the spec's exact rules
/// (spec/session.md, "The merge rules, exactly"): scan the incoming
/// branch for entity ids allocated concurrently on the main branch,
/// reallocate them, and rewrite every reference to them inside the
/// incoming batch before appending.
///
/// An id the branch *spawns* (a `SpawnEntity`, inside a `Batch`
/// included) that the main document currently holds collides;
/// references to ids the branch only *uses* (a modify of an entity both
/// lines share) are left alone. Colliding ids are reallocated in
/// ascending order onto fresh ids from the main document's effective
/// `next_entity_id` — [`WorldDoc::next_id`], the floor that keeps spent
/// ids spent — stepping past every id the branch spawns, never past the
/// 2^53-1 ceiling. Every entity reference in the merged entries is
/// rewritten through the remap — the schema's marked entity refs
/// (schema/entity-refs.json): `entity` scope on spawn entities and
/// modify patches, `avatar` and `creation` scopes on ModifyWorld's
/// patch — plus the identity fields `SpawnEntity`'s `entity.id`,
/// `ModifyEntity`'s `id` and `DeleteEntity`'s `id`, which are op
/// addresses, not schema refs. `Batch` recurses.
///
/// Name collisions get the spec's suffix: a branch spawn whose name is
/// taken — by main, or by an earlier spawn in the same merge — is
/// renamed `<name>-<n>`, `n` from 2 up, first unused. Only the
/// `SpawnEntity` changes; string references elsewhere are names, not
/// ids, and are left alone.
///
/// The merged entries keep their `id`, `parent`, `author` and `message`
/// — an entry's identity survives the merge.
pub fn merge_branch(main: &WorldDoc, entries: &[OpLogEntry]) -> Result<MergedBranch, ApplyError> {
    // Pass 1: what the branch spawns, and which of those ids the main
    // line already holds.
    let mut branch_spawns: Vec<u64> = Vec::new();
    for entry in entries {
        scan_spawns(&entry.edit_ops(), &mut branch_spawns);
    }
    let mut collisions: Vec<u64> = branch_spawns
        .iter()
        .copied()
        .filter(|id| main.contains(*id))
        .collect();
    collisions.sort_unstable();
    collisions.dedup();

    // Pass 2: fresh ids for the collisions, past everything held.
    let mut taken: std::collections::BTreeSet<u64> = main
        .entities()
        .map(|e| e.id.0)
        .chain(branch_spawns.iter().copied())
        .collect();
    let mut next = main.next_id();
    let mut remapped = std::collections::BTreeMap::new();
    for old in &collisions {
        while taken.contains(&next) {
            next += 1;
        }
        if next > crate::identity::MAX_ENTITY_ID {
            return Err(ApplyError::Invalid(format!(
                "merge needs a fresh id past {}, but ids stop at {} (2^53-1)",
                old,
                crate::identity::MAX_ENTITY_ID
            )));
        }
        taken.insert(next);
        remapped.insert(*old, next);
        next += 1;
    }

    // Pass 3: rewrite every reference to a remapped id, then rename the
    // spawns whose names main (or an earlier spawn in this merge) took.
    let mut rewritten: Vec<OpLogEntry> = entries
        .iter()
        .map(|entry| OpLogEntry {
            revision: entry.revision,
            author: entry.author.clone(),
            ops: entry
                .ops
                .iter()
                .map(|op| rewrite_session_op(op.clone(), &remapped))
                .collect(),
            timestamp_ms: entry.timestamp_ms,
            id: entry.id.clone(),
            parent: entry.parent.clone(),
            message: entry.message.clone(),
        })
        .collect();
    rename_colliding_spawns(main, &mut rewritten);
    Ok(MergedBranch {
        entries: rewritten,
        remapped,
    })
}

/// Collect the entity ids a batch of edits spawns.
fn scan_spawns(ops: &[crate::history::EditOp], out: &mut Vec<u64>) {
    for op in ops {
        match op {
            EditOp::SpawnEntity { entity } => out.push(entity.id.0),
            EditOp::Batch { ops } => scan_spawns(ops, out),
            _ => {}
        }
    }
}

/// Rewrite one session op's references through the remap. History kinds
/// ride along untouched.
fn rewrite_session_op(op: SessionOp, remapped: &std::collections::BTreeMap<u64, u64>) -> SessionOp {
    match op {
        SessionOp::Edit(edit) => SessionOp::Edit(Box::new(rewrite_edit(*edit, remapped))),
        other => other,
    }
}

/// Rewrite one edit's entity references through the remap: the schema's
/// marked entity refs (spec/world.md, "Identity") — `entity` scope on
/// spawn entities and modify patches, `avatar` and `creation` scopes on
/// ModifyWorld's patch, walked off the list. The identity fields
/// (`entity.id`, `ModifyEntity.id`, `DeleteEntity.id`) are op
/// addresses, not schema refs — they stay explicit.
fn rewrite_edit(op: EditOp, remapped: &std::collections::BTreeMap<u64, u64>) -> EditOp {
    let map_id = |id: &mut crate::identity::EntityId| {
        if let Some(&to) = remapped.get(&id.0) {
            *id = crate::identity::EntityId(to);
        }
    };
    match op {
        EditOp::SpawnEntity { mut entity } => {
            map_id(&mut entity.id);
            each_ref(&mut entity, |slot| slot.remap(remapped));
            EditOp::spawn(entity)
        }
        EditOp::DeleteEntity { mut id } => {
            map_id(&mut id);
            EditOp::delete(id)
        }
        EditOp::ModifyEntity { mut id, mut patch } => {
            map_id(&mut id);
            each_ref(&mut patch, |slot| slot.remap(remapped));
            EditOp::modify(id, patch)
        }
        EditOp::Batch { ops } => EditOp::Batch {
            ops: ops
                .into_iter()
                .map(|op| rewrite_edit(op, remapped))
                .collect(),
        },
        EditOp::ModifyWorld { mut patch } => {
            if let Some(Some(avatar)) = &mut patch.avatar {
                each_ref(avatar, |slot| slot.remap(remapped));
            }
            if let Some(creations) = &mut patch.creations {
                for creation in creations {
                    each_ref(creation, |slot| slot.remap(remapped));
                }
            }
            EditOp::ModifyWorld { patch }
        }
        other => other,
    }
}

/// The spec's name rule: a spawned name that is taken — by main, or by
/// an earlier spawn in the same merge — becomes `<name>-<n>`, `n` from
/// 2 up, first unused. Only the `SpawnEntity` changes; string
/// references elsewhere are names, not ids, and ride. In place on the
/// merged entries.
fn rename_colliding_spawns(main: &WorldDoc, entries: &mut [OpLogEntry]) {
    let mut taken: std::collections::HashSet<String> = main
        .entities()
        .map(|e| e.name.0.clone())
        .collect();
    for entry in entries {
        for op in &mut entry.ops {
            if let SessionOp::Edit(edit) = op {
                rename_spawn_ops(edit, &mut taken);
            }
        }
    }
}

/// One edit's spawns renamed against `taken`, a `Batch` recursively.
fn rename_spawn_ops(op: &mut EditOp, taken: &mut std::collections::HashSet<String>) {
    match op {
        EditOp::SpawnEntity { entity } => {
            if taken.contains(entity.name.as_str()) {
                let base = entity.name.0.clone();
                let mut n = 2;
                while taken.contains(&format!("{base}-{n}")) {
                    n += 1;
                }
                entity.name = crate::identity::EntityName::new(format!("{base}-{n}"));
            }
            taken.insert(entity.name.0.clone());
        }
        EditOp::Batch { ops } => {
            for op in ops {
                rename_spawn_ops(op, taken);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as wt;
    use crate::author::Author;

    fn entity(id: u64, name: &str) -> wt::WorldEntity {
        let mut e = wt::WorldEntity::new(id, name);
        e.transform.position = [id as f32, 0.0, 0.0];
        e
    }

    fn bare_entry(id: Option<&str>, revision: u64) -> OpLogEntry {
        OpLogEntry {
            revision,
            author: Default::default(),
            ops: Vec::new(),
            timestamp_ms: 0,
            id: id.map(str::to_string),
            parent: None,
            message: None,
        }
    }

    #[test]
    fn refs_survive_a_read_and_a_rewrite() {
        // An authority rewrites package.json on every commit. Before refs
        // were modelled, a package that named its main tip lost it here.
        let text = r#"{"format_version": 2, "name": "castle", "base_revision": 0,
                       "head_revision": 3, "refs": {"main": "e3", "moat": "e5"}}"#;
        let meta: SessionMeta = serde_json::from_str(text).unwrap();
        assert_eq!(meta.refs.get("main").map(String::as_str), Some("e3"));
        let again: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&meta).unwrap()).unwrap();
        assert_eq!(again["refs"]["main"], "e3");
        assert_eq!(again["refs"]["moat"], "e5");
    }

    #[test]
    fn members_this_version_does_not_know_are_kept_on_a_rewrite() {
        // The must-ignore rule for writers: skip what you don't know when
        // reading, and keep it when writing back.
        let text = r#"{"format_version": 2, "name": "castle", "base_revision": 0,
                       "head_revision": 0, "profiles": ["authoring"],
                       "x-studio": {"camera": "north"}}"#;
        let meta: SessionMeta = serde_json::from_str(text).unwrap();
        let again: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&meta).unwrap()).unwrap();
        assert_eq!(again["profiles"], serde_json::json!(["authoring"]));
        assert_eq!(again["x-studio"]["camera"], "north");
    }

    #[test]
    fn no_refs_and_no_extras_write_nothing_new() {
        // A package written by an older authority must not grow members it
        // never had just by being opened and saved.
        let meta = SessionMeta::new("castle");
        let value = serde_json::to_value(&meta).unwrap();
        assert!(value.get("refs").is_none(), "{value}");
    }

    #[test]
    fn the_main_tip_is_refs_main_else_the_last_entry_else_the_base() {
        let mut meta = SessionMeta::new("castle");
        // No log: the base is the head.
        assert_eq!(main_tip(&meta, &[]), None);

        // A log: its last entry, by its own id or the synthesized one.
        let entries = [bare_entry(Some("e1"), 1), bare_entry(None, 2)];
        assert_eq!(main_tip(&meta, &entries).as_deref(), Some("line-1"));
        let entries = [bare_entry(Some("e1"), 1), bare_entry(Some("e2"), 2)];
        assert_eq!(main_tip(&meta, &entries).as_deref(), Some("e2"));

        // refs.main wins over file order — which is what lets a branch be
        // appended to the log without becoming main.
        meta.refs.insert("main".into(), "e1".into());
        assert_eq!(main_tip(&meta, &entries).as_deref(), Some("e1"));

        // An empty name is no name.
        meta.refs.insert("main".into(), String::new());
        assert_eq!(main_tip(&meta, &entries).as_deref(), Some("e2"));
    }

    #[test]
    fn edit_serializes_as_the_old_format_did() {
        let op = SessionOp::Edit(Box::new(EditOp::spawn(entity(1, "lighthouse"))));
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.starts_with("{\"SpawnEntity\""));
        let back: SessionOp = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
    }

    #[test]
    fn old_log_line_parses_as_edits() {
        // The shape world-sync wrote before session ops existed.
        let line = r#"{"revision":7,"author":{"peer":3,"name":"maya"},
            "ops":[{"SpawnEntity":{"entity":{"id":1,"name":"lighthouse"}}}],
            "timestamp_ms":1700000000000}"#;
        let entry: OpLogEntry = serde_json::from_str(line).unwrap();
        assert_eq!(entry.revision, 7);
        assert_eq!(entry.edit_ops().len(), 1);
        assert!(!entry.is_history_only());
    }

    #[test]
    fn tool_input_state_clock_roundtrip_and_skip() {
        let entry = OpLogEntry {
            revision: 42,
            author: Author {
                peer: None,
                name: "llm".into(),
            },
            ops: vec![
                SessionOp::Tool(ToolRecord {
                    tool: "gen_spawn_primitive".into(),
                    args: serde_json::json!({"name": "cube"}),
                    result_hash: Some("sha256:ab".into()),
                    phase: Some("blockout".into()),
                    timestamp_ms: Some(7),
                }),
                SessionOp::Input(InputRecord {
                    input: InputSample {
                        actor: "visitor-7".into(),
                        position: Some([3.0, 1.8, -2.0]),
                        look: None,
                        click: Some(17),
                    },
                }),
                SessionOp::State(StateRecord {
                    state: BTreeMap::from([("score.chest".into(), serde_json::json!(10))]),
                }),
                SessionOp::Clock(ClockRecord {
                    clock: ClockState {
                        playing: true,
                        position_s: 41.5,
                    },
                }),
            ],
            timestamp_ms: 9,
            id: None,
            parent: None,
            message: None,
        };
        let line = crate::encode_line(&entry).unwrap();
        let back: OpLogEntry = crate::decode_line(&line).unwrap();
        assert_eq!(back.revision, 42);
        assert_eq!(back.ops.len(), 4);
        assert!(back.is_history_only());
        assert!(entry.is_history_only());
        assert!(entry.edit_ops().is_empty());
    }

    fn branched_entries() -> Vec<OpLogEntry> {
        // A trunk (1→2→3) and a fork from 2 (4→5, where 5 is a merge
        // record): two tips, like examples/forked-exploration.
        let mk = |id: Option<&str>, parent: Option<&str>, rev: u64, name: &str| OpLogEntry {
            revision: rev,
            author: Author {
                peer: None,
                name: "maya".into(),
            },
            ops: vec![SessionOp::Edit(Box::new(EditOp::spawn(entity(
                100 + rev,
                name,
            ))))],
            timestamp_ms: rev,
            id: id.map(Into::into),
            parent: parent.map(Into::into),
            message: None,
        };
        vec![
            mk(Some("e1"), None, 1, "keep"),
            mk(Some("e2"), Some("e1"), 2, "wall"),
            mk(Some("e3"), Some("e2"), 3, "garden"),
            mk(Some("e4"), Some("e2"), 3, "moat"),
            OpLogEntry {
                revision: 4,
                author: Author {
                    peer: None,
                    name: "host".into(),
                },
                ops: vec![SessionOp::Merge(MergeRecord {
                    merge: MergeSource {
                        branch: "moat-variant".into(),
                    },
                })],
                timestamp_ms: 4,
                id: Some("e5".into()),
                parent: Some("e4".into()),
                message: None,
            },
        ]
    }

    #[test]
    fn a_merge_record_reads_as_the_spec_writes_it() {
        let op: SessionOp = serde_json::from_str(r#"{"merge":{"branch":"moat-variant"}}"#).unwrap();
        assert!(matches!(&op, SessionOp::Merge(m) if m.merge.branch == "moat-variant"));
        assert_eq!(
            serde_json::to_string(&op).unwrap(),
            r#"{"merge":{"branch":"moat-variant"}}"#
        );
    }

    #[test]
    fn fold_path_folds_each_tip_of_a_branch() {
        let base = WorldDoc::new("base");
        let entries = branched_entries();

        let (trunk, path) = fold_path(&base, &entries, Some("e3")).unwrap();
        assert_eq!(path, vec!["e1", "e2", "e3"]);
        assert!(trunk.get_by_name("garden").is_some());
        assert!(trunk.get_by_name("moat").is_none());

        // The merge record folds to nothing: e5's document is e4's.
        let (variant, path) = fold_path(&base, &entries, Some("e5")).unwrap();
        assert_eq!(path, vec!["e1", "e2", "e4", "e5"]);
        assert!(variant.get_by_name("moat").is_some());
        assert!(variant.get_by_name("garden").is_none());
        assert_eq!(
            variant.len(),
            fold_path(&base, &entries, Some("e4")).unwrap().0.len()
        );

        // No tip: the last entry in file order; unknown tips refuse.
        let (last, _) = fold_path(&base, &entries, None).unwrap();
        assert!(last.get_by_name("moat").is_some());
        assert!(fold_path(&base, &entries, Some("e99")).is_err());
    }

    #[test]
    fn entries_without_ids_chain_in_file_order() {
        let base = WorldDoc::new("base");
        let entries = vec![
            OpLogEntry {
                revision: 1,
                author: Author {
                    peer: None,
                    name: "t".into(),
                },
                ops: vec![SessionOp::Edit(Box::new(EditOp::spawn(entity(1, "a"))))],
                timestamp_ms: 0,
                id: None,
                parent: None,
                message: None,
            },
            OpLogEntry {
                revision: 2,
                author: Author {
                    peer: None,
                    name: "t".into(),
                },
                ops: vec![SessionOp::Edit(Box::new(EditOp::spawn(entity(2, "b"))))],
                timestamp_ms: 1,
                id: None,
                parent: None,
                message: None,
            },
        ];
        let (doc, path) = fold_path(&base, &entries, None).unwrap();
        assert_eq!(doc.len(), 2);
        assert_eq!(path, vec!["line-0", "line-1"]);
    }

    #[test]
    fn merging_reallocates_colliding_ids_and_rewrites_references() {
        // The main line holds id 5; the branch forked before 5 existed
        // and allocated its own 5 (plus a child and a behavior ref to
        // it). The merge authority moves the branch's 5 out of the way
        // and rewrites everything that pointed at it.
        let mut main = WorldDoc::new("main");
        main.apply(&wt::EditOp::spawn(entity(1, "keep"))).unwrap();
        main.apply(&wt::EditOp::spawn(entity(5, "moat"))).unwrap();

        let mut branch_child = entity(6, "drawbridge");
        branch_child.parent = Some(wt::EntityId(5));
        branch_child.behaviors = vec![crate::behavior::BehaviorDef::LookAt {
            target: crate::identity::EntityRef::id(5),
        }];
        let branch = vec![
            OpLogEntry {
                revision: 3,
                author: Author {
                    peer: None,
                    name: "branch".into(),
                },
                ops: vec![SessionOp::Edit(Box::new(wt::EditOp::spawn(entity(
                    5, "wall",
                ))))],
                timestamp_ms: 3,
                id: Some("b1".into()),
                parent: None,
                message: None,
            },
            OpLogEntry {
                revision: 4,
                author: Author {
                    peer: None,
                    name: "branch".into(),
                },
                ops: vec![SessionOp::Edit(Box::new(wt::EditOp::spawn(branch_child)))],
                timestamp_ms: 4,
                id: Some("b2".into()),
                parent: Some("b1".into()),
                message: None,
            },
        ];

        let merged = merge_branch(&main, &branch).unwrap();
        // 5 collided; 6 did not (the main line never allocated it), so
        // the fresh id steps past everything either line holds: 7.
        assert_eq!(merged.remapped.get(&5), Some(&7));
        assert!(!merged.remapped.contains_key(&6));

        // The rewritten branch applies to the main document, and every
        // reference to the moved id moved with it.
        let doc = fold_log(&main, &merged.entries).unwrap();
        assert!(doc.get(5).is_some(), "the main line's 5 is untouched");
        let wall = doc.get_by_name("wall").unwrap();
        assert_eq!(wall.id.0, 7, "the branch's wall lives at the fresh id");
        let bridge = doc.get_by_name("drawbridge").unwrap();
        assert_eq!(bridge.parent, Some(wt::EntityId(7)));
        match &bridge.behaviors[0] {
            crate::behavior::BehaviorDef::LookAt { target } => {
                assert_eq!(*target, crate::identity::EntityRef::id(7));
            }
            other => panic!("unexpected behavior {other:?}"),
        }
    }

    #[test]
    fn merging_keeps_entry_identity_rewrites_world_refs_and_renames_taken_names() {
        // Main holds id 1 (a "lighthouse") and an id it deleted (2 —
        // spent). The branch collides on the id, references it from a
        // ModifyWorld, and mints the name again: the merge moves the id
        // past the spent floor, rewrites the world refs, suffixes the
        // name, and keeps the entry's message and author.
        let mut main = WorldDoc::new("main");
        main.apply(&wt::EditOp::spawn(entity(1, "lighthouse"))).unwrap();
        main.apply(&wt::EditOp::spawn(entity(2, "gone"))).unwrap();
        main.apply(&wt::EditOp::delete(wt::EntityId(2))).unwrap();
        assert_eq!(main.next_id(), 3, "a deleted id stays spent");

        let branch = vec![OpLogEntry {
            revision: 1,
            author: Author {
                peer: None,
                name: "branch".into(),
            },
            ops: vec![
                SessionOp::Edit(Box::new(wt::EditOp::spawn(entity(1, "lighthouse")))),
                SessionOp::Edit(Box::new(wt::EditOp::ModifyWorld {
                    patch: Box::new(crate::world_patch::WorldPatch {
                        avatar: Some(Some(crate::avatar::AvatarDef {
                            model_entity: Some(crate::identity::EntityRef::id(1)),
                            ..Default::default()
                        })),
                        creations: Some(vec![crate::creation::CreationDef {
                            id: wt::CreationId(9),
                            name: "pair".into(),
                            semantic_category: None,
                            bbox_half: [0.0; 3],
                            entities: vec![wt::EntityId(1)],
                            parts: Vec::new(),
                        }]),
                        ..Default::default()
                    }),
                })),
            ],
            timestamp_ms: 1,
            id: Some("b1".into()),
            parent: None,
            message: Some("a second lighthouse".into()),
        }];

        let merged = merge_branch(&main, &branch).unwrap();
        assert_eq!(
            merged.remapped.iter().collect::<Vec<_>>(),
            vec![(&1, &3)],
            "the fresh id comes from the spent-aware floor"
        );
        let entry = &merged.entries[0];
        assert_eq!(entry.message.as_deref(), Some("a second lighthouse"));
        assert_eq!(entry.author.name, "branch");
        assert_eq!(entry.id.as_deref(), Some("b1"));

        let doc = fold_log(&main, &merged.entries).unwrap();
        assert!(
            doc.get_by_name("lighthouse-2").is_some(),
            "the taken name is suffixed, first unused n"
        );
        let avatar = doc.avatar.as_ref().unwrap();
        assert_eq!(
            avatar.model_entity,
            Some(crate::identity::EntityRef::id(3)),
            "avatar.model_entity is an entity id: rewritten"
        );
        assert_eq!(
            doc.creations[0].entities,
            vec![wt::EntityId(3)],
            "creations[].entities are entity ids: rewritten"
        );
    }

    #[test]
    fn merging_leaves_shared_references_alone() {
        // The branch modifies an entity both lines hold: a reference,
        // not a spawn — the merge must not touch it.
        let mut main = WorldDoc::new("main");
        main.apply(&wt::EditOp::spawn(entity(1, "keep"))).unwrap();
        let entry = OpLogEntry {
            revision: 2,
            author: Author {
                peer: None,
                name: "branch".into(),
            },
            ops: vec![SessionOp::Edit(Box::new(wt::EditOp::modify(
                wt::EntityId(1),
                wt::EntityPatch {
                    name: Some(crate::identity::EntityName::new("kept")),
                    ..Default::default()
                },
            )))],
            timestamp_ms: 2,
            id: Some("b1".into()),
            parent: None,
            message: None,
        };
        let merged = merge_branch(&main, &[entry]).unwrap();
        assert!(merged.remapped.is_empty());
        let doc = fold_log(&main, &merged.entries).unwrap();
        assert_eq!(doc.get(1).unwrap().name.as_str(), "kept");
    }

    #[test]
    fn edits_stay_pascal_case_and_history_lowercase() {
        // The shape collision rule, as the serializer's guard: every
        // edit kind starts uppercase, every history kind lowercase.
        for key in EDIT_KEYS {
            assert!(op_kind_shape_ok(key), "{key} must be PascalCase");
        }
        for key in HISTORY_KEYS {
            assert!(op_kind_shape_ok(key), "{key} must be lowercase");
        }
        assert!(!op_kind_shape_ok("spawnentity"));
        assert!(!op_kind_shape_ok("Tool"));
        // And the wire agrees: an edit serializes exactly as it always did.
        let op = SessionOp::Edit(Box::new(EditOp::spawn(entity(1, "lighthouse"))));
        assert!(
            serde_json::to_string(&op)
                .unwrap()
                .starts_with("{\"SpawnEntity\"")
        );
        let record = SessionOp::Tool(ToolRecord {
            tool: "t".into(),
            args: serde_json::json!({}),
            result_hash: None,
            phase: None,
            timestamp_ms: None,
        });
        assert!(
            serde_json::to_string(&record)
                .unwrap()
                .starts_with("{\"tool\"")
        );
    }

    #[test]
    fn fold_log_applies_edits_and_skips_history() {
        let base = WorldDoc::new("base");
        let entries = vec![
            OpLogEntry {
                revision: 1,
                author: Author {
                    peer: None,
                    name: "llm".into(),
                },
                ops: vec![SessionOp::Edit(Box::new(EditOp::spawn(entity(1, "a"))))],
                timestamp_ms: 0,
                id: None,
                parent: None,
                message: None,
            },
            OpLogEntry {
                revision: 1,
                author: Author {
                    peer: None,
                    name: "llm".into(),
                },
                ops: vec![SessionOp::Tool(ToolRecord {
                    tool: "gen_modify_entity".into(),
                    args: serde_json::json!({"entity": "a"}),
                    result_hash: None,
                    phase: None,
                    timestamp_ms: None,
                })],
                timestamp_ms: 1,
                id: None,
                parent: None,
                message: None,
            },
            OpLogEntry {
                revision: 2,
                author: Author {
                    peer: None,
                    name: "maya".into(),
                },
                ops: vec![SessionOp::Edit(Box::new(EditOp::spawn(entity(2, "b"))))],
                timestamp_ms: 2,
                id: None,
                parent: None,
                message: None,
            },
        ];
        let doc = fold_log(&base, &entries).unwrap();
        assert_eq!(doc.len(), 2);
        assert!(doc.contains(2));

        let broken = vec![OpLogEntry {
            revision: 3,
            author: Author {
                peer: None,
                name: "x".into(),
            },
            ops: vec![SessionOp::Edit(Box::new(EditOp::delete(wt::EntityId(99))))],
            timestamp_ms: 3,
            id: None,
            parent: None,
            message: None,
        }];
        assert!(fold_log(&base, &broken).is_err());
    }
}
