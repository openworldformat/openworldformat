# RFC: Capability tiers — alternates, priorities, and the branch tree as the fallback

**Status:** proposed. Nothing here is normative yet; each mechanism
lands (or doesn't) with the implementers it earns, per
[CONTRIBUTING](../../CONTRIBUTING.md). The reference renderer already
demonstrates two of the three (see *Implementation status*).

## The problem

The same package opens in a phone browser, a desktop with a local
model, and a CI box with no GPU and no LLM. The format guarantees the
floor — the Viewer profile is required of everyone, must-ignore says
never reject a package for content you don't know, snapshots make
seeking cheap, and soundtrack *curves* mean modulations run without
DSP. What the format doesn't give the weak device is a **menu**: no
way to say "here is a cheaper drawing of this entity, and which lights
matter most."

The LLM is the sharpest case. A world whose interactivity is a model
in the loop (a narrator, a gen-mode host) is a world a phone cannot
run — unless the model's work happened before shipping and traveled
with the package.

## The principle

A document format does not probe devices. Capability *negotiation* is
the app's job; the document's job is to carry the options. Three
mechanisms, each small, each riding must-ignore:

### 1. Alternates — the `srcset` pattern

A representation MAY name its cheaper sibling, and the renderer picks.
The first alternate: a mesh asset reference gains an optional
`fallback` — a parametric shape. A renderer that can't load meshes (or
fails to fetch one) draws the fallback instead of nothing; a strong
renderer draws the mesh. HTML's `srcset` is the model: one document,
many devices, no negotiation protocol.

Conformance shape: a "same silhouette" world — mesh entities with
fallbacks must draw *something recognizable* on both tiers, not the
same pixels.

### 2. Priorities — drop from the tail, not at random

Lights and audio emitters MAY carry a `priority` number. A constrained
renderer that can afford three lights knows *which* three: the highest
priority first, document order breaking ties. One number per entity;
the policy stays the renderer's. Without it, a weak device drops
whatever it built last — which is authorship order, i.e. chaos.

### 3. The branch tree is the LLM fallback

Branching histories were designed for exploration, but a pre-generated
branch tree is also a behavior tree: a world that ships recorded tips
("if the player asks for a storm, fold to tip `storm`") behaves
model-lessly — the generation happened authoring-time and lives in the
package. Runtime LLM becomes an accelerator, not a dependency.

`package.json` says what the world can use and what it needs:

```json
"may_use": ["llm:local", "llm:api"],
"requires": []
```

A device with a local model generates live and logs `tool` ops; a
device with an API key does the same over the network; a device with
neither folds the recorded branch — and the player cannot tell from
the inside which one happened, because the document can't either.

This also gives the branching RFC the producer it has been waiting
for: today no producer writes branches, and a model-less fallback
world is the first reason to.

## What stays out

No device fingerprinting, no capability strings in the document, no
"quality" presets, no negotiation protocol, no renderer *requirements*
beyond must-ignore. The core stays a small mandatory core plus
profiles — never maximalism.

## Implementation status

- **Reference renderer** (`js/src/render.js`): honors `fallback`
  shapes on mesh entities (drawn when the mesh can't load — the
  placeholder wireframe becomes the declared silhouette) and drops
  lights from the tail by `priority` when over its punctual-light
  budget. Both read defensively; a world without the fields behaves
  exactly as before.
- Not yet: schema fields for `fallback` and `priority` (they are
  proposals until a second implementer wants them — the registry's
  discipline applies to core fields too), audio-emitter priorities,
  `may_use`/`requires` in `package.json`, a conformance world pinning
  the same-silhouette rule, and the first model-less branch-tree world.
