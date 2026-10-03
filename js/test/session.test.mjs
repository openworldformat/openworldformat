// Session-level compliance: entry identity, immediate name binding,
// inverses, branch merging, strict mode, and the package helpers — the
// rules spec/session.md, spec/state.md and spec/package.md state that
// the fold alone doesn't already pin. Same style as fold.test.mjs:
// node:test, assert/strict, the repository's own worlds where they fit.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import {
  MAX_ENTITY_ID,
  REGISTERED_EXTENSIONS,
  EXT_PROVENANCE_FIELDS,
  canonicalJson,
  computeEntryId,
  computeInverse,
  mergeBranch,
  snapshotFilename,
  compactPackage,
  extProvenance,
  parseManifest,
  parseLogLine,
  foldLog,
} from "../src/index.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/** One log line: revision n, author "t", the ops. */
const line = (revision, ops) =>
  parseLogLine(JSON.stringify({ revision, author: { name: "t" }, ops, timestamp_ms: revision }));

// ---------------------------------------------------------------------------
// Entry identity (spec/session.md "Entry identity, forks and branches")
// ---------------------------------------------------------------------------

test("canonical JSON sorts keys recursively, keeps array order, emits no whitespace", () => {
  assert.equal(canonicalJson({ b: 1, a: [2, 1], c: { z: true, y: null } }),
    '{"a":[2,1],"b":1,"c":{"y":null,"z":true}}');
  assert.equal(canonicalJson("s"), '"s"');
  assert.equal(canonicalJson(true), "true");
  assert.equal(canonicalJson(null), "null");
  // JS prints 1.0 as "1" — the cross-language caveat the JSDoc states:
  // hash equality across languages holds for integer-valued JSON.
  assert.equal(canonicalJson(1.0), "1");
});

test("the entry-identity golden vector — all five reference folds agree", () => {
  const entry = {
    revision: 7,
    timestamp_ms: 1790000000123,
    author: { peer: 3, name: "maya" },
    ops: [{ SpawnEntity: { entity: { id: 1, name: "beacon" } } }],
    parent: "e6",
  };
  assert.equal(
    canonicalJson(entry),
    '{"author":{"name":"maya","peer":3},"ops":[{"SpawnEntity":{"entity":{"id":1,"name":"beacon"}}}],"parent":"e6","revision":7,"timestamp_ms":1790000000123}',
  );
  assert.equal(computeEntryId(entry),
    "sha256:4b754af615abd0b36a11f6bec2753eedb7be6492080d8ed8ba6e50464dfaffa2");
  // Identity is what the entry says, not what it is called: the entry's
  // own id never reaches the hash input.
  assert.equal(computeEntryId({ ...entry, id: "whatever" }), computeEntryId(entry));
  // And not what the reader annotates it with either: a parsed line —
  // classified ops attached — hashes to the same id as the bare entry.
  const line = JSON.stringify({ ...entry, id: "line-id", ops: [{ SpawnEntity: { entity: { id: 1, name: "beacon" } } }] });
  assert.equal(computeEntryId(parseLogLine(line)), computeEntryId(entry));
});

// ---------------------------------------------------------------------------
// Immediate name binding (spec/world.md "Identity")
// ---------------------------------------------------------------------------

test("by-name behavior refs bind to ids at ingestion — a later rename can't re-bind them", () => {
  const manifest = { version: 3, meta: { name: "t" }, entities: [] };
  const entries = [
    line(1, [{ SpawnEntity: { entity: { id: 500, name: "a" } } }]),
    line(2, [{ SpawnEntity: { entity: { id: 501, name: "satellite", behaviors: [{ Orbit: { center: "a", radius: 3, speed: 10 } }] } } }]),
    line(3, [{ ModifyEntity: { id: 500, patch: { name: "b" } } }]),
  ];
  const state = foldLog(manifest, entries);
  const satellite = state.entities.find((e) => e.id === 501);
  // Bound at entry 2, to the id — entry 3's rename can't reach it.
  assert.equal(satellite.behaviors[0].Orbit.center, 500);
});

