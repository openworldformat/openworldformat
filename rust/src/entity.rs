//! WorldEntity — the composable entity definition.
//!
//! Instead of separate disconnected systems for geometry, audio, lights,
//! and behaviors, one entity can have **any combination** of component slots.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::asset::MeshAssetRef;
use crate::audio::AudioDef;
use crate::behavior::BehaviorDef;
use crate::identity::{CreationId, EntityId, EntityName};
use crate::instance::InstanceOf;
use crate::light::LightDef;
use crate::material::MaterialDef;
use crate::modulation::ModulationDef;
use crate::shape::Shape;
use crate::spatial::ChunkCoord;
use crate::trigger::TriggerDef;

/// Transform in world space (or parent-relative if parented).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WorldTransform {
    /// Position [x, y, z].
    #[serde(default)]
    pub position: [f32; 3],
    /// Euler rotation in degrees [pitch, yaw, roll].
    #[serde(default)]
    pub rotation_degrees: [f32; 3],
    /// Scale [x, y, z].
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
    /// Visibility flag.
    #[serde(default = "default_true")]
    pub visible: bool,
}

impl Default for WorldTransform {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            rotation_degrees: [0.0, 0.0, 0.0],
            scale: default_scale(),
            visible: true,
        }
    }
}

fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

fn default_true() -> bool {
    true
}

/// A single entity in the world.  Component slots are all optional —
/// any combination is valid (e.g., a glowing orb has shape + light + audio).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WorldEntity {
    /// Stable numeric identifier.
    pub id: EntityId,
    /// Human-readable name.
    pub name: EntityName,
    /// Spatial transform.
    #[cfg_attr(feature = "schema", schemars(default))]
    pub transform: WorldTransform,
    /// Parent entity (for hierarchy).
    pub parent: Option<EntityId>,
    /// Spatial chunk assignment (for large worlds).
    pub chunk: Option<ChunkCoord>,
    /// If this entity belongs to a compound creation.
    pub creation_id: Option<CreationId>,

    // ---- Component slots (all optional) ----
    /// Parametric shape — never loses dimension info.
    pub shape: Option<Shape>,
    /// PBR material properties.
    pub material: Option<MaterialDef>,
    /// Light source — can coexist with shape (e.g., glowing orb).
    pub light: Option<LightDef>,
    /// Behaviors stack — multiple can be active simultaneously.
    #[cfg_attr(feature = "schema", schemars(default))]
    pub behaviors: Vec<BehaviorDef>,
    /// Audio source — spatial or ambient.
    pub audio: Option<AudioDef>,
    /// Reference to an imported mesh asset (alternative to Shape).
    pub mesh_asset: Option<MeshAssetRef>,
    /// Signal-driven modulations (soundtrack energy, beat, oscillators)
    /// stacked on top of the authored values.
    #[cfg_attr(feature = "schema", schemars(default))]
    pub modulations: Vec<ModulationDef>,
    /// Places a copy of a reusable creation here, with per-part overrides
    /// (see [`crate::instance`]).
    pub instance_of: Option<InstanceOf>,
    /// Events and the actions they run (see [`crate::trigger`]).
    #[cfg_attr(feature = "schema", schemars(default))]
    pub triggers: Vec<TriggerDef>,
    /// Extension fields (`ext-*`), namespaced and must-ignored: what a
    /// reader doesn't understand rides along unchanged (the physics
    /// extension's body component lives here today). The typed fields
    /// above are the contract; this map is the sanctioned room around
    /// them. An empty map serializes to nothing.
    #[cfg_attr(feature = "schema", schemars(default))]
    pub extra: BTreeMap<String, serde_json::Value>,
}

// The hand-written serde matches what the derive + flatten produced in
// JSON (a flat object, extras inline) while reading BOTH RON forms:
// serde(flatten) forces map-form deserialization, which rejects RON's
// named-struct syntax — the form every hand-authored world.ron uses.
// Entering through `deserialize_any` accepts named structs, RON
// map-form, and JSON objects alike.

