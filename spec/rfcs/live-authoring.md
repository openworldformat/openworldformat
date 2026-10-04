# RFC: Live authoring — agents outside, ops in, the world in front

**Status:** accepted for draft 0.3. The normative text lives in
[the package](../package.md) (head-first, canonical text, assets, git,
the live folder), [the session log](../session.md) (`ModifyWorld`, the
total fold, authoring, the entry `message`), [profiles](../profiles.md)
(the Authoring profile) and [security](../security.md) (live authoring
surfaces); where this note and those pages differ, the pages win. All
five references implement the fold side and pass the new conformance
rules; Rust and JS implement authoring. Draft 0.x: this breaks
compatibility on purpose (package format 2), and says so.

## The problem

The format calls itself *AI-authorable*, but it specifies no way for an
AI to author. In practice the model lives inside one app (a chat panel,
a tool belt of app-specific calls), so authoring is coupled to that
app's tools, its model menu and its release cycle — and nothing about
the format helps an agent the user already has (a coding agent in a
terminal, a script, a CI job) change a world safely.

The pattern that works elsewhere: the document is a folder, the app is
a *canvas* that keeps it open, and agents work from outside. An image
editor whose projects are a manifest plus PNGs does exactly this — any
agent that can write files can build a picture while the person watches.
That editor also shows the pattern's weak spots: a bad write is ignored
without a word, a reload wipes undo, and a conflict is all-or-nothing
("Revert" or "Keep Mine").

A world can do better, because it already has the pieces that editor
lacks — ops with inverses, authorship, a log — but four things in the
current draft stand in the way:

1. **`manifest.json` is the base, not the world.** An agent that edits it
   rewrites where history starts, and later log entries still win at
   head; a viewer that reads only `manifest.json` shows a stale world.
2. **The fold is partial.** No edit op reaches `meta`, `avatar`, `tours`,
   `soundtrack` or `creations`, and the references don't carry them
   through a fold: in the Rust reference, none of the 14 worlds in
   `conformance/` and `examples/` survives a fold with an *empty* log
   (all lose `meta.description`; `instances.json` loses its creations,
   `soundtrack.json` its soundtrack).
3. **Patches replace whole structs.** A `ModifyEntity` carrying only
   `transform.position` resets rotation and scale; a material patch with
   only a texture resets the color. Agents send partial changes by
   nature.
4. **Must-ignore hides authoring mistakes.** A misspelled field is
   silently dropped by every reader — the right rule for a viewer, the
   wrong one for the thing that accepts an agent's edit.

## The design in one paragraph

A live package has one **authority** (the open app, or a command-line
tool when no app is open) and any number of **authors** outside it.
Authors read `manifest.json`, which is now the world *as it is* (the
head), and change it only by submitting **batches of edit ops**; the
authority binds names, allocates ids, merges partial struct patches,
checks every field strictly, applies the batch to a trial copy,
validates the result, and then commits the whole batch — or refuses it
whole, with a reason per op. **Assets are files** authors write directly;
a reference to one is stored by content hash and never changes. Every
commit appends to `ops.jsonl` under its author; when the package is a
**git repository**, every commit is also a git commit, and replay walks
commits as keyframes.

## 1. Head-first package *(breaking)*

`manifest.json` holds the state at the tip of `main` — the world now.
The base moves to `snapshots/`; it is the oldest keyframe.

```
castle.world/
  manifest.json        the world now: what viewers draw, what authors read
  ops.jsonl            how it got here
  package.json         head_revision; world_sha256 names manifest.json's bytes
  snapshots/base.json  the oldest state
  assets/              author-written files, and the <sha256>.<ext> copies the world uses
```

- **The invariant is unchanged** — state at any tip is a pure fold of
  the path from base to tip — and gains a consistency rule:
  `fold(base, path to main) == manifest.json`. A package whose log is
  empty is just `manifest.json` plus assets, which is what a script
  writes naturally.
- **Viewers never fold.** The Viewer profile reads `manifest.json` and is
  done; the trap of rendering a stale base disappears.
- **Compaction** becomes: drop old log entries and snapshots, write the
  new oldest state as `snapshots/base.json`. `manifest.json` is already
  the head.
- **One writer.** Only the authority writes `manifest.json`,
  `ops.jsonl`, `package.json` and `snapshots/`. Writers append the entry
  first, then `package.json` naming the new manifest bytes, then the
  manifest (temp file + rename), so a reader in another process that
  sees the new manifest also sees what explains it. An authority that
  finds `manifest.json` not matching `world_sha256` restores it and says
  so; the world only changes through ops.
- **Readable output** (SHOULD): pretty JSON, arrays of plain values on
  one line, entities in their existing order. It keeps diffs small and
  lets authors read the file.

## 2. Ingestion: what the authority does with a batch

A batch is a JSON array of ops, or `{"ops": [...], "author": "…",
"message": "…"}`. Each op passes, in order, against a trial document
that already holds the batch's earlier ops:

1. **Bind.** A string wherever an entity id goes (`ModifyEntity.id`,
   `DeleteEntity.id`, `parent`) is a name, resolved to the id it names
   *now* — the existing "names bind at ingestion" rule, extended to op
   addresses. A `SpawnEntity` without an `id` gets the next one (the
   reply reports it). Ids stay numeric in the committed log.
2. **Merge.** In `ModifyEntity` patches, object-valued struct fields
   (`transform`, `material`, `light`) merge into the entity's current
   value as a JSON merge patch (RFC 7396): present keys replace, `null`
   removes, absent keys keep. `SetEnvironment` merges the same way. The
   *committed* op carries the full merged value, so the log needs no new
   semantics to replay. Enum-valued fields (`shape`) still replace.
3. **Read strictly.** The op must parse, and no key may be one the
   format would drop (an entity's unknown keys are allowed only under
   `ext-*`). Errors carry a JSON pointer:
   `op 0: /ModifyEntity/patch/material/colour is not a field of the
   format`. This is the profile spec's *Strict Mode*, made the default
   for authoring.
4. **Store assets.** Every asset an op references must be a relative
   path inside `assets/` that exists; the authority stores an immutable
   copy at `assets/<sha256>.<ext>` and rewrites the reference to it (and
   fills `MeshAssetRef.sha256`). A path already in that form must still
   hold the bytes its name promises.
5. **Apply** to the trial; an op that no longer applies (entity not
   found, id in use) fails here.

After the last op, the composed world is validated (`validate_manifest`
or equivalent). **Any failure refuses the whole batch**: nothing is
written, and the reply lists every op's reason. Success appends one
entry — the edits, plus a history record of intent:

```json
{"revision": 7, "author": {"name": "claude"}, "timestamp_ms": 1790000000123,
 "ops": [{"tool": "live", "args": {"via": "ops", "message": "a lantern by the gate"}},
         {"SpawnEntity": {"entity": {"id": 21, "name": "lantern", …}}}]}
```

**Undo** is unchanged in meaning — append the inverse — and is just
another submission by whoever asks for it.

## 3. A total fold: `ModifyWorld`

A new edit op patches the document's scene-level fields with the same
rules as `ModifyEntity` (absent unchanged, `null` clears, objects merge
at ingestion):

```json
{"ModifyWorld": {"patch": {"meta": {"description": "…"}, "tours": [ … ],
                            "soundtrack": null}}}
```

It reaches `meta`, `avatar`, `tours`, `soundtrack` and `creations`; its
inverse is a `ModifyWorld` restoring the old values. Every reference's
fold state then carries the whole document, and a new conformance rule
holds: **every world round-trips through an empty fold.** Without this,
an ops-only API cannot change a tour at all, and head-first cannot keep
its invariant.

## 4. Git as the history carrier *(optional)*

A live package MAY be a git repository. When it is:

- **Every committed batch is a git commit**, authored by the batch's
  author, with its message as the subject and the ops summarized in the
  body. `git log` is the change history; filtering by author answers
  "what did the model do, and what did the people?".
- `.gitignore` excludes `.live/`; `.gitattributes` sets
  `ops.jsonl merge=union`.
- **Every commit is a keyframe**, because `manifest.json` is the head
  (§1). Time travel is `git show <commit>:manifest.json`.
- **Branches, refs, forks, transport and signing are git's.** For a
  git-backed package, the branching RFC's `id`/`parent`, `package.json`
  `refs` and `forked_from` are redundant; signed commits answer the
  security section's note that `author` is unauthenticated.

What git does **not** do, and stays the format's:

- **Intent at op granularity** — the log. A diff of `manifest.json`
  shows text, not "spawn lantern, then parent it".
- **Real-time sessions** — rooms stream ops; commits are too coarse.
- **Semantic merge.** Textual merges of `manifest.json` conflict.
  Merging branches needs a merge driver that merges by entity id and
  field (and reallocates ids allocated concurrently, as
  `merge_branch` does); this RFC names the need, not the driver.

Decided (review, before acceptance): inside git, git versions asset
bytes — references may use logical names, and hash-named copies are for
packages without git and for exports, which rewrite references to them
so a zip of one revision still verifies. Copying assets by hash inside a
repository would only duplicate git's own object store.

Also decided: an authority that sees `manifest.json` change to bytes
`package.json` names treats it as a checkout and reopens — a `git
checkout` or pull under an open app is not a direct write. Only other
writes are refused, and an authority may make `manifest.json` read-only
so they fail at once. Before any semantic merge driver, the canonical
text (members sorted, entities by id, plain arrays inline) keeps
ordinary textual merges clean in most cases; a driver stays open.

## 5. Replay from keyframes