test("a name the same entry spawned binds — an entry is atomic", () => {
  const manifest = { version: 3, meta: { name: "t" }, entities: [] };
  const entries = [
    line(1, [
      { SpawnEntity: { entity: { id: 1, name: "sun" } } },
      { SpawnEntity: { entity: { id: 2, name: "moon", behaviors: [{ Orbit: { center: "sun", radius: 2, speed: 10 } }] } } },
    ]),
  ];
  const state = foldLog(manifest, entries);
  assert.equal(state.entities.find((e) => e.id === 2).behaviors[0].Orbit.center, 1);
});

test("a name nothing owns at ingestion fails the entry", () => {
  const manifest = { version: 3, meta: { name: "t" }, entities: [] };
  const entries = [
    line(1, [
      { SpawnEntity: { entity: { id: 500, name: "a" } } },
      { SpawnEntity: { entity: { id: 501, name: "sat", behaviors: [{ LookAt: { target: "ghost" } }] } } },
    ]),
  ];
  assert.throws(() => foldLog(manifest, entries), /no entity named 'ghost'/);
});

test("manifest name refs resolve against the complete base — ids in, ids out", () => {
  const manifestText = readFileSync(path.join(root, "conformance/behaviors.json"), "utf8");
  const manifest = parseManifest(manifestText);
  const state = foldLog(manifest, []);
  const orbiter = state.entities.find((e) => e.name === "orbiter_entity");
  const watcher = state.entities.find((e) => e.name === "watcher");
  assert.equal(orbiter.behaviors[0].Orbit.center, 3); // "hub", resolved
  assert.equal(watcher.behaviors[0].LookAt.target, 4); // "orbiter_entity", resolved
  // The caller's manifest is untouched: the fold folds a copy.
  assert.equal(parseManifest(manifestText).entities.find((e) => e.name === "orbiter_entity")
    .behaviors[0].Orbit.center, "hub");
});

// ---------------------------------------------------------------------------
// Inverses (undo is appending the inverse)
// ---------------------------------------------------------------------------

test("an inverse round-trips: the delete's inverse restores the tree exactly", () => {
  const manifest = { version: 3, meta: { name: "t" }, entities: [] };
  const spawn = line(1, [
    { SpawnEntity: { entity: { id: 1, name: "parent", transform: { position: [1, 2, 3] } } } },
    { SpawnEntity: { entity: { id: 2, name: "child", parent: 1, shape: { Sphere: { radius: 0.5 } } } } },
  ]);
  const state = foldLog(manifest, [spawn]);
  const before = structuredClone(state.entities);

  const deleteOp = { DeleteEntity: { id: 1 } };
  const inverse = computeInverse(deleteOp, state);
  // A batch of deep-copied spawns, parents first so re-spawning applies.
  assert.deepEqual(inverse.Batch.ops.map((o) => o.SpawnEntity.entity.id), [1, 2]);
  assert.equal(inverse.Batch.ops[1].SpawnEntity.entity.parent, 1);

  const undo = line(3, [inverse]);
  const restored = foldLog(manifest, [spawn, line(2, [deleteOp]), undo]);
  assert.deepEqual(restored.entities, before);
});

