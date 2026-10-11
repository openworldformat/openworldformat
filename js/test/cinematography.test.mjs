// The ext-cinematography reference (js/src/cinematography.js,
// spec/extensions/cinematography.md): the crop math, the view, frame
// membership, the shot list — and the conformance outcome assertions,
// which are the extension's definition of "correct".

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { parseManifest } from "../src/index.js";
import {
  cameraOf, frameOf, viewOf, project, shotList, runOutcomes,
  worldOrigin, aimPointOf, focusDistanceOf,
  EXTENSION_NAME, EXTENSION_VERSION, DEFAULT_SENSOR,
} from "../src/cinematography.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (/** @type {string} */ p) => readFileSync(path.join(root, p), "utf8");

const manifest = parseManifest(read("conformance/cinematography.json"));
const byName = new Map(manifest.entities.map((e) => [e.name, e]));

test("cameraOf applies the defaults per absent field, and null for non-cameras", () => {
  assert.equal(cameraOf(byName.get("maya")), null);
  const bare = cameraOf({ id: 1, name: "c", [EXTENSION_NAME]: { camera: {} } });
  assert.deepEqual(bare.sensor_mm, DEFAULT_SENSOR);
  assert.equal(bare.focal_length_mm, 35);
  assert.equal(bare.squeeze, 1);
  assert.equal(bare.aspect_ratio, undefined);
  assert.equal(bare.aim_entity, undefined);
  assert.equal(bare.focus_entity, undefined);
  // The references read as the document holds them.
  assert.equal(cameraOf(byName.get("4A")).aim_entity, 4);
  assert.equal(cameraOf(byName.get("4A")).focus_entity, 4);
  // The extension version the module implements.
  assert.equal(EXTENSION_VERSION, "0.3.0");
});

test("worldOrigin composes the position through the parents", () => {
  // The mug sits on a tray turned 30° on the counter: its origin is its
  // local position rotated and carried, not as written.
  const mug = worldOrigin(byName.get("mug"), manifest);
  const rad = Math.PI / 6;
  const expected = [0.8 + 0.3 * Math.cos(rad), 1.05, -1 - 0.3 * Math.sin(rad)];
  for (let i = 0; i < 3; i++) assert.ok(Math.abs(mug[i] - expected[i]) < 1e-9, `axis ${i}: ${mug[i]} vs ${expected[i]}`);
  // An unparented entity's origin is its position.
  assert.deepEqual(worldOrigin(byName.get("maya"), manifest), [-1.5, 0.85, 0]);
});

test("aim_entity centres the target and focus_entity measures to it, through parents", () => {
  const tracked = byName.get("4A");
  const camera = cameraOf(tracked);
  const maya = worldOrigin(byName.get("maya"), manifest);
  assert.deepEqual(aimPointOf(camera, manifest), maya);
  const p = project(viewOf(tracked, camera, manifest), frameOf(camera), camera, maya);
  assert.ok(Math.abs(p.x) < 1e-9 && Math.abs(p.y) < 1e-9 && p.z > 0);
  assert.ok(Math.abs(focusDistanceOf(tracked, camera, manifest) - 4.3843) < 1e-3);
  // The insert centres the parented mug's world origin and focuses on it.
  const insert = byName.get("insert");
  const macro = cameraOf(insert);
  const q = project(viewOf(insert, macro, manifest), frameOf(macro), macro, worldOrigin(byName.get("mug"), manifest));
  assert.ok(Math.abs(q.x) < 1e-9 && Math.abs(q.y) < 1e-9 && q.z > 0);
  assert.ok(Math.abs(focusDistanceOf(insert, macro, manifest) - 1.4956) < 1e-3);
  // Without the world only `aim` can answer: the tracked camera has none.
  assert.equal(aimPointOf(camera), null);
});

