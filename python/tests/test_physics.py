"""The ext-physics reference, in Python — mirrors js/test/physics.test.mjs."""

import json
import unittest
from pathlib import Path

from openworldformat import classify_op, fold_log, parse_log_line, parse_manifest
from openworldformat.physics import (
    EXTENSION_NAME,
    collect_physics,
    fold_trajectories,
    run_outcomes,
    simulate_physics,
    trajectory_op,
)

ROOT = Path(__file__).resolve().parents[2]


def entry_of(ops):
    return parse_log_line(json.dumps({
        "revision": 1, "author": {"name": "t"}, "ops": ops, "timestamp_ms": 0,
    }))


class FoldTest(unittest.TestCase):
    def test_extension_ops_classify_and_fold_to_nothing(self):
        self.assertEqual(
            classify_op({"ext-physics": {"t_s": [0], "bodies": {}}}),
            {"kind": "extension", "name": "ext-physics", "value": {"t_s": [0], "bodies": {}}},
        )
        # Unknown extensions are still extensions — the namespace is the channel.
        self.assertEqual(classify_op({"ext-avatars": {"a": 1}})["kind"], "extension")
        # Not a plain string key with non-object value, not an edit.
        self.assertEqual(classify_op({"ext-physics": "nope"})["kind"], "unknown")

        manifest = parse_manifest((ROOT / "examples" / "hello-world" / "manifest.json").read_text())
        before = len(fold_log(manifest, [])["entities"])
        state = fold_log(manifest, [
            entry_of([{"ext-physics": {"t_s": [0.0, 0.1], "bodies": {"ball": [[0, 5, 0], [0, 4.95, 0]]}}}]),
        ])
        self.assertEqual(len(state["entities"]), before)  # the document never moved
        self.assertEqual(state["applied_edits"], 0)

    def test_ext_fields_survive_a_modify_patch_null_clears_them(self):
        manifest = physics_world()
        state = fold_log(manifest, [
            entry_of([{"ModifyEntity": {"id": 4, "patch": {"ext-physics": {"body": "static"}}}}]),
        ])
        ball = next(e for e in state["entities"] if e["id"] == 4)
        self.assertEqual(ball[EXTENSION_NAME], {"body": "static"})

        cleared = fold_log(manifest, [
            entry_of([{"ModifyEntity": {"id": 4, "patch": {"ext-physics": None}}}]),
        ])
        self.assertNotIn(EXTENSION_NAME, next(e for e in cleared["entities"] if e["id"] == 4))


def physics_world():
    return parse_manifest((ROOT / "conformance" / "physics.json").read_text())


class SolverTest(unittest.TestCase):
    def test_bodies_are_collected_from_the_declaration_never_inferred(self):
        collected = collect_physics(physics_world())
        self.assertEqual(collected["gravity"], [0, -9.81, 0])
        self.assertEqual(sorted(b["name"] for b in collected["dynamic"]), ["ball", "bouncy_ball"])
        self.assertEqual(collected["kinematic"], [])
        self.assertEqual(sorted(s["name"] for s in collected["statics"]), ["ground", "pedestal"])
        # The pedestal is a box around its extents: top at y = 1.
        pedestal = next(s for s in collected["statics"] if s["name"] == "pedestal")
        self.assertEqual(pedestal["max"], [1, 1, 1])
        # hello-world declares no bodies: nothing participates.
        plain = collect_physics(parse_manifest((ROOT / "examples" / "hello-world" / "manifest.json").read_text()))
        self.assertEqual(plain, {"gravity": [0, -9.81, 0], "dynamic": [], "kinematic": [], "statics": []})

    def test_the_simulation_is_deterministic_two_runs_are_identical(self):
        a = simulate_physics(physics_world(), {"until_s": 6})
        b = simulate_physics(physics_world(), {"until_s": 6})
        self.assertEqual(a, b)

    def test_a_dropped_ball_finds_the_pedestal_rests_on_it_and_the_world_settles(self):
        sim = simulate_physics(physics_world(), {"until_s": 6})
        first = next((c for c in sim["contacts"] if c["body"] == "ball" and c["other"] == "pedestal"), None)
        self.assertIsNotNone(first, "ball never touched the pedestal")
        self.assertLess(first["t_s"], 2.0, f"contact at {first['t_s']}s, later than expected")
        # Pedestal top is y = 1, ball radius 0.3: it rests at [0, 1.3, 0].
        self.assertLess(abs(sim["resting"]["ball"][1] - 1.3), 0.05, str(sim["resting"]["ball"]))
        self.assertEqual(sim["resting"]["ball"][:1], [0])  # no horizontal drift
        self.assertGreaterEqual(len([x for x in sim["bounces"] if x["body"] == "bouncy_ball"]), 4)
        self.assertLess(sim["settled_s"], 6, f"world settled at {sim['settled_s']}s")


class TrajectoryTest(unittest.TestCase):
    def test_a_trajectory_op_round_trips_simulate_write_fold_scrub(self):
        sim = simulate_physics(physics_world(), {"until_s": 2, "sample_dt_s": 0.25})
        op = trajectory_op(sim)
        self.assertEqual(len(op[EXTENSION_NAME]["t_s"]), len(sim["samples"]))

        entries = [entry_of([op])]
        # The op folds to nothing for the document…
        state = fold_log(physics_world(), entries)
        self.assertEqual(state["applied_edits"], 0)
        # …and folds to samples for playback.
        track = fold_trajectories(entries)
        self.assertEqual(sorted(track["bodies"]), ["ball", "bouncy_ball"])
        self.assertEqual(len(track["bodies"]["ball"]), len(sim["samples"]))
        self.assertEqual(track["bodies"]["ball"][0]["position"], [0, 5, 0])  # spawn
        self.assertGreater(track["span_s"], 0)
        # The fold tolerates a reader that never classified (raw entries).
        raw = [json.loads(json.dumps({"revision": 1, "author": {"name": "t"}, "ops": [op], "timestamp_ms": 0}))]
        self.assertEqual(len(fold_trajectories(raw)["bodies"]["ball"]), len(sim["samples"]))


class ConformanceTest(unittest.TestCase):
    def test_the_physics_conformance_outcomes_pass_under_the_reference_solver(self):
        outcomes = json.loads((ROOT / "conformance" / "outcomes" / "physics.json").read_text())
        result = run_outcomes(physics_world(), outcomes)
        self.assertEqual(result["failures"], [])
        self.assertTrue(result["ok"])


if __name__ == "__main__":
    unittest.main()
