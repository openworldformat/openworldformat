# The Open World Format specification

**Status: draft 0.3.** This specification describes `.world` packages at
manifest schema version 3, package format 2 (head-first). The normative definitions are:

1. [`schema/world.schema.json`](../schema/world.schema.json) — the world
   document's data model, machine-readable. Where prose and schema
   disagree, the schema wins and the prose gets fixed.
2. The [conformance worlds](../conformance/) — behavior is specified by
   what these render to. An implementation claiming a profile renders its
   worlds.
3. These documents — the rules the schema cannot express (identity,
   folding, versioning, must-ignore).

## Reading order

1. [The world document](world.md) — L0: what a world *is*.
2. [The package](package.md) — L2/L3: the folder, assets by hash,
   metadata, integrity.
3. [The session log](session.md) — L1: the ops that build a world and
   the history around them.
4. [The state document](state.md) — L0: where the game is.
5. [Profiles and extensions](profiles.md) — what an implementation must,
   may and must-ignore.
6. [Versioning policy](versioning.md) — the compatibility contract.
7. [Security and Privacy](security.md) — parsing robustness and user data.

## The one invariant

> **State at any tip is a pure fold of the path from base to tip.**

```
fold(base, path from base to tip) == state at tip
```

Everything the format offers is a consequence: rendering (read
`manifest.json`, the fold at the tip of `main`, already written down),
editing (send edit ops to the one authority), multiplayer (one authority
orders the ops), undo (append inverses), save games (base + a player's
log), replays (fold with a clock, or walk the keyframes), mods (patches
against a pinned base revision), forking (base + log prefix).

The fold is total — its state is a whole manifest, and every field is
reachable by an edit — so the head can always be written down, and an
agent working from outside the app can change anything a person can.

## What this format is not

Not a game engine, not a physics solver, not a renderer, not glTF's job
(meshes are glTF leaves), not a content-management system. The spec
describes what a world *is*; what an engine *does* with it stays the
engine's.

## Open items (before 1.0)

**Next, in order:**

1. ~~Merges that agree~~ — **done** (2026-10-09): the merge rules are
   exact in [the session log](session.md) ("The merge rules, exactly"),
   and `conformance/merge/` pins them in all five references.
2. ~~Cameras and shots~~ — **done** (2026-10-09):
   [`ext-cinematography`](extensions/cinematography.md) 0.2 is
   experimental: cameras as entities with a filmback and a lens, shots
   as ordered setups on the clock, outcome assertions in three
   references, and the JS renderer's letterboxed shot view.
3. ~~A schema marker on entity-reference fields~~ — **done**
   (2026-10-09): `x-entity-ref` marks the reference fields in the
   schema, [`schema/entity-refs.json`](../schema/entity-refs.json) is
   the generated list, and name binding, merge rewriting and ingestion
   validation in all five references read it (each embeds a copy and
   drift-tests it against the canonical file).
4. ~~Aim and focus by entity~~ — **done** (2026-10-11):
   [`ext-cinematography`](extensions/cinematography.md) 0.3 adds
   `aim_entity` and `focus_entity` as marked references — the generated
   list now carries extension schemas' markers, so the five references
   bind, rewrite and validate them like `parent` — with origins
   composed through parents, `centred` and `focus` assertions, a Swift
   reference (the fourth), and every authority committing ids for the
   deeper references, as the log always promised.
5. ~~A globe producer's export~~ — **done** (2026-10-20), three generic
   additions from one brief: the `Prism` shape with its geometry stated
   in prose ([the world document](world.md), "The geometry of the
   shapes"; `conformance/prism.json`), experimental until LocalGPT's
   Bevy renderer and the worldwalk viewers built it from the table
   beside the JS one, and a core shape since (2026-10-25);
   [`ext-h3`](extensions/h3.md), a geo frame extension carrying an
   entity's H3 cell and integer transform beside the derived core
   transform, proposed with a schema, a conformance world with outcome
   assertions and two references; and the export contract —
   `author.source` (`hand`, `library`, `ai`), day-coarse time, no prompt
   by default ([the session log](session.md) "The entry", [the
   package](package.md) "Exports from other systems"). RFCs:
   [`rfcs/ext-h3.md`](rfcs/ext-h3.md),
   [`rfcs/history-exports.md`](rfcs/history-exports.md).

**Deferred until a producer needs them:** collision-free ids, an `id` and
`parent` on every entry, and timed input samples (a breaking bundle, for git
merges and gameplay replay); a semantic merge driver for git; a runtime that
re-runs triggers over recorded input (semantic replay); a side count on
`Cylinder` and `Cone` — when the producer behind `Prism` ships `sides` on
its cylinders and cones, decide whether `sides` becomes a parameter of
those shapes rather than more shapes, so the prism is not a one-off.

- The geometry of every shape is now stated in prose ([the world
  document](world.md), "The geometry of the shapes") and pinned by
  [`conformance/geometry/shapes.json`](../conformance/geometry/shapes.json): exact
  bounds, the box, and landmark vertices, run by the JS renderer and
  the Rust reference in CI. The table settled three disagreements
  between shipping renderers — the torus plane, the wedge's facing, the
  icosahedron's orientation — in favour of the majority each time.

- The state document is now pinned with its own
  [`schema/state.schema.json`](../schema/state.schema.json); it joins
  `world.schema.json` in the generated core.
- The **extension registry** holds two experimental extensions,
  [`ext-physics`](extensions/physics.md) and
  [`ext-cinematography`](extensions/cinematography.md), and four
  proposed ones:
  [`ext-strict-determinism`](extensions/strict-determinism.md),
  [`ext-visibility`](extensions/visibility.md),
  [`ext-provenance`](extensions/provenance.md) and
  [`ext-h3`](extensions/h3.md). A proposed extension
  reserves its name; it becomes experimental with a reference
  implementation and conformance cases, and leaves experimental when a
  producer ships a package using it.
- Branching histories are specified and implemented in the reference
  fold ([rfcs/branching-histories.md](rfcs/branching-histories.md));
  [`examples/speedrun-fork`](../examples/speedrun-fork/) is the first
  package to write them (a challenge chain: two runs, one fork each).
  No app writes branches yet, and the viewer has no branch rail.
- More prose around replay determinism as implementers arrive.
- **Live authoring** is accepted ([rfcs/live-authoring.md](rfcs/live-authoring.md))
  and normative in [the package](package.md) and [the session log](session.md):
  agents outside the app submit ops to one authority, `manifest.json` is
  the head, `ModifyWorld` makes the fold total, and git may carry the
  history. Still open: a semantic merge driver for git, and the Authoring
  profile in the Python, Swift and Kotlin references (they read
  everything it writes).
