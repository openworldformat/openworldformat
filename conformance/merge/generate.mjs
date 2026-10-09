#!/usr/bin/env node
/**
 * Generate the shared merge corpus (spec/session.md "The merge rules,
 * exactly"). Every case is one JSON file in `cases/`:
 *
 *   {
 *     "name": "...", "description": "...",
 *     "base":    <manifest>,            // the world both lines build on
 *     "main":    [ <log entry> … ],     // the main line's entries
 *     "branch":  [ <log entry> … ],     // the branch's entries, to merge
 *     "expected": {
 *       "remapped": [[oldId, newId] …], // the merge's remap table
 *       "entries":  [ <log entry> … ],  // the rewritten branch entries
 *       "head":     "<canonical text>"  // fold(base, main + entries)
 *     }
 *   }
 *
 * Expected results come from the JS reference (`js/src/index.js`); every
 * other reference runs every case in its own test suite — a reference
 * that disagrees fails its own CI. Regenerate with:
 *
 *   node conformance/merge/generate.mjs
 *
 * The generator is seeded: same code, same cases. Adding a case means
 * adding a seed (or a hand-written definition) and committing the output.
 */
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { foldLog, mergeBranch, toManifest, manifestText } from "../../js/src/index.js";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "cases");

/** mulberry32 — a small seeded PRNG, so the corpus is deterministic. */
function prng(seed) {
  let a = seed >>> 0;
  return () => {
    a |= 0; a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Compute a case's expected half through the JS reference. */
function solve(caseDef) {
  const mainState = foldLog(caseDef.base, caseDef.main);
  const { entries, remapped } = mergeBranch(mainState, caseDef.branch);
  // Snapshot the rewritten entries BEFORE the head fold: the fold holds
  // references into the ops it applies, so folding would rename them
  // under us. The head folds a deep copy.
  const snapshot = JSON.parse(JSON.stringify(entries));
  const head = foldLog(caseDef.base, [...caseDef.main, ...structuredClone(entries)]);
  return {
    ...caseDef,
    expected: {
      remapped: [...remapped],
      entries: snapshot,
      head: manifestText(toManifest(head)),
    },
  };
}

// ---------------------------------------------------------------------------
// Hand-written cases — one per rule the spec states.
// ---------------------------------------------------------------------------

const handwritten = [
  {
    name: "spent-id",
    description:
      "Main spawns 101 and 102 and deletes 102; the branch spawns its own 101. " +
      "Fresh ids come from the fold's floor (103), never from the spent 102.",
    base: { version: 3, meta: { name: "spent-id" }, entities: [] },
    main: [
      { revision: 1, message: "two", ops: [
        { SpawnEntity: { entity: { id: 101, name: "a" } } },
        { SpawnEntity: { entity: { id: 102, name: "b" } } },
      ] },
      { revision: 2, message: "drop b", ops: [{ DeleteEntity: { id: 102 } }] },
    ],
    branch: [
      { revision: 2, message: "branch's a", ops: [
        { SpawnEntity: { entity: { id: 101, name: "branch-a" } } },
      ] },
    ],
  },
  {
    name: "modify-world",
    description:
      "ModifyWorld refs are entity ids too: the branch's avatar.model_entity " +
      "and creations[].entities are rewritten through the remap.",
    base: { version: 3, meta: { name: "modify-world" }, entities: [] },
    main: [
      { revision: 1, ops: [{ SpawnEntity: { entity: { id: 1, name: "main-one" } } }] },
    ],
    branch: [
      { revision: 1, ops: [
        { SpawnEntity: { entity: { id: 1, name: "branch-one" } } },
        { SpawnEntity: { entity: { id: 2, name: "branch-two" } } },
        { ModifyWorld: { patch: { avatar: { model_entity: 1 }, creations: [{ id: 1, name: "pair", entities: [1, 2] }] } } },
      ] },
    ],
  },
  {
    name: "batch",
    description:
      "A colliding spawn inside a Batch is remapped like any other, and the " +
      "references its siblings make (parent, modify id) follow it.",
    base: { version: 3, meta: { name: "batch" }, entities: [] },
    main: [
      { revision: 1, ops: [{ SpawnEntity: { entity: { id: 1, name: "main-one" } } }] },
    ],
    branch: [
      { revision: 1, ops: [
        { Batch: { ops: [
          { SpawnEntity: { entity: { id: 1, name: "batched" } } },
          { SpawnEntity: { entity: { id: 2, name: "child", parent: 1 } } },
          { ModifyEntity: { id: 1, patch: { name: "batched-renamed" } } },
        ] } },
      ] },
    ],
  },
  {
    name: "names",
    description:
      "Two branches mint a `lighthouse` each: the merged spawns take the " +
      "`<name>-<n>` suffix, n from 2 up, first unused — here -2 is taken, " +
      "so the branch gets -3 and -4.",
    base: { version: 3, meta: { name: "names" }, entities: [] },
    main: [
      { revision: 1, ops: [
        { SpawnEntity: { entity: { id: 1, name: "lighthouse" } } },
        { SpawnEntity: { entity: { id: 2, name: "lighthouse-2" } } },
      ] },
    ],
    branch: [
      { revision: 1, ops: [
        { SpawnEntity: { entity: { id: 1, name: "lighthouse" } } },
        { SpawnEntity: { entity: { id: 3, name: "lighthouse" } } },
      ] },
    ],
  },
];

// ---------------------------------------------------------------------------
// Seeded random cases.
// ---------------------------------------------------------------------------

const NAME_POOL = ["lighthouse", "gate", "tower", "dock", "lighthouse-2", "mill"];

function randomCase(seed) {
  const rand = prng(seed);
  const pick = (arr) => arr[Math.floor(rand() * arr.length)];
  const maybe = (p) => rand() < p;

  // Base: 0–3 entities, ids 1..k, some parented.
  const baseEntities = [];
  const baseCount = Math.floor(rand() * 4);
  for (let i = 1; i <= baseCount; i += 1) {
    const e = { id: i, name: `base-${i}` };
    if (i > 1 && maybe(0.4)) e.parent = i - 1;
    baseEntities.push(e);
  }
  const base = { version: 3, meta: { name: `case-${seed}` }, entities: baseEntities };
  const usedNames = new Set(baseEntities.map((e) => e.name));
  // A name from the pool, exact when it's free (so the branch can
  // collide with it), uniquely suffixed when it isn't (so main folds).
  const mainName = (id) => {
    const wanted = pick(NAME_POOL);
    if (!usedNames.has(wanted)) { usedNames.add(wanted); return wanted; }
    const unique = `${wanted}-m${id}`;
    usedNames.add(unique);
    return unique;
  };

  // Shared history: 0–2 entries both lines descend from.
  let nextId = baseCount + 1;
  let revision = 0;
  const shared = [];
  const sharedCount = Math.floor(rand() * 3);
  for (let s = 0; s < sharedCount; s += 1) {
    revision += 1;
    const id = nextId; nextId += 1;
    usedNames.add(`shared-${id}`);
    shared.push({ revision, message: `shared-${s}`, ops: [{ SpawnEntity: { entity: { id, name: `shared-${id}` } } }] });
  }

  // Main: 1–4 entries — spawns from the same counter the branch will use
  // (so some collide), deletes of its own spawns (spent ids), the odd
  // ModifyWorld or Batch.
  const main = [...shared];
  const mainSpawned = [];
  const mainCount = 1 + Math.floor(rand() * 4);
  for (let m = 0; m < mainCount; m += 1) {
    revision += 1;
    const ops = [];
    const spawns = 1 + Math.floor(rand() * 3);
    for (let s = 0; s < spawns; s += 1) {
      const id = nextId; nextId += 1;
      mainSpawned.push(id);
      ops.push({ SpawnEntity: { entity: { id, name: mainName(id) } } });
    }
    if (mainSpawned.length > 0 && maybe(0.35)) {
      const doomed = mainSpawned.pop();
      ops.push({ DeleteEntity: { id: doomed } });
    }
    if (maybe(0.25)) {
      const batched = ops.splice(0, ops.length);
      ops.push({ Batch: { ops: batched } });
    }
    main.push({ revision, message: `main-${m}`, ops });
  }
  if (mainSpawned.length > 0 && maybe(0.3)) {
    revision += 1;
    main.push({ revision, ops: [
      { ModifyWorld: { patch: { avatar: { model_entity: mainSpawned[0] } } } },
    ] });
  }

  // The branch forked after the shared prefix: it allocates from the
  // counter where the fork left it — the ids main took are its collisions.
  let branchId = baseCount + 1 + sharedCount;
  const branch = [];
  const branchSpawned = [];
  let branchRevision = sharedCount; // the branch forked after the shared prefix
  const branchCount = 1 + Math.floor(rand() * 4);
  for (let b = 0; b < branchCount; b += 1) {
    branchRevision += 1;
    const ops = [];
    const spawns = 1 + Math.floor(rand() * 3);
    for (let s = 0; s < spawns; s += 1) {
      const id = branchId; branchId += 1;
      branchSpawned.push(id);
      const entity = { id, name: pick(NAME_POOL) };
      const earlier = branchSpawned.slice(0, -1);
      if (earlier.length > 0 && maybe(0.5)) entity.parent = pick(earlier);
      if (earlier.length > 0 && maybe(0.4)) {
        entity.behaviors = pick([
          [{ Orbit: { center: pick(earlier), radius: 2, speed: 10 } }],
          [{ LookAt: { target: pick(earlier) } }],
        ]);
      }
      ops.push({ SpawnEntity: { entity } });
    }
    if (branchSpawned.length > 0 && maybe(0.4)) {
      const target = pick(branchSpawned);
      // Parent patches point only at earlier spawns (acyclic); name
      // patches use a guaranteed-free name (a taken one would refuse).
      const earlier = branchSpawned.filter((id) => id < target);
      const patch = earlier.length > 0 && maybe(0.7) ? { parent: pick(earlier) } : { name: `renamed-${target}` };
      ops.push({ ModifyEntity: { id: target, patch } });
    }
    if (branchSpawned.length > 0 && maybe(0.3)) {
      ops.push({ ModifyWorld: { patch: {
        avatar: { model_entity: pick(branchSpawned) },
        creations: [{ id: b + 1, name: `set-${b}`, entities: [...branchSpawned] }],
      } } });
    }
    if (maybe(0.25)) {
      const batched = ops.splice(0, ops.length);
      ops.push({ Batch: { ops: batched } });
    }
    branch.push({ revision: branchRevision, author: { peer: 2, name: "branch" }, message: `branch-${b}`, ops });
  }

  return {
    name: `random-${String(seed).padStart(4, "0")}`,
    description: `Seeded random case (seed ${seed}): shared prefix, concurrent spawns, deletes, batches, ModifyWorld refs, name collisions.`,
    base,
    main,
    branch,
  };
}

// ---------------------------------------------------------------------------

mkdirSync(root, { recursive: true });
const cases = [...handwritten];
for (let seed = 1; seed <= 100; seed += 1) cases.push(randomCase(seed));

for (const caseDef of cases) {
  const solved = solve(caseDef);
  writeFileSync(path.join(root, `${solved.name}.json`), `${JSON.stringify(solved, null, 2)}\n`);
}

// A stale case the generator no longer writes would keep passing nothing:
// drop every case file this run didn't write.
const written = new Set(cases.map((c) => `${c.name}.json`));
for (const file of readdirSync(root)) {
  if (file.endsWith(".json") && !written.has(file)) {
    throw new Error(`stale case file ${file} — remove it or keep its seed`);
  }
}
console.log(`wrote ${cases.length} merge cases to ${root}`);
