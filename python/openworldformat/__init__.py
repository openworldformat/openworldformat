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
it. Around the fold sit the pieces the spec asks of every reference:
canonical entry identity, immediate name binding, the computed inverse
(undo is appending it), branch merges with id remapping, package
compaction, a strict reader, and the provenance accessor.

Spec: https://openworldformat.org  ·  schema version 3
"""

from __future__ import annotations

import copy
import hashlib
import json
import re
from pathlib import Path
from typing import Any

from .entity_refs import ENTITY_REFS

__all__ = [
    "SUPPORTED_SCHEMA_VERSION",
    "SUPPORTED_FORMAT_VERSION",
    "BASE_SNAPSHOT",
    "WORLD_PATCH_KEYS",
    "MAX_ENTITY_ID",
    "REGISTERED_EXTENSIONS",
    "EXT_PROVENANCE_FIELDS",
    "ENTITY_REFS",
    "WorldFormatError",
    "parse_manifest",
    "classify_op",
    "op_kind_shape_ok",
    "parse_log_line",
    "edit_ops",
    "canonical_json",
    "compute_entry_id",
    "manifest_text",
    "fold_state",
    "build_history",
    "fold_path",
    "fold_log",
    "to_manifest",
    "ingest",
    "merge_patch",
    "compute_inverse",
    "merge_branch",
    "snapshot_filename",
    "compact_package",
    "compact",
    "read_package",
    "ext_provenance",
]


class WorldFormatError(ValueError):
    """A document or log that violates the format's rules."""


#: The manifest schema version this fold reads.
SUPPORTED_SCHEMA_VERSION = 3

#: The package format version this fold reads: 2, head-first —
#: ``manifest.json`` is the world at the tip of ``main``, the base lives
#: in ``snapshots/base.json`` (spec/package.md).
SUPPORTED_FORMAT_VERSION = 2

#: Where a head-first package keeps the state its log folds from.
BASE_SNAPSHOT = "snapshots/base.json"

#: The fields ``ModifyWorld``'s patch reaches (spec/session.md).
WORLD_PATCH_KEYS = (
    "meta", "environment", "camera", "avatar", "tours", "soundtrack",
    "ambience", "creations",
)

#: The entity id ceiling: 2^53 − 1, the largest integer every JSON
#: number implementation reads exactly. Ids above it are refused at the
#: door, not silently rounded by whichever reader has the weakest
#: number type (spec/world.md "Identity").
MAX_ENTITY_ID = 9007199254740991

#: The extensions the registry knows (spec/extensions/registry.json) —
#: the names strict mode accepts wherever an ``ext-*`` key can appear.
REGISTERED_EXTENSIONS = (
    "ext-physics",
    "ext-strict-determinism",
    "ext-visibility",
    "ext-cinematography",
    "ext-provenance",
)

#: The edit kinds in the spec's own order — the order error messages
#: name them in, kept deterministic where a frozenset would not be.
_EDIT_KIND_ORDER = (
    "SpawnEntity",
    "DeleteEntity",
    "ModifyEntity",
    "SetEnvironment",
    "SetCamera",
    "SetAmbience",
    "SpawnAudioEmitter",
    "RemoveAudioEmitter",
    "ModifyWorld",
    "Batch",
)

EDIT_KEYS = frozenset(_EDIT_KIND_ORDER)

#: The history kinds — lowercase, per the collision rule.
HISTORY_KEYS = frozenset({"tool", "input", "state", "clock", "merge"})

#: The keys strict mode allows at the manifest's top level: the
#: schema's own, plus any registered extension.
_MANIFEST_KEYS = frozenset({
    "version", "meta", "entities", "environment", "camera", "avatar",
    "ambience", "tours", "soundtrack", "creations", "next_entity_id",
})

#: The keys strict mode allows inside ``meta``.
_META_KEYS = frozenset({
    "name", "description", "time_of_day", "tags", "source",
    "variation_group", "variation", "style_ref", "compliance",
})

#: The keys strict mode allows on an entity.
_ENTITY_KEYS = frozenset({
    "id", "name", "parent", "transform", "chunk", "shape", "material",
    "light", "audio", "behaviors", "modulations", "triggers",
    "mesh_asset", "instance_of", "creation_id",
})

#: The keys a live-authored op may carry, by where it carries them
#: (spec/world.md) — what :func:`ingest` refuses beyond, not what the
#: fold ignores: the fold must tolerate, the authoring surface must not
#: write keys the format would silently drop.
_TRANSFORM_KEYS = frozenset({"position", "rotation_degrees", "scale", "visible"})
_MATERIAL_KEYS = frozenset({
    "alpha_mode", "base_color_texture", "color", "double_sided", "emissive",
    "emissive_texture", "metallic", "metallic_roughness_texture",
    "normal_map_texture", "reflectance", "roughness", "unlit",
})
_LIGHT_KEYS = frozenset({
    "color", "direction", "inner_angle", "intensity", "light_type",
    "outer_angle", "range", "shadows",
})
_ENVIRONMENT_KEYS = frozenset({
    "ambient_color", "ambient_intensity", "background_color", "fog_color",
    "fog_density",
})
_CAMERA_KEYS = frozenset({"fov_degrees", "look_at", "position"})

_EXT_KEY = re.compile(r"ext-[a-z0-9-]+")


def _is_num(v: Any) -> bool:
    """JSON number, not a bool riding int's coat tails."""
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _coalesce(v: Any, default: Any) -> Any:
    """JavaScript's ``??``: null/undefined fall back, everything else passes."""
    return default if v is None else v


# ---------------------------------------------------------------------------
# Entity-reference fields (schema/entity-refs.json, spec/world.md
# "Identity") — name binding, merge rewriting and ingestion validation
# read the schema's one marked list (the embedded copy,
# openworldformat/entity_refs.py) instead of hand-written ones.
# ---------------------------------------------------------------------------


def _refs_of(scope: str, kind: str | None = None) -> list:
    """The marked refs of one scope (and optionally one kind)."""
    return [
        ref for ref in ENTITY_REFS
        if ref["scope"] == scope and (kind is None or ref["kind"] == kind)
    ]


def _walk_ref_path(node: Any, path: list, i: int, fn) -> None:
    """Walk a marked path, calling ``fn(current, set)`` at the leaf;
    ``*`` walks every array element. Missing keys walk to nothing."""
    if not isinstance(node, (dict, list)):
        return
    key = path[i]
    last = i == len(path) - 1
    if key == "*":
        if not isinstance(node, list):
            return
        for j, element in enumerate(node):
            if last:
                fn(element, lambda v, j=j: node.__setitem__(j, v))
            else:
                _walk_ref_path(element, path, i + 1, fn)
        return
    if not isinstance(node, dict) or key not in node:
        return
    if last:
        fn(node[key], lambda v: node.__setitem__(key, v))
    else:
        _walk_ref_path(node[key], path, i + 1, fn)


