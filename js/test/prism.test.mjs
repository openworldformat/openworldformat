// The Prism shape's geometry, as spec/world.md "The prism" states it:
// the first cap vertex on +X, the ring counter-clockwise from above, flat
// caps at ±height/2, every face wound outward, 4·sides − 4 triangles, and
// `sides` clamped to the schema's 3..8. The renderer (render.js) draws
// exactly these triangles; the Rust reference's triangle estimate counts
// the same number.

import test from "node:test";
import assert from "node:assert/strict";
import { prismRing, prismSides, prismTriangles } from "../src/prism.js";

const close = (a, b, eps = 1e-9) => Math.abs(a - b) <= eps;

test("the ring starts on +X and runs counter-clockwise seen from above", () => {
  const ring = prismRing(6, 2, 0.5);
  assert.equal(ring.length, 6);
  assert.deepEqual(ring[0], [2, 0.5, -0]);
  // Vertex 1 at 60°: x = cos 60°, z = −sin 60° — toward −Z, which is "up" on
  // a map drawn from +Y with +X to the right: counter-clockwise.
  assert.ok(close(ring[1][0], 2 * Math.cos(Math.PI / 3)));
  assert.ok(close(ring[1][2], -2 * Math.sin(Math.PI / 3)));
  for (const [x, y, z] of ring) {
    assert.ok(close(Math.hypot(x, z), 2), "every vertex is on the circumcircle");
    assert.equal(y, 0.5);
  }
});

test("every prism has 4·sides − 4 triangles, flat caps, and outward faces", () => {
  for (let sides = 3; sides <= 8; sides++) {
    const radius = 0.6, height = 1.5;
    const triangles = prismTriangles(sides, radius, height);
    assert.equal(triangles.length, 4 * sides - 4, `${sides} sides`);
    for (const tri of triangles) {
      const [a, b, c] = tri;
      for (const [x, y, z] of tri) {
        assert.ok(close(Math.abs(y), height / 2), `${sides}: caps at ±height/2`);
        assert.ok(close(Math.hypot(x, z), radius), `${sides}: vertices on the circumcircle`);
      }
      // Outward: the face normal points away from the centroid at the origin.
      const u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
      const v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
      const n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
      const centroid = [(a[0] + b[0] + c[0]) / 3, (a[1] + b[1] + c[1]) / 3, (a[2] + b[2] + c[2]) / 3];
      const outward = n[0] * centroid[0] + n[1] * centroid[1] + n[2] * centroid[2];
      assert.ok(outward > 1e-9, `${sides}: a face winds inward: ${JSON.stringify(tri)}`);
    }
    // The caps: sides − 2 triangles each, at +h/2 and −h/2.
    const top = triangles.filter((t) => t.every((p) => p[1] > 0)).length;
    const bottom = triangles.filter((t) => t.every((p) => p[1] < 0)).length;
    assert.equal(top, sides - 2);
    assert.equal(bottom, sides - 2);
  }
});

test("a renderer clamps sides to the schema's range and defaults to six", () => {
  assert.equal(prismSides(2), 3);
  assert.equal(prismSides(9), 8);
  assert.equal(prismSides(5.4), 5);
  assert.equal(prismSides(Number.NaN), 6);
  assert.equal(prismTriangles(1, 1, 1).length, 8);
});
