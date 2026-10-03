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

## Tasks: the benchmark harness

A task is a world, a trajectory, and a goal — a list of predicates over
what the fold produces. Scoring is therefore deterministic and needs no
engine: fold the log, evaluate the predicates. `examples/tasks/` turns
the repository's four example packages into a benchmark dataset, each
package's own log its solution:

```bash
python -m openworldformat.eval examples/tasks/
```

```
ok   the-drop-test: the switch scores  (1 edits, 6 ops, 1 authors, revision 1)
ok   forked-exploration: the moat variant wins this branch  (3 edits, 4 ops, 3 authors, revision 4)
ok   hello-world: hang the lantern  (1 edits, 5 ops, 3 authors, revision 1)
ok   speedrun-fork: noor's run, on its own branch  (2 edits, 9 ops, 2 authors, revision 2)
```

A task file names the world and states the goal; a `tip` folds a branch
(a run, a variant) instead of the head; a budget caps the cost:

```json
{
  "task": "hello-world: hang the lantern",
  "world": "../../../examples/hello-world",
  "budget": { "max_edits": 1, "max_entries": 5 },
  "goal": [
    { "exists": { "entity": "lantern" } },
    { "near": { "entity": "lantern", "position": [-12.0, 0.0, 3.0], "tolerance": 0.5 } },
    { "field": { "name": "score.tour", "equals": 1 } }
  ]
}
```

| Predicate | Holds when |
|---|---|
| `{"exists": {"entity": name}}` | the fold holds an entity by that name |
| `{"gone": {"entity": name}}` | it doesn't |
| `{"near": {"entity", "position", "tolerance"=0.15}}` | the entity's folded position is within tolerance |
| `{"field": {"name", "equals"?, "at_least"?, "at_most"?}}` | the state document's value compares |

Budgets (`max_edits`, `max_ops`, `max_entries`, inclusive) are the cost
side of a task: solve it, but not by a million edits. Metrics report
the trajectory's shape — edits, ops, entries, authors (the audit
trail's model-vs-visitor filter is one field), span, revision.

To score an agent, don't write files at all — pass its ops to
`run_task` the moment it emits them:

```python
from openworldformat.eval import run_task

result = run_task(manifest, agent_entries, task, state_doc)
result["ok"], result["failures"], result["metrics"]
```

This task language is deliberately library-level, not a spec
extension; it graduates to `spec/extensions/` when a second implementer
wants it.

## Agents: the benchmark's other half

Scoring a recorded trajectory is one loop; scoring a live agent is the
same loop with the recording still wet. `run_agent` keeps the entries,
folds between steps so the agent observes its own edits, and scores
with `run_task`:

```python
from openworldformat.agent import run_agent

result = run_agent(manifest, task, state_doc)  # template_agent inside
result["ok"], result["metrics"], result["entries"]
```

An agent is `f(document, values, task, entries) -> entry | None`: the
folded world and state values as observation, one entry of ops as
action, `None` to stop. The built-in `template_agent` is the baseline —
a compiler from goal predicates to ops, not a brain; it exists so
benchmark authors have a floor to beat and a skeleton to copy. Its
entries serialize to `ops.jsonl` like any recording: an agent run is a
log, ready to join a dataset.

## Dataset tooling: fold a corpus, compute one number

A folder of `.world` packages is a dataset — each `ops.jsonl` a
recording, each fold a row:

```bash
python -m openworldformat.dataset examples/
```

```
world               entities  edits  ops  entries  authors               tips  span_s
forked-exploration  5         4      5    5        host, llm, maya       2     0.4
hello-world         14        1      5    5        llm, maya, visitor-7  1     1.877
speedrun-fork       9         2      15   4        kai, maya, noor       2     20.0
the-drop-test       7         1      6    2        host                  1     0.2
```

The authors column is the audit trail's dimension — model, host and
visitor in one recording, separable by one field. The tips column
counts branches; fold each with `fold_path` and the counterfactuals
come with the corpus. `summarize(dir)` gives the row as a dict for
notebooks.

## API

| Export | What it does |
|---|---|
| `parse_manifest(text, strict=False)` | parse and version-check a world document (strict also enforces the schema's key sets) |
| `parse_log_line(line, strict=False)` | parse one `ops.jsonl` line, ops classified |
| `classify_op(op)` | recognize an op by shape — edits first |
| `op_kind_shape_ok(kind)` | the shape collision rule: edits PascalCase, history lowercase |
| `edit_ops(entry)` | an entry's edits, in order |
| `canonical_json(value)` | the canonical form for hashing — sorted, whitespace-free |
| `compute_entry_id(entry)` | an entry's content id, `sha256:<hex>` |
| `fold_log(manifest, entries)` | the document at the last entry (the linear fold; by-name references bind to ids here, at ingestion) |
| `build_history(entries)` | a log's ids, parents, children and tips |
| `fold_path(manifest, entries, tip=None)` | the document at any tip (a branch) |
| `fold_state(state_doc, entries)` | the state document's values at the last entry |
| `compute_inverse(op, state)` | the inverse of one edit against the state it's about to change — undo is appending it |
| `merge_branch(state, entries)` | a branch's entries rewritten onto a main fold, colliding ids reallocated |
| `snapshot_filename(entry_id, revision)` | where a snapshot goes: by entry id, else by revision |
| `compact_package(package_json, head_revision)` | the package.json of a compaction |
| `compact(world_dir, head_revision=None)` | fold a package to its head and rewrite it as the new base |
| `ext_provenance(manifest)` | the provenance extension's five lineage fields, or None |

`openworldformat.physics` mirrors the npm package's
`openworldformat/physics`: `collect_physics`, `simulate_physics`,
`fold_trajectories`, `trajectory_op`, `run_outcomes`.
`openworldformat.soundtrack` holds the soundtrack curves — `curve_at`,
`beat_at`, `section_at`, `modulation_factor` — as plain functions over
plain numbers, for analysis rather than playback.
`openworldformat.eval` scores tasks (see above): `run_task`,
`load_task_file`, and the `python -m openworldformat.eval` CLI.
`openworldformat.agent` runs agents against them: `run_agent` and the
`template_agent` baseline. `openworldformat.dataset` summarizes
recordings — `summarize` and the `python -m openworldformat.dataset`
report.

## Test

```bash
python -m unittest discover -s tests -t .
```

The suite mirrors the JS tests — it folds the repository's own example
packages, replays the drop-test recording through the solver, and runs
the conformance outcome assertions — so both references assert the same
facts.

Apache-2.0.
