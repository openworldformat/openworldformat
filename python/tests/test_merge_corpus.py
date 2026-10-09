"""The shared merge corpus (conformance/merge/, spec/session.md "The
merge rules, exactly") — mirrors js/test/merge-corpus.test.mjs: every
case runs — remap table, rewritten entries and merged head all compared
against the committed expected results. The other four references run
the same cases in their own suites, so the five merges cannot drift
apart."""

import copy
import json
import unittest
from pathlib import Path

from openworldformat import fold_log, manifest_text, merge_branch, to_manifest

ROOT = Path(__file__).resolve().parents[2]
CASE_DIR = ROOT / "conformance" / "merge" / "cases"
CASES = [json.loads(p.read_text()) for p in sorted(CASE_DIR.glob("*.json"))]


class MergeCorpusTest(unittest.TestCase):
    def test_the_corpus_is_present_and_covers_the_hand_written_rules(self):
        names = {case["name"] for case in CASES}
        for required in ("spent-id", "modify-world", "batch", "names"):
            self.assertIn(required, names)
        self.assertGreaterEqual(len(CASES), 100)  # the generated cases too

    def test_every_case_merges_as_committed(self):
        for case in CASES:
            with self.subTest(case=case["name"]):
                state = fold_log(
                    copy.deepcopy(case["base"]), copy.deepcopy(case["main"])
                )
                merged = merge_branch(state, copy.deepcopy(case["branch"]))

                self.assertEqual(
                    [list(pair) for pair in merged["remapped"].items()],
                    case["expected"]["remapped"],
                    "the remap table",
                )
                self.assertEqual(
                    merged["entries"],
                    case["expected"]["entries"],
                    "the rewritten entries",
                )

                # Snapshot the merged entries before the fold sees them:
                # the fold holds references into the ops it applies (a
                # later ModifyEntity's patch lands on the entity object),
                # so folding what was just compared could mutate it.
                replay = copy.deepcopy(merged["entries"])
                head = fold_log(
                    copy.deepcopy(case["base"]),
                    [*copy.deepcopy(case["main"]), *replay],
                )
                self.assertEqual(
                    manifest_text(to_manifest(head)),
                    case["expected"]["head"],
                    "the merged head, as canonical text",
                )


if __name__ == "__main__":
    unittest.main()
