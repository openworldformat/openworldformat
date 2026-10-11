"""The ext-cinematography reference implementation (extension version 0.3), in Python.

Spec: spec/extensions/cinematography.md. An entity carrying
``ext-cinematography.camera`` is a camera — a filmback and a lens — and
one also carrying ``ext-cinematography.shot`` is a setup in the shot
list. What this module provides is the contract's executable half —

  - the normative crop math (frame size, horizontal and vertical FOV) —
    Unreal's crop-to-aspect rule written out: cropping never widens the
    frame past the sensor, it only trims;
  - the view (aim look-at, +Y up, else the entity's local −Z) and a
    pinhole projection, so "is it in frame" is a predicate;
  - entity origins in world space (the document's TRS hierarchy) — the
    point every assertion projects, and the point ``aim_entity`` and
    ``focus_entity`` resolve to;
  - the shot list (shot.order, ties by entity id);
  - the outcome runner for conformance/outcomes/*.json.

Conformance is math, not pixels: implementations agree on the numbers,
within the stated tolerance — never on rendered output.
"""

from __future__ import annotations

import json
import math

__all__ = [
    "EXTENSION_NAME",
    "EXTENSION_VERSION",
    "DEFAULT_SENSOR",
    "DEFAULT_FOCAL_LENGTH",
    "camera_of",
    "frame_of",
    "entity_of",
    "world_origin",
    "aim_point_of",
    "focus_distance_of",
    "view_of",
    "project",
    "shot_list",
    "run_outcomes",
]

#: The extension this module implements.
EXTENSION_NAME = "ext-cinematography"

#: The extension version this module implements.
EXTENSION_VERSION = "0.3.0"

#: The default sensor (filmback): Super 35, [w, h] in mm.
DEFAULT_SENSOR = (24.89, 18.66)

#: The default focal length, in mm.
DEFAULT_FOCAL_LENGTH = 35

