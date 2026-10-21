//! The geo frame extension — `ext-h3` (spec/extensions/h3.md), version
//! 0.1: where an entity sits on a globe, as an H3 cell plus an
//! integer-only transform in that cell's frame, beside the core
//! transform that a reader without the extension draws.
//!
//! The one invariant: **the H3 fields are authoritative, and the core
//! transform is derived from them by the producer.** This module is
//! that derivation, so a producer can write a package and a reader
//! with an H3 library can check one: the unit table, the quantisation,
//! the tangent-plane math that turns a cell centre and a translation
//! into local metres, and the conformance outcome runner.
//!
//! No H3 library is embedded. A cell's centre comes in as a [`LatLng`]
//! from whatever H3 implementation the caller has (`cellToLatLng`);
//! the conformance fixture carries its centres as data, so every
//! reference runs the same derivation without one.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::entity::{WorldEntity, WorldTransform};
use crate::world::WorldManifest;

/// The extension's namespace key, on entities and on the environment.
pub const EXT_H3_KEY: &str = "ext-h3";

/// The body the unit table below is for — the default, and the only
/// one this version defines a unit for.
pub const BODY_EARTH: &str = "earth";

/// H3's sphere, metres: the radius its cell geometry lives on.
pub const EARTH_RADIUS_M: f64 = 6_371_007.180_918_475;

/// The mean edge length of a cell at each resolution 0..=15, metres,
/// as H3 4 reports it (`getHexagonEdgeLengthAvgM`). One translation
/// unit is a hundredth of the entry at the placement's resolution.
pub const EDGE_LENGTH_M: [f64; 16] = [
    1281256.011,
    483056.8391,
    182512.9565,
    68979.22179,
    26071.75968,
    9854.09099,
    3724.532667,
    1406.475763,
    531.41401,
    200.786148,
    75.863783,
    28.663897,
    10.830188,
    4.09201,
    1.5461,
    0.584169,
];

/// Translation units per mean edge length.
pub const TRANSLATION_UNITS_PER_EDGE: f64 = 100.0;
/// Rotation units per degree.
pub const ROTATION_UNITS_PER_DEGREE: f64 = 100.0;
/// Scale units per octave: `round(log2(factor) * 1024)`, 0 is 1×.
pub const SCALE_UNITS_PER_OCTAVE: f64 = 1024.0;

/// The environment's `ext-h3` block: the package's local frame, pinned
/// to a cell. The frame's origin is the cell's centre on the body's
/// sphere; +X east, +Y up, +Z south; metres — the core's conventions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtH3Frame {
    /// The origin cell, as H3's 15-digit lowercase hex string.
    pub cell: String,
    /// The cell's resolution, 0..=15 (states what the index encodes).
    pub resolution: u8,
    /// The body the grid is on; `earth` by default.
    #[serde(default = "default_body")]
    pub body: String,
}

/// An entity's `ext-h3` block: its cell and its transform relative to
/// that cell's centre, in the cell's own frame (+X east, +Y up, +Z
/// south), as integers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtH3Placement {
    /// The cell, as H3's 15-digit lowercase hex string.
    pub cell: String,
    /// The cell's resolution, 0..=15; the translation unit depends on it.
    pub resolution: u8,
    /// The body the grid is on; `earth` by default.
    #[serde(default = "default_body")]
    pub body: String,
    /// Hundredths of the mean edge length at `resolution`, along the
    /// cell frame's east, up and south.
    #[serde(default)]
    pub translation: [i64; 3],
    /// Hundredths of a degree, intrinsic XYZ like the core.
    #[serde(default)]
    pub rotation: [i64; 3],
    /// Log2 fixed point: `round(log2(factor) * 1024)` per axis.
    #[serde(default)]
    pub scale: [i64; 3],
}

fn default_body() -> String {
    BODY_EARTH.to_string()
}

/// A point on the body, degrees. On the wire it is `[lat, lng]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(from = "[f64; 2]", into = "[f64; 2]")]
pub struct LatLng {
    /// Latitude, degrees, north positive.
    pub lat: f64,
    /// Longitude, degrees, east positive.
    pub lng: f64,
}

impl From<[f64; 2]> for LatLng {
    fn from([lat, lng]: [f64; 2]) -> Self {
        Self { lat, lng }
    }
}

impl From<LatLng> for [f64; 2] {
    fn from(ll: LatLng) -> Self {
        [ll.lat, ll.lng]
    }
}

// ---------------------------------------------------------------------------
// Units and quantisation
// ---------------------------------------------------------------------------

