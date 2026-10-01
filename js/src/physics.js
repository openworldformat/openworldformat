// The ext-physics reference implementation (extension version 0.1).
//
// Spec: spec/extensions/physics.md. The extension declares bodies, not
// simulation: how a world moves is an engine's business. What this file
// provides is the contract's executable half —
//
//   - a deliberately minimal deterministic solver (spheres against
//     floors and axis-aligned statics, fixed-timestep semi-implicit
//     Euler) so the conformance outcome assertions run anywhere and in
//     CI. It is reference-grade, not production physics;
//   - trajectory write/fold — the op kind that lets a device with no
//     solver scrub recorded motion instead of simulating it;
//   - the outcome runner for conformance/outcomes/*.json.
//
// The solver is deterministic by construction: no randomness, bodies in
// document order, fixed dt, IEEE-754 doubles. Same engine and version,
// same trajectory. Cross-engine, only the semantic contract holds —
// never bit-exact, exactly as the core spec refuses.

import { classifyOp } from "./index.js";

/** @typedef {import('./index.js').Vec3} Vec3 */
/** @typedef {import('./index.js').WorldEntity} WorldEntity */
/** @typedef {import('./index.js').WorldManifest} WorldManifest */
/** @typedef {import('./index.js').EnvironmentDef} EnvironmentDef */
/** @typedef {import('./index.js').LogEntry} LogEntry */

/** The extension this module implements. */
export const EXTENSION_NAME = "ext-physics";

/** The extension version this module implements. */
export const EXTENSION_VERSION = "0.1.0";

/** An impact at or above this speed (m/s) counts as a bounce. */
export const BOUNCE_SPEED = 0.5;

/** A dynamic body in contact moving slower than this (m/s) sleeps. */
export const SLEEP_SPEED = 0.05;

/** Default gravity when the environment block is absent (spec). */
const DEFAULT_GRAVITY = [0.0, -9.81, 0.0];

// Body-component defaults (spec: the extension page's field table).
const DEFAULTS = {
  mass: 1.0,
  restitution: 0.5,
  friction: 0.5,
  gravity_scale: 1.0,
  linear_damping: 0.0,
};

/**
 * An entity's `ext-physics` component: body kind, material parameters,
 * and an optional explicit collider (a sphere radius, a cuboid's
 * extents, or "shape" to use the parametric shape).
 * @typedef {Record<string, any> & {
 *   body?: "dynamic"|"kinematic"|"static",
 *   mass?: number, restitution?: number, friction?: number,
 *   gravity_scale?: number, linear_damping?: number, collider?: any
 * }} PhysicsComponent
 */

/**
 * A world to simulate: a manifest, or what foldLog/foldPath returned.
 * @typedef {Record<string, unknown> & {
 *   entities?: WorldEntity[], environment?: EnvironmentDef|null
 * }} PhysicsWorld
 */

/**
 * A dynamic body as the solver sees it.
 * @typedef {object} PhysicsBody
 * @property {number} id
 * @property {string} name
 * @property {Vec3} position
 * @property {Vec3} velocity
 * @property {number} radius the sphere the reference simulates
 * @property {number} mass
 * @property {number} restitution
 * @property {number} friction
 * @property {number} gravityScale
 * @property {number} damping
 * @property {boolean} asleep
 */

/** A static collider that is the floor at a y (a Plane). */
/** @typedef {{kind: "floor", id: number, name: string, y: number}} FloorCollider */

/** A static collider that is an axis-aligned box. */
/** @typedef {{kind: "box", id: number, name: string, min: Vec3, max: Vec3}} BoxCollider */

/** @typedef {FloorCollider|BoxCollider} StaticCollider */

/** A dynamic entity's collider: a sphere, or a box around its extents. */
/** @typedef {{kind: "sphere", radius: number}|{kind: "box", extents: Vec3}} EntityCollider */

/** One recorded impact. */
/**
 * @typedef {object} PhysicsContact
 * @property {number} t_s
 * @property {string} body
 * @property {string} other
 * @property {Vec3} position
 * @property {number} normal_speed
 */

