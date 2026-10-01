// The Open World Format reference fold.
//
// Pure JavaScript, no dependencies, no engine. It parses a world document
// (manifest.json) and folds a session log (ops.jsonl) over it, applying
// the same rules the specification states: only edits change the
// document, history kinds fold to nothing, ops are recognized by shape
// (edits first), and a batch applies all-or-nothing.
//
// The types below speak the schema's language (schema/world.schema.json
// `$defs`): JSDoc, checked and emitted as .d.ts by `npm run build:types`.
//
// Spec: https://openworldformat.org  ·  schema version 3

/** The manifest schema version this fold reads. */
export const SUPPORTED_SCHEMA_VERSION = 3;

/** The package format version this fold reads. */
export const SUPPORTED_FORMAT_VERSION = 1;

// ---------------------------------------------------------------------------
// Document types (the schema's $defs, as far as the fold reads them)
// ---------------------------------------------------------------------------

/** A 3-component vector, as the format serializes it: [x, y, z]. */
/** @typedef {[number, number, number]} Vec3 */

/** Transform in world space (or parent-relative if parented). */
/**
 * @typedef {Record<string, unknown> & {
 *   position?: Vec3, rotation_degrees?: Vec3, scale?: Vec3, visible?: boolean
 * }} WorldTransform
 */

/** A parametric primitive, externally tagged: {Sphere: {radius}}, {Cuboid: {x, y, z}}, ... */
/** @typedef {Record<string, Record<string, number>>} Shape */

/**
 * One entity: component slots are all optional — an entity is what it
 * declares. Any `ext-*` key rides along untouched (the extension fields
 * the fold carries, per must-ignore).
 * @typedef {Record<string, unknown> & {
 *   id: number, name: string, parent?: number|null,
 *   transform?: WorldTransform, chunk?: [number, number],
 *   shape?: Shape, material?: any, light?: any, audio?: any, mesh_asset?: any,
 *   behaviors?: any[], modulations?: any[], triggers?: any[],
 *   instance_of?: any, creation_id?: number
 * }} WorldEntity
 */

/** World metadata. */
/** @typedef {Record<string, unknown> & {name?: string}} WorldMeta */

/** Environment settings (background, ambient light, fog). */
/**
 * @typedef {Record<string, unknown> & {
 *   background_color?: Vec3, fog_color?: Vec3, fog_density?: number,
 *   ambient_color?: Vec3, ambient_intensity?: number
 * }} EnvironmentDef
 */

/** Camera definition. */
/**
 * @typedef {Record<string, unknown> & {
 *   position?: Vec3, look_at?: Vec3, fov_degrees?: number
 * }} CameraDef
 */

/** Top-level world manifest — everything needed to save/load a world. */
/**
 * @typedef {Record<string, unknown> & {
 *   version: number, meta?: WorldMeta, entities: WorldEntity[],
 *   environment?: EnvironmentDef|null, camera?: CameraDef|null, ambience?: any[],
 *   creations?: any[], next_entity_id?: number,
 *   avatar?: any, soundtrack?: any, tours?: any[]
 * }} WorldManifest
 */

/** One parsed ops.jsonl line: an entry, its ops classified on parse. */
/**
 * @typedef {object} LogEntry
 * @property {number} revision
 * @property {unknown} [author]
 * @property {number} [timestamp_ms]
 * @property {any[]} ops
 * @property {string} [id]
 * @property {string|null} [parent]
 * @property {ClassifiedOp[]} [classified]
 */

/** One op, recognized by shape — the compatibility rule, executable. */
/**
 * @typedef {{kind: "edit", edit: string, value: any}} ClassifiedEdit
 * @typedef {{kind: "tool"|"input"|"state"|"clock"|"merge", value: any}} ClassifiedHistory
 * @typedef {{kind: "extension", name: string, value: any}} ClassifiedExtension
 * @typedef {{kind: "unknown"}} ClassifiedUnknown
 * @typedef {ClassifiedEdit|ClassifiedHistory|ClassifiedExtension|ClassifiedUnknown} ClassifiedOp
 */

/** An entry with identity resolved: an id and a parent, always. */
/** @typedef {LogEntry & {id: string, parent: string|null}} IdentifiedEntry */

/**
 * The document at a point in the log — what foldLog and foldPath return.
 * `byId` and `names` are the fold's internal indexes (rebuilt as it goes);
 * `path` is set by foldPath only: the entry ids that were folded.
 *
 * @typedef {object} FoldState
 * @property {string} name the world's name (manifest meta)
 * @property {WorldEntity[]} entities base plus every applied edit
 * @property {EnvironmentDef|null} environment
 * @property {CameraDef|null} camera
 * @property {any[]} ambience
 * @property {Map<string, any>} audioEmitters
 * @property {number} appliedEdits
 * @property {Map<number, WorldEntity>} byId
 * @property {Set<string>} names
 * @property {string[]} [path]
 */

/** A declared state field (schema/state.schema.json). */
/**
 * @typedef {object} StateField
 * @property {"int"|"float"|"bool"|"string"|"map"|"list"|"json"} type
 * @property {any} [initial]
 */

/** The typed state document (state.json). */
/**
 * @typedef {object} StateDocument
 * @property {number} format_version
 * @property {Record<string, StateField>} fields
 */

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/**
 * Parse and sanity-check a world document.
 * @param {string} json the manifest's JSON text
 * @returns {WorldManifest} the manifest
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
 * edit serializes today exactly as it always did. Extension ops
 * (`ext-*`, single key, object value) are recognized as a kind of their
 * own and, like every history kind, fold to nothing for the document.
 * @param {Record<string, any>} op
 * @returns {ClassifiedOp}
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
  if (keys.length === 1 && /^ext-[a-z0-9-]+$/.test(keys[0]) &&
      op[keys[0]] !== null && typeof op[keys[0]] === "object") {
    return { kind: "extension", name: keys[0], value: op[keys[0]] };
  }
  return { kind: "unknown" };
}

/**
 * Parse one log line into an entry with classified ops. Unreadable lines
 * are the writer's crash, not the reader's — the caller decides whether
 * to skip (the spec says skip the last one, count the rest).
 * @param {string} line one line of ops.jsonl
 * @returns {LogEntry}
 */
