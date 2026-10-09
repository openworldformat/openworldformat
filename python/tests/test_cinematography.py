"""The ext-cinematography reference, in Python — mirrors js/test/cinematography.test.mjs."""

import json
import unittest
from pathlib import Path

from openworldformat import parse_manifest
from openworldformat.cinematography import (
    DEFAULT_SENSOR,
    EXTENSION_NAME,
    EXTENSION_VERSION,
    camera_of,
    frame_of,
    project,
    run_outcomes,
    shot_list,
    view_of,
)

ROOT = Path(__file__).resolve().parents[2]


def cinema_world():
    return parse_manifest((ROOT / "conformance" / "cinematography.json").read_text())


def by_name(manifest, name):
    return next(e for e in manifest["entities"] if e["name"] == name)


class CameraTest(unittest.TestCase):
    def test_camera_of_applies_the_defaults_per_absent_field_and_none_for_non_cameras(self):
        manifest = cinema_world()
        self.assertIsNone(camera_of(by_name(manifest, "maya")))
        bare = camera_of({"id": 1, "name": "c", EXTENSION_NAME: {"camera": {}}})
        self.assertEqual(bare["sensor_mm"], list(DEFAULT_SENSOR))
        self.assertEqual(bare["focal_length_mm"], 35)
        self.assertEqual(bare["squeeze"], 1)
        self.assertIsNone(bare["aspect_ratio"])
        # The extension version the module implements.
        self.assertEqual(EXTENSION_VERSION, "0.2.0")


class FrameTest(unittest.TestCase):
    def test_frame_of_is_the_normative_crop_math_cropping_trims_never_widens(self):
        manifest = cinema_world()
        # Super 35 with no aspect is the whole sensor.
        full = frame_of(camera_of(by_name(manifest, "2A")))
        self.assertLess(abs(full["aspect"] - 24.89 / 18.66), 1e-9)
        # A 2.39 crop on Super 35 trims the height only.
        cropped = frame_of(camera_of(by_name(manifest, "1A")))
        self.assertEqual(cropped["width_mm"], 24.89)
        self.assertLess(cropped["height_mm"], 18.66)
        self.assertLess(abs(cropped["aspect"] - 2.39), 1e-9)
        # The 2× anamorphic desqueezes, then crops the width.
        ana = frame_of(camera_of(by_name(manifest, "3A")))
        self.assertLess(ana["width_mm"], 24.89 * 2)
        self.assertEqual(ana["height_mm"], 18.66)
        self.assertLess(abs(ana["aspect"] - 2.39), 1e-9)


class ViewTest(unittest.TestCase):
    def test_view_of_looks_at_the_aim_with_plus_y_up_else_down_the_local_minus_z(self):
        manifest = cinema_world()
        wide = view_of(by_name(manifest, "1A"), camera_of(by_name(manifest, "1A")))
        self.assertEqual(wide["position"], [0, 1.6, 6])
        # Aiming at the origin from +Z looks down −Z.
        fx, fy, fz = wide["forward"]
        self.assertLess(abs(fx), 1e-9)
        self.assertLess(fz, 0)
        self.assertLess(fy, 0)  # tilted slightly down
        self.assertGreater(abs(wide["up"][1]), 0.9)  # +Y up

        # No aim: local −Z carried by the entity's rotation. A camera
        # yawed 90° right (intrinsic XYZ) looks down −X.
        turned_entity = {
            "id": 9, "name": "t",
            "transform": {"position": [0, 0, 0], "rotation_degrees": [0, 90, 0]},
        }
        turned = view_of(turned_entity, camera_of({"id": 9, "name": "t", EXTENSION_NAME: {"camera": {}}}))
        tx, ty, tz = turned["forward"]
        self.assertLess(abs(tx + 1), 1e-9)
        self.assertLess(abs(ty), 1e-9)
        self.assertLess(abs(tz), 1e-9)


class ProjectTest(unittest.TestCase):
    def test_project_puts_the_aim_point_at_the_frames_center(self):
        manifest = cinema_world()
        camera = camera_of(by_name(manifest, "2A"))
        view = view_of(by_name(manifest, "2A"), camera)
        frame = frame_of(camera)
        p = project(view, frame, camera, [-1.5, 1.2, 0])
        self.assertLess(abs(p["x"]), 1e-9)
        self.assertLess(abs(p["y"]), 1e-9)
        self.assertGreater(p["z"], 0)


class ShotListTest(unittest.TestCase):
    def test_shot_list_orders_by_shot_order_ties_by_entity_id_and_skips_shot_less_cameras(self):
        names = [s["name"] for s in shot_list(cinema_world())]
        self.assertEqual(names, ["1A", "2A", "2B", "3A"])  # bts carries no shot
        # Absent orders sort last; ties break by id.
        world = {"entities": [
            {"id": 7, "name": "b", EXTENSION_NAME: {"shot": {}}},
            {"id": 3, "name": "a", EXTENSION_NAME: {"shot": {}}},
            {"id": 5, "name": "c", EXTENSION_NAME: {"shot": {"order": 1}}},
        ]}
        self.assertEqual([s["name"] for s in shot_list(world)], ["c", "a", "b"])


class ConformanceTest(unittest.TestCase):
    def test_the_cinematography_conformance_outcomes_pass_under_the_reference_math(self):
        outcomes = json.loads((ROOT / "conformance" / "outcomes" / "cinematography.json").read_text())
        result = run_outcomes(cinema_world(), outcomes)
        self.assertEqual(result["failures"], [])
        self.assertTrue(result["ok"])


if __name__ == "__main__":
    unittest.main()