/// The mean edge length at a resolution, metres; `None` past 15.
pub fn edge_length_m(resolution: u8) -> Option<f64> {
    EDGE_LENGTH_M.get(usize::from(resolution)).copied()
}

/// One translation unit at a resolution, metres.
pub fn translation_unit_m(resolution: u8) -> Option<f64> {
    edge_length_m(resolution).map(|edge| edge / TRANSLATION_UNITS_PER_EDGE)
}

/// Metres to translation units at a resolution (round half away from zero).
pub fn quantize_translation(metres: f64, resolution: u8) -> Option<i64> {
    translation_unit_m(resolution).map(|unit| (metres / unit).round() as i64)
}

/// Translation units at a resolution to metres.
pub fn dequantize_translation(units: i64, resolution: u8) -> Option<f64> {
    translation_unit_m(resolution).map(|unit| units as f64 * unit)
}

/// Degrees to rotation units.
pub fn quantize_rotation(degrees: f64) -> i64 {
    (degrees * ROTATION_UNITS_PER_DEGREE).round() as i64
}

/// Rotation units to degrees.
pub fn dequantize_rotation(units: i64) -> f64 {
    units as f64 / ROTATION_UNITS_PER_DEGREE
}

/// A scale factor to log2 fixed point; `None` for a factor that has no
/// logarithm (zero, negative, non-finite).
pub fn quantize_scale(factor: f64) -> Option<i64> {
    (factor.is_finite() && factor > 0.0)
        .then(|| (factor.log2() * SCALE_UNITS_PER_OCTAVE).round() as i64)
}

/// Log2 fixed point to a scale factor.
pub fn dequantize_scale(units: i64) -> f64 {
    2f64.powf(units as f64 / SCALE_UNITS_PER_OCTAVE)
}

// ---------------------------------------------------------------------------
// Reading the blocks
// ---------------------------------------------------------------------------

/// The frame the environment declares, if it carries a well-formed block.
pub fn frame_of(manifest: &WorldManifest) -> Option<ExtH3Frame> {
    let block = manifest.environment.as_ref()?.extra.get(EXT_H3_KEY)?;
    serde_json::from_value(block.clone()).ok()
}

/// An entity's placement, if it carries a well-formed block. A block the
/// extension can't read is absent to it — must-ignore, as for any
/// extension — and the entity is wherever its core transform says.
pub fn placement_of(entity: &WorldEntity) -> Option<ExtH3Placement> {
    let block = entity.extra.get(EXT_H3_KEY)?;
    serde_json::from_value(block.clone()).ok()
}

// ---------------------------------------------------------------------------
// The derivation (normative: spec/extensions/h3.md, "Deriving the core transform")
// ---------------------------------------------------------------------------

/// A tangent frame on the sphere: the point, and the unit vectors of
/// the local +X (east), +Y (up) and +Z (south) axes, all in the body's
/// Cartesian frame (x through lat 0 / lng 0, z through the pole).
struct Tangent {
    at: [f64; 3],
    east: [f64; 3],
    up: [f64; 3],
    south: [f64; 3],
}

