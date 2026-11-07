// Pivots, phases and the swing (spec/world.md "Behavior and triggers"):
// a behavior is a function of folded time from the authored transform, a
// pivot is a local point that stays put while the entity turns or scales
// about it, and a kind the renderer does not know is skipped, never fatal.
import test from 'node:test';
import assert from 'node:assert/strict';
import * as THREE from 'three';
import { makeBehavior, holdPivot, pivotOf } from '../src/render.js';

/** A record as the viewer builds one, at an authored transform. */
function record(position, scale = [1, 1, 1], rotationDegrees = [0, 0, 0]) {
  const obj = new THREE.Object3D();
  obj.position.set(...position);
  obj.scale.set(...scale);
  obj.rotation.set(...rotationDegrees.map(THREE.MathUtils.degToRad), 'XYZ');
  return {
    def: {}, object: obj, material: null, light: null,
    base: { position: obj.position.clone(), quaternion: obj.quaternion.clone(), scale: obj.scale.clone(),
      emissive: null, emissiveIntensity: 0, lightIntensity: 0, opacity: 1 },
    behaviors: [], mods: [], resetPosition: false, resetRotation: false, resetScale: false, triggers: [], anim: null,
  };
}

/** One frame at time `t`, as the viewer's tick runs it: the resets, then the behaviors. */
function frame(rec, t) {
  if (rec.resetPosition) rec.object.position.copy(rec.base.position);
  if (rec.resetRotation) rec.object.quaternion.copy(rec.base.quaternion);
  if (rec.resetScale) rec.object.scale.copy(rec.base.scale);
  for (const b of rec.behaviors) b(1 / 60, t);
  return rec.object;
}

/** A local point of the entity, in the parent's frame. */
function at(obj, local) {
  return new THREE.Vector3(...local).multiply(obj.scale).applyQuaternion(obj.quaternion).add(obj.position);
}

function near(actual, expected, tolerance = 1e-5) {
  for (const axis of ['x', 'y', 'z']) {
    assert.ok(Math.abs(actual[axis] - expected[axis]) <= tolerance,
      `${axis}: ${actual[axis]} is not ${expected[axis]} (${actual.toArray()} vs ${expected.toArray()})`);
  }
}

function attach(rec, def) {
  const fn = makeBehavior(def, rec, new Map(), new Map());
  assert.ok(fn, `${Object.keys(def)[0]} makes a behavior`);
  rec.behaviors.push(fn);
  return rec;
}

test('pivotOf: the origin and anything that is not a point are no pivot', () => {
  assert.equal(pivotOf(undefined), null);
  assert.equal(pivotOf([0, 0, 0]), null);
  assert.equal(pivotOf([1, 2]), null);
  assert.deepEqual(pivotOf([0, -1, 0]).toArray(), [0, -1, 0]);
});

test('a spin about its base keeps the base on the hub and sweeps the tip', () => {
  // A blade 2 m tall standing on a hub at y = 0.5 (conformance/behaviors.json).
  const rec = attach(record([9, 1.5, 0]), { Spin: { axis: [0, 0, 1], speed: 90, pivot: [0, -1, 0] } });
  assert.ok(rec.resetRotation && rec.resetPosition);
  for (const t of [0, 0.5, 1, 2.3, 7.25]) {
    near(at(frame(rec, t), [0, -1, 0]), new THREE.Vector3(9, 0.5, 0));
  }
  // A quarter turn about +Z takes the tip from above the hub to its left.
  near(at(frame(rec, 1), [0, 1, 0]), new THREE.Vector3(7, 0.5, 0));
  near(at(frame(rec, 2), [0, 1, 0]), new THREE.Vector3(9, -1.5, 0));
});

test('a spin without a pivot turns in place, and phase is time already spent', () => {
  const plain = attach(record([1, 2, 3]), { Spin: { axis: [0, 1, 0], speed: 90 } });
  assert.equal(plain.resetPosition, false);
  frame(plain, 3.3);
  near(plain.object.position, new THREE.Vector3(1, 2, 3));
  const ahead = attach(record([1, 2, 3]), { Spin: { axis: [0, 1, 0], speed: 90, phase: 90 } });
  const q = frame(ahead, 0).quaternion.clone();
  const same = frame(plain, 1).quaternion;
  assert.ok(Math.abs(q.angleTo(same)) < 1e-6, 'phase 90° at t=0 is 90°/s at t=1');
});

test('two spins on one entity compose from the authored rotation each frame', () => {
  const rec = record([0, 0, 0]);
  attach(rec, { Spin: { axis: [0, 1, 0], speed: 90 } });
  attach(rec, { Spin: { axis: [1, 0, 0], speed: 30 } });
  const expected = new THREE.Quaternion()
    .multiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), Math.PI / 2))
    .multiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(1, 0, 0), Math.PI / 6));
  // The same frame twice: absolute in time, not accumulated.
  frame(rec, 1);
  assert.ok(Math.abs(frame(rec, 1).quaternion.angleTo(expected)) < 1e-6);
});

