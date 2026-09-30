# Examples

Each directory is a complete `.world` package (or will be — see each one's
`package.json` profiles).

| Package | What it shows |
|---|---|
| `hello-world/` | every shape on a ground plane, plus an `ops.jsonl` exercising all five op kinds: an edit, the tool call that caused it, a state delta, a visitor input sample, and a clock event |
| `forked-exploration/` | a branching history: the same castle up to the wall, then a garden trunk and a moat fork — two tips, named refs, and a merge record (`foldPath` folds either) |
| `the-drop-test/` | the first package using the `ext-physics` extension: declared bodies, a collision trigger scoring through a state op, and the recorded run — a trajectory op any device can scrub without a solver, plus the outcome the contact produced |
| `speedrun-fork/` | the first package whose log writes branches: one course, two recorded runs (sampled input plus a timer state op each) — a challenge chain where each run is a tip, named by a ref, and `foldPath` folds the course plus either run |

Zip `hello-world/` and you have the transport form; a viewer reads
`manifest.json` alone (Viewer profile), a session implementation folds
`ops.jsonl` over it.
