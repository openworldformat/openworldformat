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
import { parseManifest, parseLogLine, foldLog } from "openworldformat";

const manifest = parseManifest(await fs.readFile("world/manifest.json", "utf8"));
const entries = (await fs.readFile("world/ops.jsonl", "utf8"))
  .split("\n")
  .filter((l) => l.trim() !== "")
  .map(parseLogLine);

const state = foldLog(manifest, entries);
state.entities; // the world at head revision
state.appliedEdits; // how many edits the log held
```

The fold applies the spec's rules: only `edit` ops change the document;
`tool`, `input`, `state` and `clock` fold to nothing; a `Batch` applies
all-or-nothing; ids and names stay unique; deleting an entity deletes its
descendants; it stops at the first entry that no longer applies.

## API

| Export | What it does |
|---|---|
| `parseManifest(json)` | parse and version-check a world document |
| `parseLogLine(line)` | parse one `ops.jsonl` line, ops classified |
| `classifyOp(op)` | recognize an op by shape — edits first |
| `editOps(entry)` | an entry's edits, in order |
| `foldLog(manifest, entries)` | the document at the last entry |
| `foldState(stateDoc, entries)` | the game state at the last entry (declared fields, map subkeys, tolerant of the undeclared) |
| `SUPPORTED_SCHEMA_VERSION` | the manifest schema this reads (3) |

## Test

```bash
npm test
```

Tests fold the repository's own `examples/hello-world` package and pin
the compatibility rules (old-format lines, history kinds, torn lines).
Apache-2.0.
