// The Open World Format reference fold.
//
// Pure JavaScript, no dependencies, no engine. It parses a world document
// (manifest.json) and folds a session log (ops.jsonl) over it, applying
// the same rules the specification states: only edits change the
// document, history kinds fold to nothing, ops are recognized by shape
// (edits first), and a batch applies all-or-nothing.
//
// Spec: https://openworldformat.org  ·  schema version 3

/** The manifest schema version this fold reads. */
export const SUPPORTED_SCHEMA_VERSION = 3;

/** The package format version this fold reads. */
export const SUPPORTED_FORMAT_VERSION = 1;

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/**
 * Parse and sanity-check a world document.
 * @param {string} json the manifest's JSON text
 * @returns {object} the manifest
 * @throws when the text isn't JSON or the schema version isn't supported
 */
export function parseManifest(json) {
  const manifest = JSON.parse(json);
  if (typeof manifest.version !== "number") {
    throw new Error("manifest has no schema version — refusing to guess");
  }
  if (manifest.version > SUPPORTED_SCHEMA_VERSION) {
    throw new Error(
      `manifest schema version ${manifest.version} is newer than this reader (${SUPPORTED_SCHEMA_VERSION}); ` +
        "a newer reader must read it — see the versioning policy",
    );
  }
  if (!Array.isArray(manifest.entities)) {
    throw new Error("manifest has no entities array");
  }
  return manifest;
}

const EDIT_KEYS = new Set([
  "SpawnEntity",
  "DeleteEntity",
  "ModifyEntity",
  "SetEnvironment",
  "SetCamera",
  "SetAmbience",
  "SpawnAudioEmitter",
  "RemoveAudioEmitter",
  "Batch",
]);

/**
 * Classify one op by its shape, edits first — the compatibility rule: a
 * log written before the history kinds existed parses as edits, and an
 * edit serializes today exactly as it always did.
 * @param {object} op
 * @returns {{kind: "edit"|"tool"|"input"|"state"|"clock"|"unknown", edit?: string, value?: object}}
 */
export function classifyOp(op) {
  if (op === null || typeof op !== "object" || Array.isArray(op)) {
    return { kind: "unknown" };
  }
  const keys = Object.keys(op);
  if (keys.length === 1 && EDIT_KEYS.has(keys[0])) {
    return { kind: "edit", edit: keys[0], value: op[keys[0]] };
  }
  if (typeof op.tool === "string" && "args" in op) return { kind: "tool", value: op };
  if (op.input && typeof op.input.actor === "string") return { kind: "input", value: op.input };
  if (op.state && typeof op.state === "object") return { kind: "state", value: op.state };
  if (op.clock && typeof op.clock === "object") return { kind: "clock", value: op.clock };
  if (op.merge && typeof op.merge === "object") return { kind: "merge", value: op.merge };
  return { kind: "unknown" };
}

/**
 * Parse one log line into an entry with classified ops. Unreadable lines
 * are the writer's crash, not the reader's — the caller decides whether
 * to skip (the spec says skip the last one, count the rest).
 * @param {string} line one line of ops.jsonl
 * @returns {{revision: number, author: object, timestamp_ms: number, ops: array, classified: array}}
 */
export function parseLogLine(line) {
  const entry = JSON.parse(line);
  if (typeof entry.revision !== "number" || !Array.isArray(entry.ops)) {
    throw new Error("log entry needs a revision and an ops array");
  }
  return { ...entry, classified: entry.ops.map(classifyOp) };
}

/** An entry's edits, in order — the ops that change the document. */
export function editOps(entry) {
  return entry.classified === undefined
    ? entry.ops.map(classifyOp).filter((c) => c.kind === "edit")
    : entry.classified.filter((c) => c.kind === "edit");
}

// ---------------------------------------------------------------------------
// State folding
// ---------------------------------------------------------------------------

/**
 * Fold a session log's `state` ops over a state document: the values at
 * the last entry. Separate from the document fold — state ops never
 * touch entities — and equally tolerant: keys nothing declares are
 * carried, not refused (spec/state.md).
 *
 * @param {object} stateDoc parsed `state.json` ({format_version, fields})
 * @param {array} entries parsed log entries, in order
 * @returns {{values: object, undeclared: string[]}}
 */
export function foldState(stateDoc, entries) {
  const fields = stateDoc?.fields ?? {};
  const values = {};
  for (const [key, field] of Object.entries(fields)) {
    values[key] = structuredClone(field.initial ?? null);
  }
  const undeclared = new Set();

  for (const entry of entries) {
    const classified = entry.classified ?? entry.ops.map(classifyOp);
    for (const c of classified) {
      if (c.kind !== "state") continue;
      for (const [key, value] of Object.entries(c.value)) {
        if (key in fields) {
          // A declared field: set it, or reset it to its initial value.
          values[key] = value === null
            ? structuredClone(fields[key].initial ?? null)
            : structuredClone(value);
          continue;
        }
        // Maybe a subkey of a declared map field: "inventory.rope" under
        // the declared map "inventory".
        const dot = key.indexOf(".");
        if (dot > 0) {
          const base = key.slice(0, dot);
          const inner = key.slice(dot + 1);
          if (base in fields && fields[base].type === "map") {
            const map = values[base] ?? {};
            if (value === null) delete map[inner];
            else map[inner] = structuredClone(value);
            values[base] = map;
            continue;
          }
        }
        // Declared by no one: carry it, and say so.
        if (value === null) delete values[key];
        else values[key] = structuredClone(value);
        undeclared.add(key);
      }
    }
  }
  return { values, undeclared: [...undeclared] };
}

