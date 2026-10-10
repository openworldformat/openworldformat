# The session log (L1)

`ops.jsonl` is the one history: one JSON entry per line, each entry a
committed batch of ops by one author. It holds what changed the world
(edits) and what happened around the change (tool calls, visitor input,
state deltas, clocks). The world at any tip is the fold of this log over
the base — and `manifest.json` is that fold at the tip of `main`, written
down ([the package](package.md), "Head-first").

## The entry

```json
{"revision": 42, "timestamp_ms": 1790000000123,
 "author": {"peer": 3, "name": "maya"},
 "message": "a lantern by the gate",
 "ops": [ … ]}
```

`revision` is the document revision after the entry's **edits** applied.
Only edits bump it; history-only entries carry the current revision, so
readers MUST tolerate repeated and non-monotonic-looking revisions
between edit entries. Edit entries themselves are totally ordered.

`author` is who or what did it — a person, a model, a visitor, a host.
Authorship is what makes the log an audit trail: "what did the model do
here, versus the people?" is a filter on one field.

`message` (optional) is what the author says the batch is for — a commit
message. It folds to nothing, is part of the entry's identity, and is the
subject of the git commit the batch becomes in a git-backed package.

## The op kinds

An entry's `ops` hold any mix of these kinds. Each kind is serialized as
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
| `input` | an app, sampled at a normative rate of exactly 10 Hz while recording | no — playthrough replay |
| `state` | an app, when host state changes | no — the game state no document holds |
| `clock` | an app, on transport events | no — performances, song worlds, tours |
| `merge` | an app, merging a fork | no — provenance that a batch came from a branch: `{"merge": {"branch": "moat-variant"}}` |

Edits are `SpawnEntity`, `DeleteEntity`, `ModifyEntity` (an entity id
plus a field patch, where an absent key means unchanged, `null` means
clear/remove, and a value sets it), `SetEnvironment`, `SetCamera`,
`SetAmbience`, `SpawnAudioEmitter`, `RemoveAudioEmitter`, `ModifyWorld`,
and `Batch` (all-or-nothing). Deleting an entity deletes its descendants.

`ModifyWorld` patches the document's scene-wide fields with the same
rule as `ModifyEntity` — absent unchanged, `null` clears, a value sets:

```json
{"ModifyWorld": {"patch": {"meta": {"name": "harbor", "description": "…"},
                           "tours": [ … ], "soundtrack": null}}}
```

Its patch reaches `meta`, `environment`, `camera`, `avatar`, `tours`,
`soundtrack`, `ambience` and `creations`. `meta` is replaced whole and
can't be cleared (a world always has a name); clearing a list field
leaves it empty. Keys it doesn't name are ignored (must-ignore).

