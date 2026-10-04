"""Tasks: goals as predicates over the fold — the benchmark harness.

A task is a world, a trajectory, and a goal: a list of predicates over
what the fold produces — the document (entities, positions) and the
state document's values. Scoring is therefore deterministic and needs
no engine: fold the log, evaluate the predicates. That is the whole
difference from the embodied-agent benchmarks that require a
simulator, and it is why the predicates live here and not there.

Deliberately library-level, not a spec extension: this module is the
format's research surface growing a task language, and it graduates to
`spec/extensions/` only when a second implementer wants it — the
repository's own rule, applied to itself.

Run it::

    python -m openworldformat.eval examples/tasks/

or from code, to score an agent's ops as they happen::

    result = run_task(manifest, entries, task, state_doc)
    result["ok"]        # every predicate held, every budget respected
    result["failures"]  # why not
    result["metrics"]   # edits, ops, entries, authors, span_ms, revision
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

from . import (
    WorldFormatError,
    build_history,
    fold_log,
    fold_path,
    fold_state,
    parse_log_line,
    parse_manifest,
    read_package,
)

__all__ = ["run_task", "load_task_file", "entry_metrics", "main"]

#: Default tolerance for the `near` predicate — the outcomes precedent.
NEAR_TOLERANCE = 0.15


def _entity_by_name(state: dict, name):
    return next((e for e in state["entities"] if e.get("name") == name), None)


def _is_num(v) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _check_predicate(predicate: dict, state: dict, values: dict) -> list:
    """One goal predicate against one fold. Returns its failures (empty =
    held)."""
    if "exists" in predicate:
        name = predicate["exists"].get("entity")
        if _entity_by_name(state, name) is None:
            return [f"entity '{name}' does not exist"]
        return []
    if "gone" in predicate:
        name = predicate["gone"].get("entity")
        if _entity_by_name(state, name) is not None:
            return [f"entity '{name}' still exists"]
        return []
    if "near" in predicate:
        near = predicate["near"]
        name = near.get("entity")
        entity = _entity_by_name(state, name)
        if entity is None:
            return [f"entity '{name}' does not exist"]
        transform = entity.get("transform")
        position = transform.get("position") if isinstance(transform, dict) else None
        if not isinstance(position, list) or len(position) != 3:
            return [f"entity '{name}' has no position"]
        tolerance = near.get("tolerance")
        tolerance = NEAR_TOLERANCE if tolerance is None else tolerance
        want = near.get("position")
        distance = math.dist(position, want)
        if distance > tolerance:
            return [f"entity '{name}' at {position} is {distance:.3f} from {want} (tolerance {tolerance})"]
        return []
    if "field" in predicate:
        field = predicate["field"]
        name = field.get("name")
        if name not in values:
            return [f"field '{name}' has no value"]
        v = values[name]
        failures = []
        if "equals" in field and v != field["equals"]:
            failures.append(f"field '{name}' is {v!r}, expected {field['equals']!r}")
        for comparator, ok in (("at_least", lambda a, b: a >= b), ("at_most", lambda a, b: a <= b)):
            if comparator in field:
                bound = field[comparator]
                if not _is_num(v) or not _is_num(bound):
                    failures.append(f"field '{name}' is {v!r}, not comparable to {bound!r}")
                elif not ok(v, bound):
                    failures.append(f"field '{name}' is {v!r}, expected {comparator.replace('_', ' ')} {bound!r}")
        return failures
    return [f"unknown predicate {json.dumps(predicate)}"]


def _check_budget(budget: dict, metrics: dict) -> list:
    """Budgets are the cost side of a task: solve it, but not by a million
    edits. Each limit is inclusive."""
    failures = []
    for key, metric in (("max_edits", "edits"), ("max_ops", "ops"), ("max_entries", "entries")):
        if key in budget and metrics[metric] > budget[key]:
            failures.append(f"budget exceeded: {metrics[metric]} {metric}, max {budget[key]}")
    return failures


def entry_metrics(chain: list, applied_edits: int) -> dict:
    """The trajectory's shape: entries, ops, edits, authors, span, revision —
    what task scoring and dataset tooling both want per recording. The
    authors field is the audit trail's dimension: filter it on one name
    and you have what that one author did, model or visitor."""
    return {
        "entries": len(chain),
        "ops": sum(len(e.get("ops") or []) for e in chain),
        "edits": applied_edits,
        "revision": chain[-1].get("revision") if chain else None,
        "authors": sorted({(e.get("author") or {}).get("name") for e in chain} - {None}),
        "span_ms": (
            chain[-1].get("timestamp_ms", 0) - chain[0].get("timestamp_ms", 0)
            if len(chain) > 1 else 0
        ),
    }


def run_task(manifest: dict, entries: list, task: dict, state_doc: dict | None = None) -> dict:
    """Score one trajectory against one task.

    :param manifest: the base world document (parsed)
    :param entries: the trajectory — parsed log entries, in order (the
        agent's ops, or a recorded session's)
    :param task: ``{tip?, budget?, goal: [...]}`` — predicates per the
        module docstring; ``tip`` folds a branch instead of the head
    :param state_doc: the world's parsed ``state.json``, if it has one
    :return: ``{"ok", "failures", "metrics", "state", "values"}``
    :raises WorldFormatError: when the task has no goal
    """
    tip = task.get("tip")
    if tip is not None:
        state = fold_path(manifest, entries, tip)
        by_id = build_history(entries)["by_id"]
        chain = [by_id[i] for i in state["path"]]
    else:
        state = fold_log(manifest, entries)
        chain = list(entries)
    values = fold_state(state_doc or {}, chain)["values"]
    metrics = entry_metrics(chain, state["applied_edits"])

    goal = task.get("goal")
    if not isinstance(goal, list) or not goal:
        raise WorldFormatError("task needs a goal: a non-empty list of predicates")
    failures = []
    for predicate in goal:
        failures += _check_predicate(predicate, state, values)
    failures += _check_budget(task.get("budget") or {}, metrics)
    return {"ok": not failures, "failures": failures, "metrics": metrics, "state": state, "values": values}


def load_task_file(path) -> tuple:
    """Load a task file: the task document, plus the world it names.

    Paths resolve relative to the task file. ``world`` is a `.world`
    package directory (``manifest.json``, ``ops.jsonl`` unless an agent
    is expected to supply the log, ``state.json`` if the world declares
    state).

    :return: ``(manifest, entries, task, state_doc)``
    """
    path = Path(path)
    task = json.loads(path.read_text())
    world = Path(task["world"])
    if not world.is_absolute():
        world = (path.parent / world).resolve()
    manifest, entries, _head, _package = read_package(world)
    state_file = world / (task.get("state") or "state.json")
    state_doc = json.loads(state_file.read_text()) if state_file.exists() else None
    return manifest, entries, task, state_doc


def main(argv: list | None = None) -> int:
    """The CLI: score task files (or directories of them). Exit 1 when any
    task fails — the benchmark's teeth."""
    argv = sys.argv[1:] if argv is None else argv
    if not argv:
        print("usage: python -m openworldformat.eval TASK.json [TASK.json | DIR ...]", file=sys.stderr)
        return 2
    paths = []
    for arg in argv:
        p = Path(arg)
        paths += sorted(p.glob("*.json")) if p.is_dir() else [p]

    failed = 0
    for p in paths:
        manifest, entries, task, state_doc = load_task_file(p)
        result = run_task(manifest, entries, task, state_doc)
        name = task.get("task", p.stem)
        m = result["metrics"]
        if result["ok"]:
            print(f"ok   {name}  ({m['edits']} edits, {m['ops']} ops, "
                  f"{len(m['authors'])} authors, revision {m['revision']})")
        else:
            failed += 1
            print(f"FAIL {name}")
            for failure in result["failures"]:
                print(f"     - {failure}")
    total = len(paths)
    if failed:
        print(f"\n{failed} of {total} tasks failed")
        return 1
    print(f"\n{total} tasks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
