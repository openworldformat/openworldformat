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
    "entity:ext-cinematography/camera/aim_entity",
    "entity:ext-cinematography/camera/focus_entity",
    "avatar:model_entity",
    "creation:entities/*",
  ]) {
    assert.ok(paths.includes(expected), `missing ${expected}`);
  }
});

test("an extension's marked refs bind at ingestion by name, in a spawn and in a patch", () => {
  const manifest = { version: 3, meta: { name: "t" }, entities: [{ id: 1, name: "maya" }] };
  const state = foldLog(manifest, []);
  const result = ingest(state, [
    { SpawnEntity: { entity: { id: 2, name: "4A", "ext-cinematography": { camera: { aim_entity: "maya" } } } } },
    // An `ext-*` patch replaces the block, so the patch carries the whole camera.
    { ModifyEntity: { id: "4A", patch: { "ext-cinematography": { camera: { aim_entity: "maya", focus_entity: "maya" } } } } },
  ]);
  assert.ok(result.ok, JSON.stringify(result));
  const camera = result.state.byId.get(2)["ext-cinematography"].camera;
  assert.equal(camera.aim_entity, 1, "the document holds the id");
  assert.equal(camera.focus_entity, 1, "a patch's references bind too");
  // Committed ops always carry ids (spec/session.md): the spawn's and
  // the patch's own JSON are bound, not only the document.
  assert.equal(result.ops[0].SpawnEntity.entity["ext-cinematography"].camera.aim_entity, 1);
  assert.equal(result.ops[1].ModifyEntity.patch["ext-cinematography"].camera.focus_entity, 1);
  // A name nothing answers to is refused, like any marked ref.
  const refused = ingest(state, [
    { SpawnEntity: { entity: { id: 3, name: "4B", "ext-cinematography": { camera: { aim_entity: "nobody" } } } } },
  ]);
  assert.ok(!refused.ok);
});

test("merge rewriting walks the extension's marked refs", () => {
  const manifest = { version: 3, meta: { name: "t" }, entities: [] };
  const state = foldLog(manifest, [line(1, [{ SpawnEntity: { entity: { id: 1, name: "maya" } } }])]);
  const branch = [line(2, [
    { SpawnEntity: { entity: { id: 1, name: "kai" } } },
    { SpawnEntity: { entity: { id: 2, name: "4A", "ext-cinematography": { camera: { aim_entity: 1, focus_entity: 1 } } } } },
    { ModifyEntity: { id: 2, patch: { "ext-cinematography": { camera: { aim_entity: 1 } } } } },
  ])];
  const { entries, remapped } = mergeBranch(state, branch);
  assert.deepEqual([...remapped], [[1, 3]]);
  const [, spawn, patch] = entries[0].ops;
  assert.equal(spawn.SpawnEntity.entity["ext-cinematography"].camera.aim_entity, 3);
  assert.equal(spawn.SpawnEntity.entity["ext-cinematography"].camera.focus_entity, 3);
  assert.equal(patch.ModifyEntity.patch["ext-cinematography"].camera.aim_entity, 3);
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
