// prism.js — the Prism shape's geometry, exactly as spec/world.md "The
// prism" states it, as plain arrays: no three.js, so the normative vertex
// order is testable in node and reusable by any renderer. render.js turns
// the triangles into a BufferGeometry.
//
// Origin at the centroid, axis along +Y, flat caps at y = ±height/2.
// `radius` is the circumradius: cap vertex i sits at
// (radius·cos θ, ±height/2, −radius·sin θ), θ = 2π·i/sides — the first
// vertex on +X, the ring counter-clockwise seen from above (+Y). Every
// face winds counter-clockwise seen from outside (outward normals); the
// caps are fans from vertex 0; there is no segment parameter beyond
// `sides`, which a renderer clamps to 3..8 (the schema's range).

/** @typedef {[number, number, number]} Point */

/** The side count a renderer draws for a declared `sides`: an integer in 3..8.
 * @param {number} sides @returns {number} */
export function prismSides(sides) {
  const n = Math.round(Number(sides));
  return Number.isFinite(n) ? Math.min(8, Math.max(3, n)) : 6;
}

/** One cap's ring of vertices at height `y`, vertex 0 on +X, counter-clockwise from above.
 * @param {number} sides @param {number} radius @param {number} y @returns {Point[]} */
export function prismRing(sides, radius, y) {
  const n = prismSides(sides);
  return Array.from({ length: n }, (_, i) => {
    const theta = (2 * Math.PI * i) / n;
    return /** @type {Point} */ ([radius * Math.cos(theta), y, -radius * Math.sin(theta)]);
  });
}

/** The prism as triangles — `sides` side quads (two triangles each) and
 * two fanned caps: 4·sides − 4 triangles, all wound outward.
 * @param {number} sides @param {number} radius @param {number} height
 * @returns {Point[][]} */
export function prismTriangles(sides, radius, height) {
  const n = prismSides(sides);
  const top = prismRing(n, radius, height / 2);
  const bottom = prismRing(n, radius, -height / 2);
  /** @type {Point[][]} */
  const triangles = [];
  for (let i = 0; i < n; i++) {
    const j = (i + 1) % n;
    triangles.push([bottom[i], bottom[j], top[j]], [bottom[i], top[j], top[i]]);
  }
  for (let i = 1; i < n - 1; i++) {
    triangles.push([top[0], top[i], top[i + 1]]); // seen from +Y: counter-clockwise
    triangles.push([bottom[0], bottom[i + 1], bottom[i]]); // seen from −Y: counter-clockwise
  }
  return triangles;
}
