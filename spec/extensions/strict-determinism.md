# Strict Determinism (`ext-strict-determinism`)

**Version: 0.1.0** (Experimental)

The Open World Format guarantees *semantic replay* by default, where triggers and outcomes occur identically but bit-exact reproduction of physics trajectories and pixels is not guaranteed across engines.

This extension opts a world into **bit-exact strict determinism**, designed for competitive gaming, speedrun validation, and strictly synchronized replays.

## Rules

When an engine claims support for this extension and parses a package requiring it:

1. **Math Pipeline:** All engine physics steps, raycasts, and constraint solvers MUST execute using fixed-point math or strictly pinned deterministic floating-point rules (IEEE 754 compliance without fast-math flags).
2. **Step Rate:** The simulation MUST tick at a strictly pinned framerate (e.g., exactly 60 Hz). Dropped frames must cause the engine to execute multiple logical ticks before rendering, preventing variable delta-time from drifting the simulation.
3. **RNG Contract:** Random number generation requested by triggers or behaviors MUST draw from a standardized PRNG (e.g. PCG32) seeded by the `package.json` `seed` property. Any procedural events must advance the PRNG state identically on all clients.
