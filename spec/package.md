# The package (L2/L3)

A `.world` is a directory. A zip of that directory is the transport form
for finished, published worlds; the directory is canonical because the
log appends.

The package is **head-first**: `manifest.json` is the world as it is
now. A viewer reads it and nothing else; the history behind it is
there for the tools that want it.

```
thing.world/
  manifest.json            L0  the world now: the state at the tip of main
  state.json               L0  the typed state document
  ops.jsonl                L1  the session log — one JSON entry per line
  snapshots/base.json      L1  the oldest state the log folds from
  snapshots/rev-<N>.json   L1  derived keyframes, never authoritative
  assets/…                 L2  content leaves: glTF meshes, textures, audio
  package.json             L3  metadata: versions, profiles, refs, integrity
  AGENTS.md                    optional: how to change this world, for agents
  .live/                       reserved: an open authority's files, never transported
```

## Head-first

- **`manifest.json` holds the state at the tip of `main`** — the ref
  named `main` in `package.json`, else the log's last entry, else (no log)
  the base. Formally: `manifest.json` is the fold of the path from
  `snapshots/base.json` to that tip ([the session log](session.md)),
  written as a manifest.
- **`snapshots/base.json` holds the oldest state** — the document at
  `base_revision`. It MUST be present when `ops.jsonl` holds edit
  entries; a package whose log holds no edits MAY leave it out, and its
  base is then `manifest.json`. A package with no history at all is just
  `manifest.json` and its assets.
- **Viewers never fold.** The Viewer profile reads `manifest.json` and is
  done. Session readers fold from the base and MAY check that the fold to
  `main` is `manifest.json` (equal as worlds: entities by id, an absent
  field the same as `null`, numbers by value).
- **Only an authority writes** `manifest.json`, `ops.jsonl`,
  `package.json` and `snapshots/` ([the session log](session.md),
  "Authoring"). It writes in this order — append the entry, then
  `package.json` naming the new manifest bytes, then `manifest.json` by
  writing a temporary file and renaming it over — so a reader in another
  process that sees the new manifest also sees the entry and the metadata
  that explain it.
- **A manifest whose bytes `package.json` names is a checkout, not a
  write.** When `manifest.json` changes under an authority and its SHA-256
  equals `package.json`'s `world_sha256`, something that moves whole
  packages (a `git checkout`, a pull, a sync client) changed the package:
  the authority reopens it and shows the new head. Any other change is a
  direct write; the authority refuses it — restores the head and says so
  to whoever asked — because the world changes only through ops.

## `package.json`

```json
{
  "format_version": 2,
  "name": "hello-world",
  "profiles": ["viewer", "player", "session"],
  "base_revision": 0,
  "head_revision": 3,
  "refs": { "main": "e3" },
  "forked_from": null,
  "seed": null,
  "world_sha256": "…",
  "log_sha256": "…",
  "updated_ms": 1790000000123
}
```

| Field | Meaning |
|---|---|
| `format_version` | The package format's own version (this document): **2**, head-first. |
| `profiles` | Which profiles the package uses; readers ignore unknown ones. |
| `base_revision` | The revision `snapshots/base.json` holds. |
| `head_revision` | The revision `manifest.json` holds — the tip of `main`. |
| `refs` | Named tips of the history (e.g., `"main": "entry-id"`). Refs MUST point to an entry `id`, not a file line number, so they survive compaction and branching. |
| `forked_from` | When this package began as a fork: which package, at which entry. |
| `seed` | Reserved for deterministic replay. |
| `world_sha256` / `log_sha256` | Integrity: SHA-256 of `manifest.json` and of `ops.jsonl` as the authority last wrote them. |

## Canonical text

An authority SHOULD write `manifest.json` in its canonical text, so that
one world is always the same bytes — small diffs, ordinary git merges,
and a `world_sha256` that changes only when the world does:

- UTF-8, LF line ends, a trailing newline; two-space indentation;
- object members sorted by key in code-point order, members whose value
  is `null` left out, empty objects as `{}`;
- the manifest's `entities` in ascending id order (order is a
  serialization detail: readers MUST NOT rely on it);