def _each_ref(scope: str, value: Any, fn) -> None:
    """Every marked entity-reference field of ``value`` — an entity or
    patch ("entity"), the manifest's avatar ("avatar"), one creation
    ("creation") — calling ``fn(current, set)`` on each, in place."""
    if not isinstance(value, (dict, list)):
        return
    for ref in _refs_of(scope):
        _walk_ref_path(value, ref["path"], 0, fn)


# ---------------------------------------------------------------------------
# Parsing
# ---------------------------------------------------------------------------


def _check_keys(where: str, holder: dict, allowed: frozenset,
                moved: frozenset = frozenset()) -> None:
    """Strict mode's key check: every key must be in the schema's set or
    a registered extension's — the fields that moved out of the core get
    an error pointing at the extension they moved to instead."""
    for key in holder:
        if key in allowed or key in REGISTERED_EXTENSIONS:
            continue
        if key in moved:
            raise WorldFormatError(
                f"{where} key '{key}' moved to the ext-provenance extension — "
                'write it in meta["ext-provenance"] '
                "(spec/extensions/provenance.md)"
            )
        if _EXT_KEY.fullmatch(key):
            raise WorldFormatError(
                f"{where} key '{key}' isn't in the extension registry "
                "(spec/extensions/registry.json)"
            )
        raise WorldFormatError(f"{where} key '{key}' isn't in the schema")


