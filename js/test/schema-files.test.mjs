// The npm package ships the repository's normative schemas (js/schema/)
// so a consumer validates against its installed dependency —
// `require.resolve("openworldformat/schema/world.schema.json")` — instead
// of a checkout of this repository. The copies are byte-identical
// snapshots of schema/, never hand-edited: when the schema changes, copy
// the whole tree again (world, state, extensions/). These tests fail
// when a copy drifts, when the exports map stops resolving one, and
// when a new schema appears in schema/ that the package does not ship.

import test from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../.."); // this repository
const require = createRequire(import.meta.url);

// Every .schema.json under schema/, as package-relative paths ("schema/…").
const schemasOf = (dir) =>
  readdirSync(dir, { recursive: true })
    .filter((f) => f.endsWith(".schema.json"))
    .map((f) => path.join("schema", String(f)).replaceAll(path.sep, "/"))
    .sort();

test("the shipped schemas are the repository's, byte for byte", () => {
  assert.deepEqual(schemasOf(path.join(here, "../schema")), schemasOf(path.join(root, "schema")),
    "js/schema/ and schema/ hold different files — refresh the copies");
  for (const rel of schemasOf(path.join(root, "schema"))) {
    assert.equal(
      readFileSync(path.join(here, "..", rel), "utf8"),
      readFileSync(path.join(root, rel), "utf8"),
      `${rel} drifted from schema/ — copy it again, never hand-edit it`,
    );
  }
});

test("every schema resolves through the package's exports", () => {
  const pkg = JSON.parse(readFileSync(path.join(here, "../package.json"), "utf8"));
  assert.ok(pkg.files.includes("schema"), "npm pack would leave the schemas out");
  for (const rel of schemasOf(path.join(root, "schema"))) {
    const id = `openworldformat/${rel}`;
    assert.equal(require.resolve(id), path.resolve(here, "..", rel), id);
    const schema = JSON.parse(readFileSync(require.resolve(id), "utf8"));
    assert.equal(typeof schema.$schema, "string", `${id} is a JSON Schema`);
  }
});
