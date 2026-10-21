# RFC: The geo frame extension (`ext-h3`)

**Status:** proposed (version 0.1). The normative text is
[`spec/extensions/h3.md`](../extensions/h3.md); this note states the
problem, the alternatives and the cost, per
[CONTRIBUTING](../../CONTRIBUTING.md). It lands with the prism shape and
the export contract of [`history-exports.md`](history-exports.md): the
three are one producer's brief, and each is a generic addition.

## The problem

The format places things in local metres on a flat ground, and says so:
planet-scale is out of scope for the core. A producer whose worlds live
on a globe keeps its truth another way — every object sits in a cell of
the H3 hexagonal grid, with a transform relative to that cell quantised
to integers (hundredths of an edge, hundredths of a degree, log2 scale),
so that every client agrees bit for bit and nothing drifts. Such a
producer can export a world to `.world` by flattening a place to metres
(the ground is flat to centimetres within a kilometre), and any viewer
then draws it. What the export loses is the cell: a reader that *is*
globe-aware — another client of the same producer, a globe viewer, a
re-import — cannot put the world back where it stood, and a round trip
through the format would re-quantise floats and move things by a unit.

## The alternatives

1. **Latitude, longitude and altitude on each entity, as floats.** The
   obvious geo extension, and the wrong one for this producer: its truth
   is integers in a cell frame, and floats would be a second quantisation
   with its own drift. A reader that wants degrees asks H3 for the cell's
   centre; the cell is the smaller, exact fact.
2. **A frame on the environment only, entities in local metres.** Enough
   to place a whole package on the globe, not enough to put each entity
   in its cell: the producer's import pipeline and its moderation act per
   cell, and the integers would be gone. The frame is kept (it is what
   relates the flattened metres to the globe) and the placement is kept
   too.
3. **The cell plus the integer transform, beside the core transform**
   (chosen). The same bits the producer stores; a globe reader places by
   cell and ignores the floats; a flat reader draws the floats and
   ignores the cell. The cost is two truths in one entity, paid for by
   one rule: the H3 fields are authoritative and the core transform is
   derived from them, with the derivation stated exactly so a reader can
   check a package and an authority can keep an edited one consistent.
4. **Where the frame lives.** `meta` holds what a world is about, the
   environment holds the world's physical settings; `ext-physics` put
   gravity on the environment for the same reason, and the typed
   reference already carries extension keys there, so no type changes.

## The cost

A namespace on entities and the environment, no op kinds, no schema bump:
a reader without the extension draws the flattened world. The one rule
costs producers a derivation they already perform (the flattening) and
costs knowing authorities a consistency check on edits. The unit table
pins H3 version 4's mean edge lengths into the spec — a dependency on a
published constant, taken deliberately so that a reader without an H3
library can still convert units, and checked by an outcome assertion so
the number cannot drift. The reference implementations embed no H3
library: cell centres come in as data, which keeps the five references
engine-free and makes the conformance fixture runnable anywhere.
