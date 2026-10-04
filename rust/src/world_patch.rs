//! `ModifyWorld`'s patch: the document's scene-wide fields, changed the
//! way `ModifyEntity` changes an entity (spec/session.md, "The op
//! kinds") — an absent key leaves its field alone, `null` clears it, a
//! value sets it. With it every field of a manifest is reachable by an
//! edit, so the fold's state is always a whole manifest.
//!
//! Serde is hand-written for the same reason as [`EntityPatch`]'s: a
//! clearing `null` must survive the round trip, which derived
//! `Option<Option<T>>` fields can't promise in every format. Unknown keys
//! are ignored (must-ignore); strict ingestion refuses them before they
//! get here.
//!
//! [`EntityPatch`]: crate::entity::EntityPatch

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::avatar::AvatarDef;
use crate::creation::CreationDef;
use crate::history::AmbienceLayerDef;
use crate::soundtrack::SoundtrackDef;
use crate::tour::TourDef;
use crate::world::{CameraDef, EnvironmentDef, WorldMeta};

/// The keys a world patch may carry, in the order they serialize.
pub const WORLD_PATCH_KEYS: &[&str] = &[
    "meta",
    "environment",
    "camera",
    "avatar",
    "tours",
    "soundtrack",
    "ambience",
    "creations",
];

/// A change to a world's scene-wide fields. `None` leaves a field alone;
/// for optional fields `Some(None)` clears it; for lists an empty list
/// is the cleared state (`null` deserializes to it).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WorldPatch {
    /// The world's metadata, replaced whole. It can't be cleared.
    #[cfg_attr(feature = "schema", schemars(default))]
    pub meta: Option<WorldMeta>,
    #[cfg_attr(feature = "schema", schemars(default))]
    pub environment: Option<Option<EnvironmentDef>>,
    #[cfg_attr(feature = "schema", schemars(default))]
    pub camera: Option<Option<CameraDef>>,
    #[cfg_attr(feature = "schema", schemars(default))]
    pub avatar: Option<Option<AvatarDef>>,
    #[cfg_attr(feature = "schema", schemars(default))]
    pub tours: Option<Vec<TourDef>>,
    #[cfg_attr(feature = "schema", schemars(default))]
    pub soundtrack: Option<Option<SoundtrackDef>>,
    #[cfg_attr(feature = "schema", schemars(default))]
    pub ambience: Option<Vec<AmbienceLayerDef>>,
    #[cfg_attr(feature = "schema", schemars(default))]
    pub creations: Option<Vec<CreationDef>>,
}

impl WorldPatch {
    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        *self == WorldPatch::default()
    }
}

impl Serialize for WorldPatch {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{Error, SerializeMap};
        fn value<T: Serialize, E: Error>(v: &T) -> Result<Value, E> {
            serde_json::to_value(v).map_err(E::custom)
        }
        let mut out: Vec<(&str, Value)> = Vec::new();
        if let Some(meta) = &self.meta {
            out.push(("meta", value(meta)?));
        }
        if let Some(env) = &self.environment {
            out.push(("environment", value(env)?));
        }
        if let Some(camera) = &self.camera {
            out.push(("camera", value(camera)?));
        }
        if let Some(avatar) = &self.avatar {
            out.push(("avatar", value(avatar)?));
        }
        if let Some(tours) = &self.tours {
            out.push(("tours", value(tours)?));
        }
        if let Some(soundtrack) = &self.soundtrack {
            out.push(("soundtrack", value(soundtrack)?));
        }
        if let Some(ambience) = &self.ambience {
            out.push(("ambience", value(ambience)?));
        }
        if let Some(creations) = &self.creations {
            out.push(("creations", value(creations)?));
        }
        let mut map = serializer.serialize_map(Some(out.len()))?;
        for (key, v) in out {
            map.serialize_entry(key, &v)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for WorldPatch {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let map = Map::<String, Value>::deserialize(deserializer)?;
        let mut patch = WorldPatch::default();
        // A present key: `null` is the clear, anything else a value.
        fn slot<T: serde::de::DeserializeOwned, E: Error>(
            key: &str,
            v: Value,
        ) -> Result<Option<T>, E> {
            if v.is_null() {
                Ok(None)
            } else {
                serde_json::from_value(v)
                    .map(Some)
                    .map_err(|e| E::custom(format!("ModifyWorld.{key}: {e}")))
            }
        }
        for (key, v) in map {
            match key.as_str() {
                "meta" => {
                    patch.meta = Some(
                        slot::<WorldMeta, D::Error>("meta", v)?
                            .ok_or_else(|| D::Error::custom("ModifyWorld.meta can't be cleared"))?,
                    )
                }
                "environment" => patch.environment = Some(slot("environment", v)?),
                "camera" => patch.camera = Some(slot("camera", v)?),
                "avatar" => patch.avatar = Some(slot("avatar", v)?),
                "tours" => patch.tours = Some(slot("tours", v)?.unwrap_or_default()),
                "soundtrack" => patch.soundtrack = Some(slot("soundtrack", v)?),
                "ambience" => patch.ambience = Some(slot("ambience", v)?.unwrap_or_default()),
                "creations" => patch.creations = Some(slot("creations", v)?.unwrap_or_default()),
                _ => {} // must-ignore
            }
        }
        Ok(patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_null_and_value_mean_three_things() {
        let patch: WorldPatch =
            serde_json::from_str(r#"{"soundtrack": null, "tours": [], "camera": {"position": [0,1,2], "look_at": [0,0,0]}}"#)
                .unwrap();
        assert_eq!(patch.soundtrack, Some(None), "null clears");
        assert_eq!(patch.tours, Some(vec![]));
        assert!(matches!(patch.camera, Some(Some(_))));
        assert_eq!(patch.avatar, None, "absent leaves alone");

        let back = serde_json::to_value(&patch).unwrap();
        assert_eq!(
            back["soundtrack"],
            Value::Null,
            "a clear survives the round trip"
        );
        assert!(back.get("avatar").is_none());
    }

    #[test]
    fn meta_cannot_be_cleared_and_unknown_keys_are_ignored() {
        assert!(serde_json::from_str::<WorldPatch>(r#"{"meta": null}"#).is_err());
        let patch: WorldPatch = serde_json::from_str(r#"{"weather": "rain"}"#).unwrap();
        assert!(patch.is_empty());
    }
}
