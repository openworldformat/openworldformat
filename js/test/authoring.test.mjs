// The live-authoring rules (spec/session.md "Authoring", "The fold is
// total"; spec/package.md "Head-first"), the way the Rust reference runs
// them: every world survives an empty fold, every example's manifest is
// the fold to main, ModifyWorld reaches every scene field, and a batch
// is ingested whole or refused whole.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import path from "node:path";

import {
  parseManifest, parseLogLine, foldLog, foldPath, toManifest, computeInverse, ingest, mergePatch,
  manifestText, computeEntryId,
  SUPPORTED_FORMAT_VERSION,
} from "../src/index.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (/** @type {string} */ p) => readFileSync(path.join(root, p), "utf8");

const examples = readdirSync(path.join(root, "examples"))
  .filter((name) => existsSync(path.join(root, "examples", name, "manifest.json")))
  .sort();

/** A world compared as a world: entities by id, empty and null fields
 *  as absent, next_entity_id by its effective value. */
function normalized(/** @type {any} */ m) {
  /** @type {Record<string, any>} */
  const out = {};
  for (const [key, value] of Object.entries(m)) {
    if (value === null || (Array.isArray(value) && value.length === 0)) continue;
    out[key] = value;
  }
  const past = (m.entities ?? []).reduce((n, e) => Math.max(n, e.id + 1), 1);
  out.next_entity_id = Math.max(m.next_entity_id ?? 1, past);
  out.entities = [...(m.entities ?? [])].sort((a, b) => a.id - b.id);
  return JSON.parse(JSON.stringify(out));
}

test("the fold is total: every world survives an empty fold", () => {
  const worlds = readdirSync(path.join(root, "conformance"))
    .filter((f) => f.endsWith(".json"))
    .map((f) => [`conformance/${f}`, read(`conformance/${f}`)]);
  for (const name of examples) {
    worlds.push([`${name}/snapshots/base.json`, read(`examples/${name}/snapshots/base.json`)]);
    worlds.push([`${name}/manifest.json`, read(`examples/${name}/manifest.json`)]);
  }
  for (const [name, text] of worlds) {
    const manifest = parseManifest(text);
    const state = foldLog(manifest, []);
    // Names bind at ingestion: compare against the bound entities.
    const expected = { ...manifest, entities: state.entities };
    assert.deepEqual(normalized(toManifest(state)), normalized(expected), name);
  }
});

test("every example is head-first: its manifest is the fold to main", () => {
  assert.equal(SUPPORTED_FORMAT_VERSION, 2);
  for (const name of examples) {
    const pkg = JSON.parse(read(`examples/${name}/package.json`));
    assert.equal(pkg.format_version, 2, name);
    const base = parseManifest(read(`examples/${name}/snapshots/base.json`));
    const entries = read(`examples/${name}/ops.jsonl`).split("\n").filter((l) => l.trim()).map(parseLogLine);
    const state = foldPath(base, entries, pkg.refs?.main);
    const head = parseManifest(read(`examples/${name}/manifest.json`));
    assert.deepEqual(normalized(toManifest(state)), normalized(head), `${name}: head`);
    const sha = createHash("sha256").update(readFileSync(path.join(root, `examples/${name}/manifest.json`))).digest("hex");
    assert.equal(pkg.world_sha256, sha, `${name}: world_sha256 names manifest.json's bytes`);
  }
});

test("ModifyWorld reaches every scene field, and its inverse undoes it", () => {
  const state = foldLog(parseManifest(read("examples/hello-world/manifest.json")), []);
  const op = {
    ModifyWorld: {
      patch: {
        meta: { name: "hello-again", description: "renamed" },
        environment: null,
        tours: [{ name: "walk", waypoints: [] }],
        soundtrack: null,
      },
    },
  };
  const inverse = computeInverse(op, state);
  const changed = foldLog(toManifest(state), [{ revision: 1, ops: [op] }]);
  const m = toManifest(changed);
  assert.equal(m.meta.name, "hello-again");
  assert.equal(m.environment, undefined, "null clears");
  assert.equal(m.tours.length, 1);
  const back = foldLog(m, [{ revision: 2, ops: [inverse] }]);
  assert.deepEqual(normalized(toManifest(back)), normalized(toManifest(state)));
  assert.throws(
    () => foldLog(m, [{ revision: 2, ops: [{ ModifyWorld: { patch: { meta: null } } }] }]),
    /can't be cleared/,
  );
});

