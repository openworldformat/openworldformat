//! Declarative behavior definitions — data-driven entity animations.
//!
//! All 7 behavior types from the current gen crate are preserved exactly.
//! The only structural change is that entity references use [`EntityRef`]
//! instead of raw `String`.

use serde::{Deserialize, Serialize};

use crate::identity::EntityRef;

/// Declarative behavior definition — data, not code.
/// Each variant fully describes a continuous animation that the tick system evaluates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum BehaviorDef {
    /// Orbit around a center entity or point.
    Orbit {
        /// Entity to orbit around (mutually exclusive with `center_point`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "schema", schemars(extend("x-entity-ref" = "bindable")))]
        center: Option<EntityRef>,
        /// Fixed point to orbit around [x,y,z] (used if `center` is None).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        center_point: Option<[f32; 3]>,
        /// Orbit radius.
        #[serde(default = "default_orbit_radius")]
        radius: f32,
        /// Orbital speed in degrees per second.
        #[serde(default = "default_orbit_speed")]
        speed: f32,
        /// Orbit axis (normalized). Default: Y-up.
        #[serde(default = "default_y_axis")]
        axis: [f32; 3],
        /// Initial phase angle in degrees.
        #[serde(default)]
        phase: f32,
        /// Orbit tilt in degrees (inclination from the axis plane).
        #[serde(default)]
        tilt: f32,
    },
    /// Spin (rotate) around a local axis: the angle at time `t` is
    /// `speed × t + phase`, about `axis` through `pivot`.
    Spin {
        /// Local axis to spin around.
        #[serde(default = "default_y_axis")]
        axis: [f32; 3],
        /// Rotation speed in degrees per second.
        #[serde(default = "default_spin_speed")]
        speed: f32,
        /// Degrees added to the angle at time zero.
        #[serde(default = "default_phase", skip_serializing_if = "is_zero")]
        phase: f32,
        /// The point the entity turns about, in its local frame — the frame
        /// its shape is defined in, metres, before the transform's scale:
        /// the origin by default, `[0, -height/2, 0]` for a shape's base.
        /// The pivot stays where the authored transform put it.
        #[serde(default = "default_pivot", skip_serializing_if = "is_origin")]
        pivot: [f32; 3],
    },
    /// Bob up and down (sinusoidal oscillation along an axis).
    Bob {
        /// Axis of oscillation.
        #[serde(default = "default_y_axis")]
        axis: [f32; 3],
        /// Amplitude (distance from center in each direction).
        #[serde(default = "default_bob_amplitude")]
        amplitude: f32,
        /// Oscillation frequency in Hz.
        #[serde(default = "default_bob_frequency")]
        frequency: f32,
        /// Phase offset in degrees.
        #[serde(default)]
        phase: f32,
    },
    /// Continuously look at / follow another entity.
    LookAt {
        /// Entity to look at.
        #[cfg_attr(feature = "schema", schemars(extend("x-entity-ref" = "bindable")))]
        target: EntityRef,
    },
    /// Scale pulsation (breathing effect): the scale at time `t` is
    /// `min + (max − min) × (sin(2π × frequency × t + phase) + 1) / 2`,
    /// about `pivot`.
    Pulse {
        /// Minimum scale multiplier.
        #[serde(default = "default_pulse_min")]
        min_scale: f32,
        /// Maximum scale multiplier.
        #[serde(default = "default_pulse_max")]
        max_scale: f32,
        /// Pulse frequency in Hz.
        #[serde(default = "default_bob_frequency")]
        frequency: f32,
        /// Degrees added to the wave's argument at time zero.
        #[serde(default = "default_phase", skip_serializing_if = "is_zero")]
        phase: f32,
        /// The point the entity scales about, in its local frame — the frame
        /// its shape is defined in, metres, before the transform's scale:
        /// the origin by default, `[0, -height/2, 0]` to keep a shape's base
        /// on the ground. The pivot stays where the authored transform put it.
        #[serde(default = "default_pivot", skip_serializing_if = "is_origin")]
        pivot: [f32; 3],
    },
    /// Follow a path of waypoints in sequence.
    PathFollow {
        /// Ordered waypoints [[x,y,z], ...].
        waypoints: Vec<[f32; 3]>,
        /// Movement speed in units per second.
        #[serde(default = "default_path_speed")]
        speed: f32,
        /// Loop mode: "loop" wraps back to start, "ping_pong" reverses.
        #[serde(default = "default_path_mode")]
        mode: PathMode,
        /// Smoothly interpolate rotation toward movement direction.
        #[serde(default)]
        orient_to_path: bool,
    },
    /// Bouncing on a surface with gravity.
    Bounce {
        /// Height of initial/max bounce.
        #[serde(default = "default_bounce_height")]
        height: f32,
        /// Gravity acceleration (units/s^2).
        #[serde(default = "default_bounce_gravity")]
        gravity: f32,
        /// Energy retained per bounce (0.0-1.0).
        #[serde(default = "default_bounce_damping")]
        damping: f32,
        /// Surface Y level to bounce on.
        #[serde(default)]
        surface_y: f32,
    },
    /// Swing (a pendulum): a rotation about a local axis through a pivot
    /// that oscillates — the angle at time `t` is
    /// `amplitude × sin(2π × frequency × t + phase)` — a hanging sign, a
    /// bell, a seat on its ropes.
    Swing {
        /// Local axis to swing about. Default: X, a hinge across the entity.
        #[serde(default = "default_x_axis")]
        axis: [f32; 3],
        /// How far it swings each way, in degrees.
        #[serde(default = "default_swing_amplitude")]
        amplitude: f32,
        /// Swings per second, in Hz.
        #[serde(default = "default_bob_frequency")]
        frequency: f32,
        /// Degrees added to the wave's argument at time zero.
        #[serde(default = "default_phase", skip_serializing_if = "is_zero")]
        phase: f32,
        /// The point the entity swings about, in its local frame — the frame
        /// its shape is defined in, metres, before the transform's scale:
        /// the origin by default, `[0, height/2, 0]` for a shape's top.
        #[serde(default = "default_pivot", skip_serializing_if = "is_origin")]
        pivot: [f32; 3],
    },
    /// A behavior this version does not know: carried through the fold
    /// untouched and never run, so a world from a newer writer still opens
    /// (the must-ignore rule, spec/profiles.md). Not in the schema, which
    /// names only the kinds a reader can run.
    #[serde(untagged)]
    #[cfg_attr(feature = "schema", schemars(skip))]
    Other(serde_json::Value),
}

