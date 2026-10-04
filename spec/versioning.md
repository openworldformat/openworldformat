# Versioning policy

The compatibility contract, written as policy from lessons already paid
for in the format's lineage.

## Two versions, two jobs

- `manifest.json`'s `version` — the **schema version** of the world
  document (currently 3). It gates parsing.
- `package.json`'s `format_version` — the **package format** (folder
  layout, metadata, log entry envelope). Currently 2: version 2 is
  head-first — `manifest.json` became the world now and the base moved to
  `snapshots/base.json` — which changes what a version-1 reader would
  draw, so it is a new version rather than an optional field
  ([the package](package.md)).

## Rules

1. **Minor schema versions add optional fields.** A reader of version N
   reads worlds of all versions ≤ N and skips fields it doesn't know
   ([must-ignore](profiles.md)).
2. **Major schema versions may restructure, but must not silently drop
   data.** The lineage's schema 2→3 bump exists precisely because a
   version-2 reader silently lost instanced parts and triggers; the fix
   was the bump, so old readers refuse rather than lie. When meaning
   changes, bump — a reader that would misrepresent a world must fail
   loudly.
3. **Log formats evolve additively.** New op kinds fold to nothing for
   old readers; old files parse under new readers forever. A log line
   that can't be parsed loses at most itself.
4. **Conformance worlds are versioned with the schema.** An
   implementation claiming version N passes version N's suite. The suite
   is the definition of "renders correctly"; prose is the explanation.
5. **Extensions are preserved forever.** Once an extension version is
   registered, its spec page and conformance cases remain permanently.
   Claiming version N of an extension means passing its version N cases
   forever.
6. **Library Versioning.** While the format is in draft (0.x), language
   bindings SHOULD align their SemVer major/minor numbers with the spec
   draft. After 1.0, bindings SHOULD use the schema version as their
   major version.

## What implementers may rely on

- A world at schema version N renders under a version-N implementation,
  forever; nothing is deprecated out from under content.
- A log written today folds under today's and tomorrow's readers.
- `fold(base, log)` is stable: the same package folds to the same state
  on every machine, which is what assets-by-hash and the integrity
  hashes are for.
