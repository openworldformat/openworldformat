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

## The must-ignore rule

An implementation ignores what it does not understand and still renders
the core: unknown profile, unknown op kind, unknown entity field,
unknown extension. **Never reject a package for content you don't
know.** The failure modes this rule forbids are real: the format's
lineage dropped triggers and instanced parts on version-skew once, and
that bug is why schema versions gate parsing loudly while everything
else gates softly.

## Extensions

Extensions are namespaced (`ext-physics`, `ext-avatars`, …) and carry
their own version. The registry (this repository's governance process,
see [`CONTRIBUTING.md`](../CONTRIBUTING.md)) accepts a namespaced
extension only with:

1. a specification page under `spec/extensions/<name>.md`,
2. a reference implementation, and
3. conformance cases.

The registry is [`spec/extensions/registry.json`](extensions/registry.json)
— empty by design until the first extension earns its entry; physics,
tracking the glTF drafts, is the expected first. Until an extension is
registered, its fields are reserved for experiment, and producers MUST
NOT ship them in packages marked `format_version: 1`.

Extensions align with adjacent standards rather than competing with
them: physics extensions SHOULD track the glTF physics drafts
(`KHR_physics_rigid_bodies`, `KHR_implicit_shapes`) and the OMI
proposals; avatars SHOULD use VRM.