Every edit has a computable inverse; **undo is appending the inverse** —
the log never rewinds. The inverse is computed at the time of the edit,
against the document it is about to change:
- `SpawnEntity` inverses to `DeleteEntity`.
- `DeleteEntity` inverses to a `Batch` of `SpawnEntity` ops containing a deep copy of the deleted tree, parents first.
- `ModifyEntity` inverses to a `ModifyEntity` restoring the old values (clearing fields the entity didn't have).
- `ModifyWorld` inverses to a `ModifyWorld` restoring the old values of the fields it touches.
- `SetEnvironment` and `SetCamera` inverse to the setting they replace — or, when the document had none, to a `ModifyWorld` clearing it. `SetAmbience` inverses to the ambience it replaces.
- `SpawnAudioEmitter` inverses to `RemoveAudioEmitter`; `RemoveAudioEmitter` inverses to a `SpawnAudioEmitter` carrying the emitter it removed.
- A `Batch` inverses to its ops' inverses in reverse order, each computed against the state its op was about to change.

## The fold is total

The fold's state is a whole manifest: every field a manifest holds is
carried through the fold, and every one is reachable by an edit. For any
world `m`, folding an empty log gives `m` back — equal as worlds, up to
names bound to ids at ingestion, entity order, an absent field the same as
`null`, and numbers compared by value. The conformance suite holds every
reference to this for every conformance world and every example
package's base and head.

`next_entity_id` only grows. Ids are monotonic and never reused, so the
fold carries the larger of the base's declared value and one past every
id it has seen — deleting the newest entity does not free its id — and
a manifest's effective `next_entity_id` is the larger of its declared
value and one past its largest id.

## Authoring

An **authority** — an open app, or a command-line tool when no app is
open — is the one writer of a package. **Authors** (agents, scripts,
people) change the world only by sending it a **batch** of edit ops: a
JSON array, or `{"ops": [...], "author": …, "message": …}`. The
authority takes each op in order, against a trial document that already
holds the batch's earlier ops:

1. **Bind.** Where an entity id goes (`ModifyEntity.id`,
   `DeleteEntity.id`, a `parent`), a string is a name: it resolves to the
   id it names now — the identity rule of [the world document](world.md),
   applied to op addresses. A `SpawnEntity` without an `id` gets the next
   one. Committed ops always carry ids.
2. **Merge.** A patch's object-valued struct fields — `ModifyEntity`'s
   `transform`, `material` and `light`; `SetEnvironment`'s `env`;
   `ModifyWorld`'s `meta`, `environment`, `camera`, `avatar` and
   `soundtrack` — merge into the current value as a **JSON Merge Patch**
   (RFC 7396): present keys replace, `null` removes, absent keys keep.
   Send `{"transform": {"position": [0, 2, 0]}}` and the rotation and
   scale stay. The committed op carries the merged whole value, so the
   fold itself never merges.
3. **Read strictly.** No key the format would drop: an unknown field is a
   refusal with a JSON pointer to it, not a silent ignore ([profiles](profiles.md),
   "Strict Mode").
4. **Store assets.** A referenced asset must be a file inside `assets/`;
   it is stored or verified as [the package](package.md), "Assets"
   says.
5. **Apply** to the trial — an op that no longer applies fails here.

Then the world the batch makes must validate. Validation is structural:
the document holds against the schema and every reference resolves —
the same checks the fold's apply step runs. Budget limits — an entity's
extent, a chunk's entity or triangle count, an entity's behavior or
modulation count — are an authority's policy, not the format's: the
authority reports them to the author as warnings, and they never refuse
a batch. **Any structural failure refuses the whole batch**: nothing is
written, and the reply lists a reason for every op that failed. Success
appends one entry — the batch's edits, its author and its message — and
writes the new head.

Undo is a batch like any other: the inverse of the newest batch nobody
has undone, appended. Two authorities never write one package at once;
one that finds another holding the package (`.live/endpoint.json` with a
process that answers) sends its batches there.

## Compatibility

To prevent future collisions, the shape collision rule applies: **Edits MUST be PascalCase, History kinds MUST be lowercase.**

New op kinds MUST fold to nothing for readers that don't know them, and
readers MUST keep reading logs written before a kind existed. The
reference rule: op kinds are recognized by shape, edits first — so a log
containing only edits (every log written before this format had history
kinds) parses unchanged, and an edit serializes today exactly as it
always did.

### Folding and Invalid Operations
During a fold, an operation may "no longer apply" to the current state. The fold stops at the first such entry and names the reason — the same refusal class an authority returns at intake, so a log that replays cleanly folds cleanly and one that cannot never folds halfway in silence. An entry is atomic: if any of its edits no longer applies, none of them commits. The error taxonomy includes:
- **Entity not found**: Attempting to modify or delete an entity that does not exist.
- **Invalid patch**: A `ModifyEntity` patch that does not match the schema or attempts an invalid field transition.
- **Already exists**: Attempting to spawn an entity with an ID that is currently in use.

## Entry identity, forks and branches

An entry may carry an `id` (its identity) and a `parent` (the entry it builds on). To ensure identical IDs across forks, implementations computing content hashes MUST serialize the entry using a canonical JSON format (e.g., no whitespace, keys sorted alphabetically) — every field but `id`, `message` included. A **branch**
is an entry whose parent already has a child; a **tip** is an entry that
is nobody's parent. Folding generalizes: **state at any tip is a fold of
the path** — walk parent links from tip to base, fold that chain. A log
with no ids is a chain in file order (a reader synthesizes
`line-<n>` ids, counting lines from 0), so branching is purely additive and old readers see a
linear prefix. `revision` stays the room's total order; `parent` records
causality — sequence is not causality.

Forks and refs live in `package.json` ([the package](package.md)); the
`merge` op records where a merged batch came from. When merging a branch, the merge authority must handle ID collisions. See
[`rfcs/branching-histories.md`](rfcs/branching-histories.md). In a git-backed package, branches are git branches and the
same merge rules resolve a merge's conflicts ([the package](package.md), "Git").

### The merge rules, exactly

A merge appends the branch's entries to main, rewritten so they apply
cleanly on main's head. The rewriting is deterministic — the same main
head and the same branch entries always produce the same merged entries:

1. **Fresh ids.** An id the branch spawned that the main line currently
   holds is colliding. Colliding ids are reallocated in ascending
   order. Fresh ids start at main's **effective** `next_entity_id` — the
   fold already carries it: the larger of the declared value and one past
   every id main has ever held, deleted ones included — and count up,
   skipping every id the branch spawns. An id main deleted stays spent:
   fresh ids come from that floor, never from the live set.
2. **Every entity reference rewritten.** Within the merged batch, every
   field that holds an entity id is rewritten through the remap:

   | Op | Fields that hold entity ids |
   |---|---|
   | `SpawnEntity` | `entity.id`, `entity.parent`, the numeric `Orbit.center` / `LookAt.target` in `entity.behaviors[]` |
   | `ModifyEntity` | `id`, `patch.parent`, the numeric behavior refs in `patch.behaviors[]` |
   | `DeleteEntity` | `id` |
   | `Batch` | every inner op, recursively |
   | `ModifyWorld` | `patch.avatar.model_entity` (numeric), `patch.creations[].entities[]` |

   These are the schema's marked entity-reference fields
   (`x-entity-ref`, [the world document](world.md) "Identity");
   `schema/entity-refs.json` is the generated list every reference
   reads.

3. **Names.** Two branches that each mint a `lighthouse` merge into one
   world with two of them, and the authority MUST rename the merged side.
   When a branch spawn's name is already taken — by main, or by an
   earlier spawn in the same merge — the merge renames the spawn to
   `<name>-<n>`, `n` counting from 2 up, first unused. Only the
   `SpawnEntity` changes: references are ids by then ([the world
   document](world.md)), so a rename breaks nothing in the log.

The merged entries keep their `id`, `parent`, `author` and `message` —
an entry's identity survives the merge.

## Snapshots

A snapshot is the folded document written to `snapshots/entry-<id>.json` (or `snapshots/rev-<N>.json` for linear logs without explicit ids):
derived, never authoritative, deletable without loss — the fold from the
base reaches the same state. `manifest.json` is the newest keyframe of
all, always present.

Compaction moves the base up: the producer writes the head (or the state
at a chosen revision) as the new `snapshots/base.json`, sets
`package.json`'s `base_revision` to its revision, moves the entries it
now holds to `ops.archive.jsonl` (or deletes them), and keeps the rest in
`ops.jsonl`. `manifest.json` is untouched. Compaction changes nothing
observable about the current state, but truncates structural replay.

## Replay and determinism

Three levels, and only the first two are part of this format:

1. **Structural replay** — fold to any revision. Exact by construction:
   the fold *is* the document's history. Anything that folds to the same
   state — on any machine, in any language — is conformant here. A
   git-backed package also replays as keyframes: each commit's
   `manifest.json`, oldest first, with a renderer stepping from one to the
   next by the edits that turn one into the other.
2. **Semantic replay** — re-run triggers and behaviors offline over the
   log's `input` and `state` entries. The contract: the same fold plus
   the same inputs produce the same trigger outcomes and the same
   state trajectory. Frames are *approximately* the same — input is
   sampled (10 Hz), behaviors are functions of folded time, and
   renderers draw. During semantic replay, timers observe the session's recorded transport clock. After a seek, a timer observes the clock as dictated by the closest preceding `clock` entry.
3. **Bit-exact replay** — the same pixels. **Not part of this format,
   and MUST NOT be promised by implementations of it.** Floats, physics
   ordering and renderer differences make it a lie waiting to be caught;
   `package.json`'s `seed` is reserved for engines that want to try
   anyway, and means nothing to the contract above. The exception is
   opt-in: a package declaring
   [`ext-strict-determinism`](extensions/strict-determinism.md) asks for
   exactly this, and an engine claiming that extension makes the promise
   the core forbids.
