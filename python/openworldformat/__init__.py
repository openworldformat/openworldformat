"""The Open World Format reference fold, in Python.

Pure standard library — no dependencies, no engine. It parses a world
document (manifest.json) and folds a session log (ops.jsonl) over it,
applying the same rules the specification states: only edits change the
document, history kinds fold to nothing, ops are recognized by shape
(edits first), and a batch applies all-or-nothing.

This is the format's research surface: benchmarks, dataset tooling and
notebooks want ``pip install openworldformat`` and a fold, not a
renderer. It shares no code or toolchain with the JS reference, which
makes it a cross-check on the fold contract as much as a consumer of
it.

Spec: https://openworldformat.org  ·  schema version 3
"""

from __future__ import annotations

import copy
import json
import re
from typing import Any

__all__ = [
    "SUPPORTED_SCHEMA_VERSION",
    "SUPPORTED_FORMAT_VERSION",
    "WorldFormatError",
    "parse_manifest",
    "classify_op",
    "parse_log_line",
    "edit_ops",
    "fold_state",
    "build_history",
    "fold_path",
    "fold_log",
]


class WorldFormatError(ValueError):
    """A document or log that violates the format's rules."""


#: The manifest schema version this fold reads.
SUPPORTED_SCHEMA_VERSION = 3

#: The package format version this fold reads.
SUPPORTED_FORMAT_VERSION = 1

EDIT_KEYS = frozenset({
    "SpawnEntity",
    "DeleteEntity",
    "ModifyEntity",
    "SetEnvironment",
    "SetCamera",
    "SetAmbience",
    "SpawnAudioEmitter",
    "RemoveAudioEmitter",
    "Batch",
})

_EXT_KEY = re.compile(r"ext-[a-z0-9-]+")


def _is_num(v: Any) -> bool:
    """JSON number, not a bool riding int's coat tails."""
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _coalesce(v: Any, default: Any) -> Any:
    """JavaScript's ``??``: null/undefined fall back, everything else passes."""
    return default if v is None else v


# ---------------------------------------------------------------------------
# Parsing
# ---------------------------------------------------------------------------


def parse_manifest(text: str) -> dict:
    """Parse and sanity-check a world document.

    :param text: the manifest's JSON text
    :return: the manifest
    :raises ValueError: when the text isn't JSON
    :raises WorldFormatError: when the schema version isn't supported
    """
    manifest = json.loads(text)
    if not isinstance(manifest, dict) or not _is_num(manifest.get("version")):
        raise WorldFormatError("manifest has no schema version — refusing to guess")
    if manifest["version"] > SUPPORTED_SCHEMA_VERSION:
        raise WorldFormatError(
            f"manifest schema version {manifest['version']} is newer than this "
            f"reader ({SUPPORTED_SCHEMA_VERSION}); a newer reader must read it "
            "— see the versioning policy"
        )
    if not isinstance(manifest.get("entities"), list):
        raise WorldFormatError("manifest has no entities array")
    return manifest


def classify_op(op: Any) -> dict:
    """Classify one op by its shape, edits first — the compatibility rule: a
    log written before the history kinds existed parses as edits, and an
    edit serializes today exactly as it always did. Extension ops
    (``ext-*``, single key, object value) are recognized as a kind of their
    own and, like every history kind, fold to nothing for the document.

    :return: ``{"kind": "edit" | "tool" | "input" | "state" | "clock" |
        "merge" | "extension" | "unknown", ...}`` — edits carry ``edit``
        and ``value``, extensions carry ``name`` and ``value``.
    """
    if not isinstance(op, dict):
        return {"kind": "unknown"}
    keys = list(op)
    if len(keys) == 1 and keys[0] in EDIT_KEYS:
        return {"kind": "edit", "edit": keys[0], "value": op[keys[0]]}
    if isinstance(op.get("tool"), str) and "args" in op:
        return {"kind": "tool", "value": op}
    inp = op.get("input")
    if isinstance(inp, dict) and isinstance(inp.get("actor"), str):
        return {"kind": "input", "value": inp}
    if isinstance(op.get("state"), dict):
        return {"kind": "state", "value": op["state"]}
    if isinstance(op.get("clock"), dict):
        return {"kind": "clock", "value": op["clock"]}
    if isinstance(op.get("merge"), dict):
        return {"kind": "merge", "value": op["merge"]}
    if len(keys) == 1 and isinstance(op[keys[0]], dict) and _EXT_KEY.fullmatch(keys[0]):
        return {"kind": "extension", "name": keys[0], "value": op[keys[0]]}
    return {"kind": "unknown"}


