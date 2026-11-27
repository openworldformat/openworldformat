# RFC: Entity-local cinematography tracks (`ext-cinematography.track`)

**Status:** experimental (version 0.4). The normative text is
[`spec/extensions/cinematography.md`](../extensions/cinematography.md);
this note states the problem, the alternatives and the cost, per
[CONTRIBUTING](../../CONTRIBUTING.md).

## The problem

Cameras and shots made previs setups portable, but a set of static setups is
not yet an animatic. A producer needs to block an actor crossing a mark, a
crane carrying a camera, and a target the camera follows, all on the shot's
existing seconds clock. The result must be inspectable without a renderer:
the local pose at a time, the child's origin through its animated parents,
and the camera's view and focus at that same time.

The need is deliberately small: whole-entity, parent-local translation,
rotation and scale. One entity carries one track beside `camera` and `shot`.
The track animates that entity only, so `SpawnEntity` and `ModifyEntity`
remain sufficient and no target-addressing or actor-specific op family is
needed. Cameras use exactly the same track as every other entity.

## The shape and evaluation

A track has an optional inclusive `clip`, `pre` and `post` modes, and at
least one of three nonempty channels: `translation`, `rotation` and `scale`.
Keys have strictly increasing finite `time_s` values and exactly three finite
`value` numbers. Interpolation is `linear` (the default) or `step`; linear
intervals may use `linear`, `ease_in`, `ease_out` or `ease_in_out` easing.
Both interval fields belong to the key from which the interval starts.

Translation uses parent-local world units/metres. Rotation uses the core's
intrinsic XYZ Euler degrees, component-wise and without shortest-path or
angle wrapping. Scale follows the core component-wise rule, including zero
and negative values. An absent channel retains its authored base value.

An absent clip spans the minimum and maximum key times across all channels.
Outside the clip, `hold` clamps, `base` returns the complete authored local
transform, and `loop` repeats a nonzero clip using Euclidean time wrapping.
Clip endpoints themselves never wrap. Keys outside an explicit clip still
participate when a boundary is sampled.

Evaluation is a pure `f64` function of the entity and time; conversion to the
core transform's `f32` components happens only at the end. The sampled local
TRS then passes through the existing behavior array and parent composition.
A timed shot samples the camera, every parent and any aim/focus entity at the
same time, then uses the 0.3 precedence rules. No wall clock, frame index,
random state, physics, asset state or previous result participates.

Strict authorities reject malformed known tracks with a pointer to the
offending field and refuse the whole submitted batch. Tolerant readers keep
the extension and draw the base transform plus behaviors; they must not
reject a world merely because it contains a track. Tracks add no entity
references, no inline version, no world schema bump and no package format
change.

## The alternatives

1. **Reuse `PathFollow` for every move.** A path behavior can describe some
   constant-speed translation, but not keyed rotation or scale, exact key
   times, fixed easing, step intervals, clip extrapolation or a common camera
   and actor sampling contract. Substituting a path would also change the
   document's meaning and lose the authored keys. It remains useful as a
   behavior after track sampling, not as the track's storage format.
2. **Scene-global tracks with entity targets.** A separate timeline could
   animate several entities, but would need a second addressing/binding
   mechanism, new merge and deletion rules, and a new op family. Entity-local
   tracks use ordinary ids, patches, must-ignore preservation and history.
   Multiple clips, blending and scene-global choreography are separate work.
3. **Quaternions, splines and arbitrary Bézier tangents.** Quaternions avoid
   Euler's presentation ambiguities but introduce serialization, shortest-path
   and sign choices; splines and arbitrary tangents add solver and validation
   complexity. Fixed component-wise Euler keys and four stated easing curves
   answer blocking's needs with arithmetic every implementation can share.
   Quaternion animation, per-axis streams and arbitrary tangents need another
   RFC and producer.
4. **Lens keys, skeletal animation, events and ping-pong.** These are real
   animation features, but none is required to block whole actors and cameras.
   They are excluded rather than partially specified. Lens animation belongs
   beside the camera component; skeletal/node addressing needs an asset-node
   contract; events need a state and replay contract.
5. **Rendered-image conformance.** Screenshots are useful producer acceptance
   evidence, but pixels make headless implementation impossible and smuggle
   renderer differences into a data-format contract. `pose`, `origin`, `view`
   and the existing timed framing/focus assertions compare semantic numbers
   within tolerance instead.
6. **Wait for every binding.** The extension's experimental level requires a
   reference implementation and outcomes, not uniform language parity. Rust
   is the reference track evaluator for the producer that consumes it. The
   other bindings keep their 0.3 camera/shot surfaces and strict authoring
   checks; they must not claim 0.4 evaluation until they pass these outcomes.

## The cost

One component and a small typed channel/key vocabulary; a strict parser that
must give useful field pointers; one pure sampler that must remain separate
from the behavior layer; and timed outcome plumbing through the existing
camera math. Hierarchies and aimed cameras make sampling the whole manifest
at one time essential — evaluating only the selected entity would quietly
produce the wrong origin, aim and focus.

The scope discipline is also a cost: no clip blending, quaternion tracks,
per-axis streams, arbitrary Béziers, skeletal tracks, lens keys, events or
ping-pong. Those omissions keep 0.4 reviewable and deterministic. The
extension remains experimental until its normal governance requirements are
met; numeric outcomes, not a published package or a rendered screenshot,
are the 0.4 support gate.