test('a swing hangs from its top and rocks to its amplitude', () => {
  // A sign 1.2 m tall under a hook at y = 3 (conformance/behaviors.json).
  const rec = attach(record([9, 2.4, -4]), { Swing: { axis: [0, 0, 1], amplitude: 30, frequency: 0.25, pivot: [0, 0.6, 0] } });
  for (const t of [0, 0.3, 1, 2, 3.7]) near(at(frame(rec, t), [0, 0.6, 0]), new THREE.Vector3(9, 3, -4));
  // sin(2π · 0.25 · 1) = 1: the full 30° about +Z, the bottom swings to +X.
  const bottom = at(frame(rec, 1), [0, -0.6, 0]);
  near(bottom, new THREE.Vector3(9 + 1.2 * Math.sin(Math.PI / 6), 3 - 1.2 * Math.cos(Math.PI / 6), -4));
  // sin(2π · 0.25 · 3) = −1: the other way.
  near(at(frame(rec, 3), [0, -0.6, 0]), new THREE.Vector3(9 - 1.2 * Math.sin(Math.PI / 6), 3 - 1.2 * Math.cos(Math.PI / 6), -4));
  // At rest it hangs straight: the authored transform.
  near(at(frame(rec, 2), [0, -0.6, 0]), new THREE.Vector3(9, 1.8, -4));
});

test('a swing takes its defaults: a 30° hinge across X at half a hertz', () => {
  const rec = attach(record([0, 0, 0]), { Swing: {} });
  const q = frame(rec, 0.5).quaternion; // sin(π/2) = 1
  const expected = new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(1, 0, 0), THREE.MathUtils.degToRad(30));
  assert.ok(Math.abs(q.angleTo(expected)) < 1e-6);
});

test('a pulse about its base breathes on the ground', () => {
  // A unit cube on the ground, pivot at its base.
  const rec = attach(record([-9, 0.5, 0]), { Pulse: { min_scale: 0.5, max_scale: 1.5, frequency: 0.25, pivot: [0, -0.5, 0] } });
  assert.ok(rec.resetScale && rec.resetPosition);
  for (const t of [0, 1, 2, 3, 0.37]) near(at(frame(rec, t), [0, -0.5, 0]), new THREE.Vector3(-9, 0, 0));
  // sin(2π · 0.25 · 1) = 1: scale 1.5, the top at 1.5 m.
  near(at(frame(rec, 1), [0, 0.5, 0]), new THREE.Vector3(-9, 1.5, 0));
  near(frame(rec, 1).scale, new THREE.Vector3(1.5, 1.5, 1.5));
  // sin(…· 3) = −1: scale 0.5.
  near(at(frame(rec, 3), [0, 0.5, 0]), new THREE.Vector3(-9, 0.5, 0));
});

test('a pulse without a pivot scales about its centre, with the schema defaults', () => {
  const rec = attach(record([2, 2, 2]), { Pulse: { frequency: 0.25 } });
  assert.equal(rec.resetPosition, false);
  near(frame(rec, 1).scale, new THREE.Vector3(1.1, 1.1, 1.1));
  near(frame(rec, 3).scale, new THREE.Vector3(0.9, 0.9, 0.9));
  near(rec.object.position, new THREE.Vector3(2, 2, 2));
});

test('the pivot is measured before the scale and turns with the authored rotation', () => {
  // A blade scaled ×2 and authored turned 90° about Y: the base is still
  // [0, −1, 0] in its own frame, 2 m below its centre in the parent's.
  const rec = attach(record([0, 2, 0], [2, 2, 2], [0, 90, 0]), { Spin: { axis: [0, 0, 1], speed: 90, pivot: [0, -1, 0] } });
  for (const t of [0, 0.5, 1.5]) near(at(frame(rec, t), [0, -1, 0]), new THREE.Vector3(0, 0, 0));
});

test('holdPivot moves an object by what a change did to the point', () => {
  const obj = new THREE.Object3D();
  obj.position.set(0, 1, 0);
  const q0 = obj.quaternion.clone();
  const s0 = obj.scale.clone();
  obj.quaternion.setFromAxisAngle(new THREE.Vector3(0, 0, 1), Math.PI / 2);
  holdPivot(obj, new THREE.Vector3(0, -1, 0), q0, s0);
  // The base, once at the origin, is still there.
  near(at(obj, [0, -1, 0]), new THREE.Vector3(0, 0, 0));
});

test('a kind the renderer does not know is skipped, never fatal', () => {
  const rec = record([0, 0, 0]);
  assert.equal(makeBehavior({ Wobble: { rate: 2 } }, rec, new Map(), new Map()), null);
  assert.equal(makeBehavior('Spin', rec, new Map(), new Map()), null);
});

test('a bob adds to what the behaviors before it left, so a wheel can bob on its axle', () => {
  // A quarter turn about the base moves the centre a metre left; a bob at its
  // crest lifts it half a metre from there — not from the authored position.
  const rec = attach(record([9, 1.5, 0]), { Spin: { axis: [0, 0, 1], speed: 90, pivot: [0, -1, 0] } });
  attach(rec, { Bob: { axis: [0, 1, 0], amplitude: 0.5, frequency: 0.25 } });
  near(frame(rec, 1).position, new THREE.Vector3(8, 1, 0));
  // Alone, from the authored position, and back to it at the trough.
  const alone = attach(record([2, 3, 4]), { Bob: { amplitude: 0.5, frequency: 0.25 } });
  near(frame(alone, 1).position, new THREE.Vector3(2, 3.5, 4));
  near(frame(alone, 3).position, new THREE.Vector3(2, 2.5, 4));
});