def parse_log_line(line: str) -> dict:
    """Parse one log line into an entry with classified ops. Unreadable lines
    are the writer's crash, not the reader's — the caller decides whether
    to skip (the spec says skip the last one, count the rest).

    :param line: one line of ops.jsonl
    :return: the entry, with ``classified`` added
    :raises ValueError: when the line isn't JSON
    :raises WorldFormatError: when the entry has no revision or ops array
    """
    entry = json.loads(line)
    if (
        not isinstance(entry, dict)
        or not _is_num(entry.get("revision"))
        or not isinstance(entry.get("ops"), list)
    ):
        raise WorldFormatError("log entry needs a revision and an ops array")
    entry["classified"] = [classify_op(op) for op in entry["ops"]]
    return entry


def edit_ops(entry: dict) -> list:
    """An entry's edits, in order — the ops that change the document."""
    classified = entry.get("classified")
    if classified is None:
        classified = [classify_op(op) for op in entry.get("ops") or []]
    return [c for c in classified if c["kind"] == "edit"]


# ---------------------------------------------------------------------------
# State folding
# ---------------------------------------------------------------------------


def fold_state(state_doc: dict | None, entries: list) -> dict:
    """Fold a session log's ``state`` ops over a state document: the values at
    the last entry. Separate from the document fold — state ops never
    touch entities — and equally tolerant: keys nothing declares are
    carried, not refused (spec/state.md).

    :param state_doc: parsed ``state.json`` (``{format_version, fields}``)
    :param entries: parsed log entries, in order
    :return: ``{"values": dict, "undeclared": list of str}``
    """
    fields = state_doc.get("fields") if isinstance(state_doc, dict) else None
    if not isinstance(fields, dict):
        fields = {}
    values = {}
    for key, field in fields.items():
        values[key] = copy.deepcopy(_coalesce(field.get("initial"), None))
    undeclared: dict = {}  # insertion-ordered set

    for entry in entries:
        classified = entry.get("classified")
        if classified is None:
            classified = [classify_op(op) for op in entry.get("ops") or []]
        for c in classified:
            if c["kind"] != "state":
                continue
            for key, value in c["value"].items():
                if key in fields:
                    # A declared field: set it, or reset it to its initial value.
                    if value is None:
                        values[key] = copy.deepcopy(_coalesce(fields[key].get("initial"), None))
                    else:
                        values[key] = copy.deepcopy(value)
                    continue
                # Maybe a subkey of a declared map field: "inventory.rope" under
                # the declared map "inventory".
                dot = key.find(".")
                if dot > 0:
                    base, inner = key[:dot], key[dot + 1:]
                    if base in fields and fields[base].get("type") == "map":
                        m = values.get(base)
                        if not isinstance(m, dict):
                            m = {}
                        if value is None:
                            m.pop(inner, None)
                        else:
                            m[inner] = copy.deepcopy(value)
                        values[base] = m
                        continue
                # Declared by no one: carry it, and say so.
                if value is None:
                    values.pop(key, None)
                else:
                    values[key] = copy.deepcopy(value)
                undeclared[key] = None
    return {"values": values, "undeclared": list(undeclared)}


# ---------------------------------------------------------------------------
# Branching histories
# ---------------------------------------------------------------------------


def _with_identity(entries: list) -> tuple:
    """Give every entry an id and a parent, per spec/session.md: an entry's
    own ``id`` if present, else a synthesized ``line-<n>``; its ``parent``
    if present, else the previous entry (None for the first). A log with
    no ids is therefore a chain in file order.
    """
    by_id: dict = {}
    ordered: list = []
    previous = None
    for n, raw in enumerate(entries):
        id_ = raw["id"] if isinstance(raw.get("id"), str) else f"line-{n}"
        if id_ in by_id:
            raise WorldFormatError(f"duplicate entry id '{id_}'")
        parent = raw["parent"] if isinstance(raw.get("parent"), str) else previous
        if parent is not None and parent not in by_id:
            raise WorldFormatError(
                f"entry '{id_}' names parent '{parent}', which isn't in the log"
            )
        entry = dict(raw)
        entry["id"] = id_
        entry["parent"] = parent
        by_id[id_] = entry
        ordered.append(entry)
        previous = id_
    return ordered, by_id


