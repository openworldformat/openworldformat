// The fold: state at any revision is a pure fold of the log over the
// base. Only edits change the document; history kinds fold to nothing;
// a batch applies all-or-nothing; and an entry that no longer applies
// stops the fold, exactly as the specification's readers do.
//
// [FoldState.copyForTrial] is the all-or-nothing rule: JsonElement
// trees and data classes are immutable, so a trial is one copy, and
// committing is returning it — the deep copy the JS reference makes
// with structuredClone, paid only when asked for.

package org.openworldformat

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

/**
 * The document at a point in the log — what [foldLog] and [foldPath]
 * return. [path] is set by [foldPath] only: the entry ids folded.
 */
data class FoldState(
    /** The world's name (manifest meta). */
    val name: String,
    /** Base plus every applied edit. */
    val entities: List<WorldEntity>,
    val environment: EnvironmentDef?,
    val camera: CameraDef?,
    val ambience: List<JsonElement>,
    val audioEmitters: Map<String, JsonElement>,
    /** How many edit ops the folded entries carried. */
    val appliedEdits: Int,
    /** The entry ids folded, in order ([foldPath] only). */
    val path: List<String>? = null,
) {
    /** A value copy safe to mutate speculatively. */
    internal fun copyForTrial(): FoldState = copy(
        entities = entities.toList(),
        audioEmitters = HashMap(audioEmitters),
    )
}

/**
 * Fold log entries over a manifest: the document at the last entry.
 *
 * @throws [WorldFormatException] at the first entry that no longer
 *   applies — the fold stops there, as the specification's readers do.
 */
fun foldLog(manifest: WorldManifest, entries: List<LogEntry>): FoldState {
    var state = FoldState(
        name = manifest.name,
        entities = manifest.entities,
        environment = manifest.environment,
        camera = manifest.camera,
        ambience = manifest.ambience,
        audioEmitters = emptyMap(),
        appliedEdits = 0,
    )
    for (entry in entries) {
        val edits = editOps(entry)
        if (edits.isEmpty()) continue  // history folds to nothing
        var trial = state.copyForTrial()  // all-or-nothing, per entry
        for (edit in edits) {
            trial = applyEdit(trial, edit.name, edit.value)
        }
        state = trial
        state = state.copy(appliedEdits = state.appliedEdits + edits.size)
    }
    return state
}

// ---------------------------------------------------------------------------
// Applying one edit
// ---------------------------------------------------------------------------

private fun idIndex(entities: List<WorldEntity>): Map<Int, WorldEntity> =
    entities.associateBy { it.id }

private fun names(entities: List<WorldEntity>): Set<String> =
    entities.mapTo(mutableSetOf()) { it.name }

/** Apply one edit op to a fold state, returning the new state.
 *  Indexes are derived per call — the trial state is the truth, and
 *  worlds are tens of entities. */
