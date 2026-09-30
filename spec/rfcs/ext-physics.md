# RFC: The physics extension (`ext-physics`)

**Status:** accepted as experimental (version 0.1). The normative text is
[`spec/extensions/physics.md`](../extensions/physics.md); this note states
the problem, the alternatives and the cost, per [CONTRIBUTING](../../CONTRIBUTING.md).

## The problem

The core format has a `collision` trigger event that nothing produces.
Behaviors are kinematic — `bounce` moves things, but nothing falls,
nothing topples, nothing rests. Every genre sketch that goes past walking
and clicking (a ball down a ramp, a tower of crates, a jump that can
fail) needs a dynamics model, and the moment two engines each invent
one, worlds stop meaning the same thing on both. So the format needs to
say what a body *is* — without becoming a physics engine, which
[the spec](../README.md) explicitly is not.

The hard part is not the fields. It is the collision between physics and
the format's own contract: state at any revision is a pure fold of the
log over the base, and replay is semantic — the same fold plus the same
inputs produce the same trigger outcomes and state trajectory — while
simulation is continuous, floating-point, order-dependent and
solver-specific. A naive "add physics" breaks one or the other.

## The alternatives

1. **Bake dynamics to animations.** A falling ball becomes a
   `path_follow` behavior or a mesh animation. Deterministic by
   construction — but it is choreography, not physics; it cannot answer
   "where did the ball end up" for a world it did not already visit, and
   every interaction must be pre-authored. This is the vestibular-only
   answer; it abandons the denotative half.
2. **Lockstep-deterministic simulation in the spec.** Mandate a solver,
   a fixed timestep, a float discipline, so every engine computes the
   same trajectory. This is the esports/RTS approach, and it is a lie in
   a multi-engine format: cross-engine bit-exactness is exactly what the
   core spec refuses to promise ([session.md, replay levels](../session.md)).
   It would also fossilize one solver into a document format.
3. **Solver in the spec.** Specify step order, broad phase, contact
   resolution. The format becomes a physics engine — the thing the first
   paragraph of the spec says it is not.

## The chosen design

Split physics by *consequence*, not by hardware:

- **Declare bodies, not simulation.** An `ext-physics` component states
  what a body is (type, mass, collider, restitution, friction); how it
  moves stays the engine's, exactly as with rendering.
- **The document is the stage, not the aftermath.** Simulation never
  writes edits. A dynamic body's spawn transform is document state;
  where it rolls is runtime state.
- **Outcomes cross the log; trajectories don't have to.** Physics
  results that change game state (a collision trigger, a score, a
  teleport) reach the log as ops — the same path a click takes today —
  so semantic replay holds by construction: truth is folded, never
  re-simulated. Visual-only motion may diverge across devices and is
  never recorded.
- **Playback without a solver.** The extension's `trajectory` op kind
  carries sampled transforms of dynamic bodies, so a device with no
  physics support scrubs recorded motion instead of simulating it.
- **Determinism stays honest.** Intra-engine replay with the reserved
  `seed`; cross-engine, only the semantic contract. Never bit-exact.

The cost: two notions of "what happened" (recorded outcome vs.
re-simulated motion) that implementations must keep straight; a
conformance model that cannot be pixel-diffed and needs outcome
assertions instead; and one more thing an editor UI must round-trip.
All three are cheaper than any alternative's failure mode.

## Implementation status

- **JS reference** (`viewer/src/physics.js`, exported as
  `openworldformat/physics`): the extension op kind in the fold, patch
  passthrough for `ext-*` fields, a deliberately minimal deterministic
  solver (spheres, floors, axis-aligned statics; fixed 1/120 s
  semi-implicit Euler), trajectory write/fold, and the runner for the
  conformance outcome assertions.
- **Rust reference** (LocalGPT's `world-physics` crate): the second
  implementer — `SessionOp::Extension` in the session layer, `extra`
  maps so extension fields ride entities, environments and patches,
  the same solver step for step, the same outcome runner, and the
  fixtures mirrored with a drift test against this repository's
  copies. The two engines agree on the outcomes; they do not agree on
  the bits, which is the contract.
- **Conformance:** `conformance/physics.json` (a renderable world — a
  renderer without the extension draws it at rest) and
  `conformance/outcomes/physics.json` (the machine-checkable outcomes,
  run by both references in their CI).
- Not yet: a producer shipping a package that uses the extension (the
  gate for leaving experimental); joints and constraints; character
  controllers; mesh-level colliders.
