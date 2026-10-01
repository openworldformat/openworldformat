"""The task harness — the demo suite, and the predicates' teeth."""

import json
import unittest
from pathlib import Path

from openworldformat import WorldFormatError, parse_manifest
from openworldformat.eval import load_task_file, main, run_task

ROOT = Path(__file__).resolve().parents[2]
TASKS = ROOT / "python" / "examples" / "tasks"
HELLO = ROOT / "examples" / "hello-world"


class SuiteTest(unittest.TestCase):
    """The repo's four example packages, as a benchmark dataset: every
    task's own log is its solution."""

    def setUp(self):
        self.manifest = parse_manifest((HELLO / "manifest.json").read_text())
        self.state_doc = json.loads((HELLO / "state.json").read_text())

    def test_every_demo_task_passes_with_its_own_log(self):
        tasks = sorted(TASKS.glob("*.json"))
        self.assertEqual(len(tasks), 4)
        for path in tasks:
            with self.subTest(task=path.stem):
                manifest, entries, task, state_doc = load_task_file(path)
                result = run_task(manifest, entries, task, state_doc)
                self.assertEqual(result["failures"], [], f"{path.stem}: {result['failures']}")
                self.assertTrue(result["ok"])

    def test_the_cli_scores_the_suite_and_exits_zero(self):
        self.assertEqual(main([str(TASKS)]), 0)

    def test_metrics_report_the_trajectory_shape(self):
        path = TASKS / "speedrun-fork-noor.task.json"
        manifest, entries, task, state_doc = load_task_file(path)
        result = run_task(manifest, entries, task, state_doc)
        m = result["metrics"]
        # The course head plus noor's run: banner, flag, then her entry.
        self.assertEqual(m["entries"], 3)
        self.assertEqual(m["edits"], 2)
        self.assertEqual(m["ops"], 9)
        self.assertEqual(m["authors"], ["maya", "noor"])  # kai never happened here
        self.assertEqual(m["revision"], 2)

    def test_an_empty_log_fails_the_task_not_the_runner(self):
        # A world awaiting an agent: no ops.jsonl to load, nothing folded.
        task = {"goal": [{"exists": {"entity": "lantern"}}]}
        result = run_task(self.manifest, [], task, self.state_doc)
        self.assertFalse(result["ok"])
        self.assertEqual(result["failures"], ["entity 'lantern' does not exist"])


class PredicateTest(unittest.TestCase):
    def setUp(self):
        self.manifest = parse_manifest((HELLO / "manifest.json").read_text())
        self.state_doc = json.loads((HELLO / "state.json").read_text())

    def run_goal(self, goal, budget=None):
        return run_task(self.manifest, [], {"goal": goal, "budget": budget}, self.state_doc)

    def test_exists_and_gone(self):
        self.assertEqual(self.run_goal([{"exists": {"entity": "ground"}}])["failures"], [])
        failures = self.run_goal([{"gone": {"entity": "ground"}}])["failures"]
        self.assertEqual(failures, ["entity 'ground' still exists"])

    def test_near_without_a_position_or_an_entity(self):
        # A world whose only entity never declared a transform.
        bare = {"meta": {"name": "t"}, "entities": [{"id": 1, "name": "ghost"}]}
        result = run_task(bare, [], {"goal": [
            {"near": {"entity": "ghost", "position": [0, 0, 0]}},
        ]}, None)
        self.assertEqual(result["failures"], ["entity 'ghost' has no position"])
        failures = self.run_goal([{"near": {"entity": "dragon", "position": [0, 0, 0]}}])["failures"]
        self.assertEqual(failures, ["entity 'dragon' does not exist"])

    def test_field_comparators(self):
        self.assertEqual(self.run_goal([{"field": {"name": "score.tour", "equals": 0}}])["failures"], [])
        failures = self.run_goal([{"field": {"name": "score.tour", "equals": 5}}])["failures"]
        self.assertEqual(failures, ["field 'score.tour' is 0, expected 5"])
        failures = self.run_goal([{"field": {"name": "nope", "equals": 1}}])["failures"]
        self.assertEqual(failures, ["field 'nope' has no value"])
        failures = self.run_goal([{"field": {"name": "score.tour", "at_least": 2}}])["failures"]
        self.assertEqual(failures, ["field 'score.tour' is 0, expected at least 2"])

    def test_unknown_predicates_and_missing_goals_refuse(self):
        failures = self.run_goal([{"maybe": True}])["failures"]
        self.assertEqual(len(failures), 1)
        self.assertIn("unknown predicate", failures[0])
        with self.assertRaises(WorldFormatError):
            run_task(self.manifest, [], {"goal": []}, self.state_doc)

    def test_budgets_are_the_cost_side_of_a_task(self):
        # Limits are inclusive: an empty log respects even a zero budget.
        failures = self.run_goal([{"exists": {"entity": "ground"}}], budget={"max_edits": 0})["failures"]
        self.assertEqual(failures, [])
        failures = self.run_goal([{"exists": {"entity": "ground"}}], budget={"max_entries": -1})["failures"]
        self.assertEqual(failures, ["budget exceeded: 0 entries, max -1"])

    def test_scoring_an_agent_in_process(self):
        # The API's point: score ops the moment an agent emits them.
        from openworldformat import parse_log_line

        entries = [parse_log_line(json.dumps({
            "revision": 6, "author": {"name": "agent"}, "timestamp_ms": 1,
            "ops": [{"SpawnEntity": {"entity": {
                "id": 500, "name": "lantern", "transform": {"position": [-12.0, 0.0, 3.0]},
            }}}],
        }))]
        task = {"goal": [{"exists": {"entity": "lantern"}},
                         {"near": {"entity": "lantern", "position": [-12.0, 0.0, 3.0]}}],
                "budget": {"max_edits": 1}}
        result = run_task(self.manifest, entries, task, self.state_doc)
        self.assertTrue(result["ok"], result["failures"])
        self.assertEqual(result["metrics"]["authors"], ["agent"])


if __name__ == "__main__":
    unittest.main()
