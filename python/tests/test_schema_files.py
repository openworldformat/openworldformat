"""The package ships the repository's normative schemas
(openworldformat/schema/) so a consumer validates against its
installed dependency — schema_path("world.schema.json") — instead of a
checkout of this repository. The copies are byte-identical snapshots
of schema/, never hand-edited: when the schema changes, copy the whole
tree again (world, state, extensions/). These tests fail when a copy
drifts and when a new schema appears in schema/ that the package does
not ship. Mirrors js/test/schema-files.test.mjs."""

import json
import unittest
from pathlib import Path

from openworldformat import schema_path

ROOT = Path(__file__).resolve().parents[2]  # this repository
SHIPPED = Path(__file__).resolve().parents[1] / "openworldformat" / "schema"


def schema_names() -> list:
    """Every .schema.json under schema/, as names the package ships."""
    return sorted(
        p.relative_to(ROOT / "schema").as_posix()
        for p in (ROOT / "schema").rglob("*.schema.json")
    )


class SchemaFilesTest(unittest.TestCase):
    def test_the_shipped_schemas_are_the_repositorys_byte_for_byte(self):
        self.assertEqual(
            sorted(
                p.relative_to(SHIPPED).as_posix()
                for p in SHIPPED.rglob("*.schema.json")
            ),
            schema_names(),
            "openworldformat/schema/ and schema/ hold different files — "
            "refresh the copies",
        )
        for name in schema_names():
            self.assertEqual(
                (SHIPPED / name).read_text(),
                (ROOT / "schema" / name).read_text(),
                f"schema/{name} drifted from schema/ — copy it again, "
                "never hand-edit it",
            )

    def test_every_schema_resolves_through_the_accessor(self):
        for name in schema_names():
            path = schema_path(name)
            self.assertTrue(path.is_file(), name)
            schema = json.loads(path.read_text())
            self.assertIsInstance(
                schema.get("$schema"), str, f"{name} is a JSON Schema"
            )
        with self.assertRaises(FileNotFoundError):
            schema_path("no-such.schema.json")


if __name__ == "__main__":
    unittest.main()
