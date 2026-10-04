"""The example packages, folded and replayed — mirrors js/test/example.test.mjs."""

import json
import math
import unittest
from pathlib import Path

from openworldformat import (
    build_history,
    classify_op,
    fold_log,
    fold_path,
    fold_state,
    parse_log_line,
    parse_manifest,
)
from openworldformat.physics import fold_trajectories, simulate_physics

ROOT = Path(__file__).resolve().parents[2]


def read_entries(directory):
    return [
        parse_log_line(line)
        for line in (directory / "ops.jsonl").read_text().splitlines() if line.strip()
    ]


DROP = ROOT / "examples" / "the-drop-test"
manifest_text = (DROP / "snapshots" / "base.json").read_text()
entries = read_entries(DROP)


class DropTest(unittest.TestCase):
    def test_the_drop_test_package_folds_one_edit_everything_else_is_history(self):
        base = parse_manifest(manifest_text)
        self.assertEqual(len(base["entities"]), 6)
        state = fold_log(base, entries)
        self.assertEqual(state["applied_edits"], 1)
        self.assertTrue(any(e["name"] == "ball_late" for e in state["entities"]))
        # The recorded run folds to nothing for the document…
        self.assertEqual(len(state["entities"]), 7)

    def test_the_switch_score_crossed_the_log_as_a_click_would(self):
        state_doc = json.loads((DROP / "state.json").read_text())
        folded = fold_state(state_doc, entries)
        self.assertEqual(folded["values"]["score.switch"], 10)
        self.assertEqual(folded["undeclared"], [])

    def test_semantic_replay_resimulating_the_fold_reproduces_the_recorded_run(self):
        base = parse_manifest(manifest_text)
        folded = fold_log(base, entries)
        sim = simulate_physics(folded, {"until_s": 6.0, "sample_dt_s": 0.1})

        # What the log carried…
        track = fold_trajectories(entries)
        self.assertEqual(sorted(track["bodies"]), ["ball", "ball_late", "bouncy", "feather"])
        self.assertEqual(track["span_s"], sim["samples"][-1]["t_s"])

        # …is what the solver says again: the outcomes agree (positions
        # within a hair — same engine, same algorithm), which is the
        # extension's replay contract in miniature.
        for name, samples in track["bodies"].items():
            recorded = samples[-1]["position"]
            resting = sim["resting"][name]
            drift = math.dist(recorded, resting)
            self.assertLess(drift, 0.001, f"{name} drifted {drift} between recording and replay")

        # The switch contact is in the re-simulation too.
        self.assertTrue(any(c["body"] == "ball" and c["other"] == "switch_pad" for c in sim["contacts"]))


class SpeedrunForkTest(unittest.TestCase):
    def test_a_challenge_chain_is_a_history_two_runs_fork_one_course(self):
        fork_dir = ROOT / "examples" / "speedrun-fork"
        fork_manifest = parse_manifest((fork_dir / "snapshots" / "base.json").read_text())
        fork_entries = read_entries(fork_dir)

        history = build_history(fork_entries)
        # Two tips — the two runs — and both are children of the course head.
        self.assertEqual(sorted(history["tips"]), ["e3", "e4"])
        self.assertEqual(sorted(history["children"]["e2"]), ["e3", "e4"])

        # The trunk is the course: banner and checkpoint flag, no runs folded in.
        trunk = fold_path(fork_manifest, fork_entries, "e2")
        trunk_names = {e["name"] for e in trunk["entities"]}
        self.assertIn("banner", trunk_names)
        self.assertIn("checkpoint_flag", trunk_names)

        # Each run folds the course plus its own inputs and its own time.
        runs = {"e3": ("run.kai", 9.42), "e4": ("run.noor", 7.91)}
        state_doc = json.loads((fork_dir / "state.json").read_text())
        by_id = {e["id"]: e for e in history["ordered"]}
        for tip, (field, time) in runs.items():
            run = fold_path(fork_manifest, fork_entries, tip)
            self.assertEqual(len(run["entities"]), len(trunk["entities"]))  # no edits in a run
            chain = [by_id[id_] for id_ in run["path"]]
            folded = fold_state(state_doc, chain)
            self.assertEqual(folded["values"][field], time)
            other = "run.noor" if field == "run.kai" else "run.kai"
            self.assertEqual(folded["values"][other], 0.0)  # the other run never happened here
            samples = [o for e in chain for o in e["ops"] if classify_op(o)["kind"] == "input"]
            self.assertEqual(len(samples), 5)  # the playthrough, recorded


if __name__ == "__main__":
    unittest.main()
