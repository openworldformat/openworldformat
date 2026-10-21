# The world document (L0)

A world document — `manifest.json`, or a snapshot — is a self-contained
description of a world: its entities, their shapes and materials, the
lights, the environment, the sound, and how the world behaves. Its data
model is [`world.schema.json`](../schema/world.schema.json); this page
states the rules the schema cannot.

## Conventions

Every renderer follows these, so two renderers draw the same geometry and lighting intent, within their own tone mapping pipelines (the format does not pin exposure or ACES/Reinhardt output transforms):

- Positions are world units (meters at 1:1 scale), **Y up**.
- Rotations are **intrinsic** XYZ Euler angles in **degrees** (applied in X, Y, Z order relative to the local moving frame).
- Colours are RGBA in `0..=1`, **sRGB-encoded**, except `emissive`,
  which is **linear** (values above 1 glow).
- Directional light intensity is **lux**; point and spot lights are
  **lumens**; spot angles are **radians**. (Renderers SHOULD convert spot light lumens to candela using the cone angle at draw time).
- Asset paths are relative to the package's `assets/` folder.

The format names no engine. Which renderer drew a world is not part of
the world.

## Identity: every entity has an id and a name

An entity has a stable numeric `id` (monotonic, never reused within a
world) and a unique, human-readable `name`. Cross-entity references —
behaviors, parenting, audio attachment, orbit centers — may be written
by **name** (what authors and models produce) and MUST be resolved to **id**
at ingestion time against the fold-so-far; saved worlds always contain ids. Delaying
name resolution until fold time is strictly forbidden because it breaks log determinism
when entities are renamed. Never address entities by array
position: positions are a serialization detail, ids are the contract.

A world's `next_entity_id` is where the next id comes from. Its
effective value is the larger of what the manifest declares and one past
its largest id, and it never goes down: deleting the newest entity does
not free its id.

**The reference fields are marked in the schema.** Every field that
holds an entity reference carries an `x-entity-ref` annotation in
[`schema/world.schema.json`](../schema/world.schema.json) —
`"bindable"` where a name is accepted at intake (it MUST bind to an id
at ingestion), `"id"` where only the numeric form is valid — and
[`schema/entity-refs.json`](../schema/entity-refs.json) is that list,
generated. Name binding, merge rewriting ([the session log](session.md),
"The merge rules, exactly") and ingestion validation MUST read every
field the list names: when the format gains a reference field, the
marker is how the five references all see it, instead of five
hand-written lists drifting apart. Extension schemas
([`schema/extensions/`](../schema/extensions/)) mark their reference
fields the same way, and the list carries them with the path rooted at
the extension's namespaced key — `ext-cinematography`'s
`camera.aim_entity` binds, rewrites and validates exactly like
`parent`, through the same list.

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

### The prism (experimental)

The shapes are `Cuboid`, `Sphere`, `Cylinder`, `Cone`, `Capsule`,
`Torus`, `Plane`, `Pyramid`, `Tetrahedron`, `Icosahedron`, `Wedge` and,
experimentally, `Prism`. The schema states each one's parameters and
defaults. The prism's geometry is stated here as well, because a radius
and a side count leave more for two renderers to disagree about than a
cylinder does:

- `Prism { sides, radius, height }` is a right prism on a regular polygon
  with `sides` sides, 3 to 8 (6 is the hexagonal prism, 5 the pentagonal
  one). `sides` is its only segment parameter; there is no other.
- Its origin is its centroid; its axis is local +Y; its caps are flat, at
  y = ±`height`/2.
- `radius` is the **circumradius**, centre to vertex: cap vertex *i* sits
  at (`radius`·cos θ, ±`height`/2, −`radius`·sin θ) with θ = 2π·*i*/`sides`
  — the first vertex on +X, the ring counter-clockwise seen from above
  (+Y).
- Every face winds outward: each side is one planar quad (two
  triangles), each cap a fan from vertex 0, and renderers draw the sides
  flat, not smoothed across the edges.
- Its bounding box is the inscribing cylinder's: half-extents
  (`radius`, `height`/2, `radius`).

**Status.** `Prism` is experimental, under the ladder
[profiles](profiles.md) sets for extensions, applied to a core shape:
one renderer draws it (the reference three.js viewer), and it leaves
experimental when a second renderer draws
[`conformance/prism.json`](../conformance/prism.json). Until then a
Viewer-profile claim does not require it, `shapes.json` does not include
it, and a reader without it skips the shape — must-ignore, never an
error. The RFC is [`rfcs/history-exports.md`](rfcs/history-exports.md).

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
point of view, speed), tours (named waypoint sequences), the ambient
soundscape (`ambience`), the soundtrack, creations and the world's
`meta` are part of the document, not renderer state — and each is
reachable by an edit (`ModifyWorld`, [the session log](session.md)).
