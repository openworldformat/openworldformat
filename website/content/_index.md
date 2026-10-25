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

## Tools that use it {#tools}

Packages are written and read outside this repository, each tool riding
one of the references above.

<ul class="packages">
<li><strong>worldprobe</strong> — the desktop inspector (<a href="https://github.com/yilab/worldprobe">source</a>): drop a package folder and see the fold, the validation verdict, the branches, a diff between any two points of the history, the state and the physics. Over the Rust crate, adding no format semantics of its own.</li>
<li><strong>worldwalk</strong> — a viewer for macOS, iOS, iPadOS and visionOS (SwiftUI and RealityKit, over the Swift package) and Android (Compose, over the Kotlin package) (<a href="https://github.com/yilab/worldwalk">source</a>): open a package, pick a branch tip, walk the world it folds to. It draws what the fold says and nothing more.</li>
<li><strong>LocalGPT</strong> — local apps that write packages from a prompt, a Markdown document or a song, and open any package as a live canvas that agents edit from outside (<a href="https://localgpt.app">localgpt.app</a>, over the Rust crate). <a href="https://localgpt.md">localgpt.md</a> drafts a screenplay into one package per scene in the browser, and <a href="https://localgpt.world">localgpt.world</a> plays worlds in the reference renderer.</li>
</ul>

## Status

Draft 0.3, describing manifest schema version 3 and package format 2
(head-first). The schema, the conformance suite and the five reference
implementations [above](#packages) are in the
[repository](https://github.com/openworldformat/openworldformat), tested
in CI. Expect churn until 1.0; the
[versioning policy](spec/versioning/) is the contract.

Since draft 0.3, within schema version 3 — every addition optional, so a
reader that predates it stays conforming by ignoring it:

- **Receipts for agents.** A batch may name the revision it read, the
  authority's reply is a receipt, and every reference runs one
  [authoring outcome suite](https://github.com/openworldformat/openworldformat/tree/main/conformance/authoring)
  ([the session log](spec/session/#authoring)).
- **The geometry of every shape**, stated here rather than inherited
  from an engine, and pinned by landmark points any renderer can check
  ([the world document](spec/world/#the-geometry-of-the-shapes)).
- **`Prism`**, a prism with a side count, 3 to 8 —
  experimental ([see it](world.html?src=conformance/prism.json)).
- **Exports from other systems.** `author.source` says how each batch
  came to be — `hand`, `library` or `ai` — and a producer that keeps its
  own live truth can export a world with the history that built it
  ([the package](spec/package/#exports-from-other-systems)).
- **Extensions.** [`ext-cinematography`](https://github.com/openworldformat/openworldformat/blob/main/spec/extensions/cinematography.md)
  is experimental at 0.3 — cameras with a filmback and a lens, shots
  aimed and focused by entity — in four references;
  [`ext-h3`](https://github.com/openworldformat/openworldformat/blob/main/spec/extensions/h3.md),
  a geo frame for worlds that live on a globe, is proposed.