test("computeInverse: every kind, and the refusals", () => {
  const manifest = {
    version: 3,
    meta: { name: "t" },
    entities: [{ id: 9, name: "e", light: { light_type: "point", intensity: 800 } }],
  };
  const state = foldLog(manifest, []);

  // Spawn inverses to the delete of what it spawns.
  assert.deepEqual(
    computeInverse({ SpawnEntity: { entity: { id: 20, name: "x" } } }, state),
    { DeleteEntity: { id: 20 } },
  );

  // Modify inverses to the old values: present fields restore, absent
  // ones clear; name and parent included.
  assert.deepEqual(
    computeInverse({ ModifyEntity: { id: 9, patch: { light: { light_type: "spot" }, shape: { Sphere: { radius: 1 } }, name: "renamed", parent: null } } }, state),
    { ModifyEntity: { id: 9, patch: { light: { light_type: "point", intensity: 800 }, shape: null, name: "e", parent: null } } },
  );

  // The scene-wide sets inverse to what they replace — or the defaults.
  assert.deepEqual(computeInverse({ SetCamera: { camera: { fov_degrees: 60 } } }, state).SetCamera.camera,
    { position: [5, 5, 5], look_at: [0, 0, 0], fov_degrees: 45 });
  assert.deepEqual(computeInverse({ SetEnvironment: { env: { fog_density: 0.2 } } }, state).SetEnvironment.env, {});
  assert.deepEqual(computeInverse({ SetAmbience: { ambience: [{ Wind: {} }] } }, state).SetAmbience.ambience, []);

  // Audio emitters spawn and remove into each other.
  assert.deepEqual(
    computeInverse({ SpawnAudioEmitter: { name: "wind", audio: { volume: 0.5 } } }, state),
    { RemoveAudioEmitter: { name: "wind", audio: { volume: 0.5 } } },
  );
  const withEmitter = foldLog(manifest, [line(1, [{ SpawnAudioEmitter: { name: "wind", audio: { volume: 0.5 } } }])]);
  assert.deepEqual(
    computeInverse({ RemoveAudioEmitter: { name: "wind" } }, withEmitter),
    { SpawnAudioEmitter: { name: "wind", audio: { volume: 0.5 } } },
  );

  // A batch inverses in reverse order, each against what came before it.
  const batch = {
    Batch: {
      ops: [
        { SpawnEntity: { entity: { id: 30, name: "a" } } },
        { ModifyEntity: { id: 30, patch: { name: "b" } } },
      ],
    },
  };
  assert.deepEqual(computeInverse(batch, state).Batch.ops, [
    { ModifyEntity: { id: 30, patch: { name: "a" } } }, // the rename's inverse, against post-spawn state
    { DeleteEntity: { id: 30 } },
  ]);

  // Refusals: not an edit, unknown, or an entity that isn't there.
  assert.throws(() => computeInverse({ nope: 1 }, state), /isn't an edit/);
  assert.throws(() => computeInverse({ DeleteEntity: { id: 404 } }, state), /no entity 404/);
  assert.throws(() => computeInverse({ RemoveAudioEmitter: { name: "silence" } }, state), /no audio emitter named 'silence'/);
});

// ---------------------------------------------------------------------------
// Merging branches
// ---------------------------------------------------------------------------

test("mergeBranch remaps colliding ids and rewrites every reference", () => {
  const manifest = { version: 3, meta: { name: "t" }, entities: [] };
  const main = [line(1, [{ SpawnEntity: { entity: { id: 5, name: "main-five" } } }])];
  const state = foldLog(manifest, main);

  const branch = [line(2, [
    { SpawnEntity: { entity: { id: 5, name: "branch-five" } } },
    { SpawnEntity: { entity: { id: 6, name: "child", parent: 5, behaviors: [{ Orbit: { center: 5, radius: 2, speed: 10 } }] } } },
    { SpawnEntity: { entity: { id: 8, name: "watcher", behaviors: [{ LookAt: { target: "main-five" } }] } } },
  ])];

  const { entries: rewritten, remapped } = mergeBranch(state, branch);
  // Only the branch's 5 collides (main holds 5); 6 is fresh-but-branch-
  // owned, so the remap skips past it to 7.
  assert.deepEqual([...remapped], [[5, 7]]);

  const [spawnFive, spawnChild, spawnWatcher] = rewritten[0].ops;
  assert.equal(spawnFive.SpawnEntity.entity.id, 7);
  assert.equal(spawnChild.SpawnEntity.entity.parent, 7);
  assert.equal(spawnChild.SpawnEntity.entity.behaviors[0].Orbit.center, 7);
  // Names are never remapped — the caller pre-renames; here the merged
  // fold still resolves it, at ingestion, to main's 5.
  assert.equal(spawnWatcher.SpawnEntity.entity.behaviors[0].LookAt.target, "main-five");

  // The inputs are untouched.
  assert.equal(branch[0].ops[0].SpawnEntity.entity.id, 5);
  assert.equal(branch[0].ops[1].SpawnEntity.entity.parent, 5);
  assert.equal(branch[0].ops[1].SpawnEntity.entity.behaviors[0].Orbit.center, 5);

  // Main plus the rewritten branch folds clean, both fives alive.
  const merged = foldLog(manifest, [...main, ...rewritten]);
  assert.ok(merged.entities.some((e) => e.id === 5 && e.name === "main-five"));
  assert.ok(merged.entities.some((e) => e.id === 7 && e.name === "branch-five"));
  const child = merged.entities.find((e) => e.id === 6);
  assert.equal(child.parent, 7);
  assert.equal(child.behaviors[0].Orbit.center, 7);
  assert.equal(merged.entities.find((e) => e.name === "watcher").behaviors[0].LookAt.target, 5);
});

test("mergeBranch never allocates past the id ceiling", () => {
  const manifest = { version: 3, meta: { name: "t" }, entities: [] };
  const main = [line(1, [{ SpawnEntity: { entity: { id: MAX_ENTITY_ID, name: "top" } } }])];
  const state = foldLog(manifest, main);
  // The branch spawns the one id main holds at the ceiling: the remap
  // has nowhere above it to go.
  const branch = [line(2, [{ SpawnEntity: { entity: { id: MAX_ENTITY_ID, name: "colliding" } } }])];
  assert.throws(() => mergeBranch(state, branch), /ceiling/);
});

// ---------------------------------------------------------------------------
// Strict mode (spec/profiles.md "Strict Mode for Authoring")
// ---------------------------------------------------------------------------

const cleanManifest = { version: 3, meta: { name: "t" }, entities: [{ id: 1, name: "a" }] };

test("non-strict tolerates what strict refuses — must-ignore is the runtime rule", () => {
  const odd = { ...cleanManifest, mystery: 1, meta: { name: "t", prompt: "a castle" } };
  parseManifest(JSON.stringify(odd)); // rides along, non-strict
  parseLogLine(JSON.stringify({ revision: 1, ops: [{ nope: 1 }] }));
  assert.throws(() => parseManifest(JSON.stringify(odd), { strict: true }), /mystery/);
  assert.throws(() => parseLogLine(JSON.stringify({ revision: 1, ops: [{ nope: 1 }] }), { strict: true }), /no shape recognizes/);
});

test("strict mode limits manifest, meta and entity keys, with directions for the moved ones", () => {
  // The v2 multi-file layout is gone; the error says so.
  assert.throws(
    () => parseManifest(JSON.stringify({ ...cleanManifest, layout_file: "layout.json" }), { strict: true }),
    /layout_file.*gone in schema v3/,
  );
  assert.throws(
    () => parseManifest(JSON.stringify({ ...cleanManifest, region_files: [] }), { strict: true }),
    /gone in schema v3/,
  );
  // Lineage moved out of core meta into the provenance extension.
  assert.throws(
    () => parseManifest(JSON.stringify({ ...cleanManifest, meta: { name: "t", model: "big-1" } }), { strict: true }),
    /ext-provenance/,
  );
  assert.throws(
    () => parseManifest(JSON.stringify({ ...cleanManifest, meta: { name: "t", biome: "alpine" } }), { strict: true }),
    /meta\.biome.*ext-provenance/,
  );
  // An unknown entity key is a typo, not an extension.
  assert.throws(
    () => parseManifest(JSON.stringify({ ...cleanManifest, entities: [{ id: 1, name: "a", colour: [1, 0, 0] }] }), { strict: true }),
    /entity key 'colour'/,
  );
  // The plain-boolean form still works (backwards-compatible signature).
  assert.throws(
    () => parseManifest(JSON.stringify({ ...cleanManifest, mystery: 1 }), true),
    /mystery/,
  );
  // And a conformant manifest parses strict — the repository's own.
  const example = readFileSync(path.join(root, "examples/hello-world/manifest.json"), "utf8");
  assert.equal(parseManifest(example, { strict: true }).entities.length > 0, true);
  const logLines = readFileSync(path.join(root, "examples/hello-world/ops.jsonl"), "utf8")
    .split("\n").filter((l) => l.trim() !== "");
  for (const l of logLines) parseLogLine(l, { strict: true }); // every op has a shape
});

test("strict mode admits registered extensions and refuses unregistered ones", () => {
  assert.equal(REGISTERED_EXTENSIONS.includes("ext-provenance"), true);
  parseManifest(JSON.stringify({ ...cleanManifest, "ext-provenance": { prompt: "p" } }), { strict: true });
  assert.throws(
    () => parseManifest(JSON.stringify({ ...cleanManifest, "ext-avatars": {} }), { strict: true }),
    /ext-avatars.*unregistered extension.*registry/,
  );
  assert.throws(
    () => parseManifest(JSON.stringify({ ...cleanManifest, entities: [{ id: 1, name: "a", "ext-avatars": {} }] }), { strict: true }),
    /unregistered extension/,
  );
  assert.throws(
    () => parseManifest(JSON.stringify({ ...cleanManifest, meta: { name: "t", "ext-avatars": {} } }), { strict: true }),
    /unregistered extension/,
  );
  assert.throws(
    () => parseLogLine(JSON.stringify({ revision: 1, ops: [{ "ext-avatars": {} }] }), { strict: true }),
    /unregistered extension/,
  );
  parseLogLine(JSON.stringify({ revision: 1, ops: [{ "ext-physics": { bodies: [] } }] }), { strict: true });
});

// ---------------------------------------------------------------------------
// Provenance, snapshots, compaction
// ---------------------------------------------------------------------------

test("extProvenance reads the five lineage fields, or null", () => {
  assert.deepEqual(EXT_PROVENANCE_FIELDS,
    ["prompt", "model", "generation_duration_ms", "biome", "semantic_category"]);
  assert.equal(extProvenance({ version: 3, entities: [] }), null); // absent
  assert.deepEqual(extProvenance({ version: 3, entities: [], meta: {} }), null);
  assert.deepEqual(
    extProvenance({
      version: 3,
      entities: [],
      meta: { name: "t", "ext-provenance": { prompt: "a castle", model: "big-1", generation_duration_ms: 2500 } },
    }),
    { prompt: "a castle", model: "big-1", generation_duration_ms: 2500 },
  );
});

test("snapshot filenames sanitize the entry id and stay inside snapshots/", () => {
  assert.equal(snapshotFilename("sha256:4b754af615abd0b36a11f6bec2753eedb7be6492080d8ed8ba6e50464dfaffa2", 7),
    "snapshots/entry-sha256_4b754af615abd0b36a11f6bec2753eedb7be6492080d8ed8ba6e50464dfaffa2.json");
  assert.equal(snapshotFilename("e6", 7), "snapshots/entry-e6.json");
  assert.equal(snapshotFilename(null, 7), "snapshots/rev-7.json");
});

test("compactPackage moves base_revision to the head and touches nothing else", () => {
  const pkg = { format_version: 1, name: "w", base_revision: 0, head_revision: 3, refs: { main: "e3" } };
  const compacted = compactPackage(pkg, 3);
  assert.equal(compacted.base_revision, 3);
  assert.equal(compacted.head_revision, 3); // left alone
  assert.deepEqual(compacted.refs, { main: "e3" });
  assert.equal(pkg.base_revision, 0); // the input is untouched
  assert.notEqual(compacted, pkg); // a new object
});
