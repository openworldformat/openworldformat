//! # openworldformat::cinematography
//!
//! The `ext-cinematography` extension's reference implementation (the
//! JS reference is `openworldformat/cinematography`; the extension's
//! spec is `openworldformat/spec/extensions/cinematography.md`). An
//! entity carrying `ext-cinematography.camera` is a camera — a filmback
//! and a lens — and one also carrying `ext-cinematography.shot` is a
//! setup in the shot list. What this module is:
//!
//! - [`CameraComponent::of`] / [`frame_of`] — the camera declaration,
//!   and the normative crop math (Unreal's crop-to-aspect rule written
//!   out: cropping never widens the frame past the sensor, it trims);
//! - [`view_of`] / [`project`] — the aim look-at (+Y up, else the
//!   entity's local −Z) and a pinhole projection, so "is it in frame"
//!   is a predicate;
//! - [`shot_list`] — the setups, ordered for the clock;
//! - [`run_outcomes`] — the conformance outcome runner: one assertion
//!   file, any implementation, the same predicates.
//!
//! Conformance is math, not pixels: implementations agree on the
//! numbers, within the stated tolerance — never on rendered output.

use serde::Deserialize;

use crate as wt;

/// The extension this module implements.
pub const EXTENSION_NAME: &str = "ext-cinematography";

/// The extension version this module implements.
pub const EXTENSION_VERSION: &str = "0.2.0";

/// The default sensor (filmback): Super 35, `[w, h]` in mm.
pub const DEFAULT_SENSOR: [f64; 2] = [24.89, 18.66];

/// The default focal length, in mm.
pub const DEFAULT_FOCAL_LENGTH: f64 = 35.0;

// ---------------------------------------------------------------------------
// The camera, the frame, the view
// ---------------------------------------------------------------------------

/// One entity's `ext-cinematography.camera` component, parsed from
/// `WorldEntity::extra` with every default applied.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraComponent {
    /// Sensor (filmback) `[w, h]`, mm.
    pub sensor_mm: [f64; 2],
    /// The lens, mm.
    pub focal_length_mm: f64,
    /// Crop to this frame aspect (w/h); absent keeps the desqueezed
    /// sensor's.
    pub aspect_ratio: Option<f64>,
    /// Anamorphic desqueeze.
    pub squeeze: f64,
    /// Look at this world point, +Y up; absent looks down the entity's
    /// local −Z (the glTF / three.js / Bevy convention).
    pub aim: Option<[f64; 3]>,
    /// Focus distance, m.
    pub focus_distance_m: Option<f64>,
    /// The f-stop.
    pub f_stop: Option<f64>,
}

impl CameraComponent {
    /// Parse the component from an entity's `extra` map, defaults per
    /// absent field (`{}` is a 35 mm on Super 35). `None` when the
    /// entity declares no camera.
    pub fn of(entity: &wt::WorldEntity) -> Option<Self> {
        let camera = entity.extra.get(EXTENSION_NAME)?.get("camera")?.as_object()?;
        let num = |k: &str, d: f64| camera.get(k).and_then(serde_json::Value::as_f64).unwrap_or(d);
        let opt = |k: &str| camera.get(k).and_then(serde_json::Value::as_f64);
        let vec2 = |k: &str| -> Option<[f64; 2]> {
            let a: Vec<f64> = camera
                .get(k)?
                .as_array()?
                .iter()
                .filter_map(serde_json::Value::as_f64)
                .collect();
            (a.len() == 2).then_some([a[0], a[1]])
        };
        let vec3 = |k: &str| -> Option<[f64; 3]> {
            let a: Vec<f64> = camera
                .get(k)?
                .as_array()?
                .iter()
                .filter_map(serde_json::Value::as_f64)
                .collect();
            (a.len() == 3).then_some([a[0], a[1], a[2]])
        };
        Some(CameraComponent {
            sensor_mm: vec2("sensor_mm").unwrap_or(DEFAULT_SENSOR),
            focal_length_mm: num("focal_length_mm", DEFAULT_FOCAL_LENGTH),
            aspect_ratio: opt("aspect_ratio"),
            squeeze: num("squeeze", 1.0),
            aim: vec3("aim"),
            focus_distance_m: opt("focus_distance_m"),
            f_stop: opt("f_stop"),
        })
    }
}