const yard = () => foldLog(parseManifest(JSON.stringify({
  version: 3,
  meta: { name: "yard" },
  entities: [
    { id: 1, name: "ground", shape: { Plane: { x: 20, z: 20 } } },
    { id: 2, name: "crate", transform: { position: [0, 0.5, 0], scale: [2, 2, 2] },
      material: { color: [0.6, 0.4, 0.2, 1], roughness: 0.8 } },
  ],
  next_entity_id: 3,
})), []);

test("ingest binds names, allocates ids and merges partial patches", () => {
  const done = ingest(yard(), [
    { SpawnEntity: { entity: { name: "lamp", parent: "crate" } } },
    { ModifyEntity: { id: "crate", patch: { transform: { position: [3, 0.5, 0] }, material: { base_color_texture: "brick.png" } } } },
    { ModifyWorld: { patch: { meta: { description: "a yard" } } } },
  ]);
  assert.equal(done.ok, true);
  if (!done.ok) return;
  assert.equal(done.spawned.lamp, 3);
  const crate = done.state.byId.get(2);
  assert.deepEqual(crate.transform.scale, [2, 2, 2], "scale kept");
  assert.equal(crate.material.roughness, 0.8, "roughness kept");
  assert.equal(done.state.byId.get(3).parent, 2);
  assert.equal(toManifest(done.state).meta.description, "a yard");
  assert.equal(done.state.name, "yard", "meta merges, the name stays");
  // What commits is the whole value: the fold needs no merging.
  assert.deepEqual(done.ops[1].ModifyEntity.patch.transform.scale, [2, 2, 2]);
});

test("one bad op refuses the batch, with a reason per op", () => {
  const before = toManifest(yard());
  const state = yard();
  const done = ingest(state, [
    { ModifyEntity: { id: "crate", patch: { material: { colour: [1, 0, 0, 1] } } } },
    { DeleteEntity: { id: "nobody" } },
    { MoveEntity: { id: 2 } },
    { SpawnEntity: { entity: { id: 1, name: "again" } } },
    { ModifyEntity: { id: "ground", patch: { transform: { position: [0, 1, 0] } } } },
  ]);
  assert.equal(done.ok, false);
  if (done.ok) return;
  assert.equal(done.errors.length, 4, done.errors.join("\n"));
  assert.match(done.errors[0], /^op 0: \/ModifyEntity\/patch\/material\/colour/);
  assert.match(done.errors[1], /no entity is named "nobody"/);
  assert.match(done.errors[2], /isn't an op kind/);
  assert.match(done.errors[3], /already exists/);
  assert.deepEqual(toManifest(state), before, "the state it was given is untouched");
});

test("mergePatch is RFC 7396", () => {
  assert.deepEqual(
    mergePatch({ a: 1, b: { c: 2, d: 3 } }, { b: { c: null, e: 4 }, f: 5 }),
    { a: 1, b: { d: 3, e: 4 }, f: 5 },
  );
});

test("every example head is in canonical text", () => {
  for (const name of examples) {
    const text = read(`examples/${name}/manifest.json`);
    assert.equal(manifestText(parseManifest(text)), text, `${name}: canonical text`);
  }
  // Members sorted, nulls left out, plain arrays inline, entities by id.
  assert.equal(
    manifestText({ version: 3, meta: { name: "t", description: null }, entities: [{ name: "b", id: 2 }, { id: 1, name: "a", transform: { position: [0.1, 2, -3.5] } }] }),
    '{\n  "entities": [\n    {\n      "id": 1,\n      "name": "a",\n      "transform": {\n        "position": [0.1, 2, -3.5]\n      }\n    },\n    {\n      "id": 2,\n      "name": "b"\n    }\n  ],\n  "meta": {\n    "name": "t"\n  },\n  "version": 3\n}\n',
  );
});

test("an entry's message is part of its identity in every reference", () => {
  const entry = parseLogLine('{"id":"x","parent":"e6","revision":7,"author":{"name":"claude"},"timestamp_ms":1790000000123,"message":"a lantern by the gate","ops":[{"DeleteEntity":{"id":21}}]}');
  assert.equal(entry.message, "a lantern by the gate");
  assert.equal(computeEntryId(entry), "sha256:a7cd0955d35a2ff15b16ad7cc3440fb064d675ceceeca865a7ace463a5d177f2");
});
