# openworldformat (crates.io)

The Open World Format's Rust reference: the world document (types and
edits), the session fold (log, branches, state), and the `ext-physics`
extension's executable half — one serde-only crate, no Bevy, no async,
no sockets. The same code that implemented the format in LocalGPT since
its origin, extracted into the format's own home.

```bash
cargo add openworldformat        # or, from this checkout:
cargo add --path rust openworldformat
```

```rust
use openworldformat::{WorldManifest, OpLogEntry, fold_log, fold_path};

let manifest: WorldManifest = serde_json::from_str(&manifest_text)?;
let entries: Vec<OpLogEntry> = log_text.lines()
    .filter(|l| !l.trim().is_empty())
    .map(serde_json::from_str).collect::<Result<_, _>>()?;

let doc = fold_log(&manifest.as_base()?, &entries)?;   // the world at head
let (doc, path) = fold_path(&base, &entries, Some("e3"))?; // or at any tip
```

`openworldformat::state::fold_state` folds the game state over
`state.json` (save games are base + declaration + a player's log);
`openworldformat::physics` carries the extension's deterministic
reference solver, trajectory write/fold, and the conformance outcome
runner; the `schema` feature generates `world.schema.json`.

## Provenance and the plan

Extracted from LocalGPT's `world-types`, `world-sync` and
`world-physics` — not rewritten: in one language, two hand-written type
layers would only be drift risk. LocalGPT remains the app-side consumer
with its own copies until it's quiet enough to flip them to re-export
shims over this crate; until then, **both sides generate the same
`world.schema.json`**, and the snapshot test in each repository fails
when they disagree. The schema's `generation_tool` default is a pinned
literal for the same reason: a published schema must not churn with a
generating crate's version.

Two cross-implementation fixes landed with the extraction, applied on
both sides: entries may omit `author` (the spec's own example logs do;
the JS fold always read them), and the folded schema is
version-independent.

## Test

```bash
cargo test                      # unit + conformance + examples + physics outcomes
cargo test --features schema    # + the schema snapshot (both generators agree)
```

CI runs both, alongside the `js` and `python` jobs — three references,
one set of worlds and outcome assertions.

Apache-2.0.
