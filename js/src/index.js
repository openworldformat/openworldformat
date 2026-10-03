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

// This file is the Node-side fold: the package requires Node ≥ 20, and
// entry identity needs exactly one Node API — node:crypto's sha256.
// render.js is the browser-facing entry and pulls nothing from here at
// runtime (its imports of these types are JSDoc-only, erased at build).
import { createHash } from "node:crypto";

/** The manifest schema version this fold reads. */
export const SUPPORTED_SCHEMA_VERSION = 3;

/** The package format version this fold reads. */
export const SUPPORTED_FORMAT_VERSION = 1;

/**
 * The entity id ceiling: 2^53 − 1, the largest integer every IEEE-754
 * double holds exactly — ids MUST stay at or below it so an id never
 * differs between languages (or between writer and reader).
 */
export const MAX_ENTITY_ID = 9007199254740991;

/**
 * The extensions the registry (spec/extensions/registry.json) has
 * accepted: namespaced, each with a spec page, a reference
 * implementation and conformance cases. Strict mode admits `ext-*` keys
 * on these names and no others; the runtime folds any `ext-*` it knows
 * or not — must-ignore (spec/profiles.md).
 */
export const REGISTERED_EXTENSIONS = [
  "ext-physics",
  "ext-strict-determinism",
  "ext-visibility",
  "ext-cinematography",
  "ext-provenance",
];

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

/** World metadata. Lineage (prompt, model, biome…) is not core: it rides
 *  in `meta["ext-provenance"]` — spec/extensions/provenance.md. */
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
 * @property {Map<string, number>} nameToId names → ids: the ingestion-time binding index
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

/** Any JSON value — what canonicalJson walks. */
/**
 * @typedef {null|boolean|number|string|any[]|Record<string, any>} JsonValue
 */

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/**
 * Parse and sanity-check a world document.
 *
 * Non-strict (the default, and the runtime rule — spec/profiles.md
 * "the must-ignore rule"): unknown keys ride along; the fold carries
 * what it doesn't know.
 *
 * Strict (`{strict: true}`, or a plain `true` — for authoring tools
 * and validators, spec/profiles.md "Strict Mode for Authoring"): keys
 * outside the core schema and outside registered `ext-*` extensions
 * throw instead. The manifest admits `version, meta, entities,
 * environment, camera, avatar, tours, soundtrack, creations,
 * next_entity_id` (the v2 multi-file keys `layout_file/region_files/
 * behavior_files/audio_files/avatar_file` are gone — v3 is one file);
 * `meta` admits its nine core keys (the legacy lineage keys moved to
 * `meta["ext-provenance"]`); entities admit their sixteen.
 *
 * @param {string} json the manifest's JSON text
 * @param {{strict?: boolean}|boolean} [opts] strict mode, off by default
 * @returns {WorldManifest} the manifest
 * @throws when the text isn't JSON, the schema version isn't supported,
 *   or (strict) a key is neither core nor a registered extension
 */