_sub = lambda a, b: [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
_add = lambda a, b: [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
_dot = lambda a, b: a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
_len = lambda a: math.sqrt(_dot(a, a))
_cross = lambda a, b: [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
]
_is_num = lambda v: isinstance(v, (int, float)) and not isinstance(v, bool)


def _norm(v: list) -> list:
    length = _len(v)
    return [x / length for x in v]


def _rotate_intrinsic_xyz(v: list, degrees: list) -> list:
    """Rotate ``v`` by intrinsic XYZ Euler degrees — R = Rx·Ry·Rz, the
    transform convention of spec/world.md "Conventions"."""
    rx, ry, rz = (math.radians(d) for d in degrees)
    x, y, z = v
    # Rz first (rightmost), then Ry, then Rx — R = Rx·Ry·Rz applied to v.
    c, s = math.cos(rz), math.sin(rz)
    x, y = c * x - s * y, s * x + c * y
    c, s = math.cos(ry), math.sin(ry)
    x, z = c * x + s * z, -s * x + c * z
    c, s = math.cos(rx), math.sin(rx)
    y, z = c * y - s * z, s * y + c * z
    return [x, y, z]


def _position_of(entity: dict) -> list:
    transform = entity.get("transform")
    pos = transform.get("position") if isinstance(transform, dict) else None
    return list(pos) if pos is not None else [0.0, 0.0, 0.0]


def camera_of(entity: dict) -> dict | None:
    """An entity's ``ext-cinematography.camera`` with defaults applied per
    absent field (``{}`` is a 35 mm on Super 35), or None when the entity
    carries no camera component.

    :param entity: a parsed entity
    :return: ``{"sensor_mm", "focal_length_mm", "aspect_ratio", "squeeze",
        "aim", "aim_entity", "focus_distance_m", "focus_entity", "f_stop"}``
        or None — ``aim_entity`` and ``focus_entity`` are entity
        references, the id (bound) or the name (at intake)
    """
    ext = entity.get(EXTENSION_NAME)
    camera = ext.get("camera") if isinstance(ext, dict) else None
    if not isinstance(camera, dict):
        return None
    sensor = camera.get("sensor_mm")
    return {
        "sensor_mm": list(sensor) if sensor is not None else list(DEFAULT_SENSOR),
        "focal_length_mm": camera.get("focal_length_mm") if camera.get("focal_length_mm") is not None else DEFAULT_FOCAL_LENGTH,
        "aspect_ratio": camera.get("aspect_ratio"),
        "squeeze": camera.get("squeeze") if camera.get("squeeze") is not None else 1,
        "aim": list(camera["aim"]) if camera.get("aim") is not None else None,
        "aim_entity": camera.get("aim_entity"),
        "focus_distance_m": camera.get("focus_distance_m"),
        "focus_entity": camera.get("focus_entity"),
        "f_stop": camera.get("f_stop"),
    }


def frame_of(camera: dict) -> dict:
    """The normative crop math (spec "Derived values"): with sensor
    ``w × h``, squeeze ``s``, focal length ``f`` and aspect ``a`` —
    desqueezed sensor aspect ``A = w·s / h``; frame ``W = w·s · min(1, a/A)``,
    ``H = h · min(1, A/a)`` (no aspect: the whole desqueezed sensor); FOVs
    ``2·atan(W / 2f)``, ``2·atan(H / 2f)``. Cropping never widens the frame
    past the sensor, it only trims.

    :param camera: a :func:`camera_of` result
    :return: ``{"width_mm", "height_mm", "hfov_degrees", "vfov_degrees", "aspect"}``
    """
    w, h = camera["sensor_mm"]
    s, f = camera["squeeze"], camera["focal_length_mm"]
    a = camera.get("aspect_ratio")
    A = w * s / h
    width_mm = w * s * min(1, a / A if a else 1)
    height_mm = h * min(1, A / a if a else 1)
    return {
        "width_mm": width_mm,
        "height_mm": height_mm,
        "hfov_degrees": math.degrees(2 * math.atan(width_mm / (2 * f))),
        "vfov_degrees": math.degrees(2 * math.atan(height_mm / (2 * f))),
        "aspect": width_mm / height_mm,
    }


def entity_of(reference, world: dict | None) -> dict | None:
    """The entity a reference names, in a world: by id (a bound reference,
    what a document holds) or by name (an authoring-time one, before
    ingestion binds it). None when nothing answers.

    :param reference: an id, a name, or None
    :param world: a manifest or a fold result (``{entities}``)
    """
    if reference is None or world is None:
        return None
    entities = world.get("entities") or []
    if _is_num(reference):
        return next((e for e in entities if e.get("id") == reference), None)
    if isinstance(reference, str):
        return next((e for e in entities if e.get("name") == reference), None)
    return None


def world_origin(entity: dict, world: dict) -> list:
    """An entity's origin in world space: its ``transform.position``
    composed through its parents — the document's TRS hierarchy,
    ``p = P + R·(S ⊙ p)`` up the chain (spec/world.md "Conventions"). An
    unparented entity's origin is its position as written; a parent the
    world doesn't hold ends the chain. This is the point every outcome
    assertion projects and the point ``aim_entity`` and ``focus_entity``
    resolve to.

    :param entity: a parsed entity
    :param world: the world it lives in (``{entities}``)
    :return: the world point
    """
    entities = world.get("entities") or []
    point = _position_of(entity)
    current = entity
    for _ in range(4096):
        parent_id = current.get("parent")
        if parent_id is None:
            break
        parent = next((e for e in entities if e.get("id") == parent_id), None)
        if parent is None:
            break
        transform = parent.get("transform") if isinstance(parent.get("transform"), dict) else {}
        P = list(transform.get("position") or [0.0, 0.0, 0.0])
        R = list(transform.get("rotation_degrees") or [0.0, 0.0, 0.0])
        S = list(transform.get("scale") or [1.0, 1.0, 1.0])
        point = _add(P, _rotate_intrinsic_xyz([point[0] * S[0], point[1] * S[1], point[2] * S[2]], R))
        current = parent
    return point


def aim_point_of(camera: dict, world: dict | None = None) -> list | None:
    """The point a camera aims at: ``aim_entity``'s world origin while
    that entity exists in the world, else ``aim``, else None (the camera
    looks down its local −Z). Without a world only ``aim`` can answer.

    :param camera: a :func:`camera_of` result
    :param world: the world the reference resolves in
    """
    target = entity_of(camera.get("aim_entity"), world)
    if target is not None and world is not None:
        return world_origin(target, world)
    aim = camera.get("aim")
    return list(aim) if aim is not None else None


def focus_distance_of(entity: dict, camera: dict, world: dict | None = None) -> float | None:
    """A camera's focus distance, metres: the distance from the camera
    entity's world origin to ``focus_entity``'s while that entity exists,
    else ``focus_distance_m``, else None.

    :param entity: the camera entity
    :param camera: a :func:`camera_of` result
    :param world: the world the reference resolves in
    """
    target = entity_of(camera.get("focus_entity"), world)
    if target is not None and world is not None:
        return _len(_sub(world_origin(target, world), world_origin(entity, world)))
    return camera.get("focus_distance_m")


def view_of(entity: dict, camera: dict, world: dict | None = None) -> dict:
    """A camera's view: with an aim point — ``aim_entity``'s origin when
    the world is given and the entity exists, else ``aim`` — the camera
    looks at it, +Y up; without one it looks down the entity's local −Z
    (the glTF / three.js / Bevy convention), its frame carried by the
    entity's rotation. The camera stands at its own world origin.

    :param entity: the camera entity (its transform places it)
    :param camera: a :func:`camera_of` result
    :param world: the world the entity references resolve in
    :return: ``{"position", "forward", "right", "up"}``
    """
    position = world_origin(entity, world) if world is not None else _position_of(entity)
    aim = aim_point_of(camera, world)
    if aim is not None:
        forward = _norm(_sub(aim, position))
        right = _norm(_cross(forward, [0, 1, 0]))
        up = _cross(right, forward)
        return {"position": position, "forward": forward, "right": right, "up": up}
    transform = entity.get("transform")
    rot = transform.get("rotation_degrees") if isinstance(transform, dict) else None
    rotation = list(rot) if rot is not None else [0.0, 0.0, 0.0]
    return {
        "position": position,
        "forward": _rotate_intrinsic_xyz([0, 0, -1], rotation),
        "right": _rotate_intrinsic_xyz([1, 0, 0], rotation),
        "up": _rotate_intrinsic_xyz([0, 1, 0], rotation),
    }


def project(view: dict, frame: dict, camera: dict, point: list) -> dict:
    """Project a world point through a camera: the point in the camera's
    normalized frame — ``x`` and ``y`` in frame-half units (inside when
    ``|x| <= 1`` and ``|y| <= 1``), ``z`` the distance along the look
    direction (in front when positive).

    :param view: a :func:`view_of` result
    :param frame: a :func:`frame_of` result
    :param camera: a :func:`camera_of` result
    :param point: the world point
    :return: ``{"x", "y", "z"}``
    """
    d = _sub(point, view["position"])
    z = _dot(d, view["forward"])
    scale = camera["focal_length_mm"] / z
    return {
        "x": (_dot(d, view["right"]) * scale) / (frame["width_mm"] / 2),
        "y": (_dot(d, view["up"]) * scale) / (frame["height_mm"] / 2),
        "z": z,
    }


def shot_list(world: dict) -> list:
    """The shot list: every entity carrying ``ext-cinematography.shot``, as
    ``{id, name, shot}``, ordered by ``shot.order`` (absent last), ties by
    entity id. The entity's name is the shot's name.

    :param world: a manifest or a fold result (``{entities}``)
    :return: the setups, in order
    """
    shots = []
    for entity in world.get("entities") or []:
        ext = entity.get(EXTENSION_NAME)
        shot = ext.get("shot") if isinstance(ext, dict) else None
        if not isinstance(shot, dict):
            continue
        shots.append({"id": entity.get("id"), "name": entity.get("name"), "shot": shot})

    def rank(s):
        order = s["shot"].get("order")
        return order if _is_num(order) else math.inf

    shots.sort(key=lambda s: (rank(s), s["id"]))
    return shots


def run_outcomes(manifest: dict, outcomes: dict) -> dict:
    """Run a conformance outcomes document against a world: derive, then
    check every assertion. This is the extension's conformance — math,
    not pixels.

    :param manifest: the world (parsed)
    :param outcomes: ``{expect: [...]}``
    :return: ``{"ok", "failures"}``
    """
    failures: list = []
    by_name = {e.get("name"): e for e in manifest.get("entities") or []}

    def camera_entity(name):
        entity = by_name.get(name)
        camera = camera_of(entity) if entity else None
        if not entity or not camera:
            failures.append(f"no camera named {name}")
            return None
        return {
            "entity": entity,
            "camera": camera,
            "frame": frame_of(camera),
            "view": view_of(entity, camera, manifest),
        }

    def origin_of(name):
        entity = by_name.get(name)
        if not entity:
            failures.append(f"no entity named {name}")
            return [0.0, 0.0, 0.0]
        return world_origin(entity, manifest)

    for assertion in outcomes.get("expect") or []:
        fov = assertion.get("fov")
        centred = assertion.get("centred")
        focus = assertion.get("focus")
        if isinstance(fov, dict):
            name = fov.get("camera")
            tolerance = fov.get("tolerance")
            tolerance = 0.001 if tolerance is None else tolerance
            c = camera_entity(name)
            if not c:
                continue
            frame = c["frame"]
            if abs(frame["hfov_degrees"] - fov["hfov_degrees"]) > tolerance:
                failures.append(f"{name}: hfov {frame['hfov_degrees']} != {fov['hfov_degrees']} (tolerance {tolerance})")
            if abs(frame["vfov_degrees"] - fov["vfov_degrees"]) > tolerance:
                failures.append(f"{name}: vfov {frame['vfov_degrees']} != {fov['vfov_degrees']} (tolerance {tolerance})")
            if abs(frame["aspect"] - fov["aspect"]) > tolerance:
                failures.append(f"{name}: aspect {frame['aspect']} != {fov['aspect']} (tolerance {tolerance})")
        elif assertion.get("in_frame") or assertion.get("out_of_frame"):
            target = assertion.get("in_frame") or assertion.get("out_of_frame")
            name, entity_name = target.get("camera"), target.get("entity")
            c = camera_entity(name)
            if not c:
                continue
            p = project(c["view"], c["frame"], c["camera"], origin_of(entity_name))
            inside = p["z"] > 0 and abs(p["x"]) <= 1 and abs(p["y"]) <= 1
            if assertion.get("in_frame") and not inside:
                failures.append(f"{name}: {entity_name} projects outside the frame ({p['x']:.3f}, {p['y']:.3f})")
            if assertion.get("out_of_frame") and inside:
                failures.append(f"{name}: {entity_name} projects inside the frame ({p['x']:.3f}, {p['y']:.3f})")
        elif isinstance(centred, dict):
            # The camera aims at the entity: its origin projects at the
            # frame's centre, in front — what ``aim_entity`` promises.
            name, entity_name = centred.get("camera"), centred.get("entity")
            tolerance = centred.get("tolerance")
            tolerance = 0.001 if tolerance is None else tolerance
            c = camera_entity(name)
            if not c:
                continue
            p = project(c["view"], c["frame"], c["camera"], origin_of(entity_name))
            if not (p["z"] > 0 and abs(p["x"]) <= tolerance and abs(p["y"]) <= tolerance):
                failures.append(
                    f"{name}: {entity_name} projects at ({p['x']:.4f}, {p['y']:.4f}), z {p['z']:.3f}"
                    f" — not the frame's centre (tolerance {tolerance})"
                )
        elif isinstance(focus, dict):
            # The derived focus distance — ``focus_entity``'s origin from
            # the camera, else ``focus_distance_m`` — within ``tolerance``.
            name = focus.get("camera")
            tolerance = focus.get("tolerance")
            tolerance = 0.001 if tolerance is None else tolerance
            c = camera_entity(name)
            if not c:
                continue
            actual = focus_distance_of(c["entity"], c["camera"], manifest)
            if actual is None:
                failures.append(f"{name}: no focus distance")
            elif abs(actual - focus["distance_m"]) > tolerance:
                failures.append(f"{name}: focus distance {actual} != {focus['distance_m']} (tolerance {tolerance})")
        elif isinstance(assertion.get("shot_list"), list):
            actual = [s["name"] for s in shot_list(manifest)]
            expected = assertion["shot_list"]
            if actual != expected:
                failures.append(f"shot list {json.dumps(actual)} != {json.dumps(expected)}")
        else:
            failures.append(f"unknown assertion {json.dumps(assertion)}")
    return {"ok": not failures, "failures": failures}
