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
//! - [`track_of`] / [`transform_at`] / [`sample_world_at`] — strict 0.4
//!   entity-local tracks and pure `f64` time-addressed TRS sampling;
//! - [`run_outcomes`] — the conformance outcome runner: one assertion
//!   file, any implementation, the same predicates.
//!
//! Conformance is math, not pixels: implementations agree on the
//! numbers, within the stated tolerance — never on rendered output.

use serde::{Deserialize, Serialize};

use crate as wt;

/// The extension this module implements.
pub const EXTENSION_NAME: &str = "ext-cinematography";

/// The extension version this module implements.
pub const EXTENSION_VERSION: &str = "0.4.0";

/// The default sensor (filmback): Super 35, `[w, h]` in mm.
pub const DEFAULT_SENSOR: [f64; 2] = [24.89, 18.66];

/// The default focal length, in mm.
pub const DEFAULT_FOCAL_LENGTH: f64 = 35.0;

// ---------------------------------------------------------------------------
// Entity-local tracks
// ---------------------------------------------------------------------------

const TRACK_PATH: &str = "/ext-cinematography/track";

/// A malformed known track, with a JSON pointer to the offending field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackError {
    /// JSON pointer rooted at the entity, or empty for an evaluation argument.
    pub pointer: String,
    /// What the field violates.
    pub message: String,
}

impl TrackError {
    fn new(pointer: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            pointer: pointer.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TrackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.pointer.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.pointer, self.message)
        }
    }
}

impl std::error::Error for TrackError {}

/// A track's explicit time clip, in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TrackClip {
    /// Inclusive clip start.
    pub start_s: f64,
    /// Inclusive clip end; must not precede `start_s`.
    pub end_s: f64,
}

/// Interpolation from one key to the next.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Interpolation {
    /// Component-wise linear interpolation, after easing.
    #[default]
    Linear,
    /// Hold the interval's first value until its next key.
    Step,
}

/// Easing applied to a linear interval's normalized time.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Easing {
    /// No easing.
    #[default]
    Linear,
    /// `u²`.
    EaseIn,
    /// `1 - (1-u)²`.
    EaseOut,
    /// Quadratic acceleration and deceleration.
    EaseInOut,
}

/// Evaluation outside the clip.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Extrapolation {
    /// Clamp to the clip endpoint.
    #[default]
    Hold,
    /// Return the entity's authored local transform, skipping every channel.
    Base,
    /// Repeat the clip with Euclidean time wrapping.
    Loop,
}

/// One vec3 key. Interpolation and easing govern the interval to the next key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TrackKey {
    /// Finite key time in seconds.
    pub time_s: f64,
    /// Translation, intrinsic XYZ Euler degrees, or component-wise scale.
    pub value: [f64; 3],
    /// Interval interpolation; linear by default.
    #[serde(default)]
    pub interpolation: Interpolation,
    /// Interval easing; linear by default and ignored by `step`.
    #[serde(default)]
    pub easing: Easing,
}

/// The three independent channels. At least one must be present and nonempty.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(transform = at_least_one_channel))]
pub struct TrackChannels {
    /// Parent-local translation in world units/metres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schema", schemars(length(min = 1)))]
    pub translation: Option<Vec<TrackKey>>,
    /// Intrinsic XYZ Euler degrees, interpolated without angle wrapping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schema", schemars(length(min = 1)))]
    pub rotation: Option<Vec<TrackKey>>,
    /// Component-wise scale, including zero and negative values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schema", schemars(length(min = 1)))]
    pub scale: Option<Vec<TrackKey>>,
}

/// One entity-local timed translation/rotation/scale track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Track {
    /// Explicit inclusive clip; absent uses the minimum and maximum key times.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<TrackClip>,
    /// Evaluation strictly before the clip.
    #[serde(default)]
    pub pre: Extrapolation,
    /// Evaluation strictly after the clip.
    #[serde(default)]
    pub post: Extrapolation,
    /// At least one nonempty channel, with strictly increasing key times.
    pub channels: TrackChannels,
}

#[cfg(feature = "schema")]
fn at_least_one_channel(schema: &mut schemars::Schema) {
    schema.insert("minProperties".to_string(), serde_json::json!(1));
}

