# The world document (L0)

A world document — `manifest.json`, or a snapshot — is a self-contained
description of a world: its entities, their shapes and materials, the
lights, the environment, the sound, and how the world behaves. Its data
model is [`world.schema.json`](../schema/world.schema.json); this page
states the rules the schema cannot.

## Conventions

Every renderer follows these, so two renderers draw the same world the
same way (the conformance worlds hold them to it):

- Positions are world units (meters at 1:1 scale), **Y up**.
- Rotations are **intrinsic** XYZ Euler angles in **degrees** (applied in X, Y, Z order relative to the local moving frame).
- Colours are RGBA in `0..=1`, **sRGB-encoded**, except `emissive`,
  which is **linear** (values above 1 glow).
- Directional light intensity is **lux**; point and spot lights are
  **lumens**; spot angles are **radians**.
- Asset paths are relative to the package's `assets/` folder.

The format names no engine. Which renderer drew a world is not part of
the world.

## Identity: every entity has an id and a name

An entity has a stable numeric `id` (monotonic, never reused within a
world) and a unique, human-readable `name`. Cross-entity references —
behaviors, parenting, audio attachment, orbit centers — may be written
by **name** (what authors and models produce) and are resolved to **id**
on ingestion; saved worlds contain ids. Never address entities by array
position: positions are a serialization detail, ids are the contract.

## Entities

An entity is a transform, and any of: a shape, a material, a light,
audio, behaviors, triggers, modulations, an instanced creation, or a
mesh asset reference. All the optional parts are independent — a light
can have no shape; an audio emitter can be invisible.

### Shapes are parametric

`Cylinder(radius, height)`, not triangles. Parametric shapes survive
round-trips unchanged: an editor reads `radius: 0.2, height: 3.0` and
edits exactly those numbers. Baking to meshes is what **exports** do
(glTF); it is never what the source document does.

Complex geometry is a **mesh asset reference**: a relative path, the
SHA-256 of the file, an optional node within it, and per-node overrides
(hide, recolour) applied to named nodes of that asset for this entity
only.

### Reuse: creations and instances

A creation is a named, reusable tree of entities. An entity with
`instance_of` places a copy; its overrides are patches keyed by part
name (add, reshape, recolour, remove). Instances expand to parts named
`<instance>/<part>`, in a fixed order, so renderers agree on the result.

### Behavior and triggers

Behaviors (`orbit`, `spin`, `bob`, `look_at`, `pulse`, `path_follow`,
`bounce`) are declarative constraints that run from load, referencing
other entities by id. Triggers wait for an event (`start`, `click`,
`proximity`, `area_enter`, `area_exit`, `collision`, `timer`) and run an
action (`show_text`, `show`, `hide`, `toggle`, `remove`, `animate`,
`teleport`, or a host action). Actions a renderer doesn't understand are
skipped, never fatal.

### Audio and performance

Audio is procedural descriptions (wind, rain, water with a turbulence
parameter…) or asset samples, ambient or spatial. Soundtracks carry
analysis curves (BPM, sections, energy and stem envelopes) and
modulations bind entity properties to signals (`energy`, `bass`,
`beat`, stems) — so a world can *perform* a piece of music without
shipping the recording.

## Scene-wide settings

Environment (background, ambient, fog), camera, avatar (spawn point,
point of view, speed), and tours (named waypoint sequences) are part of
the document, not renderer state.
