# The game extension (`ext-game`), version 0.1

**Status: proposed.** This page reserves the name and states the shape.
No reference folds a rule set yet and no conformance case pins one; the
extension becomes experimental with a reference implementation of one
rule set and that rule set's outcome assertions, and leaves experimental
when two implementers sign off and a producer ships a package using it
([CONTRIBUTING](../../CONTRIBUTING.md), [the registry](registry.json)).
The RFC is [`rfcs/ext-game.md`](../rfcs/ext-game.md).

The core format already holds most of a board game. A game is a log:
each move is an entry whose edits spawn or move the pieces and whose
`state` op writes the board, the turn and the captures; the fold replays
it, branches hold its variations, base plus log is a save, and one
authority orders the moves of two players ([the session
log](../session.md)). What the core lacks is the move itself — the log
records what a move *did*, never what it *was* — and anyone to say
whether it was legal. This extension adds the move as an op, the rule
set it is a move in (named by this page, never shipped as code), and the
one rule that keeps the move, the pieces and the state from disagreeing.

Everything the extension adds rides under the `ext-game` key — on `meta`
(which game this world is), on entities (which cell and piece an entity
is) and as an op kind (the move) — so the core rule, ignore what you do
not understand and never reject a package for it, makes a game harmless
to any conforming reader: a reader without the extension draws the board
the producer wrote and reads the score from state, and a reader with it
folds the moves.

## What it adds

### The declaration, on `meta`

```json
"meta": {"name": "evening game", "description": "nine by nine, seven and a half komi",
  "ext-game": {"rules": "go", "version": 1, "params": {"size": 9, "komi": 7.5}}}
```

| Field | Type | Default | Rule |
|---|---|---|---|
| `rules` | string | — | a rule set this page registers (`go`, `chess`, `snake`); a producer MUST NOT ship a name this page does not list |
| `version` | integer | — | the rule set's version, as this page numbers it |
| `params` | object | `{}` | the rule set's parameters; each rule set lists its own with their defaults, and an authority MUST refuse a parameter the rule set does not name |

The declaration is document state: it reaches `manifest.json`, so the
Viewer profile knows what game a world is from the head alone, and
`ModifyWorld` sets it (`meta` is replaced whole). A package carrying it
lists `ext-game` in `package.json`'s `extensions` ([the
package](../package.md), "Exports from other systems").

### The move, an op kind

```json
{"id": "e12", "parent": "e11", "revision": 7, "timestamp_ms": 1790000300000,
 "author": {"peer": 2, "name": "kai", "source": "hand"},
 "ops": [
   {"ext-game": {"seat": "black", "move": {"cell": 40}, "notation": "ee"}},
   {"SpawnEntity": {"entity": {"id": 31, "name": "stone-7",
     "transform": {"position": [0.0, 0.11, 0.0]},
     "shape": {"Sphere": {"radius": 0.11}}, "material": {"color": [0.05, 0.05, 0.05, 1.0]},
     "ext-game": {"cell": 40, "seat": "black"}}}},
   {"state": {"game.position.40": "black", "game.turn": "white", "game.ply": 7}}]}
```

| Field | Type | Rule |
|---|---|---|
| `seat` | string | the seat the move is for — a seat id the rule set defines |
| `move` | object | what was played, in the shape the rule set states |
| `notation` | string, optional | the move in the game's own notation (SAN, an SGF coordinate) — derived, for people and round trips; a reader MUST NOT fold it |
| `tick` | integer, ticked rule sets only | the tick the move applies to; the authority assigns it ("Ticked rule sets") |
| `event` | string, in place of `seat` and `move` | something the rules made happen without a move — the authority's op, in the vocabulary the rule set defines; carries `tick` in a ticked rule set |

`ext-game` is a lowercase history kind ([the session log](../session.md),
"Compatibility"): it folds to nothing for the document fold, and a
reader that predates the extension carries it. An entry carries at most
one move or event, SHOULD write it as its first op, and carries the edits
and state deltas the move caused after it, in the same entry. The
entry's `author` is who moved; `seat` is which side. A move an engine or
a model made carries `author.source: "ai"` — the log's audit filter,
unchanged, now answers "which moves were the machine's".

### The piece, on entities