/// Parse an entity's known track strictly, or return `None` when it has none.
/// Unknown and malformed track data is an error with its entity-rooted pointer.
pub fn track_of(entity: &wt::WorldEntity) -> Result<Option<Track>, TrackError> {
    let Some(block) = entity.extra.get(EXTENSION_NAME) else {
        return Ok(None);
    };
    let Some(track) = block.get("track") else {
        return Ok(None);
    };
    parse_track(track).map(Some)
}

pub(crate) fn validate_block(block: &serde_json::Value) -> Result<(), TrackError> {
    let Some(track) = block.get("track") else {
        return Ok(());
    };
    parse_track(track).map(|_| ())
}

fn parse_track(value: &serde_json::Value) -> Result<Track, TrackError> {
    let object = track_object(value, TRACK_PATH)?;
    known_fields(object, &["clip", "pre", "post", "channels"], TRACK_PATH)?;
    let clip = match object.get("clip") {
        None => None,
        Some(value) => {
            let path = format!("{TRACK_PATH}/clip");
            let clip = track_object(value, &path)?;
            known_fields(clip, &["start_s", "end_s"], &path)?;
            let start_s = required_number(clip, "start_s", &path)?;
            let end_s = required_number(clip, "end_s", &path)?;
            if start_s > end_s {
                return Err(TrackError::new(
                    format!("{path}/end_s"),
                    "clip end_s must not precede start_s",
                ));
            }
            Some(TrackClip { start_s, end_s })
        }
    };
    let pre = parse_extrapolation(object.get("pre"), "pre")?;
    let post = parse_extrapolation(object.get("post"), "post")?;

    let path = format!("{TRACK_PATH}/channels");
    let channels_value = object
        .get("channels")
        .ok_or_else(|| TrackError::new(&path, "channels is required"))?;
    let channels = track_object(channels_value, &path)?;
    known_fields(channels, &["translation", "rotation", "scale"], &path)?;
    if channels.is_empty() {
        return Err(TrackError::new(&path, "at least one channel is required"));
    }
    let mut parsed = TrackChannels::default();
    for (name, slot) in [
        ("translation", &mut parsed.translation),
        ("rotation", &mut parsed.rotation),
        ("scale", &mut parsed.scale),
    ] {
        let Some(value) = channels.get(name) else {
            continue;
        };
        let channel_path = format!("{path}/{name}");
        let keys = value
            .as_array()
            .ok_or_else(|| TrackError::new(&channel_path, "a channel must be an array of keys"))?;
        if keys.is_empty() {
            return Err(TrackError::new(
                &channel_path,
                "a channel must contain at least one key",
            ));
        }
        let mut parsed_keys = Vec::with_capacity(keys.len());
        let mut previous = None;
        for (index, value) in keys.iter().enumerate() {
            let key_path = format!("{channel_path}/{index}");
            let key = track_object(value, &key_path)?;
            known_fields(
                key,
                &["time_s", "value", "interpolation", "easing"],
                &key_path,
            )?;
            let time_s = required_number(key, "time_s", &key_path)?;
            if previous.is_some_and(|previous| time_s <= previous) {
                return Err(TrackError::new(
                    format!("{key_path}/time_s"),
                    "key times must be strictly increasing",
                ));
            }
            previous = Some(time_s);
            let value_path = format!("{key_path}/value");
            let value = key
                .get("value")
                .ok_or_else(|| TrackError::new(&value_path, "value is required"))?;
            let components = value.as_array().ok_or_else(|| {
                TrackError::new(&value_path, "value must be an array of three numbers")
            })?;
            if components.len() != 3 {
                return Err(TrackError::new(
                    &value_path,
                    "value must contain exactly three numbers",
                ));
            }
            let mut vector = [0.0; 3];
            for (axis, component) in components.iter().enumerate() {
                vector[axis] = component
                    .as_f64()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| {
                        TrackError::new(
                            format!("{value_path}/{axis}"),
                            "value components must be finite numbers",
                        )
                    })?;
            }
            parsed_keys.push(TrackKey {
                time_s,
                value: vector,
                interpolation: parse_interpolation(key.get("interpolation"), &key_path)?,
                easing: parse_easing(key.get("easing"), &key_path)?,
            });
        }
        *slot = Some(parsed_keys);
    }
    let track = Track {
        clip,
        pre,
        post,
        channels: parsed,
    };
    let effective_clip = track.clip.unwrap_or_else(|| inferred_clip(&track));
    if effective_clip.start_s == effective_clip.end_s {
        if pre == Extrapolation::Loop {
            return Err(TrackError::new(
                format!("{TRACK_PATH}/pre"),
                "loop requires a nonzero clip duration",
            ));
        }
        if post == Extrapolation::Loop {
            return Err(TrackError::new(
                format!("{TRACK_PATH}/post"),
                "loop requires a nonzero clip duration",
            ));
        }
    }
    Ok(track)
}