def parse_manifest(text: str, strict: bool = False) -> dict:
    """Parse and sanity-check a world document.

    :param text: the manifest's JSON text
    :param strict: also enforce the schema's key sets — top level,
        ``meta``, and every entity limited to the schema's keys plus
        registered extensions, the provenance fields pointed at their
        extension, unregistered ``ext-*`` named for what they are. The
        default stays tolerant: must-ignore is the reader's side of the
        contract, and history must never break a fold.
    :return: the manifest
    :raises ValueError: when the text isn't JSON
    :raises WorldFormatError: when the schema version isn't supported,
        or (strict) when a key is outside the schema
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
    if strict:
        _check_keys("manifest", manifest, _MANIFEST_KEYS)
        meta = manifest.get("meta")
        if isinstance(meta, dict):
            _check_keys("meta", meta, _META_KEYS, moved=EXT_PROVENANCE_FIELDS)
        for n, entity in enumerate(manifest["entities"]):
            if isinstance(entity, dict):
                _check_keys(f"entity {entity.get('id', n)}", entity, _ENTITY_KEYS)
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


def op_kind_shape_ok(kind: str) -> bool:
    """The shape collision rule as a predicate (spec/session.md
    "Compatibility"): edits MUST be PascalCase and history kinds MUST be
    lowercase, so the two namespaces can never collide — a lowercase
    kind a reader doesn't know folds to nothing, and nothing lowercase
    can ever be mistaken for an edit by shape. Extension names sit
    outside the predicate: they are a registry concern, not a casing
    concern.

    :param kind: an op kind name
    :return: True when the kind is a cased edit kind, or one of the
        lowercase history kinds
    """
    return (kind in EDIT_KEYS and kind[:1].isupper()) or kind in HISTORY_KEYS


def parse_log_line(line: str, strict: bool = False) -> dict:
    """Parse one log line into an entry with classified ops. Unreadable lines
    are the writer's crash, not the reader's — the caller decides whether
    to skip (the spec says skip the last one, count the rest).

    :param line: one line of ops.jsonl
    :param strict: also refuse ops that classify as unknown, and
        extension ops the registry doesn't name. The default keeps the
        tolerant read: an unknown op folds to nothing, and must.
    :return: the entry, with ``classified`` added
    :raises ValueError: when the line isn't JSON
    :raises WorldFormatError: when the entry has no revision or ops
        array, or (strict) when an op is unrecognized or unregistered
    """
    entry = json.loads(line)
    if (
        not isinstance(entry, dict)
        or not _is_num(entry.get("revision"))
        or not isinstance(entry.get("ops"), list)
    ):
        raise WorldFormatError("log entry needs a revision and an ops array")
    classified = [classify_op(op) for op in entry["ops"]]
    if strict:
        for op, c in zip(entry["ops"], classified):
            if c["kind"] == "unknown":
                raise WorldFormatError(
                    "op shape not recognized (strict mode): "
                    + json.dumps(op)[:100]
                )
            if c["kind"] == "extension" and c["name"] not in REGISTERED_EXTENSIONS:
                raise WorldFormatError(
                    f"extension '{c['name']}' isn't in the extension registry "
                    "(spec/extensions/registry.json)"
                )
    entry["classified"] = classified
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


def canonical_json(value: Any) -> str:
    """The canonical JSON form for hashing (spec/session.md "Entry
    identity, forks and branches"): no whitespace, keys sorted
    recursively, arrays in order. Identical entries therefore hash
    identically across forks — and across languages, for integer-valued
    JSON. Float formatting is each language's own (``1.0`` may print as
    ``1`` elsewhere, and neither is wrong), so cross-language hash
    equality is only promised for documents whose numbers are integers.

    :param value: any JSON value
    :return: its canonical text
    """
    return json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    )


def compute_entry_id(entry: dict) -> str:
    """An entry's content id: the entry deep-copied, its own ``id``
    dropped (an entry is content, not a self-addressing brick — the id
    is the *of* the hash, never part of it), canonicalized and
    SHA-256'd. Two peers appending the same entry to forked logs
    compute the same id, which is the property that keeps identical
    ids identical across branches.

    :param entry: a log entry
    :return: ``sha256:<hex>``
    """
    body = copy.deepcopy(entry)
    body.pop("id", None)
    # ``classified`` is this reader's annotation of the ops, not
    # content: :func:`parse_log_line` adds it, and the hash must not
    # see it, or the same line hashed before and after parsing would
    # name two entries.
    body.pop("classified", None)
    digest = hashlib.sha256(canonical_json(body).encode("utf-8")).hexdigest()
    return f"sha256:{digest}"


def manifest_text(manifest: dict) -> str:
    """A manifest as its canonical text (spec/package.md "Canonical
    text"): what an authority writes to ``manifest.json``, so that the
    same world is always the same bytes — small diffs, ordinary git
    merges, a ``world_sha256`` that means something. Members sorted by
    code point, null members left out, entities in id order, two-space
    indentation, arrays of plain values on one line, numbers in their
    shortest form (integral values without a fraction), a trailing
    newline.

    :param manifest: a world document
    :return: its canonical text
    """
    world = copy.deepcopy(manifest)
    if isinstance(world.get("entities"), list):
        world["entities"].sort(key=lambda e: e.get("id", 0))
    return _canonical_pretty(world, 0) + "\n"


def _canonical_scalar(value: Any) -> str:
    """One non-container value in its shortest JSON form — ``1.0`` is
    ``1``, as every JSON writer but Python's would print it."""
    if isinstance(value, float) and value.is_integer() and abs(value) < 1e16:
        return str(int(value))
    return json.dumps(value, ensure_ascii=False)


def _canonical_pretty(value: Any, depth: int) -> str:
    """The canonical pretty form: objects multi-line with sorted members,
    arrays of plain values inline, nested arrays multi-line."""
    if value is None or not isinstance(value, (dict, list)):
        return _canonical_scalar(value)
    pad = "  " * depth
    inner = "  " * (depth + 1)
    if isinstance(value, list):
        if not value:
            return "[]"
        if all(x is None or not isinstance(x, (dict, list)) for x in value):
            return "[" + ", ".join(_canonical_scalar(x) for x in value) + "]"
        return ("[\n" + ",\n".join(inner + _canonical_pretty(x, depth + 1) for x in value)
                + "\n" + pad + "]")
    keys = sorted(k for k in value if value[k] is not None)
    if not keys:
        return "{}"
    return ("{\n" + ",\n".join(
        f'{inner}{json.dumps(k, ensure_ascii=False)}: {_canonical_pretty(value[k], depth + 1)}'
        for k in keys) + "\n" + pad + "}")


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


def _resolve_names(state: dict, entity: dict) -> None:
    """Bind one entity's by-name references to ids, in place
    (spec/world.md "Identity"): cross-entity references may be written
    by name — what authors and models produce — and MUST resolve to id
    at ingestion against the fold-so-far. Delaying resolution until
    fold time is strictly forbidden: it breaks log determinism the
    moment an entity is renamed. The fields are the schema's marked
    entity refs (``entity`` scope, ``bindable``) except the top-level
    ones (``parent``), which op intake binds *before* apply — the
    fold's apply step validates them, so a raw string there is already
    a refusal. ``modulations[]`` targets name properties, not entities,
    and are never touched.

    :raises WorldFormatError: when a named entity isn't in the fold
    """

    def bind(current, set_):
        if not isinstance(current, str):
            return  # already an id — saved worlds always contain ids
        id_ = state["name_to_id"].get(current)
        if id_ is None:
            raise _invalid(f"no entity named '{current}'")
        set_(id_)

    for ref in _refs_of("entity", "bindable"):
        if len(ref["path"]) == 1:
            continue  # bound at op intake (_bind_op)
        _walk_ref_path(entity, ref["path"], 0, bind)


def _touched_ids(edits: list) -> list:
    """The entity ids an entry's edits touch — spawned or modified,
    batches recursed — the entities whose by-name references bind at
    ingestion. Deletions need no binding: what they touched is gone."""
    ids: list = []
    for c in edits:
        edit, value = c["edit"], c["value"]
        if edit == "SpawnEntity":
            entity = value.get("entity") if isinstance(value, dict) else None
            if isinstance(entity, dict) and _is_num(entity.get("id")):
                ids.append(entity["id"])
        elif edit == "ModifyEntity":
            if isinstance(value, dict):
                ids.append(value.get("id"))
        elif edit == "Batch":
            inner = value.get("ops") if isinstance(value, dict) else None
            if isinstance(inner, list):
                classified = [classify_op(op) for op in inner]
                ids.extend(_touched_ids(
                    [c for c in classified if c["kind"] == "edit"]
                ))
    return ids


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
        if entity["id"] > MAX_ENTITY_ID:
            raise _invalid(
                f"entity {entity['id']} exceeds the id ceiling "
                f"{MAX_ENTITY_ID} (2^53-1)"
            )
        if entity["id"] in state["by_id"]:
            raise _invalid(f"entity {entity['id']} already exists")
        if entity["name"] in state["names"]:
            raise _invalid(f"an entity named '{entity['name']}' already exists")
        parent = entity.get("parent")
        if parent is not None and parent not in state["by_id"]:
            raise _invalid(f"entity {entity['id']}'s parent {parent} isn't in the document")
        # The fold owns its copy: name binding (and any later edit) writes
        # the document's entity, never the entry the writer still holds.
        entity = copy.deepcopy(entity)
        scene = state["scene"]
        scene["next_entity_id"] = max(scene["next_entity_id"], entity["id"] + 1)
        state["entities"].append(entity)
        state["by_id"][entity["id"]] = entity
        state["names"].add(entity["name"])
        state["name_to_id"][entity["name"]] = entity["id"]
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
                state["name_to_id"].pop(e["name"], None)
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
                state["name_to_id"].pop(entity["name"], None)
                entity["name"] = patch["name"]
                state["names"].add(patch["name"])
                state["name_to_id"][patch["name"]] = entity["id"]
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
    elif edit == "ModifyWorld":
        # The scene-wide fields, patched like an entity: absent
        # unchanged, None clears, a value sets (spec/session.md).
        patch = _coalesce(value.get("patch"), {})
        scene = state["scene"]
        if "meta" in patch:
            meta = patch["meta"]
            if not isinstance(meta, dict) or not isinstance(meta.get("name"), str) \
                    or not meta["name"].strip():
                raise _invalid(
                    "ModifyWorld.meta must be a meta object with a name; "
                    "it can't be cleared"
                )
            scene["meta"] = meta
            state["name"] = meta["name"]
        if "environment" in patch:
            state["environment"] = patch["environment"]
        if "camera" in patch:
            state["camera"] = patch["camera"]
        if "ambience" in patch:
            state["ambience"] = _coalesce(patch["ambience"], [])
        for field in ("avatar", "soundtrack"):
            if field in patch:
                if patch[field] is None:
                    scene.pop(field, None)
                else:
                    scene[field] = patch[field]
        for field in ("tours", "creations"):
            if field in patch:
                scene[field] = _coalesce(patch[field], [])
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
            "name": state.get("name", ""),
            "scene": copy.deepcopy(state["scene"]),
            "entities": entities,
            "environment": copy.deepcopy(state["environment"]),
            "camera": copy.deepcopy(state["camera"]),
            "ambience": copy.deepcopy(state["ambience"]),
            "audio_emitters": {
                k: copy.deepcopy(v) for k, v in state["audio_emitters"].items()
            },
            "by_id": {e["id"]: e for e in entities},
            "names": {e["name"] for e in entities},
            "name_to_id": {e["name"]: e["id"] for e in entities},
        },
    }


