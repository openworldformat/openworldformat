# openworldformat (PyPI)

The Open World Format research package. The core — parse a `.world`
manifest, fold its session log — is pure standard library, no
dependencies, and deliberately **no renderer**: this is the format's
research surface, not a viewer. Embodied-agent benchmarks (tasks as
predicates over the state document, trajectories in the format's own
op vocabulary), dataset tooling over session logs (fold a thousand
recordings, compute one number), and notebooks — none of that wants
three.js or Bevy; all of it wants `pip install openworldformat` and
a fold.

It is also the fold contract's third independently-built
implementation, sharing no code or toolchain with the JS reference or
the Rust one — every conformance world and outcome assertion the `js`
CI job runs, this package runs too, and the two folds agree
byte-for-byte on the example packages.

## Install

```bash
pip install openworldformat
```

## Use

```python
from pathlib import Path
from openworldformat import parse_manifest, parse_log_line, fold_log

manifest = parse_manifest(Path("world/manifest.json").read_text())
entries = [parse_log_line(line)
           for line in Path("world/ops.jsonl").read_text().splitlines()
           if line.strip()]

state = fold_log(manifest, entries)
state["entities"]       # the world at head revision
state["applied_edits"]  # how many edits the log held
```

The fold applies the spec's rules: only `edit` ops change the document;
`tool`, `input`, `state` and `clock` fold to nothing; a `Batch` applies
all-or-nothing; ids and names stay unique; deleting an entity deletes
its descendants; it stops at the first entry that no longer applies.

Branches are tips: `build_history` gives a log its shape, `fold_path`
folds any branch of it. Session `state` ops fold separately, over
`state.json` (`fold_state`). `openworldformat.physics` carries the
`ext-physics` extension's executable half — the deterministic reference
solver, trajectory write/fold, and the outcome runner:

```python
from openworldformat.physics import simulate_physics, fold_trajectories, run_outcomes

sim = simulate_physics(manifest, {"until_s": 6})  # deterministic
sim["contacts"]   # impacts: {t_s, body, other, position, normal_speed}
sim["resting"]    # where each dynamic body ended up
track = fold_trajectories(entries)  # scrub recorded motion, no solver
run_outcomes(manifest, outcomes_doc)  # the conformance assertions
```

The solver is deliberately minimal — spheres against floors and
axis-aligned statics, fixed 1/120 s semi-implicit Euler, sleep on rest —
so the conformance outcome assertions run anywhere and in CI.
Reference-grade, not production physics; deterministic within an
engine, semantic-only across engines, exactly as the core spec refuses
bit-exactness.

## API

| Export | What it does |
|---|---|
| `parse_manifest(text)` | parse and version-check a world document |
| `parse_log_line(line)` | parse one `ops.jsonl` line, ops classified |
| `classify_op(op)` | recognize an op by shape — edits first |
| `edit_ops(entry)` | an entry's edits, in order |
| `fold_log(manifest, entries)` | the document at the last entry (the linear fold) |
| `build_history(entries)` | a log's ids, parents, children and tips |
| `fold_path(manifest, entries, tip=None)` | the document at any tip (a branch) |
| `fold_state(state_doc, entries)` | the state document's values at the last entry |

`openworldformat.physics` mirrors the npm package's
`openworldformat/physics`: `collect_physics`, `simulate_physics`,
`fold_trajectories`, `trajectory_op`, `run_outcomes`.
`openworldformat.soundtrack` holds the soundtrack curves — `curve_at`,
`beat_at`, `section_at`, `modulation_factor` — as plain functions over
plain numbers, for analysis rather than playback.

## Test

```bash
python -m unittest discover -s tests -t .
```

The suite mirrors the JS tests — it folds the repository's own example
packages, replays the drop-test recording through the solver, and runs
the conformance outcome assertions — so both references assert the same
facts.

Apache-2.0.
