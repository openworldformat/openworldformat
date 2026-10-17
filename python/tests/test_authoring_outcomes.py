"""The authoring outcome suite (conformance/authoring/cases.json) through
this reference's ``ingest``: each batch over the base world, and what must
come back — mirrors rust/tests/authoring_outcomes.rs, the normative
runner. ``ops[].path`` is a JSON pointer into the committed op,
``world[].path`` one into the named entity; numbers compare by value."""

import json
import unittest
from pathlib import Path

from openworldformat import fold_log, ingest, parse_manifest

ROOT = Path(__file__).resolve().parents[2]
SUITE = json.loads((ROOT / "conformance" / "authoring" / "cases.json").read_text())


def at(value, pointer: str):
    """A JSON pointer into a value (``/a/0/b``), or None where it leads nowhere."""
    node = value
    for raw in pointer.split("/")[1:]:
        key = raw.replace("~1", "/").replace("~0", "~")
        if isinstance(node, list):
            try:
                node = node[int(key)]
            except (ValueError, IndexError):
                return None
        elif isinstance(node, dict) and key in node:
            node = node[key]
        else:
            return None
    return node


def same(a, b) -> bool:
    """Numbers compare by value — the suite writes 2, a float field reads back 2.0."""
    if isinstance(a, bool) or isinstance(b, bool):
        return a == b
    if isinstance(a, (int, float)) and isinstance(b, (int, float)):
        return abs(a - b) <= 1e-6 * max(abs(a), abs(b), 1.0)
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b))
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(same(a[k], b[k]) for k in a)
    return a == b


class AuthoringOutcomesTest(unittest.TestCase):
    def check_world(self, name: str, state: dict, predicates: list) -> None:
        for p in predicates:
            entity_name = p["entity"]
            id_ = state["name_to_id"].get(entity_name)
            if p.get("absent"):
                self.assertIsNone(id_, f"{name}: entity '{entity_name}' should be absent")
                continue
            self.assertIsNotNone(id_, f"{name}: no entity '{entity_name}'")
            entity = state["by_id"][id_]
            got = at(entity, p["path"])
            self.assertIsNotNone(got, f"{name}: '{entity_name}' has nothing at {p['path']}: {entity}")
            self.assertTrue(
                same(got, p["equals"]),
                f"{name}: '{entity_name}' at {p['path']} is {got!r}, expected {p['equals']!r}",
            )

    def test_authoring_outcomes_hold(self):
        base = fold_log(parse_manifest(json.dumps(SUITE["base"])), [])
        self.assertTrue(SUITE["cases"])
        for case in SUITE["cases"]:
            name, expect = case["name"], case["expect"]
            done = ingest(base, case["batch"])
            if expect["result"] == "committed":
                self.assertTrue(done["ok"], f"{name}: refused: {done.get('errors')}")
                self.assertNotIn("errors_contain", expect, f"{name}: committed, but the suite expects an error")
                for want in expect.get("warnings_contain", []):
                    self.assertTrue(
                        any(want in w for w in done["warnings"]),
                        f"{name}: no warning contains {want!r}: {done['warnings']}",
                    )
                for n, id_ in (expect.get("spawned") or {}).items():
                    self.assertEqual(done["spawned"].get(n), id_, f"{name}: spawned {n} -> {done['spawned']}")
                for p in expect.get("ops", []):
                    op = done["ops"][p["op"]]
                    got = at(op, p["path"])
                    self.assertIsNotNone(got, f"{name}: op {p['op']} has nothing at {p['path']}: {op}")
                    self.assertTrue(
                        same(got, p["equals"]),
                        f"{name}: op {p['op']} at {p['path']} is {got!r}, expected {p['equals']!r}",
                    )
                self.check_world(name, done["state"], expect.get("world", []))
            else:
                self.assertFalse(done["ok"], f"{name}: committed, expected refused")
                for want in expect.get("errors_contain", []):
                    self.assertTrue(
                        any(want in e for e in done["errors"]),
                        f"{name}: no error contains {want!r}: {done['errors']}",
                    )
                self.check_world(name, base, expect.get("world", []))


if __name__ == "__main__":
    unittest.main()
