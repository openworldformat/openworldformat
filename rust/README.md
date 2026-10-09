# openworldformat (crates.io)

The Open World Format's Rust reference: the world document (types and
edits), the session fold (log, branches, state), and the `ext-physics`
and `ext-cinematography` extensions' executable halves — one serde-only
crate, no Bevy, no async, no sockets. The same code that implemented
the format in LocalGPT since its origin, extracted into the format's
own home.

```bash
cargo add openworldformat        # or, from this checkout:
cargo add --path rust openworldformat
```

```rust
use openworldformat::{WorldManifest, OpLogEntry, fold_log, fold_path};

// A head-first package: manifest.json is the world now…
let world: WorldManifest = serde_json::from_str(&manifest_text)?;
// …and the log folds over snapshots/base.json back to it.
let base: WorldManifest = serde_json::from_str(&base_text)?;
let entries: Vec<OpLogEntry> = log_text.lines()
    .filter(|l| !l.trim().is_empty())
    .map(serde_json::from_str).collect::<Result<_, _>>()?;

let doc = fold_log(&base.as_base()?, &entries)?;      // the world at head
let (doc, path) = fold_path(&base.as_base()?, &entries, Some("e3"))?; // or at any tip
let same = doc.to_manifest();                          // a whole manifest again
```

`openworldformat::state::fold_state` folds the game state over
`state.json` (save games are base + declaration + a player's log);
`openworldformat::physics` carries the extension's deterministic
reference solver, trajectory write/fold, and the conformance outcome
runner; `openworldformat::cinematography` carries the
`ext-cinematography` extension's crop math, view and projection, shot
list, and outcome runner; the `schema` feature generates
`world.schema.json`.

## The 0.3 surface (draft 0.3: head-first, live authoring)

- **The fold is total.** `WorldDoc` carries every manifest field —
  version, meta, soundtrack, creations, ambience, a `next_entity_id` that
  never goes down — and `to_manifest` writes them all back.
- **`EditOp::ModifyWorld { patch: Box<WorldPatch> }`** reaches meta,
  environment, camera, avatar, tours, soundtrack, ambience and creations
  (absent unchanged, `null` clears); its inverse restores them. A first
  `SetEnvironment`/`SetCamera` now inverts to a clearing `ModifyWorld`.
- **`authoring::ingest(&doc, &batch)`** — the Authoring profile: names to
  ids, ids for spawns, partial struct patches merged (`merge_patch`, RFC
  7396), strict reading, whole-batch refusal with a reason per op.
- **`manifest_text` / `manifest_text_of`** — the canonical text an
  authority writes, byte-identical to the JS `manifestText`.
- **`OpLogEntry.message`** (optional) — part of the entry's identity.
- **Head-first packages**: `PACKAGE_FORMAT_VERSION` is 2,
  `BASE_SNAPSHOT` is `snapshots/base.json`; `MergeRecord` reads the
  spec's `{"merge": {"branch": …}}` (it read `{"branch": …}` before).

## The 0.2 surface (draft 0.2 compliance)

The draft-0.2 tightening, as this crate reads it:

- `manifest.version` is **required** — a world that doesn't say which
  schema it speaks is refused, not defaulted.
- Entity ids cap at `MAX_ENTITY_ID` (2^53 − 1): `alloc_entity_id`
  returns `Result`, and the fold refuses ids past the ceiling, so
  worlds this crate generates never overflow a JSON-safe reader.
- The v2 multi-file manifest fields (`layout_file`, `region_files`,
  `behavior_files`, `audio_files`, `avatar_file`) are gone from the
  type, as they already were from the schema.
- LLM lineage lives under `meta["ext-provenance"]`
  (`ext_provenance::ExtProvenance`) — the `ext-provenance` extension,
  not core `WorldMeta` fields.
- **Names bind at ingestion**: `WorldDoc::apply_entry` (which
  `fold_log` uses) resolves `Orbit.center`/`LookAt.target` name
  references against the fold-so-far, and `WorldManifest::as_base`
  resolves a base manifest against itself — never at fold time.
- `oplog::canonical_json` + `oplog::compute_entry_id`: the canonical
  serializer (no whitespace, sorted keys) and the `sha256:` entry
  identity computed over it, identical across the five references for
  integer-valued JSON.
- `EditOp::compute_inverse(&doc)`: the undo rule in full — a delete's
  inverse is a batch of spawns holding a deep copy of the deleted tree.
- `session::merge_branch`: fork merging that reallocates ids allocated
  concurrently on the main line and rewrites every reference to them
  before appending.
- `package::snapshot_filename` (`snapshots/entry-<id>.json`, falling
  back to `rev-<N>.json`) and `package::compact_plan` (base_revision →
  head; the host performs the file moves).
- `strict::{parse_manifest_strict, decode_line_strict}`: must-ignore's
  off switch for authoring tools — unknown fields, legacy lineage keys
  and unregistered extensions are refusals, with the extension
  registry embedded as `REGISTERED_EXTENSIONS`.

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
cargo test                      # unit + conformance + examples + extension outcomes
cargo test --features schema    # + the schema snapshot (both generators agree)
```

CI runs both, alongside the `js` and `python` jobs — three references,
one set of worlds and outcome assertions.

Apache-2.0.
