# Contributing

## The process

Changes to the specification are RFCs, in the spirit the format's
lineage already uses:

1. Open a pull request with the change and a `spec/rfcs/` note stating
   the problem, the alternatives and the cost.
2. Changes that alter what a conforming renderer draws require new or
   changed conformance cases in the same PR — no spec change without a
   test that pins it.
3. Changes to the data model land in the JSON Schema in the same PR,
   and bump the schema version per the [versioning policy](spec/versioning.md).
4. Two implementers' sign-off (currently: the reference fold
   implementation in `js/`, and a renderer) before merge.

Schema-first: `schema/world.schema.json` is generated from the
reference types, not hand-edited. Conformance-first: behavior is what
the worlds in `conformance/` render to. The prose explains; the files
decide.

## Repository conventions

- Conventional commits (`feat:`, `fix:`, `docs:`, `spec:`), no
  Co-Authored-By trailers.
- `npm test` in `js/` and `zola build` in `website/` must pass; CI
  runs both.
- Spec prose is English, second person, and states rules with
  MUST / SHOULD / MAY.

## Governance

The format is stewarded in this repository, independently of any
producer. The reference producers (the LocalGPT apps) hold no special
merge authority beyond being implementers. A formal governance body and
a spec-text license split (CC-BY 4.0 for prose, Apache-2.0 for code)
are planned before 1.0; until then, the maintainer listed in the repo
settings is the tiebreaker of last resort.

## What we won't do

- Add a feature with one implementer and no second user. The lineage's
  rule — add things when a second app needs them — is why the core
  stayed small.
- Promise bit-exact cross-engine replay. Semantic replay is the
  contract.
- Put core semantics in any must-ignore channel. If it matters, it has
  a schema field and a conformance world.
