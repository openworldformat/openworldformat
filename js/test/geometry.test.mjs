// Geometry parity: the reference renderer's meshes against
// conformance/geometry.json (spec/world.md, "The geometry of the shapes").
// For every shape instance the fixture names, the mesh createGeometry builds
// must have the exact bounds, fit inside the format's conservative box
// (shapeHalfExtents, the mirror of Shape::local_aabb_half), and hold a
// vertex at every landmark — which way the wedge faces, where the apex is,
// the plane the torus lies in, the prism's first vertex, a polyhedron's
// orientation. Dimensions alone never caught a mesh facing the wrong way;
// this does. render.js imports three.js, which node loads fine without a
// renderer: only geometry is built here.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { createGeometry, shapeHalfExtents } from "../src/render.js";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "../..");
const fixture = JSON.parse(readFileSync(path.join(root, "conformance/geometry/shapes.json"), "utf8"));
const tolerance = fixture.tolerance ?? 0.001;

/** @param {import('three').BufferGeometry} geometry @returns {number[][]} */
function vertices(geometry) {
  const p = geometry.getAttribute("position");
  const out = [];
  for (let i = 0; i < p.count; i++) out.push([p.getX(i), p.getY(i), p.getZ(i)]);
  return out;
}

for (const entry of fixture.shapes) {
  test(`${entry.name}: the renderer's mesh has the normative bounds and landmarks`, () => {
    const v = vertices(createGeometry(entry.shape));
    assert.ok(v.length >= 3, `${entry.name} builds a mesh`);
    const min = [Infinity, Infinity, Infinity];
    const max = [-Infinity, -Infinity, -Infinity];
    for (const p of v) {
      for (let i = 0; i < 3; i++) {
        min[i] = Math.min(min[i], p[i]);
        max[i] = Math.max(max[i], p[i]);
      }
    }
    for (let i = 0; i < 3; i++) {
      assert.ok(Math.abs(min[i] - entry.bounds.min[i]) <= tolerance, `${entry.name}: min ${JSON.stringify(min)}, not ${JSON.stringify(entry.bounds.min)}`);
      assert.ok(Math.abs(max[i] - entry.bounds.max[i]) <= tolerance, `${entry.name}: max ${JSON.stringify(max)}, not ${JSON.stringify(entry.bounds.max)}`);
    }
    // The format's conservative box is the fixture's, and the mesh fits in it.
    const half = shapeHalfExtents(entry.shape);
    assert.ok(half, `${entry.name}: shapeHalfExtents knows the shape`);
    for (let i = 0; i < 3; i++) {
      assert.ok(Math.abs(half[i] - entry.half_extents[i]) <= 1e-6, `${entry.name}: half-extents ${JSON.stringify(half)}, not ${JSON.stringify(entry.half_extents)}`);
      assert.ok(max[i] <= half[i] + tolerance && -min[i] <= half[i] + tolerance, `${entry.name}: the mesh leaves the box on axis ${i}`);
    }
    for (const mark of entry.landmarks ?? []) {
      let nearest = Infinity;
      for (const p of v) nearest = Math.min(nearest, Math.hypot(p[0] - mark[0], p[1] - mark[1], p[2] - mark[2]));
      assert.ok(nearest <= tolerance, `${entry.name}: no vertex within ${tolerance} of landmark ${JSON.stringify(mark)} (nearest is ${nearest.toFixed(4)} away)`);
    }
  });
}

test("the fixture covers every shape the conformance worlds draw", () => {
  const drawn = new Set();
  for (const world of ["shapes.json", "prism.json"]) {
    const manifest = JSON.parse(readFileSync(path.join(root, "conformance", world), "utf8"));
    for (const e of manifest.entities) if (e.shape) drawn.add(Object.keys(e.shape)[0]);
  }
  const covered = new Set(fixture.shapes.map((s) => Object.keys(s.shape)[0]));
  assert.deepEqual([...drawn].sort(), [...covered].sort());
  assert.equal(covered.size, 12);
});
