# Cinematography (`ext-cinematography`)

**Version: 0.1.0** (Experimental)

While the core format relies on declarative triggers (`orbit`, `look_at`) for interactive worlds, AI filmmakers and machinima creators require strict temporal control over cameras and characters.

## Rules

This extension provides non-linear video editor (NLE) semantics directly in the log:

1. **Camera Splines:** Allows defining a camera's transform using a Bezier or Catmull-Rom spline evaluated strictly against the transport clock.
2. **Keyframed Animation Tracks:** Entities can carry `ext-cinematography` animation arrays that override interactive behaviors, pinning their translation, rotation, or scale to exact timestamps.
3. **Focus and Depth of Field:** Introduces a `focus_target` to cameras, dynamically calculating focal distance based on another entity's position to maintain cinematic depth of field.