export function parseLogLine(line) {
  const entry = JSON.parse(line);
  if (typeof entry.revision !== "number" || !Array.isArray(entry.ops)) {
    throw new Error("log entry needs a revision and an ops array");
  }
  return { ...entry, classified: entry.ops.map(classifyOp) };
}

/**
 * @param {ClassifiedOp} c
 * @returns {c is ClassifiedEdit}
 */
const isEdit = (c) => c.kind === "edit";

/** An entry's edits, in order — the ops that change the document.
 * @param {LogEntry} entry
 * @returns {ClassifiedEdit[]} */
export function editOps(entry) {
  return entry.classified === undefined
    ? entry.ops.map(classifyOp).filter(isEdit)
    : entry.classified.filter(isEdit);
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
 * @param {StateDocument} stateDoc parsed `state.json` ({format_version, fields})
 * @param {LogEntry[]} entries parsed log entries, in order
 * @returns {{values: Record<string, any>, undeclared: string[]}}
 */
export function foldState(stateDoc, entries) {
  const fields = stateDoc?.fields ?? {};
  const values = /** @type {Record<string, any>} */ ({});
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
 * @param {LogEntry[]} entries
 * @returns {{ordered: IdentifiedEntry[], byId: Map<string, IdentifiedEntry>}}
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
 * @param {LogEntry[]} entries parsed log entries, in file order
 * @returns {{ordered: IdentifiedEntry[], byId: Map<string, IdentifiedEntry>,
 *            children: Map<string, string[]>, tips: string[]}}
 */
export function buildHistory(entries) {
  const { ordered, byId } = withIdentity(entries);
  const children = new Map(ordered.map((e) => [e.id, []]));
  for (const entry of ordered) {
    if (entry.parent !== null && children.has(entry.parent)) {
      /** @type {string[]} */ (children.get(entry.parent)).push(entry.id);
    }
  }
  const tips = ordered.filter((e) => /** @type {string[]} */ (children.get(e.id)).length === 0).map((e) => e.id);
  return { ordered, byId, children, tips };
}

/**
 * Fold one path of the history: the document at `tip` (default: the last
 * entry in file order), reached by walking parent links to the base and
 * folding that chain. A branch is just a different tip.
 *
 * @param {WorldManifest} manifest the base world document
 * @param {LogEntry[]} entries parsed log entries, in file order
 * @param {string} [tip] an entry id from buildHistory
 * @returns {FoldState} the fold state (as foldLog returns), plus `path` ids
 * @throws on an unknown tip, or the first entry that no longer applies
 */
export function foldPath(manifest, entries, tip) {
  const { ordered, byId } = withIdentity(entries);
  const last = ordered.length ? ordered[ordered.length - 1].id : null;
  const target = tip ?? last;
  if (target === null || !byId.has(target)) {
    throw new Error(`no entry '${target}' in this log`);
  }
  const chain = [];
  for (let id = /** @type {string|null} */ (target); id !== null; id = /** @type {IdentifiedEntry} */ (byId.get(id)).parent) {
    chain.push(/** @type {IdentifiedEntry} */ (byId.get(id)));
  }
  chain.reverse();
  const state = foldLog(manifest, chain);
  state.path = chain.map((e) => e.id);
  return state;
}

// ---------------------------------------------------------------------------
// Folding
// ---------------------------------------------------------------------------

/** @param {string} message @returns {Error} why the fold refused */
function invalid(message) {
  return new Error(`invalid: ${message}`);
}

/** Apply one edit op to a fold state, all-or-nothing. Throws on refusal.
 * @param {FoldState} state
 * @param {string} edit
 * @param {any} value */
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
      // Extension fields ride along: any `ext-*` key patches like the
      // known ones — set, or clear on null — so a physics component (or
      // any future extension's) survives a modify round-trip. The core
      // schema leaves room for them; must-ignore is the reader's side.
      for (const field of Object.keys(patch)) {
        if (field.startsWith("ext-")) {
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

/** A deep-copied trial state, with its id/name maps rebuilt.
 * @param {FoldState} state
 * @returns {{state: FoldState}} */
function freshTrial(state) {
  const entities = structuredClone(state.entities);
  return {
    state: {
      // name and appliedEdits ride along for the shape; commit ignores them.
      name: state.name,
      appliedEdits: state.appliedEdits,
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

/** Commit a trial's document fields and rebuilt maps onto the fold state.
 * @param {FoldState} state
 * @param {{state: FoldState}} trial */
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
 * @param {WorldManifest} manifest a parsed manifest (the base, at base_revision)
 * @param {LogEntry[]} entries parsed log entries, in order
 * @returns {FoldState}
 * @throws at the first entry that no longer applies — the fold stops there,
 *   exactly as the specification's readers do.
 */
export function foldLog(manifest, entries) {
  const state = /** @type {FoldState} */ ({
    name: manifest.meta?.name ?? "",
    entities: structuredClone(manifest.entities),
    environment: manifest.environment ?? null,
    camera: manifest.camera ?? null,
    ambience: manifest.ambience ?? [],
    audioEmitters: new Map(),
    appliedEdits: 0,
    byId: new Map(),
    names: new Set(),
  });
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
