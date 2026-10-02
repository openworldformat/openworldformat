# The Open World Format specification

**Status: draft 0.2.** This specification describes `.world` packages at
manifest schema version 3. The normative definitions are:

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

Everything the format offers is a consequence: rendering (fold nothing,
read the base), editing (append edit ops), multiplayer (one authority
orders the ops), undo (append inverses), save games (base + a player's
log), replays (fold with a clock), mods (patches against a pinned base
revision), forking (base + log prefix).

## What this format is not

Not a game engine, not a physics solver, not a renderer, not glTF's job
(meshes are glTF leaves), not a content-management system. The spec
describes what a world *is*; what an engine *does* with it stays the
engine's.

## Open items (before 1.0)

- The state document is specified (experimental until 1.0) with its own
  [`schema/state.schema.json`](../schema/state.schema.json); it joins
  `world.schema.json` in the generated core when it settles.
- The **extension registry** holds its first extension —
  [`ext-physics`](extensions/physics.md) 0.1, experimental: two
  implementers (the JS and Rust references) running the same outcome
  assertions; it leaves experimental when a producer ships a package
  using it.
- Branching histories are specified and implemented in the reference
  fold ([rfcs/branching-histories.md](rfcs/branching-histories.md));
  [`examples/speedrun-fork`](../examples/speedrun-fork/) is the first
  package to write them (a challenge chain: two runs, one fork each).
  No app writes branches yet, and the viewer has no branch rail.
- More prose around replay determinism as implementers arrive.
