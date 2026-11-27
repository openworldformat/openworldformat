# The cinematography extension (`ext-cinematography`), version 0.4

**Status: experimental.** Four reference implementations — JS
(`openworldformat/cinematography`), Rust, Python and Swift — run the 0.3
camera/shot conformance outcome assertions, and the JS reference renderer
looks through a shot letterboxed to its frame. Version 0.4 adds entity-local
tracks with a Rust reference evaluator and new timed outcomes; the other
bindings validate tracks at strict authoring but do not advertise track
evaluation. The extension leaves experimental when a producer ships a
package using it ([CONTRIBUTING](../../CONTRIBUTING.md),
[the registry](registry.json)).

> Version 0.1 of this page promised spline camera paths, keyframed
> animation tracks and a `focus_target` — the whole NLE at once, and
> none of it implemented. Version 0.2 scopes to what previs checks
> first: cameras with a real filmback and lens, and shots as ordered
> setups on the clock. Version 0.3 adds the two references a second
> pass needs — `aim_entity` and `focus_entity`, a camera that aims and
> focuses on an actor and stays on them when they move — as entity
> references in the core sense, bound at intake and rewritten at merge
> like `parent`. Version 0.4 adds `ext-cinematography.track`: one
> entity-local timed translation/rotation/scale track for actors and
> cameras, with strict authoring and pure time-addressed outcomes.

The core format has one scene-wide `CameraDef` (`position`, `look_at`,
`fov_degrees`) — where a visitor starts. Previs needs the other kind of
camera: many of them, each a setup with a sensor and a lens, in an
order, with timing — "12A, 24 mm on Super 35, cropped to 2.39" — and
the numbers people check on set: lens, height, distance to subject,
field of view.

Everything the extension adds rides under the `ext-cinematography` key
on entities, so the core rule — ignore what you do not understand,
never reject a package for it — makes a cinematography world harmless
to any conforming reader: a renderer without the extension draws the
set and ignores the cameras.

## What it adds

An entity is a **camera** when it carries `ext-cinematography.camera`.
It may also carry `ext-cinematography.shot`, which makes it a setup in
the shot list. The entity's transform places the camera in the set; the
entity's name is the shot's name ("12A") — names are already unique.

```json
{"id": 30, "name": "12A",
 "transform": {"position": [0, 1.6, 6], "rotation_degrees": [0, 0, 0]},
 "ext-cinematography": {
   "camera": {"sensor_mm": [24.89, 18.66], "focal_length_mm": 24,
              "aspect_ratio": 2.39, "squeeze": 1,
              "aim": [0, 1.5, 0], "focus_distance_m": 6, "f_stop": 2.8},
   "shot": {"scene": "12", "order": 1, "in_s": 0, "out_s": 4.5,
            "size": "WS", "description": "Maya and Kai at the counter"}}}
```

| Field | Unit | Default | Rule |
|---|---|---|---|
| `sensor_mm` | mm `[w, h]` | `[24.89, 18.66]` (Super 35) | both > 0 |
| `focal_length_mm` | mm | `35` | > 0 |
| `aspect_ratio` | w/h | the desqueezed sensor's | > 0; the frame is the largest rectangle of this aspect inside the sensor, centred |
| `squeeze` | × | `1` | > 0; anamorphic desqueeze |
| `aim` | world point `[x, y, z]` | none | when present the camera looks at it, +Y up; else it looks down the entity's local −Z (the glTF / three.js / Bevy convention) |
| `aim_entity` | entity reference (a name at intake, an id in the document) | none | while the entity exists the camera looks at its origin, +Y up — over `aim`; an entity that is gone leaves `aim`, then the rotation |
| `focus_distance_m` | m | none | > 0 |
| `focus_entity` | entity reference | none | while the entity exists the focus distance is the distance from the camera to its origin — over `focus_distance_m` |
| `f_stop` | | none | > 0 |
| `shot.scene` | text | none | free text |
| `shot.order` | integer | none | shot list sort key; ties break by entity id |
| `shot.in_s`, `shot.out_s` | s on the clock | none | `in_s` ≤ `out_s` |
| `shot.size` | text | none | free text; conventionally EWS, WS, MS, MCU, CU, ECU, INSERT, OTS, POV, TWO |
| `shot.description` | text | none | free text |

Defaults apply per absent field, so `"camera": {}` is a 35 mm lens on
Super 35 looking down the entity's local −Z.

### The entity references

