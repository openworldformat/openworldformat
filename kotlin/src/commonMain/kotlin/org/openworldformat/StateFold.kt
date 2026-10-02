// The typed state document (state.json) and its fold.
//
// State ops never touch entities — this pass is separate, and equally
// tolerant: keys nothing declares are carried, not refused. A save
// game is base + declaration + a player's log. Spec: spec/state.md.

package org.openworldformat

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

/** A declared state field (`schema/state.schema.json`). */
data class StateField(
    /** One of int, float, bool, string, map, list, json. */
    val type: String,
    val initial: JsonElement?,
) {
    companion object {
        operator fun invoke(json: JsonElement?): StateField {
            val o = json?.obj
            return StateField(
                type = o?.get("type")?.str ?: "json",
                initial = o?.get("initial"),
            )
        }
    }
}

/** The typed state document: `{format_version, fields}`. */
data class StateDocument(
    val formatVersion: Int? = null,
    val fields: Map<String, StateField> = emptyMap(),
) {
    companion object {
        operator fun invoke(json: JsonElement?): StateDocument {
            val o = json?.obj
            return StateDocument(
                formatVersion = o?.get("format_version")?.int,
                fields = (o?.get("fields")?.obj ?: JsonObject(emptyMap()))
                    .mapValues { (_, v) -> StateField(v) },
            )
        }
    }
}

/** What [foldState] produced: the values at the last entry, and the
 *  keys no declaration claimed — carried, and said so. */
data class StateFoldResult(
    val values: Map<String, JsonElement>,
    val undeclared: List<String>,
)

/**
 * Fold a session log's `state` ops over a state document: the values
 * at the last entry.
 *
 * A declared field is set by its value, or reset to its initial by
 * null. A dotted key under a declared map field ("inventory.rope"
 * under the map "inventory") sets or removes that entry. A key
 * declared by no one is carried — null removes it — and named in
 * [StateFoldResult.undeclared].
 */
fun foldState(stateDoc: StateDocument?, entries: List<LogEntry>): StateFoldResult {
    val fields = stateDoc?.fields ?: emptyMap()
    val values = LinkedHashMap<String, JsonElement>()
    for ((key, field) in fields) {
        values[key] = field.initial ?: kotlinx.serialization.json.JsonNull
    }
    val undeclared = mutableListOf<String>()

    for (entry in entries) {
        val classified = if (entry.classified.isEmpty() && entry.ops.isNotEmpty()) {
            entry.ops.map(::classifyOp)
        } else {
            entry.classified
        }
        for (op in classified) {
            if (op !is ClassifiedOp.State) continue
            for ((key, value) in op.value) {
                val field = fields[key]
                if (field != null) {
                    // A declared field: set it, or reset it to its initial.
                    values[key] = if (value.isNull) field.initial ?: kotlinx.serialization.json.JsonNull else value
                    continue
                }
                // Maybe a subkey of a declared map field.
                val dot = key.indexOf('.')
                if (dot > 0) {
                    val base = key.substring(0, dot)
                    val inner = key.substring(dot + 1)
                    if (fields[base]?.type == "map") {
                        val map = LinkedHashMap((values[base] as? JsonObject)?.toList()?.toMap() ?: emptyMap())
                        if (value.isNull) map.remove(inner) else map[inner] = value
                        values[base] = JsonObject(map)
                        continue
                    }
                }
                // Declared by no one: carry it, and say so.
                if (value.isNull) values.remove(key) else values[key] = value
                if (key !in undeclared) undeclared.add(key)
            }
        }
    }
    return StateFoldResult(values, undeclared)
}