fn track_object<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Result<&'a serde_json::Map<String, serde_json::Value>, TrackError> {
    value
        .as_object()
        .ok_or_else(|| TrackError::new(path, "expected an object"))
}

fn known_fields(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
    path: &str,
) -> Result<(), TrackError> {
    for field in object.keys() {
        if !allowed.contains(&field.as_str()) {
            return Err(TrackError::new(
                format!("{path}/{field}"),
                "unknown track field",
            ));
        }
    }
    Ok(())
}

fn required_number(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    path: &str,
) -> Result<f64, TrackError> {
    let pointer = format!("{path}/{field}");
    object
        .get(field)
        .ok_or_else(|| TrackError::new(&pointer, format!("{field} is required")))?
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| TrackError::new(pointer, "expected a finite number"))
}

fn enum_string<'a>(
    value: Option<&'a serde_json::Value>,
    field: &str,
    path: &str,
) -> Result<Option<&'a str>, TrackError> {
    value
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| TrackError::new(format!("{path}/{field}"), "expected a string enum"))
        })
        .transpose()
}

fn parse_extrapolation(
    value: Option<&serde_json::Value>,
    field: &str,
) -> Result<Extrapolation, TrackError> {
    match enum_string(value, field, TRACK_PATH)? {
        None | Some("hold") => Ok(Extrapolation::Hold),
        Some("base") => Ok(Extrapolation::Base),
        Some("loop") => Ok(Extrapolation::Loop),
        Some(_) => Err(TrackError::new(
            format!("{TRACK_PATH}/{field}"),
            "expected hold, base, or loop",
        )),
    }
}

fn parse_interpolation(
    value: Option<&serde_json::Value>,
    path: &str,
) -> Result<Interpolation, TrackError> {
    match enum_string(value, "interpolation", path)? {
        None | Some("linear") => Ok(Interpolation::Linear),
        Some("step") => Ok(Interpolation::Step),
        Some(_) => Err(TrackError::new(
            format!("{path}/interpolation"),
            "expected linear or step",
        )),
    }
}

fn parse_easing(value: Option<&serde_json::Value>, path: &str) -> Result<Easing, TrackError> {
    match enum_string(value, "easing", path)? {
        None | Some("linear") => Ok(Easing::Linear),
        Some("ease_in") => Ok(Easing::EaseIn),
        Some("ease_out") => Ok(Easing::EaseOut),
        Some("ease_in_out") => Ok(Easing::EaseInOut),
        Some(_) => Err(TrackError::new(
            format!("{path}/easing"),
            "expected linear, ease_in, ease_out, or ease_in_out",
        )),
    }
}

/// Sample an entity's authored local transform plus its track at `time_s`.
/// Behaviors are deliberately not applied; visibility always remains authored.
pub fn transform_at(
    entity: &wt::WorldEntity,
    time_s: f64,
) -> Result<wt::WorldTransform, TrackError> {
    if !time_s.is_finite() {
        return Err(TrackError::new("", "evaluation time_s must be finite"));
    }
    let base = entity.transform.clone();
    let Some(track) = track_of(entity)? else {
        return Ok(base);
    };
    let clip = track.clip.unwrap_or_else(|| inferred_clip(&track));
    let resolved = if time_s >= clip.start_s && time_s <= clip.end_s {
        Some(time_s)
    } else {
        let (mode, endpoint) = if time_s < clip.start_s {
            (track.pre, clip.start_s)
        } else {
            (track.post, clip.end_s)
        };
        match mode {
            Extrapolation::Base => None,
            Extrapolation::Hold => Some(endpoint),
            Extrapolation::Loop => {
                let duration = clip.end_s - clip.start_s;
                Some(clip.start_s + (time_s - clip.start_s).rem_euclid(duration))
            }
        }
    };
    let Some(time_s) = resolved else {
        return Ok(base);
    };
    let position = track
        .channels
        .translation
        .as_deref()
        .map(|keys| sample_channel(keys, time_s))
        .unwrap_or_else(|| base.position.map(f64::from));
    let rotation_degrees = track
        .channels
        .rotation
        .as_deref()
        .map(|keys| sample_channel(keys, time_s))
        .unwrap_or_else(|| base.rotation_degrees.map(f64::from));
    let scale = track
        .channels
        .scale
        .as_deref()
        .map(|keys| sample_channel(keys, time_s))
        .unwrap_or_else(|| base.scale.map(f64::from));
    Ok(wt::WorldTransform {
        position: position.map(|value| value as f32),
        rotation_degrees: rotation_degrees.map(|value| value as f32),
        scale: scale.map(|value| value as f32),
        visible: base.visible,
    })
}

