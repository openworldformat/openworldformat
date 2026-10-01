"""Agents: the benchmark's other half — something that emits ops.

Scoring a recorded trajectory is one loop; scoring a live agent is the
same loop with the recording still wet. :func:`run_agent` keeps the
entries, folds between steps so the agent observes its own edits, and
scores with :func:`openworldformat.eval.run_task` — no engine, no
renderer, an agent and a fold.

:func:`template_agent` is the baseline: it compiles a task's goal
predicates into ops — spawn what must exist, move what must be near,
delete what must be gone, set what fields must hold — and stops when
the goal reads satisfied. It is a compiler, not a brain: it exists so
benchmark authors have a floor to beat and a skeleton to copy, and so
the agent interface has a reference user. Real agents read the world,
not the answer key; the interesting ones will too.
"""

from __future__ import annotations

from . import fold_log, fold_state
from .eval import run_task

__all__ = ["run_agent", "template_agent"]

#: What run_agent names the baseline in the log's author field.
TEMPLATE_NAME = "template"


def _find(document: dict, name):
    return next((e for e in document["entities"] if e.get("name") == name), None)


def _is_num(v) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def template_agent(document: dict, values: dict, task: dict, entries: list) -> dict | None:
    """Compile the goal into one entry of ops — the baseline solver.

    Observes the folded world (``document``) and the state values, and
    emits what's missing: one entry per call, or None when the goal
    already reads satisfied (or holds nothing this template knows how
    to do — unknown predicates are beyond it, by design).
    """
    # Per-entity requirements, merged so `exists` + `near` on one entity
    # spawn it once, at the target. `gone` wins over both.
    targets: dict = {}
    deletions: set = set()
    field_ops: dict = {}
    for predicate in task.get("goal") or []:
        if "exists" in predicate:
            targets.setdefault(predicate["exists"].get("entity"), None)
        elif "near" in predicate:
            near = predicate["near"]
            targets[near.get("entity")] = list(near.get("position") or [0.0, 0.0, 0.0])
        elif "gone" in predicate:
            name = predicate["gone"].get("entity")
            deletions.add(name)
            targets.pop(name, None)
        elif "field" in predicate:
            field = predicate["field"]
            name, v = field.get("name"), values.get(field.get("name"))
            want = None
            if "equals" in field and v != field["equals"]:
                want = field["equals"]
            if "at_least" in field and _is_num(v) and v < field["at_least"]:
                want = field["at_least"]
            if "at_most" in field and _is_num(v) and v > field["at_most"]:
                want = field["at_most"]
            if want is not None:
                field_ops[name] = want

    ops: list = []
    next_id = max((e.get("id", 0) for e in document["entities"]), default=0) + 1
    for name, position in targets.items():
        entity = _find(document, name)
        if entity is None:
            entity_doc = {"id": next_id, "name": name}
            if position is not None:
                entity_doc["transform"] = {"position": position}
            ops.append({"SpawnEntity": {"entity": entity_doc}})
            next_id += 1
        elif position is not None:
            transform = entity.get("transform")
            current = transform.get("position") if isinstance(transform, dict) else None
            if current != position:
                # Keep rotation/scale if the entity had them; move the position.
                moved = dict(transform) if isinstance(transform, dict) else {}
                moved["position"] = position
                ops.append({"ModifyEntity": {"id": entity["id"], "patch": {"transform": moved}}})
    for name in sorted(deletions):
        entity = _find(document, name)
        if entity is not None:
            ops.append({"DeleteEntity": {"id": entity["id"]}})
    for name, want in field_ops.items():
        ops.append({"state": {name: want}})

    if not ops:
        return None
    revision = max((e.get("revision", 0) for e in entries), default=0) + 1
    timestamp = max((e.get("timestamp_ms", 0) for e in entries), default=0) + 1
    return {
        "revision": revision,
        "timestamp_ms": timestamp,
        "author": {"name": TEMPLATE_NAME},
        "ops": ops,
    }


def run_agent(manifest: dict, task: dict, state_doc: dict | None = None,
              agent=None, max_steps: int = 8) -> dict:
    """Run an agent against a task: observe, emit, fold, score, repeat.

    :param manifest: the base world document (parsed)
    :param task: as :func:`openworldformat.eval.run_task` takes it
    :param state_doc: the world's parsed ``state.json``, if it has one
    :param agent: ``f(document, values, task, entries) -> entry | None``
        — the folded world and state values as observation, one entry of
        ops as action, None to stop. Defaults to :func:`template_agent`
    :param max_steps: the step budget — an agent that never stops, stops
    :return: the :func:`run_task` result, plus ``entries`` (the agent's
        ops — serialize to ``ops.jsonl`` and they're a recording)
    """
    if agent is None:
        agent = template_agent
    entries: list = []
    result = None
    for _ in range(max_steps):
        document = fold_log(manifest, entries)
        values = fold_state(state_doc or {}, entries)["values"]
        entry = agent(document, values, task, entries)
        if entry is None:
            break
        entries.append(entry)
        result = run_task(manifest, entries, task, state_doc)
        if result["ok"]:
            break
    if result is None:
        result = run_task(manifest, entries, task, state_doc)
    result = dict(result)
    result["entries"] = entries
    return result
