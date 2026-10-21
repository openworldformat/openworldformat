// h3.js — the ext-h3 extension's reference half (spec/extensions/h3.md,
// version 0.1): where an entity sits on a globe, as an H3 cell plus an
// integer-only transform in that cell's frame, beside the core transform
// a reader without the extension draws.
//
// The one invariant: the H3 fields are authoritative, and the core
// transform is derived from them by the producer. This module is that
// derivation — the unit table, the quantisation, the tangent-plane math
// that turns a cell centre and a translation into local metres, and the
// conformance outcome runner — so a producer can write a package and a
// reader with an H3 library can check one. The same code, line for line,
// as the Rust reference's `ext_h3` module.
//
// No H3 library is embedded: a cell's centre comes in as {lat, lng} (or
// [lat, lng]) from whatever H3 implementation the caller has
// (`cellToLatLng`); the conformance fixture carries its centres as data.

/** @typedef {import('./index.js').WorldManifest} WorldManifest */
/** @typedef {import('./index.js').WorldEntity} WorldEntity */
/** @typedef {import('./index.js').WorldTransform} WorldTransform */
/** @typedef {import('./index.js').Vec3} Vec3 */

/** The extension's namespace key, on entities and on the environment. */
export const EXTENSION_NAME = "ext-h3";

/** The body the unit table is for — the default, and the only one this
 *  version defines a unit for. */
export const BODY_EARTH = "earth";

/** H3's sphere, metres: the radius its cell geometry lives on. */
export const EARTH_RADIUS_M = 6371007.180918475;

/** The mean edge length of a cell at each resolution 0..15, metres, as
 *  H3 4 reports it (`getHexagonEdgeLengthAvgM`). One translation unit is
 *  a hundredth of the entry at the placement's resolution.
 *  @type {readonly number[]} */
export const EDGE_LENGTH_M = Object.freeze([
  1281256.011, 483056.8391, 182512.9565, 68979.22179, 26071.75968, 9854.09099,
  3724.532667, 1406.475763, 531.41401, 200.786148, 75.863783, 28.663897,
  10.830188, 4.09201, 1.5461, 0.584169,
]);

/** Translation units per mean edge length. */
export const TRANSLATION_UNITS_PER_EDGE = 100;
/** Rotation units per degree. */
export const ROTATION_UNITS_PER_DEGREE = 100;
/** Scale units per octave: round(log2(factor) * 1024), 0 is 1×. */
export const SCALE_UNITS_PER_OCTAVE = 1024;

/** The environment's block: the package's local frame, pinned to a cell
 *  whose centre is the origin (+X east, +Y up, +Z south, metres).
 *  @typedef {{cell: string, resolution: number, body: string}} ExtH3Frame */
/** An entity's block: its cell, and its transform relative to the cell's
 *  centre in the cell's own frame, as integers.
 *  @typedef {{cell: string, resolution: number, body: string, translation: Vec3, rotation: Vec3, scale: Vec3}} ExtH3Placement */
/** A point on the body, degrees.
 *  @typedef {{lat: number, lng: number}} LatLng */
/** A point as {lat, lng} or [lat, lng].
 *  @typedef {LatLng | [number, number]} LatLngLike */

// ---------------------------------------------------------------------------
// Units and quantisation
// ---------------------------------------------------------------------------

/** Round half away from zero (Math.round rounds −0.5 up; the producing
 *  format and the Rust reference round away).
 *  @param {number} x @returns {number} */
function roundHalfAway(x) {
  const r = Math.sign(x) * Math.round(Math.abs(x));
  return r === 0 ? 0 : r;
}

/** The mean edge length at a resolution, metres; null past 15.
 * @param {number} resolution @returns {number|null} */
export function edgeLengthM(resolution) {
  return Number.isInteger(resolution) && resolution >= 0 && resolution <= 15 ? EDGE_LENGTH_M[resolution] : null;
}

