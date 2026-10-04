"""The fold, over the format's own examples — mirrors js/test/fold.test.mjs."""

import json
import unittest
from pathlib import Path

from openworldformat import (
    SUPPORTED_SCHEMA_VERSION,
    build_history,
    classify_op,
    edit_ops,
    fold_path,
    fold_state,
    fold_log,
    parse_log_line,
    parse_manifest,
)

ROOT = Path(__file__).resolve().parents[2]
HELLO = ROOT / "examples" / "hello-world"
manifest_text = (HELLO / "snapshots" / "base.json").read_text()
log_text = (HELLO / "ops.jsonl").read_text()
entries = [parse_log_line(line) for line in log_text.splitlines() if line.strip()]


def entry_of(ops, revision=1, timestamp_ms=0):
    return parse_log_line(json.dumps({
        "revision": revision, "author": {"name": "t"}, "ops": ops,
        "timestamp_ms": timestamp_ms,
    }))


class ManifestTest(unittest.TestCase):
    def test_the_example_manifest_parses_at_the_supported_schema_version(self):
        manifest = parse_manifest(manifest_text)
        self.assertEqual(manifest["version"], SUPPORTED_SCHEMA_VERSION)
        self.assertGreater(len(manifest["entities"]), 0)

    def test_a_newer_manifest_is_refused_loudly_per_the_versioning_policy(self):
        newer = json.dumps({**json.loads(manifest_text), "version": SUPPORTED_SCHEMA_VERSION + 1})
        with self.assertRaisesRegex(Exception, "newer than this reader"):
            parse_manifest(newer)


class ClassifyTest(unittest.TestCase):
    def test_ops_are_recognized_by_shape_edits_first(self):
        self.assertEqual(classify_op({"SpawnEntity": {"entity": {"id": 1, "name": "a"}}})["kind"], "edit")
        self.assertEqual(classify_op({"SpawnEntity": {"entity": {"id": 1, "name": "a"}}})["edit"], "SpawnEntity")
        self.assertEqual(classify_op({"tool": "x", "args": {}})["kind"], "tool")
        self.assertEqual(classify_op({"input": {"actor": "v"}})["kind"], "input")
        self.assertEqual(classify_op({"state": {"score.x": 1}})["kind"], "state")
        self.assertEqual(classify_op({"clock": {"playing": True, "position_s": 0}})["kind"], "clock")
        self.assertEqual(classify_op({"nope": 1})["kind"], "unknown")

    def test_an_old_format_line_edits_only_parses_as_edits(self):
        line = json.dumps({
            "revision": 7,
            "author": {"peer": 3, "name": "maya"},
            "ops": [{"DeleteEntity": {"id": 1}}],
            "timestamp_ms": 1,
        })
        entry = parse_log_line(line)
        edits = edit_ops(entry)
        self.assertEqual(len(edits), 1)
        self.assertEqual(edits[0]["edit"], "DeleteEntity")


