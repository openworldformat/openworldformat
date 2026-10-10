// The fold: state at any revision is a pure fold of the log over the
// base. Only edits change the document; history kinds fold to nothing;
// a batch applies all-or-nothing; and an entry that no longer applies
// stops the fold, exactly as the specification's readers do.
//
// [FoldState.copyForTrial] is the all-or-nothing rule where one is
// needed: JsonElement trees and data classes are immutable, so a trial
// is one copy and committing is returning it. A `Batch` takes one and so
// does computing an inverse; the fold loop does not, because it owns its
// state and throws without it, and that copy is O(entities) per entry.

package org.openworldformat

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

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
    /** The manifest schema version the world was read at. */
    val version: Int = SUPPORTED_SCHEMA_VERSION,
    /** The world's metadata (its name is [name]). */
    val meta: WorldMeta? = null,
    /**
     * The rest of the manifest — avatar, tours, soundtrack, creations,
     * next_entity_id and anything this reader doesn't type — so the
     * fold's state is always a whole manifest ([toManifest]).
     */
    val scene: JsonObject = JsonObject(emptyMap()),
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
 * Name-written behavior references bind at ingestion (spec/world.md
 * "Identity"): the base resolves against itself after the state
 * initializes, and every entity an entry touches resolves within the
 * entry's trial — an entry is atomic, so an unresolvable name fails
 * the entry and the fold stops there.
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
        version = manifest.version,
        meta = manifest.meta,
        scene = sceneOf(manifest),
    )
    // The base resolves first, against the whole base: a saved world's
    // manifest may still carry name refs an author wrote.
    for (i in state.entities.indices) {
        state = resolveNames(state, i)
    }
    for (entry in entries) {
        val edits = editOps(entry)
        if (edits.isEmpty()) continue  // history folds to nothing
        val touched = touchedEntityIds(edits)
        // No trial copy here. `foldLog` owns `state` and throws without it
        // when an entry no longer applies, so a per-entry copy protected a
        // state nobody could observe — and `copyForTrial` copies the entity
        // list, which is O(entities) per entry. A `Batch` still takes one:
        // there the all-or-nothing rule is the op's own.
        for (edit in edits) {
            state = applyEdit(state, edit.name, edit.value)
        }
        for (id in touched) {
            val index = state.entities.indexOfFirst { it.id == id }
            if (index >= 0) state = resolveNames(state, index)
        }
        state = state.copy(appliedEdits = state.appliedEdits + edits.size)
    }
    return state
}

// ---------------------------------------------------------------------------
// Immediate name binding (spec/world.md "Identity")
// ---------------------------------------------------------------------------

/**
 * Resolve one entity's name-written refs to ids, against the
 * fold-so-far held in [state] — value semantics, an updated copy back.
 * The fields are the schema's marked refs (`entity` scope, `bindable`)
 * except the top-level one (`parent`): the fold's apply validates it,
 * so a string there is already a refusal — it binds at op intake.
 * Saved worlds always contain ids; this is the ingestion half.
 * (`modulations[].target` names a *property*, not an entity — the
 * list doesn't mark it, so it is never touched.)
 *
 * @throws [WorldFormatException] when a ref names no entity.
 */
internal fun resolveNames(state: FoldState, index: Int): FoldState {
    val entity = state.entities.getOrNull(index) ?: return state
    // The name ledger builds on the first name ref found: most
    // entities hold none, and hashing every entity per call is what
    // makes folding a long log quadratic.
    var idByName: HashMap<String, Int>? = null
    fun idFor(name: String): Int {
        var map = idByName
        if (map == null) {
            map = HashMap(state.entities.size)
            for (e in state.entities) map[e.name] = e.id
            idByName = map
        }
        return map[name]
            ?: throw WorldFormatException.invalid("no entity named '$name'")
    }
    var fields = entity.fields
    for (ref in refsOf("entity", EntityRefKind.BINDABLE)) {
        if (ref.path.size == 1) continue  // bound at op intake
        // Paths here start with a named key over the entity's fields,
        // so the walk hands a JsonObject back.
        fields = walkRefPath(fields, ref.path, 0) { current ->
            val name = current.str ?: return@walkRefPath current
            JsonPrimitive(idFor(name))
        } as JsonObject
    }
    if (fields === entity.fields) return state
    val entities = state.entities.toMutableList()
    entities[index] = entity.copy(fields = fields)
    return state.copy(entities = entities)
}

/** The entity ids an entry's edit ops touch (spawn or modify,
 *  recursing batches) — the entities whose refs bind at ingestion. */
private fun touchedEntityIds(edits: List<ClassifiedOp.Edit>): List<Int> {
    val ids = mutableListOf<Int>()
    fun visit(edit: ClassifiedOp.Edit) {
        when (edit.name) {
            "SpawnEntity" -> edit.value.obj?.get("entity")?.obj?.get("id")?.int?.let(ids::add)
            "ModifyEntity" -> edit.value.obj?.get("id")?.int?.let(ids::add)
            "Batch" -> edit.value.obj?.get("ops")?.arr?.forEach { op ->
                (classifyOp(op) as? ClassifiedOp.Edit)?.let(::visit)
            }
        }
    }
    edits.forEach(::visit)
    return ids
}

// ---------------------------------------------------------------------------
// The whole document
// ---------------------------------------------------------------------------

/** The manifest's untyped fields, with `next_entity_id` at its
 *  effective value: ids are never reused, so it is at least one past
 *  the largest. */
internal fun sceneOf(manifest: WorldManifest): JsonObject {
    val past = (manifest.entities.maxOfOrNull { it.id } ?: 0) + 1
    val declared = manifest.fields["next_entity_id"]?.int ?: 1
    return JsonObject(manifest.fields + ("next_entity_id" to JsonPrimitive(maxOf(declared, past))))
}