```json
{"id": 31, "name": "stone-7", "ext-game": {"cell": 40, "seat": "black"}}
{"id": 44, "name": "knight-b1", "ext-game": {"cell": 1, "seat": "white", "piece": "N"}}
{"id": 5, "name": "e4", "ext-game": {"cell": 28}}
```

| Field | Type | Rule |
|---|---|---|
| `cell` | integer ≥ 0 | the cell the entity stands on, in the rule set's indexing |
| `seat` | string, optional | whose piece; absent on a board cell |
| `piece` | string, optional | which piece, in the rule set's codes; absent where the rule set has one kind of piece |

An entity carrying `seat` or `piece` is a piece. One carrying `cell`
alone is a board cell — a square, an intersection — which is how a
viewer turns a click on the board into a move to send: the core `click`
trigger reaches the entity, the entity names the cell. Where a cell sits
in metres is the producer's layout (a grid extension may state the
derivation one day, as [`ext-h3`](h3.md) does for its cells); the format
checks pieces against cells, never against metres, and an edit to a
piece's transform changes no game.

### The state, in the `game.` namespace

A package using the extension declares these fields in `state.json`
([the state document](../state.md)), with the initial values the rule
set's initial position gives them, and the authority writes them with
every move and event:

| Field | Type | Meaning |
|---|---|---|
| `game.turn` | `string` | the seat to move; once the game is over it is whatever it last was, and `game.over` says so |
| `game.over` | `bool` | whether the game has ended |
| `game.winner` | `string` | the winning seat; `null` while playing and for a draw |
| `game.reason` | `string` | why it ended, in the rule set's vocabulary; `null` while playing |
| `game.position` | per rule set | the position in the rule set's form — chess a FEN string, Go a map of cell to seat, Snake the snake and its food |
| `game.score` | `map` | seat → number, where the rule set scores |
| `game.seat` | `map` | seat → the name of the actor holding it, as seats are taken |
| `game.ply` | `int` | moves played, passes included — the index a game record keys on |

A rule set MAY add fields under `game.` (Go's `game.captures`, Snake's
`game.length`) and lists them in its section. The fields exist so that a
reader without the rule set — a gallery, a HUD, a search index — reads
any game the same way; a reader with the rule set derives them.

## The one invariant

> **The moves are authoritative.** The cells of the pieces and the
> `game.` state after a move are derived from the moves by the rule set,
> by the authority, and written in the move's entry. A reader that knows
> the rule set MUST take the position from folding the moves and MUST
> NOT read the entities or the state as a second truth; a reader that
> does not, draws the entities and reads the state.

The invariant keeps three views of one game from disagreeing — the move
log, the pieces a viewer draws, the state a HUD shows — by naming which
one is the game. An authority that knows the rule set MUST keep the
other two consistent: a batch that moves, spawns or deletes a piece
entity, or writes a `game.` field, without a move or event in the same
entry is refused (`not a move`), stated to the author on the receipt. An
authority that does not know the rule set cannot keep them consistent,
and a reader with the rule set still has the rule above: the moves win.

## Legality at intake

An authority that knows the rule set checks every move against the
position it folds from the moves so far, before any op of the batch
applies, and a move that fails refuses the whole batch — nothing is
written, as any structural failure does ([the session
log](../session.md), "Authoring"). The reasons, one class each:

| Refusal | When |
|---|---|
| `unknown rule set` | `meta["ext-game"]` names a rule set or version this page does not |
| `bad parameter` | a parameter the rule set does not name, or a value outside its range |
| `not a seat` | `seat` is not one the rule set defines |
| `not your seat` | `game.seat` holds the seat for another actor |
| `not your turn` | `game.turn` is another seat, in a turn-based rule set |
| `game over` | `game.over` is true |
| `illegal move` | the rule set refuses the move, with its reason after a colon — `illegal move: ko`, `illegal move: in check` |
| `not a move` | an edit or a state delta that only a move may cause ("The one invariant") |

The first actor to move for a seat nobody holds takes it: the authority
writes `game.seat.<seat>` in the same entry. One actor MAY hold every
seat — a person at one board, an engine playing itself.

A reader with the rule set MAY re-check a log as it folds the moves. An
entry whose move it would have refused is reported, as an entry that no
longer applies is, and the game fold stops there: the document fold
goes on (structural replay never breaks), and the game is what the
rules say up to that entry.

## Takebacks and variations