def build_history(entries: list) -> dict:
    """The history of a log: entries with identity, and its shape.

    :param entries: parsed log entries, in file order
    :return: ``{"ordered": list, "by_id": dict, "children": dict of
        id -> [child ids], "tips": [ids with no children]}``
    """
    ordered, by_id = _with_identity(entries)
    children = {e["id"]: [] for e in ordered}
    for entry in ordered:
        if entry["parent"] is not None and entry["parent"] in children:
            children[entry["parent"]].append(entry["id"])
    tips = [e["id"] for e in ordered if not children[e["id"]]]
    return {"ordered": ordered, "by_id": by_id, "children": children, "tips": tips}


def fold_path(manifest: dict, entries: list, tip: str | None = None) -> dict:
    """Fold one path of the history: the document at ``tip`` (default: the
    last entry in file order), reached by walking parent links to the base
    and folding that chain. A branch is just a different tip.

    :param manifest: the base world document
    :param entries: parsed log entries, in file order
    :param tip: an entry id from :func:`build_history`
    :return: the fold state (as :func:`fold_log` returns), plus ``path`` ids
    :raises WorldFormatError: on an unknown tip, or the first entry that
        no longer applies
    """
    ordered, by_id = _with_identity(entries)
    last = ordered[-1]["id"] if ordered else None
    target = tip if tip is not None else last
    if target is None or target not in by_id:
        raise WorldFormatError(f"no entry '{target}' in this log")
    chain = []
    id_ = target
    while id_ is not None:
        chain.append(by_id[id_])
        id_ = by_id[id_]["parent"]
    chain.reverse()
    state = fold_log(manifest, chain)
    state["path"] = [e["id"] for e in chain]
    return state


# ---------------------------------------------------------------------------
# Folding
# ---------------------------------------------------------------------------


def _invalid(message: str) -> WorldFormatError:
    return WorldFormatError(f"invalid: {message}")


def _apply_edit(state: dict, edit: str, value: dict) -> None:
    """Apply one edit op to a fold state, all-or-nothing. Throws on refusal."""
    if edit == "SpawnEntity":
        entity = value.get("entity")
        if (
            not isinstance(entity, dict)
            or not _is_num(entity.get("id"))
            or not isinstance(entity.get("name"), str)
        ):
            raise _invalid("SpawnEntity needs an entity with id and name")
        if entity["id"] in state["by_id"]:
            raise _invalid(f"entity {entity['id']} already exists")
        if entity["name"] in state["names"]:
            raise _invalid(f"an entity named '{entity['name']}' already exists")
        parent = entity.get("parent")
        if parent is not None and parent not in state["by_id"]:
            raise _invalid(f"entity {entity['id']}'s parent {parent} isn't in the document")
        state["entities"].append(entity)
        state["by_id"][entity["id"]] = entity
        state["names"].add(entity["name"])
    elif edit == "DeleteEntity":
        id_ = value.get("id")
        entity = state["by_id"].get(id_)
        if entity is None:
            raise _invalid(f"no entity {id_}")
        # Descendants go with it: collect the subtree, then remove.
        doomed = {id_}
        grew = True
        while grew:
            grew = False
            for e in state["entities"]:
                p = e.get("parent")
                if p is not None and p in doomed and e["id"] not in doomed:
                    doomed.add(e["id"])
                    grew = True
        state["entities"] = [e for e in state["entities"] if e["id"] not in doomed]
        for d in doomed:
            e = state["by_id"].pop(d, None)
            if e is not None:
                state["names"].discard(e["name"])
    elif edit == "ModifyEntity":
        entity = state["by_id"].get(value.get("id"))
        if entity is None:
            raise _invalid(f"no entity {value.get('id')}")
        patch = _coalesce(value.get("patch"), {})
        # Absent = unchanged; null = clear; value = set (Option<Option<T>>).
        if "name" in patch:
            if patch["name"] is None:
                raise _invalid("an entity can't have no name")
            if patch["name"] != entity["name"]:
                if patch["name"] in state["names"]:
                    raise _invalid(f"an entity named '{patch['name']}' already exists")
                state["names"].discard(entity["name"])
                entity["name"] = patch["name"]
                state["names"].add(patch["name"])
        if "parent" in patch:
            if patch["parent"] is not None and patch["parent"] not in state["by_id"]:
                raise _invalid(f"parent {patch['parent']} isn't in the document")
            # A parent cycle would make the entity its own ancestor.
            ancestor = patch["parent"]
            seen = {entity["id"]}
            while ancestor is not None:
                if ancestor in seen:
                    raise _invalid(f"entity {entity['id']} can't be its own ancestor")
                seen.add(ancestor)
                a = state["by_id"].get(ancestor)
                ancestor = a.get("parent") if a is not None else None
            entity["parent"] = patch["parent"]
        for field in (
            "transform", "shape", "material", "light", "audio", "behaviors",
            "mesh_asset", "modulations", "instance_of", "triggers",
        ):
            if field in patch:
                if patch[field] is None:
                    entity.pop(field, None)
                else:
                    entity[field] = patch[field]
        # Extension fields ride along: any `ext-*` key patches like the
        # known ones — set, or clear on null — so a physics component (or
        # any future extension's) survives a modify round-trip. The core
        # schema leaves room for them; must-ignore is the reader's side.
        for field in patch:
            if field.startswith("ext-"):
                if patch[field] is None:
                    entity.pop(field, None)
                else:
                    entity[field] = patch[field]
    elif edit == "SetEnvironment":
        state["environment"] = value.get("env")
    elif edit == "SetCamera":
        state["camera"] = value.get("camera")
    elif edit == "SetAmbience":
        state["ambience"] = _coalesce(value.get("ambience"), [])
    elif edit == "SpawnAudioEmitter":
        if not isinstance(value.get("name"), str):
            raise _invalid("SpawnAudioEmitter needs a name")
        state["audio_emitters"][value["name"]] = value.get("audio")
    elif edit == "RemoveAudioEmitter":
        name = value.get("name")
        if name not in state["audio_emitters"]:
            raise _invalid(f"no audio emitter named '{name}'")
        del state["audio_emitters"][name]
    elif edit == "Batch":
        ops = _coalesce(value.get("ops"), [])
        # All-or-nothing: apply to a deep copy, commit on success.
        trial = _fresh_trial(state)
        for op in ops:
            c = classify_op(op)
            if c["kind"] != "edit":
                continue  # history inside a batch folds to nothing too
            _apply_edit(trial["state"], c["edit"], c["value"])
        _commit_trial(state, trial)
    else:
        raise _invalid(f"unknown edit {edit}")


