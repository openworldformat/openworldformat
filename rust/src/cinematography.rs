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
//! - [`view_of`] / [`view_in`] / [`project`] — the aim look-at (+Y up,
//!   else the entity's local −Z) and a pinhole projection, so "is it in
//!   frame" is a predicate; `view_in` resolves `aim_entity` in a world;
//! - [`world_origin`] — an entity's origin composed through its
//!   parents, the point every assertion projects and the point
//!   `aim_entity` and `focus_entity` resolve to ([`aim_point_of`],
//!   [`focus_distance_of`]);
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
pub const EXTENSION_VERSION: &str = "0.3.0";

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
    /// Look at this entity's origin, over `aim`, while it exists — an
    /// entity reference: the id in a document, a name at intake (bound
    /// at ingestion, rewritten at merge, like `parent`).
    pub aim_entity: Option<wt::EntityRef>,
    /// Focus distance, m.
    pub focus_distance_m: Option<f64>,
    /// Focus on this entity: the focus distance is the distance to its
    /// origin, over `focus_distance_m`, while it exists.
    pub focus_entity: Option<wt::EntityRef>,
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
        let reference = |k: &str| -> Option<wt::EntityRef> {
            serde_json::from_value(camera.get(k)?.clone()).ok()
        };
        Some(CameraComponent {
            sensor_mm: vec2("sensor_mm").unwrap_or(DEFAULT_SENSOR),
            focal_length_mm: num("focal_length_mm", DEFAULT_FOCAL_LENGTH),
            aspect_ratio: opt("aspect_ratio"),
            squeeze: num("squeeze", 1.0),
            aim: vec3("aim"),
            aim_entity: reference("aim_entity"),
            focus_distance_m: opt("focus_distance_m"),
            focus_entity: reference("focus_entity"),
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

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
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

/// The entity a reference names, in a world: by id (a bound reference,
/// what a document holds) or by name (an authoring-time one, before
/// ingestion binds it). `None` when nothing answers.
pub fn entity_of<'a>(
    reference: &wt::EntityRef,
    manifest: &'a wt::WorldManifest,
) -> Option<&'a wt::WorldEntity> {
    manifest.entities.iter().find(|entity| match reference {
        wt::EntityRef::Id(id) => entity.id == *id,
        wt::EntityRef::Name(name) => entity.name.as_str() == name.as_str(),
    })
}

/// An entity's origin in world space: its `transform.position` composed
/// through its parents — the document's TRS hierarchy, `p = P + R·(S ⊙ p)`
/// up the chain (spec/world.md "Conventions"). An unparented entity's
/// origin is its position as written; a parent the world doesn't hold
/// ends the chain. This is the point every outcome assertion projects
/// and the point `aim_entity` and `focus_entity` resolve to.
pub fn world_origin(entity: &wt::WorldEntity, manifest: &wt::WorldManifest) -> [f64; 3] {
    let mut point: [f64; 3] = entity.transform.position.map(|p| p as f64);
    let mut current = entity;
    for _ in 0..4096 {
        let Some(parent_id) = current.parent else {
            break;
        };
        let Some(parent) = manifest.entities.iter().find(|e| e.id == parent_id) else {
            break;
        };
        let p: [f64; 3] = parent.transform.position.map(|v| v as f64);
        let r: [f64; 3] = parent.transform.rotation_degrees.map(|v| v as f64);
        let s: [f64; 3] = parent.transform.scale.map(|v| v as f64);
        point = add(
            p,
            rotate_intrinsic_xyz([point[0] * s[0], point[1] * s[1], point[2] * s[2]], r),
        );
        current = parent;
    }
    point
}

/// The point a camera aims at: `aim_entity`'s world origin while that
/// entity exists in the world, else `aim`, else `None` (the camera
/// looks down its local −Z).
pub fn aim_point_of(camera: &CameraComponent, manifest: &wt::WorldManifest) -> Option<[f64; 3]> {
    if let Some(target) = camera
        .aim_entity
        .as_ref()
        .and_then(|reference| entity_of(reference, manifest))
    {
        return Some(world_origin(target, manifest));
    }
    camera.aim
}

/// A camera's focus distance, metres: the distance from the camera
/// entity's world origin to `focus_entity`'s while that entity exists,
/// else `focus_distance_m`, else `None`.
pub fn focus_distance_of(
    entity: &wt::WorldEntity,
    camera: &CameraComponent,
    manifest: &wt::WorldManifest,
) -> Option<f64> {
    if let Some(target) = camera
        .focus_entity
        .as_ref()
        .and_then(|reference| entity_of(reference, manifest))
    {
        let d = sub(world_origin(target, manifest), world_origin(entity, manifest));
        return Some(dot(d, d).sqrt());
    }
    camera.focus_distance_m
}