def _commit_trial(state: dict, trial: dict) -> None:
    """Commit a trial's document fields and rebuilt maps onto the fold state."""
    t = trial["state"]
    state["name"] = t["name"]
    state["scene"] = t["scene"]
    state["entities"] = t["entities"]
    state["environment"] = t["environment"]
    state["camera"] = t["camera"]
    state["ambience"] = t["ambience"]
    state["audio_emitters"] = t["audio_emitters"]
    state["by_id"] = t["by_id"]
    state["names"] = t["names"]
    state["name_to_id"] = t["name_to_id"]


def _scene_of(manifest: dict) -> dict:
    """The manifest fields a fold carries besides entities, environment,
    camera and ambience. ``next_entity_id`` only grows: ids are never
    reused."""
    entities = _coalesce(manifest.get("entities"), [])
    past = max((e.get("id", 0) + 1 for e in entities), default=1)
    scene = {
        "version": manifest.get("version"),
        "meta": copy.deepcopy(manifest.get("meta")),
        "tours": copy.deepcopy(_coalesce(manifest.get("tours"), [])),
        "creations": copy.deepcopy(_coalesce(manifest.get("creations"), [])),
        "next_entity_id": max(_coalesce(manifest.get("next_entity_id"), 1), past),
    }
    for field in ("avatar", "soundtrack"):
        if manifest.get(field) is not None:
            scene[field] = copy.deepcopy(manifest[field])
    return scene


def to_manifest(state: dict) -> dict:
    """The fold's state as a manifest — the whole document. The fold is
    total (spec/session.md): ``to_manifest(fold_log(m, []))`` is ``m``
    again, up to name binding, entity order and absent-versus-default
    fields; a head-first package's ``manifest.json`` is ``to_manifest``
    of its fold to ``main``.

    :param state: a fold state, as :func:`fold_log` returns
    :return: a manifest dict
    """
    scene = state["scene"]
    meta = copy.deepcopy(scene.get("meta")) if isinstance(scene.get("meta"), dict) else {}
    meta["name"] = state.get("name", "")
    manifest = {"version": scene.get("version"), "meta": meta}
    if state.get("environment") is not None:
        manifest["environment"] = copy.deepcopy(state["environment"])
    if state.get("camera") is not None:
        manifest["camera"] = copy.deepcopy(state["camera"])
    if "avatar" in scene:
        manifest["avatar"] = copy.deepcopy(scene["avatar"])
    if scene.get("tours"):
        manifest["tours"] = copy.deepcopy(scene["tours"])
    if "soundtrack" in scene:
        manifest["soundtrack"] = copy.deepcopy(scene["soundtrack"])
    if state.get("ambience"):
        manifest["ambience"] = copy.deepcopy(state["ambience"])
    manifest["entities"] = copy.deepcopy(state["entities"])
    if scene.get("creations"):
        manifest["creations"] = copy.deepcopy(scene["creations"])
    past = max((e["id"] + 1 for e in state["entities"]), default=1)
    manifest["next_entity_id"] = max(scene["next_entity_id"], past)
    return manifest


def fold_log(manifest: dict, entries: list) -> dict:
    """Fold log entries over a manifest: the document at the last entry.

    By-name references bind here, at ingestion: the base's entities
    resolve against the complete base, each entry's against the
    fold-so-far including the entry's own edits (spec/world.md
    "Identity" — saved worlds always contain ids).

    :param manifest: a parsed manifest (the base, at base_revision — in a
        head-first package, ``snapshots/base.json``)
    :param entries: parsed log entries, in order
    :return: ``{"name", "entities", "environment", "camera", "ambience",
        "audio_emitters", "applied_edits"}`` — plus the internal
        ``by_id``/``names``/``name_to_id`` maps the fold maintains
    :raises WorldFormatError: at the first entry that no longer applies
        — or whose names don't bind — the fold stops there, exactly as
        the specification's readers do.
    """
    meta = manifest.get("meta")
    name = meta.get("name") if isinstance(meta, dict) else None
    state = {
        "name": _coalesce(name, ""),
        "entities": copy.deepcopy(_coalesce(manifest.get("entities"), [])),
        "environment": manifest.get("environment"),
        "camera": manifest.get("camera"),
        "ambience": copy.deepcopy(_coalesce(manifest.get("ambience"), [])),
        "scene": _scene_of(manifest),
        "audio_emitters": {},
        "applied_edits": 0,
    }
    state["by_id"] = {e["id"]: e for e in state["entities"]}
    state["names"] = {e["name"] for e in state["entities"]}
    state["name_to_id"] = {e["name"]: e["id"] for e in state["entities"]}

    # A manifest written by name binds now, against the complete base —
    # an entity may reference a neighbor declared after it.
    for entity in state["entities"]:
        _resolve_names(state, entity)

    for entry in entries:
        edits = edit_ops(entry)
        if not edits:
            continue  # history folds to nothing
        # No trial copy here. ``fold_log`` owns ``state`` — its entities
        # were deep-copied off the manifest above — and it raises without
        # it when an entry no longer applies, so a per-entry copy
        # protected a document nobody could observe. It deep-copied every
        # entity per entry, which made folding quadratic in the log's
        # length: 2,000 entries took 4.2 s. ``_apply_edit`` maintains
        # ``entities``, ``by_id``, ``names`` and ``name_to_id`` itself, so
        # the trial's rebuild was never load-bearing either.
        for c in edits:
            _apply_edit(state, c["edit"], c["value"])
        # Names bind after the entry's edits apply: an entry is atomic in
        # its *ingestion*, and an unresolvable name fails it whole.
        for id_ in _touched_ids(edits):
            entity = state["by_id"].get(id_)
            if entity is not None:
                _resolve_names(state, entity)
        state["applied_edits"] += len(edits)
    return state


# ---------------------------------------------------------------------------
# Live authoring (spec/session.md "Authoring")
# ---------------------------------------------------------------------------


def merge_patch(current: Any, change: Any) -> Any:
    """JSON merge patch (RFC 7396): objects merge key by key, ``None``
    removes, anything else replaces.

    :param current: the value as it stands
    :param change: the patch
    :return: the patched value — neither input is touched
    """
    if not isinstance(current, dict) or not isinstance(change, dict):
        return copy.deepcopy(change)
    out = copy.deepcopy(current)
    for key, value in change.items():
        if value is None:
            out.pop(key, None)
        else:
            out[key] = merge_patch(out[key], value) if key in out else copy.deepcopy(value)
    return out


