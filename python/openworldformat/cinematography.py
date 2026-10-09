"""The ext-cinematography reference implementation (extension version 0.2), in Python.

Spec: spec/extensions/cinematography.md. An entity carrying
``ext-cinematography.camera`` is a camera — a filmback and a lens — and
one also carrying ``ext-cinematography.shot`` is a setup in the shot
list. What this module provides is the contract's executable half —

  - the normative crop math (frame size, horizontal and vertical FOV) —
    Unreal's crop-to-aspect rule written out: cropping never widens the
    frame past the sensor, it only trims;
  - the view (aim look-at, +Y up, else the entity's local −Z) and a
    pinhole projection, so "is it in frame" is a predicate;
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
    "view_of",
    "project",
    "shot_list",
    "run_outcomes",
]

#: The extension this module implements.
EXTENSION_NAME = "ext-cinematography"

#: The extension version this module implements.
EXTENSION_VERSION = "0.2.0"

#: The default sensor (filmback): Super 35, [w, h] in mm.
DEFAULT_SENSOR = (24.89, 18.66)

#: The default focal length, in mm.
DEFAULT_FOCAL_LENGTH = 35

_sub = lambda a, b: [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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


def camera_of(entity: dict) -> dict | None:
    """An entity's ``ext-cinematography.camera`` with defaults applied per
    absent field (``{}`` is a 35 mm on Super 35), or None when the entity
    carries no camera component.

    :param entity: a parsed entity
    :return: ``{"sensor_mm", "focal_length_mm", "aspect_ratio", "squeeze",
        "aim", "focus_distance_m", "f_stop"}`` or None
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
        "focus_distance_m": camera.get("focus_distance_m"),
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


def view_of(entity: dict, camera: dict) -> dict:
    """A camera's view: with ``aim``, the camera looks at the world point,
    +Y up; without it, it looks down the entity's local −Z (the glTF /
    three.js / Bevy convention), its frame carried by the entity's rotation.

    :param entity: the camera entity (its transform places it)
    :param camera: a :func:`camera_of` result
    :return: ``{"position", "forward", "right", "up"}``
    """
    transform = entity.get("transform")
    pos = transform.get("position") if isinstance(transform, dict) else None
    position = list(pos) if pos is not None else [0.0, 0.0, 0.0]
    if camera.get("aim") is not None:
        forward = _norm(_sub(camera["aim"], position))
        right = _norm(_cross(forward, [0, 1, 0]))
        up = _cross(right, forward)
        return {"position": position, "forward": forward, "right": right, "up": up}
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
        return {"entity": entity, "camera": camera, "frame": frame_of(camera), "view": view_of(entity, camera)}

    def origin_of(name):
        entity = by_name.get(name)
        if not entity:
            failures.append(f"no entity named {name}")
        transform = entity.get("transform") if entity else None
        pos = transform.get("position") if isinstance(transform, dict) else None
        return list(pos) if pos is not None else [0.0, 0.0, 0.0]

    for assertion in outcomes.get("expect") or []:
        fov = assertion.get("fov")
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
        elif isinstance(assertion.get("shot_list"), list):
            actual = [s["name"] for s in shot_list(manifest)]
            expected = assertion["shot_list"]
            if actual != expected:
                failures.append(f"shot list {json.dumps(actual)} != {json.dumps(expected)}")
        else:
            failures.append(f"unknown assertion {json.dumps(assertion)}")
    return {"ok": not failures, "failures": failures}