// ---------------------------------------------------------------------------
// Branching histories
// ---------------------------------------------------------------------------

/**
 * Give every entry an id and a parent, per spec/session.md: an entry's
 * own `id` if present, else a synthesized `line-<n>`; its `parent` if
 * present, else the previous entry (null for the first). A log with no
 * ids is therefore a chain in file order.
 */
function withIdentity(entries) {
  const byId = new Map();
  const ordered = [];
  let previous = null;
  for (let n = 0; n < entries.length; n++) {
    const raw = entries[n];
    const id = typeof raw.id === "string" ? raw.id : `line-${n}`;
    if (byId.has(id)) {
      throw new Error(`duplicate entry id '${id}'`);
    }
    const parent = typeof raw.parent === "string" ? raw.parent : previous;
    if (parent !== null && !byId.has(parent) && parent !== undefined) {
      throw new Error(`entry '${id}' names parent '${parent}', which isn't in the log`);
    }
    const entry = { ...raw, id, parent: parent ?? null };
    byId.set(id, entry);
    ordered.push(entry);
    previous = id;
  }
  return { ordered, byId };
}

/**
 * The history of a log: entries with identity, and its shape.
 *
 * @param {array} entries parsed log entries, in file order
 * @returns {{ordered: array, byId: Map, children: Map<string, string[]>, tips: string[]}}
 */
export function buildHistory(entries) {
  const { ordered, byId } = withIdentity(entries);
  const children = new Map(ordered.map((e) => [e.id, []]));
  for (const entry of ordered) {
    if (entry.parent !== null && children.has(entry.parent)) {
      children.get(entry.parent).push(entry.id);
    }
  }
  const tips = ordered.filter((e) => children.get(e.id).length === 0).map((e) => e.id);
  return { ordered, byId, children, tips };
}

/**
 * Fold one path of the history: the document at `tip` (default: the last
 * entry in file order), reached by walking parent links to the base and
 * folding that chain. A branch is just a different tip.
 *
 * @param {object} manifest the base world document
 * @param {array} entries parsed log entries, in file order
 * @param {string} [tip] an entry id from buildHistory
 * @returns {object} the fold state (as foldLog returns), plus `path` ids
 * @throws on an unknown tip, or the first entry that no longer applies
 */
export function foldPath(manifest, entries, tip) {
  const { ordered, byId } = withIdentity(entries);
  const last = ordered.length ? ordered[ordered.length - 1].id : null;
  const target = tip ?? last;
  if (!byId.has(target)) {
    throw new Error(`no entry '${target}' in this log`);
  }
  const chain = [];
  for (let id = target; id !== null; id = byId.get(id).parent) {
    chain.push(byId.get(id));
  }
  chain.reverse();
  const state = foldLog(manifest, chain);
  state.path = chain.map((e) => e.id);
  return state;
}

// ---------------------------------------------------------------------------
// Folding
// ---------------------------------------------------------------------------

/** @returns {string} why the fold refused, as an Error */
function invalid(message) {
  return new Error(`invalid: ${message}`);
}