/// A camera's frame: the largest rectangle of the aspect inside the
/// desqueezed sensor, centred, and the fields of view it gives the lens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraFrame {
    /// Frame width (desqueezed), mm.
    pub width_mm: f64,
    /// Frame height, mm.
    pub height_mm: f64,
    /// Horizontal field of view, degrees.
    pub hfov_degrees: f64,
    /// Vertical field of view, degrees.
    pub vfov_degrees: f64,
    /// Frame width / height.
    pub aspect: f64,
}

/// The normative crop math (spec "Derived values"): with sensor `w × h`,
/// squeeze `s`, focal length `f` and aspect `a` — desqueezed sensor
/// aspect `A = w·s / h`; frame `W = w·s · min(1, a/A)`,
/// `H = h · min(1, A/a)` (no aspect: the whole desqueezed sensor); FOVs
/// `2·atan(W / 2f)`, `2·atan(H / 2f)`. Cropping never widens the frame
/// past the sensor, it only trims.
pub fn frame_of(camera: &CameraComponent) -> CameraFrame {
    let [w, h] = camera.sensor_mm;
    let s = camera.squeeze;
    let f = camera.focal_length_mm;
    let a = w * s / h;
    let (width_mm, height_mm) = match camera.aspect_ratio {
        Some(aspect) if aspect != 0.0 => (
            w * s * (aspect / a).min(1.0),
            h * (a / aspect).min(1.0),
        ),
        _ => (w * s, h),
    };
    let deg = |rad: f64| rad * 180.0 / std::f64::consts::PI;
    CameraFrame {
        width_mm,
        height_mm,
        hfov_degrees: deg(2.0 * (width_mm / (2.0 * f)).atan()),
        vfov_degrees: deg(2.0 * (height_mm / (2.0 * f)).atan()),
        aspect: width_mm / height_mm,
    }
}

/// A camera's view: where it stands and which way it looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraView {
    /// World point.
    pub position: [f64; 3],
    /// Unit vector, the look direction.
    pub forward: [f64; 3],
    /// Unit vector.
    pub right: [f64; 3],
    /// Unit vector.
    pub up: [f64; 3],
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(v: [f64; 3]) -> [f64; 3] {
    let l = dot(v, v).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

/// Rotate `v` by intrinsic XYZ Euler degrees — R = Rx·Ry·Rz, the
/// transform convention of spec/world.md "Conventions".
fn rotate_intrinsic_xyz(v: [f64; 3], degrees: [f64; 3]) -> [f64; 3] {
    let rad = |d: f64| d * std::f64::consts::PI / 180.0;
    let (rx, ry, rz) = (rad(degrees[0]), rad(degrees[1]), rad(degrees[2]));
    let [x, y, z] = v;
    // Rz first (rightmost), then Ry, then Rx — R = Rx·Ry·Rz applied to v.
    let (c, s) = (rz.cos(), rz.sin());
    let (x, y) = (c * x - s * y, s * x + c * y);
    let (c, s) = (ry.cos(), ry.sin());
    let (x, z) = (c * x + s * z, -s * x + c * z);
    let (c, s) = (rx.cos(), rx.sin());
    let (y, z) = (c * y - s * z, s * y + c * z);
    [x, y, z]
}

/// A camera's view: with `aim`, the camera looks at the world point, +Y
/// up; without it, it looks down the entity's local −Z, its frame
/// carried by the entity's rotation.
pub fn view_of(entity: &wt::WorldEntity, camera: &CameraComponent) -> CameraView {
    let position: [f64; 3] = entity.transform.position.map(|p| p as f64);
    if let Some(aim) = camera.aim {
        let forward = norm(sub(aim, position));
        let right = norm(cross(forward, [0.0, 1.0, 0.0]));
        let up = cross(right, forward);
        return CameraView {
            position,
            forward,
            right,
            up,
        };
    }
    let rotation: [f64; 3] = entity.transform.rotation_degrees.map(|r| r as f64);
    CameraView {
        position,
        forward: rotate_intrinsic_xyz([0.0, 0.0, -1.0], rotation),
        right: rotate_intrinsic_xyz([1.0, 0.0, 0.0], rotation),
        up: rotate_intrinsic_xyz([0.0, 1.0, 0.0], rotation),
    }
}

/// A world point in a camera's normalized frame (see [`project`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    /// Frame-half units; inside when `|x| <= 1`.
    pub x: f64,
    /// Frame-half units; inside when `|y| <= 1`.
    pub y: f64,
    /// Distance along the look direction; in front when positive.
    pub z: f64,
}

