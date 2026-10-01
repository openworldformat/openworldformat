"""The ext-physics reference implementation (extension version 0.1), in Python.

Spec: spec/extensions/physics.md. The extension declares bodies, not
simulation: how a world moves is an engine's business. What this module
provides is the contract's executable half —

  - a deliberately minimal deterministic solver (spheres against
    floors and axis-aligned statics, fixed-timestep semi-implicit
    Euler) so the conformance outcome assertions run anywhere and in
    CI. It is reference-grade, not production physics;
  - trajectory write/fold — the op kind that lets a device with no
    solver scrub recorded motion instead of simulating it;
  - the outcome runner for conformance/outcomes/*.json.

The solver is deterministic by construction: no randomness, bodies in
document order, fixed dt, IEEE-754 doubles. Same engine and version,
same trajectory. Cross-engine, only the semantic contract holds —
never bit-exact, exactly as the core spec refuses.
"""

from __future__ import annotations

import json
import math

from . import classify_op

__all__ = [
    "EXTENSION_NAME",
    "EXTENSION_VERSION",
    "BOUNCE_SPEED",
    "SLEEP_SPEED",
    "collect_physics",
    "simulate_physics",
    "fold_trajectories",
    "trajectory_op",
    "run_outcomes",
]

#: The extension this module implements.
EXTENSION_NAME = "ext-physics"

#: The extension version this module implements.
EXTENSION_VERSION = "0.1.0"

#: An impact at or above this speed (m/s) counts as a bounce.
BOUNCE_SPEED = 0.5

#: A dynamic body in contact moving slower than this (m/s) sleeps.
SLEEP_SPEED = 0.05

#: Default gravity when the environment block is absent (spec).
DEFAULT_GRAVITY = (0.0, -9.81, 0.0)

# Body-component defaults (spec: the extension page's field table).
DEFAULTS = {
    "mass": 1.0,
    "restitution": 0.5,
    "friction": 0.5,
    "gravity_scale": 1.0,
    "linear_damping": 0.0,
}