/** One translation unit at a resolution, metres.
 * @param {number} resolution @returns {number|null} */
export function translationUnitM(resolution) {
  const edge = edgeLengthM(resolution);
  return edge == null ? null : edge / TRANSLATION_UNITS_PER_EDGE;
}

/** Metres to translation units at a resolution.
 * @param {number} metres @param {number} resolution @returns {number|null} */
export function quantizeTranslation(metres, resolution) {
  const unit = translationUnitM(resolution);
  return unit == null ? null : roundHalfAway(metres / unit);
}

/** Translation units at a resolution to metres.
 * @param {number} units @param {number} resolution @returns {number|null} */
export function dequantizeTranslation(units, resolution) {
  const unit = translationUnitM(resolution);
  return unit == null ? null : units * unit;
}

/** Degrees to rotation units. @param {number} degrees @returns {number} */
export function quantizeRotation(degrees) {
  return roundHalfAway(degrees * ROTATION_UNITS_PER_DEGREE);
}

/** Rotation units to degrees. @param {number} units @returns {number} */
export function dequantizeRotation(units) {
  return units / ROTATION_UNITS_PER_DEGREE;
}

/** A scale factor to log2 fixed point; null for a factor with no
 *  logarithm (zero, negative, non-finite).
 * @param {number} factor @returns {number|null} */
export function quantizeScale(factor) {
  return Number.isFinite(factor) && factor > 0 ? roundHalfAway(Math.log2(factor) * SCALE_UNITS_PER_OCTAVE) : null;
}

/** Log2 fixed point to a scale factor. @param {number} units @returns {number} */
export function dequantizeScale(units) {
  return 2 ** (units / SCALE_UNITS_PER_OCTAVE);
}

// ---------------------------------------------------------------------------
// Reading the blocks
// ---------------------------------------------------------------------------

const CELL = /^[0-9a-f]{15}$/;

/** @param {unknown} v @returns {v is Record<string, unknown>} */
const isObject = (v) => v !== null && typeof v === "object" && !Array.isArray(v);

/** Three integers, or the fallback when absent, or null when malformed.
 * @param {unknown} v @param {Vec3} fallback @returns {Vec3|null} */
function int3(v, fallback) {
  if (v === undefined) return fallback;
  if (!Array.isArray(v) || v.length !== 3 || !v.every((x) => Number.isInteger(x))) return null;
  return [Number(v[0]), Number(v[1]), Number(v[2])];
}

/** The cell, resolution and body a block names, or null when it can't be
 *  read — must-ignore: a block the extension can't read is absent to it.
 * @param {unknown} block @returns {ExtH3Frame|null} */
function readFrame(block) {
  if (!isObject(block)) return null;
  const { cell, resolution, body = BODY_EARTH } = block;
  if (typeof cell !== "string" || !CELL.test(cell)) return null;
  if (!Number.isInteger(resolution) || Number(resolution) < 0 || Number(resolution) > 15) return null;
  if (typeof body !== "string") return null;
  return { cell, resolution: Number(resolution), body };
}

/** The frame the environment declares, if it carries a well-formed block.
 * @param {WorldManifest} manifest @returns {ExtH3Frame|null} */
export function frameOf(manifest) {
  const env = manifest?.environment;
  return isObject(env) ? readFrame(env[EXTENSION_NAME]) : null;
}

/** An entity's placement, if it carries a well-formed block.
 * @param {WorldEntity} entity @returns {ExtH3Placement|null} */
export function placementOf(entity) {
  const block = entity?.[EXTENSION_NAME];
  const frame = readFrame(block);
  if (!frame || !isObject(block)) return null;
  const translation = int3(block.translation, [0, 0, 0]);
  const rotation = int3(block.rotation, [0, 0, 0]);
  const scale = int3(block.scale, [0, 0, 0]);
  if (!translation || !rotation || !scale) return null;
  return { ...frame, translation, rotation, scale };
}

