"""The soundtrack curves — the pure math half of the soundtrack block.

Spec: spec/world.md (the soundtrack section). A world's soundtrack is
curves over time: energy per second, beats at a bpm, sections as
fractions of the duration, modulations over a signal. Renderers and
audio engines consume them; here they are plain functions over plain
numbers — the notebook's half, no audio required.

Ported from the JS reference's pure-curve functions (``js/src/render.js``),
which is where the renderer keeps them because it is the only JS
consumer. Python's consumers are analysis, not playback.
"""

from __future__ import annotations

import math

__all__ = [
    "curve_at",
    "beat_at",
    "section_at",
    "modulation_factor",
]


def _clamp01(v: float) -> float:
    return min(1.0, max(0.0, v))


def _mod(a: float, n: float) -> float:
    return (a % n + n) % n


def _num(v) -> float | None:
    """A JSON number, or None — JavaScript's ``x > 0`` on undefined."""
    if isinstance(v, (int, float)) and not isinstance(v, bool):
        return v
    return None


def curve_at(curve: list | None, t: float) -> float:
    """``SoundtrackDef::energy_at`` / ``curve_at``: per-second curve, linear
    interpolation, clamped to [0, 1]. Time outside the curve holds its
    endpoints."""
    if not curve:
        return 0.0
    n = len(curve)
    if n == 1:
        return _clamp01(curve[0])
    tt = min(max(t, 0.0), n - 1)
    i = math.floor(tt)
    f = tt - i
    if i + 1 >= n:
        return _clamp01(curve[-1])
    return _clamp01(curve[i] + (curve[i + 1] - curve[i]) * f)


def beat_at(soundtrack: dict | None, t: float) -> float:
    """``SoundtrackDef::beat_at``: 1 on a beat, decaying linearly to 0 at
    the next. 0 when there is no bpm."""
    st = soundtrack if isinstance(soundtrack, dict) else {}
    bpm = _num(st.get("bpm"))
    if bpm is None or not bpm > 0:
        return 0.0
    period = 60 / bpm
    offset = _num(st.get("beat_offset"))
    since = _mod(t - (offset or 0.0), period)
    return 1 - since / period


def section_at(soundtrack: dict | None, t: float) -> int:
    """``SoundtrackDef::section_at``: the index of the section holding ``t``
    (sections are start fractions of the duration). 0 when there is no
    duration or no sections."""
    st = soundtrack if isinstance(soundtrack, dict) else {}
    sections = st.get("sections")
    sections = sections if isinstance(sections, list) else []
    duration = _num(st.get("duration"))
    if duration is None or not duration > 0 or not sections:
        return 0
    frac = _clamp01(t / duration)
    idx = 0
    for i in range(len(sections)):
        if sections[i] <= frac:
            idx = i
    return idx


def modulation_factor(definition: dict | None, signal: float) -> float:
    """``ModulationDef::factor``: map a [0, 1] signal onto ``range``."""
    d = definition if isinstance(definition, dict) else {}
    rng = d.get("range") or [1.0, 1.0]
    a, b = rng[0], rng[1]
    return a + (b - a) * _clamp01(signal)