/// An `Option` that serializes `Some(x)` / `None` explicitly — RON
/// writes the markers, JSON ignores them (value / null) — so one
/// serializer serves both dialects and RON round-trips through the
/// strict `deserialize_option`.
pub(crate) struct SomeRef<'a, T>(pub(crate) &'a Option<T>);

/// [`SomeRef`] over a borrowed inner — for `Option<Option<T>>` slots
/// without cloning the value out.
struct SomeRefBorrowed<'a, T>(Option<&'a T>);
impl<T: Serialize> Serialize for SomeRefBorrowed<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Some(value) => serializer.serialize_some(value),
            None => serializer.serialize_none(),
        }
    }
}
impl<T: Serialize> Serialize for SomeRef<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Some(value) => serializer.serialize_some(value),
            None => serializer.serialize_none(),
        }
    }
}

/// Extension keys interned to `&'static str` so struct-form
/// serialization can emit them (`serialize_field` demands static keys).
/// The set of extension namespaces is small and stable by design.
pub(crate) fn interned(key: &str) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static TABLE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let table = TABLE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = table.lock().unwrap();
    if let Some(existing) = guard.get(key) {
        return existing;
    }
    let leaked: &'static str = Box::leak(key.to_string().into_boxed_str());
    guard.insert(key.to_string(), leaked);
    leaked
}

impl Serialize for WorldEntity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        // Struct-form (RON named, readable) when there are no extension
        // fields; map-form when there are — a dashed namespace key
        // cannot be spelled as a RON identifier. JSON is identical
        // either way, and JSON is the canonical dialect for
        // extension-bearing packages.
        if self.extra.is_empty() {
            let mut s = serializer.serialize_struct("WorldEntity", 3)?;
            s.serialize_field("id", &self.id)?;
            s.serialize_field("name", &self.name)?;
            s.serialize_field("transform", &self.transform)?;
            fn slot<'a, M, T>(
                s: &mut M,
                key: &'static str,
                value: &'a Option<T>,
            ) -> Result<(), M::Error>
            where
                M: SerializeStruct,
                T: Serialize + 'a,
            {
                s.serialize_field(key, &SomeRef(value))
            }
            slot(&mut s, "parent", &self.parent)?;
            slot(&mut s, "chunk", &self.chunk)?;
            slot(&mut s, "creation_id", &self.creation_id)?;
            slot(&mut s, "shape", &self.shape)?;
            slot(&mut s, "material", &self.material)?;
            slot(&mut s, "light", &self.light)?;
            if !self.behaviors.is_empty() {
                s.serialize_field("behaviors", &self.behaviors)?;
            }
            slot(&mut s, "audio", &self.audio)?;
            slot(&mut s, "mesh_asset", &self.mesh_asset)?;
            if !self.modulations.is_empty() {
                s.serialize_field("modulations", &self.modulations)?;
            }
            slot(&mut s, "instance_of", &self.instance_of)?;
            if !self.triggers.is_empty() {
                s.serialize_field("triggers", &self.triggers)?;
            }
            s.end()
        } else {
            self.serialize_map_form(serializer)
        }
    }
}

