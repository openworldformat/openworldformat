# Examples

Each directory is a complete `.world` package (or will be — see each one's
`package.json` profiles).

| Package | What it shows |
|---|---|
| `hello-world/` | every shape on a ground plane, plus an `ops.jsonl` exercising all five op kinds: an edit, the tool call that caused it, a state delta, a visitor input sample, and a clock event |
| `forked-exploration/` | a branching history: the same castle up to the wall, then a garden trunk and a moat fork — two tips, named refs, and a merge record (`foldPath` folds either) |

Zip `hello-world/` and you have the transport form; a viewer reads
`manifest.json` alone (Viewer profile), a session implementation folds
`ops.jsonl` over it.
