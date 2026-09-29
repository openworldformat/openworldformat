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

Edits are `SpawnEntity`, `DeleteEntity`, `ModifyEntity` (an entity id
plus a field patch), `SetEnvironment`, `SetCamera`, `SetAmbience`,
`SpawnAudioEmitter`, `RemoveAudioEmitter`, and `Batch` (all-or-nothing).
Deleting an entity deletes its descendants. Every edit has a computable
inverse; **undo is appending the inverse** — the log never rewinds.

## Compatibility

New op kinds MUST fold to nothing for readers that don't know them, and
readers MUST keep reading logs written before a kind existed. The
reference rule: op kinds are recognized by shape, edits first — so a log
containing only edits (every log written before this format had history
kinds) parses unchanged, and an edit serializes today exactly as it
always did.

## Snapshots

A snapshot is the folded document written to `snapshots/rev-<N>.json`:
derived, never authoritative, deletable without loss — the fold from the
base reaches the same state. Compaction (folding to a new base and
keeping the old log for history) changes nothing observable.

## Replay

- **Structural replay** — fold to any revision: exact by construction.
- **Playthrough replay** — re-run triggers offline over `input` and
  `state` entries: faithful, not bit-exact (input is sampled).
- The spec defines **semantic replay** only. Bit-exact cross-engine
  replay is not achievable and MUST NOT be promised by implementations
  of this format.