Moves are never unmade. An authority MUST refuse an undo of an entry
that carries a move or an event; a takeback is a branch — build the
next move on the entry before, and the line it leaves is a tip. That is
also what an analysis tree is: variations, a joseki, an opening's
alternatives are branches of one log, `refs` name the lines, and the
fold at any tip is that line's position ([the session
log](../session.md), "Entry identity, forks and branches"). Branches of
a game are never merged; an authority SHOULD refuse the merge.

## Ticked rule sets

A turn-based rule set moves when a seat moves. A ticked rule set
advances on its own: its position changes `tick_hz` times a second
whether or not anyone moved, and a move is an input that takes effect
at a tick. For such a rule set:

- **The game runs on the session's transport clock.** A `clock` entry
  playing from zero starts it; tick *k* is due when the clock reaches
  *k* / `tick_hz` seconds; a paused clock pauses the game. Semantic
  replay already keys timers on `clock` entries ([the session
  log](../session.md), "Replay and determinism"); ticks follow the same
  rule.
- **The authority assigns `tick`**: the first tick not yet simulated
  when it took the batch in. A tick an author wrote is ignored. Moves
  with the same tick apply in entry order.
- **The authority writes events** for what the rules made happen — a
  snake ate, a game ended — each an entry with `event` and `tick` and
  the edits and state deltas it caused, so a reader without the rule set
  still shows the outcome. Motion between events is runtime state, as a
  physics body's path is ([`ext-physics`](physics.md), "The contract"):
  a reader with the rule set simulates it, one without draws the world
  at rest. At the ending event the authority writes every piece at its
  final cell, so the head is the end of the game.
- **Determinism is the rule set's integers.** A ticked rule set names
  its random number generator and seeds it from its `params`, never from
  `package.json`'s reserved `seed`; two readers folding the same moves
  reach the same position at every tick, bit for bit, because nothing
  in the fold is a float. The core's refusal to promise bit-exact replay
  ([the session log](../session.md)) is about engines, pixels and
  physics, and is not stretched here.

## Interchange

The move log is a game record. An exporter walks the entries with moves
from the base to a tip and writes each `notation`; the declaration, the
seats and the result map onto a record's headers; branches are a
record's variations. An importer sends moves to an authority that knows
the rule set, which derives the pieces and the state. Nothing more is
needed, so this version defines no mapping tables.

## Rule sets

A rule set is a section of this page. It states its seats and their
order, its `params`, its initial position, the shape of a move and its
notation, which moves are legal, how a move advances the position, when
the game ends with what reason and winner, the form of `game.position`,
its extra `game.` fields, how it indexes cells, and — for a ticked rule
set — its tick rate, its events and its random number generator. Rule
set versions are integers: a change to legality, to a form or to a
default is a new version, and a package names the version it was played
under. An implementation claims the extension per rule set, and a
reader that knows none still conforms as a reader without the
extension.

### `go`, version 1

Two seats, `black` and `white`; `black` moves first.

| Parameter | Type | Default | Meaning |
|---|---|---|---|
| `size` | integer 5..25 | `19` | a square board of `size` × `size` intersections |
| `board` | object | — | in place of `size`: `{"cells": N, "adjacency": [[…], …]}`, an explicit graph for a board that is not a grid (one laid on a polyhedron, or a sphere); `adjacency[i]` lists the neighbours of cell `i`, symmetric, no cell its own neighbour |
| `komi` | number | `7.5` | added to white's score |
| `superko` | `"positional"` \| `"situational"` \| `"simple"` | `"positional"` | which repetitions are illegal: any earlier board; any earlier board with the same seat to move; only the immediate recapture (the ko rule alone) |
| `suicide` | boolean | `false` | whether a move that captures nothing and leaves its own group without a liberty is legal (the group is then removed) |
| `setup` | list of `[cell, seat]` | `[]` | stones on the board before the first move — handicap stones |
| `first` | seat | `"black"` | the seat that moves first |

**Cells.** A `size` board indexes an intersection as `row × size + col`,
row 0 the top row and column 0 the left column as a game record reads
them; `notation` is the SGF coordinate, two lowercase letters, column
then row (`"dd"` is cell `3 × size + 3`). A `board` graph indexes cells
as `adjacency` lists them and carries no notation.

**Moves.** `{"cell": n}` places a stone; `{"pass": true}` passes;
`{"resign": true}` resigns. A move is legal when the cell is empty, when
the stone — after every opposing group it leaves without a liberty is
removed — has a liberty or `suicide` allows it, and when the board that
results does not repeat under `superko`.