/** One sampled moment, all dynamic bodies. */
/** @typedef {{t_s: number, bodies: Record<string, Vec3>}} PhysicsSample */

/** What simulatePhysics returns. */
/**
 * @typedef {object} SimulationResult
 * @property {PhysicsSample[]} samples at sample_dt_s, from t=0
 * @property {PhysicsContact[]} contacts every impact, in order
 * @property {{body: string, other: string, t_s: number}[]} bounces the impacts ≥ BOUNCE_SPEED
 * @property {Record<string, Vec3>} resting where each dynamic body ended up
 * @property {number} settled_s when all bodies slept (or when it stopped)
 */

/**
 * @param {number} x @param {number} y @param {number} z
 * @returns {Vec3}
 */
const v3 = (x, y, z) => [x, y, z];
/**
 * @param {number[]} a @param {number[]} b
 * @returns {Vec3}
 */
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
/** @param {number[]} a @param {number[]} b @returns {number} */
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
/** @param {number[]} a @returns {number} */
const len = (a) => Math.sqrt(dot(a, a));
/** @param {number} v @param {number} lo @param {number} hi @returns {number} */
const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** An entity's `ext-physics` component, or undefined.
 * @param {WorldEntity} entity
 * @returns {PhysicsComponent|undefined} */
function component(entity) {
  return /** @type {PhysicsComponent|undefined} */ (entity[EXTENSION_NAME]);
}

/** The entity's parametric shape as a bounding box [x, y, z] extents.
 * @param {WorldEntity} entity
 * @returns {Vec3|null} */
function shapeExtents(entity) {
  const s = entity.shape;
  if (!s || typeof s !== "object") return null;
  const e = Object.values(s)[0];
  if (!e) return null;
  if ("x" in e && "y" in e && "z" in e) return v3(e.x, e.y, e.z); // Cuboid, Wedge
  if ("radius" in e && "height" in e) return v3(2 * e.radius, e.height, 2 * e.radius); // Cylinder, Cone
  if ("radius" in e && "half_length" in e) {
    return v3(2 * e.radius, 2 * (e.half_length + e.radius), 2 * e.radius); // Capsule
  }
  if ("radius" in e) return v3(2 * e.radius, 2 * e.radius, 2 * e.radius); // Sphere, Tetrahedron, Icosahedron
  if ("major_radius" in e && "minor_radius" in e) {
    const w = 2 * (e.major_radius + e.minor_radius); // Torus, laid flat
    return v3(w, 2 * e.minor_radius, w);
  }
  if ("base_x" in e && "base_z" in e && "height" in e) {
    return v3(e.base_x, e.height, e.base_z); // Pyramid
  }
  return null;
}

/** An entity's collider: {kind:"sphere", radius} or {kind:"box", extents},
 * from the explicit collider when present, else the parametric shape.
 * @param {WorldEntity} entity
 * @returns {EntityCollider|null} */
function colliderOf(entity) {
  const c = component(entity)?.collider;
  if (c && typeof c === "object") {
    if (typeof c.sphere === "number") return { kind: "sphere", radius: c.sphere };
    if (Array.isArray(c.cuboid)) return { kind: "box", extents: c.cuboid };
  }
  const extents = shapeExtents(entity);
  if (c === "shape" || c === undefined) {
    if (extents) {
      return entity.shape && "Sphere" in entity.shape
        ? { kind: "sphere", radius: extents[0] / 2 }
        : { kind: "box", extents };
    }
    return null;
  }
  return null;
}

/**
 * Collect a world's physics: gravity, the dynamic bodies, and the
 * static colliders. Only entities carrying the component participate —
 * a body is declared, never inferred. Kinematic bodies are collected
 * for completeness; the reference treats them as static colliders (they
 * move by behaviors, which live outside this solver).
 *
 * @param {PhysicsWorld} state a manifest or a fold result ({entities, environment})
 * @returns {{gravity: Vec3, dynamic: PhysicsBody[], kinematic: {id: number, name: string}[], statics: StaticCollider[]}}
 */
