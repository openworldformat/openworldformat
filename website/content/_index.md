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
define correct rendering.

## Try it in a minute

```bash
npm install openworldformat
```

```js
import { parseManifest, parseLogLine, foldLog } from "openworldformat";

const manifest = parseManifest(await fs.readFile("world/manifest.json", "utf8"));
const entries = (await fs.readFile("world/ops.jsonl", "utf8"))
  .split("\n").filter(Boolean).map(parseLogLine);
const state = foldLog(manifest, entries); // the world at head revision
```

## Status

Draft 0.1, describing manifest schema version 3. The schema, the
conformance suite and a reference fold implementation are in the
[repository](https://github.com/openworldformat/openworldformat), tested
in CI. Expect churn until 1.0; the
[versioning policy](spec/versioning/) is the contract.