def _bind_op(trial: dict, op: dict, spawned: dict) -> None:
    """Names to ids, ids for spawns, merged struct patches — in place
    (spec/session.md "Authoring"). A string where an entity id goes is a
    name; a spawn without an id gets the next one; the struct fields of
    a patch merge into the current value so an author may send a
    partial transform and keep the scale they never mentioned.

    :raises WorldFormatError: when a name nothing owns is used
    """
    if not isinstance(op, dict):
        raise WorldFormatError('an op is an object like {"SpawnEntity": {…}}')
    kinds = list(op)
    if len(kinds) != 1:
        raise WorldFormatError(
            f"an op holds exactly one kind, one of: {', '.join(_EDIT_KIND_ORDER)}"
        )
    kind = kinds[0]
    body = op[kind]

    def resolve(ref):
        if not isinstance(ref, str):
            return ref
        id_ = spawned.get(ref)
        if id_ is None:
            id_ = trial["name_to_id"].get(ref)
        if id_ is None:
            raise WorldFormatError(f'no entity is named "{ref}"')
        return id_

    def bind_top_level_refs(value):
        # Top-level bindable entity refs (today: ``parent``) bind at op
        # intake, before the op applies; behavior refs bind after,
        # against the spawned world (_resolve_names) — the same list,
        # walked per pass.
        for ref in _refs_of("entity", "bindable"):
            if len(ref["path"]) == 1:
                _walk_ref_path(value, ref["path"], 0,
                               lambda current, set_: set_(resolve(current)))

    if kind == "SpawnEntity":
        entity = body.get("entity") if isinstance(body, dict) else None
        if not isinstance(entity, dict):
            raise WorldFormatError('SpawnEntity needs an "entity" object')
        if entity.get("id") is None:
            entity["id"] = max(
                [trial["scene"]["next_entity_id"]] + [i + 1 for i in spawned.values()]
            )
        elif not _is_num(entity["id"]):
            raise WorldFormatError("a new entity's id is a number, or left out to get one")
        bind_top_level_refs(entity)
        if isinstance(entity.get("name"), str):
            spawned[entity["name"]] = entity["id"]
    elif kind == "ModifyEntity":
        if not isinstance(body, dict):
            raise WorldFormatError("ModifyEntity needs an object body")
        body["id"] = resolve(body.get("id"))
        current = trial["by_id"].get(body["id"])
        patch = body.get("patch")
        if isinstance(patch, dict):
            bind_top_level_refs(patch)
            if current is not None:
                for field in ("transform", "material", "light"):
                    change, held = patch.get(field), current.get(field)
                    if isinstance(change, dict) and isinstance(held, dict):
                        patch[field] = merge_patch(held, change)
    elif kind == "DeleteEntity":
        if not isinstance(body, dict):
            raise WorldFormatError("DeleteEntity needs an object body")
        body["id"] = resolve(body.get("id"))
    elif kind == "SetEnvironment":
        env = body.get("env") if isinstance(body, dict) else None
        if isinstance(env, dict) and isinstance(trial.get("environment"), dict):
            body["env"] = merge_patch(trial["environment"], env)
    elif kind == "ModifyWorld":
        patch = body.get("patch") if isinstance(body, dict) else None
        if isinstance(patch, dict):
            # The avatar's marked refs (today: ``model_entity``) bind
            # like any other — a name where an entity id goes resolves
            # at intake (spec/world.md "Identity").
            avatar = patch.get("avatar")
            if isinstance(avatar, dict):
                _each_ref("avatar", avatar,
                          lambda current, set_: set_(resolve(current)))
            now = to_manifest(trial)
            for field in ("meta", "environment", "camera", "avatar", "soundtrack"):
                change, held = patch.get(field), now.get(field)
                if isinstance(change, dict) and isinstance(held, dict):
                    patch[field] = merge_patch(held, change)
    elif kind == "Batch":
        inner = body.get("ops") if isinstance(body, dict) else None
        if isinstance(inner, list):
            for op_ in inner:
                _bind_op(trial, op_, spawned)
    elif kind not in EDIT_KEYS:
        raise WorldFormatError(
            f'"{kind}" isn\'t an op kind; the format has: {", ".join(_EDIT_KIND_ORDER)}'
        )


def _strict_op(op: dict) -> None:
    """Refuse keys the format would drop, with a path to each — the
    authoring side of must-ignore: a tolerant fold would silently lose
    an unknown key, so the surface that writes ops refuses it instead.

    :raises WorldFormatError: naming every key outside the schema
    """
    unknown: list = []

    def check(obj, allowed, path):
        if not isinstance(obj, dict):
            return
        for key in obj:
            if key in allowed or key in REGISTERED_EXTENSIONS:
                continue
            unknown.append(f"{path}/{key}")

    def check_entity(entity, path, is_patch):
        keys = set(_ENTITY_KEYS)
        if is_patch:
            keys.discard("id")
        check(entity, keys, path)
        transform = entity.get("transform") if isinstance(entity, dict) else None
        check(transform, _TRANSFORM_KEYS, f"{path}/transform")
        material = entity.get("material") if isinstance(entity, dict) else None
        check(material, _MATERIAL_KEYS, f"{path}/material")
        light = entity.get("light") if isinstance(entity, dict) else None
        check(light, _LIGHT_KEYS, f"{path}/light")

    kind = next(iter(op))
    body = op[kind]
    if kind == "SpawnEntity":
        check(body, {"entity"}, f"/{kind}")
        check_entity(body.get("entity") if isinstance(body, dict) else None,
                     f"/{kind}/entity", False)
    elif kind == "ModifyEntity":
        check(body, {"id", "patch"}, f"/{kind}")
        check_entity(body.get("patch") if isinstance(body, dict) else None,
                     f"/{kind}/patch", True)
    elif kind == "DeleteEntity":
        check(body, {"id"}, f"/{kind}")
    elif kind == "SetEnvironment":
        check(body.get("env") if isinstance(body, dict) else None,
              _ENVIRONMENT_KEYS, f"/{kind}/env")
    elif kind == "SetCamera":
        check(body.get("camera") if isinstance(body, dict) else None,
              _CAMERA_KEYS, f"/{kind}/camera")
    elif kind == "ModifyWorld":
        patch = body.get("patch") if isinstance(body, dict) else None
        check(patch, frozenset(WORLD_PATCH_KEYS), f"/{kind}/patch")
        check(patch.get("meta") if isinstance(patch, dict) else None,
              _META_KEYS, f"/{kind}/patch/meta")
        check(patch.get("environment") if isinstance(patch, dict) else None,
              _ENVIRONMENT_KEYS, f"/{kind}/patch/environment")
        check(patch.get("camera") if isinstance(patch, dict) else None,
              _CAMERA_KEYS, f"/{kind}/patch/camera")
    elif kind == "Batch":
        inner = body.get("ops") if isinstance(body, dict) else None
        if isinstance(inner, list):
            for op_ in inner:
                _strict_op(op_)
    if unknown:
        raise WorldFormatError(
            "; ".join(f"{p} is not a field of the format" for p in unknown)
        )


