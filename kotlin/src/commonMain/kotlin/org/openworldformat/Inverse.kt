// Undo. Every edit has a computable inverse, and undo is appending
// that inverse — the log never rewinds (spec/session.md "The op
// kinds"). The inverse is computed at the time of the edit, against
// the state the edit is about to change.

package org.openworldformat

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** The format's default camera — what a SetCamera inverses to when the
 *  fold holds no camera of its own: `{"position":[5,5,5],"look_at":
 *  [0,0,0],"fov_degrees":45}`, integers as every reference writes it. */
private val FORMAT_DEFAULT_CAMERA: JsonObject = buildJsonObject {
    put("position", JsonArray(listOf(5L, 5L, 5L).map { JsonPrimitive(it) }))
    put("look_at", JsonArray(listOf(0L, 0L, 0L).map { JsonPrimitive(it) }))
    put("fov_degrees", JsonPrimitive(45L))
}

/**
 * The inverse of one edit op, computed against the state the op is
 * about to change (the state *before* it applies):
 *
 * - `SpawnEntity` → `DeleteEntity` by id
 * - `DeleteEntity` → a `Batch` of `SpawnEntity` ops restoring the
 *   deleted subtree, parents first, verbatim
 * - `ModifyEntity` → the inverse patch: every patch key set to the
 *   entity's current value, or `null` when absent (name and parent
 *   included)
 * - `SetEnvironment` / `SetCamera` / `SetAmbience` → the previous
 *   value, or the empty form (format defaults, for the camera)
 * - `SpawnAudioEmitter` ↔ `RemoveAudioEmitter`, symmetric
 * - `Batch` → the member inverses in reverse order, each computed
 *   against a running trial of the batch
 *
 * @throws [WorldFormatException] for ops that aren't edits, and for
 *   edits naming entities the state doesn't hold.
 */
fun computeInverse(op: JsonElement, state: FoldState): JsonElement {
    val classified = classifyOp(op)
    if (classified !is ClassifiedOp.Edit) {
        throw WorldFormatException.invalid("no inverse for a non-edit op")
    }
    val value = classified.value
    return when (classified.name) {
        "SpawnEntity" -> {
            val id = value.obj?.get("entity")?.obj?.get("id")?.int
                ?: throw WorldFormatException.invalid("SpawnEntity needs an entity with id and name")
            buildJsonObject { put("DeleteEntity", buildJsonObject { put("id", id) }) }
        }

        "DeleteEntity" -> {
            val id = value.obj?.get("id")?.int
                ?: throw WorldFormatException.invalid("DeleteEntity needs an id")
            val byId = idIndexForInverse(state)
            byId[id] ?: throw WorldFormatException.invalid("no entity $id")
            // The subtree goes with it; the inverse restores it whole,
            // parents first (a child can't spawn before its parent).
            val doomed = mutableSetOf(id)
            var grew = true
            while (grew) {
                grew = false
                for (e in state.entities) {
                    if (e.parent != null && e.parent in doomed && e.id !in doomed) {
                        doomed.add(e.id)
                        grew = true
                    }
                }
            }
            val subtree = state.entities.filter { it.id in doomed }.sortedBy { depthOf(it, byId) }
            buildJsonObject {
                put("Batch", buildJsonObject {
                    put("ops", buildJsonArray {
                        for (e in subtree) {
                            add(buildJsonObject {
                                put("SpawnEntity", buildJsonObject { put("entity", e.toJson()) })
                            })
                        }
                    })
                })
            }
        }

        "ModifyEntity" -> {
            val id = value.obj?.get("id")?.int
                ?: throw WorldFormatException.invalid("ModifyEntity needs an id")
            val entity = state.entities.firstOrNull { it.id == id }
                ?: throw WorldFormatException.invalid("no entity $id")
            val patch = value.obj?.get("patch")?.obj ?: JsonObject(emptyMap())
            // Absent-in-the-entity becomes null in the inverse patch:
            // the undo clears what the edit set.
            val inversePatch = buildJsonObject {
                for ((key, _) in patch) {
                    when (key) {
                        "name" -> put("name", entity.name)
                        "parent" -> entity.parent?.let { put("parent", it) } ?: put("parent", JsonNull)
                        else -> {
                            val current = entity.fields[key]
                            if (current == null) put(key, JsonNull) else put(key, current)
                        }
                    }
                }
            }
            buildJsonObject {
                put("ModifyEntity", buildJsonObject {
                    put("id", id)
                    put("patch", inversePatch)
                })
            }
        }

        "SetEnvironment" -> buildJsonObject {
            put("SetEnvironment", buildJsonObject {
                put("env", state.environment?.toJson() ?: JsonObject(emptyMap()))
            })
        }

        "SetCamera" -> buildJsonObject {
            put("SetCamera", buildJsonObject {
                put("camera", state.camera?.toJson() ?: FORMAT_DEFAULT_CAMERA)
            })
        }

        "SetAmbience" -> buildJsonObject {
            put("SetAmbience", buildJsonObject { put("ambience", JsonArray(state.ambience)) })
        }

        "SpawnAudioEmitter" -> {
            val name = value.obj?.get("name")?.str
                ?: throw WorldFormatException.invalid("SpawnAudioEmitter needs a name")
            buildJsonObject { put("RemoveAudioEmitter", buildJsonObject { put("name", name) }) }
        }

        "RemoveAudioEmitter" -> {
            val name = value.obj?.get("name")?.str
                ?: throw WorldFormatException.invalid("RemoveAudioEmitter needs a name")
            val audio = state.audioEmitters[name]
                ?: throw WorldFormatException.invalid("no audio emitter named '$name'")
            buildJsonObject {
                put("SpawnAudioEmitter", buildJsonObject {
                    put("name", name)
                    put("audio", audio)
                })
            }
        }

        "Batch" -> {
            val ops = value.obj?.get("ops")?.arr ?: emptyList()
            var trial = state.copyForTrial()
            val inverses = mutableListOf<JsonElement>()
            for (inner in ops) {
                val c = classifyOp(inner)
                if (c is ClassifiedOp.Edit) {
                    inverses.add(computeInverse(inner, trial))
                    trial = applyEdit(trial, c.name, c.value)
                }
            }
            inverses.reverse()  // undo runs back-to-front
            buildJsonObject { put("Batch", buildJsonObject { put("ops", JsonArray(inverses)) }) }
        }

        else -> throw WorldFormatException.invalid("no inverse for ${classified.name}")
    }
}

/** [idIndex] is private to Fold.kt; the same derived index, for here. */
private fun idIndexForInverse(state: FoldState): Map<Int, WorldEntity> =
    state.entities.associateBy { it.id }

/** An entity's depth: 0 at a root, one more than its parent. */
private fun depthOf(entity: WorldEntity, byId: Map<Int, WorldEntity>): Int {
    var depth = 0
    var cursor = entity.parent
    while (cursor != null) {
        depth += 1
        cursor = byId[cursor]?.parent
    }
    return depth
}

/** [buildJsonArray] — kept here so the branches read cleanly. */
private fun buildJsonArray(builder: kotlinx.serialization.json.JsonArrayBuilder.() -> Unit): JsonArray =
    kotlinx.serialization.json.buildJsonArray(builder)
