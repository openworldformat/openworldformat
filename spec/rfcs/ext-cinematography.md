# RFC: The cinematography extension (`ext-cinematography`)

**Status:** proposed (version 0.2). The normative text is
[`spec/extensions/cinematography.md`](../extensions/cinematography.md);
this note states the problem, the alternatives and the cost, per
[CONTRIBUTING](../../CONTRIBUTING.md).

## The problem

The format has exactly one camera: the scene-wide `CameraDef` that says
where a visitor starts. Previs — the producer need behind this — works
in the other direction: a set with several cameras placed in it, each a
setup with a sensor and a lens, played in an order with in and out
times. What previs checks is physical: does the 24 mm see both actors
from here, what does the 2.39 crop cut, how high is the camera, how far
from the subject. A world that cannot state those numbers cannot be a
storyboard, and every previs tool that invents its own camera model
makes worlds that mean different things in different tools.

Version 0.1 of this page promised the whole editor — spline paths,
keyframed tracks, focus targets — and implemented none of it. The
scoping lesson from [ext-physics](ext-physics.md) applies: state what a
thing *is*, in fields that are pure functions to check, and leave
motion and rendering to the engine. So 0.2 is cameras and shots; tracks
come back when a producer needs animatics.

## The alternatives

1. **One scene-wide camera with more fields.** Extend `CameraDef` with
   a filmback and a shot list beside it. This keeps cameras outside the
   world — but a previs camera *is* a thing in the set: it has a
   position you check against the walls, it rides a crane, a dolly move
   is a behavior. Making it an entity gives it all of that for free,
   and gives the format one kind of object instead of two.
2. **Angles in degrees, the way `CameraDef` does it.** State
   `fov_degrees` on each camera and never mention sensors. Small — but
   it is not what filmmakers check (they check lens and filmback), and
   the same physical camera would be written different ways by
   different authors: a 24 mm on Super 35 cropped to 2.39 has exactly
   one field of view, and deriving it beats hoping two tools derive it
   the same way. So the document states the physical setup and the FOV
   is normative math.
3. **Render comparisons for conformance.** Draw each shot and diff
   pixels. The core suite already pins rendering for the core fields;
   an extension's first job is agreeing on its own numbers. Pixels
   would make the cases un-runnable for headless implementations and
   would smuggle renderer differences into a camera contract. Outcome
   assertions — the physics pattern — check FOV, frame membership and
   the shot list by math alone.

## The cost

A new component namespace and the discipline that cameras stay
ordinary entities: no op kinds, no log changes, no schema bump — a
renderer without the extension draws the set and ignores the cameras,
which is exactly the must-ignore channel working. The aim convention
(look-at point, +Y up, else local −Z) matches what the ecosystem's
engines already do, so the math is everyone's default and nobody's
special case.