impl WorldEntity {
    fn serialize_map_form<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(3 + self.extra.len()))?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("transform", &self.transform)?;
        fn opt<M, T: Serialize>(
            map: &mut M,
            key: &'static str,
            value: &Option<T>,
        ) -> Result<(), M::Error>
        where
            M: serde::ser::SerializeMap,
        {
            if value.is_some() {
                map.serialize_entry(key, &SomeRef(value))?;
            }
            Ok(())
        }
        opt(&mut map, "parent", &self.parent)?;
        opt(&mut map, "chunk", &self.chunk)?;
        opt(&mut map, "creation_id", &self.creation_id)?;
        opt(&mut map, "shape", &self.shape)?;
        opt(&mut map, "material", &self.material)?;
        opt(&mut map, "light", &self.light)?;
        opt(&mut map, "audio", &self.audio)?;
        opt(&mut map, "mesh_asset", &self.mesh_asset)?;
        opt(&mut map, "instance_of", &self.instance_of)?;
        if !self.behaviors.is_empty() {
            map.serialize_entry("behaviors", &self.behaviors)?;
        }
        if !self.modulations.is_empty() {
            map.serialize_entry("modulations", &self.modulations)?;
        }
        if !self.triggers.is_empty() {
            map.serialize_entry("triggers", &self.triggers)?;
        }
        for (key, value) in &self.extra {
            map.serialize_entry(interned(key), value)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for WorldEntity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = WorldEntity;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a world entity")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<WorldEntity, A::Error> {
                let mut entity = WorldEntity::new(0, "");
                entity.extra = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => entity.id = map.next_value()?,
                        "name" => entity.name = map.next_value()?,
                        "transform" => entity.transform = map.next_value()?,
                        "parent" => entity.parent = map.next_value::<NullOr<_>>()?.into_option(),
                        "chunk" => entity.chunk = map.next_value::<NullOr<_>>()?.into_option(),
                        "creation_id" => {
                            entity.creation_id = map.next_value::<NullOr<_>>()?.into_option()
                        }
                        "shape" => entity.shape = map.next_value::<NullOr<_>>()?.into_option(),
                        "material" => {
                            entity.material = map.next_value::<NullOr<_>>()?.into_option()
                        }
                        "light" => entity.light = map.next_value::<NullOr<_>>()?.into_option(),
                        "behaviors" => entity.behaviors = map.next_value()?,
                        "audio" => entity.audio = map.next_value::<NullOr<_>>()?.into_option(),
                        "mesh_asset" => {
                            entity.mesh_asset = map.next_value::<NullOr<_>>()?.into_option()
                        }
                        "modulations" => entity.modulations = map.next_value()?,
                        "instance_of" => {
                            entity.instance_of = map.next_value::<NullOr<_>>()?.into_option()
                        }
                        "triggers" => entity.triggers = map.next_value()?,
                        _ => {
                            let value: serde_json::Value = map.next_value()?;
                            entity.extra.insert(key, value);
                        }
                    }
                }
                if entity.name.as_str().is_empty() {
                    return Err(serde::de::Error::custom("an entity needs a name"));
                }
                Ok(entity)
            }
        }
        // `deserialize_any`: the one entry that accepts RON named-struct
        // syntax AND map syntax AND JSON objects — see the impl note above.
        deserializer.deserialize_any(Visitor)
    }
}

impl WorldEntity {
    /// Create a minimal entity with just an ID and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id: EntityId(id),
            name: EntityName::new(name),
            transform: WorldTransform::default(),
            parent: None,
            chunk: None,
            creation_id: None,
            shape: None,
            material: None,
            light: None,
            behaviors: Vec::new(),
            audio: None,
            mesh_asset: None,
            modulations: Vec::new(),
            instance_of: None,
            triggers: Vec::new(),
            extra: BTreeMap::new(),
        }
    }

    /// Builder: set shape.
    pub fn with_shape(mut self, shape: Shape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// Builder: set material.
    pub fn with_material(mut self, material: MaterialDef) -> Self {
        self.material = Some(material);
        self
    }

    /// Builder: set light.
    pub fn with_light(mut self, light: LightDef) -> Self {
        self.light = Some(light);
        self
    }

    /// Builder: add a signal-driven modulation.
    pub fn with_modulation(mut self, modulation: ModulationDef) -> Self {
        self.modulations.push(modulation);
        self
    }

    /// Builder: add a behavior.
    pub fn with_behavior(mut self, behavior: BehaviorDef) -> Self {
        self.behaviors.push(behavior);
        self
    }

    /// Builder: set audio.
    pub fn with_audio(mut self, audio: AudioDef) -> Self {
        self.audio = Some(audio);
        self
    }

    /// Builder: set position.
    pub fn at(mut self, position: [f32; 3]) -> Self {
        self.transform.position = position;
        self
    }
}

