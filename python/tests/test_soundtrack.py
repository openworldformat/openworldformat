"""The soundtrack curves — pins the math against the conformance world."""

import json
import unittest
from pathlib import Path

from openworldformat.soundtrack import beat_at, curve_at, modulation_factor, section_at

ROOT = Path(__file__).resolve().parents[2]
WORLD = json.loads((ROOT / "conformance" / "soundtrack.json").read_text())
ST = WORLD["soundtrack"]  # 60 s at 120 bpm, first beat at 0.1, sections at 0/0.25/0.75


class CurveTest(unittest.TestCase):
    def test_energy_interpolates_per_second_and_holds_its_endpoints(self):
        energy = ST["energy"]
        self.assertEqual(curve_at(energy, 0), energy[0])
        self.assertAlmostEqual(curve_at(energy, 2.5), energy[2] + (energy[3] - energy[2]) * 0.5)
        self.assertEqual(curve_at(energy, -5), energy[0])  # before the curve
        self.assertEqual(curve_at(energy, 9999), energy[-1])  # past it
        self.assertEqual(curve_at(None, 3), 0)
        self.assertEqual(curve_at([0.7], 100), 0.7)  # a constant curve

    def test_beats_decay_to_zero_at_the_next_beat(self):
        # 120 bpm: period 0.5 s, first beat at the 0.1 s offset.
        self.assertAlmostEqual(beat_at(ST, 0.1), 1.0)
        self.assertAlmostEqual(beat_at(ST, 0.35), 0.5)
        self.assertAlmostEqual(beat_at(ST, 0.6), 1.0)  # the next beat
        self.assertEqual(beat_at({"bpm": 0}, 1.0), 0)
        self.assertEqual(beat_at(None, 1.0), 0)

    def test_sections_index_by_fraction_of_duration(self):
        self.assertEqual(section_at(ST, 0), 0)
        self.assertEqual(section_at(ST, 15.0), 1)  # exactly at 0.25
        self.assertEqual(section_at(ST, 30.0), 1)
        self.assertEqual(section_at(ST, 45.0), 2)
        self.assertEqual(section_at(ST, 59.9), 2)
        self.assertEqual(section_at({"sections": [0.0]}, 1.0), 0)  # no duration
        self.assertEqual(section_at({"duration": 10}, 1.0), 0)  # no sections

    def test_modulations_map_a_signal_onto_their_range(self):
        mod = {"range": [0.5, 2.0]}
        self.assertAlmostEqual(modulation_factor(mod, 0.25), 0.875)
        self.assertEqual(modulation_factor(mod, -1), 0.5)  # clamped
        self.assertEqual(modulation_factor(mod, 2), 2.0)
        self.assertEqual(modulation_factor({}, 0.3), 1.0)  # default range


if __name__ == "__main__":
    unittest.main()