/// Path follow loop mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PathMode {
    #[default]
    Loop,
    PingPong,
    Once,
}

// ---- Default value helpers (match current gen crate exactly) ----

pub(crate) fn default_orbit_radius() -> f32 {
    5.0
}
pub(crate) fn default_orbit_speed() -> f32 {
    36.0
}
pub(crate) fn default_y_axis() -> [f32; 3] {
    [0.0, 1.0, 0.0]
}
pub(crate) fn default_spin_speed() -> f32 {
    90.0
}
pub(crate) fn default_x_axis() -> [f32; 3] {
    [1.0, 0.0, 0.0]
}
pub(crate) fn default_swing_amplitude() -> f32 {
    30.0
}
pub(crate) fn default_phase() -> f32 {
    0.0
}
pub(crate) fn default_pivot() -> [f32; 3] {
    [0.0, 0.0, 0.0]
}
/// A phase or an angle left at its default writes nothing, so a world
/// written before the field existed reads back to the same text.
pub(crate) fn is_zero(v: &f32) -> bool {
    *v == 0.0
}
/// A pivot at the origin writes nothing (see [`is_zero`]).
pub(crate) fn is_origin(v: &[f32; 3]) -> bool {
    *v == [0.0, 0.0, 0.0]
}
pub(crate) fn default_bob_amplitude() -> f32 {
    0.5
}
pub(crate) fn default_bob_frequency() -> f32 {
    0.5
}
pub(crate) fn default_pulse_min() -> f32 {
    0.9
}
pub(crate) fn default_pulse_max() -> f32 {
    1.1
}
pub(crate) fn default_path_speed() -> f32 {
    2.0
}
pub(crate) fn default_path_mode() -> PathMode {
    PathMode::Loop
}
pub(crate) fn default_bounce_height() -> f32 {
    3.0
}
pub(crate) fn default_bounce_gravity() -> f32 {
    9.8
}
pub(crate) fn default_bounce_damping() -> f32 {
    0.7
}