/// Patch for modifying an existing entity.
///
/// Uses `Option<Option<T>>` semantics:
/// - `None` — field not changed
/// - `Some(None)` — field removed/cleared
/// - `Some(Some(v))` — field set to `v`
///
/// Serialize/Deserialize are hand-written (not derived) because the
/// derived form has no room for extension keys: an `ext-*` key in a
/// patch sets (or clears, on `null`) the entity's [`WorldEntity::extra`]
/// entry, exactly as the fold's core fields patch — and `serde(flatten)`
/// can't be used here, since flattening collapses an explicit `null`
/// into an absent key and would erase the clear-field semantics above.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EntityPatch {
    pub name: Option<EntityName>,
    pub transform: Option<WorldTransform>,
    pub parent: Option<Option<EntityId>>,
    pub shape: Option<Option<Shape>>,
    pub material: Option<Option<MaterialDef>>,
    pub light: Option<Option<LightDef>>,
    pub behaviors: Option<Vec<BehaviorDef>>,
    pub audio: Option<Option<AudioDef>>,
    pub mesh_asset: Option<Option<MeshAssetRef>>,
    pub modulations: Option<Vec<ModulationDef>>,
    pub instance_of: Option<Option<InstanceOf>>,
    pub triggers: Option<Vec<TriggerDef>>,
    /// Extension fields (`ext-*`) this patch sets — or clears, on `None`.
    /// Other unknown keys are dropped, matching the reference fold.
    //
    // Not an `Option`, so schemars would call it required — but the
    // hand-written `Serialize` omits it when empty, and a patch that
    // touches no `ext-*` field (the common case) carries no `extra`.
    // `default` tells the schema what the serializer already does.
    #[cfg_attr(feature = "schema", schemars(default))]
    pub extra: BTreeMap<String, Option<serde_json::Value>>,
}

impl Serialize for EntityPatch {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        // A set slot writes its value; a clearing slot writes its inner
        // `None` — `null` in JSON, `None` in RON — so the format, not
        // this code, spells the clear.
        fn slot<M, T>(
            map: &mut M,
            key: &'static str,
            value: &Option<Option<T>>,
        ) -> Result<(), M::Error>
        where
            M: SerializeMap,
            T: Serialize,
        {
            // One `Some`, not two: `Option<Option<T>>` is the patch's
            // set-or-clear semantic, but the wire form is a single
            // option (`Some(v)` sets, `None` clears) — RON's any-mode
            // cannot serve options nested in options, and no reader
            // wants `Some(Some(..))` anyway.
            if let Some(inner) = value {
                match inner {
                    Some(v) => map.serialize_entry(key, &SomeRefBorrowed(Some(v)))?,
                    None => map.serialize_entry(key, &SomeRefBorrowed(None::<&T>))?,
                }
            }
            Ok(())
        }
        let set = [
            self.name.is_some(),
            self.transform.is_some(),
            self.parent.is_some(),
            self.shape.is_some(),
            self.material.is_some(),
            self.light.is_some(),
            self.behaviors.is_some(),
            self.audio.is_some(),
            self.mesh_asset.is_some(),
            self.modulations.is_some(),
            self.instance_of.is_some(),
            self.triggers.is_some(),
        ]
        .into_iter()
        .filter(|s| *s)
        .count();
        let mut map = serializer.serialize_map(Some(set + self.extra.len()))?;
        if let Some(name) = &self.name {
            map.serialize_entry("name", name)?;
        }
        if let Some(transform) = &self.transform {
            map.serialize_entry("transform", transform)?;
        }
        slot(&mut map, "parent", &self.parent)?;
        slot(&mut map, "shape", &self.shape)?;
        slot(&mut map, "material", &self.material)?;
        slot(&mut map, "light", &self.light)?;
        slot(&mut map, "audio", &self.audio)?;
        slot(&mut map, "mesh_asset", &self.mesh_asset)?;
        slot(&mut map, "instance_of", &self.instance_of)?;
        if let Some(behaviors) = &self.behaviors {
            map.serialize_entry("behaviors", behaviors)?;
        }
        if let Some(modulations) = &self.modulations {
            map.serialize_entry("modulations", modulations)?;
        }
        if let Some(triggers) = &self.triggers {
            map.serialize_entry("triggers", triggers)?;
        }
        for (key, value) in &self.extra {
            match value {
                Some(value) => map.serialize_entry(key, value)?,
                // A clearing entry must survive the round trip: absent
                // means "not in the patch", null means "clear it".
                None => map.serialize_entry(key, &serde_json::Value::Null)?,
            }
        }
        map.end()
    }
}

