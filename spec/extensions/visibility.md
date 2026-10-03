# Visibility and Fog of War (`ext-visibility`)

**Version: 0.1.0** (Experimental)

By default, the Open World Format treats the folded document as the absolute truth of the world for all observers. However, tabletop RPG workflows and competitive multiplayer require "secret state" or "fog of war."

## Rules

This extension introduces a visibility property to entities:

```json
{
  "ext-visibility": {
    "hidden_from": ["viewer", "player-2"],
    "requires_reveal": true
  }
}
```

1. **Client Masking:** An authoritative server MUST NOT broadcast entities to a client if that client's profile or peer ID matches the `hidden_from` list.
2. **Reveal Operations:** A new trigger action `reveal_entity` acts as a server-side broadcast, pushing the previously hidden entity to the appropriate clients' session logs.
3. **Save State Masking:** If a host saves the world, they may elect to write the full state. If a client saves the world, they only write the subset of the document they were permitted to observe.
