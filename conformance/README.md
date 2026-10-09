# Conformance worlds

Behavior is specified by what these worlds render to. An implementation
claiming a profile renders the worlds of that profile; "renders" means
the same scene, within the tolerance the format's conventions allow
([spec/world.md](../spec/world.md)). Tolerance: rendered geometry must fall within a 1mm bounding box difference, and colors within 1/256 sRGB threshold.

| World | Covers |
|---|---|
| `shapes.json` | every parametric shape, on a ground plane under a sun |
| `materials.json` | PBR parameters, alpha modes, emissive |
| `lights.json` | directional, point, spot; intensity units |
| `behaviors.json` | all behavior types running from load |
| `hierarchy_tours.json` | parenting, camera, tour waypoints |
| `hierarchy_rotations.json` | verifies intrinsic XYZ Euler composition and rotated parent-child hierarchies |
| `textures.json` | texture maps (`assets/textures/`) |
| `instances.json` | creations instanced with per-part overrides |
| `triggers.json` | every trigger event and action |
| `soundtrack.json` | soundtrack curves, modulations |
| `physics.json` | the `ext-physics` extension, drawn at rest |

`assets/textures/` holds the checker and grid textures `textures.json`
references. Worlds referencing `assets/` are packages in miniature: the
same path rule as a full `.world`.

`outcomes/` holds outcome assertions, the conformance form for things
that cannot be pixel-diffed. `outcomes/physics.json` pairs with
`physics.json`: a renderer without the extension draws the world at
rest (that is its conformance), while an engine claiming `ext-physics`
passes the assertions — contact, rest, bounces — under simulation or
scrubbed playback ([spec/extensions/physics.md](../spec/extensions/physics.md)).
The reference solver runs them in CI (`npm test` in `js/`).

Each world is a complete `manifest.json` — also valid base worlds for
session tests. License: Apache-2.0, from the LocalGPT conformance suite.

## Rules every reference runs

Beyond rendering, two rules hold for the documents themselves
([spec/session.md](../spec/session.md), [spec/package.md](../spec/package.md)):

- **The fold is total.** Every world here, and every example package's
  base and head, folds with an empty log back to itself — up to names
  bound to ids, entity order, absent-versus-`null`, numbers by value,
  and `next_entity_id` at its effective value.
- **Head-first.** Every example package's `manifest.json` is the fold of
  its log over `snapshots/base.json` to `main`, `package.json` names its
  bytes in `world_sha256`, and it is in canonical text.
- **Merges agree.** Every merge case in `merge/cases/` merges to the
  same rewritten entries and the same head, in every reference.

## Merge cases

`merge/` holds the shared merge corpus ([spec/session.md](../spec/session.md),
"The merge rules, exactly"). Each case in `merge/cases/` is one JSON
file: a `base` world, the `main` line's entries, the `branch`'s entries,
and the `expected` half — the merge's remap table, the rewritten branch
entries, and the fold of main plus the merged branch as canonical text.
Expected results come from the JS reference: `node merge/generate.mjs`
regenerates the corpus (seeded; same code, same cases), and every
reference runs every case in its own test suite. A reference that
disagrees fails its own CI, which makes the corpus a differential test
without a cross-language harness.

## Adding a case

Spec changes that alter what a renderer draws require new or changed
cases here, in the same PR ([CONTRIBUTING.md](../CONTRIBUTING.md)).
A case states, in its `meta.description`, exactly what it pins.