**Advance.** Place the stone; remove every opposing group without a
liberty and add the stones removed to `game.captures.<seat>`; under
`suicide`, remove the stone's own group if it has no liberty. A pass
changes nothing on the board. Each move hands the turn to the other
seat.

**End.** Two consecutive passes end the game by area scoring: a seat's
score is its stones on the board plus the empty cells reachable from no
stone of the other seat; white adds `komi`; the higher score wins
(`score`), equal scores draw. A resignation ends the game for the other
seat (`resignation`). Every stone on the board counts as alive: this
version has no dead-stone agreement — under area scoring a dispute is
settled by playing on, at no cost.

**State.** `game.position` is a `map` of cell (as a decimal string key)
to seat, written as deltas — a capture is `null`; `game.captures` is a
`map` of seat to the stones it has taken; `game.score` is written at the
end.

### `chess`, version 1

Two seats, `white` and `black`; `white` moves first unless `fen` says
otherwise.

| Parameter | Type | Default | Meaning |
|---|---|---|---|
| `fen` | string | the standard starting position | the initial position, all six FEN fields; castling rights it names are exercised by the standard castling move, so a right the king or rook cannot exercise from the standard squares is a `bad parameter` |

**Cells.** `rank × 8 + file`: `a1` is 0, `h1` is 7, `a8` is 56, `h8` is
63. Moves name squares (`"e2"`); piece entities carry the index and a
`piece` of `K`, `Q`, `R`, `B`, `N` or `P`. `notation` is standard
algebraic notation as the PGN standard writes it.

**Moves.** `{"from": "e2", "to": "e4"}`, with `"promotion"` of `q`, `r`,
`b` or `n` on a pawn reaching its last rank (required then, refused
otherwise); castling is the king's two-square move; an en passant
capture is the pawn's diagonal move to the empty square.
`{"resign": true}` resigns. `{"draw": true}` is a claim when a draw can
be claimed for the position on the board with the claiming seat to move
— a third occurrence (`repetition`) or fifty moves by each seat without
a capture or a pawn move (`fifty`) — and the game ends; otherwise it is
an offer, standing until the other seat's next op: `{"draw": true}`
accepts (`agreement`), any move declines. A move is legal when the Laws
of Chess make it so in the position: the piece moves as it moves,
castling's conditions hold, en passant follows the double step at once,
and the seat's king is not left in check.

**End.** Checkmate wins for the seat that gave it (`checkmate`).
Stalemate (`stalemate`), a dead position — king against king, king and
bishop or knight against king, and king and bishop against king and
bishop on the same colour (`dead`) — a fifth occurrence (`repetition`)
and seventy-five moves by each seat without a capture or a pawn move
(`seventy-five`) draw without a claim. Resignation (`resignation`),
agreement and the claims above end it as stated.

**State.** `game.position` is a `string`: the FEN after the move, all
six fields. No extra fields.

### `snake`, version 1 — ticked

One seat, `p1`.

| Parameter | Type | Default | Meaning |
|---|---|---|---|
| `width`, `height` | integer ≥ 4 | `16`, `16` | the board, in cells |
| `tick_hz` | integer 1..60 | `10` | ticks a second |
| `wrap` | boolean | `false` | whether leaving one edge enters the opposite one; otherwise the edge ends the game |
| `seed` | integer 0..2⁵³−1 | `0` | the random number generator's seed, within what every JSON reader keeps exact |
| `length` | integer ≥ 2 | `3` | the snake's segments at the start |
| `growth` | integer ≥ 1 | `1` | segments a food adds |

**Cells.** `row × width + col`; `up` is row − 1, `down` row + 1, `left`
column − 1, `right` column + 1. The head starts at
`⌊height / 2⌋ × width + ⌊width / 2⌋` heading `right`, the body in the
`length − 1` cells to its left; a board that cannot hold the start is a
`bad parameter`. The first food is drawn before tick 1.

**Moves.** `{"dir": "up" | "down" | "left" | "right"}` at `tick` *k*
sets the heading for tick *k*, unless it is the reverse of the heading
at tick *k* − 1, which is ignored: the snake cannot turn back on itself.
The last move for a tick wins. No notation.

**Tick** *k* ≥ 1, in this order:

1. The new head is the head moved one cell along the heading; with
   `wrap`, modulo the board; without, a head off the board ends the game
   (`wall`).
2. The snake eats when the new head is the food cell.
3. Eating adds `growth` to the growth owed. The snake becomes the new
   head followed by its segments; then, if growth is owed, the last
   segment stays and one less is owed, and otherwise the last segment
   is dropped.
4. A new head on any other segment of the snake it has become ends the
   game (`self`).
5. On eating, `game.score.p1` grows by one and the next food is drawn:
   with `free` the cells under no segment, in increasing index, the food
   is the `below(free)`-th of them, counting from zero. No free cell
   ends the game, won (`full`).

**Random numbers.** PCG32, the XSH RR 64/32 generator of the PCG
reference implementation, with `initseq` 0:

```
mul   = 6364136223846793005          inc = 1        (all arithmetic mod 2⁶⁴)
seed: state = 0; state = state × mul + inc; state = state + seed; state = state × mul + inc
next: old = state; state = old × mul + inc
      x = ((old >> 18) xor old) >> 27, as 32 bits;  rot = old >> 59
      out = (x >> rot) | (x << (32 − rot)), as 32 bits
below(n) = (out × n) >> 32
```

Seed 7 draws `0xf2393151`, `0x7fbbcd3a`, `0xa3537acf`, `0xc9ca4c3f`,
`0xd363db3c`; with 253 free cells (the default board and snake) those
are `below` values 239, 126, 161, 199, 208. One draw per food, in order.

**Events.** `eat` at the tick it happened: the state deltas
(`game.score.p1`, `game.length`, `game.position`, `game.tick`), the food
entity at its new cell, a new segment entity. `over` at the ending tick:
`game.over`, `game.reason`, `game.winner` (`p1` on `full`, else `null`),
`game.tick`, and every segment entity at its final cell.

**State.** `game.position` is `json`: `{"head": n, "body": [n, …],
"heading": "right", "food": n, "owed": 0}`, the whole snake head first,
written at events; `game.length` and `game.tick` are `int`.

## Conformance

The extension's conformance is the fold of the moves, not pixels, and
it is per rule set: `conformance/outcomes/game-<rules>.json` pairs with a
board world `conformance/game-<rules>.json` whose `meta` declares the
rule set, and holds move lists with what must follow. The assertion
kinds:

| Assertion | Checks |
|---|---|
| `position` | after the listed moves, the folded position equals this, in the rule set's form |
| `turn`, `over`, `winner`, `reason`, `score` | the field after the listed moves |
| `accepted` | the authority takes this move for this seat after the listed moves |
| `refused` | the authority refuses it, with this refusal class and reason |
| `pieces` | the world's piece entities match the folded position — the invariant checked from both ends |
| `draws` | for a ticked rule set, the first *n* outputs of the generator for this seed |

A renderer without the extension draws the board (its conformance,
through the core render check); a reader claiming a rule set passes
that rule set's outcomes. These files arrive with the first reference
implementation, when the extension becomes experimental.

## Reference implementations

None yet. The five references admit the name in strict mode and carry
the blocks and the op through the fold untouched; none folds a rule set.
The first is planned in JS (`openworldformat/game`), the renderer's
language, and the second in Rust (`openworldformat::ext_game`) for the
inspector, each starting with one rule set and its outcomes.

## What is not here

- **Time controls.** Clocks per seat and a flag that falls want a
  producer first; the shape a second version would take is a `time`
  parameter, `game.clock.<seat>` in seconds, and a `timeout` event.
- **Hidden information.** A game whose seats see different boards is
  [`ext-visibility`](visibility.md)'s problem, not this page's.
- **Presentation.** Which cell is highlighted, how a piece slides, what
  the HUD shows — the app's, not the document's.
- **An opponent.** Who moves for a seat is an actor; a model's or an
  engine's move is a move with `author.source: "ai"`. The format carries
  no engine.
- **Rules as data or code.** A rule set this page does not name cannot
  be shipped. A declarative rule language, or a sandboxed module a
  package would carry, is the escape hatch this version does not open:
  the registry's discipline — add things when a second implementer
  needs them — applies to rule sets as it applies to extensions.
- **More of each game.** Territory scoring and dead-stone agreement,
  chess960 castling, more than one snake: each with the producer that
  needs it.
