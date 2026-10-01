# The physics extension (`ext-physics`), version 0.1

**Status: experimental.** Two implementers — the JS reference
(`openworldformat/physics`) and the Rust reference (LocalGPT's
`world-physics` crate) — both running the same conformance outcome
assertions, and one shipped package using the extension
([`examples/the-drop-test`](../../examples/the-drop-test/)). It leaves
experimental when a producer outside this repository ships a package
using it ([CONTRIBUTING](../../CONTRIBUTING.md)). It tracks the glTF
physics drafts (`KHR_physics_rigid_bodies`, `KHR_implicit_shapes`) and
the OMI proposals: field names follow the drafts where they do not fight
this format, and diverge where the drafts assume a scene-graph engine.

A package using this extension SHOULD list it in `package.json`:

```json
"extensions": ["ext-physics"]
```

Everything the extension adds is namespaced under the `ext-physics` key
(on entities, on the environment, and as an op kind), so the core rule —
ignore what you do not understand, never reject a package for it — makes
a physics world harmless to any conforming reader. A renderer without
the extension draws the world **at rest**: bodies at their spawn
transforms, triggers that never fire. The scene at load is the world
before the first step.

## What it adds

An entity gains an `ext-physics` component; the environment gains a
block:

```json
{ "name": "bowling_ball", "transform": { "position": [0, 5, 0] },
  "shape": { "Sphere": { "radius": 0.3 } },
  "ext-physics": {
    "body": "dynamic", "mass": 6,
    "collider": "shape", "restitution": 0.3, "friction": 0.4 } }
```

```json
"environment": { "background_color": [0.1, 0.12, 0.18, 1.0],
  "ext-physics": { "gravity": [0.0, -9.81, 0.0] } }
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `body` | `"static"` \| `"kinematic"` \| `"dynamic"` | — | A static body never moves; a kinematic body moves only by behaviors; a dynamic body is simulated. |
| `mass` | kg, > 0 | `1.0` | Dynamic bodies only. |
| `collider` | `"shape"` \| `{"sphere": r}` \| `{"cuboid": [x, y, z]}` | `"shape"` | The collision volume. `"shape"` derives it from the entity's parametric shape. |
| `restitution` | 0..1 | `0.5` | Bounciness of contacts. |
| `friction` | 0..1 | `0.5` | Tangential damping at contacts. |
| `gravity_scale` | any | `1.0` | Multiplies the environment gravity for this body. |
| `linear_damping` | ≥ 0 | `0.0` | Velocity loss per second, contact or not. |

Colliders SHOULD be convex; the primitive sphere and cuboid forms exist
because they are. Gravity defaults to `[0, -9.81, 0]` when the block is
absent. Mesh-precision colliders (convex hulls, trimeshes) are not in
this version.

## The contract, in five rules

1. **Declare bodies, not simulation.** The component states what a body
   *is*. Which solver moves it, at what timestep, with what broad phase,
   is the engine's business — the format names no engine, and it names
   no solver either.
2. **The document is the stage, not the aftermath.** Simulation MUST NOT
   write edits. A dynamic body's spawn transform is document state;
   where it rolls is runtime state, and a settled tower of crates
   changes no revision. Only deliberate change is an edit — a trigger
   that removes a wall, a designer that moves a ramp.
3. **Outcomes cross the log.** A physics result that changes game state
   — a `collision` trigger firing, a score, a teleport — MUST reach the
   log as ops through the session authority, exactly as a click does
   today. Motion that changes nothing (debris, cloth settle, visual
   jitter) MAY diverge across devices and is never recorded. With this
   rule, semantic replay holds by construction: the truth is folded
   from the log, never re-simulated; only the pixels are.
4. **Playback without a solver.** The extension's op kind carries
   sampled transforms of dynamic bodies, so any device can scrub
   recorded motion:

   ```json
   {"ext-physics": {"t_s": [0.0, 0.1], "bodies": {"ball": [[0, 5, 0], [0, 4.95, 0]]}}}
   ```

   Writers SHOULD sample at or below 10 Hz (the format's input-sampling
   rate); readers interpolate. The op folds to nothing for the document
   fold, per the log's compatibility rule — unknown op kinds never
   break the fold. A device with the extension MAY play a recorded
   trajectory instead of simulating; which one happened is not
   observable from the document.
5. **Determinism stays honest.** Within one engine, version and
   platform, a fixed timestep SHOULD make simulation reproducible, and
   `package.json`'s reserved `seed` is the hook for it. Across engines,
   only the semantic contract is promised: the same fold plus the same
   recorded outcomes. Implementations MUST NOT promise bit-exact
   cross-engine trajectories — the core spec's rule, unchanged.

## Collision triggers

The core `collision` trigger event becomes real under this extension:
it fires on solver contact with the entity. Without the extension, a
renderer MAY approximate it with the event's `radius` as a proximity
zone, or never fire it. When `radius` is present it widens the contact
test (a catch radius), it does not replace contact.

## Saves and checkpoints

Mid-flight state resumes through the state document: declare a
`json`-typed field (`"physics.checkpoint"`) and write positions and
velocities to it as a `state` op. The payload's shape is engine
specific — it is a save, not a replay; another engine resumes the
world approximately (spawn transforms) or not at all, and both are
conformant.

## Conformance

Physics conformance is **outcome assertions, not render comparisons**:
fold the world, simulate or scrub, check predicates over the result.
The machine-checkable cases live in
[`conformance/outcomes/physics.json`](../../conformance/outcomes/physics.json)
alongside the renderable world [`conformance/physics.json`](../../conformance/physics.json)
(the render check draws it at rest). An implementation claims this
extension by passing the outcomes; the assertion kinds are:

| Assertion | Checks |
|---|---|
| `contact` | two named bodies touch, within `within_s` seconds |
| `rest` | a body's resting position is within `tolerance` of `near` |
| `bounces` | a body's impacts above 0.5 m/s number at least `min` |

## Reference implementations

Two, in two languages, running the same conformance outcome assertions
from `conformance/outcomes/physics.json` in their own CI:

- **JS** — `openworldformat/physics` (in this repository,
  `js/src/physics.js`): the op kind in the fold, `ext-*` patch
  passthrough, the solver, trajectory write/fold, and the outcome
  runner.
- **Rust** — LocalGPT's `world-physics` crate
  (`localgpt-world-physics`): `SessionOp::Extension` in the session
  layer, `extra` maps on entities and the environment so extension
  fields ride the fold (including patches — null clears, in JSON and
  RON alike), the same solver algorithm step for step, and the same
  outcome runner.

Both are deliberately minimal — spheres against floors and
axis-aligned statics, fixed 1/120 s semi-implicit Euler, sleep on rest
— so the conformance cases are executable by anyone and the contract
is testable in CI. They are reference-grade, not production physics;
conforming engines are expected to use real ones. That the two engines
agree on outcomes without agreeing on bits is the contract working:
cross-engine trajectories are never promised, cross-engine outcomes
are. Static rotations are ignored by the references (axis-aligned
colliders only); production engines are not so limited.