// ---------------------------------------------------------------------------
// The derivation (normative: spec/extensions/h3.md, "Deriving the core transform")
// ---------------------------------------------------------------------------

/** @param {LatLngLike} p @returns {LatLng} */
export function latLng(p) {
  return Array.isArray(p) ? { lat: p[0], lng: p[1] } : p;
}

/** A tangent frame on the sphere: the point, and the unit vectors of the
 *  local +X (east), +Y (up) and +Z (south) axes, in the body's Cartesian
 *  frame (x through lat 0 / lng 0, z through the pole).
 * @param {LatLngLike} p
 * @returns {{at: Vec3, east: Vec3, up: Vec3, south: Vec3}} */
function tangent(p) {
  const { lat, lng } = latLng(p);
  const la = (lat * Math.PI) / 180, lo = (lng * Math.PI) / 180;
  const sinLat = Math.sin(la), cosLat = Math.cos(la), sinLng = Math.sin(lo), cosLng = Math.cos(lo);
  /** @type {Vec3} */
  const up = [cosLat * cosLng, cosLat * sinLng, sinLat];
  return {
    at: [up[0] * EARTH_RADIUS_M, up[1] * EARTH_RADIUS_M, up[2] * EARTH_RADIUS_M],
    east: [-sinLng, cosLng, 0],
    up,
    south: [sinLat * cosLng, sinLat * sinLng, -cosLat],
  };
}

/** @param {Vec3} a @param {Vec3} b @returns {number} */
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

/** The local position of a point given as a cell centre plus a translation
 *  in that cell's frame (metres: east, up, south), in the frame at
 *  `frameOrigin`: the cell centre's offset from the origin, plus the
 *  translation carried over exactly — the two tangent frames differ by
 *  their tilt, and that tilt is applied, not ignored. When the origin is
 *  the cell itself, the result is the translation.
 * @param {LatLngLike} frameOrigin @param {LatLngLike} cellCentre @param {Vec3} translationM
 * @returns {Vec3} */
export function localPosition(frameOrigin, cellCentre, translationM) {
  const o = tangent(frameOrigin);
  const c = tangent(cellCentre);
  const [tx, ty, tz] = translationM;
  /** @type {Vec3} */
  const v = [0, 0, 0];
  for (let axis = 0; axis < 3; axis++) {
    v[axis] = c.at[axis] - o.at[axis] + tx * c.east[axis] + ty * c.up[axis] + tz * c.south[axis];
  }
  return [dot(v, o.east), dot(v, o.up), dot(v, o.south)];
}

/** The core transform a producer writes for a placement: position by
 *  `localPosition` (the frame's origin when the environment declares one,
 *  else the entity's own cell, which makes the offset zero), rotation and
 *  scale dequantised axis by axis. Null when the body is not `earth` or the
 *  resolution has no unit — the extension then says nothing about where the
 *  entity is, and the core transform stands.
 * @param {LatLngLike|null|undefined} frameOrigin @param {LatLngLike} cellCentre @param {ExtH3Placement} placement
 * @returns {{position: Vec3, rotation_degrees: Vec3, scale: Vec3, visible: boolean}|null} */
export function deriveTransform(frameOrigin, cellCentre, placement) {
  if (placement.body !== BODY_EARTH) return null;
  const unit = translationUnitM(placement.resolution);
  if (unit == null) return null;
  const [tx, ty, tz] = placement.translation;
  const position = localPosition(frameOrigin ?? cellCentre, cellCentre, [tx * unit, ty * unit, tz * unit]);
  const [rx, ry, rz] = placement.rotation;
  const [sx, sy, sz] = placement.scale;
  return {
    position,
    rotation_degrees: [dequantizeRotation(rx), dequantizeRotation(ry), dequantizeRotation(rz)],
    scale: [dequantizeScale(sx), dequantizeScale(sy), dequantizeScale(sz)],
    visible: true,
  };
}

