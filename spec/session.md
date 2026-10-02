# The session log (L1)

`ops.jsonl` is the one history: one JSON entry per line, each entry a
committed batch of ops by one author. It holds what changed the world
(edits) and what happened around the change (tool calls, visitor input,
state deltas, clocks). The room's document is the fold of this log over
the base — nothing else is authoritative.

## The entry

```json
{"revision": 42, "timestamp_ms": 1790000000123,
 "author": {"peer": 3, "name": "maya"},
 "ops": [ … ]}
```

`revision` is the document revision after the entry's **edits** applied.
Only edits bump it; history-only entries carry the current revision, so
readers MUST tolerate repeated and non-monotonic-looking revisions
between edit entries. Edit entries themselves are totally ordered.

`author` is who or what did it — a person, a model, a visitor, a host.
Authorship is what makes the log an audit trail: "what did the model do
here, versus the people?" is a filter on one field.

## The op kinds

An entry's `ops` hold any mix of five kinds. Each kind is serialized as
its own fields (there is no wrapping tag object); an op is recognized by
its shape:

```json
{"SpawnEntity": {"entity": {"id": 1, "name": "lighthouse", "transform": …}}}
```
```json
{"tool": "gen_spawn_primitive", "args": {…},
 "result_hash": "sha256:…", "phase": "blockout", "timestamp_ms": 1790000000131}
```
```json
{"input": {"actor": "visitor-7", "position": [3.1, 1.8, -2.0],
           "look": [12.0, -4.0], "click": 17}}
```
```json
{"state": {"score.chest": 10}}
```
```json
{"clock": {"playing": true, "position_s": 41.5}}
```

| Kind | Who writes it | Changes the document? |
|---|---|---|
| `edit` (an `EditOp`) | the session authority, for anyone | **yes** — the only kind that does |
| `tool` | an app, when a tool call runs | no — intent, recorded next to the edits it caused |
| `input` | an app, sampled (~10 Hz) while recording | no — playthrough replay |
| `state` | an app, when host state changes | no — the game state no document holds |
| `clock` | an app, on transport events | no — performances, song worlds, tours |
| `merge` | an app, merging a fork | no — provenance that a batch came from a branch |

Edits are `SpawnEntity`, `DeleteEntity`, `ModifyEntity` (an entity id
plus a field patch, where an absent key means unchanged, `null` means clear/remove, and a value sets it), `SetEnvironment`, `SetCamera`, `SetAmbience`,
`SpawnAudioEmitter`, `RemoveAudioEmitter`, and `Batch` (all-or-nothing).
Deleting an entity deletes its descendants. Every edit has a computable
inverse; **undo is appending the inverse** — the log never rewinds.
The inverse is computed at the time of the edit:
- `SpawnEntity` inverses to `DeleteEntity`.
- `DeleteEntity` inverses to a `Batch` of `SpawnEntity` ops containing a deep copy of the deleted tree.
- `ModifyEntity` inverses to a `ModifyEntity` restoring the old values.

## Compatibility

To prevent future collisions, the shape collision rule applies: **Edits MUST be PascalCase, History kinds MUST be lowercase.**

New op kinds MUST fold to nothing for readers that don't know them, and
readers MUST keep reading logs written before a kind existed. The
reference rule: op kinds are recognized by shape, edits first — so a log
containing only edits (every log written before this format had history
kinds) parses unchanged, and an edit serializes today exactly as it
always did.

### Folding and Invalid Operations
During a fold, an operation may "no longer apply" to the current state. Such operations are skipped without failing the fold. The error taxonomy includes:
- **Entity not found**: Attempting to modify or delete an entity that does not exist.
- **Invalid patch**: A `ModifyEntity` patch that does not match the schema or attempts an invalid field transition.
- **Already exists**: Attempting to spawn an entity with an ID that is currently in use.

## Entry identity, forks and branches

An entry may carry an `id` (its identity — content hashes recommended,
opaque to readers) and a `parent` (the entry it builds on). A **branch**
is an entry whose parent already has a child; a **tip** is an entry that
is nobody's parent. Folding generalizes: **state at any tip is a fold of
the path** — walk parent links from tip to base, fold that chain. A log
with no ids is a chain in file order (a reader synthesizes
`line-<n>` ids), so branching is purely additive and old readers see a
linear prefix. `revision` stays the room's total order; `parent` records
causality — sequence is not causality.

Forks and refs live in `package.json` ([the package](package.md)); the
`merge` op records where a merged batch came from. When merging a branch, the merge authority must handle ID collisions. If the branch introduces entities with IDs that were concurrently allocated on the main branch, the merge authority MUST reallocate those colliding IDs and rewrite all their references within the merged batch. See
[`rfcs/branching-histories.md`](rfcs/branching-histories.md).

## Snapshots

A snapshot is the folded document written to `snapshots/entry-<id>.json` (or `snapshots/rev-<N>.json` for linear logs without explicit ids):
derived, never authoritative, deletable without loss — the fold from the
base reaches the same state. Compaction (folding to a new base and
keeping the old log for history) changes nothing observable.

## Replay and determinism

Three levels, and only the first two are part of this format:

1. **Structural replay** — fold to any revision. Exact by construction:
   the fold *is* the document's history. Anything that folds to the same
   state — on any machine, in any language — is conformant here.
2. **Semantic replay** — re-run triggers and behaviors offline over the
   log's `input` and `state` entries. The contract: the same fold plus
   the same inputs produce the same trigger outcomes and the same
   state trajectory. Frames are *approximately* the same — input is
   sampled (~10 Hz), behaviors are functions of folded time, and
   renderers draw. During semantic replay, timers observe the session's recorded transport clock. After a seek, a timer observes the clock as dictated by the closest preceding `clock` entry.
3. **Bit-exact replay** — the same pixels. **Not part of this format,
   and MUST NOT be promised by implementations of it.** Floats, physics
   ordering and renderer differences make it a lie waiting to be caught;
   `package.json`'s `seed` is reserved for engines that want to try
   anyway, and means nothing to the contract above.