impl BehaviorDef {
    /// Returns the behavior kind as a string.
    pub fn kind(&self) -> &'static str {
        match self {
            BehaviorDef::Orbit { .. } => "orbit",
            BehaviorDef::Spin { .. } => "spin",
            BehaviorDef::Bob { .. } => "bob",
            BehaviorDef::LookAt { .. } => "look_at",
            BehaviorDef::Pulse { .. } => "pulse",
            BehaviorDef::PathFollow { .. } => "path_follow",
            BehaviorDef::Bounce { .. } => "bounce",
            BehaviorDef::Swing { .. } => "swing",
            BehaviorDef::Other(_) => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn behavior_orbit_roundtrip() {
        let b = BehaviorDef::Orbit {
            center: Some(EntityRef::name("sun")),
            center_point: None,
            radius: 10.0,
            speed: 45.0,
            axis: [0.0, 1.0, 0.0],
            phase: 0.0,
            tilt: 15.0,
        };
        let json = serde_json::to_string(&b).unwrap();
        let back: BehaviorDef = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn behavior_look_at_with_entity_ref() {
        let b = BehaviorDef::LookAt {
            target: EntityRef::name("player"),
        };
        let json = serde_json::to_string(&b).unwrap();
        assert!(json.contains("player"));
        let back: BehaviorDef = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn all_behavior_kinds() {
        let behaviors = [
            BehaviorDef::Orbit {
                center: None,
                center_point: Some([0.0, 0.0, 0.0]),
                radius: 5.0,
                speed: 36.0,
                axis: [0.0, 1.0, 0.0],
                phase: 0.0,
                tilt: 0.0,
            },
            BehaviorDef::Spin {
                axis: [0.0, 1.0, 0.0],
                speed: 90.0,
                phase: 0.0,
                pivot: [0.0; 3],
            },
            BehaviorDef::Bob {
                axis: [0.0, 1.0, 0.0],
                amplitude: 0.5,
                frequency: 0.5,
                phase: 0.0,
            },
            BehaviorDef::LookAt {
                target: EntityRef::name("cam"),
            },
            BehaviorDef::Pulse {
                min_scale: 0.9,
                max_scale: 1.1,
                frequency: 0.5,
                phase: 0.0,
                pivot: [0.0; 3],
            },
            BehaviorDef::PathFollow {
                waypoints: vec![[0.0, 0.0, 0.0], [5.0, 0.0, 0.0]],
                speed: 2.0,
                mode: PathMode::PingPong,
                orient_to_path: true,
            },
            BehaviorDef::Bounce {
                height: 3.0,
                gravity: 9.8,
                damping: 0.7,
                surface_y: 0.0,
            },
            BehaviorDef::Swing {
                axis: [1.0, 0.0, 0.0],
                amplitude: 30.0,
                frequency: 0.5,
                phase: 0.0,
                pivot: [0.0, 0.6, 0.0],
            },
        ];
        let kinds: Vec<&str> = behaviors.iter().map(|b| b.kind()).collect();
        assert_eq!(
            kinds,
            vec![
                "orbit",
                "spin",
                "bob",
                "look_at",
                "pulse",
                "path_follow",
                "bounce",
                "swing"
            ]
        );
    }

    #[test]
    fn spin_and_pulse_without_a_pivot_serialize_as_before() {
        // A behavior written before pivots and phases existed reads back
        // with the defaults, and writes back to the same text.
        let text = r#"{"Spin":{"axis":[0.0,1.0,0.0],"speed":90.0}}"#;
        let b: BehaviorDef = serde_json::from_str(text).unwrap();
        assert_eq!(
            b,
            BehaviorDef::Spin {
                axis: [0.0, 1.0, 0.0],
                speed: 90.0,
                phase: 0.0,
                pivot: [0.0; 3],
            }
        );
        assert_eq!(serde_json::to_string(&b).unwrap(), text);
        let pulse = BehaviorDef::Pulse {
            min_scale: 0.9,
            max_scale: 1.1,
            frequency: 0.5,
            phase: 0.0,
            pivot: [0.0; 3],
        };
        assert!(!serde_json::to_string(&pulse).unwrap().contains("pivot"));
        let wheel = BehaviorDef::Spin {
            axis: [0.0, 0.0, -1.0],
            speed: 45.0,
            phase: 90.0,
            pivot: [0.0, -1.0, 0.0],
        };
        let text = serde_json::to_string(&wheel).unwrap();
        assert!(text.contains("\"pivot\":[0.0,-1.0,0.0]"));
        let back: BehaviorDef = serde_json::from_str(&text).unwrap();
        assert_eq!(wheel, back);
    }

    #[test]
    fn swing_reads_with_its_defaults() {
        let b: BehaviorDef = serde_json::from_str(r#"{"Swing":{"pivot":[0.0,0.6,0.0]}}"#).unwrap();
        assert_eq!(
            b,
            BehaviorDef::Swing {
                axis: [1.0, 0.0, 0.0],
                amplitude: 30.0,
                frequency: 0.5,
                phase: 0.0,
                pivot: [0.0, 0.6, 0.0],
            }
        );
        assert_eq!(b.kind(), "swing");
    }

    #[test]
    fn a_kind_this_version_does_not_know_is_carried() {
        // The must-ignore rule for behaviors: a newer writer's kind reads
        // as `Other`, never runs, and writes back as it came — and the
        // entity around it still reads.
        let text = r#"{"Wobble":{"rate":2.0}}"#;
        let b: BehaviorDef = serde_json::from_str(text).unwrap();
        assert_eq!(b.kind(), "unknown");
        assert_eq!(serde_json::to_string(&b).unwrap(), text);
        let entity: crate::WorldEntity = serde_json::from_str(
            r#"{"id":1,"name":"a","behaviors":[{"Wobble":{"rate":2.0}},{"Spin":{"speed":10.0}}]}"#,
        )
        .unwrap();
        assert_eq!(entity.behaviors.len(), 2);
        assert_eq!(entity.behaviors[0].kind(), "unknown");
        assert_eq!(entity.behaviors[1].kind(), "spin");
    }

    #[test]
    fn path_mode_roundtrip() {
        for mode in [PathMode::Loop, PathMode::PingPong, PathMode::Once] {
            let json = serde_json::to_string(&mode).unwrap();
            let back: PathMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back);
        }
    }
}