// ---------------------------------------------------------------------------
// Outcome assertions — the extension's conformance
// ---------------------------------------------------------------------------

/** A conformance outcomes document for the extension: the cell centres the
 *  derivation needs (data, since no reference carries H3) and the assertions.
 *  @typedef {{world?: string, cell_centres?: Record<string, [number, number]>, expect: any[]}} OutcomesDoc */

/** @param {unknown} actual @param {Vec3} expected @param {number} tolerance @returns {boolean} */
function within(actual, expected, tolerance) {
  if (!Array.isArray(actual) || actual.length !== 3) return false;
  return expected.every((e, i) => Math.abs(Number(actual[i]) - e) <= tolerance);
}

/** Run a conformance outcomes document against a world: read the blocks,
 *  derive, compare — the derivation against the expected values, and the
 *  core transform the producer wrote against the same values, which is the
 *  invariant checked from both ends.
 * @param {WorldManifest} manifest @param {OutcomesDoc} outcomes
 * @returns {{ok: boolean, failures: string[]}} */
export function runOutcomes(manifest, outcomes) {
  /** @type {string[]} */
  const failures = [];
  const centres = outcomes.cell_centres ?? {};
  const frame = frameOf(manifest);
  const origin = frame ? centres[frame.cell] : undefined;
  for (const assertion of outcomes.expect) {
    if ("frame" in assertion) {
      const expected = assertion.frame;
      const same = frame && frame.cell === expected.cell && frame.resolution === expected.resolution
        && frame.body === (expected.body ?? BODY_EARTH);
      if (!same) failures.push(`the environment declares ${JSON.stringify(frame)}, not ${JSON.stringify(expected)}`);
    } else if ("edge_length" in assertion) {
      const { resolution, metres, tolerance = 1e-9 } = assertion.edge_length;
      const found = edgeLengthM(resolution);
      if (found == null || Math.abs(found - metres) > tolerance) {
        failures.push(`edge length at resolution ${resolution} is ${found}, not ${metres}`);
      }
    } else if ("placed" in assertion) {
      const placed = assertion.placed;
      const tolerance = placed.tolerance ?? 0.001;
      const entity = manifest.entities.find((e) => e.name === placed.entity);
      if (!entity) { failures.push(`no entity is named ${placed.entity}`); continue; }
      const placement = placementOf(entity);
      if (!placement) { failures.push(`${placed.entity} carries no readable ext-h3 block`); continue; }
      const centre = centres[placement.cell];
      if (!centre) { failures.push(`${placed.entity}: the fixture has no centre for cell ${placement.cell}`); continue; }
      if (frame && !origin) { failures.push("the fixture has no centre for the frame's origin cell"); continue; }
      const derived = deriveTransform(origin, centre, placement);
      if (!derived) {
        failures.push(`${placed.entity}: nothing derives from body ${placement.body} at resolution ${placement.resolution}`);
        continue;
      }
      const core = entity.transform ?? {};
      /** @param {string} what @param {unknown} actual @param {Vec3} expected */
      const check = (what, actual, expected) => {
        if (!within(actual, expected, tolerance)) {
          failures.push(`${placed.entity}: ${what} is ${JSON.stringify(actual)}, not within ${tolerance} of ${JSON.stringify(expected)}`);
        }
      };
      check("derived position", derived.position, placed.position);
      check("core position", core.position ?? [0, 0, 0], placed.position);
      if (placed.rotation_degrees) {
        check("derived rotation", derived.rotation_degrees, placed.rotation_degrees);
        check("core rotation", core.rotation_degrees ?? [0, 0, 0], placed.rotation_degrees);
      }
      if (placed.scale) {
        check("derived scale", derived.scale, placed.scale);
        check("core scale", core.scale ?? [1, 1, 1], placed.scale);
      }
    } else {
      failures.push(`unknown assertion ${JSON.stringify(assertion)}`);
    }
  }
  return { ok: failures.length === 0, failures };
}
