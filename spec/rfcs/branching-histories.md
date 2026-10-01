# RFC: Branching histories — forks, refs and merge provenance

**Status:** implemented in the reference layer (the JS fold and the Rust
session layer); the first producer to write branches is
[`examples/speedrun-fork`](../../examples/speedrun-fork/) — a challenge
chain where each run is a tip — but no app writes branches yet. Adds to
draft 0.2; nothing here changes what a linear log means.

## The problem

A session is one linear log: one authority, one revision counter, one
trunk. But world-building is exploratory — a model tries three castle
layouts, a player tests a variant, an editor experiments — and today
those alternatives either vanish or overwrite the trunk. The ask: a
history you can **fork at any point, develop independently, and scrub as
a tree** — git's model, where every commit is an op batch; an emulator
save-state tree, for worlds.

## The design in one paragraph

Entries gain optional `id` and `parent` fields. A branch is an entry
whose `parent` is an entry that already has a child; a *tip* is an entry
that is nobody's parent. Folding generalizes from "state at any revision
is a fold of the log" to **"state at any tip is a fold of the path"** —
walk parent links from tip to base, fold that chain. Everything else
(snapshots, compaction, the must-ignore rule, linear logs) is unchanged.

## Entry identity

```json
{"id": "e3", "parent": "e2", "revision": 3, "author": …, "ops": […], "timestamp_ms": …}
```

- `id` — the entry's identity, a string. Content hashes are the
  recommended form (writers may compute them); readers treat ids as
  opaque and never compute them.
- `parent` — the entry this one builds on. Absent on the first entry.
- **Compatibility:** a log with no ids is a chain in file order — a
  reader synthesizes `id: line-<n>` and `parent: line-<n-1>`. A log that
  mixes both is fine. This is the format's must-ignore discipline applied
  to history: old readers see a linear prefix, new readers see the tree.
- `revision` remains the *room's* linear order (multiplayer needs a total
  order); `parent` records *causal* order. Two different things, both
  kept — sequence is not causality.

## Packages: forks, refs, merge provenance

- **`refs`** (optional, `package.json`): named tips —
  `"refs": {"main": "e3", "moat-variant": "e5"}`. The room's trunk is
  conventionally `main`. Refs are pointers, never data; deleting one
  deletes nothing.
- **Forks**: the copy form (base + a prefix) remains valid; the
  reference form adds `forked_from` to `package.json` — provenance for
  "this package began as that one, at that entry".
- **`merge` op kind**: provenance that folds to nothing, like `tool` and
  `clock`: `{"merge": {"branch": "moat-variant"}}`. The merged edits
  arrive as normal edit entries; the merge record says where they came
  from, so the tree shows convergences. Merge *semantics* stay the
  document's own: per-entity field patches are the conflict unit, the
  same unit the multiplayer authority already linearizes — a room is
  concurrent branches with an arbiter; merging forks is the same
  machinery run offline.
- **Content-addressed snapshots** (optimization, not required): a
  snapshot keyed by the entry it folds from, hashed over the folded
  document, lets two branches that reach the same state confirm
  convergence without replay — git's tree-equality trick. The reference
  layer does not do this yet.

## Replay efficiency

Folding a path costs O(path length) — same as linear. Branches share
prefixes, and content-addressed entries dedupe storage across forks
naturally (the same exploration step on two forks is one object). Seek
within a branch uses that branch's nearest snapshot. The new cost is
*visualizing* a large tree — a UI concern (the branch rail), not a
format concern.

## What stays out

No CRDT: forks are explicit and merge is validated against the document,
not merged algebraically. No garbage collection semantics yet (dead
branches are pruned app-side; entries referenced by a live ref must
survive). No change to the linear log's meaning.

## Implementation status

- **JS reference** (`js/src/index.js`): `buildHistory` (ids, parents,
  children, tips), `foldPath(manifest, entries, tip?)`, and the `merge`
  op kind in `classifyOp`. Tested over
  `examples/forked-exploration/`, whose log holds a real fork.
- **Rust reference** (LocalGPT's `world-sync`/`world-agent`):
  `OpLogEntry` carries `id`/`parent`; `MergeRecord` folds to nothing;
  `fold_path` walks a tip; `fork_package(from, to, at)` writes the copy
  form with `forked_from` provenance.
- Not yet: branch-aware UI (the viewer's branch rail is the scrub
  controls item, upgraded), a model writing to scratch branches, the
  hashed-entry store (`history/<sha>` objects with dedup).
