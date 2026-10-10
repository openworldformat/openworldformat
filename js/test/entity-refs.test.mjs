// The one list of entity-reference fields (spec/world.md "Identity",
// schema/entity-refs.json): the JS reference embeds a generated copy
// (js/src/entity-refs.mjs) and this test fails when the copy drifts
// from the canonical file — and pins that the passes actually walk it.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { ENTITY_REFS } from "../src/entity-refs.mjs";
import { parseManifest, foldLog, ingest, mergeBranch } from "../src/index.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const canonical = JSON.parse(readFileSync(path.join(root, "schema/entity-refs.json"), "utf8"));

test("the embedded list is the canonical list", () => {
  assert.deepEqual(ENTITY_REFS, canonical.refs);
});

test("the canonical list holds every field the merge table rewrites", () => {
  const paths = ENTITY_REFS.map((r) => `${r.scope}:${r.path.join("/")}`);
  for (const expected of [
    "entity:parent",
    "entity:behaviors/*/Orbit/center",
    "entity:behaviors/*/LookAt/target",
    "avatar:model_entity",
    "creation:entities/*",
  ]) {
    assert.ok(paths.includes(expected), `missing ${expected}`);
  }
});

const line = (/** @type {number} */ revision, /** @type {any[]} */ ops) => ({ revision, ops });

test("name binding walks the list: a string parent binds at intake", () => {
  const manifest = { version: 3, meta: { name: "t" }, entities: [{ id: 1, name: "sun" }] };
  const state = foldLog(manifest, []);
  const result = ingest(state, [
    { SpawnEntity: { entity: { id: 2, name: "planet", parent: "sun" } } },
    { SpawnEntity: { entity: { name: "moon", parent: "planet" } } },
  ]);
  assert.ok(result.ok);
  assert.equal(result.ops[0].SpawnEntity.entity.parent, 1);
  assert.equal(result.ops[1].SpawnEntity.entity.parent, 2);
  // A raw log's string parent is not a committed form: the fold refuses
  // it at apply (committed ops carry ids).
  assert.throws(
    () => foldLog(manifest, [line(1, [{ SpawnEntity: { entity: { id: 4, name: "x", parent: "sun" } } }])]),
    /parent/,
  );
});

test("ingest binds the avatar's marked refs: model_entity by name", () => {
  const manifest = { version: 3, meta: { name: "t" }, entities: [{ id: 1, name: "hero" }] };
  const state = foldLog(manifest, []);
  const result = ingest(state, [{ ModifyWorld: { patch: { avatar: { model_entity: "hero" } } } }]);
  assert.ok(result.ok);
  assert.equal(result.ops[0].ModifyWorld.patch.avatar.model_entity, 1);
  assert.equal(result.state.scene.avatar.model_entity, 1);
});

test("merge rewriting walks the list for every scope", () => {
  const manifest = { version: 3, meta: { name: "t" }, entities: [] };
  const main = [line(1, [{ SpawnEntity: { entity: { id: 1, name: "main-one" } } }])];
  const state = foldLog(manifest, main);
  const branch = [line(2, [
    { SpawnEntity: { entity: { id: 1, name: "branch-one", parent: 1, behaviors: [{ Orbit: { center: 1, radius: 2, speed: 10 } }, { LookAt: { target: 1 } }] } } },
    { ModifyWorld: { patch: { avatar: { model_entity: 1 }, creations: [{ id: 1, name: "c", entities: [1] }] } } },
  ])];
  const { entries, remapped } = mergeBranch(state, branch);
  assert.deepEqual([...remapped], [[1, 2]]);
  const [spawn, world] = entries[0].ops;
  assert.equal(spawn.SpawnEntity.entity.parent, 2);
  assert.equal(spawn.SpawnEntity.entity.behaviors[0].Orbit.center, 2);
  assert.equal(spawn.SpawnEntity.entity.behaviors[1].LookAt.target, 2);
  assert.equal(world.ModifyWorld.patch.avatar.model_entity, 2);
  assert.deepEqual(world.ModifyWorld.patch.creations[0].entities, [2]);
});

test("strict readers admit marked worlds (parseManifest strict, the registry rule)", () => {
  const text = readFileSync(path.join(root, "conformance/cinematography.json"), "utf8");
  parseManifest(text, { strict: true }); // ext-cinematography is registered
});
