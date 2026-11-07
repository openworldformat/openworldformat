# RFC: Pivots, phases and the swing — and what a translucent colour means

**Status:** proposed, implemented in the JS renderer and the Rust
reference's types; it lands when a second renderer draws the pivots and
the swing, per [CONTRIBUTING](../../CONTRIBUTING.md). The normative text
is in [the world document](../world.md) ("Conventions", "Behavior and
triggers"); `conformance/behaviors.json` and `conformance/materials.json`
pin it.

## The problem

A producer exporting its moving blocks found three gaps, each a block
that looks wrong or does not move in a `.world`:

1. **Nothing turns about a point but its centre.** `spin` turns an
   entity about its own origin, which for every shape is the middle of
   its box. A blade that wheels about its base, a propeller on its hub,
   a door on its hinge have no way to say where the axis passes. The
   producer's wheel was dropped at export, since there was nothing to
   write.
2. **Nothing swings.** The format has rotations that run forever and
   translations that oscillate, and no rotation that oscillates: a
   hanging sign, a bell, a seat on its ropes. Dropped at export too.
3. **A translucent colour meant two things.** A colour with alpha below
   1 and no `alpha_mode` was blended by the reference renderer and drawn
   opaque by the other shipping renderer, and the spec said nothing. A
   pane of glass was glass on one screen and a wall on the next.

Two smaller ones came with them: `spin` and `pulse` had no phase, so a
set of parts that move together in the producer could not be told to
in the format (`bob` and `orbit` had one); and `pulse` scaled about the
centre where the producer scales about the bottom, so a breathing block
sank into the ground and rose out of it.

## The alternatives

1. **A parent node per pivot.** The hierarchy exists: an empty entity at
   the hinge, the blade its child, `spin` on the parent. Rejected: two
   entities for one thing in every export and import, the child's
   placement no longer the producer's row, and still no swing.
2. **A keyframed track behavior.** General, and far more than three
   gaps ask for; the cinematography extension deferred tracks for the
   same reason. Not now.
3. **A `pivot` on the behaviors that turn or scale, a `phase` where it
   was missing, and a `swing`** (chosen). The pivot is a point in the
   entity's local frame, metres before the transform's scale, so a
   shape's base is `[0, -height/2, 0]` at any size and any turn; the
   rule that the pivot stays where the authored transform put it is one
   sentence, and the same sentence for all three behaviors. `swing` is
   `bob`'s wave applied to an angle. Every field is optional with a
   default, so a world written before them reads and writes back as it
   was.
4. **For the colour: require `alpha_mode`.** A producer that forgets
   still gets two renderings. The rule chosen reads the alpha when the
   mode is absent — what the reference renderer always did, what a
   person expects of a half-transparent colour, and what the other
   renderer now does too; `alpha_mode` set still wins.

## What else this changes

The Rust reference used to refuse a whole manifest over a behavior kind
it did not know — a `swing` written today would have closed every
reader on the published crate. That was never the format's rule
(must-ignore: "never reject a package for content you don't know"), and
it was the JS renderer's practice already. `BehaviorDef` now carries an
unknown kind through the fold untouched and never runs it; the schema
still names only the kinds a reader can run.

## The cost

Four optional fields and one behavior kind, no schema version bump
(additive, and a reader may ignore them). Renderers that run behaviors
pay the pivot's one displacement per frame and the swing's one sine;
the Rust crate takes a `0.x` minor bump, since a consumer matching the
enum exhaustively must say what it draws for a swing and for a kind it
does not know. The alpha rule costs a renderer one default. Readers on
earlier versions draw a swing at rest, a wheel turning about its
centre, and a breathing block about its centre — the world opens, the
motion is approximate, which is the contract for behaviors a reader
lacks.
