# The state document (L0)

A world's *document* says what the scene is. Its *state* says where the
game is: score, inventory, quests, doors opened. Without a state
document those live in engine memory and die with the process; with one,
a **save game is base + a player's log** — the oldest result in this
format.

`state.json` declares the world's state and its initial values, in one
file:

```json
{
  "format_version": 1,
  "fields": {
    "score.tour":  { "type": "int",    "initial": 0 },
    "quest.lanterns_lit": { "type": "int", "initial": 0 },
    "inventory":   { "type": "map",    "initial": {} },
    "has_map":     { "type": "bool",   "initial": false }
  }
}
```

## Rules

- **Fields are declared, typed, namespaced.** A dot-prefix is a
  namespace by convention (`score.`, `quest.`, `inv.`); a world's own
  state SHOULD stay in one namespace. Types: `int`, `float`, `bool`,
  `string`, `map` (string keys to any JSON value), `list`, `json`
  (anything).
- **The log carries deltas.** A `state` op maps dotted keys to values:
  `{"state": {"score.tour": 3}}`. A key naming a declared field sets it,
  and `null` resets it to its initial value. A key *under* a declared
  `map` field (`inventory.rope`) sets that entry, and `null` removes it.
- **State folds separately from the document.** `state` ops never touch
  the world document (the fold's rule); folding state is its own pass:
  `foldState(state.json, state ops) → values`. A world renders from the
  document; a *game* runs on both.
- **The fold tolerates undeclared keys** — it carries them and a
  validator flags them — because history must never break the fold. A
  producer that writes state ops SHOULD declare the fields it writes.

## What it's for

- **Save games**: base world + state declaration + the player's session
  log (edits they made, state they changed, inputs they gave).
- **Host state in multiplayer**: the score every visitor sees is state
  ops through the same authority as edits.
- **Triggers**: `TriggerDef`'s `requires_item` and host actions like
  `add_score` read and write declared fields, so interactivity survives
  the process that ran it.

The state document is new (draft 0.1): the log's `state` op kind is
stable, the declaration above is the proposed shape, and producers
SHOULD treat it as experimental until 1.0 pins it.