/** Apply one edit op to a fold state, all-or-nothing. Throws on refusal. */
function applyEdit(state, edit, value) {
  switch (edit) {
    case "SpawnEntity": {
      const entity = value.entity;
      if (!entity || typeof entity.id !== "number" || typeof entity.name !== "string") {
        throw invalid("SpawnEntity needs an entity with id and name");
      }
      if (state.byId.has(entity.id)) throw invalid(`entity ${entity.id} already exists`);
      if (state.names.has(entity.name)) {
        throw invalid(`an entity named '${entity.name}' already exists`);
      }
      if (entity.parent !== undefined && entity.parent !== null && !state.byId.has(entity.parent)) {
        throw invalid(`entity ${entity.id}'s parent ${entity.parent} isn't in the document`);
      }
      state.entities.push(entity);
      state.byId.set(entity.id, entity);
      state.names.add(entity.name);
      return;
    }
    case "DeleteEntity": {
      const id = value.id;
      const entity = state.byId.get(id);
      if (!entity) throw invalid(`no entity ${id}`);
      // Descendants go with it: collect the subtree, then remove.
      const doomed = new Set([id]);
      let grew = true;
      while (grew) {
        grew = false;
        for (const e of state.entities) {
          if (e.parent !== undefined && e.parent !== null && doomed.has(e.parent) && !doomed.has(e.id)) {
            doomed.add(e.id);
            grew = true;
          }
        }
      }
      state.entities = state.entities.filter((e) => !doomed.has(e.id));
      for (const d of doomed) {
        const e = state.byId.get(d);
        state.byId.delete(d);
        if (e) state.names.delete(e.name);
      }
      return;
    }
    case "ModifyEntity": {
      const entity = state.byId.get(value.id);
      if (!entity) throw invalid(`no entity ${value.id}`);
      const patch = value.patch ?? {};
      // Absent = unchanged; null = clear; value = set (Option<Option<T>>).
      if ("name" in patch) {
        if (patch.name === null) throw invalid("an entity can't have no name");
        if (patch.name !== entity.name) {
          if (state.names.has(patch.name)) {
            throw invalid(`an entity named '${patch.name}' already exists`);
          }
          state.names.delete(entity.name);
          entity.name = patch.name;
          state.names.add(patch.name);
        }
      }
      if ("parent" in patch) {
        if (patch.parent !== null && !state.byId.has(patch.parent)) {
          throw invalid(`parent ${patch.parent} isn't in the document`);
        }
        // A parent cycle would make the entity its own ancestor.
        let ancestor = patch.parent;
        const seen = new Set([entity.id]);
        while (ancestor !== undefined && ancestor !== null) {
          if (seen.has(ancestor)) throw invalid(`entity ${entity.id} can't be its own ancestor`);
          seen.add(ancestor);
          ancestor = state.byId.get(ancestor)?.parent ?? null;
        }
        entity.parent = patch.parent;
      }
      for (const field of [
        "transform",
        "shape",
        "material",
        "light",
        "audio",
        "behaviors",
        "mesh_asset",
        "modulations",
        "instance_of",
        "triggers",
      ]) {
        if (field in patch) {
          if (patch[field] === null) delete entity[field];
          else entity[field] = patch[field];
        }
      }
      return;
    }
    case "SetEnvironment":
      state.environment = value.env ?? null;
      return;
    case "SetCamera":
      state.camera = value.camera ?? null;
      return;
    case "SetAmbience":
      state.ambience = value.ambience ?? [];
      return;
    case "SpawnAudioEmitter":
      if (typeof value.name !== "string") throw invalid("SpawnAudioEmitter needs a name");
      state.audioEmitters.set(value.name, value.audio ?? null);
      return;
    case "RemoveAudioEmitter":
      if (!state.audioEmitters.has(value.name)) {
        throw invalid(`no audio emitter named '${value.name}'`);
      }
      state.audioEmitters.delete(value.name);
      return;
    case "Batch": {
      const ops = value.ops ?? [];
      // All-or-nothing: apply to a deep copy, commit on success.
      const trial = freshTrial(state);
      for (const op of ops) {
        const c = classifyOp(op);
        if (c.kind !== "edit") continue; // history inside a batch folds to nothing too
        applyEdit(trial.state, c.edit, c.value);
      }
      commitTrial(state, trial);
      return;
    }
    default:
      throw invalid(`unknown edit ${edit}`);
  }
}

/** A deep-copied trial state, with its id/name maps rebuilt. */
function freshTrial(state) {
  const entities = structuredClone(state.entities);
  return {
    state: {
      entities,
      environment: structuredClone(state.environment),
      camera: structuredClone(state.camera),
      ambience: structuredClone(state.ambience),
      audioEmitters: new Map(structuredClone([...state.audioEmitters.entries()])),
      byId: new Map(entities.map((e) => [e.id, e])),
      names: new Set(entities.map((e) => e.name)),
    },
  };
}

/** Commit a trial's document fields and rebuilt maps onto the fold state. */
function commitTrial(state, trial) {
  state.entities = trial.state.entities;
  state.environment = trial.state.environment;
  state.camera = trial.state.camera;
  state.ambience = trial.state.ambience;
  state.audioEmitters = trial.state.audioEmitters;
  state.byId = trial.state.byId;
  state.names = trial.state.names;
}

/**
 * Fold log entries over a manifest: the document at the last entry.
 *
 * @param {object} manifest a parsed manifest (the base, at base_revision)
 * @param {array} entries parsed log entries, in order
 * @returns {{name: string, entities: array, environment, camera, ambience,
 *            audioEmitters: Map, appliedEdits: number}}
 * @throws at the first entry that no longer applies — the fold stops there,
 *   exactly as the specification's readers do.
 */
export function foldLog(manifest, entries) {
  const state = {
    name: manifest.meta?.name ?? "",
    entities: structuredClone(manifest.entities),
    environment: manifest.environment ?? null,
    camera: manifest.camera ?? null,
    ambience: manifest.ambience ?? [],
    audioEmitters: new Map(),
    appliedEdits: 0,
  };
  state.byId = new Map(state.entities.map((e) => [e.id, e]));
  state.names = new Set(state.entities.map((e) => e.name));

  for (const entry of entries) {
    const edits = editOps(entry);
    if (edits.length === 0) continue; // history folds to nothing
    const trial = freshTrial(state);
    for (const c of edits) {
      applyEdit(trial.state, c.edit, c.value);
    }
    commitTrial(state, trial);
    state.appliedEdits += edits.length;
  }
  return state;
}
