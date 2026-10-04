# openworldformat (npm)

The Open World Format reference package. The core — parse a `.world`
manifest, fold its session log — is pure JavaScript with no dependencies:
the document semantics, exactly as [the spec](https://openworldformat.org)
states them, in the smallest implementation that passes the format's own
tests. The reference three.js renderer rides along under
`openworldformat/render`.

The reference 3D renderer ships in the same package as
`openworldformat/render` (three.js is a peer dependency):

```js
import { createWorldViewer } from "openworldformat/render";
const viewer = createWorldViewer(container, manifest, { assetBase: "assets/" });
```

`createWorldViewer` draws a manifest the way the conformance suite pins
it and returns `sceneInfo()`, `startTour`, `toggleAudio`, `applyOps`
(for live session ops) and `dispose`. Provenance: copied verbatim from
the LocalGPT web viewer, which rendered the suite in production before
this repository existed; when this copy becomes upstream, the LocalGPT
apps consume it from here instead of keeping a second copy.

## Install

```bash
npm install openworldformat
```

## Use

```js
import { parseManifest, parseLogLine, foldLog, toManifest } from "openworldformat";

// A head-first package: manifest.json is the world now; a viewer stops here.
const world = parseManifest(await fs.readFile("world/manifest.json", "utf8"));

// The log folds over snapshots/base.json back to it.
const base = parseManifest(await fs.readFile("world/snapshots/base.json", "utf8"));
const entries = (await fs.readFile("world/ops.jsonl", "utf8"))
  .split("\n")
  .filter((l) => l.trim() !== "")
  .map(parseLogLine);

const state = foldLog(base, entries);
state.entities; // the world at head revision
state.appliedEdits; // how many edits the log held
toManifest(state); // the whole manifest again — the fold is total
```

The fold applies the spec's rules: only `edit` ops change the document;
`tool`, `input`, `state` and `clock` fold to nothing; a `Batch` applies
all-or-nothing; ids and names stay unique; deleting an entity deletes its
descendants; it stops at the first entry that no longer applies.

## Types

The package ships TypeScript declarations, and the source stays what it
always was: the types are JSDoc in the JavaScript, checked `strict` and
emitted to `dist/*.d.ts` by `npm run build:types` (publishing runs it —
`prepublishOnly`). No build step touches the code that runs.

```ts
import { foldLog, type WorldManifest, type LogEntry } from "openworldformat";
```

The exported types speak the schema's language — `Vec3`, `WorldEntity`,
`WorldManifest`, `LogEntry`, `FoldState`, `SimulationResult`,
`ViewerOptions` — so `state.entities[0].transform?.position` is a
`Vec3`, not `any`. `openworldformat/physics` follows the same rule.
Checking code that uses `openworldformat/render`'s types also needs
`@types/three` (a dev dependency, like it is here); the runtime is
unchanged — peer `three`, nothing else.

## API

| Export | What it does |
|---|---|
| `parseManifest(json, opts?)` | parse and version-check a world document (`{strict: true}` for authoring tools) |
| `parseLogLine(line, opts?)` | parse one `ops.jsonl` line, ops classified (strict refuses unknown shapes) |
| `classifyOp(op)` | recognize an op by shape — edits first |
| `opKindShapeOk(kind)` | the shape collision rule: edits PascalCase, history kinds lowercase |
| `editOps(entry)` | an entry's edits, in order |
| `foldLog(manifest, entries)` | the document at the last entry (the linear fold) |
| `toManifest(state)` | the fold's state as a whole manifest — `toManifest(foldLog(m, []))` is `m` again |
| `ingest(state, batch)` | the Authoring profile: a batch by name, partial patches merged, read strictly, all or nothing |
| `mergePatch(current, change)` | JSON Merge Patch (RFC 7396), as ingestion applies it |
| `manifestText(manifest)` | the canonical text an authority writes (byte-identical to the Rust reference's) |
| `buildHistory(entries)` | ids, parents, children and tips of a branching log |
| `foldPath(manifest, entries, tip?)` | the document at a tip — fork anywhere, fold that path |
| `foldState(stateDoc, entries)` | the game state at the last entry (declared fields, map subkeys, tolerant of the undeclared) |
| `canonicalJson(value)` | deterministic serialization — sorted keys, no whitespace — the hash input |
| `computeEntryId(entry)` | `sha256:<hex>` of the canonical entry, its own `id` excluded |
| `computeInverse(op, state)` | the inverse of one edit op — undo is appending it |
| `mergeBranch(state, entries)` | remap a branch's colliding ids onto the main head, rewriting every reference |
| `snapshotFilename(entryId, revision)` | where a snapshot lives, the entry id sanitized |
| `compactPackage(packageJson, headRevision)` | the `package.json` half of compaction (`base_revision` to head) |
| `extProvenance(manifest)` | read `meta["ext-provenance"]` lineage, or null |
| `SUPPORTED_SCHEMA_VERSION` | the manifest schema this reads (3) |
| `SUPPORTED_FORMAT_VERSION`, `BASE_SNAPSHOT` | the package format (2, head-first) and where its base lives |
| `WORLD_PATCH_KEYS` | the fields `ModifyWorld` reaches |
| `MAX_ENTITY_ID` | the id ceiling, 2^53 − 1 |
| `REGISTERED_EXTENSIONS` | the extensions the registry has accepted |

The fold binds by-name behavior references to ids at ingestion (a later
rename can't re-bind them), refuses ids past `MAX_ENTITY_ID`, and stops
at the first entry that no longer applies — the rules
[the spec](https://openworldformat.org) states. Strict mode is for
authoring tools and validators; runtime consumers keep must-ignore.

## The physics extension (`openworldformat/physics`)

The reference implementation of [`ext-physics`](../spec/extensions/physics.md)
0.1: bodies are declared, never simulated by the document — but the
contract needs an executable half, and this is it.

```js
import { simulatePhysics, trajectoryOp, foldTrajectories, runOutcomes }
  from "openworldformat/physics";

const sim = simulatePhysics(manifest, { until_s: 6 }); // deterministic
sim.contacts;  // impacts: {t_s, body, other, position, normal_speed}
sim.resting;   // where each dynamic body ended up
const op = trajectoryOp(sim);       // the playback op — ≤10 Hz samples
const track = foldTrajectories(entries); // scrub it back, no solver
runOutcomes(manifest, outcomesDoc); // the conformance assertions
```

The solver is deliberately minimal — spheres against floors and
axis-aligned statics, fixed 1/120 s semi-implicit Euler, sleep on rest —
so the conformance outcome assertions run anywhere and in CI. It is
reference-grade, not production physics; conforming engines use real
ones. Deterministic within an engine (no randomness, document order,
IEEE-754 doubles); across engines, only the semantic contract holds —
never bit-exact, per the spec.

## Test

```bash
npm test
```

Tests fold the repository's own `examples/hello-world` package and pin
the compatibility rules (old-format lines, history kinds, torn lines).
Apache-2.0.