/// One slot value as the patch reads it: a clear (`null` in JSON, `None`
/// in RON) or a set. The distinction `Option<Option<T>>` carries, read
/// format-agnostically — serde's own `Option` eats the clear.
enum NullOr<T> {
    /// Clear the slot.
    Null,
    /// Set the slot.
    Value(T),
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for NullOr<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
            type Value = NullOr<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a value or a clear")
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(NullOr::Null)
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(NullOr::Null)
            }
            fn visit_some<D: serde::Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<Self::Value, D::Error> {
                T::deserialize(deserializer).map(NullOr::Value)
            }
            // Bare values arrive in RON's any-mode (and in JSON, where
            // options are transparent); primitives bridge through
            // serde_json::Value because its deserializer is lenient
            // where the value.rs ones are not.
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                T::deserialize(serde_json::Value::from(v))
                    .map(NullOr::Value)
                    .map_err(serde::de::Error::custom)
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                T::deserialize(serde_json::Value::from(v))
                    .map(NullOr::Value)
                    .map_err(serde::de::Error::custom)
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                T::deserialize(serde_json::Value::from(v))
                    .map(NullOr::Value)
                    .map_err(serde::de::Error::custom)
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                T::deserialize(serde_json::Value::from(v))
                    .map(NullOr::Value)
                    .map_err(serde::de::Error::custom)
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                T::deserialize(serde_json::Value::from(v))
                    .map(NullOr::Value)
                    .map_err(serde::de::Error::custom)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                seq: A,
            ) -> Result<Self::Value, A::Error> {
                use serde::de::value::SeqAccessDeserializer;
                T::deserialize(SeqAccessDeserializer::new(seq)).map(NullOr::Value)
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                use serde::de::value::MapAccessDeserializer;
                T::deserialize(MapAccessDeserializer::new(map)).map(NullOr::Value)
            }
            fn visit_enum<A: serde::de::EnumAccess<'de>>(
                self,
                data: A,
            ) -> Result<Self::Value, A::Error> {
                use serde::de::value::EnumAccessDeserializer;
                T::deserialize(EnumAccessDeserializer::new(data)).map(NullOr::Value)
            }
        }
        // `deserialize_any`: RON's any-mode cannot serve
        // `deserialize_option` (patch slots inside instances hit
        // "Expected struct … but found Some"), so the visitor accepts
        // `Some(x)`, bare `x`, and clears — in both dialects, and JSON.
        deserializer.deserialize_any(Visitor(std::marker::PhantomData))
    }
}

impl<'de> Deserialize<'de> for EntityPatch {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = EntityPatch;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an entity patch")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<EntityPatch, A::Error> {
                let mut patch = EntityPatch::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => patch.name = Some(map.next_value()?),
                        "transform" => patch.transform = Some(map.next_value()?),
                        "parent" => patch.parent = map.next_value::<NullOr<_>>()?.into(),
                        "shape" => patch.shape = map.next_value::<NullOr<_>>()?.into(),
                        "material" => patch.material = map.next_value::<NullOr<_>>()?.into(),
                        "light" => patch.light = map.next_value::<NullOr<_>>()?.into(),
                        "audio" => patch.audio = map.next_value::<NullOr<_>>()?.into(),
                        "mesh_asset" => {
                            patch.mesh_asset = map.next_value::<NullOr<_>>()?.into();
                        }
                        "instance_of" => {
                            patch.instance_of = map.next_value::<NullOr<_>>()?.into();
                        }
                        "behaviors" => patch.behaviors = Some(map.next_value()?),
                        "modulations" => patch.modulations = Some(map.next_value()?),
                        "triggers" => patch.triggers = Some(map.next_value()?),
                        // Extension keys patch like the core ones;
                        // everything else unknown is dropped, matching
                        // the reference fold.
                        _ if key.starts_with("ext-") => {
                            let value: Option<serde_json::Value> = map.next_value()?;
                            patch.extra.insert(key, value);
                        }
                        _ => {
                            let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(patch)
            }
        }
        // `deserialize_any`: the one entry that accepts RON named-struct
        // syntax AND map syntax AND JSON objects — see the impl note above.
        deserializer.deserialize_any(Visitor)
    }
}