fn inferred_clip(track: &Track) -> TrackClip {
    let mut times = [
        track.channels.translation.as_deref(),
        track.channels.rotation.as_deref(),
        track.channels.scale.as_deref(),
    ]
    .into_iter()
    .flatten()
    .flatten()
    .map(|key| key.time_s);
    let first = times.next().expect("a parsed track has a key");
    let (minimum, maximum) = times.fold((first, first), |(minimum, maximum), time| {
        (minimum.min(time), maximum.max(time))
    });
    TrackClip {
        start_s: minimum,
        end_s: maximum,
    }
}

fn sample_channel(keys: &[TrackKey], time_s: f64) -> [f64; 3] {
    if time_s <= keys[0].time_s {
        return keys[0].value;
    }
    if time_s >= keys[keys.len() - 1].time_s {
        return keys[keys.len() - 1].value;
    }
    let next = keys
        .iter()
        .position(|key| key.time_s >= time_s)
        .expect("an internal time has a next key");
    let b = &keys[next];
    if time_s == b.time_s {
        return b.value;
    }
    let a = &keys[next - 1];
    if a.interpolation == Interpolation::Step {
        return a.value;
    }
    let u = (time_s - a.time_s) / (b.time_s - a.time_s);
    let e = match a.easing {
        Easing::Linear => u,
        Easing::EaseIn => u * u,
        Easing::EaseOut => 1.0 - (1.0 - u) * (1.0 - u),
        Easing::EaseInOut if u < 0.5 => 2.0 * u * u,
        Easing::EaseInOut => 1.0 - 2.0 * (1.0 - u) * (1.0 - u),
    };
    std::array::from_fn(|axis| a.value[axis] + (b.value[axis] - a.value[axis]) * e)
}

/// Sample every entity independently at one time, preserving the rest of the
/// document and every entity's ids, parents, components and opaque extensions.
pub fn sample_world_at(
    manifest: &wt::WorldManifest,
    time_s: f64,
) -> Result<wt::WorldManifest, TrackError> {
    if !time_s.is_finite() {
        return Err(TrackError::new("", "evaluation time_s must be finite"));
    }
    let mut sampled = manifest.clone();
    for entity in &mut sampled.entities {
        entity.transform = transform_at(entity, time_s)?;
    }
    Ok(sampled)
}

// ---------------------------------------------------------------------------
// The camera, the frame, the view
// ---------------------------------------------------------------------------

/// One entity's `ext-cinematography.camera` component, parsed from
/// `WorldEntity::extra` with every default applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CameraComponent {
    /// Sensor (filmback) `[w, h]`, mm.
    #[cfg_attr(feature = "schema", schemars(schema_with = "positive_pair_schema"))]
    pub sensor_mm: [f64; 2],
    /// The lens, mm.
    #[cfg_attr(feature = "schema", schemars(schema_with = "positive_number_schema"))]
    pub focal_length_mm: f64,
    /// Crop to this frame aspect (w/h); absent keeps the desqueezed
    /// sensor's.
    #[cfg_attr(feature = "schema", schemars(schema_with = "positive_number_schema"))]
    pub aspect_ratio: Option<f64>,
    /// Anamorphic desqueeze.
    #[cfg_attr(feature = "schema", schemars(schema_with = "positive_number_schema"))]
    pub squeeze: f64,
    /// Look at this world point, +Y up; absent looks down the entity's
    /// local −Z (the glTF / three.js / Bevy convention).
    pub aim: Option<[f64; 3]>,
    /// Look at this entity's origin, over `aim`, while it exists — an
    /// entity reference: the id in a document, a name at intake (bound
    /// at ingestion, rewritten at merge, like `parent`).
    #[cfg_attr(feature = "schema", schemars(schema_with = "entity_reference_schema"))]
    pub aim_entity: Option<wt::EntityRef>,
    /// Focus distance, m.
    #[cfg_attr(feature = "schema", schemars(schema_with = "positive_number_schema"))]
    pub focus_distance_m: Option<f64>,
    /// Focus on this entity: the focus distance is the distance to its
    /// origin, over `focus_distance_m`, while it exists.
    #[cfg_attr(feature = "schema", schemars(schema_with = "entity_reference_schema"))]
    pub focus_entity: Option<wt::EntityRef>,
    /// The f-stop.
    #[cfg_attr(feature = "schema", schemars(schema_with = "positive_number_schema"))]
    pub f_stop: Option<f64>,
}