class FoldTest(unittest.TestCase):
    def test_the_example_log_folds_the_lantern_appears_history_folds_to_nothing(self):
        base = parse_manifest(manifest_text)
        before = len(base["entities"])
        state = fold_log(base, entries)
        # Five entries, one edit op among them: everything else is history.
        self.assertEqual(state["applied_edits"], 1)
        self.assertEqual(len(state["entities"]), before + 1)
        lantern = next(e for e in state["entities"] if e["id"] == 100)
        self.assertEqual(lantern["name"], "lantern")
        self.assertEqual(lantern["transform"]["position"], [-12.0, 0.0, 3.0])
        # History entries carried the revision without bumping anything:
        self.assertTrue(all(e["id"] != 101 for e in state["entities"]))

    def test_modify_applies_a_patch_absent_fields_are_unchanged_null_clears(self):
        base = parse_manifest(manifest_text)
        state = fold_log(base, [entry_of([
            {"ModifyEntity": {"id": 1, "patch": {
                "shape": {"Sphere": {"radius": 0.5}}, "material": None,
            }}},
        ])])
        ground = next(e for e in state["entities"] if e["id"] == 1)
        self.assertEqual(ground["shape"], {"Sphere": {"radius": 0.5}})
        self.assertNotIn("material", ground)
        self.assertEqual(ground["transform"]["position"], [0.0, 0.0, 0.0])  # untouched

    def test_deleting_an_entity_deletes_its_descendants(self):
        base = parse_manifest(manifest_text)
        state = fold_log(base, [
            entry_of([
                {"SpawnEntity": {"entity": {"id": 200, "name": "p"}}},
                {"SpawnEntity": {"entity": {"id": 201, "name": "c1", "parent": 200}}},
                {"SpawnEntity": {"entity": {"id": 202, "name": "c2", "parent": 201}}},
            ]),
            entry_of([{"DeleteEntity": {"id": 200}}], revision=2, timestamp_ms=1),
        ])
        ids = {e["id"] for e in state["entities"]}
        self.assertNotIn(200, ids)
        self.assertNotIn(201, ids)
        self.assertNotIn(202, ids)

    def test_a_batch_applies_all_or_nothing(self):
        base = parse_manifest(manifest_text)
        batch = entry_of([
            {"Batch": {"ops": [
                {"SpawnEntity": {"entity": {"id": 300, "name": "ok"}}},
                {"DeleteEntity": {"id": 99999}},  # refuses
            ]}},
        ])
        with self.assertRaisesRegex(Exception, "no entity 99999"):
            fold_log(base, [batch])

    def test_state_folds_over_the_declaration_tolerating_the_undeclared(self):
        state_doc = json.loads((HELLO / "state.json").read_text())
        result = fold_state(state_doc, entries)
        # The example's log sets score.tour to 1; the declaration's initial was 0.
        self.assertEqual(result["values"]["score.tour"], 1)
        self.assertEqual(result["undeclared"], [])

        richer = {
            "format_version": 1,
            "fields": {
                "score.main": {"type": "int", "initial": 0},
                "inventory": {"type": "map", "initial": {}},
                "has.map": {"type": "bool", "initial": False},
            },
        }
        ops = [
            {"state": {"score.main": 5}},
            {"state": {"inventory.rope": 1, "inventory.torch": 2}},
            {"state": {"inventory.rope": None}},
            {"state": {"has.map": True}},
            {"state": {"has.map": None}},
            {"state": {"unknown.key": 7}},
            {"state": {"unknown.key": None}},
        ]
        entries_ = [entry_of([op], timestamp_ms=i) for i, op in enumerate(ops)]
        folded = fold_state(richer, entries_)
        self.assertEqual(folded["values"]["score.main"], 5)
        self.assertEqual(folded["values"]["inventory"], {"torch": 2})
        self.assertIs(folded["values"]["has.map"], False)  # null reset the initial
        self.assertNotIn("unknown.key", folded["values"])  # set, carried, then removed
        self.assertEqual(folded["undeclared"], ["unknown.key"])

    def test_a_forked_history_folds_per_tip_same_prefix_different_worlds(self):
        forked = ROOT / "examples" / "forked-exploration"
        manifest = parse_manifest((forked / "snapshots" / "base.json").read_text())
        entries_ = [
            parse_log_line(line)
            for line in (forked / "ops.jsonl").read_text().splitlines() if line.strip()
        ]

        history = build_history(entries_)
        # Two tips: the trunk's garden end, and the moat variant.
        self.assertEqual(sorted(history["tips"]), ["e3", "e5"])
        # The fork point has both children.
        self.assertEqual(sorted(history["children"]["e2"]), ["e3", "e4"])

        trunk = fold_path(manifest, entries_, "e3")
        names = {e["name"] for e in trunk["entities"]}
        self.assertIn("garden", names)
        self.assertNotIn("moat", names)
        self.assertEqual(trunk["path"], ["e1", "e2", "e3"])

        variant = fold_path(manifest, entries_, "e5")
        names = {e["name"] for e in variant["entities"]}
        self.assertIn("moat", names)
        self.assertNotIn("garden", names)
        self.assertEqual(variant["path"], ["e1", "e2", "e4", "e5"])

        # Default tip is the last entry in file order; the merge record folds
        # to nothing, so the variant's document is unchanged by it.
        self.assertEqual(
            len(variant["entities"]),
            len(fold_path(manifest, entries_, "e4")["entities"]),
        )

        # Unknown tips refuse loudly.
        with self.assertRaisesRegex(Exception, "no entry 'e99'"):
            fold_path(manifest, entries_, "e99")

    def test_a_log_with_no_ids_is_a_chain_and_mixed_logs_work(self):
        manifest = parse_manifest(manifest_text)
        # The hello-world log has no ids: one tip, the last line.
        history = build_history(entries)
        self.assertEqual(history["tips"], [f"line-{len(entries) - 1}"])
        state = fold_path(manifest, entries)
        self.assertEqual(len(state["entities"]), len(manifest["entities"]) + 1)  # the lantern

        # Mixed: an id-bearing branch grafted onto a synthesized chain.
        chain = [
            {"revision": 1, "author": {"name": "t"},
             "ops": [{"SpawnEntity": {"entity": {"id": 900, "name": "a"}}}], "timestamp_ms": 0},
            {"revision": 2, "author": {"name": "t"},
             "ops": [{"SpawnEntity": {"entity": {"id": 901, "name": "b"}}}], "timestamp_ms": 1},
            {"id": "x", "parent": "line-0", "revision": 2, "author": {"name": "t"},
             "ops": [{"SpawnEntity": {"entity": {"id": 902, "name": "c"}}}], "timestamp_ms": 2},
        ]
        chain = [parse_log_line(json.dumps(e)) for e in chain]
        mixed = build_history(chain)
        self.assertEqual(sorted(mixed["tips"]), ["line-1", "x"])
        grafted = fold_path(manifest, chain, "x")["entities"]
        self.assertEqual(len([e for e in grafted if e["id"] == 902]), 1)

    def test_merge_ops_classify_and_fold_to_nothing(self):
        self.assertEqual(classify_op({"merge": {"branch": "moat-variant"}})["kind"], "merge")

    def test_a_torn_line_loses_at_most_itself(self):
        lines = [line for line in log_text.splitlines() if line.strip()]
        parsed = []
        torn = 0
        for line in lines:
            try:
                parsed.append(parse_log_line(line))
            except ValueError:  # json or structure — either way, the writer's crash
                torn += 1  # skip, count
        self.assertEqual(torn, 0)  # the example's log is whole
        # And a genuinely torn last line:
        with self.assertRaises(ValueError):
            parse_log_line(lines[0][:20])


if __name__ == "__main__":
    unittest.main()