def ingest(state: dict, batch) -> dict:
    """Ingest a batch — ``[op, …]`` or ``{"ops": [op, …]}`` — against a
    fold state (spec/session.md "Authoring": agents outside, ops in, the
    world in front). Each op, in order, against a trial holding the
    batch's earlier ops: names bind to ids (a string where an entity id
    goes is a name), a spawn without an id gets the next one, the struct
    fields of a patch (:func:`merge_patch` above) merge into the current
    value, no key the format would drop is allowed, and the op must
    apply. Any failure refuses the whole batch.

    :param state: the world as it stands (unchanged by this call)
    :param batch: the ops an author or agent is sending
    :return: ``{"ok": True, "ops", "spawned", "state"}`` — the committed
        ops as they bind (serialize into an entry and append), the names
        the batch spawned with the ids they got, the new world — or
        ``{"ok": False, "errors"}``, one reason per op
    """
    ops = batch if isinstance(batch, list) else (
        batch.get("ops") if isinstance(batch, dict) else None
    )
    if not isinstance(ops, list):
        return {"ok": False, "errors": ['a batch is [op, …] or {"ops": [op, …]}']}
    if not ops:
        return {"ok": False, "errors": ["the batch holds no ops"]}
    trial = _fresh_trial(state)["state"]
    trial["applied_edits"] = state.get("applied_edits", 0)
    spawned: dict = {}
    errors: list = []
    committed: list = []
    for i, raw in enumerate(ops):
        try:
            op = copy.deepcopy(raw)
            _bind_op(trial, op, spawned)
            _strict_op(op)
            c = classify_op(op)
            if c["kind"] != "edit":
                raise WorldFormatError("only edit ops can be sent")
            # An op is an entry of one: apply, then bind the names it wrote.
            step = _fresh_trial(trial)
            _apply_edit(step["state"], c["edit"], c["value"])
            for id_ in _touched_ids([c]):
                entity = step["state"]["by_id"].get(id_)
                if entity is not None:
                    _resolve_names(step["state"], entity)
            _commit_trial(trial, step)
            committed.append(op)
        except WorldFormatError as e:
            errors.append(f"op {i}: {e}")
    if errors:
        return {"ok": False, "errors": errors}
    trial["applied_edits"] += len(committed)
    return {"ok": True, "ops": committed, "spawned": spawned, "state": trial}


# ---------------------------------------------------------------------------
# Undo and merge
# ---------------------------------------------------------------------------

def _subtree_parent_first(state: dict, id_) -> list:
    """The entity and its descendants, every parent before its children —
    the order a Batch of SpawnEntity ops needs to re-plant a deleted
    tree, since a spawn's parent must already stand. Document order
    isn't that order: a re-parent can hang an entity below a younger
    sibling."""
    doomed = {id_}
    grew = True
    while grew:
        grew = False
        for e in state["entities"]:
            p = e.get("parent")
            if p is not None and p in doomed and e["id"] not in doomed:
                doomed.add(e["id"])
                grew = True
    remaining = [e for e in state["entities"] if e["id"] in doomed]
    ordered = []
    placed = set()
    while remaining:
        progress = False
        for e in list(remaining):
            p = e.get("parent")
            if p is None or p not in doomed or p in placed:
                ordered.append(e)
                placed.add(e["id"])
                remaining.remove(e)
                progress = True
        if not progress:
            # Only a malformed base reaches here: a parent cycle no
            # edit path can build but a hand-written manifest might.
            raise _invalid(f"entity {id_}'s subtree has a parent cycle")
    return ordered


def compute_inverse(op: dict, state: dict) -> dict:
    """The inverse of one edit op against the state it's about to change
    (spec/session.md: every edit has a computable inverse, and undo is
    appending it — the log never rewinds). Call with the fold from
    *before* the op applied; append the result after it, and the next
    fold stands where this one did.

    :param op: the raw op dict (``{"SpawnEntity": {...}}``)
    :param state: the fold state the op is about to apply to (as
        :func:`fold_log` returns)
    :return: the inverse op dict
    :raises WorldFormatError: for a non-edit op (only edits invert), an
        unknown edit, or an edit naming an entity the state doesn't
        have — it would never apply
    """
    c = classify_op(op)
    if c["kind"] != "edit":
        raise _invalid(f"no inverse for a '{c['kind']}' op — only edits have one")
    edit, value = c["edit"], c["value"]
    if edit == "SpawnEntity":
        entity = value.get("entity") if isinstance(value, dict) else None
        if not isinstance(entity, dict) or not _is_num(entity.get("id")):
            raise _invalid("SpawnEntity needs an entity with id and name")
        return {"DeleteEntity": {"id": entity["id"]}}
    if edit == "DeleteEntity":
        id_ = value.get("id") if isinstance(value, dict) else None
        if id_ not in state["by_id"]:
            raise _invalid(f"no entity {id_}")
        return {"Batch": {"ops": [
            {"SpawnEntity": {"entity": copy.deepcopy(e)}}
            for e in _subtree_parent_first(state, id_)
        ]}}
    if edit == "ModifyEntity":
        id_ = value.get("id") if isinstance(value, dict) else None
        entity = state["by_id"].get(id_)
        if entity is None:
            raise _invalid(f"no entity {id_}")
        patch = _coalesce(value.get("patch"), {})
        # Every patched key set to what it holds now; None for a field
        # the entity lacks, which is exactly the patch that clears it
        # back off. Name and parent included.
        return {"ModifyEntity": {"id": entity["id"], "patch": {
            field: copy.deepcopy(entity.get(field)) for field in patch
        }}}
    # A scene setting that didn't exist comes back as absent, not as a
    # default one: ModifyWorld clears it.
    if edit == "SetEnvironment":
        if state.get("environment") is None:
            return {"ModifyWorld": {"patch": {"environment": None}}}
        return {"SetEnvironment": {"env": copy.deepcopy(state["environment"])}}
    if edit == "SetCamera":
        if state.get("camera") is None:
            return {"ModifyWorld": {"patch": {"camera": None}}}
        return {"SetCamera": {"camera": copy.deepcopy(state["camera"])}}
    if edit == "ModifyWorld":
        patch = _coalesce(value.get("patch"), {}) if isinstance(value, dict) else {}
        now = to_manifest(state)
        inverse = {}
        for field in patch:
            if field not in WORLD_PATCH_KEYS:
                continue  # must-ignore
            if field in ("tours", "creations", "ambience"):
                inverse[field] = copy.deepcopy(now.get(field, []))
            else:
                inverse[field] = copy.deepcopy(now.get(field))
        return {"ModifyWorld": {"patch": inverse}}
    if edit == "SetAmbience":
        ambience = _coalesce(state.get("ambience"), [])
        return {"SetAmbience": {"ambience": copy.deepcopy(ambience)}}
    if edit == "SpawnAudioEmitter":
        if not isinstance(value, dict) or not isinstance(value.get("name"), str):
            raise _invalid("SpawnAudioEmitter needs a name")
        return {"RemoveAudioEmitter": {"name": value["name"]}}
    if edit == "RemoveAudioEmitter":
        name = value.get("name") if isinstance(value, dict) else None
        if name not in state.get("audio_emitters", {}):
            raise _invalid(f"no audio emitter named '{name}'")
        return {"SpawnAudioEmitter": {
            "name": name,
            "audio": copy.deepcopy(state["audio_emitters"][name]),
        }}
    if edit == "Batch":
        ops = _coalesce(value.get("ops"), []) if isinstance(value, dict) else []
        # Each inner inverse against the state its op was about to
        # change: fold forward over a trial, undoing as you go, then
        # reverse — the last undone first.
        trial = _fresh_trial(state)
        inverses = []
        for inner in ops:
            ic = classify_op(inner)
            if ic["kind"] != "edit":
                continue  # history inside a batch has nothing to undo
            inverses.append(compute_inverse(inner, trial["state"]))
            _apply_edit(trial["state"], ic["edit"], ic["value"])
        inverses.reverse()
        return {"Batch": {"ops": inverses}}
    raise _invalid(f"unknown edit {edit}")