export function collectPhysics(state) {
  const ext = /** @type {PhysicsComponent|undefined} */ (state.environment?.[EXTENSION_NAME]);
  const gravity = ext?.gravity ?? DEFAULT_GRAVITY;
  /** @type {PhysicsBody[]} */ const dynamic = [];
  /** @type {{id: number, name: string}[]} */ const kinematic = [];
  /** @type {StaticCollider[]} */ const statics = [];
  for (const entity of state.entities ?? []) {
    const c = component(entity);
    if (!c) continue;
    const position = /** @type {Vec3} */ ([...(entity.transform?.position ?? [0, 0, 0])]);
    const collider = colliderOf(entity);
    if (c.body === "dynamic") {
      // Dynamics are spheres in the reference: the collider's radius, or
      // the bounding sphere of whatever the entity declared.
      const extents = collider?.kind === "box" ? collider.extents : null;
      const radius = collider?.kind === "sphere"
        ? collider.radius
        : extents ? len(extents.map((x) => x / 2)) : 0.5;
      dynamic.push({
        id: entity.id,
        name: entity.name,
        position,
        velocity: v3(0, 0, 0),
        radius,
        mass: c.mass ?? DEFAULTS.mass,
        restitution: c.restitution ?? DEFAULTS.restitution,
        friction: c.friction ?? DEFAULTS.friction,
        gravityScale: c.gravity_scale ?? DEFAULTS.gravity_scale,
        damping: c.linear_damping ?? DEFAULTS.linear_damping,
        asleep: false,
      });
    } else if (c.body === "kinematic") {
      kinematic.push({ id: entity.id, name: entity.name });
      pushStatic(statics, entity, position, collider);
    } else {
      pushStatic(statics, entity, position, collider);
    }
  }
  return { gravity, dynamic, kinematic, statics };
}

/**
 * @param {StaticCollider[]} statics
 * @param {WorldEntity} entity
 * @param {Vec3} position
 * @param {EntityCollider|null} collider
 */
function pushStatic(statics, entity, position, collider) {
  // A Plane is the floor at its y (the reference ignores its rotation);
  // anything else is an axis-aligned box around its extents.
  if (entity.shape && "Plane" in entity.shape && collider?.kind !== "sphere") {
    statics.push({ id: entity.id, name: entity.name, kind: "floor", y: position[1] });
    return;
  }
  const extents = collider?.kind === "box"
    ? collider.extents
    : collider?.kind === "sphere"
      ? [2 * collider.radius, 2 * collider.radius, 2 * collider.radius]
      : shapeExtents(entity);
  if (!extents) return;
  const half = extents.map((x) => x / 2);
  statics.push({
    id: entity.id,
    name: entity.name,
    kind: "box",
    min: v3(position[0] - half[0], position[1] - half[1], position[2] - half[2]),
    max: v3(position[0] + half[0], position[1] + half[1], position[2] + half[2]),
  });
}

/** Resolve one body against one static. Returns the impact speed, or -1.
 * @param {PhysicsBody} body
 * @param {StaticCollider} stat
 * @returns {number} */
function resolveContact(body, stat) {
  /** @type {Vec3} */ let n;
  if (stat.kind === "floor") {
    const pen = stat.y + body.radius - body.position[1];
    if (pen <= 0) return -1;
    body.position[1] = stat.y + body.radius;
    n = v3(0, 1, 0);
  } else {
    const p = body.position;
    const c = v3(
      clamp(p[0], stat.min[0], stat.max[0]),
      clamp(p[1], stat.min[1], stat.max[1]),
      clamp(p[2], stat.min[2], stat.max[2]),
    );
    const d = sub(p, c);
    const dist = len(d);
    if (dist >= body.radius) return -1;
    n = dist > 0 ? /** @type {Vec3} */ (d.map((x) => x / dist)) : v3(0, 1, 0);
    body.position = [c[0] + n[0] * body.radius, c[1] + n[1] * body.radius, c[2] + n[2] * body.radius];
  }
  const vn = dot(body.velocity, n);
  if (vn >= 0) return 0; // resting against it, no impact
  // Reflect by restitution, damp tangentially by friction, one contact.
  const vt = sub(body.velocity, n.map((x) => x * vn));
  const restitution = -vn * body.restitution;
  const keep = 1 - body.friction;
  body.velocity = v3(
    n[0] * restitution + vt[0] * keep,
    n[1] * restitution + vt[1] * keep,
    n[2] * restitution + vt[2] * keep,
  );
  return -vn;
}

