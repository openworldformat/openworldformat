import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { parseManifest, parseLogLine, foldLog, foldState, foldPath, buildHistory, classifyOp } from "../src/index.js";
import { simulatePhysics, foldTrajectories } from "../src/physics.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const dir = path.join(root, "examples/the-drop-test");
const manifestText = readFileSync(path.join(dir, "manifest.json"), "utf8");
const logText = readFileSync(path.join(dir, "ops.jsonl"), "utf8");
const entries = logText.split("\n").filter((l) => l.trim() !== "").map(parseLogLine);

test("the drop-test package folds: one edit, everything else is history", () => {
  const base = parseManifest(manifestText);
  assert.equal(base.entities.length, 6);
  const state = foldLog(base, entries);
  assert.equal(state.appliedEdits, 1);
  assert.ok(state.entities.some((e) => e.name === "ball_late"));
  // The recorded run folds to nothing for the document…
  assert.equal(state.entities.length, 7);
});

test("the switch's score crossed the log, as a click would", () => {
  const stateDoc = JSON.parse(readFileSync(path.join(dir, "state.json"), "utf8"));
  const folded = foldState(stateDoc, entries);
  assert.equal(folded.values["score.switch"], 10);
  assert.deepEqual(folded.undeclared, []);
});

test("semantic replay: re-simulating the fold reproduces the recorded run", () => {
  const base = parseManifest(manifestText);
  const folded = foldLog(base, entries);
  const sim = simulatePhysics(folded, { until_s: 6.0, sample_dt_s: 0.1 });

  // What the log carried…
  const track = foldTrajectories(entries);
  assert.deepEqual(Object.keys(track.bodies).sort(),
    ["ball", "ball_late", "bouncy", "feather"]);
  assert.equal(track.span_s, sim.samples.at(-1).t_s);

  // …is what the solver says again: the outcomes agree (positions
  // within a hair — same engine, same algorithm), which is the
  // extension's replay contract in miniature.
  for (const [name, samples] of Object.entries(track.bodies)) {
    const recorded = samples.at(-1).position;
    const resting = sim.resting[name];
    const drift = Math.hypot(recorded[0] - resting[0], recorded[1] - resting[1], recorded[2] - resting[2]);
    assert.ok(drift < 0.001, `${name} drifted ${drift} between recording and replay`);
  }

  // The switch contact is in the re-simulation too.
  assert.ok(sim.contacts.some((c) => c.body === "ball" && c.other === "switch_pad"));
});

// ------------------------------------------------------- speedrun-fork

const forkDir = path.join(root, "examples/speedrun-fork");
const forkManifest = parseManifest(readFileSync(path.join(forkDir, "manifest.json"), "utf8"));
const forkEntries = readFileSync(path.join(forkDir, "ops.jsonl"), "utf8")
  .split("\n").filter((l) => l.trim() !== "").map(parseLogLine);

test("a challenge chain is a history: two runs fork one course", () => {
  const history = buildHistory(forkEntries);
  // Two tips — the two runs — and both are children of the course head.
  assert.deepEqual([...history.tips].sort(), ["e3", "e4"]);
  assert.deepEqual(history.children.get("e2").sort(), ["e3", "e4"]);

  // The trunk is the course: banner and checkpoint flag, no runs folded in.
  const trunk = foldPath(forkManifest, forkEntries, "e2");
  assert.ok(trunk.entities.some((e) => e.name === "banner"));
  assert.ok(trunk.entities.some((e) => e.name === "checkpoint_flag"));

  // Each run folds the course plus its own inputs and its own time.
  const runs = { e3: ["run.kai", 9.42], e4: ["run.noor", 7.91] };
  const stateDoc = JSON.parse(readFileSync(path.join(forkDir, "state.json"), "utf8"));
  const byId = new Map(history.ordered.map((e) => [e.id, e]));
  for (const [tip, [field, time]] of Object.entries(runs)) {
    const run = foldPath(forkManifest, forkEntries, tip);
    assert.equal(run.entities.length, trunk.entities.length); // no edits in a run
    const chain = run.path.map((id) => byId.get(id));
    const folded = foldState(stateDoc, chain);
    assert.equal(folded.values[field], time);
    const other = field === "run.kai" ? "run.noor" : "run.kai";
    assert.equal(folded.values[other], 0.0); // the other run never happened here
    const samples = chain.flatMap((e) => e.ops.filter((o) => classifyOp(o).kind === "input"));
    assert.equal(samples.length, 5); // the playthrough, recorded
  }
});