test("a gone target leaves aim, then the rotation, and focus_distance_m", () => {
  const orphan = {
    id: 40, name: "orphan", transform: { position: [0, 1, 5] },
    [EXTENSION_NAME]: { camera: { aim_entity: 999, aim: [0, 1, 0], focus_entity: 999, focus_distance_m: 2.5 } },
  };
  const camera = cameraOf(orphan);
  assert.deepEqual(aimPointOf(camera, manifest), [0, 1, 0]);
  assert.equal(focusDistanceOf(orphan, camera, manifest), 2.5);
  assert.ok(Math.abs(viewOf(orphan, camera, manifest).forward[2] + 1) < 1e-9);
});

test("frameOf is the normative crop math: cropping trims, never widens", () => {
  // Super 35 with no aspect is the whole sensor.
  const full = frameOf(cameraOf(byName.get("2A")));
  assert.ok(Math.abs(full.aspect - 24.89 / 18.66) < 1e-9);
  // A 2.39 crop on Super 35 trims the height only.
  const cropped = frameOf(cameraOf(byName.get("1A")));
  assert.equal(cropped.width_mm, 24.89);
  assert.ok(cropped.height_mm < 18.66);
  assert.ok(Math.abs(cropped.aspect - 2.39) < 1e-9);
  // The 2× anamorphic desqueezes, then crops the width.
  const ana = frameOf(cameraOf(byName.get("3A")));
  assert.ok(ana.width_mm < 24.89 * 2);
  assert.equal(ana.height_mm, 18.66);
  assert.ok(Math.abs(ana.aspect - 2.39) < 1e-9);
});

test("viewOf looks at the aim with +Y up, else down the entity's local −Z", () => {
  const wide = viewOf(byName.get("1A"), cameraOf(byName.get("1A")));
  assert.deepEqual(wide.position, [0, 1.6, 6]);
  // Aiming at the origin from +Z looks down −Z.
  const [fx, fy, fz] = wide.forward;
  assert.ok(Math.abs(fx) < 1e-9 && fz < 0 && fy < 0); // tilted slightly down
  assert.ok(Math.abs(wide.up[1]) > 0.9); // +Y up

  // No aim: local −Z carried by the entity's rotation. A camera yawed
  // 90° right (intrinsic XYZ) looks down −X.
  const turned = viewOf(
    { id: 9, name: "t", transform: { position: [0, 0, 0], rotation_degrees: [0, 90, 0] } },
    cameraOf({ id: 9, name: "t", [EXTENSION_NAME]: { camera: {} } }),
  );
  const [tx, ty, tz] = turned.forward;
  assert.ok(Math.abs(tx + 1) < 1e-9 && Math.abs(ty) < 1e-9 && Math.abs(tz) < 1e-9);
});

test("project puts the aim point at the frame's center", () => {
  const camera = cameraOf(byName.get("2A"));
  const view = viewOf(byName.get("2A"), camera);
  const frame = frameOf(camera);
  const p = project(view, frame, camera, [-1.5, 1.2, 0]);
  assert.ok(Math.abs(p.x) < 1e-9 && Math.abs(p.y) < 1e-9 && p.z > 0);
});

test("shotList orders by shot.order, ties by entity id, and skips shot-less cameras", () => {
  const names = shotList(manifest).map((s) => s.name);
  assert.deepEqual(names, ["1A", "2A", "2B", "3A", "4A"]); // bts and insert carry no shot
  // Absent orders sort last; ties break by id.
  const world = { entities: [
    { id: 7, name: "b", [EXTENSION_NAME]: { shot: {} } },
    { id: 3, name: "a", [EXTENSION_NAME]: { shot: {} } },
    { id: 5, name: "c", [EXTENSION_NAME]: { shot: { order: 1 } } },
  ] };
  assert.deepEqual(shotList(world).map((s) => s.name), ["c", "a", "b"]);
});

test("the cinematography conformance outcomes pass under the reference math", () => {
  const outcomes = JSON.parse(read("conformance/outcomes/cinematography.json"));
  const result = runOutcomes(manifest, outcomes);
  assert.deepEqual(result.failures, []);
  assert.ok(result.ok);
});
