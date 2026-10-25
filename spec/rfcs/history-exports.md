# RFC: Exporting a build history from another system — the prism, the source label, day-coarse time

**Status:** accepted. The prism left experimental on 2026-10-25, once
LocalGPT's Bevy renderer and the worldwalk viewers built it from the
geometry table beside the reference three.js viewer. The
normative text is in [the world document](../world.md) ("The geometry
of the shapes"), [the session log](../session.md)
("The entry") and [the package](../package.md) ("Exports from other
systems"); this note states the problem, the alternatives and the cost,
per [CONTRIBUTING](../../CONTRIBUTING.md). It lands with
[`ext-h3`](ext-h3.md): one producer's brief, three generic additions.

## The problem

A producer that keeps its own live truth — rows in a database, validated
by its own rules — wants to export what people build, with the history of
how they built it, as a `.world`: a snapshot any viewer draws, and a log
any session reader replays, step by step, outside the producer's own
apps. The format already is that envelope: `manifest.json` is the head,
`ops.jsonl` is the replayable history, each entry has an author. Three
things stood in the way.

- **A shape the format lacked.** The producer's cell-filling shapes are
  hexagonal and pentagonal prisms. The format had eleven parametric
  shapes and no polygonal prism, so every such row would have exported
  as a mesh asset — a glTF leaf standing in for two numbers — and
  re-imported as nothing.
- **A disclosure the entry could not carry.** The producer labels every
  build event by how it came to be — by hand, from a library of
  ready-made content, with a model's help — and shows that label to
  everyone, neutrally and always. The entry's `author` says who; nothing
  said how.
- **Time as a trail.** A per-event timeline on a place tells when a person
  was where. The producer coarsens public timestamps to the day; the
  format's entry has `timestamp_ms` and no word on precision, so an
  exporter could not tell whether a day was conformant.

## The alternatives

**The shape.** (1) A mesh asset per prism — exact, heavy, and lossy on
import. (2) A `sides` parameter on `Cylinder` — the smallest change, but
it turns every cylinder reader into a prism reader by surprise, and a
cylinder with eight sides is not a cylinder; readers that skip the field
would draw a smooth tube where a flat-sided tower was meant. (3) A
`Prism { sides, radius, height }` variant (chosen): externally tagged
like every shape, so a reader that lacks it skips the shape instead of
drawing something wrong; `sides` is the only segment parameter, 3..8,
the two cases a producer needs today plus the ones between. The geometry
is stated in prose for the first time — origin, axis, the first vertex,
winding, caps — because two renderers drawing a prism from a radius and a
side count can disagree in four ways a cylinder cannot. It was marked
experimental under the format's own ladder until a second renderer drew
it, and left the ladder when three renderers built it from the table.

**The label.** (1) `ext-provenance`: the manifest-level lineage block.
Its fields are about a world's generation — prompt, model, duration — and
it lives on `meta`; a per-entry label would need the block on every
entry, carrying nothing but one word, and would tie "a person placed a
library item" to an LLM-provenance namespace it has nothing to do with.
(2) A `tool` history op beside each edit: `tool` records a call, its
arguments and its result hash, which is the wrong grain and the wrong
privacy (the arguments may be the prompt). (3) A key on the entry's
`author` (chosen): "what did the model do here, versus the people?" is
already a filter on `author`, and the label is a fact about the actor.
Three values — `hand`, `library`, `ai` — and unknown values read as no
label, so the vocabulary can grow without a bump. The field is part of
the entry's identity like every other entry field; the five references
carry it (four as passthrough, the typed Rust reference as a field).

**Time.** State the rule the entry always implied: the format requires no
precision. A writer may coarsen a timestamp — to the day, for a public
history — and a reader never assumes a timestamp is finer than written.
No new field; `timestamp_ms` stays, in milliseconds, holding the first
instant of the day when that is all the writer says.

**The prompt.** `ext-provenance` already says tools circulating worlds
MUST warn or scrub the `prompt` field before publishing. For an exporter
writing a package for others, that becomes the default: no prompt unless
the author opts in. The producer behind this RFC never stores the prompt
at all; the format should not make an exporter choose to leak one.

## The cost

A new shape variant in the schema (additive: a minor schema change, no
version bump — a version-3 reader that lacks the prism skips the shape,
per must-ignore), one optional entry field, and prose. The
prism costs renderers a geometry routine and a budget estimate; the label
costs exporters one word per entry and gives every session reader a
hand/library/ai legend for free. Day-coarse time costs nothing and makes
explicit that a replay's clock is the recorded `clock` entries, never the
entry timestamps.
