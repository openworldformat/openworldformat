// The renderer reports its version (`sceneInfo().viewerVersion`, the
// `VIEWER_VERSION` export), and that version is the package's: a release
// bumps package.json and this constant together, or this test fails.
// Read from the source text — render.js imports three.js, which the
// test runner has no reason to load.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));

test("VIEWER_VERSION is the package version", () => {
  const { version } = JSON.parse(readFileSync(path.join(here, "../package.json"), "utf8"));
  const source = readFileSync(path.join(here, "../src/render.js"), "utf8");
  const match = source.match(/export const VIEWER_VERSION = '([^']+)'/);
  assert.ok(match, "render.js exports VIEWER_VERSION as a string literal");
  assert.equal(match[1], version, "bump VIEWER_VERSION with package.json");
});
