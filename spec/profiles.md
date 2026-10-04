# Profiles and extensions

Cover use cases through a small mandatory core plus profiles — never
through maximalism. An implementation implements a profile; a package
declares which profiles it uses.

## Profiles

| Profile | Adds | For |
|---|---|---|
| **Viewer** (core, required of everyone) | manifest, entities, parametric shapes + mesh refs, PBR materials, punctual lights, environment, hierarchy, ids/names, versioning | any renderer, converter, gallery |
| **Player** | behaviors, triggers, ambient/emitter audio, soundtrack + modulation, avatar, tours, the state document | games, walkable documents, song worlds |
| **Session** | the ops log, revisions, authorship, snapshots, undo semantics | multiplayer, recordings, editors, save games |
| **Authoring** | an authority's ingestion of batches (names bound, ids allocated, struct patches merged as JSON Merge Patch, strict reading, assets stored, all-or-nothing), the head-first write order, the canonical text | editors, agent canvases, command-line tools — anything that changes a package |

## The must-ignore rule

An implementation ignores what it does not understand and still renders
the core: unknown profile, unknown op kind, unknown entity field,
unknown extension. **Never reject a package for content you don't
know.** The failure modes this rule forbids are real: the format's
lineage dropped triggers and instanced parts on version-skew once, and
that bug is why schema versions gate parsing loudly while everything
else gates softly.

### Strict Mode for Authoring

While runtime consumers (viewers, players) MUST follow the must-ignore rule, authoring tools and validators SHOULD implement a "Strict Mode". In Strict Mode, unknown fields, unregistered extensions, and unmapped properties are treated as validation errors rather than ignored. This prevents silent typos during world generation and ensures emitted packages are fully compliant.

An authority implementing the Authoring profile MUST read the batches it
is sent strictly: a key the format would drop is a refusal with a JSON
pointer to it ([the session log](session.md), "Authoring"). An author is
told about its typo; a viewer reading the world later never has to be.

## Extensions

Extensions are namespaced (`ext-physics`, `ext-avatars`, …) and carry
their own version. The registry (this repository's governance process,
see [`CONTRIBUTING.md`](../CONTRIBUTING.md)) accepts a namespaced
extension only with:

1. a specification page under `spec/extensions/<name>.md`,
2. a reference implementation, and
3. conformance cases.

The registry is [`spec/extensions/registry.json`](extensions/registry.json).
Its first entry is [`ext-physics`](extensions/physics.md) 0.1 —
experimental, tracking the glTF drafts, one implementer so far. Until
an extension is registered, its fields are reserved for experiment, and
producers MUST NOT ship them in packages marked `format_version: 1`.

Extensions align with adjacent standards rather than competing with
them: physics extensions SHOULD track the glTF physics drafts
(`KHR_physics_rigid_bodies`, `KHR_implicit_shapes`) and the OMI
proposals; avatars SHOULD use VRM.
