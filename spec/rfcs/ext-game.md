# RFC: The game extension (`ext-game`)

**Status:** proposed (version 0.1). The normative text is
[`spec/extensions/game.md`](../extensions/game.md); this note states the
problem, the alternatives and the cost, per
[CONTRIBUTING](../../CONTRIBUTING.md).

## The problem

A board game is almost a `.world` already. Each move is an entry: edits
that spawn or move the pieces, a `state` op that writes the board, the
turn and the captures. The fold replays the game, inverses undo it,
branches hold variations, base plus log is a save, one authority orders
two players' moves, and a viewer draws any position without knowing the
rules. Three things are missing, and the first is the one that matters:

1. **The move itself.** The log records what a move did — a stone
   spawned, a stone deleted, a score bumped — never what it was. A
   second engine cannot re-derive the position, an analysis tool cannot
   read the game as a game, and a record in the game's own notation
   cannot be written back out.
2. **Legality.** Any author can append a batch that moves a knight
   like a queen; the authority checks structure and nothing else.
3. **Time, for games that move on their own.** The `input` op is an
   avatar sampled ten times a second and the clock is a transport
   clock; a snake that advances every tenth of a second whether or not
   anyone pressed a key has no tick to key its inputs on, and no seed
   to place its food by. The spec README defers "timed input samples"
   as a breaking bundle until a producer needs them; here is the
   producer.

## The alternatives

1. **Code in the package** — a script or a sandboxed module the
   package carries, run by the reader. Rejected. The format names no
   engine and ships no runtime; must-ignore has no meaning for code
   (a reader either runs it or has nothing); five references in five
   languages would each need a sandbox; and a package that is a
   program is neither AI-authorable nor safe to open.
2. **A declarative rule language** — board topology, movement
   patterns, capture rules, win conditions, as data. Tempting, and not
   now: chess in such a language is a project of its own, the three
   games asked for need three rule sets whichever way they are
   written, and a language with one implementer is exactly what the
   registry's discipline refuses. Nothing here precludes it: a rule set
   written in such a language would be one more name on the page.
3. **State only, no move op.** What the core does today: the app
   computes the outcome and writes it. Works, and stays the floor —
   every reader without the rule set lives on it — but as the whole
   story it is a record of effects: no legality, no interchange, no
   second engine able to say the log is honest.
4. **Rule sets by name, moves as ops, outcomes derived** (chosen).
   The move is an op in the log, the rule set is a name this page
   registers, the authority that knows it checks legality at intake
   and writes what the move caused in the same entry, and one invariant
   says which of the three views is the game: the moves. Readers that
   know the rule set fold the moves; readers that do not draw the
   entities and read the state, as they do today. Ticked rule sets
   ride the transport clock the semantic-replay rule already keys
   timers on, assign a tick on intake, and seed their own generator —
   the deferred bundle, taken as an extension op kind instead of a
   breaking change.

## The cost

A namespace on `meta` and on entities, one lowercase op kind, a
reserved `game.` state namespace, no schema bump: a reader without the
extension draws the board and reads the score, which is what it does
now. The real cost is the rule sets: an implementation that claims one
implements its rules — chess in five languages, if five references want
chess — paid per rule set, per implementer that wants it, and never by
a viewer. The page pins each rule set exactly enough to be implemented
twice and checked by outcome assertions, which is what experimental
status will require of the first one. The one cross-cutting rule, that
a piece moves only by a move, costs an authority that knows the rule
set a check on every batch, and costs one that does not nothing.
