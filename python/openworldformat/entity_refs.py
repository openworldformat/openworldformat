"""The one list of entity-reference fields (spec/world.md "Identity",
schema/entity-refs.json) — the package's embedded copy. The canonical
list is generated from world.schema.json's ``x-entity-ref`` markers by
schema/generate-entity-refs.mjs, but a published package can't read the
repo's schema/ at runtime, so the passes read this table instead, and
tests/test_entity_refs.py fails when it drifts from the canonical file.

``kind: "bindable"`` — a name is accepted at intake and MUST bind to an
id at ingestion; ``kind: "id"`` — numeric only. ``scope`` is the object
the path walks from: an entity (or its patch, the same fields), the
manifest's avatar, or one creation in ``creations[]``. ``"*"`` walks
every array element.
"""

from __future__ import annotations

ENTITY_REFS = [
    {"scope": "entity", "path": ["behaviors", "*", "LookAt", "target"], "kind": "bindable"},
    {"scope": "entity", "path": ["behaviors", "*", "Orbit", "center"], "kind": "bindable"},
    {"scope": "entity", "path": ["parent"], "kind": "bindable"},
    {"scope": "avatar", "path": ["model_entity"], "kind": "bindable"},
    {"scope": "creation", "path": ["entities", "*"], "kind": "id"},
]