/**
 * The fold's state as a manifest — the whole document. The fold is
 * total (spec/session.md): `toManifest(foldLog(m, []))` is `m` again, up
 * to name binding, entity order and absent-versus-default fields; a
 * head-first package's `manifest.json` is `toManifest` of its fold to
 * `main`.
 */
fun toManifest(state: FoldState): WorldManifest {
    val past = (state.entities.maxOfOrNull { it.id } ?: 0) + 1
    val floor = state.scene["next_entity_id"]?.int ?: 1
    val json = buildJsonObject {
        state.scene.forEach { (k, v) -> put(k, v) }
        put("version", state.version)
        put("meta", buildJsonObject {
            state.meta?.fields?.forEach { (k, v) -> put(k, v) }
            put("name", state.name)
            state.meta?.description?.let { put("description", it) }
            if (state.meta?.tags?.isNotEmpty() == true) {
                put("tags", JsonArray(state.meta.tags.map(::JsonPrimitive)))
            }
        })
        state.environment?.let { put("environment", it.toJson()) }
        state.camera?.let { put("camera", it.toJson()) }
        if (state.ambience.isNotEmpty()) put("ambience", JsonArray(state.ambience))
        put("entities", JsonArray(state.entities.map { it.toJson() }))
        put("next_entity_id", maxOf(floor, past))
    }
    return WorldManifest(json)
}

// ---------------------------------------------------------------------------
// Applying one edit
// ---------------------------------------------------------------------------

/** Apply one edit op to a fold state, returning the new state.
 *
 *  Nothing here derives a whole index. Hashing every entity per edit is
 *  what made folding a long log quadratic with a large constant, so the
 *  checks are predicates over the list, and the one place that needs
 *  parent links builds an ids-only map, only when a patch reparents. */
internal fun applyEdit(state: FoldState, edit: String, value: JsonElement): FoldState = when (edit) {
        "SpawnEntity" -> {
            val raw = value.obj?.get("entity")
            val entity = try {
                raw?.let { WorldEntity(it) }
            } catch (e: WorldFormatException) {
                null
            } ?: throw WorldFormatException.invalid("SpawnEntity needs an entity with id and name")
            // The id ceiling (2^53 − 1), the same line the other
            // references hold. This common surface types ids as Int —
            // JS-safe, so nothing above the ceiling can get here —
            // the check is symmetry, not reachability.
            if (entity.id.toLong() > MAX_ENTITY_ID) {
                throw WorldFormatException.invalid(
                    "entity ${entity.id} exceeds the id ceiling $MAX_ENTITY_ID (2^53-1)")
            }
            // Predicates, not rebuilt indexes: `associateBy` and `mapTo`
            // hash every entity, and doing that per edit is what made
            // folding a long log quadratic with a large constant — 16,000
            // entries took 4.1 s. A scan answers the same questions
            // without building anything.
            if (state.entities.any { it.id == entity.id }) {
                throw WorldFormatException.invalid("entity ${entity.id} already exists")
            }
            if (state.entities.any { it.name == entity.name }) {
                throw WorldFormatException.invalid("an entity named '${entity.name}' already exists")
            }
            if (entity.parent != null && state.entities.none { it.id == entity.parent }) {
                throw WorldFormatException.invalid(
                    "entity ${entity.id}'s parent ${entity.parent} isn't in the document")
            }
            val floor = state.scene["next_entity_id"]?.int ?: 1
            state.copy(
                entities = state.entities + entity,
                scene = JsonObject(state.scene + ("next_entity_id" to JsonPrimitive(maxOf(floor, entity.id + 1)))),
            )
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
                if (candidate != null && state.entities.none { it.id == candidate }) {
                    throw WorldFormatException.invalid("parent $candidate isn't in the document")
                }
                // A parent cycle would make the entity its own ancestor. The
                // walk needs parent links only, so this maps id to parent id
                // — no entities in it — and is built only when a patch
                // actually reparents.
                val parentOf: Map<Int, Int?> = state.entities.associate { it.id to it.parent }
                val seen = mutableSetOf(entity.id)
                var ancestor = candidate
                while (ancestor != null) {
                    if (ancestor in seen) {
                        throw WorldFormatException.invalid(
                            "entity ${entity.id} can't be its own ancestor")
                    }
                    seen.add(ancestor)
                    ancestor = parentOf[ancestor]
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

        "ModifyWorld" -> {
            // The scene-wide fields, patched like an entity: absent
            // unchanged, null clears, a value sets (spec/session.md).
            val patch = value.obj?.get("patch")?.obj ?: JsonObject(emptyMap())
            var next = state
            patch["meta"]?.let { meta ->
                val name = meta.obj?.get("name")?.str
                if (name == null || name.isBlank()) {
                    throw WorldFormatException.invalid(
                        "ModifyWorld.meta must be a meta object with a name; it can't be cleared")
                }
                next = next.copy(meta = WorldMeta(meta), name = name)
            }
            patch["environment"]?.let { env ->
                next = next.copy(environment = if (env.isNull) null else EnvironmentDef(env))
            }
            patch["camera"]?.let { camera ->
                next = next.copy(camera = if (camera.isNull) null else CameraDef(camera))
            }
            patch["ambience"]?.let { ambience ->
                next = next.copy(ambience = ambience.arr?.toList() ?: emptyList())
            }
            val scene = next.scene.toMutableMap()
            for (field in listOf("avatar", "soundtrack", "tours", "creations")) {
                val v = patch[field] ?: continue
                if (v.isNull || v.arr?.isEmpty() == true) scene.remove(field) else scene[field] = v
            }
            next.copy(scene = JsonObject(scene))
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