export function parseManifest(json, opts) {
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
  if (strictMode(opts)) {
    checkStrictKeys("manifest", manifest, MANIFEST_KEYS);
    if (manifest.meta && typeof manifest.meta === "object") {
      checkStrictKeys("meta", manifest.meta, META_KEYS);
    }
    for (const entity of manifest.entities) {
      if (entity && typeof entity === "object") {
        checkStrictKeys("entity", entity, ENTITY_KEYS);
      }
    }
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

/** The history kinds, by shape: each serialized as its own lowercase field. */
const HISTORY_KINDS = new Set(["tool", "input", "state", "clock", "merge"]);

/**
 * The shape collision rule as a predicate (spec/session.md
 * "Compatibility"): edits MUST be PascalCase, history kinds MUST be
 * lowercase — the two namespaces can never collide, and a kind yet to
 * be named is recognizable as one or the other by its case alone. The
 * serializer's guard; classification itself is unchanged (edits first,
 * unknown stays unknown).
 * @param {string} kind
 * @returns {boolean}
 */
export function opKindShapeOk(kind) {
  return (EDIT_KEYS.has(kind) && /^[A-Z]/.test(kind)) ||
    (HISTORY_KINDS.has(kind) && /^[a-z]/.test(kind));
}

// Strict mode's key sets (spec/profiles.md): the core schema's own
// keys per scope, with registered `ext-*` admitted alongside.
const MANIFEST_KEYS = new Set([
  "version", "meta", "entities", "environment", "camera", "avatar",
  "tours", "soundtrack", "creations", "next_entity_id",
]);
const META_KEYS = new Set([
  "name", "description", "time_of_day", "tags", "source",
  "variation_group", "variation", "style_ref", "compliance",
]);
const ENTITY_KEYS = new Set([
  "id", "name", "parent", "transform", "chunk", "shape", "material",
  "light", "audio", "behaviors", "modulations", "triggers", "mesh_asset",
  "instance_of", "creation_id",
]);
/** The v2 multi-file layout: gone in v3, one manifest since. */
const V2_FILE_KEYS = new Set([
  "layout_file", "region_files", "behavior_files", "audio_files", "avatar_file",
]);
/** Lineage keys that moved out of core meta into the provenance extension. */
const LEGACY_META_KEYS = new Set([
  "prompt", "model", "generation_duration_ms", "biome", "semantic_category",
]);

/** Strict mode from an options object or a plain boolean — the old
 *  signature still works.
 *  @param {{strict?: boolean}|boolean} [opts]
 *  @returns {boolean} */
function strictMode(opts) {
  return typeof opts === "boolean" ? opts : opts?.strict === true;
}

/** One strict key check: allowed, or a registered `ext-*`, or a refusal
 *  that names the key and says where it went (if anywhere).
 *  @param {string} scope "manifest" | "meta" | "entity"
 *  @param {Record<string, any>} object
 *  @param {Set<string>} allowed */
function checkStrictKeys(scope, object, allowed) {
  for (const key of Object.keys(object)) {
    if (allowed.has(key)) continue;
    if (key.startsWith("ext-")) {
      if (!REGISTERED_EXTENSIONS.includes(key)) {
        throw new Error(
          `${scope} key '${key}' is an unregistered extension — ` +
            "the registry is spec/extensions/registry.json",
        );
      }
      continue;
    }
    if (V2_FILE_KEYS.has(key)) {
      throw new Error(
        `${scope} key '${key}' is the v2 multi-file layout, gone in schema v3 — fold it into the manifest`,
      );
    }
    if (scope === "meta" && LEGACY_META_KEYS.has(key)) {
      throw new Error(
        `meta.${key} is lineage, not core metadata — it lives in meta["ext-provenance"] (spec/extensions/provenance.md)`,
      );
    }
    throw new Error(
      `${scope} key '${key}' is not in the schema — strict mode refuses it (must-ignore is the runtime rule)`,
    );
  }
}

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
 * to skip (the spec says skip the last one, count the rest). Strict mode
 * (spec/profiles.md) refuses ops no shape recognizes and extension ops
 * the registry has no entry for; non-strict classifies them as `unknown`
 * and `extension` exactly as ever.
 * @param {string} line one line of ops.jsonl
 * @param {{strict?: boolean}|boolean} [opts] strict mode, off by default
 * @returns {LogEntry}
 */
export function parseLogLine(line, opts) {
  const entry = JSON.parse(line);
  if (typeof entry.revision !== "number" || !Array.isArray(entry.ops)) {
    throw new Error("log entry needs a revision and an ops array");
  }
  const classified = entry.ops.map(classifyOp);
  if (strictMode(opts)) {
    for (const c of classified) {
      if (c.kind === "unknown") {
        throw new Error(
          "strict mode refuses an op no shape recognizes — must-ignore is the runtime rule",
        );
      }
      if (c.kind === "extension" && !REGISTERED_EXTENSIONS.includes(c.name)) {
        throw new Error(
          `op '${c.name}' is an unregistered extension — ` +
            "the registry is spec/extensions/registry.json",
        );
      }
    }
  }
  return { ...entry, classified };
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
// The provenance extension (spec/extensions/provenance.md)
// ---------------------------------------------------------------------------

/** The lineage fields `ext-provenance` carries — none of them core meta. */
export const EXT_PROVENANCE_FIELDS = [
  "prompt",
  "model",
  "generation_duration_ms",
  "biome",
  "semantic_category",
];

/** Lineage metadata as the extension carries it: all five fields optional.
 *  @typedef {object} ExtProvenance
 *  @property {string} [prompt] the text that generated the content
 *  @property {string} [model] the generative model's identifier
 *  @property {number} [generation_duration_ms]
 *  @property {string} [biome] a procedural generation hint
 *  @property {string} [semantic_category] a tag for reasoning and search
 */

/**
 * Read `manifest.meta["ext-provenance"]` into a plain object — the five
 * lineage fields it holds, nothing else — or null when the manifest
 * carries no provenance. The prompt is private by nature: tools
 * circulating worlds MUST warn or scrub it before publishing.
 * @param {WorldManifest} manifest
 * @returns {ExtProvenance|null}
 */
export function extProvenance(manifest) {
  const raw = /** @type {Record<string, any>|null|undefined} */ (manifest?.meta?.["ext-provenance"]);
  if (raw === null || raw === undefined || typeof raw !== "object") return null;
  /** @type {Record<string, any>} */
  const out = {};
  for (const field of EXT_PROVENANCE_FIELDS) {
    if (raw[field] !== undefined) out[field] = raw[field];
  }
  return out;
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
// Entry identity (spec/session.md "Entry identity, forks and branches")
// ---------------------------------------------------------------------------

/** Less-than by code point, not by UTF-16 unit: astral characters would
 *  otherwise sort before U+E000..U+FFFF, and canonical key order is a
 *  cross-language contract (Rust's BTreeMap sorts UTF-8 bytes, Python's
 *  sorted() sorts code points — both agree with this).
 *  @param {string} a @param {string} b @returns {number} */
function compareByCodePoint(a, b) {
  const as = [...a], bs = [...b]; // spread splits by code point
  const n = Math.min(as.length, bs.length);
  for (let k = 0; k < n; k++) {
    const ca = as[k].codePointAt(0) ?? 0, cb = bs[k].codePointAt(0) ?? 0;
    if (ca !== cb) return ca < cb ? -1 : 1;
  }
  return as.length - bs.length;
}

/**
 * Canonical JSON — deterministic serialization: no whitespace, object
 * keys sorted recursively by code point, arrays in order, strings
 * escaped the way JSON escapes them, integers printed as integers.
 * This is the buffer entry hashes run over, so identical entries hash
 * identically across forks — and, for integer-valued JSON, across
 * languages too: float formatting is each language's own (JS prints
 * 1.0 as "1"), so cross-language hash equality holds exactly when the
 * values are integers. Hashing runs through node:crypto (above); the
 * browser-facing render.js never calls this.
 * @param {JsonValue} value
 * @returns {string}
 */
export function canonicalJson(value) {
  if (Array.isArray(value)) {
    return `[${value.map((v) => canonicalJson(v === undefined ? null : v)).join(",")}]`;
  }
  if (value !== null && typeof value === "object") {
    const keys = Object.keys(value)
      .filter((k) => value[k] !== undefined) // absent, as JSON.stringify drops it
      .sort(compareByCodePoint);
    return `{${keys.map((k) => `${JSON.stringify(k)}:${canonicalJson(value[k])}`).join(",")}}`;
  }
  return value === undefined ? "null" : JSON.stringify(value);
}

/**
 * An entry's content id: SHA-256 over the canonical JSON of the entry
 * with its own `id` removed — identity is what the entry says, never
 * what it is called — returned as `sha256:<hex>`. Two forks writing the
 * same entry compute the same id, which is what makes refs and merges
 * survive divergence.
 * @param {LogEntry} entry
 * @returns {string}
 */
export function computeEntryId(entry) {
  const copy = structuredClone(entry);
  delete copy.id;
  // `classified` is this reader's annotation of the ops, not content —
  // parseLogLine adds it, and the hash must not see it, or the same
  // line hashed before and after parsing would name two entries.
  delete copy.classified;
  return `sha256:${createHash("sha256").update(canonicalJson(copy), "utf8").digest("hex")}`;
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

/** The subtree an id drags with it: the entity and every descendant —
 *  deleting an entity deletes its descendants (spec/session.md).
 *  @param {FoldState} state
 *  @param {number} id
 *  @returns {Set<number>} */
function subtreeOf(state, id) {
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
  return doomed;
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
      if (entity.id > MAX_ENTITY_ID) {
        throw invalid(`entity ${entity.id} exceeds the id ceiling ${MAX_ENTITY_ID} (2^53-1)`);
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
      state.nameToId.set(entity.name, entity.id);
      return;
    }
    case "DeleteEntity": {
      const id = value.id;
      const entity = state.byId.get(id);
      if (!entity) throw invalid(`no entity ${id}`);
      // Descendants go with it: collect the subtree, then remove.
      const doomed = subtreeOf(state, id);
      state.entities = state.entities.filter((e) => !doomed.has(e.id));
      for (const d of doomed) {
        const e = state.byId.get(d);
        state.byId.delete(d);
        if (e) {
          state.names.delete(e.name);
          state.nameToId.delete(e.name);
        }
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
          state.nameToId.delete(entity.name);
          entity.name = patch.name;
          state.names.add(patch.name);
          state.nameToId.set(patch.name, entity.id);
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
      nameToId: new Map(entities.map((e) => [e.name, e.id])),
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
  state.nameToId = trial.state.nameToId;
}

/** @param {FoldState} state @param {string} name @returns {number} */
function requireNamed(state, name) {
  const id = state.nameToId.get(name);
  if (id === undefined) throw invalid(`no entity named '${name}'`);
  return id;
}

/**
 * Resolve an entity's by-name behavior references to ids, in place —
 * spec/world.md "Identity": cross-entity references may be written by
 * name (what authors and models produce) and MUST be resolved to id at
 * ingestion against the fold-so-far; saved worlds always contain ids.
 * Delayed resolution is strictly forbidden: a rename would otherwise
 * re-bind a logged ref and break log determinism. `modulations[]`
 * `.target` is a property name (emissive, scale…), never an entity
 * ref — untouched.
 * @param {FoldState} state the fold the names resolve against
 * @param {WorldEntity} entity mutated: string refs become ids
 */
function resolveNames(state, entity) {
  if (!Array.isArray(entity.behaviors)) return;
  for (const behavior of entity.behaviors) {
    if (behavior === null || typeof behavior !== "object") continue;
    const keys = Object.keys(behavior);
    if (keys.length !== 1) continue; // externally tagged: one kind per object
    const def = behavior[keys[0]];
    if (def === null || typeof def !== "object") continue;
    if (keys[0] === "Orbit" && typeof def.center === "string") {
      def.center = requireNamed(state, def.center);
    } else if (keys[0] === "LookAt" && typeof def.target === "string") {
      def.target = requireNamed(state, def.target);
    }
  }
}

/** The entity ids an entry's edit ops touch — spawn and modify targets,
 *  recursing into batches — the entities whose by-name refs bind at
 *  this entry.
 *  @param {ClassifiedEdit[]} edits
 *  @returns {number[]} */
function touchedIds(edits) {
  /** @type {number[]} */ const ids = [];
  /** @param {ClassifiedEdit[]} list */
  const walk = (list) => {
    for (const c of list) {
      if (c.edit === "Batch") {
        walk((c.value.ops ?? []).map(classifyOp).filter(isEdit));
      } else if (c.edit === "SpawnEntity") {
        if (typeof c.value?.entity?.id === "number") ids.push(c.value.entity.id);
      } else if (c.edit === "ModifyEntity") {
        ids.push(c.value.id);
      }
    }
  };
  walk(edits);
  return ids;
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
    nameToId: new Map(),
  });
  state.byId = new Map(state.entities.map((e) => [e.id, e]));
  state.names = new Set(state.entities.map((e) => e.name));
  state.nameToId = new Map(state.entities.map((e) => [e.name, e.id]));
  // Name binding at ingestion starts at the base: a manifest's refs
  // resolve against the complete base, every name it holds.
  for (const entity of state.entities) resolveNames(state, entity);

  for (const entry of entries) {
    const edits = editOps(entry);
    if (edits.length === 0) continue; // history folds to nothing
    const trial = freshTrial(state);
    for (const c of edits) {
      applyEdit(trial.state, c.edit, c.value);
    }
    // …and continues per entry, inside the trial, after all its edits
    // apply and before it commits: an entry is atomic, so refs it writes
    // resolve against the fold-so-far including the entry's own spawns.
    // A name nothing owns at ingestion fails the entry — the fold stops
    // there, the same refusal class as "no longer applies".
    for (const id of touchedIds(edits)) {
      const entity = trial.state.byId.get(id);
      if (entity) resolveNames(trial.state, entity);
    }
    commitTrial(state, trial);
    state.appliedEdits += edits.length;
  }
  return state;
}

// ---------------------------------------------------------------------------
// Inverses (undo is appending the inverse — spec/session.md)
// ---------------------------------------------------------------------------

/**
 * The inverse of one edit op against a fold state: every edit has a
 * computable inverse, and undo is appending it — the log never rewinds.
 * Computed at the time of the edit, against the state the edit is about
 * to apply to:
 *
 * - `SpawnEntity` inverses to the `DeleteEntity` of what it spawns.
 * - `DeleteEntity` inverses to a `Batch` of `SpawnEntity` ops holding
 *   deep copies of the deleted tree, parents first, so re-spawning
 *   applies.
 * - `ModifyEntity` inverses to a `ModifyEntity` restoring the old
 *   values: every patched key maps to the entity's current value, or
 *   `null` when it doesn't have the field (name and parent included —
 *   the inverse name is the current name, the inverse parent the
 *   current parent or null).
 * - The scene-wide sets inverse to the state they replace — or the
 *   format's defaults when the document never set one.
 * - Audio emitters spawn and remove into each other.
 * - A `Batch` inverses to its ops' inverses in reverse order, each
 *   computed against a running trial copy of the state (value
 *   semantics: clone, inverse op i against the running clone, then
 *   apply op i to the clone) so a later op inverses against what the
 *   earlier ones left behind.
 *
 * @param {Record<string, any>} op the raw op, `{"Kind": {...}}` as serialized
 * @param {FoldState} state the fold the op is about to apply to
 * @returns {Record<string, any>} the inverse op, the same `{"Kind": {...}}` shape
 * @throws on a non-edit op, an unknown kind, or a missing entity
 */
export function computeInverse(op, state) {
  const c = classifyOp(op);
  if (c.kind !== "edit") {
    throw invalid(`no inverse for an op that isn't an edit (${c.kind})`);
  }
  switch (c.edit) {
    case "SpawnEntity": {
      const entity = c.value.entity;
      if (!entity || typeof entity.id !== "number") {
        throw invalid("SpawnEntity needs an entity with id and name");
      }
      return { DeleteEntity: { id: entity.id } };
    }
    case "DeleteEntity": {
      if (!state.byId.has(c.value.id)) throw invalid(`no entity ${c.value.id}`);
      const doomed = subtreeOf(state, c.value.id);
      /** How deep an entity sits: parents sort before children. */
      const depth = (/** @type {WorldEntity} */ e) => {
        let d = 0;
        let p = e.parent;
        while (p !== undefined && p !== null) {
          d += 1;
          p = state.byId.get(p)?.parent ?? null;
        }
        return d;
      };
      const ops = state.entities
        .filter((e) => doomed.has(e.id))
        .sort((a, b) => depth(a) - depth(b)) // stable: document order within a depth
        .map((e) => ({ SpawnEntity: { entity: structuredClone(e) } }));
      return { Batch: { ops } };
    }
    case "ModifyEntity": {
      const entity = state.byId.get(c.value.id);
      if (!entity) throw invalid(`no entity ${c.value.id}`);
      const patch = c.value.patch ?? {};
      /** @type {Record<string, any>} */
      const inverse = {};
      for (const field of Object.keys(patch)) {
        if (field === "name") {
          inverse.name = entity.name; // an entity always has one
        } else if (field === "parent") {
          inverse.parent = entity.parent ?? null;
        } else if (field in entity) {
          inverse[field] = structuredClone(entity[field]);
        } else {
          inverse[field] = null; // it didn't have the field: the inverse clears
        }
      }
      return { ModifyEntity: { id: c.value.id, patch: inverse } };
    }
    case "SetEnvironment":
      return { SetEnvironment: { env: structuredClone(state.environment ?? {}) } };
    case "SetCamera":
      // The format's defaults (spec/world.md's camera): where the camera
      // starts when the document never set one.
      return {
        SetCamera: {
          camera: structuredClone(
            state.camera ?? { position: [5, 5, 5], look_at: [0, 0, 0], fov_degrees: 45 },
          ),
        },
      };
    case "SetAmbience":
      return { SetAmbience: { ambience: structuredClone(state.ambience ?? []) } };
    case "SpawnAudioEmitter":
      return {
        RemoveAudioEmitter: { name: c.value.name, audio: structuredClone(c.value.audio ?? null) },
      };
    case "RemoveAudioEmitter": {
      if (!state.audioEmitters.has(c.value.name)) {
        throw invalid(`no audio emitter named '${c.value.name}'`);
      }
      return {
        SpawnAudioEmitter: {
          name: c.value.name,
          audio: structuredClone(state.audioEmitters.get(c.value.name) ?? null),
        },
      };
    }
    case "Batch": {
      const ops = c.value.ops ?? [];
      /** @type {Record<string, any>[]} */
      const inverses = [];
      const running = freshTrial(state); // value semantics: run on a copy
      for (const op of ops) {
        const ci = classifyOp(op);
        if (ci.kind !== "edit") continue; // history inside a batch folds to nothing too
        // Walk forward — inverse of op i against what the earlier ops
        // left, then op i applies — and emit the inverses reversed: the
        // last op's inverse undoes first.
        inverses.push(computeInverse(op, running.state));
        applyEdit(running.state, ci.edit, ci.value);
      }
      inverses.reverse();
      return { Batch: { ops: inverses } };
    }
    default:
      throw invalid(`unknown edit ${c.edit}`);
  }
}

// ---------------------------------------------------------------------------
// Merging branches (spec/session.md "Entry identity, forks and branches")
// ---------------------------------------------------------------------------

/** Collect the ids a list of ops' SpawnEntity ops spawn, recursing batches.
 *  @param {any[]} ops @param {Set<number>} into */
function collectSpawned(ops, into) {
  for (const op of ops) {
    const c = classifyOp(op);
    if (c.kind !== "edit") continue;
    if (c.edit === "SpawnEntity") {
      if (typeof c.value?.entity?.id === "number") into.add(c.value.entity.id);
    } else if (c.edit === "Batch") {
      collectSpawned(c.value.ops ?? [], into);
    }
  }
}

/** Remap numeric behavior refs (`Orbit.center`, `LookAt.target`) inside
 *  a behaviors array, in place on the copy being built. String refs are
 *  names, not ids — left alone.
 *  @param {any} behaviors
 *  @param {Map<number, number>} remapped */
function remapBehaviorRefs(behaviors, remapped) {
  if (!Array.isArray(behaviors)) return;
  for (const behavior of behaviors) {
    if (behavior === null || typeof behavior !== "object") continue;
    const [kind] = Object.keys(behavior);
    const def = behavior[kind];
    if (def === null || typeof def !== "object") continue;
    if (kind === "Orbit" && typeof def.center === "number" && remapped.has(def.center)) {
      def.center = remapped.get(def.center);
    } else if (kind === "LookAt" && typeof def.target === "number" && remapped.has(def.target)) {
      def.target = remapped.get(def.target);
    }
  }
}

/** One op rewritten through an id remap: new objects throughout, the
 *  inputs never mutated. History and extension ops deep-copy unchanged.
 *  @param {Record<string, any>} op
 *  @param {Map<number, number>} remapped
 *  @returns {Record<string, any>} */
function remapOp(op, remapped) {
  if (remapped.size === 0) return structuredClone(op);
  const c = classifyOp(op);
  if (c.kind !== "edit") return structuredClone(op);
  /** @param {number} id */
  const remap = (id) => (remapped.has(id) ? remapped.get(id) : id);
  switch (c.edit) {
    case "SpawnEntity": {
      const entity = structuredClone(c.value.entity);
      entity.id = remap(entity.id);
      if (entity.parent !== undefined && entity.parent !== null) entity.parent = remap(entity.parent);
      remapBehaviorRefs(entity.behaviors, remapped);
      return { SpawnEntity: { entity } };
    }
    case "ModifyEntity": {
      const value = structuredClone(c.value);
      value.id = remap(value.id);
      if (value.patch && value.patch.parent !== undefined && value.patch.parent !== null) {
        value.patch.parent = remap(value.patch.parent);
      }
      remapBehaviorRefs(value.patch?.behaviors, remapped);
      return { ModifyEntity: value };
    }
    case "DeleteEntity": {
      const value = structuredClone(c.value);
      value.id = remap(value.id);
      return { DeleteEntity: value };
    }
    case "Batch":
      return { Batch: { ops: (c.value.ops ?? []).map((/** @type {any} */ o) => remapOp(o, remapped)) } };
    default:
      return structuredClone(op); // the scene-wide edits carry no entity ids
  }
}

/**
 * Merge a branch onto a head — the pass the merge authority runs
 * (spec/session.md): ids the branch spawned that the main branch
 * concurrently allocated MUST be reallocated and every reference to
 * them inside the merged batch rewritten. This scans the branch for
 * spawned ids, remaps the colliding ones onto fresh ids past the main
 * head (skipping what either branch already uses, never past the id
 * ceiling), and rewrites the entries through that map — new deep
 * copies, the inputs untouched: `SpawnEntity.entity.id` and `.parent`,
 * `ModifyEntity.id` and `patch.parent`, `DeleteEntity.id`, and the
 * numeric behavior refs (`Orbit.center`, `LookAt.target`). History ops
 * ride untouched.
 *
 * Name collisions are NOT remapped — the merge contract scopes to ids —
 * so merged entries still refuse on apply when both branches spawned
 * the same name; the caller pre-renames.
 *
 * @param {FoldState} state the main branch's fold at its head
 * @param {LogEntry[]} entries the branch's parsed log entries, to append
 * @returns {{entries: LogEntry[], remapped: Map<number, number>}} the
 *   rewritten entries, and the oldId→newId map they were rewritten through
 * @throws when the id space is exhausted up to the ceiling
 */
export function mergeBranch(state, entries) {
  /** @type {Set<number>} */
  const spawned = new Set();
  for (const entry of entries) collectSpawned(entry.ops ?? [], spawned);

  // Fresh ids for the collisions: past the main head, skipping what the
  // main branch and the branch already use — and never past the ceiling.
  /** @type {Map<number, number>} */
  const remapped = new Map();
  let next = state.byId.size ? Math.max(...state.byId.keys()) + 1 : 1;
  for (const id of spawned) {
    if (!state.byId.has(id)) continue; // no collision, no remap
    while (state.byId.has(next) || spawned.has(next)) next += 1;
    if (next > MAX_ENTITY_ID) {
      throw invalid(`merge ran out of entity ids below the ceiling ${MAX_ENTITY_ID} (2^53-1)`);
    }
    remapped.set(id, next);
    next += 1;
  }

  const rewritten = entries.map((entry) => {
    const ops = (entry.ops ?? []).map((/** @type {any} */ op) => remapOp(op, remapped));
    const out = structuredClone({ ...entry });
    out.ops = ops;
    out.classified = ops.map(classifyOp);
    return out;
  });
  return { entries: rewritten, remapped };
}

// ---------------------------------------------------------------------------
// Packages (spec/package.md)
// ---------------------------------------------------------------------------

/**
 * Where a snapshot lives (spec/session.md "Snapshots"): derived, never
 * authoritative, deletable without loss. Entry ids are opaque strings —
 * a `sha256:…` has a colon no filesystem wants — so every character
 * outside `[A-Za-z0-9._-]` becomes `_` and the name stays inside
 * `snapshots/`. Linear logs with no entry ids keyframe by revision.
 * @param {string|null} entryId the entry the snapshot folds to, when ids exist
 * @param {number} revision the revision, for linear logs
 * @returns {string}
 */
export function snapshotFilename(entryId, revision) {
  if (typeof entryId === "string" && entryId !== "") {
    const sanitized = entryId.replace(/[^A-Za-z0-9._-]/g, "_");
    return `snapshots/entry-${sanitized}.json`;
  }
  return `snapshots/rev-${revision}.json`;
}

/**
 * The `package.json` half of compaction (spec/session.md "Snapshots"):
 * a producer writing a new base manifest discards history — changing
 * nothing observable about the current state, only truncating
 * structural replay. This returns a NEW package.json object with
 * `base_revision` moved to the head revision; `head_revision` and every
 * other field are left as they are. The file routine it drives is the
 * host app's — this package stays string-based: write the folded
 * manifest at head as the new `manifest.json`, rename `ops.jsonl` →
 * `ops.archive.jsonl` (or delete it), start a fresh empty `ops.jsonl`.
 * @param {Record<string, any>} packageJson the parsed package.json
 * @param {number} headRevision the revision the compacted base holds
 * @returns {Record<string, any>}
 */
export function compactPackage(packageJson, headRevision) {
  return { ...structuredClone(packageJson), base_revision: headRevision };
}
