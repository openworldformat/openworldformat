// The ext-h3 extension (spec/extensions/h3.md): the unit table, the
// quantisation the producing format uses, the tangent-plane derivation,
// the block readers — and the conformance outcome assertions, which
// check the one invariant from both ends on conformance/h3.json.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseManifest } from "../src/index.js";
import {
  EARTH_RADIUS_M, EXTENSION_NAME, edgeLengthM, dequantizeRotation, dequantizeScale,
  dequantizeTranslation, deriveTransform, frameOf, localPosition, placementOf,
  quantizeRotation, quantizeScale, quantizeTranslation, runOutcomes,
} from "../src/h3.js";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (p) => readFileSync(path.join(root, p), "utf8");
const close = (a, b, eps = 1e-9) => Math.abs(a - b) <= eps;

test("quantisation matches the producing format", () => {
  assert.equal(quantizeScale(2), 1024);
  assert.equal(quantizeScale(0.5), -1024);
  assert.equal(quantizeScale(1), 0);
  assert.equal(quantizeScale(0), null);
  assert.ok(close(dequantizeScale(1024), 2));
  assert.equal(quantizeRotation(30), 3000);
  assert.equal(quantizeRotation(-0.005), -1); // half away from zero
  assert.equal(dequantizeRotation(-4500), -45);
  // Resolution 15: an edge is 0.584169 m, a unit 5.84169 mm.
  assert.equal(quantizeTranslation(0.75, 15), 128);
  assert.ok(close(dequantizeTranslation(128, 15), 0.74773632));
  assert.equal(edgeLengthM(16), null);
  assert.equal(quantizeTranslation(1, 16), null);
});

test("the origin cell places by translation alone; east is +X, north is −Z", () => {
  const here = { lat: 48.85839598760832, lng: 2.29450022174426 };
  const p = localPosition(here, here, [1, 2, 3]);
  assert.ok(close(p[0], 1) && close(p[1], 2) && close(p[2], 3), JSON.stringify(p));
  const origin = [0, 0];
  const step = 1e-5; // degrees: 1.1119 m on H3's sphere
  const metres = EARTH_RADIUS_M * (step * Math.PI) / 180;
  const east = localPosition(origin, [0, step], [0, 0, 0]);
  assert.ok(close(east[0], metres, 1e-6) && close(east[2], 0), JSON.stringify(east));
  assert.ok(east[1] < 0 && east[1] > -1e-6, `the tangent plane drops: ${east[1]}`);
  const north = localPosition(origin, [step, 0], [0, 0, 0]);
  assert.ok(close(north[2], -metres, 1e-6) && close(north[0], 0), JSON.stringify(north));
  const upThere = localPosition(origin, [step, 0], [0, 5, 0]);
  assert.ok(close(upThere[1], 5, 1e-6), "a translation in the far cell's frame is carried into the origin's");
});

test("the blocks read leniently and derive the core transform", () => {
  const entity = {
    id: 7, name: "column",
    [EXTENSION_NAME]: { cell: "8f1fb46741a12d5", resolution: 15, translation: [0, 128, 0], rotation: [0, 3000, 0], scale: [0, 1024, 0] },
  };
  const placement = placementOf(entity);
  assert.deepEqual(placement, {
    cell: "8f1fb46741a12d5", resolution: 15, body: "earth",
    translation: [0, 128, 0], rotation: [0, 3000, 0], scale: [0, 1024, 0],
  });
  const centre = [48.85839598760832, 2.29450022174426];
  const t = deriveTransform(null, centre, placement);
  assert.ok(t && close(t.position[1], 0.74773632, 1e-6));
  assert.deepEqual(t.rotation_degrees, [0, 30, 0]);
  assert.deepEqual(t.scale, [1, 2, 1]);
  assert.equal(deriveTransform(null, centre, { ...placement, body: "moon" }), null);
  // Malformed blocks are absent to the extension.
  assert.equal(placementOf({ id: 1, name: "x", [EXTENSION_NAME]: { cell: "nope", resolution: 15 } }), null);
  assert.equal(placementOf({ id: 1, name: "x", [EXTENSION_NAME]: { cell: "8f1fb46741a12d5", resolution: 15, translation: [0, 1.5, 0] } }), null);
  assert.equal(placementOf({ id: 1, name: "x" }), null);
  assert.deepEqual(
    frameOf({ version: 3, entities: [], environment: { [EXTENSION_NAME]: { cell: "8f1fb46741a12d5", resolution: 15 } } }),
    { cell: "8f1fb46741a12d5", resolution: 15, body: "earth" },
  );
  assert.equal(frameOf({ version: 3, entities: [] }), null);
});

test("the h3 conformance outcomes pass under the reference derivation", () => {
  const manifest = parseManifest(read("conformance/h3.json"));
  const outcomes = JSON.parse(read("conformance/outcomes/h3.json"));
  const result = runOutcomes(manifest, outcomes);
  assert.deepEqual(result, { ok: true, failures: [] });
});

test("the outcome runner checks the invariant from both ends", () => {
  const manifest = parseManifest(read("conformance/h3.json"));
  const outcomes = JSON.parse(read("conformance/outcomes/h3.json"));
  // Move the column by its core transform alone: the derivation still
  // agrees with the fixture, the core transform no longer does.
  const liar = manifest.entities.find((e) => e.name === "column");
  liar.transform.position = [5, 0.747736, 0];
  const result = runOutcomes(manifest, outcomes);
  assert.equal(result.ok, false);
  assert.deepEqual(result.failures.map((f) => f.split(" is ")[0]), ["column: core position"]);
});