With head-first commits, a replay walks the history's keyframes — each
commit's `manifest.json`, oldest first — and a renderer steps between
consecutive keyframes by diffing them into ops. No fold is needed to
seek, and a branch replays from its own commits. The log-based replay
(structural replay over `ops.jsonl`) stays valid and is the only one
without git.

## 6. The live folder (non-normative conventions)

So that any agent can find its way without app-specific setup:

- **`AGENTS.md`** in the package: how to read the world, how to submit
  ops, the asset rule. Agents that read `AGENTS.md` on their own then
  need no prompt engineering: `cd castle.world && claude "add a moat"`.
- **`.live/endpoint.json`**, written by an open authority: its URL and a
  token. A plain local HTTP API (`POST /ops`, `POST /undo`, `GET /log`,
  `GET /world`, `GET /screenshot`, `GET /selection`) is enough; an MCP
  server may wrap the same calls. The token keeps web pages from posting
  to a local port.
- **Feedback** the agent can see: the reply to every submission, a fresh
  render on request (3D needs eyes), and what the person has selected
  ("make *this* taller").
- **One authority at a time.** A command-line authority defers to an
  open app that answers at the endpoint, and ignores a stale one.

These are conventions for interoperating tools, not format semantics;
they live under `.live/`, which is never zipped or versioned.

## Alternatives considered

- **Agents edit `manifest.json`; the authority diffs and journals it.**
  The zero-protocol option, closest to the image editor. Rejected as the
  primary path: the diff loses intent, a typo becomes a silent ignore or
  a confusing rejection after the fact, two writers inside one
  coalescing window lose an edit, and the authority has to write back
  normalizations (ids, hashes) into a file an agent is holding open.
  Ops give validation *before* anything changes.
- **Agents append to `ops.jsonl`.** Precise, but every author must
  produce revisions, entry ids and valid patches, and concurrent appends
  race. The authority does that work once.
- **Keep the agent inside the app.** Works, and stays possible — an
  in-app agent is just another author — but it ties authoring to one
  app's tools and models, which is what this RFC exists to undo.

## Cost

- **Breaking:** head-first changes what `manifest.json` means;
  examples, conformance packages and all five references move with it.
- **New normative surface:** ingestion rules (binding, merge, strict),
  `ModifyWorld`, the total-fold conformance rule, the authority's write
  order.
- **Two history models** for a while (log DAG and git) until §4's open
  choice is settled.
- **Coarser history** for whole-batch commits than for per-call tools —
  intent survives only as the message and the op list.

## Evidence

The proof of concept (LocalGPT `poc/live-editing`: an authority in
`world-agent`, Gen's `--live` canvas with the HTTP API, a headless
command-line authority) was driven against `examples/hello-world` by an
agent working only through files and the API:

- a batch by name — environment, spawn, move, delete — committed as
  revision 1 and appeared live;
- a batch with `colour` for `color` and an unknown parent name was
  refused whole with one pointer per op, its valid op unapplied and every
  file byte-identical;
- a texture written, referenced, rewritten and referenced again became
  two stored versions, and undo restored the first — which works only
  because stored versions never change;
- a direct write to `manifest.json` was restored within half a second;
- `git log` read as the change history across three authors, `git diff`
  of the manifest read like a review, and a replay from commits showed
  every keyframe and returned to the head;
- after every step, `fold(base, log) == manifest.json` and every asset
  matched its hash.

Gaps it hit, which this RFC answers: partial patches resetting fields
(§2.2), unreachable scene fields (§3), the base-first manifest (§1).

## Open questions

- The semantic merge driver for git: in the spec, or a reference tool —
  after the canonical text has been tried on real merges.
- Whether ids should become collision-free strings for branch-heavy work
  (names bind at ingestion and committed logs carry ids — settled).
- Redo, and per-author undo outside rooms.
- The Authoring profile in the Python, Swift and Kotlin references.

## Implementation status

- **LocalGPT (proof of concept):** `crates/world-agent/src/live.rs`
  (ingestion, commit, undo, guard, verify, git commits and history),
  `crates/gen/src/gen3d/live.rs` (`--live` canvas, HTTP API, replay from
  commits). Built on the 0.1 crate, so `ModifyWorld` is not there yet:
  scene-level fields are simply unchangeable through it.
- **This repository (draft 0.3):**
  - all five references carry the whole document through the fold, apply
    and invert `ModifyWorld`, clear on the inverse of a first
    `SetEnvironment`/`SetCamera`, read head-first packages, and carry the
    entry `message` in its identity (one golden id, five references);
  - conformance: every conformance world and every example's base and
    head survive an empty fold; every example's `manifest.json` is the
    fold to `main` and is named by `world_sha256`;
  - Rust (`authoring::ingest`, `manifest_text`) and JS (`ingest`,
    `manifestText`) implement authoring and the canonical text, and write
    the same bytes;
  - the examples are head-first, in canonical text; the schema gained the
    manifest's `ambience`.