/// A camera's view from its own transform alone: with `aim`, the camera
/// looks at the world point, +Y up; without it, it looks down the
/// entity's local −Z, its frame carried by the entity's rotation. An
/// `aim_entity` needs the world to resolve — see [`view_in`].
pub fn view_of(entity: &wt::WorldEntity, camera: &CameraComponent) -> CameraView {
    let position: [f64; 3] = entity.transform.position.map(|p| p as f64);
    view_from(entity, position, camera.aim)
}

/// A camera's view in a world: the camera stands at its world origin
/// and looks at its aim point — `aim_entity`'s origin while that entity
/// exists, else `aim` — +Y up; with neither, down its local −Z.
pub fn view_in(
    entity: &wt::WorldEntity,
    camera: &CameraComponent,
    manifest: &wt::WorldManifest,
) -> CameraView {
    view_from(entity, world_origin(entity, manifest), aim_point_of(camera, manifest))
}

fn view_from(entity: &wt::WorldEntity, position: [f64; 3], aim: Option<[f64; 3]>) -> CameraView {
    if let Some(aim) = aim {
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
/// `in_frame`, `out_of_frame`, `centred`, `focus`, `shot_list`.
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
    /// An entity's origin projects at the frame's centre — what
    /// `aim_entity` promises.
    Centred {
        /// The centred assertion.
        centred: CentredAssertion,
    },
    /// A camera's derived focus distance.
    Focus {
        /// The focus assertion.
        focus: FocusAssertion,
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

/// The `centred` assertion's payload.
#[derive(Debug, Clone, Deserialize)]
pub struct CentredAssertion {
    /// The camera entity's name.
    pub camera: String,
    /// The entity whose origin projects at the centre.
    pub entity: String,
    /// The tolerance, frame-half units (default 0.001).
    #[serde(default)]
    pub tolerance: Option<f64>,
}

/// The `focus` assertion's payload.
#[derive(Debug, Clone, Deserialize)]
pub struct FocusAssertion {
    /// The camera entity's name.
    pub camera: String,
    /// Expected focus distance, metres.
    pub distance_m: f64,
    /// The tolerance, metres (default 0.001).
    #[serde(default)]
    pub tolerance: Option<f64>,
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
                let Some((_, _, frame, _)) = camera_setup(manifest, &fov.camera, &mut failures)
                else {
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
                let Some((_, camera, frame, view)) =
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
            Assertion::Centred { centred } => {
                let Some((_, camera, frame, view)) =
                    camera_setup(manifest, &centred.camera, &mut failures)
                else {
                    continue;
                };
                let tolerance = centred.tolerance.unwrap_or(0.001);
                let origin = origin_of(manifest, &centred.entity, &mut failures);
                let p = project(&view, &frame, &camera, origin);
                if !(p.z > 0.0 && p.x.abs() <= tolerance && p.y.abs() <= tolerance) {
                    failures.push(format!(
                        "{}: {} projects at ({:.4}, {:.4}), z {:.3} — not the frame's centre (tolerance {tolerance})",
                        centred.camera, centred.entity, p.x, p.y, p.z
                    ));
                }
            }
            Assertion::Focus { focus } => {
                let Some((entity, camera, _, _)) =
                    camera_setup(manifest, &focus.camera, &mut failures)
                else {
                    continue;
                };
                let tolerance = focus.tolerance.unwrap_or(0.001);
                match focus_distance_of(entity, &camera, manifest) {
                    None => failures.push(format!("{}: no focus distance", focus.camera)),
                    Some(actual) if (actual - focus.distance_m).abs() > tolerance => {
                        failures.push(format!(
                            "{}: focus distance {} != {} (tolerance {tolerance})",
                            focus.camera, actual, focus.distance_m
                        ));
                    }
                    Some(_) => {}
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

/// The named camera entity, its component, frame and view in the world
/// — or a failure line and `None`.
fn camera_setup<'a>(
    manifest: &'a wt::WorldManifest,
    name: &str,
    failures: &mut Vec<String>,
) -> Option<(&'a wt::WorldEntity, CameraComponent, CameraFrame, CameraView)> {
    let entity = manifest.entities.iter().find(|e| e.name.as_str() == name);
    let camera = entity.and_then(CameraComponent::of);
    let (Some(entity), Some(camera)) = (entity, camera) else {
        failures.push(format!("no camera named {name}"));
        return None;
    };
    let frame = frame_of(&camera);
    let view = view_in(entity, &camera, manifest);
    Some((entity, camera, frame, view))
}

/// An entity's world origin — or a failure line and the world origin
/// (the JS reference's fallback).
fn origin_of(manifest: &wt::WorldManifest, name: &str, failures: &mut Vec<String>) -> [f64; 3] {
    match manifest.entities.iter().find(|e| e.name.as_str() == name) {
        Some(entity) => world_origin(entity, manifest),
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
        assert_eq!(bare.aim_entity, None);
        assert_eq!(bare.focus_entity, None);
        // The references read as the document holds them: an id, or a
        // name before ingestion binds it.
        let tracked = CameraComponent::of(by_name(&manifest, "4A")).unwrap();
        assert_eq!(tracked.aim_entity, Some(wt::EntityRef::id(4)));
        assert_eq!(tracked.focus_entity, Some(wt::EntityRef::id(4)));
        let named = CameraComponent::of(&entity(
            2,
            "n",
            serde_json::json!({"camera": {"aim_entity": "maya"}}),
        ))
        .unwrap();
        assert!(matches!(named.aim_entity, Some(wt::EntityRef::Name(_))));
        // The extension version the module implements.
        assert_eq!(EXTENSION_VERSION, "0.3.0");
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
    fn world_origin_composes_the_position_through_the_parents() {
        let manifest = cinema_world();
        // The mug sits on a tray turned 30° on the counter: its origin
        // is its local position rotated and carried, not as written.
        let mug = world_origin(by_name(&manifest, "mug"), &manifest);
        let expected = [0.8 + 0.3 * 30f64.to_radians().cos(), 1.05, -1.0 - 0.3 * 30f64.to_radians().sin()];
        for i in 0..3 {
            assert!((mug[i] - expected[i]).abs() < 1e-5, "axis {i}: {} vs {}", mug[i], expected[i]);
        }
        // An unparented entity's origin is its position.
        let maya = world_origin(by_name(&manifest, "maya"), &manifest);
        assert!((maya[0] + 1.5).abs() < 1e-6 && (maya[1] - 0.85).abs() < 1e-6);
    }

    #[test]
    fn aim_entity_centres_the_target_and_focus_entity_measures_to_it() {
        let manifest = cinema_world();
        let tracked = by_name(&manifest, "4A");
        let camera = CameraComponent::of(tracked).unwrap();
        let view = view_in(tracked, &camera, &manifest);
        let frame = frame_of(&camera);
        let maya = world_origin(by_name(&manifest, "maya"), &manifest);
        let p = project(&view, &frame, &camera, maya);
        assert!(p.x.abs() < 1e-6 && p.y.abs() < 1e-6 && p.z > 0.0);
        assert_eq!(aim_point_of(&camera, &manifest), Some(maya));
        let focus = focus_distance_of(tracked, &camera, &manifest).unwrap();
        assert!((focus - 4.3843).abs() < 1e-3, "{focus}");
        // Through the parents: the insert centres the mug's world
        // origin, and focuses on it from there.
        let insert = by_name(&manifest, "insert");
        let camera = CameraComponent::of(insert).unwrap();
        let view = view_in(insert, &camera, &manifest);
        let p = project(&view, &frame_of(&camera), &camera, world_origin(by_name(&manifest, "mug"), &manifest));
        assert!(p.x.abs() < 1e-6 && p.y.abs() < 1e-6 && p.z > 0.0);
        let focus = focus_distance_of(insert, &camera, &manifest).unwrap();
        assert!((focus - 1.4956).abs() < 1e-3, "{focus}");
    }

    #[test]
    fn a_gone_target_leaves_aim_then_the_rotation_and_focus_distance_m() {
        let manifest = cinema_world();
        let mut orphan = entity(
            40,
            "orphan",
            serde_json::json!({"camera": {"aim_entity": 999, "aim": [0.0, 1.0, 0.0],
                "focus_entity": 999, "focus_distance_m": 2.5}}),
        );
        orphan.transform.position = [0.0, 1.0, 5.0];
        let camera = CameraComponent::of(&orphan).unwrap();
        assert_eq!(aim_point_of(&camera, &manifest), Some([0.0, 1.0, 0.0]));
        assert_eq!(focus_distance_of(&orphan, &camera, &manifest), Some(2.5));
        let view = view_in(&orphan, &camera, &manifest);
        assert!((view.forward[2] + 1.0).abs() < 1e-9, "looks down −Z at the aim point");
    }

    #[test]
    fn shot_list_orders_by_shot_order_ties_by_entity_id_and_skips_shot_less_cameras() {
        let manifest = cinema_world();
        let shots = shot_list(&manifest);
        let names: Vec<&str> = shots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["1A", "2A", "2B", "3A", "4A"]); // bts and insert carry no shot
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