def _spawned_ids(ops: list) -> list:
    """Every id these ops' SpawnEntity ops spawn, batches recursed — the
    ids a merge has to check against the trunk's own."""
    ids: list = []
    for op in ops:
        c = classify_op(op)
        if c["kind"] != "edit":
            continue
        edit, value = c["edit"], c["value"]
        if edit == "SpawnEntity":
            entity = value.get("entity") if isinstance(value, dict) else None
            if isinstance(entity, dict) and _is_num(entity.get("id")):
                ids.append(entity["id"])
        elif edit == "Batch":
            inner = value.get("ops") if isinstance(value, dict) else None
            if isinstance(inner, list):
                ids.extend(_spawned_ids(inner))
    return ids


def _merge_rewrite_op(op: dict, remapped: dict) -> dict:
    """One op rewritten through a merge's id remapping — a new, deep
    copied op. The reference fields are the schema's marked entity
    refs: ``entity`` scope on spawn entities and modify patches,
    ``avatar`` and ``creation`` scopes on ``ModifyWorld``'s patch;
    numeric references follow their entities to fresh ids, by-name
    references don't (they bind at ingestion, after the merge).
    Identity fields (``entity.id``, ``ModifyEntity.id``,
    ``DeleteEntity.id``) are op addresses, not schema refs — they stay
    explicit. History ops pass through untouched, because they fold to
    nothing and their contents are nobody's to rewrite."""
    rewritten = copy.deepcopy(op)
    if not remapped:
        return rewritten
    c = classify_op(rewritten)
    if c["kind"] != "edit":
        return rewritten
    edit, value = c["edit"], c["value"]
    if not isinstance(value, dict):
        return rewritten

    def remap(ref):
        if isinstance(ref, int) and not isinstance(ref, bool) and ref in remapped:
            return remapped[ref]
        return ref

    def remap_refs(scope, node):
        _each_ref(scope, node, lambda current, set_: set_(remap(current)))

    if edit == "SpawnEntity":
        entity = value.get("entity")
        if isinstance(entity, dict):
            if "id" in entity:
                entity["id"] = remap(entity["id"])
            remap_refs("entity", entity)
    elif edit == "ModifyEntity":
        if "id" in value:
            value["id"] = remap(value["id"])
        patch = value.get("patch")
        if isinstance(patch, dict):
            remap_refs("entity", patch)
    elif edit == "DeleteEntity":
        if "id" in value:
            value["id"] = remap(value["id"])
    elif edit == "Batch":
        inner = value.get("ops")
        if isinstance(inner, list):
            value["ops"] = [_merge_rewrite_op(o, remapped) for o in inner]
    elif edit == "ModifyWorld":
        patch = value.get("patch")
        if isinstance(patch, dict):
            remap_refs("avatar", patch.get("avatar"))
            creations = patch.get("creations")
            if isinstance(creations, list):
                for creation in creations:
                    remap_refs("creation", creation)
    return rewritten


def _rename_colliding_spawns(state: dict, entries: list) -> None:
    """The spec's name rule, in place on the rewritten entries: a spawned
    name that is taken — by main's fold, or by an earlier spawn in the
    same merge — becomes ``<name>-<n>``, n from 2 up, first unused. Only
    the ``SpawnEntity`` changes; references are ids by then, so a rename
    breaks nothing in the log."""
    taken = set(state["names"])

    def rename_ops(ops: list) -> None:
        for op in ops:
            c = classify_op(op)
            if c["kind"] != "edit":
                continue
            edit, value = c["edit"], c["value"]
            if not isinstance(value, dict):
                continue
            if edit == "Batch":
                inner = value.get("ops")
                if isinstance(inner, list):
                    rename_ops(inner)
            elif edit == "SpawnEntity":
                entity = value.get("entity")
                if not isinstance(entity, dict) or not isinstance(entity.get("name"), str):
                    continue
                name = entity["name"]
                if name not in taken:
                    taken.add(name)
                    continue
                n = 2
                while f"{name}-{n}" in taken:
                    n += 1
                entity["name"] = f"{name}-{n}"
                taken.add(entity["name"])

    for entry in entries:
        ops = entry.get("ops")
        if isinstance(ops, list):
            rename_ops(ops)


def merge_branch(state: dict, entries: list) -> dict:
    """Merge a branch's entries onto a main branch's fold (spec/session.md
    "The merge rules, exactly"): an id the branch spawned that main's
    fold holds is reallocated — the collisions in ascending order — onto
    fresh ids from the fold's effective ``next_entity_id`` (the monotonic
    floor the fold carries: the larger of the declared value and one past
    every id main has ever held, so an id main deleted stays spent),
    counting up, skipping every id the branch spawns, never past the
    ceiling. Every reference to a reallocated id inside the merged batch
    is rewritten through the remap. A branch spawn whose name is taken —
    by main, or by an earlier spawn in the same merge — is renamed
    ``<name>-<n>``, n from 2 up, first unused. The merged entries keep
    their ``id``, ``parent``, ``author`` and ``message`` — an entry's
    identity survives the merge.

    :param state: the main branch's fold state at its head (as
        :func:`fold_log` returns)
    :param entries: the branch's parsed entries, in order
    :return: ``{"entries": new deep-copied entries with references
        rewritten — plain content, the parser's ``classified``
        annotation stripped —, "remapped": old id -> new id}``
    :raises WorldFormatError: when the ids run out under the ceiling
    """
    main_ids = set(state["by_id"])
    ops = [op for entry in entries for op in (entry.get("ops") or [])]
    spawned = set(_spawned_ids(ops))
    remapped: dict = {}
    candidate = state["scene"]["next_entity_id"]
    for old in sorted(spawned):
        if old not in main_ids:
            continue  # no collision, no remap
        while candidate in spawned:
            candidate += 1
        if candidate > MAX_ENTITY_ID:
            raise _invalid(
                f"merge ran out of entity ids under the ceiling {MAX_ENTITY_ID}"
            )
        remapped[old] = candidate
        candidate += 1

    rewritten = []
    for entry in entries:
        new_entry = copy.deepcopy(entry)
        # ``classified`` is this reader's annotation of the ops, not the
        # log's content: it never rides out of a merge.
        new_entry.pop("classified", None)
        new_entry["ops"] = [
            _merge_rewrite_op(op, remapped) for op in new_entry.get("ops") or []
        ]
        rewritten.append(new_entry)
    _rename_colliding_spawns(state, rewritten)
    return {"entries": rewritten, "remapped": remapped}