impl Default for CameraComponent {
    fn default() -> Self {
        Self {
            sensor_mm: DEFAULT_SENSOR,
            focal_length_mm: DEFAULT_FOCAL_LENGTH,
            aspect_ratio: None,
            squeeze: 1.0,
            aim: None,
            aim_entity: None,
            focus_distance_m: None,
            focus_entity: None,
            f_stop: None,
        }
    }
}

/// An entity's shot component, as declared on the clock.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ShotComponent {
    /// Scene slug, free text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// Shot list sort key; ties break by entity id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<i64>,
    /// Setup start in seconds; must not exceed `out_s`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_s: Option<f64>,
    /// Setup end in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_s: Option<f64>,
    /// Shot size, free text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// What the setup is, free text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// An entity's cinematography block: a camera, shot, and/or entity-local track.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(transform = at_least_one_channel))]
pub struct EntityBlock {
    /// A filmback and lens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraComponent>,
    /// A setup in the shot list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shot: Option<ShotComponent>,
    /// One entity-local timed TRS track.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<Track>,
}

#[cfg(feature = "schema")]
fn positive_number_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "type": "number", "exclusiveMinimum": 0 })
}

#[cfg(feature = "schema")]
fn positive_pair_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "array",
        "items": { "type": "number", "exclusiveMinimum": 0 },
        "minItems": 2,
        "maxItems": 2
    })
}

#[cfg(feature = "schema")]
fn entity_reference_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "x-entity-ref": "bindable",
        "oneOf": [
            { "type": "integer", "minimum": 0 },
            { "type": "string", "minLength": 1 }
        ]
    })
}

/// Generate the normative extension schema from these reference types.
#[cfg(feature = "schema")]
pub fn extension_schema() -> serde_json::Value {
    let generated = schemars::schema_for!(EntityBlock);
    let mut root = serde_json::to_value(generated).expect("extension schema JSON");
    reject_nulls(&mut root);
    let object = root.as_object_mut().expect("extension schema object");
    let mut definitions = object
        .remove("$defs")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let mut block = serde_json::Map::new();
    for key in [
        "description",
        "type",
        "properties",
        "additionalProperties",
        "minProperties",
    ] {
        if let Some(value) = object.remove(key) {
            block.insert(key.to_string(), value);
        }
    }
    definitions.insert("EntityBlock".to_string(), serde_json::Value::Object(block));
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "ExtCinematography",
        "description": "The ext-cinematography extension, version 0.4 (experimental). Everything rides under the namespaced 'ext-cinematography' key on entities: 'camera' makes an entity a camera, 'shot' makes it a setup in the shot list, and 'track' animates that entity's parent-local translation, rotation and scale. The core schema leaves room for the key (unknown fields are allowed and must-ignored); this schema pins the extension's own shape. The camera's aim_entity and focus_entity are entity references in the core sense — marked x-entity-ref here, so the generated schema/entity-refs.json carries them. Spec: spec/extensions/cinematography.md.",
        "type": "object",
        "$defs": definitions
    })
}

#[cfg(feature = "schema")]
fn reject_nulls(value: &mut serde_json::Value) {
    if let Some(object) = value.as_object_mut() {
        if object
            .get("default")
            .is_some_and(serde_json::Value::is_null)
        {
            object.remove("default");
        }
        if let Some(types) = object
            .get_mut("type")
            .and_then(|value| value.as_array_mut())
        {
            types.retain(|kind| kind != "null");
        }
        let single_non_null = object
            .get("anyOf")
            .and_then(serde_json::Value::as_array)
            .and_then(|choices| {
                let mut choices = choices.iter().filter(|choice| {
                    choice.get("type") != Some(&serde_json::Value::String("null".to_string()))
                });
                let only = choices.next()?;
                choices.next().is_none().then_some(only.clone())
            });
        if let Some(serde_json::Value::Object(replacement)) = single_non_null {
            object.remove("anyOf");
            for (key, value) in replacement {
                object.entry(key).or_insert(value);
            }
        }
        for value in object.values_mut() {
            reject_nulls(value);
        }
    } else if let Some(array) = value.as_array_mut() {
        for value in array {
            reject_nulls(value);
        }
    }
}