def _fresh_trial(state: dict) -> dict:
    """A deep-copied trial state, with its id/name maps rebuilt."""
    entities = copy.deepcopy(state["entities"])
    return {
        "state": {
            "entities": entities,
            "environment": copy.deepcopy(state["environment"]),
            "camera": copy.deepcopy(state["camera"]),
            "ambience": copy.deepcopy(state["ambience"]),
            "audio_emitters": {
                k: copy.deepcopy(v) for k, v in state["audio_emitters"].items()
            },
            "by_id": {e["id"]: e for e in entities},
            "names": {e["name"] for e in entities},
        },
    }


def _commit_trial(state: dict, trial: dict) -> None:
    """Commit a trial's document fields and rebuilt maps onto the fold state."""
    t = trial["state"]
    state["entities"] = t["entities"]
    state["environment"] = t["environment"]
    state["camera"] = t["camera"]
    state["ambience"] = t["ambience"]
    state["audio_emitters"] = t["audio_emitters"]
    state["by_id"] = t["by_id"]
    state["names"] = t["names"]


def fold_log(manifest: dict, entries: list) -> dict:
    """Fold log entries over a manifest: the document at the last entry.

    :param manifest: a parsed manifest (the base, at base_revision)
    :param entries: parsed log entries, in order
    :return: ``{"name", "entities", "environment", "camera", "ambience",
        "audio_emitters", "applied_edits"}`` — plus the internal
        ``by_id``/``names`` maps the fold maintains
    :raises WorldFormatError: at the first entry that no longer applies —
        the fold stops there, exactly as the specification's readers do.
    """
    meta = manifest.get("meta")
    name = meta.get("name") if isinstance(meta, dict) else None
    state = {
        "name": _coalesce(name, ""),
        "entities": copy.deepcopy(_coalesce(manifest.get("entities"), [])),
        "environment": manifest.get("environment"),
        "camera": manifest.get("camera"),
        "ambience": _coalesce(manifest.get("ambience"), []),
        "audio_emitters": {},
        "applied_edits": 0,
    }
    state["by_id"] = {e["id"]: e for e in state["entities"]}
    state["names"] = {e["name"] for e in state["entities"]}

    for entry in entries:
        edits = edit_ops(entry)
        if not edits:
            continue  # history folds to nothing
        trial = _fresh_trial(state)
        for c in edits:
            _apply_edit(trial["state"], c["edit"], c["value"])
        _commit_trial(state, trial)
        state["applied_edits"] += len(edits)
    return state
