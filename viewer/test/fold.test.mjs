import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import {
  SUPPORTED_SCHEMA_VERSION,
  classifyOp,
  parseManifest,
  parseLogLine,
  editOps,
  foldLog,
  foldState,
} from "../src/index.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const manifestText = readFileSync(path.join(root, "examples/hello-world/manifest.json"), "utf8");
const logText = readFileSync(path.join(root, "examples/hello-world/ops.jsonl"), "utf8");
const entries = logText
  .split("\n")
  .filter((l) => l.trim() !== "")
  .map(parseLogLine);

test("the example manifest parses at the supported schema version", () => {
  const manifest = parseManifest(manifestText);
  assert.equal(manifest.version, SUPPORTED_SCHEMA_VERSION);
  assert.ok(manifest.entities.length > 0);
});

test("a newer manifest is refused loudly, per the versioning policy", () => {
  const newer = JSON.stringify({ ...JSON.parse(manifestText), version: SUPPORTED_SCHEMA_VERSION + 1 });
  assert.throws(() => parseManifest(newer), /newer than this reader/);
});

test("ops are recognized by shape, edits first", () => {
  assert.equal(classifyOp({ SpawnEntity: { entity: { id: 1, name: "a" } } }).kind, "edit");
  assert.deepEqual(classifyOp({ SpawnEntity: { entity: { id: 1, name: "a" } } }).edit, "SpawnEntity");
  assert.equal(classifyOp({ tool: "x", args: {} }).kind, "tool");
  assert.equal(classifyOp({ input: { actor: "v" } }).kind, "input");
  assert.equal(classifyOp({ state: { "score.x": 1 } }).kind, "state");
  assert.equal(classifyOp({ clock: { playing: true, position_s: 0 } }).kind, "clock");
  assert.equal(classifyOp({ nope: 1 }).kind, "unknown");
});

test("an old-format line (edits only) parses as edits", () => {
  const line = JSON.stringify({
    revision: 7,
    author: { peer: 3, name: "maya" },
    ops: [{ DeleteEntity: { id: 1 } }],
    timestamp_ms: 1,
  });
  const entry = parseLogLine(line);
  assert.equal(editOps(entry).length, 1);
  assert.equal(editOps(entry)[0].edit, "DeleteEntity");
});

test("the example log folds: the lantern appears, history folds to nothing", () => {
  const base = parseManifest(manifestText);
  const before = base.entities.length;
  const state = foldLog(base, entries);
  // Five entries, one edit op among them: everything else is history.
  assert.equal(state.appliedEdits, 1);
  assert.equal(state.entities.length, before + 1);
  const lantern = state.entities.find((e) => e.id === 100);
  assert.equal(lantern.name, "lantern");
  assert.deepEqual(lantern.transform.position, [-12.0, 0.0, 3.0]);
  // History entries carried the revision without bumping anything:
  assert.ok(state.entities.every((e) => e.id !== 101));
});

test("modify applies a patch; absent fields are unchanged, null clears", () => {
  const base = parseManifest(manifestText);
  const entries = [
    parseLogLine(
      JSON.stringify({
        revision: 1,
        author: { name: "t" },
        ops: [
          {
            ModifyEntity: {
              id: 1,
              patch: { shape: { Sphere: { radius: 0.5 } }, material: null },
            },
          },
        ],
        timestamp_ms: 0,
      }),
    ),
  ];
  const state = foldLog(base, entries);
  const ground = state.entities.find((e) => e.id === 1);
  assert.deepEqual(ground.shape, { Sphere: { radius: 0.5 } });
  assert.equal(ground.material, undefined);
  assert.deepEqual(ground.transform.position, [0.0, 0.0, 0.0]); // untouched
});

test("deleting an entity deletes its descendants", () => {
  const base = parseManifest(manifestText);
  const entries = [
    parseLogLine(
      JSON.stringify({
        revision: 1,
        author: { name: "t" },
        ops: [
          { SpawnEntity: { entity: { id: 200, name: "p" } } },
          { SpawnEntity: { entity: { id: 201, name: "c1", parent: 200 } } },
          { SpawnEntity: { entity: { id: 202, name: "c2", parent: 201 } } },
        ],
        timestamp_ms: 0,
      }),
    ),
    parseLogLine(
      JSON.stringify({
        revision: 2,
        author: { name: "t" },
        ops: [{ DeleteEntity: { id: 200 } }],
        timestamp_ms: 1,
      }),
    ),
  ];
  const state = foldLog(base, entries);
  assert.ok(!state.entities.some((e) => e.id === 200));
  assert.ok(!state.entities.some((e) => e.id === 201));
  assert.ok(!state.entities.some((e) => e.id === 202));
});

test("a batch applies all-or-nothing", () => {
  const base = parseManifest(manifestText);
  const batch = parseLogLine(
    JSON.stringify({
      revision: 1,
      author: { name: "t" },
      ops: [
        {
          Batch: {
            ops: [
              { SpawnEntity: { entity: { id: 300, name: "ok" } } },
              { DeleteEntity: { id: 99999 } }, // refuses
            ],
          },
        },
      ],
      timestamp_ms: 0,
    }),
  );
  assert.throws(() => foldLog(base, [batch]), /no entity 99999/);
});

test("state folds over the declaration, tolerating the undeclared", () => {
  const stateDoc = JSON.parse(readFileSync(
    path.join(root, "examples/hello-world/state.json"), "utf8"));
  const result = foldState(stateDoc, entries);
  // The example's log sets score.tour to 1; the declaration's initial was 0.
  assert.equal(result.values["score.tour"], 1);
  assert.deepEqual(result.undeclared, []);

  const richer = {
    format_version: 1,
    fields: {
      "score.main": { type: "int", initial: 0 },
      inventory: { type: "map", initial: {} },
      "has.map": { type: "bool", initial: false },
    },
  };
  const ops = [
    { state: { "score.main": 5 } },
    { state: { "inventory.rope": 1, "inventory.torch": 2 } },
    { state: { "inventory.rope": null } },
    { state: { "has.map": true } },
    { state: { "has.map": null } },
    { state: { "unknown.key": 7 } },
    { state: { "unknown.key": null } },
  ].map((op, i) => parseLogLine(JSON.stringify({
    revision: 1, author: { name: "t" }, ops: [op], timestamp_ms: i,
  })));
  const folded = foldState(richer, ops);
  assert.equal(folded.values["score.main"], 5);
  assert.deepEqual(folded.values.inventory, { torch: 2 });
  assert.equal(folded.values["has.map"], false); // null reset the initial
  assert.ok(!("unknown.key" in folded.values)); // set, carried, then removed
  assert.deepEqual(folded.undeclared, ["unknown.key"]);
});

test("a torn line loses at most itself", () => {
  const base = parseManifest(manifestText);
  const lines = logText.split("\n").filter((l) => l.trim() !== "");
  const parsed = [];
  let torn = 0;
  for (const line of lines) {
    try {
      parsed.push(parseLogLine(line));
    } catch {
      torn += 1; // skip, count
    }
  }
  assert.equal(torn, 0); // the example's log is whole
  // And a genuinely torn last line:
  assert.throws(() => parseLogLine(lines[0].slice(0, 20)));
});