impl<T> From<NullOr<T>> for Option<Option<T>> {
    fn from(value: NullOr<T>) -> Self {
        match value {
            NullOr::Null => Some(None),
            NullOr::Value(value) => Some(Some(value)),
        }
    }
}

impl EntityPatch {
    /// The patch that turns `base` into `target`, field by field, leaving
    /// out `name` and `parent` (a part's place comes from its definition).
    /// Numbers within `1e-4` count as equal, so values that went through
    /// an engine's float conversions (quaternions and back, sRGB and back)
    /// don't show up as changes.
    pub fn between(base: &WorldEntity, target: &WorldEntity) -> EntityPatch {
        fn changed<T: Serialize + Clone>(a: &T, b: &T) -> Option<T> {
            (!values_close(a, b)).then(|| b.clone())
        }
        EntityPatch {
            name: None,
            parent: None,
            transform: changed(&base.transform, &target.transform),
            shape: changed(&base.shape, &target.shape),
            material: changed(&base.material, &target.material),
            light: changed(&base.light, &target.light),
            behaviors: changed(&base.behaviors, &target.behaviors),
            audio: changed(&base.audio, &target.audio),
            mesh_asset: changed(&base.mesh_asset, &target.mesh_asset),
            modulations: changed(&base.modulations, &target.modulations),
            instance_of: changed(&base.instance_of, &target.instance_of),
            triggers: changed(&base.triggers, &target.triggers),
            extra: diff_extra(&base.extra, &target.extra),
        }
    }

    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        *self == EntityPatch::default()
    }

    /// Apply this patch to a WorldEntity, modifying it in place.
    pub fn apply(&self, entity: &mut WorldEntity) {
        if let Some(ref name) = self.name {
            entity.name = name.clone();
        }
        if let Some(ref transform) = self.transform {
            entity.transform = transform.clone();
        }
        if let Some(ref parent) = self.parent {
            entity.parent = *parent;
        }
        if let Some(ref shape) = self.shape {
            entity.shape = shape.clone();
        }
        if let Some(ref material) = self.material {
            entity.material = material.clone();
        }
        if let Some(ref light) = self.light {
            entity.light = light.clone();
        }
        if let Some(ref behaviors) = self.behaviors {
            entity.behaviors = behaviors.clone();
        }
        if let Some(ref audio) = self.audio {
            entity.audio = audio.clone();
        }
        if let Some(ref mesh_asset) = self.mesh_asset {
            entity.mesh_asset = mesh_asset.clone();
        }
        if let Some(ref modulations) = self.modulations {
            entity.modulations = modulations.clone();
        }
        if let Some(ref instance_of) = self.instance_of {
            entity.instance_of = instance_of.clone();
        }
        if let Some(ref triggers) = self.triggers {
            entity.triggers = triggers.clone();
        }
        for (key, slot) in &self.extra {
            match slot {
                Some(value) => {
                    entity.extra.insert(key.clone(), value.clone());
                }
                None => {
                    entity.extra.remove(key);
                }
            }
        }
    }
}

/// The `ext-*` diff between two extra maps: present on the target sets,
/// absent on the target clears. Non-extension keys never patch.
fn diff_extra(
    base: &BTreeMap<String, serde_json::Value>,
    target: &BTreeMap<String, serde_json::Value>,
) -> BTreeMap<String, Option<serde_json::Value>> {
    let mut diff = BTreeMap::new();
    for key in base.keys().chain(target.keys()) {
        if !key.starts_with("ext-") {
            continue;
        }
        match target.get(key) {
            Some(value) => {
                if base.get(key) != Some(value) {
                    diff.insert(key.clone(), Some(value.clone()));
                }
            }
            None => {
                diff.insert(key.clone(), None);
            }
        }
    }
    diff
}

/// Whether two values serialize to the same JSON, with numbers compared to
/// within `1e-4` (relative for large values): equal up to the float noise of
/// a round trip through an engine (quaternions and back, sRGB and back).
pub fn values_close<T: Serialize>(a: &T, b: &T) -> bool {
    match (serde_json::to_value(a), serde_json::to_value(b)) {
        (Ok(a), Ok(b)) => json_close(&a, &b),
        _ => false,
    }
}