`aim_entity` and `focus_entity` are entity references in the core
sense ([the world document](../world.md), "Identity"): the extension's
schema marks them `x-entity-ref` (`bindable`), so they appear in the
generated [`schema/entity-refs.json`](../../schema/entity-refs.json)
and every reference binds a name at intake, rewrites the id at merge
([the session log](../session.md), "The merge rules, exactly") and
refuses an unresolvable one at ingestion — an extension field with the
core's reference guarantees, through the one list, not a second
mechanism. An author writes the name; the document holds the id:

```json
{"id": 35, "name": "4A",
 "transform": {"position": [1, 1.7, 3.5]},
 "ext-cinematography": {
   "camera": {"focal_length_mm": 50, "aspect_ratio": 2.39,
              "aim_entity": 4, "focus_entity": 4, "f_stop": 2.8},
   "shot": {"scene": "4", "order": 5, "in_s": 18, "out_s": 22.5,
            "size": "MCU", "description": "Maya, tracked"}}}
```

A target that no longer exists — deleted after the camera was placed —
is not an error in the document: the camera falls back to `aim`, then
to its rotation, and to `focus_distance_m`. A renderer that wants to
flag the stale reference may; the format does not.

## Entity-local tracks (0.4, normative)

An entity MAY carry one `track` beside `camera` and `shot`. It animates that
entity only; there is no `target` or `entity` field and no inline track
version. A camera is animated exactly like any other entity. Parent,
`aim_entity` and `focus_entity` remain the extension's only entity
references. Multiple clips, blending, per-axis streams, quaternion tracks,
arbitrary Bézier tangents, skeletal/node addressing, events, lens keys and
ping-pong are outside 0.4.

```json
{"ext-cinematography": {"track": {
  "clip": {"start_s": 0, "end_s": 4},
  "pre": "hold",
  "post": "hold",
  "channels": {
    "translation": [
      {"time_s": 0, "value": [0, 0, 0], "interpolation": "linear", "easing": "linear"},
      {"time_s": 2, "value": [2, 0, 0]},
      {"time_s": 4, "value": [4, 0, 0]}
    ],
    "rotation": [
      {"time_s": 0, "value": [0, 0, 0]},
      {"time_s": 4, "value": [0, 180, 0], "interpolation": "step"}
    ],
    "scale": [
      {"time_s": 0, "value": [1, 1, 1]},
      {"time_s": 4, "value": [2, 2, 2], "easing": "ease_in_out"}
    ]
  }
}}}
```

`track` has only `clip`, `pre`, `post` and the required `channels`.
`channels` has at least one of `translation`, `rotation` and `scale`, each a
nonempty key array. Unknown fields and channels are not 0.4. Every object
level in the [generated extension schema](../../schema/extensions/ext-cinematography.schema.json)
has `additionalProperties: false`.

A key requires finite `time_s` and exactly three finite `value` numbers, and
MAY carry `interpolation` and `easing`. Key times MUST strictly increase in
document order; an evaluator MUST NOT sort or deduplicate them. Seconds match
`shot.in_s/out_s`. Translation is parent-local world units/metres. Rotation
is intrinsic XYZ Euler degrees under the core convention. Scale is
component-wise and follows core scale, including zero and negative values.

`interpolation` is `linear` (default) or `step`. `easing` is `linear`
(default), `ease_in`, `ease_out` or `ease_in_out`. Fields on a key govern the
interval from that key to the next; on the last key they have no interval.
`step` ignores easing.

An explicit `clip` requires `start_s <= end_s`. An absent clip defaults to
`[minimum key time, maximum key time]` across the present channels. Keys
outside an explicit clip remain meaningful when an endpoint is interpolated.
`pre` and `post` are `hold` (default), `base` or `loop` and govern times
strictly outside the clip. `loop` requires `end_s > start_s`; there is no
cycle count.

### Exact track evaluation

All track calculations use `f64`; conversion to core `f32` happens only at
the end. Evaluation has no wall clock, frame index, random, physics or asset
state, and no previous-result dependency. For an entity at time `t`:

1. Start with its authored local `WorldTransform` (position,
   `rotation_degrees`, scale). Visibility is untouched.
2. Resolve evaluation time `T`. Within inclusive clip endpoints, `T = t`;
   exact endpoints are not wrapped. Outside, `hold` clamps to that endpoint.
   `base` skips every channel and returns the complete base local transform.
   For `loop`, `D = end - start` and
   `T = start + ((t - start) rem_euclid D)`. Exact `t = end` evaluates at
   end; later exact cycle multiples evaluate at start. Pre-loop uses the same
   negative-multiple rule.