/**
 * Simulate a world: deterministic, fixed-timestep, semi-implicit Euler.
 * Spheres against floors and axis-aligned statics, bodies in document
 * order, sleep on rest. Contacts are recorded on impact (normal speed
 * ≥ SLEEP_SPEED); impacts at or above BOUNCE_SPEED are bounces.
 *
 * @param {PhysicsWorld} state a manifest or a fold result
 * @param {{until_s?: number, dt_s?: number, sample_dt_s?: number}} [opts]
 * @returns {SimulationResult}
 */
export function simulatePhysics(state, opts = {}) {
  const dt = opts.dt_s ?? 1 / 120;
  const until = opts.until_s ?? 5;
  const sampleDt = opts.sample_dt_s ?? 0.1;
  const { gravity, dynamic, statics } = collectPhysics(state);

  /** @type {PhysicsSample[]} */ const samples = [];
  /** @type {PhysicsContact[]} */ const contacts = [];
  /** @type {{body: string, other: string, t_s: number}[]} */ const bounces = [];
  let t = 0;
  let nextSample = 0;
  let allAsleepAt = null;

  const sample = () => {
    samples.push({
      t_s: Number(t.toFixed(4)),
      bodies: Object.fromEntries(dynamic.map((b) => [b.name, [...b.position]])),
    });
  };
  sample();

  while (t < until && allAsleepAt === null) {
    for (const body of dynamic) {
      if (body.asleep) continue;
      // Integrate, then resolve against every static in document order.
      const g = gravity.map((x) => x * body.gravityScale);
      const damp = body.damping * dt;
      body.velocity = v3(
        body.velocity[0] + (g[0] - damp * body.velocity[0]) * dt,
        body.velocity[1] + (g[1] - damp * body.velocity[1]) * dt,
        body.velocity[2] + (g[2] - damp * body.velocity[2]) * dt,
      );
      body.position = v3(
        body.position[0] + body.velocity[0] * dt,
        body.position[1] + body.velocity[1] * dt,
        body.position[2] + body.velocity[2] * dt,
      );
      let touching = false;
      for (const stat of statics) {
        const impact = resolveContact(body, stat);
        if (impact < 0) continue;
        touching = true;
        if (impact >= SLEEP_SPEED) {
          contacts.push({
            t_s: Number(t.toFixed(4)),
            body: body.name,
            other: stat.name,
            position: /** @type {Vec3} */ ([...body.position]),
            normal_speed: Number(impact.toFixed(4)),
          });
          if (impact >= BOUNCE_SPEED) {
            bounces.push({ body: body.name, other: stat.name, t_s: Number(t.toFixed(4)) });
          }
        }
      }
      if (touching && len(body.velocity) < SLEEP_SPEED) {
        body.velocity = v3(0, 0, 0);
        body.asleep = true;
      }
    }
    t += dt;
    if (dynamic.length > 0 && dynamic.every((b) => b.asleep) && allAsleepAt === null) {
      allAsleepAt = t;
    }
    if (t >= nextSample) {
      sample();
      nextSample += sampleDt;
    }
  }

  return {
    samples,
    contacts,
    bounces,
    resting: Object.fromEntries(dynamic.map((b) => [b.name, [...b.position]])),
    settled_s: allAsleepAt === null ? Number(t.toFixed(4)) : Number(allAsleepAt.toFixed(4)),
  };
}

