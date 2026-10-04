"""The agent loop — the baseline solves, and its ops are a recording."""

import json
import unittest
from pathlib import Path

from openworldformat import parse_log_line, parse_manifest
from openworldformat.agent import TEMPLATE_NAME, run_agent, template_agent

ROOT = Path(__file__).resolve().parents[2]
HELLO = ROOT / "examples" / "hello-world"


class TemplateAgentTest(unittest.TestCase):
    def setUp(self):
        self.manifest = parse_manifest((HELLO / "snapshots" / "base.json").read_text())
        self.state_doc = json.loads((HELLO / "state.json").read_text())

    def test_the_template_solves_a_task_it_understands(self):
        task = {
            "goal": [
                {"exists": {"entity": "lantern"}},
                {"near": {"entity": "lantern", "position": [-12.0, 0.0, 3.0]}},
                {"field": {"name": "score.tour", "equals": 1}},
            ],
            "budget": {"max_edits": 1, "max_entries": 1},
        }
        result = run_agent(self.manifest, task, self.state_doc)
        self.assertTrue(result["ok"], result["failures"])
        self.assertEqual(len(result["entries"]), 1)  # one entry held everything
        m = result["metrics"]
        self.assertEqual(m["edits"], 1)   # the spawn; the state op is history
        self.assertEqual(m["ops"], 2)     # spawn + state
        self.assertEqual(m["authors"], [TEMPLATE_NAME])

    def test_moves_and_deletes_are_compiled_too(self):
        task = {
            "goal": [
                {"near": {"entity": "sphere", "position": [5.0, 0.0, 5.0]}},
                {"gone": {"entity": "torus"}},
            ],
            "budget": {"max_edits": 2},
        }
        result = run_agent(self.manifest, task, None)
        self.assertTrue(result["ok"], result["failures"])
        self.assertEqual(result["metrics"]["edits"], 2)  # one move, one delete
        names = {e["name"] for e in result["state"]["entities"]}
        self.assertNotIn("torus", names)
        sphere = next(e for e in result["state"]["entities"] if e["name"] == "sphere")
        self.assertEqual(sphere["transform"]["position"], [5.0, 0.0, 5.0])

    def test_an_empty_world_gives_the_agent_an_empty_log(self):
        result = run_agent(self.manifest, {"goal": [{"exists": {"entity": "ground"}}]}, None)
        self.assertTrue(result["ok"])
        self.assertEqual(result["entries"], [])  # nothing to do, nothing emitted

    def test_unknown_predicates_are_beyond_the_template_by_design(self):
        result = run_agent(self.manifest, {"goal": [{"maybe": True}]}, None)
        self.assertFalse(result["ok"])  # it can't solve what it can't read…
        self.assertEqual(len(result["entries"]), 0)  # …so it emits nothing

    def test_the_agents_ops_are_a_valid_recording(self):
        task = {"goal": [{"exists": {"entity": "lantern"}},
                         {"near": {"entity": "lantern", "position": [-12.0, 0.0, 3.0]}}]}
        result = run_agent(self.manifest, task, None)
        # Serialize the entries and parse them back: an agent run is a log.
        lines = [json.dumps(e) for e in result["entries"]]
        reparsed = [parse_log_line(line) for line in lines]
        self.assertEqual(len(reparsed), 1)
        self.assertEqual(reparsed[0]["ops"], result["entries"][0]["ops"])
        # And the template is importable as a skeleton to copy.
        self.assertTrue(callable(template_agent))


if __name__ == "__main__":
    unittest.main()