# ---------------------------------------------------------------------------
# Extensions
# ---------------------------------------------------------------------------

#: The fields the provenance extension defines
#: (spec/extensions/provenance.md) — the LLM lineage fields that moved
#: out of the core meta and into an extension of their own.
EXT_PROVENANCE_FIELDS = (
    "prompt",
    "model",
    "generation_duration_ms",
    "biome",
    "semantic_category",
)


def ext_provenance(manifest: dict) -> dict | None:
    """The provenance extension's fields, extracted from a manifest's
    ``meta["ext-provenance"]``: the five the extension defines and
    nothing else — unknown keys inside an extension are must-ignore,
    same as everywhere else in the format. None when the manifest
    carries none.

    A tool circulating worlds MUST warn the user or scrub ``prompt``
    before publishing (spec/extensions/provenance.md "Security and
    Privacy") — it often carries the exact text the author typed.

    :param manifest: a parsed manifest
    :return: the extension's fields as a dict, or None
    """
    meta = manifest.get("meta")
    block = meta.get("ext-provenance") if isinstance(meta, dict) else None
    if not isinstance(block, dict):
        return None
    return {
        field: copy.deepcopy(block[field])
        for field in EXT_PROVENANCE_FIELDS
        if field in block
    }


# ---------------------------------------------------------------------------
# Packages
# ---------------------------------------------------------------------------


def snapshot_filename(entry_id: str | None, revision: int) -> str:
    """Where a snapshot goes (spec/session.md "Snapshots"):
    ``snapshots/entry-<id>.json`` when the log's entries carry ids,
    ``snapshots/rev-<N>.json`` for a linear log without them — derived,
    never authoritative, deletable without loss. Characters a
    filesystem can't be trusted with fold to ``_``, so a content id
    (``sha256:…``) becomes a plain filename.

    :param entry_id: the entry's id, or None for a linear log
    :param revision: the document revision the snapshot holds
    :return: the path, relative to the package root
    """
    if entry_id:
        safe = re.sub(r"[^A-Za-z0-9._-]", "_", entry_id)
        return f"snapshots/entry-{safe}.json"
    return f"snapshots/rev-{revision}.json"


def compact_package(package_json: dict, head_revision: int) -> dict:
    """The package.json of a compaction: the same metadata with
    ``base_revision`` moved to the head the new base holds — the update
    the spec's compaction procedure demands of the producer. A new
    dict; the original is left as it was.

    :param package_json: the parsed package.json
    :param head_revision: the revision the compacted manifest holds
    :return: the new package.json dict
    """
    updated = copy.deepcopy(package_json)
    updated["base_revision"] = head_revision
    return updated


def read_package(world_dir) -> tuple:
    """Read a package the head-first way (spec/package.md): the base the
    log folds from, the log, the head and ``package.json``.

    The base is ``snapshots/base.json``; a package whose log holds no
    edits may leave it out, and its base is then its head.

    :param world_dir: the package directory
    :return: ``(base, entries, head, package)`` — ``package`` is None
        when there is no ``package.json``
    :raises WorldFormatError: when the log holds edits but the package
        has no base to fold them from
    """
    world = Path(world_dir)
    head = parse_manifest((world / "manifest.json").read_text())
    log = world / "ops.jsonl"
    entries = [
        parse_log_line(line) for line in log.read_text().splitlines() if line.strip()
    ] if log.exists() else []
    base_path = world / BASE_SNAPSHOT
    if base_path.exists():
        base = parse_manifest(base_path.read_text())
    elif any(edit_ops(e) for e in entries):
        raise WorldFormatError(
            f"{world.name}: the log holds edits but there is no {BASE_SNAPSHOT} to fold them from"
        )
    else:
        base = head
    package_path = world / "package.json"
    package = json.loads(package_path.read_text()) if package_path.exists() else None
    return base, entries, head, package


def compact(world_dir, head_revision: int | None = None) -> Path:
    """Compact a head-first package (spec/session.md "Snapshots"): move
    the base up to the head — or to the given revision — archive the
    entries the new base already holds as ``ops.archive.jsonl``, and keep
    the rest in ``ops.jsonl``. ``manifest.json`` (the head) is untouched:
    compaction changes nothing observable about the current state, it
    truncates structural replay and nothing else.

    :param world_dir: the package directory
    :param head_revision: the revision the new base holds; None for the
        log's own head
    :return: the package directory, compacted
    :raises WorldFormatError: as :func:`fold_log` — moving the base to a
        revision is a fold, and it must succeed
    """
    world = Path(world_dir)
    base, entries, _head, package = read_package(world)
    log_path = world / "ops.jsonl"
    lines = [line for line in log_path.read_text().splitlines() if line.strip()] \
        if log_path.exists() else []

    if head_revision is None:
        head_revision = max(
            (e["revision"] for e in entries),
            default=_coalesce(_coalesce(package, {}).get("base_revision"), 0),
        )
        new_base = (world / "manifest.json").read_text()
        archived, kept = lines, []
    else:
        cut = [i for i, e in enumerate(entries) if e["revision"] <= head_revision]
        upto = (cut[-1] + 1) if cut else 0
        state = fold_log(base, entries[:upto])
        new_base = json.dumps(to_manifest(state), indent=2) + "\n"
        archived, kept = lines[:upto], lines[upto:]

    (world / "snapshots").mkdir(exist_ok=True)
    (world / BASE_SNAPSHOT).write_text(new_base)

    if package is None:
        package = {"format_version": SUPPORTED_FORMAT_VERSION}
    package_path = world / "package.json"
    package_path.write_text(
        json.dumps(compact_package(package, head_revision), indent=2) + "\n"
    )

    if archived:
        (world / "ops.archive.jsonl").write_text("\n".join(archived) + "\n")
    log_path.write_text("".join(line + "\n" for line in kept))
    return world