_v3 = lambda x, y, z: [x, y, z]
_sub = lambda a, b: [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
_dot = lambda a, b: a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
_len = lambda a: math.sqrt(_dot(a, a))
_clamp = lambda v, lo, hi: min(hi, max(lo, v))
_is_num = lambda v: isinstance(v, (int, float)) and not isinstance(v, bool)


def _component(entity: dict):
    """An entity's ``ext-physics`` component, or None."""
    c = entity.get(EXTENSION_NAME)
    return c if isinstance(c, dict) else None


def _shape_extents(entity: dict):
    """The entity's parametric shape as a bounding box [x, y, z] extents."""
    s = entity.get("shape")
    if not isinstance(s, dict) or not s:
        return None
    e = next(iter(s.values()))
    if not isinstance(e, dict):
        return None
    if "x" in e and "y" in e and "z" in e:
        return _v3(e["x"], e["y"], e["z"])  # Cuboid, Wedge
    if "radius" in e and "height" in e:
        return _v3(2 * e["radius"], e["height"], 2 * e["radius"])  # Cylinder, Cone
    if "radius" in e and "half_length" in e:
        return _v3(2 * e["radius"], 2 * (e["half_length"] + e["radius"]), 2 * e["radius"])  # Capsule
    if "radius" in e:
        return _v3(2 * e["radius"], 2 * e["radius"], 2 * e["radius"])  # Sphere, Tetrahedron, Icosahedron
    if "major_radius" in e and "minor_radius" in e:
        w = 2 * (e["major_radius"] + e["minor_radius"])  # Torus, laid flat
        return _v3(w, 2 * e["minor_radius"], w)
    if "base_x" in e and "base_z" in e and "height" in e:
        return _v3(e["base_x"], e["height"], e["base_z"])  # Pyramid
    return None


def _collider_of(entity: dict):
    """An entity's collider: ``{"kind": "sphere", "radius"}`` or
    ``{"kind": "box", "extents"}``, from the explicit collider when
    present, else the parametric shape."""
    comp = _component(entity)
    c = comp.get("collider") if comp else None
    if isinstance(c, dict):
        if _is_num(c.get("sphere")):
            return {"kind": "sphere", "radius": c["sphere"]}
        if isinstance(c.get("cuboid"), list):
            return {"kind": "box", "extents": list(c["cuboid"])}
    extents = _shape_extents(entity)
    if c == "shape" or c is None:
        if extents:
            shape = entity.get("shape")
            if isinstance(shape, dict) and "Sphere" in shape:
                return {"kind": "sphere", "radius": extents[0] / 2}
            return {"kind": "box", "extents": extents}
        return None
    return None


def collect_physics(state: dict) -> dict:
    """Collect a world's physics: gravity, the dynamic bodies, and the
    static colliders. Only entities carrying the component participate —
    a body is declared, never inferred. Kinematic bodies are collected
    for completeness; the reference treats them as static colliders (they
    move by behaviors, which live outside this solver).

    :param state: a manifest or a fold result (``{entities, environment}``)
    :return: ``{"gravity", "dynamic", "kinematic", "statics"}``
    """
    env = state.get("environment")
    env = env if isinstance(env, dict) else {}
    ext = env.get(EXTENSION_NAME)
    ext = ext if isinstance(ext, dict) else {}
    gravity = ext.get("gravity")
    if gravity is None:
        gravity = list(DEFAULT_GRAVITY)

    dynamic: list = []
    kinematic: list = []
    statics: list = []
    for entity in state.get("entities") or []:
        c = _component(entity)
        if c is None:
            continue
        transform = entity.get("transform")
        pos = transform.get("position") if isinstance(transform, dict) else None
        position = list(pos) if pos is not None else [0.0, 0.0, 0.0]
        collider = _collider_of(entity)
        if c.get("body") == "dynamic":
            # Dynamics are spheres in the reference: the collider's radius, or
            # the bounding sphere of whatever the entity declared.
            extents = collider["extents"] if collider and collider["kind"] == "box" else None
            if collider and collider["kind"] == "sphere":
                radius = collider["radius"]
            elif extents:
                radius = _len([x / 2 for x in extents])
            else:
                radius = 0.5
            dynamic.append({
                "id": entity.get("id"),
                "name": entity.get("name"),
                "position": position,
                "velocity": _v3(0.0, 0.0, 0.0),
                "radius": radius,
                "mass": c.get("mass") if c.get("mass") is not None else DEFAULTS["mass"],
                "restitution": c.get("restitution") if c.get("restitution") is not None else DEFAULTS["restitution"],
                "friction": c.get("friction") if c.get("friction") is not None else DEFAULTS["friction"],
                "gravity_scale": c.get("gravity_scale") if c.get("gravity_scale") is not None else DEFAULTS["gravity_scale"],
                "damping": c.get("linear_damping") if c.get("linear_damping") is not None else DEFAULTS["linear_damping"],
                "asleep": False,
            })
        elif c.get("body") == "kinematic":
            kinematic.append({"id": entity.get("id"), "name": entity.get("name")})
            _push_static(statics, entity, position, collider)
        else:
            _push_static(statics, entity, position, collider)
    return {"gravity": gravity, "dynamic": dynamic, "kinematic": kinematic, "statics": statics}


def _push_static(statics: list, entity: dict, position: list, collider) -> None:
    # A Plane is the floor at its y (the reference ignores its rotation);
    # anything else is an axis-aligned box around its extents.
    shape = entity.get("shape")
    if isinstance(shape, dict) and "Plane" in shape and (collider is None or collider["kind"] != "sphere"):
        statics.append({"id": entity.get("id"), "name": entity.get("name"), "kind": "floor", "y": position[1]})
        return
    if collider and collider["kind"] == "box":
        extents = collider["extents"]
    elif collider and collider["kind"] == "sphere":
        extents = [2 * collider["radius"], 2 * collider["radius"], 2 * collider["radius"]]
    else:
        extents = _shape_extents(entity)
    if not extents:
        return
    half = [x / 2 for x in extents]
    statics.append({
        "id": entity.get("id"),
        "name": entity.get("name"),
        "kind": "box",
        "min": _v3(position[0] - half[0], position[1] - half[1], position[2] - half[2]),
        "max": _v3(position[0] + half[0], position[1] + half[1], position[2] + half[2]),
    })


def _resolve_contact(body: dict, stat: dict) -> float:
    """Resolve one body against one static. Returns the impact speed, or -1."""
    if stat["kind"] == "floor":
        pen = stat["y"] + body["radius"] - body["position"][1]
        if pen <= 0:
            return -1
        body["position"][1] = stat["y"] + body["radius"]
        n = _v3(0.0, 1.0, 0.0)
    else:
        p = body["position"]
        c = _v3(
            _clamp(p[0], stat["min"][0], stat["max"][0]),
            _clamp(p[1], stat["min"][1], stat["max"][1]),
            _clamp(p[2], stat["min"][2], stat["max"][2]),
        )
        d = _sub(p, c)
        dist = _len(d)
        if dist >= body["radius"]:
            return -1
        n = [x / dist for x in d] if dist > 0 else _v3(0.0, 1.0, 0.0)
        body["position"] = [c[0] + n[0] * body["radius"], c[1] + n[1] * body["radius"], c[2] + n[2] * body["radius"]]
    vn = _dot(body["velocity"], n)
    if vn >= 0:
        return 0  # resting against it, no impact
    # Reflect by restitution, damp tangentially by friction, one contact.
    vt = _sub(body["velocity"], [x * vn for x in n])
    restitution = -vn * body["restitution"]
    keep = 1 - body["friction"]
    body["velocity"] = _v3(
        n[0] * restitution + vt[0] * keep,
        n[1] * restitution + vt[1] * keep,
        n[2] * restitution + vt[2] * keep,
    )
    return -vn


def simulate_physics(state: dict, opts: dict | None = None) -> dict:
    """Simulate a world: deterministic, fixed-timestep, semi-implicit Euler.
    Spheres against floors and axis-aligned statics, bodies in document
    order, sleep on rest. Contacts are recorded on impact (normal speed
    ≥ SLEEP_SPEED); impacts at or above BOUNCE_SPEED are bounces.

    :param state: a manifest or a fold result
    :param opts: ``{until_s, dt_s, sample_dt_s}``
    :return: ``{"samples", "contacts", "bounces", "resting", "settled_s"}``
    """
    opts = opts or {}
    dt = opts.get("dt_s")
    dt = 1 / 120 if dt is None else dt
    until = opts.get("until_s")
    until = 5 if until is None else until
    sample_dt = opts.get("sample_dt_s")
    sample_dt = 0.1 if sample_dt is None else sample_dt
    collected = collect_physics(state)
    gravity, dynamic, statics = collected["gravity"], collected["dynamic"], collected["statics"]

    samples: list = []
    contacts: list = []
    bounces: list = []
    t = 0.0
    next_sample = 0.0
    all_asleep_at = None

    def sample(at: float) -> None:
        samples.append({
            "t_s": round(at, 4),
            "bodies": {b["name"]: list(b["position"]) for b in dynamic},
        })

    sample(t)

    while t < until and all_asleep_at is None:
        for body in dynamic:
            if body["asleep"]:
                continue
            # Integrate, then resolve against every static in document order.
            g = [x * body["gravity_scale"] for x in gravity]
            damp = body["damping"] * dt
            v = body["velocity"]
            body["velocity"] = _v3(
                v[0] + (g[0] - damp * v[0]) * dt,
                v[1] + (g[1] - damp * v[1]) * dt,
                v[2] + (g[2] - damp * v[2]) * dt,
            )
            p = body["position"]
            v = body["velocity"]
            body["position"] = _v3(p[0] + v[0] * dt, p[1] + v[1] * dt, p[2] + v[2] * dt)
            touching = False
            for stat in statics:
                impact = _resolve_contact(body, stat)
                if impact < 0:
                    continue
                touching = True
                if impact >= SLEEP_SPEED:
                    contacts.append({
                        "t_s": round(t, 4),
                        "body": body["name"],
                        "other": stat["name"],
                        "position": list(body["position"]),
                        "normal_speed": round(impact, 4),
                    })
                    if impact >= BOUNCE_SPEED:
                        bounces.append({"body": body["name"], "other": stat["name"], "t_s": round(t, 4)})
            if touching and _len(body["velocity"]) < SLEEP_SPEED:
                body["velocity"] = _v3(0.0, 0.0, 0.0)
                body["asleep"] = True
        t += dt
        if dynamic and all(b["asleep"] for b in dynamic) and all_asleep_at is None:
            all_asleep_at = t
        if t >= next_sample:
            sample(t)
            next_sample += sample_dt

    return {
        "samples": samples,
        "contacts": contacts,
        "bounces": bounces,
        "resting": {b["name"]: list(b["position"]) for b in dynamic},
        "settled_s": round(all_asleep_at if all_asleep_at is not None else t, 4),
    }


def fold_trajectories(entries: list) -> dict:
    """Fold the ``ext-physics`` trajectory ops out of a log: the sampled
    transforms of dynamic bodies, for playback without a solver. The op
    folds to nothing for the document; this reads what it carried.

    :param entries: parsed log entries, in order
    :return: ``{"bodies": {name: [{"t_s", "position"}]}, "span_s": float}``
    """
    bodies: dict = {}
    span = 0.0
    for entry in entries:
        classified = entry.get("classified")
        if classified is None:
            classified = [classify_op(op) for op in entry.get("ops") or []]
        for c in classified:
            if c["kind"] != "extension" or c["name"] != EXTENSION_NAME:
                continue
            value = c["value"] or {}
            t_s, sampled = value.get("t_s"), value.get("bodies")
            if not isinstance(t_s, list) or not isinstance(sampled, dict):
                continue
            for name, positions in sampled.items():
                if not isinstance(positions, list):
                    continue
                track = bodies.setdefault(name, [])
                for i in range(min(len(positions), len(t_s))):
                    track.append({"t_s": t_s[i], "position": positions[i]})
                    if t_s[i] > span:
                        span = t_s[i]
    return {"bodies": bodies, "span_s": span}


def trajectory_op(sim: dict) -> dict:
    """Build an ``ext-physics`` trajectory op from simulation samples (or any
    per-body sample lists): the writer's half of playback. Sample at or
    below 10 Hz, per the spec.

    :param sim: a :func:`simulate_physics` result
    :return: the op — put it in an entry's ops array
    """
    t_s = [s["t_s"] for s in sim["samples"]]
    bodies: dict = {}
    for s in sim["samples"]:
        for name, p in s["bodies"].items():
            bodies.setdefault(name, []).append(p)
    return {EXTENSION_NAME: {"t_s": t_s, "bodies": bodies}}


def run_outcomes(manifest: dict, outcomes: dict) -> dict:
    """Run a conformance outcomes document against a world: simulate, then
    check every assertion. This is the extension's conformance — outcome
    predicates, not pixels.

    :param manifest: the world (parsed)
    :param outcomes: ``{simulate_s, options?, expect: [...]}``
    :return: ``{"ok", "failures", "simulation"}``
    """
    opts = {"until_s": outcomes.get("simulate_s")}
    opts.update(outcomes.get("options") or {})
    sim = simulate_physics(manifest, opts)
    failures: list = []

    for assertion in outcomes.get("expect") or []:
        contact = assertion.get("contact")
        if isinstance(contact, list):
            a, b = contact[0], contact[1]
            limit = assertion.get("within_s")
            limit = math.inf if limit is None else limit
            hit = any(
                c["t_s"] <= limit and ((c["body"] == a and c["other"] == b) or (c["body"] == b and c["other"] == a))
                for c in sim["contacts"]
            )
            if not hit:
                failures.append(f"contact {a}/{b} within {limit}s never happened")
        elif "rest" in assertion:
            rest = assertion["rest"]
            body, near = rest.get("body"), rest.get("near")
            tolerance = rest.get("tolerance")
            tolerance = 0.15 if tolerance is None else tolerance
            at = sim["resting"].get(body)
            ok = at is not None and _len(_sub(at, near)) <= tolerance
            if not ok:
                failures.append(
                    f"{body} rests at {json.dumps(at)}, not within {tolerance} of {json.dumps(near)}"
                )
        elif "bounces" in assertion:
            want = assertion["bounces"]
            body, minimum = want.get("body"), want.get("min")
            count = sum(1 for x in sim["bounces"] if x["body"] == body)
            if count < minimum:
                failures.append(f"{body} bounced {count} times, expected at least {minimum}")
        else:
            failures.append(f"unknown assertion {json.dumps(assertion)}")
    return {"ok": not failures, "failures": failures, "simulation": sim}