/**
 * Fold the `ext-physics` trajectory ops out of a log: the sampled
 * transforms of dynamic bodies, for playback without a solver. The op
 * folds to nothing for the document; this reads what it carried.
 *
 * @param {LogEntry[]} entries parsed log entries, in order
 * @returns {{bodies: Record<string, {t_s: number, position: any}[]>, span_s: number}}
 */
export function foldTrajectories(entries) {
  /** @type {Record<string, {t_s: number, position: any}[]>} */ const bodies = {};
  let span = 0;
  for (const entry of entries) {
    const classified = entry.classified ?? entry.ops.map(classifyOp);
    for (const c of classified) {
      if (c.kind !== "extension" || c.name !== EXTENSION_NAME) continue;
      const { t_s, bodies: sampled } = c.value ?? {};
      if (!Array.isArray(t_s) || typeof sampled !== "object") continue;
      for (const [name, positions] of Object.entries(sampled)) {
        if (!Array.isArray(positions)) continue;
        const track = bodies[name] ?? (bodies[name] = []);
        for (let i = 0; i < positions.length && i < t_s.length; i++) {
          track.push({ t_s: t_s[i], position: positions[i] });
          if (t_s[i] > span) span = t_s[i];
        }
      }
    }
  }
  return { bodies, span_s: span };
}

/**
 * Build an `ext-physics` trajectory op from simulation samples (or any
 * per-body sample lists): the writer's half of playback. Sample at or
 * below 10 Hz, per the spec.
 *
 * @param {Pick<SimulationResult, "samples">} sim a simulatePhysics result
 * @returns {{"ext-physics": {t_s: number[], bodies: Record<string, Vec3[]>}}} the op — put it in an entry's ops array
 */
export function trajectoryOp(sim) {
  const t_s = sim.samples.map((s) => s.t_s);
  /** @type {Record<string, Vec3[]>} */ const bodies = {};
  for (const s of sim.samples) {
    for (const [name, p] of Object.entries(s.bodies)) {
      (bodies[name] ?? (bodies[name] = [])).push(p);
    }
  }
  return { [EXTENSION_NAME]: { t_s, bodies } };
}

/**
 * Run a conformance outcomes document against a world: simulate, then
 * check every assertion. This is the extension's conformance — outcome
 * predicates, not pixels.
 *
 * @param {PhysicsWorld} manifest the world (parsed)
 * @param {any} outcomes {simulate_s, options?, expect: [...]}
 * @returns {{ok: boolean, failures: string[], simulation: SimulationResult}}
 */
export function runOutcomes(manifest, outcomes) {
  const sim = simulatePhysics(manifest, {
    until_s: outcomes.simulate_s,
    ...(outcomes.options ?? {}),
  });
  /** @type {string[]} */ const failures = [];
  const nameOf = (/** @type {any} */ assertion) => JSON.stringify(assertion);

  for (const assertion of outcomes.expect ?? []) {
    if (Array.isArray(assertion.contact)) {
      const [a, b] = assertion.contact;
      const limit = assertion.within_s ?? Infinity;
      const hit = sim.contacts.some(
        (c) => c.t_s <= limit && ((c.body === a && c.other === b) || (c.body === b && c.other === a)),
      );
      if (!hit) failures.push(`contact ${a}/${b} within ${limit}s never happened`);
    } else if (assertion.rest) {
      const { body, near, tolerance = 0.15 } = assertion.rest;
      const at = sim.resting[body];
      const ok = at && len(sub(at, near)) <= tolerance;
      if (!ok) failures.push(`${body} rests at ${JSON.stringify(at)}, not within ${tolerance} of ${JSON.stringify(near)}`);
    } else if (assertion.bounces) {
      const { body, min } = assertion.bounces;
      const count = sim.bounces.filter((x) => x.body === body).length;
      if (count < min) failures.push(`${body} bounced ${count} times, expected at least ${min}`);
    } else {
      failures.push(`unknown assertion ${nameOf(assertion)}`);
    }
  }
  return { ok: failures.length === 0, failures, simulation: sim };
}
