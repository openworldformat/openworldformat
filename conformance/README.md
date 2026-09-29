# Conformance worlds

Behavior is specified by what these worlds render to. An implementation
claiming a profile renders the worlds of that profile; "renders" means
the same scene, within the tolerance the format's conventions allow
([spec/world.md](../spec/world.md)).

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

`assets/textures/` holds the checker and grid textures `textures.json`
references. Worlds referencing `assets/` are packages in miniature: the
same path rule as a full `.world`.

Each world is a complete `manifest.json` — also valid base worlds for
session tests. License: Apache-2.0, from the LocalGPT conformance suite.

## Adding a case

Spec changes that alter what a renderer draws require new or changed
cases here, in the same PR ([CONTRIBUTING.md](../CONTRIBUTING.md)).
A case states, in its `meta.description`, exactly what it pins.