/// Project a world point through a camera: the point in the camera's
/// normalized frame — `x` and `y` in frame-half units (inside when
/// `|x| <= 1` and `|y| <= 1`), `z` the distance along the look direction
/// (in front when positive).
pub fn project(
    view: &CameraView,
    frame: &CameraFrame,
    camera: &CameraComponent,
    point: [f64; 3],
) -> Projection {
    let d = sub(point, view.position);
    let z = dot(d, view.forward);
    let scale = camera.focal_length_mm / z;
    Projection {
        x: (dot(d, view.right) * scale) / (frame.width_mm / 2.0),
        y: (dot(d, view.up) * scale) / (frame.height_mm / 2.0),
        z,
    }
}

// ---------------------------------------------------------------------------
// The shot list
// ---------------------------------------------------------------------------

/// A setup in the shot list: an entity carrying
/// `ext-cinematography.shot`, and what the shot says.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotEntry {
    /// The entity's id.
    pub id: u64,
    /// The entity's name — the shot's name.
    pub name: String,
    /// The `shot` object, as declared.
    pub shot: serde_json::Value,
}

/// The shot list: every entity carrying `ext-cinematography.shot`,
/// ordered by `shot.order` (absent last), ties by entity id.
pub fn shot_list(manifest: &wt::WorldManifest) -> Vec<ShotEntry> {
    let mut shots: Vec<ShotEntry> = manifest
        .entities
        .iter()
        .filter_map(|entity| {
            let shot = entity.extra.get(EXTENSION_NAME)?.get("shot")?;
            if !shot.is_object() {
                return None;
            }
            Some(ShotEntry {
                id: entity.id.0,
                name: entity.name.as_str().to_string(),
                shot: shot.clone(),
            })
        })
        .collect();
    let rank =
        |s: &ShotEntry| s.shot.get("order").and_then(serde_json::Value::as_f64).unwrap_or(f64::MAX);
    shots.sort_by(|a, b| {
        rank(a)
            .partial_cmp(&rank(b))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    shots
}

// ---------------------------------------------------------------------------
// Outcome assertions — the extension's conformance
// ---------------------------------------------------------------------------

/// A conformance outcomes document: derived-math assertions, not
/// pixels. `expect` holds the assertions.
#[derive(Debug, Clone, Deserialize)]
pub struct OutcomesDoc {
    /// The world the assertions belong to (a conformance path; the
    /// runner's caller resolves it).
    #[serde(default)]
    pub world: String,
    /// The assertions.
    pub expect: Vec<Assertion>,
}

/// One assertion. The shapes match the conformance JSON: `fov`,
/// `in_frame`, `out_of_frame`, `shot_list`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Assertion {
    /// A camera's derived FOVs and frame aspect, within `tolerance`.
    Fov {
        /// The fov assertion.
        fov: FovAssertion,
    },
    /// An entity's origin projects inside a camera's frame.
    InFrame {
        /// The in_frame assertion.
        in_frame: FrameAssertion,
    },
    /// It doesn't.
    OutOfFrame {
        /// The out_of_frame assertion.
        out_of_frame: FrameAssertion,
    },
    /// The cameras with a `shot`, in order.
    ShotList {
        /// The expected entity names.
        shot_list: Vec<String>,
    },
    /// A shape this runner doesn't know — a failure line, never a parse
    /// error (the JS reference's "unknown assertion").
    Unknown(serde_json::Value),
}

/// The `fov` assertion's payload.
#[derive(Debug, Clone, Deserialize)]
pub struct FovAssertion {
    /// The camera entity's name.
    pub camera: String,
    /// Expected horizontal FOV, degrees.
    pub hfov_degrees: f64,
    /// Expected vertical FOV, degrees.
    pub vfov_degrees: f64,
    /// Expected frame aspect.
    pub aspect: f64,
    /// The tolerance, degrees (default 0.001).
    #[serde(default)]
    pub tolerance: Option<f64>,
}

/// The `in_frame` / `out_of_frame` assertions' payload.
#[derive(Debug, Clone, Deserialize)]
pub struct FrameAssertion {
    /// The camera entity's name.
    pub camera: String,
    /// The entity whose origin projects (or not).
    pub entity: String,
}

/// An outcomes run's verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct OutcomeResult {
    /// True when every assertion held.
    pub ok: bool,
    /// One line per failure, for the runner's report.
    pub failures: Vec<String>,
}

