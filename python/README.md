# The Python reference (placeholder)

What belongs here: a pure-Python package — `parse_manifest`,
`fold_log`, `fold_path`, `fold_state`, trajectory folding, soundtrack
curves. Deliberately **no renderer**: this is the format's research
surface, not a viewer.

Who it is for: the origin niche pointed outward — embodied-agent
benchmarks (tasks as predicates over the state document, trajectories
in the format's own vocabulary), dataset tooling over session logs
(fold a thousand recordings, compute one number), and notebooks. None
of that wants three.js or Bevy; all of it wants `pip install
openworldformat` and a fold.

It would also be the fold contract's third independently-built
implementation — a good cross-check, since it shares no code or
toolchain with the JS and Rust references.

When it lands: a `pyproject.toml`, a stdlib-only core (json, math,
hashlib — the format is JSON all the way down), and CI running the
conformance worlds and outcome assertions the way the JS job does.
