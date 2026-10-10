"""The one list of entity-reference fields (spec/world.md "Identity",
schema/entity-refs.json): the python reference embeds a copy
(openworldformat/entity_refs.py) and this test fails when the copy
drifts from the canonical file — and pins that the passes actually
walk it. Mirrors js/test/entity-refs.test.mjs."""

import json
import unittest
from pathlib import Path

from openworldformat import (
    ENTITY_REFS,
    WorldFormatError,
    fold_log,
    ingest,
    merge_branch,
    parse_manifest,
)

ROOT = Path(__file__).resolve().parents[2]
CANONICAL = json.loads((ROOT / "schema" / "entity-refs.json").read_text())


class EntityRefsTest(unittest.TestCase):
    def test_the_embedded_list_is_the_canonical_list(self):
        self.assertEqual(ENTITY_REFS, CANONICAL["refs"])

    def test_the_canonical_list_holds_every_field_the_merge_table_rewrites(self):
        paths = [f"{ref['scope']}:{'/'.join(ref['path'])}" for ref in ENTITY_REFS]
        for expected in (
            "entity:parent",
            "entity:behaviors/*/Orbit/center",
            "entity:behaviors/*/LookAt/target",
            "avatar:model_entity",
            "creation:entities/*",
        ):
            self.assertIn(expected, paths)

    def test_name_binding_walks_the_list_a_string_parent_binds_at_intake(self):
        manifest = {
            "version": 3,
            "meta": {"name": "t"},
            "entities": [{"id": 1, "name": "sun"}],
        }
        state = fold_log(manifest, [])
        result = ingest(state, [
            {"SpawnEntity": {"entity": {"id": 2, "name": "planet", "parent": "sun"}}},
            {"SpawnEntity": {"entity": {"name": "moon", "parent": "planet"}}},
        ])
        self.assertTrue(result["ok"])
        self.assertEqual(result["ops"][0]["SpawnEntity"]["entity"]["parent"], 1)
        self.assertEqual(result["ops"][1]["SpawnEntity"]["entity"]["parent"], 2)
        # A raw log's string parent is not a committed form: the fold
        # refuses it at apply (committed ops carry ids).
        with self.assertRaisesRegex(WorldFormatError, "parent"):
            fold_log(manifest, [{"revision": 1, "ops": [
                {"SpawnEntity": {"entity": {"id": 4, "name": "x", "parent": "sun"}}},
            ]}])

    def test_ingest_binds_the_avatars_marked_refs_model_entity_by_name(self):
        manifest = {
            "version": 3,
            "meta": {"name": "t"},
            "entities": [{"id": 1, "name": "hero"}],
        }
        state = fold_log(manifest, [])
        result = ingest(state, [
            {"ModifyWorld": {"patch": {"avatar": {"model_entity": "hero"}}}},
        ])
        self.assertTrue(result["ok"])
        self.assertEqual(
            result["ops"][0]["ModifyWorld"]["patch"]["avatar"]["model_entity"], 1
        )
        self.assertEqual(result["state"]["scene"]["avatar"]["model_entity"], 1)

    def test_merge_rewriting_walks_the_list_for_every_scope(self):
        manifest = {"version": 3, "meta": {"name": "t"}, "entities": []}
        main = [{"revision": 1, "ops": [
            {"SpawnEntity": {"entity": {"id": 1, "name": "main-one"}}},
        ]}]
        state = fold_log(manifest, main)
        branch = [{"revision": 2, "ops": [
            {"SpawnEntity": {"entity": {
                "id": 1, "name": "branch-one", "parent": 1,
                "behaviors": [
                    {"Orbit": {"center": 1, "radius": 2, "speed": 10}},
                    {"LookAt": {"target": 1}},
                ],
            }}},
            {"ModifyWorld": {"patch": {
                "avatar": {"model_entity": 1},
                "creations": [{"id": 1, "name": "c", "entities": [1]}],
            }}},
        ]}]
        merged = merge_branch(state, branch)
        self.assertEqual(merged["remapped"], {1: 2})
        spawn, world = merged["entries"][0]["ops"]
        self.assertEqual(spawn["SpawnEntity"]["entity"]["parent"], 2)
        self.assertEqual(
            spawn["SpawnEntity"]["entity"]["behaviors"][0]["Orbit"]["center"], 2
        )
        self.assertEqual(
            spawn["SpawnEntity"]["entity"]["behaviors"][1]["LookAt"]["target"], 2
        )
        self.assertEqual(world["ModifyWorld"]["patch"]["avatar"]["model_entity"], 2)
        self.assertEqual(
            world["ModifyWorld"]["patch"]["creations"][0]["entities"], [2]
        )

    def test_strict_readers_admit_marked_worlds(self):
        # ext-cinematography is registered (the registry rule).
        text = (ROOT / "conformance" / "cinematography.json").read_text()
        parse_manifest(text, strict=True)


if __name__ == "__main__":
    unittest.main()
