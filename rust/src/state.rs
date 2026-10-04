//! The state document and its fold: where the game is, not what the
//! scene is (spec/state.md). A save game is base + state declaration +
//! a player's session log — the fold below is the arithmetic.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::oplog::OpLogEntry;
use crate::session::SessionOp;

/// `state.json` — the world's declared state and its initial values.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateDoc {
    /// The state document's own version.
    pub format_version: u32,
    /// The declared fields: dotted name → `{type, initial}`.
    pub fields: BTreeMap<String, StateField>,
}

/// One declared field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateField {
    /// The field's type: int, float, bool, string, map, list, json.
    #[serde(rename = "type")]
    pub kind: String,
    /// The initial value; absent means the type's zero (or null for json).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial: Option<Value>,
}

impl StateField {
    /// The value a reset returns to: the declared initial, else the
    /// type's zero.
    pub fn initial_value(&self) -> Value {
        self.initial
            .clone()
            .unwrap_or_else(|| match self.kind.as_str() {
                "int" | "float" => Value::from(0),
                "bool" => Value::from(false),
                "string" => Value::from(""),
                "map" => serde_json::json!({}),
                "list" => serde_json::json!([]),
                _ => Value::Null,
            })
    }
}

/// A state fold's result.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StateFold {
    /// The values at the last entry.
    pub values: BTreeMap<String, Value>,
    /// Keys no declaration covers — carried, and said so, because
    /// history must never break the fold.
    pub undeclared: Vec<String>,
}

/// Fold a session log's `state` ops over a state document: the values at
/// the last entry. Separate from the document fold — state ops never
/// touch entities — and equally tolerant.
///
/// The rules (spec/state.md): a key naming a declared field sets it,
/// and `null` resets it to its initial; a key *under* a declared `map`
/// field sets or removes that entry; a key declaring nothing is carried
/// and reported.
pub fn fold_state(state_doc: &StateDoc, entries: &[OpLogEntry]) -> StateFold {
    let mut values: BTreeMap<String, Value> = state_doc
        .fields
        .iter()
        .map(|(name, field)| (name.clone(), field.initial_value()))
        .collect();
    let mut undeclared: Vec<String> = Vec::new();

    for entry in entries {
        for op in &entry.ops {
            let SessionOp::State(record) = op else {
                continue;
            };
            for (key, value) in &record.state {
                if state_doc.fields.contains_key(key) {
                    match value {
                        Value::Null => {
                            values.insert(key.clone(), state_doc.fields[key].initial_value());
                        }
                        v => {
                            values.insert(key.clone(), v.clone());
                        }
                    }
                    continue;
                }
                if let Some((base, inner)) = key.split_once('.')
                    && state_doc.fields.get(base).map(|f| f.kind.as_str()) == Some("map")
                    && let Some(Value::Object(map)) = values.get_mut(base)
                {
                    match value {
                        Value::Null => {
                            map.remove(inner);
                        }
                        v => {
                            map.insert(inner.to_string(), v.clone());
                        }
                    }
                    continue;
                }
                match value {
                    Value::Null => {
                        values.remove(key);
                    }
                    v => {
                        values.insert(key.clone(), v.clone());
                        if !undeclared.contains(key) {
                            undeclared.push(key.clone());
                        }
                    }
                }
            }
        }
    }
    StateFold { values, undeclared }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::StateRecord;

    fn entry(state: &[(&str, Value)]) -> OpLogEntry {
        OpLogEntry {
            revision: 1,
            author: crate::author::Author {
                peer: None,
                name: "t".into(),
            },
            ops: vec![SessionOp::State(StateRecord {
                state: state
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
            })],
            timestamp_ms: 0,
            id: None,
            parent: None,
            message: None,
        }
    }

    /// spec/state.md: if a declared field exactly matches a dotted key,
    /// the declared field ALWAYS takes precedence over map sub-keys.
    /// Declaring both `inventory` (a map) and `inventory.rope` (an int)
    /// is the whole game.
    #[test]
    fn a_declared_field_beats_the_map_subkey_it_resembles() {
        let state_doc = StateDoc {
            format_version: 1,
            fields: BTreeMap::from([
                (
                    "inventory".to_string(),
                    StateField {
                        kind: "map".into(),
                        initial: Some(serde_json::json!({})),
                    },
                ),
                (
                    "inventory.rope".to_string(),
                    StateField {
                        kind: "int".into(),
                        initial: Some(serde_json::json!(0)),
                    },
                ),
            ]),
        };
        let folded = fold_state(
            &state_doc,
            &[entry(&[("inventory.rope", serde_json::json!(5))])],
        );
        assert_eq!(folded.values["inventory.rope"], serde_json::json!(5));
        // The map kept its initial emptiness: not a sub-key hit.
        assert_eq!(folded.values["inventory"], serde_json::json!({}));
        assert!(folded.undeclared.is_empty());
        // And a null on the declared field resets it, rather than
        // deleting an inventory entry.
        let folded = fold_state(&state_doc, &[entry(&[("inventory.rope", Value::Null)])]);
        assert_eq!(folded.values["inventory.rope"], serde_json::json!(0));
    }

    #[test]
    fn map_subkeys_still_work_without_a_shadowing_declaration() {
        let state_doc = StateDoc {
            format_version: 1,
            fields: BTreeMap::from([(
                "inventory".to_string(),
                StateField {
                    kind: "map".into(),
                    initial: Some(serde_json::json!({})),
                },
            )]),
        };
        let folded = fold_state(
            &state_doc,
            &[entry(&[
                ("inventory.rope", serde_json::json!(1)),
                ("inventory.torch", serde_json::json!(2)),
                ("inventory.rope", Value::Null),
            ])],
        );
        assert_eq!(folded.values["inventory"], serde_json::json!({"torch": 2}));
    }
}