3. Replace each present channel's base components; absent channels retain
   base values. At/before the first key return its value; at/after the last
   return its value; at an exact internal key return that value exactly.
   Between keys `a` and `b`, `u = (T-a.time_s)/(b.time_s-a.time_s)`. `step`
   returns `a.value` until `b` is reached. Linear interpolation eases `u` to
   `e`, then computes every component as `a + (b-a)*e`:
   - `linear`: `e = u`
   - `ease_in`: `e = u²`
   - `ease_out`: `e = 1-(1-u)²`
   - `ease_in_out`: `e = 2u²` when `u < 0.5`, else `e = 1-2(1-u)²`

   Translation, Euler rotation and scale interpolate component-wise. Euler
   values are not angle-wrapped or shortest-path adjusted, so turns over 360
   degrees survive and a 180-degree interval has no tie ambiguity.
4. Apply existing behaviors in array order to the sampled local TRS, then
   compose parents as today with `p = P + R·(S⊙p)`. `Bob`, `Spin` and other
   relative behaviors MAY ride a keyed pose; an absolute behavior such as
   `PathFollow` MAY overwrite keyed translation.
5. For a shot at `t`, sample the camera, every parent and every
   `aim_entity`/`focus_entity` at that same `t`, then apply the 0.3
   precedence: living aim entity, else aim point, else local −Z; living focus
   entity, else focus distance. Keyed camera rotation matters only when aim
   does not override it.

Repeated calls at the same document and `t` MUST return the same semantic
values. The reference track-only API is pure: `transform_at` returns
base+track local TRS, and `sample_world_at` independently samples every
entity while preserving ids, names, parents, components, references and
opaque extension data. Existing `world_origin` and `view_in` operate on that
sampled manifest without applying tracks again. Cross-language conformance
uses numeric tolerances, never bit-exact or pixel-exact renderer comparison.

### Track validation and tolerant reading

Strict authoring MUST reject, with a JSON pointer to the offending field:
malformed track/channel/key types; unknown fields; empty channels or key
arrays; non-increasing or duplicate key times; wrong vector lengths; invalid
enums; `start_s > end_s`; and looping over a zero-length clip. JSON parsers
already reject NaN and infinity. Validation happens per op against the trial
world, so one bad key refuses the whole batch and leaves the base world
unchanged.

A tolerant viewer preserves unknown `track` data and renders the base
transform plus behaviors; it MUST NOT reject solely because a track exists.
A strict authority implementing 0.4 rejects malformed known tracks. Future
unknown track data follows the same preserve/must-ignore rule for readers
and strict refusal for older authors. World schema version 3 and package
format 2 are unchanged. An implementation MUST NOT claim 0.4 track support
until it passes the 0.4 outcomes.

## Derived values (normative)

With sensor `w × h`, squeeze `s`, focal length `f` and aspect `a`:

- desqueezed sensor aspect `A = w·s / h`
- frame width `W = w·s · min(1, a/A)`; frame height `H = h · min(1, A/a)`
  (no `aspect_ratio`: `W = w·s`, `H = h`)
- horizontal FOV `2·atan(W / 2f)`; vertical FOV `2·atan(H / 2f)`;
  frame aspect `W/H`