- arrays whose elements are all plain values (strings, numbers, booleans,
  `null`) on one line, `[0, 1.5, -2]`; other arrays one element per line;
  empty arrays as `[]`;
- numbers in their shortest form that reads back to the same value,
  integral values without a fraction (`2`, never `2.0`); strings escaped
  only where JSON requires it.

The reference writers (`manifest_text` in Rust, `manifestText` in JS)
write the same bytes, and every example package's `manifest.json` is in
this form.

## Assets

Assets live under `assets/`. A world references them by path, relative
to `assets/`, and a mesh reference also carries the file's SHA-256 — a
missing or changed file is detected, not silently drawn.

**Content addressing is the baseline.** A referenced asset's bytes MUST
NOT change: history at an earlier revision has to find the bytes it used,
and the package must stay a self-contained, portable database on devices
with no git (mobile included). So the authority stores every asset an op
references as an immutable copy named by its hash,
`assets/<sha256>.<ext>`, and the committed reference points at the copy.
An author may write and rewrite a working file of any name; referencing
it again stores a new version, and the old one stays. A file named by a
hash that no longer holds those bytes is corrupt.

A package that is a git repository MAY let references use logical names
(`assets/brick.png`), with git versioning the bytes — readable asset
diffs in exchange for needing the repository. That is the repository's
choice, not the format's default. An export to the transport form (a zip
of one revision) SHOULD rewrite references to hash-named copies, so the
package verifies without git. And under git the copies cost almost
nothing: they are immutable and content-addressed, so identical bytes
are one git blob, and only genuinely new versions add to the object
store — which any history would.

Meshes are glTF (`.glb`); textures are PNG; audio formats are determined by
the Player profile (e.g., OGG, WAV, MP3). The package never invents leaf
formats — it composes them.

## Git

A package MAY be a git repository — an optional layer for desktop
workflows, never a dependency: a `.world` is fully self-contained
without it (see *Assets*). When it is, git carries the history the
format describes, and an authority:

- makes every committed batch a git commit, authored by the batch's
  author, with the entry's `message` as the subject;
- keeps `.live/` out of the repository (`.gitignore`) and marks
  `ops.jsonl` `merge=union` (`.gitattributes`): concurrent appends merge
  as a union;
- reads branches, refs, forks and transport as git's. A branch of the
  package is a git branch; `package.json`'s `refs` and `forked_from` MAY
  then be left out.

Because `manifest.json` is the head, every commit holds a whole world:
seeking is reading a commit's `manifest.json`, and replay is walking
commits ([the session log](session.md), "Replay and determinism"). Git
does not merge worlds by meaning; the canonical text keeps most textual
merges clean, and a merge that conflicts is resolved as the session log
merges branches.

## The live folder

These are conventions, not format semantics: tools that open a package
for editing use them so that any agent can find its way in.

- **`AGENTS.md`** — how to read the world and how to change it. Agents
  that read such files on their own then need no other instruction.
- **`.live/endpoint.json`** — written by an open authority: the URL of
  its local API, a token, and its process. The API takes batches of ops
  ([the session log](session.md), "Authoring"), undoes, reports the
  history, renders the current view, and reports what the person has
  selected. An MCP server over the same calls is an equal way in.
  `.live/` is never transported and never versioned.
- **A read-only `manifest.json`** — an authority MAY clear the file's
  write permission while it holds the package, so that a direct write
  fails at once instead of being undone later. Tools that replace files
  (a git checkout) still work.

## Integrity and tolerance

Writers SHOULD refresh `log_sha256`, `world_sha256` and `head_revision`
whenever the log appends, and SHOULD snapshot periodically (e.g., every
1,000 revisions or 5MB of log growth). Keyframes for seeking: read at
revision N = nearest base-or-snapshot ≤ N, then fold forward.

Readers MUST be tolerant: a torn last log line loses at most itself and
is skipped (and countable); the fold stops at the first entry that no
longer applies. Integrity hashes, when present, SHOULD be checked before
trusting a package from a third party.

Engines SHOULD implement a "Safe Mode" boot that explicitly warns the user
if a session log contains operations from profiles or extensions they do
not have installed. This prevents silent erasure of custom content when
saving the world back to disk.
