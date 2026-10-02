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

## Adding a case

Spec changes that alter what a renderer draws require new or changed
cases here, in the same PR ([CONTRIBUTING.md](../CONTRIBUTING.md)).
A case states, in its `meta.description`, exactly what it pins.