/// Run a conformance outcomes document against a world: derive, then
/// check every assertion. This is the extension's conformance — math,
/// not pixels.
pub fn run_outcomes(manifest: &wt::WorldManifest, outcomes: &OutcomesDoc) -> OutcomeResult {
    let mut failures = Vec::new();
    for assertion in &outcomes.expect {
        match assertion {
            Assertion::Fov { fov } => {
                let Some((_, frame, _)) = camera_setup(manifest, &fov.camera, &mut failures) else {
                    continue;
                };
                let tolerance = fov.tolerance.unwrap_or(0.001);
                if (frame.hfov_degrees - fov.hfov_degrees).abs() > tolerance {
                    failures.push(format!(
                        "{}: hfov {} != {} (tolerance {tolerance})",
                        fov.camera, frame.hfov_degrees, fov.hfov_degrees
                    ));
                }
                if (frame.vfov_degrees - fov.vfov_degrees).abs() > tolerance {
                    failures.push(format!(
                        "{}: vfov {} != {} (tolerance {tolerance})",
                        fov.camera, frame.vfov_degrees, fov.vfov_degrees
                    ));
                }
                if (frame.aspect - fov.aspect).abs() > tolerance {
                    failures.push(format!(
                        "{}: aspect {} != {} (tolerance {tolerance})",
                        fov.camera, frame.aspect, fov.aspect
                    ));
                }
            }
            Assertion::InFrame { in_frame: target }
            | Assertion::OutOfFrame {
                out_of_frame: target,
            } => {
                let expect_inside = matches!(assertion, Assertion::InFrame { .. });
                let Some((camera, frame, view)) =
                    camera_setup(manifest, &target.camera, &mut failures)
                else {
                    continue;
                };
                let origin = origin_of(manifest, &target.entity, &mut failures);
                let p = project(&view, &frame, &camera, origin);
                let inside = p.z > 0.0 && p.x.abs() <= 1.0 && p.y.abs() <= 1.0;
                if expect_inside && !inside {
                    failures.push(format!(
                        "{}: {} projects outside the frame ({:.3}, {:.3})",
                        target.camera, target.entity, p.x, p.y
                    ));
                }
                if !expect_inside && inside {
                    failures.push(format!(
                        "{}: {} projects inside the frame ({:.3}, {:.3})",
                        target.camera, target.entity, p.x, p.y
                    ));
                }
            }
            Assertion::ShotList { shot_list: expected } => {
                let shots = shot_list(manifest);
                let actual: Vec<&str> = shots.iter().map(|s| s.name.as_str()).collect();
                let expected: Vec<&str> = expected.iter().map(String::as_str).collect();
                if actual != expected {
                    failures.push(format!("shot list {actual:?} != {expected:?}"));
                }
            }
            Assertion::Unknown(value) => {
                failures.push(format!("unknown assertion {value}"));
            }
        }
    }
    OutcomeResult {
        ok: failures.is_empty(),
        failures,
    }
}

/// The named camera entity's component, frame and view — or a failure
/// line and `None`.
fn camera_setup(
    manifest: &wt::WorldManifest,
    name: &str,
    failures: &mut Vec<String>,
) -> Option<(CameraComponent, CameraFrame, CameraView)> {
    let entity = manifest.entities.iter().find(|e| e.name.as_str() == name);
    let camera = entity.and_then(CameraComponent::of);
    let (Some(entity), Some(camera)) = (entity, camera) else {
        failures.push(format!("no camera named {name}"));
        return None;
    };
    let frame = frame_of(&camera);
    let view = view_of(entity, &camera);
    Some((camera, frame, view))
}