impl CameraComponent {
    /// Parse the component from an entity's `extra` map, defaults per
    /// absent field (`{}` is a 35 mm on Super 35). `None` when the
    /// entity declares no camera.
    pub fn of(entity: &wt::WorldEntity) -> Option<Self> {
        let camera = entity
            .extra
            .get(EXTENSION_NAME)?
            .get("camera")?
            .as_object()?;
        let num = |k: &str, d: f64| {
            camera
                .get(k)
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(d)
        };
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
        Some(aspect) if aspect != 0.0 => (w * s * (aspect / a).min(1.0), h * (a / aspect).min(1.0)),
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
        let d = sub(
            world_origin(target, manifest),
            world_origin(entity, manifest),
        );
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
    view_from(
        entity,
        world_origin(entity, manifest),
        aim_point_of(camera, manifest),
    )
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
    let rank = |s: &ShotEntry| {
        s.shot
            .get("order")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(f64::MAX)
    };
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

/// One assertion. The shapes match the conformance JSON: `pose`, `origin`,
/// `view`, `fov`, `in_frame`, `out_of_frame`, `centred`, `focus`, `shot_list`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Assertion {
    /// An entity's sampled local TRS at one time.
    Pose {
        /// The pose assertion.
        pose: PoseAssertion,
    },
    /// An entity's sampled origin, composed through its parents.
    Origin {
        /// The origin assertion.
        origin: OriginAssertion,
    },
    /// A camera's resolved view at one time.
    View {
        /// The view assertion.
        view: ViewAssertion,
    },
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

/// The `pose` assertion's payload.
#[derive(Debug, Clone, Deserialize)]
pub struct PoseAssertion {
    /// The entity's name.
    pub entity: String,
    /// Evaluation time in seconds.
    pub time_s: f64,
    /// Expected parent-local position.
    pub position: [f64; 3],
    /// Expected parent-local intrinsic XYZ Euler degrees.
    pub rotation_degrees: [f64; 3],
    /// Expected parent-local scale.
    pub scale: [f64; 3],
    /// Component tolerance; defaults to `1e-6`.
    #[serde(default)]
    pub tolerance: Option<f64>,
}

/// The `origin` assertion's payload.
#[derive(Debug, Clone, Deserialize)]
pub struct OriginAssertion {
    /// The entity's name.
    pub entity: String,
    /// Evaluation time in seconds.
    pub time_s: f64,
    /// Expected world-space origin.
    pub position: [f64; 3],
    /// Component tolerance; defaults to `1e-6`.
    #[serde(default)]
    pub tolerance: Option<f64>,
}

/// The `view` assertion's payload.
#[derive(Debug, Clone, Deserialize)]
pub struct ViewAssertion {
    /// The camera entity's name.
    pub camera: String,
    /// Evaluation time in seconds.
    pub time_s: f64,
    /// Expected world-space camera position.
    pub position: [f64; 3],
    /// Expected unit forward vector.
    pub forward: [f64; 3],
    /// Expected unit up vector.
    pub up: [f64; 3],
    /// Component tolerance; defaults to `1e-6`.
    #[serde(default)]
    pub tolerance: Option<f64>,
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
    /// Optional evaluation time; absent preserves static-world behavior.
    #[serde(default)]
    pub time_s: Option<f64>,
}

/// The `centred` assertion's payload.
#[derive(Debug, Clone, Deserialize)]
pub struct CentredAssertion {
    /// The camera entity's name.
    pub camera: String,
    /// The entity whose origin projects at the centre.
    pub entity: String,
    /// Optional evaluation time; absent preserves static-world behavior.
    #[serde(default)]
    pub time_s: Option<f64>,
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
    /// Optional evaluation time; absent preserves static-world behavior.
    #[serde(default)]
    pub time_s: Option<f64>,
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
            Assertion::Pose { pose } => {
                let Some(manifest) = sampled_manifest(manifest, Some(pose.time_s), &mut failures)
                else {
                    continue;
                };
                let Some(entity) = manifest
                    .entities
                    .iter()
                    .find(|entity| entity.name.as_str() == pose.entity)
                else {
                    failures.push(format!("no entity named {}", pose.entity));
                    continue;
                };
                let tolerance = pose.tolerance.unwrap_or(1e-6);
                for (field, actual, expected) in [
                    ("position", entity.transform.position, pose.position),
                    (
                        "rotation_degrees",
                        entity.transform.rotation_degrees,
                        pose.rotation_degrees,
                    ),
                    ("scale", entity.transform.scale, pose.scale),
                ] {
                    compare_components(
                        &pose.entity,
                        field,
                        actual,
                        expected,
                        tolerance,
                        &mut failures,
                    );
                }
            }
            Assertion::Origin { origin } => {
                let Some(manifest) = sampled_manifest(manifest, Some(origin.time_s), &mut failures)
                else {
                    continue;
                };
                let actual = origin_of(&manifest, &origin.entity, &mut failures);
                compare_components(
                    &origin.entity,
                    "origin",
                    actual,
                    origin.position,
                    origin.tolerance.unwrap_or(1e-6),
                    &mut failures,
                );
            }
            Assertion::View { view: expected } => {
                let Some(manifest) =
                    sampled_manifest(manifest, Some(expected.time_s), &mut failures)
                else {
                    continue;
                };
                let Some((_, _, _, actual)) =
                    camera_setup(&manifest, &expected.camera, &mut failures)
                else {
                    continue;
                };
                let tolerance = expected.tolerance.unwrap_or(1e-6);
                for (field, actual, wanted) in [
                    ("position", actual.position, expected.position),
                    ("forward", actual.forward, expected.forward),
                    ("up", actual.up, expected.up),
                ] {
                    compare_components(
                        &expected.camera,
                        field,
                        actual,
                        wanted,
                        tolerance,
                        &mut failures,
                    );
                }
            }
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
                let Some(manifest) = sampled_manifest(manifest, target.time_s, &mut failures)
                else {
                    continue;
                };
                let Some((_, camera, frame, view)) =
                    camera_setup(&manifest, &target.camera, &mut failures)
                else {
                    continue;
                };
                let origin = origin_of(&manifest, &target.entity, &mut failures);
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
                let Some(manifest) = sampled_manifest(manifest, centred.time_s, &mut failures)
                else {
                    continue;
                };
                let Some((_, camera, frame, view)) =
                    camera_setup(&manifest, &centred.camera, &mut failures)
                else {
                    continue;
                };
                let tolerance = centred.tolerance.unwrap_or(0.001);
                let origin = origin_of(&manifest, &centred.entity, &mut failures);
                let p = project(&view, &frame, &camera, origin);
                if !(p.z > 0.0 && p.x.abs() <= tolerance && p.y.abs() <= tolerance) {
                    failures.push(format!(
                        "{}: {} projects at ({:.4}, {:.4}), z {:.3} — not the frame's centre (tolerance {tolerance})",
                        centred.camera, centred.entity, p.x, p.y, p.z
                    ));
                }
            }
            Assertion::Focus { focus } => {
                let Some(manifest) = sampled_manifest(manifest, focus.time_s, &mut failures) else {
                    continue;
                };
                let Some((entity, camera, _, _)) =
                    camera_setup(&manifest, &focus.camera, &mut failures)
                else {
                    continue;
                };
                let tolerance = focus.tolerance.unwrap_or(0.001);
                match focus_distance_of(entity, &camera, &manifest) {
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
            Assertion::ShotList {
                shot_list: expected,
            } => {
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

fn sampled_manifest<'a>(
    manifest: &'a wt::WorldManifest,
    time_s: Option<f64>,
    failures: &mut Vec<String>,
) -> Option<std::borrow::Cow<'a, wt::WorldManifest>> {
    match time_s {
        None => Some(std::borrow::Cow::Borrowed(manifest)),
        Some(time_s) => match sample_world_at(manifest, time_s) {
            Ok(sampled) => Some(std::borrow::Cow::Owned(sampled)),
            Err(error) => {
                failures.push(format!("at time {time_s}: {error}"));
                None
            }
        },
    }
}

fn compare_components<T: Into<f64> + Copy>(
    entity: &str,
    field: &str,
    actual: [T; 3],
    expected: [f64; 3],
    tolerance: f64,
    failures: &mut Vec<String>,
) {
    for axis in 0..3 {
        let actual = actual[axis].into();
        if (actual - expected[axis]).abs() > tolerance {
            failures.push(format!(
                "{entity}: {field}[{axis}] {actual} != {} (tolerance {tolerance})",
                expected[axis]
            ));
        }
    }
}

/// The named camera entity, its component, frame and view in the world
/// — or a failure line and `None`.
fn camera_setup<'a>(
    manifest: &'a wt::WorldManifest,
    name: &str,
    failures: &mut Vec<String>,
) -> Option<(
    &'a wt::WorldEntity,
    CameraComponent,
    CameraFrame,
    CameraView,
)> {
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
    fn strict_track_errors_name_the_offending_field() {
        let duplicate = entity(
            1,
            "duplicate",
            serde_json::json!({"track": {"channels": {"translation": [
                {"time_s": 0, "value": [0, 0, 0]},
                {"time_s": 0, "value": [1, 0, 0]}
            ]}}}),
        );
        let error = track_of(&duplicate).unwrap_err();
        assert_eq!(
            error.pointer,
            "/ext-cinematography/track/channels/translation/1/time_s"
        );
        let unknown = entity(
            2,
            "unknown",
            serde_json::json!({"track": {"channels": {"translation": [
                {"time_s": 0, "value": [0, 0, 0], "tangents": []}
            ]}}}),
        );
        assert_eq!(
            track_of(&unknown).unwrap_err().pointer,
            "/ext-cinematography/track/channels/translation/0/tangents"
        );
    }

    #[test]
    fn world_sampling_preserves_document_data_and_does_not_mutate_the_base() {
        let manifest: WorldManifest = serde_json::from_str(
            &std::fs::read_to_string(repo("conformance/cinematography-tracks.json")).unwrap(),
        )
        .unwrap();
        let original = manifest.clone();
        let sampled = sample_world_at(&manifest, 2.0).unwrap();
        assert_eq!(manifest, original);
        assert_eq!(sampled.entities.len(), original.entities.len());
        for (before, after) in original.entities.iter().zip(&sampled.entities) {
            assert_eq!(before.id, after.id);
            assert_eq!(before.name, after.name);
            assert_eq!(before.parent, after.parent);
            assert_eq!(before.extra, after.extra);
            assert_eq!(before.behaviors, after.behaviors);
        }
        let preserved = by_name(&original, "preserved");
        assert!(!by_name(&original, "partial").transform.visible);
        assert!(!by_name(&sampled, "partial").transform.visible);
        assert_eq!(preserved.transform.position, [7.0, 8.0, 9.0]);
        let round_tripped: WorldEntity =
            serde_json::from_value(serde_json::to_value(preserved).unwrap()).unwrap();
        assert_eq!(&round_tripped, preserved);
        assert_eq!(sample_world_at(&original, 2.0).unwrap(), sampled);
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
        assert_eq!(EXTENSION_VERSION, "0.4.0");
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
        let turned = view_of(
            &turned_entity,
            &CameraComponent::of(&turned_entity).unwrap(),
        );
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
        let expected = [
            0.8 + 0.3 * 30f64.to_radians().cos(),
            1.05,
            -1.0 - 0.3 * 30f64.to_radians().sin(),
        ];
        for i in 0..3 {
            assert!(
                (mug[i] - expected[i]).abs() < 1e-5,
                "axis {i}: {} vs {}",
                mug[i],
                expected[i]
            );
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
        let p = project(
            &view,
            &frame_of(&camera),
            &camera,
            world_origin(by_name(&manifest, "mug"), &manifest),
        );
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
        assert!(
            (view.forward[2] + 1.0).abs() < 1e-9,
            "looks down −Z at the aim point"
        );
    }

    #[test]
    fn shot_list_orders_by_shot_order_ties_by_entity_id_and_skips_shot_less_cameras() {
        let manifest = cinema_world();
        let shots = shot_list(&manifest);
        let names: Vec<&str> = shots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["1A", "2A", "2B", "3A", "4A"]); // bts and insert carry no shot
        // Absent orders sort last; ties break by id.
        let mut world = WorldManifest::new("t");
        world
            .entities
            .push(entity(7, "b", serde_json::json!({"shot": {}})));
        world
            .entities
            .push(entity(3, "a", serde_json::json!({"shot": {}})));
        world
            .entities
            .push(entity(5, "c", serde_json::json!({"shot": {"order": 1}})));
        let shots = shot_list(&world);
        let names: Vec<&str> = shots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["c", "a", "b"]);
    }
}
