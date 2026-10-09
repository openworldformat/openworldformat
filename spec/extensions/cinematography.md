# The cinematography extension (`ext-cinematography`), version 0.2

**Status: proposed.** The name is reserved and this page is the
contract; it becomes experimental with a reference implementation and
conformance cases, and leaves experimental when a producer ships a
package using it ([CONTRIBUTING](../../CONTRIBUTING.md),
[the registry](registry.json)).

> Version 0.1 of this page promised spline camera paths, keyframed
> animation tracks and a `focus_target` — the whole NLE at once, and
> none of it implemented. Version 0.2 scopes to what previs checks
> first: cameras with a real filmback and lens, and shots as ordered
> setups on the clock. Keyframed tracks return as
> `ext-cinematography.track` when a producer needs animatics.

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
| `focus_distance_m` | m | none | > 0 |
| `f_stop` | | none | > 0 |
| `shot.scene` | text | none | free text |
| `shot.order` | integer | none | shot list sort key; ties break by entity id |
| `shot.in_s`, `shot.out_s` | s on the clock | none | `in_s` ≤ `out_s` |
| `shot.size` | text | none | free text; conventionally EWS, WS, MS, MCU, CU, ECU, INSERT, OTS, POV, TWO |
| `shot.description` | text | none | free text |

Defaults apply per absent field, so `"camera": {}` is a 35 mm lens on
Super 35 looking down the entity's local −Z.

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
(a renderer without the extension draws the set). An implementation
claims this extension by passing the outcomes; the assertion kinds are:

| Assertion | Checks |
|---|---|
| `fov` | a camera's derived horizontal and vertical FOV and frame aspect, within `tolerance` (degrees) |
| `in_frame` | an entity's origin projects inside a camera's frame |
| `out_of_frame` | it doesn't |
| `shot_list` | the cameras with a `shot`, in order |

## Reference implementations

Three, running the same conformance outcome assertions in their own CI:

- **JS** — `openworldformat/cinematography` (in this repository,
  `js/src/cinematography.js`): `cameraOf`, `frameOf`, `viewOf`,
  `project`, `shotList`, and the outcome runner. The reference renderer
  looks through a shot (`opts.shot`, an entity name): vertical FOV and
  frame aspect from the math above, the canvas letterboxed to it.
- **Rust** — `rust/src/cinematography.rs`, the same surface.
- **Python** — `python/openworldformat/cinematography.py`, the same
  surface.

They are deliberately minimal — the crop math, a look-at view matrix, a
pinhole projection — so the conformance cases are executable by anyone.
Production renderers bring real depth of field and motion blur; the
format promises the numbers, not the bokeh.
