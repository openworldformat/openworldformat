import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { parseManifest, parseLogLine, classifyOp, foldLog } from "../src/index.js";
import {
  collectPhysics,
  simulatePhysics,
  foldTrajectories,
  trajectoryOp,
  runOutcomes,
  EXTENSION_NAME,
} from "../src/physics.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (p) => readFileSync(path.join(root, p), "utf8");

const entry = (ops) => parseLogLine(JSON.stringify({
  revision: 1, author: { name: "t" }, ops, timestamp_ms: 0,
}));

// --------------------------------------------------------------------- fold

test("extension ops classify and fold to nothing", () => {
  assert.deepEqual(classifyOp({ "ext-physics": { t_s: [0], bodies: {} } }), {
    kind: "extension", name: "ext-physics", value: { t_s: [0], bodies: {} },
  });
  // Unknown extensions are still extensions — the namespace is the channel.
  assert.equal(classifyOp({ "ext-avatars": { a: 1 } }).kind, "extension");
  // Not a plain string key with non-object value, not an edit.
  assert.equal(classifyOp({ "ext-physics": "nope" }).kind, "unknown");

  const manifest = parseManifest(read("examples/hello-world/manifest.json"));
  const before = foldLog(manifest, []).entities.length;
  const state = foldLog(manifest, [
    entry([{ "ext-physics": { t_s: [0.0, 0.1], bodies: { ball: [[0, 5, 0], [0, 4.95, 0]] } } }]),
  ]);
  assert.equal(state.entities.length, before); // the document never moved
  assert.equal(state.appliedEdits, 0);
});

test("ext-* fields survive a modify patch; null clears them", () => {
  const manifest = parseManifest(read("conformance/physics.json"));
  const state = foldLog(manifest, [
    entry([{ ModifyEntity: { id: 4, patch: { "ext-physics": { body: "static" } } } }]),
  ]);
  const ball = state.entities.find((e) => e.id === 4);
  assert.deepEqual(ball[EXTENSION_NAME], { body: "static" });

  const cleared = foldLog(manifest, [
    entry([{ ModifyEntity: { id: 4, patch: { "ext-physics": null } } }]),
  ]);
  assert.equal(cleared.entities.find((e) => e.id === 4)[EXTENSION_NAME], undefined);
});

// ------------------------------------------------------------------- solver

const physicsWorld = () => parseManifest(read("conformance/physics.json"));

test("bodies are collected from the declaration, never inferred", () => {
  const { gravity, dynamic, kinematic, statics } = collectPhysics(physicsWorld());
  assert.deepEqual(gravity, [0, -9.81, 0]);
  assert.deepEqual(dynamic.map((b) => b.name).sort(), ["ball", "bouncy_ball"]);
  assert.deepEqual(kinematic, []);
  assert.deepEqual(statics.map((s) => s.name).sort(), ["ground", "pedestal"]);
  // The pedestal is a box around its extents: top at y = 1.
  const pedestal = statics.find((s) => s.name === "pedestal");
  assert.deepEqual(pedestal.max, [1, 1, 1]);
  // hello-world declares no bodies: nothing participates.
  const plain = collectPhysics(parseManifest(read("examples/hello-world/manifest.json")));
  assert.deepEqual(plain, { gravity: [0, -9.81, 0], dynamic: [], kinematic: [], statics: [] });
});

test("the simulation is deterministic: two runs are identical", () => {
  const a = simulatePhysics(physicsWorld(), { until_s: 6 });
  const b = simulatePhysics(physicsWorld(), { until_s: 6 });
  assert.deepEqual(a, b);
});

test("a dropped ball finds the pedestal, rests on it, and the world settles", () => {
  const sim = simulatePhysics(physicsWorld(), { until_s: 6 });
  const first = sim.contacts.find((c) => c.body === "ball" && c.other === "pedestal");
  assert.ok(first, "ball never touched the pedestal");
  assert.ok(first.t_s < 2.0, `contact at ${first.t_s}s, later than expected`);
  // Pedestal top is y = 1, ball radius 0.3: it rests at [0, 1.3, 0].
  assert.ok(Math.abs(sim.resting.ball[1] - 1.3) < 0.05,
    `ball rests at ${sim.resting.ball}`);
  assert.deepEqual(sim.resting.ball.slice(0, 1), [0]); // no horizontal drift
  assert.ok(sim.bounces.filter((x) => x.body === "bouncy_ball").length >= 4);
  assert.ok(sim.settled_s < 6, `world settled at ${sim.settled_s}s`);
});

// --------------------------------------------------- trajectory round-trip

test("a trajectory op round-trips: simulate, write, fold, scrub", () => {
  const sim = simulatePhysics(physicsWorld(), { until_s: 2, sample_dt_s: 0.25 });
  const op = trajectoryOp(sim);
  assert.equal(op[EXTENSION_NAME].t_s.length, sim.samples.length);

  const entries = [entry([op])];
  // The op folds to nothing for the document…
  const state = foldLog(physicsWorld(), entries);
  assert.equal(state.appliedEdits, 0);
  // …and folds to samples for playback.
  const track = foldTrajectories(entries);
  assert.deepEqual(Object.keys(track.bodies).sort(), ["ball", "bouncy_ball"]);
  assert.equal(track.bodies.ball.length, sim.samples.length);
  assert.deepEqual(track.bodies.ball[0].position, [0, 5, 0]); // spawn
  assert.ok(track.span_s > 0);
  // The fold tolerates a reader that never classified (raw entries).
  const raw = [JSON.parse(JSON.stringify({ revision: 1, author: { name: "t" }, ops: [op], timestamp_ms: 0 }))];
  assert.equal(foldTrajectories(raw).bodies.ball.length, sim.samples.length);
});

// ----------------------------------------------------------- conformance CI

test("the physics conformance outcomes pass under the reference solver", () => {
  const manifest = physicsWorld();
  const outcomes = JSON.parse(read("conformance/outcomes/physics.json"));
  const result = runOutcomes(manifest, outcomes);
  assert.deepEqual(result.failures, []);
  assert.ok(result.ok);
});
