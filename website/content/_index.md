+++
title = "Open World Format"
+++

<div class="hero">
  <h1>An open format for worlds</h1>
  <p class="tagline">The scene, the history that built it, and what happened in it — one <code>.world</code> package anyone can read.</p>
</div>

A `.world` is a folder (a zip is its transport form) holding a world
document, a typed state document, an append-only session log, and
content-addressed assets. State at any revision is a pure fold of the
log over the base — which makes rendering, editing, multiplayer, undo,
save games, replays and mods all reads over one history.

It is the layer **above** glTF, not a competitor: meshes stay glTF
leaves, referenced by content hash. glTF owns transport; `.world` owns
composition, parametric authoring, behavior, audio, game state and
sessions.

## Read the spec

1. [The world document](spec/world/) — what a world *is*
2. [The package](spec/package/) — the folder, assets by hash, integrity
3. [The session log](spec/session/) — the ops that build a world, and the history around them
4. [Profiles and extensions](spec/profiles/) — must, may, must-ignore
5. [Versioning policy](spec/versioning/) — the compatibility contract

The [JSON Schema](https://github.com/openworldformat/openworldformat/blob/main/schema/world.schema.json)
is normative; the [conformance worlds](https://github.com/openworldformat/openworldformat/tree/main/conformance)
define correct rendering — and you can watch them draw:
**[open the live viewer](world.html?src=conformance/shapes.json)**, the
reference three.js renderer running the whole suite in this tab. The same
page opens any manifest: `world.html?src=…`.

## Try it in a minute

```bash
npm install openworldformat
```

```js
import { parseManifest, parseLogLine, foldLog, toManifest } from "openworldformat";

// manifest.json is the world now — a viewer needs nothing else.
const world = parseManifest(await fs.readFile("world/manifest.json", "utf8"));

// The history folds over snapshots/base.json back to the same world.
const base = parseManifest(await fs.readFile("world/snapshots/base.json", "utf8"));
const entries = (await fs.readFile("world/ops.jsonl", "utf8"))
  .split("\n").filter(Boolean).map(parseLogLine);
const head = toManifest(foldLog(base, entries)); // the same world
```

Or in Python, where the same fold is pure standard library:

```python
from pathlib import Path
from openworldformat import read_package, fold_log

base, entries, head, package = read_package(Path("world"))
state = fold_log(base, entries)   # the world at head revision
state["entities"], state["applied_edits"]
```

Agents change a world from outside the app that shows it: they send
batches of edit ops to the package's one authority, which binds names,
merges partial patches, reads strictly and commits all or nothing
([the session log](spec/session/), "Authoring").

## Get it {#packages}

Five references parse and fold a `.world`. Each runs the same conformance
worlds and the same [authoring outcome suite](https://github.com/openworldformat/openworldformat/tree/main/conformance/authoring),
so a fold — and a receipt — reads the same in every one.

<ul class="packages">
<li><strong>JavaScript</strong> — <code>npm install openworldformat</code> (<a href="https://www.npmjs.com/package/openworldformat">npm</a>). The reference three.js renderer ships in the package as <code>openworldformat/render</code>; it is what the <a href="world.html?src=conformance/shapes.json">viewer</a> runs.</li>
<li><strong>Python</strong> — <code>pip install openworldformat</code> (<a href="https://pypi.org/project/openworldformat/">PyPI</a>). Pure standard library and deliberately no renderer: the research surface, for benchmarks, dataset tooling over session logs, and notebooks.</li>
<li><strong>Rust</strong> — <code>cargo add openworldformat</code> (<a href="https://crates.io/crates/openworldformat">crates.io</a>, <a href="https://docs.rs/openworldformat">docs.rs</a>). The types, the fold and the branches, game state, and the <code>ext-physics</code> and <code>ext-cinematography</code> extensions' executable halves; serde only, no engine.</li>
<li><strong>Swift</strong> — <code>.package(url: "https://github.com/openworldformat/openworldformat", from: "0.3.5")</code>, product <code>OpenWorldFormat</code>. macOS, iOS, iPadOS and visionOS; released on bare <code>vX.Y.Z</code> tags of the repository.</li>
<li><strong>Kotlin</strong> — <a href="https://github.com/openworldformat/openworldformat/tree/main/kotlin"><code>kotlin/</code></a>, JVM and Android (minSdk 26), built from source as a Gradle composite build until it reaches Maven Central.</li>
</ul>

## Status

Draft 0.3, describing manifest schema version 3 and package format 2
(head-first). The schema, the conformance suite and the five reference
implementations [above](#packages) are in the
[repository](https://github.com/openworldformat/openworldformat), tested
in CI. Expect churn until 1.0; the
[versioning policy](spec/versioning/) is the contract.
