# The package (L2/L3)

A `.world` is a directory. A zip of that directory is the transport form
for finished, published worlds; the directory is canonical because the
log appends.

```
thing.world/
  manifest.json            L0  the base world document
  state.json               L0  the typed state document (open item)
  ops.jsonl                L1  the session log — one JSON entry per line
  snapshots/rev-<N>.json   L1  derived keyframes, never authoritative
  assets/<sha256>…         L2  content leaves: glTF meshes, textures, audio
  package.json             L3  metadata: versions, profiles, integrity
```

## `package.json`

```json
{
  "format_version": 1,
  "name": "hello-world",
  "profiles": ["viewer", "player", "session"],
  "base_revision": 0,
  "head_revision": 3,
  "seed": null,
  "world_sha256": "…",
  "log_sha256": "…",
  "updated_ms": 1790000000123
}
```

| Field | Meaning |
|---|---|
| `format_version` | The package format's own version (this document). |
| `profiles` | Which profiles the package uses; readers ignore unknown ones. |
| `base_revision` | The revision `manifest.json` (or the newest snapshot) holds. |
| `head_revision` | The newest revision the log reaches. |
| `seed` | Reserved for deterministic replay. |
| `world_sha256` / `log_sha256` | Integrity: SHA-256 of `manifest.json` and of `ops.jsonl` as of head. |

## Assets are content-addressed

Assets live under `assets/`, named by (or keyed on) their SHA-256. A
package is therefore self-contained and verifiable: two machines holding
the same package render the same world. Asset references inside
manifests also carry the hash, so a missing or changed file is detected,
not silently drawn.

Meshes are glTF (`.glb`); textures are PNG; audio is whatever the audio
profile accepts. The package never invents leaf formats — it composes
them.

## Integrity and tolerance

Writers SHOULD refresh `log_sha256` and `head_revision` whenever the log
appends, and MAY snapshot periodically (keyframes for seeking: read at
revision N = nearest base-or-snapshot ≤ N, then fold forward).

Readers MUST be tolerant: a torn last log line loses at most itself and
is skipped (and countable); the fold stops at the first entry that no
longer applies. Integrity hashes, when present, SHOULD be checked before
trusting a package from a third party.