internal fun applyEdit(state: FoldState, edit: String, value: JsonElement): FoldState = when (edit) {
        "SpawnEntity" -> {
            val raw = value.obj?.get("entity")
            val entity = try {
                raw?.let { WorldEntity(it) }
            } catch (e: WorldFormatException) {
                null
            } ?: throw WorldFormatException.invalid("SpawnEntity needs an entity with id and name")
            val byId = idIndex(state.entities)
            val nameSet = names(state.entities)
            if (byId.containsKey(entity.id)) {
                throw WorldFormatException.invalid("entity ${entity.id} already exists")
            }
            if (entity.name in nameSet) {
                throw WorldFormatException.invalid("an entity named '${entity.name}' already exists")
            }
            if (entity.parent != null && !byId.containsKey(entity.parent)) {
                throw WorldFormatException.invalid(
                    "entity ${entity.id}'s parent ${entity.parent} isn't in the document")
            }
            state.copy(entities = state.entities + entity)
        }

        "DeleteEntity" -> {
            val id = value.obj?.get("id")?.int
                ?: throw WorldFormatException.invalid("DeleteEntity needs an id")
            if (state.entities.none { it.id == id }) {
                throw WorldFormatException.invalid("no entity $id")
            }
            // Descendants go with it: collect the subtree, then remove.
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
            state.copy(entities = state.entities.filterNot { it.id in doomed })
        }

        "ModifyEntity" -> {
            val id = value.obj?.get("id")?.int
                ?: throw WorldFormatException.invalid("ModifyEntity needs an id")
            val index = state.entities.indexOfFirst { it.id == id }
            if (index < 0) throw WorldFormatException.invalid("no entity $id")
            val entity = state.entities[index]
            val patch = value.obj?.get("patch")?.obj ?: kotlinx.serialization.json.JsonObject(emptyMap())
            val byId = idIndex(state.entities)

            var newName = entity.name
            patch["name"]?.let { nameJson ->
                val candidate = nameJson.str
                    ?: throw WorldFormatException.invalid("an entity can't have no name")
                if (candidate != entity.name) {
                    if (state.entities.any { it.name == candidate }) {
                        throw WorldFormatException.invalid(
                            "an entity named '$candidate' already exists")
                    }
                    newName = candidate
                }
            }
            var newParent = entity.parent
            patch["parent"]?.let { parentJson ->
                val candidate = if (parentJson.isNull) null else parentJson.int
                if (candidate != null && !byId.containsKey(candidate)) {
                    throw WorldFormatException.invalid("parent $candidate isn't in the document")
                }
                // A parent cycle would make the entity its own ancestor.
                var seen = mutableSetOf(entity.id)
                var ancestor = candidate
                while (ancestor != null) {
                    if (ancestor in seen) {
                        throw WorldFormatException.invalid(
                            "entity ${entity.id} can't be its own ancestor")
                    }
                    seen.add(ancestor)
                    ancestor = byId[ancestor]?.parent
                }
                newParent = candidate
            }
            // Absent = unchanged; null = clear; value = set — components
            // and `ext-*` fields alike, so a physics component survives
            // a modify round-trip.
            val componentFields = listOf(
                "transform", "shape", "material", "light", "audio", "behaviors",
                "mesh_asset", "modulations", "instance_of", "triggers",
            )
            val newFields = entity.fields.toMutableMap()
            for (field in componentFields) {
                val v = patch[field] ?: continue
                if (v.isNull) newFields.remove(field) else newFields[field] = v
            }
            for ((field, v) in patch) {
                if (!field.startsWith("ext-")) continue
                if (v.isNull) newFields.remove(field) else newFields[field] = v
            }
            val updated = entity.copy(
                name = newName,
                parent = newParent,
                fields = kotlinx.serialization.json.JsonObject(newFields),
            )
            val entities = state.entities.toMutableList()
            entities[index] = updated
            state.copy(entities = entities)
        }

        "SetEnvironment" -> state.copy(
            environment = value.obj?.get("env")?.takeIf { !it.isNull }?.let { EnvironmentDef(it) })

        "SetCamera" -> state.copy(
            camera = value.obj?.get("camera")?.takeIf { !it.isNull }?.let { CameraDef(it) })

        "SetAmbience" -> state.copy(
            ambience = value.obj?.get("ambience")?.arr?.toList() ?: emptyList())

        "SpawnAudioEmitter" -> {
            val name = value.obj?.get("name")?.str
                ?: throw WorldFormatException.invalid("SpawnAudioEmitter needs a name")
            val audio = value.obj?.get("audio") ?: kotlinx.serialization.json.JsonNull
            state.copy(audioEmitters = state.audioEmitters + (name to audio))
        }

        "RemoveAudioEmitter" -> {
            val name = value.obj?.get("name")?.str
                ?: throw WorldFormatException.invalid("RemoveAudioEmitter needs a name")
            if (!state.audioEmitters.containsKey(name)) {
                throw WorldFormatException.invalid("no audio emitter named '$name'")
            }
            state.copy(audioEmitters = state.audioEmitters - name)
        }

        "Batch" -> {
            val ops = value.obj?.get("ops")?.arr ?: return state
            var trial = state.copyForTrial()  // all-or-nothing, per batch
            for (op in ops) {
                val classified = classifyOp(op)
                if (classified is ClassifiedOp.Edit) {
                    trial = applyEdit(trial, classified.name, classified.value)
                }
                // history inside a batch folds to nothing too
            }
            trial
        }

        else -> throw WorldFormatException.invalid("unknown edit $edit")
    }