/// JSON equality with numbers compared to within `1e-4` (relative for large
/// values).
fn json_close(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => (x - y).abs() <= 1e-4 * x.abs().max(y.abs()).max(1.0),
            _ => x == y,
        },
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| json_close(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| json_close(v, w)))
        }
        _ => a == b,
    }
}

impl<T> NullOr<T> {
    /// The tolerant option: `Some(x)`, bare `x`, or a clear.
    fn into_option(self) -> Option<T> {
        match self {
            NullOr::Value(v) => Some(v),
            NullOr::Null => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioDef, AudioKind, AudioSource, Rolloff};

    #[test]
    fn patch_between_ignores_float_noise() {
        let base = WorldEntity::new(1, "trunk")
            .with_shape(Shape::Cylinder {
                radius: 0.3,
                height: 3.0,
            })
            .at([0.0, 1.5, 0.0]);
        let mut target = base.clone();
        target.name = EntityName::new("oak_1/trunk");
        target.parent = Some(EntityId(9));
        target.transform.position[1] = 1.500_001;
        target.transform.rotation_degrees[1] = -0.000_02;
        assert!(EntityPatch::between(&base, &target).is_empty());

        target.shape = Some(Shape::Cylinder {
            radius: 0.3,
            height: 4.5,
        });
        let patch = EntityPatch::between(&base, &target);
        assert_eq!(patch.shape, Some(target.shape.clone()));
        assert!(patch.transform.is_none() && patch.name.is_none());
        let mut rebuilt = base.clone();
        patch.apply(&mut rebuilt);
        assert_eq!(rebuilt.shape, target.shape);
    }

    #[test]
    fn entity_builder() {
        let e = WorldEntity::new(1, "campfire")
            .at([5.0, 0.0, 3.0])
            .with_shape(Shape::Cone {
                radius: 0.5,
                height: 1.0,
            })
            .with_light(LightDef {
                light_type: crate::light::LightType::Point,
                color: [1.0, 0.8, 0.3, 1.0],
                intensity: 500.0,
                direction: None,
                shadows: true,
                range: None,
                outer_angle: None,
                inner_angle: None,
            })
            .with_audio(AudioDef {
                kind: AudioKind::Sfx,
                source: AudioSource::Fire {
                    intensity: 0.8,
                    crackle: 0.5,
                },
                volume: 0.7,
                radius: Some(15.0),
                rolloff: Rolloff::InverseSquare,
            });

        assert_eq!(e.name.as_str(), "campfire");
        assert!(e.shape.is_some());
        assert!(e.light.is_some());
        assert!(e.audio.is_some());
        assert_eq!(e.transform.position, [5.0, 0.0, 3.0]);
    }

    #[test]
    fn entity_roundtrip() {
        let e = WorldEntity::new(42, "test_cube").with_shape(Shape::Cuboid {
            x: 2.0,
            y: 3.0,
            z: 4.0,
        });
        let json = serde_json::to_string_pretty(&e).unwrap();
        let back: WorldEntity = serde_json::from_str(&json).unwrap();
        assert_eq!(e.id, back.id);
        assert_eq!(e.name, back.name);
        assert_eq!(e.shape, back.shape);
    }

    #[test]
    fn entity_patch_apply() {
        let mut e = WorldEntity::new(1, "box");
        let patch = EntityPatch {
            name: Some(EntityName::new("renamed_box")),
            shape: Some(Some(Shape::Sphere { radius: 2.0 })),
            ..Default::default()
        };
        patch.apply(&mut e);
        assert_eq!(e.name.as_str(), "renamed_box");
        assert!(matches!(e.shape, Some(Shape::Sphere { radius }) if radius == 2.0));
    }

    #[test]
    fn entity_patch_clear_field() {
        let mut e = WorldEntity::new(1, "lit_box").with_light(LightDef::default());
        assert!(e.light.is_some());

        let patch = EntityPatch {
            light: Some(None), // Clear the light
            ..Default::default()
        };
        patch.apply(&mut e);
        assert!(e.light.is_none());
    }
}
