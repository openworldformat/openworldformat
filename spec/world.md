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

### The geometry of the shapes

The shapes are `Cuboid`, `Sphere`, `Cylinder`, `Cone`, `Capsule`,
`Torus`, `Plane`, `Pyramid`, `Tetrahedron`, `Icosahedron`, `Wedge` and
`Prism`. The schema states each one's parameters and
defaults; the geometry is stated here, not inherited from any engine.
Two renderers drawing a shape from its parameters have more to disagree
about than its dimensions — which way a wedge faces, where an apex is,
the plane a torus lies in — and each of those has been disagreed about
by this format's own renderers. The common rules, then each shape:

- **Origin and axes.** Every shape is centred on the origin of its
  bounding box, with its axis of symmetry along local +Y where it has
  one; the conventions above apply (Y up, metres).
- **Winding.** Every face winds counter-clockwise seen from outside
  (outward normals). A plane's front face is +Y.
- **Segments.** Shapes of revolution — sphere, cylinder, cone, capsule,
  torus — are drawn with at least 24 segments around their axis, and the
  phase of a ring around the axis is the renderer's: none of their rim
  vertices is pinned, only their bounds and the points on the axis.
  Every other shape is drawn flat, with exactly the vertices stated.
- **Bounds.** A shape's box is stated with it, as half-extents; the
  references' `local_aabb_half` is that box, conservative where the
  exact extent is smaller.

| Shape | Geometry |
|---|---|
| `Cuboid { x, y, z }` | The box `[−x/2, x/2] × [−y/2, y/2] × [−z/2, z/2]`. |
| `Sphere { radius }` | Centred, with vertices at the poles `(0, ±radius, 0)`. Box (radius, radius, radius). |
| `Cylinder { radius, height }` | Axis +Y, flat caps at y = ±height/2. Box (radius, height/2, radius). |
| `Cone { radius, height }` | A base disc of `radius` at y = −height/2 and the apex at `(0, height/2, 0)`. Box (radius, height/2, radius). |
| `Capsule { radius, half_length }` | A cylinder of `radius` from y = −half_length to +half_length, closed by hemispheres; poles at `(0, ±(half_length + radius), 0)`. Box (radius, half_length + radius, radius). |
| `Torus { major_radius, minor_radius }` | The ring lies flat in the XZ plane, around +Y: the tube's centre circle has radius `major_radius` at y = 0 and the tube radius `minor_radius`, so `(major_radius ± minor_radius, 0, 0)` and `(major_radius, ±minor_radius, 0)` are on it. Box (major + minor, minor, major + minor). |
| `Plane { x, z }` | The rectangle `[−x/2, x/2] × {0} × [−z/2, z/2]`, front face +Y. Box (x/2, 0, z/2). |
| `Pyramid { base_x, base_z, height }` | The base `[−base_x/2, base_x/2] × [−base_z/2, base_z/2]` at y = −height/2 and the apex `(0, height/2, 0)`: four side triangles and a two-triangle base, outward. Box (base_x/2, height/2, base_z/2). |
| `Tetrahedron { radius }` | The regular tetrahedron with circumradius `radius` whose vertices are the four cube corners with an even number of negative coordinates: `radius·(±1, ±1, ±1)/√3` for `(+,+,+)`, `(+,−,−)`, `(−,+,−)` and `(−,−,+)`. Exact extent ±radius/√3 on every axis; box (radius, radius, radius). |
| `Icosahedron { radius }` | The regular icosahedron with circumradius `radius` on the three golden rectangles: vertices `(±1, ±φ, 0)`, `(0, ±1, ±φ)` and `(±φ, 0, ±1)` scaled by `radius/√(1 + φ²)`, φ the golden ratio. Exact extent ±radius·φ/√(1 + φ²) (0.851·radius) on every axis; box (radius, radius, radius). |
| `Wedge { x, y, z }` | Half a box cut diagonally: a right-triangle profile with its vertical face at x = −x/2 (full height, y from −y/2 to +y/2), its base at y = −y/2 (x from −x/2 to +x/2) and its slope from `(−x/2, +y/2)` down to `(+x/2, −y/2)`, extruded along Z from −z/2 to +z/2. Its six vertices are `(−x/2, ±y/2, ±z/2)` and `(+x/2, −y/2, ±z/2)`. Box (x/2, y/2, z/2). |
| `Prism { sides, radius, height }` | A right prism on a regular polygon with `sides` sides, 3 to 8 (6 is the hexagonal prism, 5 the pentagonal one); `sides` is its only segment parameter. Axis +Y, flat caps at y = ±height/2. `radius` is the circumradius, centre to vertex: cap vertex *i* sits at `(radius·cos θ, ±height/2, −radius·sin θ)` with θ = 2π·i/sides — the first vertex on +X, the ring counter-clockwise seen from above. Each side is one planar quad, each cap a fan from vertex 0, all outward, sides flat-shaded. Box (radius, height/2, radius), the inscribing cylinder's. |

[`conformance/geometry/shapes.json`](../conformance/geometry/shapes.json)
pins this table: for one instance of every shape, the exact bounds, the box, and
landmark points that must be vertices of a renderer's mesh — an apex, a
wedge's six corners, a torus's equator, a prism's first vertex, a
polyhedron's orientation. The reference renderer runs it in CI, the Rust
reference checks the boxes, and any renderer can run it; a renderer
whose mesh fails a landmark is drawing a different shape, whatever its
dimensions say.

**The prism is a core shape.** It entered draft 0.3 as experimental,
under the ladder [profiles](profiles.md) sets for extensions applied to
a core shape, with one renderer drawing it (the reference three.js
viewer), and left experimental once two more renderers built it from
this table: LocalGPT's Bevy renderer, whose tests check the mesh against
the box, and worldwalk's on RealityKit and on Android, whose Android
tests run the geometry fixture. A Viewer-profile claim draws it like any
other shape, and [`conformance/prism.json`](../conformance/prism.json)
is part of the Viewer suite. A reader older than the shape skips it —
must-ignore, never an error. The RFC is
[`rfcs/history-exports.md`](rfcs/history-exports.md).

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