fn tangent(ll: LatLng) -> Tangent {
    let (lat, lng) = (ll.lat.to_radians(), ll.lng.to_radians());
    let (sin_lat, cos_lat) = lat.sin_cos();
    let (sin_lng, cos_lng) = lng.sin_cos();
    let up = [cos_lat * cos_lng, cos_lat * sin_lng, sin_lat];
    Tangent {
        at: [
            up[0] * EARTH_RADIUS_M,
            up[1] * EARTH_RADIUS_M,
            up[2] * EARTH_RADIUS_M,
        ],
        east: [-sin_lng, cos_lng, 0.0],
        up,
        south: [sin_lat * cos_lng, sin_lat * sin_lng, -cos_lat],
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The local position of a point given as a cell centre plus a
/// translation in that cell's frame (metres: east, up, south), in the
/// frame at `frame_origin`: the cell centre's offset from the origin,
/// plus the translation carried over exactly — the two tangent frames
/// differ by their tilt, and that tilt is applied, not ignored. When
/// the origin is the cell itself, the result is the translation.
pub fn local_position(
    frame_origin: LatLng,
    cell_centre: LatLng,
    translation_m: [f64; 3],
) -> [f64; 3] {
    let o = tangent(frame_origin);
    let c = tangent(cell_centre);
    let [tx, ty, tz] = translation_m;
    let v: [f64; 3] = std::array::from_fn(|axis| {
        c.at[axis] - o.at[axis] + tx * c.east[axis] + ty * c.up[axis] + tz * c.south[axis]
    });
    [dot(v, o.east), dot(v, o.up), dot(v, o.south)]
}

/// The core transform a producer writes for a placement: position by
/// [`local_position`] (the frame's origin when the environment declares
/// one, else the entity's own cell, which makes the offset zero),
/// rotation and scale dequantised axis by axis. `None` when the body is
/// not `earth` or the resolution has no unit — the extension then says
/// nothing about where the entity is, and the core transform stands.
pub fn derive_transform(
    frame_origin: Option<LatLng>,
    cell_centre: LatLng,
    placement: &ExtH3Placement,
) -> Option<WorldTransform> {
    if placement.body != BODY_EARTH {
        return None;
    }
    let unit = translation_unit_m(placement.resolution)?;
    let translation_m = [
        placement.translation[0] as f64 * unit,
        placement.translation[1] as f64 * unit,
        placement.translation[2] as f64 * unit,
    ];
    let position = local_position(
        frame_origin.unwrap_or(cell_centre),
        cell_centre,
        translation_m,
    );
    let f = |v: f64| v as f32;
    Some(WorldTransform {
        position: [f(position[0]), f(position[1]), f(position[2])],
        rotation_degrees: [
            f(dequantize_rotation(placement.rotation[0])),
            f(dequantize_rotation(placement.rotation[1])),
            f(dequantize_rotation(placement.rotation[2])),
        ],
        scale: [
            f(dequantize_scale(placement.scale[0])),
            f(dequantize_scale(placement.scale[1])),
            f(dequantize_scale(placement.scale[2])),
        ],
        visible: true,
    })
}

// ---------------------------------------------------------------------------
// Outcome assertions — the extension's conformance
// ---------------------------------------------------------------------------

/// A conformance outcomes document for the extension: the cell centres
/// the derivation needs (data, since no reference carries H3), and the
/// assertions.
#[derive(Debug, Clone, Deserialize)]
pub struct OutcomesDoc {
    /// The world the assertions belong to (a conformance path; the
    /// runner's caller resolves it).
    #[serde(default)]
    pub world: String,
    /// `cellToLatLng` of every cell the world names, `[lat, lng]` in
    /// degrees. A runner with an H3 library SHOULD verify them.
    #[serde(default)]
    pub cell_centres: BTreeMap<String, LatLng>,
    /// The assertions.
    pub expect: Vec<Assertion>,
}

/// One assertion. The shapes match the conformance JSON: `frame`,
/// `edge_length`, `placed`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Assertion {
    /// The environment declares exactly this frame.
    Frame {
        /// The expected frame.
        frame: ExtH3Frame,
    },
    /// The unit table says this at a resolution.
    EdgeLength {
        /// The expected entry.
        edge_length: EdgeLengthAssertion,
    },
    /// An entity's derived transform is this, and so is its core transform.
    Placed {
        /// The expected placement.
        placed: PlacedAssertion,
    },
}

/// The `edge_length` assertion's payload.
#[derive(Debug, Clone, Deserialize)]
pub struct EdgeLengthAssertion {
    /// The resolution.
    pub resolution: u8,
    /// The mean edge length, metres.
    pub metres: f64,
    /// Absolute tolerance (default 1e-9).
    #[serde(default)]
    pub tolerance: Option<f64>,
}

/// The `placed` assertion's payload.
#[derive(Debug, Clone, Deserialize)]
pub struct PlacedAssertion {
    /// The entity's name.
    pub entity: String,
    /// The expected local position, metres.
    pub position: [f64; 3],
    /// The expected rotation, degrees (unchecked when absent).
    #[serde(default)]
    pub rotation_degrees: Option<[f64; 3]>,
    /// The expected scale (unchecked when absent).
    #[serde(default)]
    pub scale: Option<[f64; 3]>,
    /// Absolute tolerance per component (default 0.001).
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

fn within(actual: [f32; 3], expected: [f64; 3], tolerance: f64) -> bool {
    (0..3).all(|i| (f64::from(actual[i]) - expected[i]).abs() <= tolerance)
}

/// Run a conformance outcomes document against a world: read the blocks,
/// derive, compare — the derivation against the expected values, and the
/// core transform the producer wrote against the same values, which is
/// the invariant checked from both ends.
pub fn run_outcomes(manifest: &WorldManifest, outcomes: &OutcomesDoc) -> OutcomeResult {
    let mut failures = Vec::new();
    let frame = frame_of(manifest);
    let origin = frame
        .as_ref()
        .and_then(|f| outcomes.cell_centres.get(&f.cell).copied());
    for assertion in &outcomes.expect {
        match assertion {
            Assertion::Frame { frame: expected } => {
                if frame.as_ref() != Some(expected) {
                    failures.push(format!(
                        "the environment declares {frame:?}, not {expected:?}"
                    ));
                }
            }
            Assertion::EdgeLength { edge_length } => {
                let tolerance = edge_length.tolerance.unwrap_or(1e-9);
                match edge_length_m(edge_length.resolution) {
                    Some(m) if (m - edge_length.metres).abs() <= tolerance => {}
                    found => failures.push(format!(
                        "edge length at resolution {} is {found:?}, not {}",
                        edge_length.resolution, edge_length.metres
                    )),
                }
            }
            Assertion::Placed { placed } => {
                let tolerance = placed.tolerance.unwrap_or(0.001);
                let Some(entity) = manifest
                    .entities
                    .iter()
                    .find(|e| e.name.as_str() == placed.entity)
                else {
                    failures.push(format!("no entity is named {}", placed.entity));
                    continue;
                };
                let Some(placement) = placement_of(entity) else {
                    failures.push(format!(
                        "{} carries no readable ext-h3 block",
                        placed.entity
                    ));
                    continue;
                };
                let Some(centre) = outcomes.cell_centres.get(&placement.cell).copied() else {
                    failures.push(format!(
                        "{}: the fixture has no centre for cell {}",
                        placed.entity, placement.cell
                    ));
                    continue;
                };
                if frame.is_some() && origin.is_none() {
                    failures.push("the fixture has no centre for the frame's origin cell".into());
                    continue;
                }
                let Some(derived) = derive_transform(origin, centre, &placement) else {
                    failures.push(format!(
                        "{}: nothing derives from body {} at resolution {}",
                        placed.entity, placement.body, placement.resolution
                    ));
                    continue;
                };
                let core = &entity.transform;
                let mut check = |what: &str, actual: [f32; 3], expected: [f64; 3]| {
                    if !within(actual, expected, tolerance) {
                        failures.push(format!(
                            "{}: {what} is {actual:?}, not within {tolerance} of {expected:?}",
                            placed.entity
                        ));
                    }
                };
                check("derived position", derived.position, placed.position);
                check("core position", core.position, placed.position);
                if let Some(rotation) = placed.rotation_degrees {
                    check("derived rotation", derived.rotation_degrees, rotation);
                    check("core rotation", core.rotation_degrees, rotation);
                }
                if let Some(scale) = placed.scale {
                    check("derived scale", derived.scale, scale);
                    check("core scale", core.scale, scale);
                }
            }
        }
    }
    OutcomeResult {
        ok: failures.is_empty(),
        failures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantisation_matches_the_producing_format() {
        assert_eq!(quantize_scale(2.0), Some(1024));
        assert_eq!(quantize_scale(0.5), Some(-1024));
        assert_eq!(quantize_scale(1.0), Some(0));
        assert_eq!(quantize_scale(0.0), None);
        assert!((dequantize_scale(1024) - 2.0).abs() < 1e-12);
        assert_eq!(quantize_rotation(30.0), 3000);
        assert_eq!(quantize_rotation(-0.005), -1); // half away from zero
        assert_eq!(dequantize_rotation(-4500), -45.0);
        // Resolution 15: an edge is 0.584169 m, a unit 5.84169 mm.
        assert_eq!(quantize_translation(0.75, 15), Some(128));
        assert!((dequantize_translation(128, 15).unwrap() - 0.74773632).abs() < 1e-9);
        assert_eq!(edge_length_m(16), None);
        assert_eq!(quantize_translation(1.0, 16), None);
    }

    #[test]
    fn the_origin_cell_places_by_translation_alone() {
        let here = LatLng {
            lat: 48.85839598760832,
            lng: 2.29450022174426,
        };
        let p = local_position(here, here, [1.0, 2.0, 3.0]);
        assert!(
            (p[0] - 1.0).abs() < 1e-9 && (p[1] - 2.0).abs() < 1e-9 && (p[2] - 3.0).abs() < 1e-9
        );
    }

    #[test]
    fn east_is_plus_x_north_is_minus_z_and_the_tangent_plane_drops() {
        let origin = LatLng { lat: 0.0, lng: 0.0 };
        let step = 1e-5; // degrees: 1.1119 m on H3's sphere
        let east = local_position(
            origin,
            LatLng {
                lat: 0.0,
                lng: step,
            },
            [0.0; 3],
        );
        let metres = EARTH_RADIUS_M * step.to_radians();
        assert!((east[0] - metres).abs() < 1e-6, "{east:?}");
        assert!(east[2].abs() < 1e-9);
        assert!(
            east[1] < 0.0 && east[1] > -1e-6,
            "the drop of the tangent plane: {}",
            east[1]
        );
        let north = local_position(
            origin,
            LatLng {
                lat: step,
                lng: 0.0,
            },
            [0.0; 3],
        );
        assert!((north[2] + metres).abs() < 1e-6, "{north:?}");
        assert!(north[0].abs() < 1e-9);
        // A translation in the far cell's frame is carried into the origin's.
        let up_there = local_position(
            origin,
            LatLng {
                lat: step,
                lng: 0.0,
            },
            [0.0, 5.0, 0.0],
        );
        assert!((up_there[1] - 5.0).abs() < 1e-6, "{up_there:?}");
    }

    #[test]
    fn derive_reads_the_blocks_and_writes_the_core_transform() {
        let json = serde_json::json!({
            "cell": "8f1fb46741a12d5", "resolution": 15,
            "translation": [0, 128, 0], "rotation": [0, 3000, 0], "scale": [0, 1024, 0]
        });
        let placement: ExtH3Placement = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(placement.body, "earth");
        let centre = LatLng {
            lat: 48.85839598760832,
            lng: 2.29450022174426,
        };
        let t = derive_transform(None, centre, &placement).unwrap();
        assert!((t.position[1] - 0.74773632).abs() < 1e-6);
        assert_eq!(t.rotation_degrees, [0.0, 30.0, 0.0]);
        assert_eq!(t.scale, [1.0, 2.0, 1.0]);
        // Another body: the extension says nothing.
        let moon = ExtH3Placement {
            body: "moon".into(),
            ..placement.clone()
        };
        assert_eq!(derive_transform(None, centre, &moon), None);

        let mut entity = WorldEntity::new(7, "column");
        entity.extra.insert(EXT_H3_KEY.into(), json);
        assert_eq!(placement_of(&entity), Some(placement));
        let mut manifest = WorldManifest::new("terrace");
        let mut env = crate::world::EnvironmentDef::default();
        env.extra.insert(
            EXT_H3_KEY.into(),
            serde_json::json!({"cell": "8f1fb46741a12d5", "resolution": 15}),
        );
        manifest.environment = Some(env);
        assert_eq!(
            frame_of(&manifest),
            Some(ExtH3Frame {
                cell: "8f1fb46741a12d5".into(),
                resolution: 15,
                body: "earth".into()
            })
        );
    }

    #[test]
    fn the_outcome_runner_checks_the_invariant_from_both_ends() {
        let manifest: WorldManifest = serde_json::from_value(serde_json::json!({
            "version": 3, "meta": {"name": "t"},
            "environment": {"ext-h3": {"cell": "8f1fb46741a12d5", "resolution": 15}},
            "entities": [
                {"id": 1, "name": "column",
                 "transform": {"position": [0.0, 0.74773632, 0.0], "rotation_degrees": [0.0, 30.0, 0.0], "scale": [1.0, 2.0, 1.0]},
                 "ext-h3": {"cell": "8f1fb46741a12d5", "resolution": 15,
                            "translation": [0, 128, 0], "rotation": [0, 3000, 0], "scale": [0, 1024, 0]}},
                {"id": 2, "name": "liar",
                 "transform": {"position": [5.0, 0.0, 0.0]},
                 "ext-h3": {"cell": "8f1fb46741a12d5", "resolution": 15}}
            ]
        }))
        .unwrap();
        let outcomes: OutcomesDoc = serde_json::from_value(serde_json::json!({
            "cell_centres": {"8f1fb46741a12d5": [48.85839598760832, 2.29450022174426]},
            "expect": [
                {"frame": {"cell": "8f1fb46741a12d5", "resolution": 15, "body": "earth"}},
                {"edge_length": {"resolution": 15, "metres": 0.584169}},
                {"placed": {"entity": "column", "position": [0.0, 0.74773632, 0.0],
                            "rotation_degrees": [0.0, 30.0, 0.0], "scale": [1.0, 2.0, 1.0]}},
                {"placed": {"entity": "liar", "position": [0.0, 0.0, 0.0]}}
            ]
        }))
        .unwrap();
        let result = run_outcomes(&manifest, &outcomes);
        assert_eq!(result.failures.len(), 1, "{:?}", result.failures);
        assert!(
            result.failures[0].starts_with("liar: core position"),
            "{:?}",
            result.failures
        );
    }
}