/// An entity's origin — or a failure line and the world origin (the JS
/// reference's fallback).
fn origin_of(manifest: &wt::WorldManifest, name: &str, failures: &mut Vec<String>) -> [f64; 3] {
    match manifest.entities.iter().find(|e| e.name.as_str() == name) {
        Some(entity) => entity.transform.position.map(|p| p as f64),
        None => {
            failures.push(format!("no entity named {name}"));
            [0.0, 0.0, 0.0]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WorldEntity, WorldManifest};

    fn repo(relative: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../")
            .join(relative)
    }

    fn cinema_world() -> WorldManifest {
        serde_json::from_str(
            &std::fs::read_to_string(repo("conformance/cinematography.json")).unwrap(),
        )
        .unwrap()
    }

    fn by_name<'a>(manifest: &'a WorldManifest, name: &str) -> &'a WorldEntity {
        manifest
            .entities
            .iter()
            .find(|e| e.name.as_str() == name)
            .unwrap()
    }

    fn entity(id: u64, name: &str, ext: serde_json::Value) -> WorldEntity {
        let mut e = WorldEntity::new(id, name);
        e.extra.insert(EXTENSION_NAME.to_string(), ext);
        e
    }

    #[test]
    fn camera_of_applies_the_defaults_per_absent_field_and_none_for_non_cameras() {
        let manifest = cinema_world();
        assert!(CameraComponent::of(by_name(&manifest, "maya")).is_none());
        let bare = CameraComponent::of(&entity(1, "c", serde_json::json!({"camera": {}}))).unwrap();
        assert_eq!(bare.sensor_mm, DEFAULT_SENSOR);
        assert_eq!(bare.focal_length_mm, DEFAULT_FOCAL_LENGTH);
        assert_eq!(bare.squeeze, 1.0);
        assert_eq!(bare.aspect_ratio, None);
        // The extension version the module implements.
        assert_eq!(EXTENSION_VERSION, "0.2.0");
    }

    #[test]
    fn frame_of_is_the_normative_crop_math_cropping_trims_never_widens() {
        let manifest = cinema_world();
        // Super 35 with no aspect is the whole sensor.
        let full = frame_of(&CameraComponent::of(by_name(&manifest, "2A")).unwrap());
        assert!((full.aspect - 24.89 / 18.66).abs() < 1e-9);
        // A 2.39 crop on Super 35 trims the height only.
        let cropped = frame_of(&CameraComponent::of(by_name(&manifest, "1A")).unwrap());
        assert_eq!(cropped.width_mm, 24.89);
        assert!(cropped.height_mm < 18.66);
        assert!((cropped.aspect - 2.39).abs() < 1e-9);
        // The 2× anamorphic desqueezes, then crops the width.
        let ana = frame_of(&CameraComponent::of(by_name(&manifest, "3A")).unwrap());
        assert!(ana.width_mm < 24.89 * 2.0);
        assert_eq!(ana.height_mm, 18.66);
        assert!((ana.aspect - 2.39).abs() < 1e-9);
    }

    #[test]
    fn view_of_looks_at_the_aim_with_plus_y_up_else_down_the_local_minus_z() {
        let manifest = cinema_world();
        let wide = view_of(
            by_name(&manifest, "1A"),
            &CameraComponent::of(by_name(&manifest, "1A")).unwrap(),
        );
        assert_eq!(wide.position[0], 0.0);
        assert!((wide.position[1] - 1.6).abs() < 1e-6); // the transform's f32 quantization
        assert_eq!(wide.position[2], 6.0);
        // Aiming at the origin from +Z looks down −Z.
        assert!(wide.forward[0].abs() < 1e-9 && wide.forward[2] < 0.0 && wide.forward[1] < 0.0);
        assert!(wide.up[1].abs() > 0.9); // +Y up

        // No aim: local −Z carried by the entity's rotation. A camera
        // yawed 90° right (intrinsic XYZ) looks down −X.
        let mut turned_entity = entity(9, "t", serde_json::json!({"camera": {}}));
        turned_entity.transform.rotation_degrees = [0.0, 90.0, 0.0];
        let turned = view_of(&turned_entity, &CameraComponent::of(&turned_entity).unwrap());
        assert!(
            (turned.forward[0] + 1.0).abs() < 1e-9
                && turned.forward[1].abs() < 1e-9
                && turned.forward[2].abs() < 1e-9
        );
    }

    #[test]
    fn project_puts_the_aim_point_at_the_frames_center() {
        let manifest = cinema_world();
        let single = by_name(&manifest, "2A");
        let camera = CameraComponent::of(single).unwrap();
        let view = view_of(single, &camera);
        let frame = frame_of(&camera);
        let p = project(&view, &frame, &camera, [-1.5, 1.2, 0.0]);
        // 1e-6, not the JS reference's 1e-9: the transform stores f32,
        // so the camera's position is quantized on the way in.
        assert!(p.x.abs() < 1e-6 && p.y.abs() < 1e-6 && p.z > 0.0);
    }

    #[test]
    fn shot_list_orders_by_shot_order_ties_by_entity_id_and_skips_shot_less_cameras() {
        let manifest = cinema_world();
        let shots = shot_list(&manifest);
        let names: Vec<&str> = shots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["1A", "2A", "2B", "3A"]); // bts carries no shot
        // Absent orders sort last; ties break by id.
        let mut world = WorldManifest::new("t");
        world.entities.push(entity(7, "b", serde_json::json!({"shot": {}})));
        world.entities.push(entity(3, "a", serde_json::json!({"shot": {}})));
        world
            .entities
            .push(entity(5, "c", serde_json::json!({"shot": {"order": 1}})));
        let shots = shot_list(&world);
        let names: Vec<&str> = shots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["c", "a", "b"]);
    }
}
