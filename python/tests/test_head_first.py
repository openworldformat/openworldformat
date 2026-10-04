"""The live-authoring rules this reference reads (spec/session.md "The fold
is total", spec/package.md "Head-first") — mirrors js/test/authoring.test.mjs
and the Rust conformance suite: every world survives an empty fold, every
example's manifest is the fold to main, and ModifyWorld reaches every scene
field and undoes. Python compares 2 and 2.0 as equal, as JSON does."""

import hashlib
import json
import unittest
from pathlib import Path

from openworldformat import (
    BASE_SNAPSHOT,
    SUPPORTED_FORMAT_VERSION,
    WorldFormatError,
    compute_entry_id,
    compute_inverse,
    fold_log,
    fold_path,
    parse_log_line,
    parse_manifest,
    read_package,
    to_manifest,
)

ROOT = Path(__file__).resolve().parents[2]
EXAMPLES = sorted(d for d in (ROOT / "examples").iterdir() if (d / "manifest.json").exists())


def normalized(m: dict) -> dict:
    """A world compared as a world: entities by id, empty and null fields
    as absent, next_entity_id by its effective value."""
    out = {k: v for k, v in m.items() if v is not None and v != []}
    past = max((e["id"] + 1 for e in m.get("entities", [])), default=1)
    out["next_entity_id"] = max(m.get("next_entity_id") or 1, past)
    out["entities"] = sorted(m.get("entities", []), key=lambda e: e["id"])
    return out


class HeadFirstTest(unittest.TestCase):
    def test_the_fold_is_total_every_world_survives_an_empty_fold(self):
        worlds = [(p.name, p) for p in sorted((ROOT / "conformance").glob("*.json"))]
        for d in EXAMPLES:
            worlds.append((f"{d.name}/base", d / BASE_SNAPSHOT))
            worlds.append((f"{d.name}/head", d / "manifest.json"))
        for name, path in worlds:
            manifest = parse_manifest(path.read_text())
            state = fold_log(manifest, [])
            # Names bind at ingestion: compare against the bound entities.
            bound = {**manifest, "entities": state["entities"]}
            self.assertEqual(normalized(to_manifest(state)), normalized(bound), name)

    def test_every_example_is_head_first_its_manifest_is_the_fold_to_main(self):
        self.assertEqual(SUPPORTED_FORMAT_VERSION, 2)
        for d in EXAMPLES:
            base, entries, head, package = read_package(d)
            self.assertEqual(package["format_version"], 2, d.name)
            tip = (package.get("refs") or {}).get("main")
            state = fold_path(base, entries, tip)
            self.assertEqual(normalized(to_manifest(state)), normalized(head), d.name)
            sha = hashlib.sha256((d / "manifest.json").read_bytes()).hexdigest()
            self.assertEqual(package["world_sha256"], sha, f"{d.name}: world_sha256")

    def test_modify_world_reaches_every_scene_field_and_undoes(self):
        manifest = parse_manifest((ROOT / "examples" / "hello-world" / "manifest.json").read_text())
        state = fold_log(manifest, [])
        op = {"ModifyWorld": {"patch": {
            "meta": {"name": "hello-again", "description": "renamed"},
            "environment": None,
            "tours": [{"name": "walk", "waypoints": []}],
            "soundtrack": None,
        }}}
        inverse = compute_inverse(op, state)
        line = lambda ops, rev: parse_log_line(json.dumps({"revision": rev, "ops": ops}))
        changed = fold_log(manifest, [line([op], 1)])
        m = to_manifest(changed)
        self.assertEqual(m["meta"]["name"], "hello-again")
        self.assertNotIn("environment", m, "null clears")
        self.assertEqual(len(m["tours"]), 1)
        back = fold_log(m, [line([inverse], 2)])
        self.assertEqual(normalized(to_manifest(back)), normalized(to_manifest(state)))
        with self.assertRaises(WorldFormatError):
            fold_log(m, [line([{"ModifyWorld": {"patch": {"meta": None}}}], 2)])


    def test_an_entrys_message_is_part_of_its_identity_in_every_reference(self):
        entry = parse_log_line('{"id":"x","parent":"e6","revision":7,"author":{"name":"claude"},"timestamp_ms":1790000000123,"message":"a lantern by the gate","ops":[{"DeleteEntity":{"id":21}}]}')
        self.assertEqual(entry["message"], "a lantern by the gate")
        self.assertEqual(compute_entry_id(entry), "sha256:a7cd0955d35a2ff15b16ad7cc3440fb064d675ceceeca865a7ace463a5d177f2")


if __name__ == "__main__":
    unittest.main()
