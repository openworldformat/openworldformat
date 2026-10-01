"""Dataset tooling — the corpus of example recordings, as rows."""

import unittest
from pathlib import Path

from openworldformat.dataset import main, summarize

ROOT = Path(__file__).resolve().parents[2]


class DatasetTest(unittest.TestCase):
    def setUp(self):
        self.examples = ROOT / "examples"
        self.rows = {
            d.name: summarize(d)
            for d in sorted(self.examples.iterdir())
            if (d / "manifest.json").exists()
        }

    def test_every_example_package_is_a_row(self):
        self.assertEqual(sorted(self.rows),
                         ["forked-exploration", "hello-world", "speedrun-fork", "the-drop-test"])

    def test_the_rows_measure_what_the_folds_pinned(self):
        self.assertEqual(self.rows["hello-world"]["entities"], 14)  # base + the lantern
        self.assertEqual(self.rows["hello-world"]["edits"], 1)
        self.assertEqual(self.rows["the-drop-test"]["edits"], 1)
        self.assertEqual(self.rows["speedrun-fork"]["tips"], 2)  # two runs, one course
        self.assertEqual(self.rows["forked-exploration"]["tips"], 2)  # trunk and variant
        for row in self.rows.values():
            self.assertGreaterEqual(row["ops"], row["edits"])  # history rides along
            self.assertGreaterEqual(row["span_s"], 0)

    def test_the_cli_walks_a_tree_for_recordings(self):
        self.assertEqual(main([str(ROOT)]), 0)
        self.assertEqual(main([str(self.examples / "nope")]), 1)  # nothing to fold


if __name__ == "__main__":
    unittest.main()
