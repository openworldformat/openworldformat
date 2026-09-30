# Open World Format

**An open format for `.world` packages: interactive, AI-authorable 3D
worlds — the scene, the history that built it, and what happened in it.**

A `.world` is a folder (a zip is its transport form) that holds a world
document, a typed state document, an append-only session log, and
content-addressed assets. State at any revision is a pure fold of the log
over the base. That one invariant gives you rendering, editing,
multiplayer, undo, save games, replays and mods as reads over one history.

It is the layer **above** glTF, not a competitor to it: meshes stay glTF
leaves, referenced by content hash. glTF owns transport; `.world` owns
composition, parametric authoring, behavior, audio, game state and
sessions.

## Status

Draft 0.2, describing schema version 3. The schema, the conformance
worlds and a reference fold implementation are in this repository and
tested in CI; two renderers (Bevy, three.js) already draw the conformance
suite in the format's origin project. Expect churn until 1.0; the
[versioning policy](spec/versioning.md) is the contract.

## Repository map

| Path | What it is |
|---|---|
| [`spec/`](spec/) | The specification (start at [`spec/README.md`](spec/README.md)) |
| [`schema/`](schema/) | The normative JSON Schema (`world.schema.json`) |
| [`conformance/`](conformance/) | Conformance worlds — compliance means rendering these |
| [`examples/`](examples/) | Example `.world` packages |
| [`viewer/`](viewer/) | The reference npm package: parse and fold, no engine required |
| [`website/`](website/) | [openworldformat.org](https://openworldformat.org) (Zola, no theme) |

## Try it in a minute

```bash
npm install openworldformat
```

```js
import { parseManifest, parseLog, foldLog } from "openworldformat";

const manifest = parseManifest(await fs.readFile("examples/hello-world/manifest.json", "utf8"));
const entries = (await fs.readFile("examples/hello-world/ops.jsonl", "utf8"))
  .split("\n").filter(Boolean).map(parseLog);
const world = foldLog(manifest, entries); // the world at head revision
```

## License

Apache-2.0 today, for everything. Splitting the spec text to CC-BY 4.0
(code stays Apache-2.0) is a planned governance step; see
[`CONTRIBUTING.md`](CONTRIBUTING.md) for the process before that lands.

## Lineage

The format was extracted from the open-source LocalGPT apps (Gen, MD,
Verse), which remain its reference producers. The Open World Format is
governed here, independently of any producer.