This is the crop-to-aspect rule film cameras and engines actually use
(Unreal's `CineCameraComponent`), written out: cropping never widens
the frame past the sensor, it only trims. Anamorphic is a squeeze and a
crop, nothing special.

An entity's **origin** is its `transform.position` composed through its
parents — the document's TRS hierarchy, `p = P + R·(S ⊙ p)` for each
parent up the chain, with the core's intrinsic XYZ rotation — so a mug
on a turned tray is where the renderer draws it, not where its local
position says. Every assertion below projects origins, and the
references resolve to them:

- the **aim point**: `aim_entity`'s origin while that entity exists,
  else `aim`, else none (the camera looks down its local −Z); the
  camera itself stands at its own origin
- the **focus distance**: `|origin(focus_entity) − origin(camera)|`
  while that entity exists, else `focus_distance_m`

## The contract, in three rules

1. **Cameras are entities.** They spawn, move, parent and delete like
   anything else — a crane is a parent, a dolly move is a behavior.
   Nothing about a camera writes to the log that an ordinary entity
   wouldn't.
2. **A shot is a setup, not a render.** The extension states what the
   camera *is* and when the setup runs on the clock. What a renderer
   does with it — a storyboard frame, a letterboxed preview, one angle
   in a multi-cam playback — is the renderer's business. The document
   holds no takes, no pixels.
3. **Conformance is math, not pixels.** Frame, field of view and the
   shot list are pure functions of the fields above, so implementations
   agree on numbers, within a stated tolerance — never on rendered
   output.

## Conformance

Cinematography conformance is **outcome assertions, not render
comparisons**, the same pattern as [ext-physics](physics.md): fold the
world, compute, check predicates. The machine-checkable cases live in
[`conformance/outcomes/cinematography.json`](../../conformance/outcomes/cinematography.json)
alongside the renderable world
[`conformance/cinematography.json`](../../conformance/cinematography.json)
(a renderer without the extension draws the set). Version 0.4 adds
[`conformance/cinematography-tracks.json`](../../conformance/cinematography-tracks.json)
and [`conformance/outcomes/cinematography-tracks.json`](../../conformance/outcomes/cinematography-tracks.json).
The original cases remain permanently required. An implementation claims
each version by passing that version's outcomes; the assertion kinds are:

| Assertion | Checks |
|---|---|
| `pose` | an entity's base+track local TRS at required `time_s`, within `tolerance` (metres, degrees and scale; default `1e-6`) |
| `origin` | an entity's origin through sampled parents at required `time_s`, within `tolerance` (default `1e-6`) |
| `view` | a camera's resolved `position`, `forward` and `up` at required `time_s`, within `tolerance` (default `1e-6`) |
| `fov` | a camera's derived horizontal and vertical FOV and frame aspect, within `tolerance` (degrees); time-independent |
| `in_frame` | an entity's origin projects inside a camera's frame; optional `time_s`, absent preserves static behavior |
| `out_of_frame` | it doesn't; optional `time_s` |
| `centred` | an entity's origin projects at the frame's centre, within `tolerance` (frame-half units) — what `aim_entity` promises; optional `time_s` |
| `focus` | a camera's derived focus distance, within `tolerance` (metres); optional `time_s` |
| `shot_list` | the cameras with a `shot`, in order only; time-independent |

The conformance world's `4A` aims and focuses on an actor by entity,
and its `insert` on a mug parented to a turned tray on the counter: an
implementation that reads a target's local position instead of its
origin, or ignores `aim_entity`, fails the `centred` and `focus` cases.

The 0.4 cases cover exact keys and midpoint linear TRS; step; every easing at
0.25/0.5/0.75; first/last holds; explicit clip trimming; pre/post hold/base;
forward/back loop, endpoints and exact cycle multiples; partial channels over
non-default bases; animated parents and children; moving cameras and moving
aim/focus targets; and repeated evaluation. Authoring cases in
[`conformance/authoring/cases.json`](../../conformance/authoring/cases.json)
cover every strict-validation failure and prove that a valid sibling op does
not commit beside an invalid key. The preservation fixture round-trips track
and opaque extension data while its authored base pose remains available.
Default tolerance for new track outcomes is `1e-6`; no screenshot comparison
is an upstream gate.

## Reference implementations

Four camera/shot implementations running the 0.3 conformance outcome
assertions in their own CI; Rust additionally runs the 0.4 track outcomes:

- **JS** — `openworldformat/cinematography` (in this repository,
  `js/src/cinematography.js`): `cameraOf`, `frameOf`, `viewOf`,
  `project`, `worldOrigin`, `aimPointOf`, `focusDistanceOf`,
  `shotList`, and the outcome runner. The reference renderer looks
  through a shot (`opts.shot`, an entity name): vertical FOV and frame
  aspect from the math above, the canvas letterboxed to it, the aim
  resolved through the world.
- **Rust** — `rust/src/cinematography.rs`, the same surface (`view_in`
  is the view resolved in a world; `view_of` stands alone), plus typed
  tracks and keys, strict pointer errors, `track_of`, `transform_at`,
  `sample_world_at` and the 0.4 timed outcome runner.
- **Python** — `python/openworldformat/cinematography.py`, the same
  surface.
- **Swift** — `swift/Sources/OpenWorldFormat/Cinematography.swift`,
  the same surface, for the Apple viewer.

They are deliberately minimal — the crop math, a look-at view matrix, a
pinhole projection — so the conformance cases are executable by anyone. The
JS, Python and Swift modules remain 0.3 evaluators; their strict authorities
validate known tracks but they do not claim 0.4 sampling. Kotlin likewise
has strict track authoring only. Production renderers bring real depth of
field and motion blur; the format promises the numbers, not the bokeh.
