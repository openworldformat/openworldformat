// Authoring: what an authority does with a batch of ops an author sends
// (spec/session.md "Authoring"). The fold reads committed ops; this is
// the step before — turning what an agent, a script or a person wrote
// into ops worth committing, or refusing them with reasons.
//
// Each op, in order, against a trial document that already holds the
// batch's earlier ops:
//
// 1. Bind — a string where an entity id goes is a name, resolved to the
//    id it names now; a spawn without an `id` gets the next one.
// 2. Merge — the object-valued struct fields of a patch (`ModifyEntity`'s
//    `transform`, `material`, `light`; `SetEnvironment`'s `env`;
//    `ModifyWorld`'s `meta`, `environment`, `camera`, `avatar`,
//    `soundtrack`) merge into the current value as a JSON merge patch
//    (RFC 7396). The committed op carries the merged whole.
// 3. Read strictly — no key the format would drop.
// 4. Apply — to the trial.
//
// Any failure refuses the whole batch, with a reason per failing op.
// Files are the host's: this package does no I/O — the authority that
// calls this writes the entry and the head (in `manifestText`'s
// canonical bytes).

package org.openworldformat

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * [ingest]'s reply: a batch taken whole, or refused whole — one reason
 * per failing op (`op 2: …`), never a partial write.
 */
sealed class IngestResult {
    /** A batch the world can take. */
    data class Ingested(
        /** The ops to commit: names bound to ids, struct patches merged. */
        val ops: List<JsonElement>,
        /** Entities the batch spawns, name → id. */
        val spawned: Map<String, Int>,
        /** The world after the batch. */
        val state: FoldState,
        /**
         * What validation warns about in the new world — a behavior whose
         * target the batch deleted — in the words the Rust crate's
         * validation uses. Warnings never refuse (spec/session.md
         * "Authoring"); an authority puts them on the receipt.
         */
        val warnings: List<String> = emptyList(),
    ) : IngestResult()

    /** Nothing was written: one reason per failing op, `op N: <reason>`. */
    data class Refused(val errors: List<String>) : IngestResult()
}

/**
 * Ingest a batch — `[op, …]` or `{"ops": [op, …], …}` (an envelope's
 * other keys — `author`, `message`, `expected_revision` — are the
 * authority's to read, not this step's) — against a fold state. The
 * state passed in is untouched: a refusal changes nothing, and a
 * success hands the new world back in [IngestResult.Ingested].
 */
fun ingest(state: FoldState, batch: JsonElement): IngestResult {
    val notABatch = "a batch is [op, …] or {\"ops\": [op, …]}"
    val ops: List<JsonElement> = when (batch) {
        is JsonArray -> batch
        is JsonObject -> batch["ops"]?.arr ?: return IngestResult.Refused(listOf(notABatch))
        else -> return IngestResult.Refused(listOf(notABatch))
    }
    if (ops.isEmpty()) return IngestResult.Refused(listOf("the batch holds no ops"))
    var trial = state
    val spawned = LinkedHashMap<String, Int>()
    val committed = mutableListOf<JsonElement>()
    val errors = mutableListOf<String>()
    for ((i, raw) in ops.withIndex()) {
        try {
            val op = bindOp(trial, raw, spawned)
            strictOp(op)
            val edit = classifyOp(op) as? ClassifiedOp.Edit
                ?: throw WorldFormatException.invalid("only edit ops can be sent")
            // An op is an entry of one: apply, then bind the names it wrote.
            var step = applyEdit(trial, edit.name, edit.value)
            for (id in touchedEntityIds(listOf(edit))) {
                val index = step.entities.indexOfFirst { it.id == id }
                if (index >= 0) step = resolveNames(step, index)
            }
            // The committed op carries ids in every marked field, like
            // the document it just changed.
            committed.add(bindCommittedRefs(op, step))
            trial = step
        } catch (e: WorldFormatException) {
            errors.add("op $i: ${reasonOf(e)}")
        }
    }
    if (errors.isNotEmpty()) return IngestResult.Refused(errors)
    return IngestResult.Ingested(
        ops = committed,
        spawned = spawned,
        state = trial.copy(appliedEdits = state.appliedEdits + committed.size),
        warnings = receiptWarnings(trial),
    )
}

/**
 * What the batch left that validation warns about and never refuses
 * (spec/session.md "Authoring": warnings come back on the receipt): a
 * behavior whose target the world no longer holds — a `LookAt` target or
 * an `Orbit` center whose entity a delete took — in the words the Rust
 * crate's validation uses, so a receipt reads the same from every
 * reference.
 */
internal fun receiptWarnings(state: FoldState): List<String> {
    val ids = state.entities.mapTo(HashSet()) { it.id }
    val names = state.entities.mapTo(HashSet()) { it.name }
    val warnings = mutableListOf<String>()
    for (entity in state.entities) {
        val behaviors = entity.fields["behaviors"]?.arr ?: continue
        for (behavior in behaviors) {
            val look = behavior.obj["LookAt"]?.obj
            val orbit = behavior.obj["Orbit"]?.obj
            val (kind, target) = when {
                look != null -> "look_at" to look["target"]
                orbit != null -> "orbit" to orbit["center"]
                else -> continue
            }
            val id = target?.int
            val name = target?.str
            val gone = when {
                id != null -> if (id in ids) continue else "entity $id"
                name != null -> if (name in names) continue else "'$name'"
                else -> continue
            }
            warnings += "Entity '${entity.name}' has a $kind behavior targeting $gone, " +
                "which is not in the world (it looks at nothing until retargeted or dropped)"
        }
    }
    return warnings
}

/** The raw reason, without the `invalid: ` prefix the fold's thrown
 *  form carries — the per-op reasons read as the Swift reference's do. */
private fun reasonOf(e: WorldFormatException): String =
    (e.message ?: "refused").removePrefix("invalid: ")

/** The edit kinds as the refusal messages quote them — [EDIT_KEYS] in
 *  its declared order, which `setOf` keeps. */
private val OP_KIND_LIST: String = EDIT_KEYS.joinToString(", ")

// ---------------------------------------------------------------------------
// Bind
// ---------------------------------------------------------------------------

/**
 * Names to ids, ids for spawns that left theirs out, merged struct
 * patches — on the raw JSON, before it is read. [spawned] gains every
 * named spawn as it binds, so the batch's later ops can address it
 * (a `Batch` binds before it applies, so the trial hasn't seen its
 * own spawns yet).
 *
 * @throws [WorldFormatException] for an op of no shape, no kind or an
 *   unknown one, a name nothing answers to, or a spawn whose id isn't
 *   a number.
 */
internal fun bindOp(trial: FoldState, op: JsonElement, spawned: MutableMap<String, Int>): JsonElement {
    val o = op.obj
        ?: throw WorldFormatException.invalid("an op is an object like {\"SpawnEntity\": {…}}")
    if (o.size != 1) {
        throw WorldFormatException.invalid("an op holds exactly one kind, one of: $OP_KIND_LIST")
    }
    val (kind, body) = o.entries.first()
    val fields: MutableMap<String, JsonElement> = (body.obj ?: JsonObject(emptyMap())).toMutableMap()

    /** A name becomes the id it names now; ids pass through. */
    fun resolve(reference: JsonElement): JsonElement {
        val name = reference.str ?: return reference
        val id = spawned[name] ?: trial.entities.firstOrNull { it.name == name }?.id
            ?: throw WorldFormatException.invalid("no entity is named \"$name\"")
        return JsonPrimitive(id)
    }

    /** The top-level bindable entity refs (today: `parent`) bind at op
     *  intake, before the op applies; the deeper ones bind after,
     *  against the world the op made ([bindCommittedRefs],
     *  [resolveNames]) — the same list, walked per pass. */
    fun bindTopLevelRefs(value: JsonObject): JsonObject {
        var out: JsonElement = value
        for (ref in refsOf("entity", EntityRefKind.BINDABLE)) {
            if (ref.path.size == 1) out = walkRefPath(out, ref.path, 0, ::resolve)
        }
        return out as JsonObject
    }

    when (kind) {
        "SpawnEntity" -> {
            val entity = fields["entity"]?.obj?.toMutableMap()
                ?: throw WorldFormatException.invalid("SpawnEntity needs an \"entity\" object")
            val given = entity["id"]
            val id = if (given == null || given.isNull) {
                // The next id: past every id the trial holds, and every id
                // this batch already handed out.
                val declared = trial.scene["next_entity_id"]?.int ?: 1
                val handedOut = spawned.values.maxOfOrNull { it + 1 } ?: 0
                maxOf(declared, handedOut)
            } else {
                plainInt(given)
                    ?: throw WorldFormatException.invalid("a new entity's id is a number, or left out to get one")
            }
            entity["id"] = JsonPrimitive(id)
            val bound = bindTopLevelRefs(JsonObject(entity))
            bound["name"]?.str?.let { name -> spawned[name] = id }
            fields["entity"] = bound
        }

        "ModifyEntity" -> {
            val reference = fields["id"]
                ?: throw WorldFormatException.invalid("ModifyEntity needs an \"id\" (or a name)")
            val id = resolve(reference)
            fields["id"] = id
            val current = trial.entities.firstOrNull { it.id == plainInt(id) }?.fields
            fields["patch"]?.obj?.let { raw ->
                val patch = bindTopLevelRefs(raw).toMutableMap()
                if (current != null) {
                    for (field in listOf("transform", "material", "light")) {
                        val change = patch[field]?.obj ?: continue
                        val now = current[field]?.obj ?: continue
                        patch[field] = mergePatch(now, change)
                    }
                }
                fields["patch"] = JsonObject(patch)
            }
        }

        "DeleteEntity" -> {
            val reference = fields["id"]
                ?: throw WorldFormatException.invalid("DeleteEntity needs an \"id\" (or a name)")
            fields["id"] = resolve(reference)
        }

        "SetEnvironment" -> {
            val change = fields["env"]?.obj
            val now = trial.environment
            if (change != null && now != null) fields["env"] = mergePatch(now.toJson(), change)
        }

        "ModifyWorld" -> {
            fields["patch"]?.obj?.let { raw ->
                val patch = raw.toMutableMap()
                // The avatar's marked refs (today: `model_entity`) bind
                // like any other — a name where an entity id goes resolves
                // at intake (spec/world.md: refs MUST resolve at ingestion).
                raw["avatar"]?.obj?.let { avatar -> patch["avatar"] = eachRef("avatar", avatar, ::resolve) }
                val now = try {
                    toManifest(trial).toJson()
                } catch (e: WorldFormatException) {
                    JsonObject(emptyMap())
                }
                for (field in listOf("meta", "environment", "camera", "avatar", "soundtrack")) {
                    val change = patch[field]?.obj ?: continue
                    val current = now[field]?.obj ?: continue
                    patch[field] = mergePatch(current, change)
                }
                fields["patch"] = JsonObject(patch)
            }
        }

        "Batch" -> {
            val inner = fields["ops"]?.arr
                ?: throw WorldFormatException.invalid("Batch needs an \"ops\" array")
            fields["ops"] = JsonArray(inner.map { bindOp(trial, it, spawned) })
        }

        else -> if (kind !in EDIT_KEYS) {
            throw WorldFormatException.invalid(
                "\"$kind\" isn't an op kind; the format has: $OP_KIND_LIST")
        }
    }
    // A body that wasn't an object passes through untouched — no case
    // could have bound anything into it, and inventing an empty one
    // would paper over the malformed op.
    if (body.obj == null) return op
    return JsonObject(mapOf(kind to JsonObject(fields)))
}

/**
 * Bind the deeper marked refs in a committed op's own JSON — the
 * spawn's entity, the patch, a batch's inner ops — against the state
 * the op just applied to, so the log carries ids in every marked field
 * (spec/session.md: committed ops always carry ids), not only the
 * document. The top-level ones bound at intake.
 *
 * @throws [WorldFormatException] when a ref names no entity.
 */
internal fun bindCommittedRefs(op: JsonElement, state: FoldState): JsonElement {
    val edit = classifyOp(op) as? ClassifiedOp.Edit ?: return op
    val body = edit.value.obj ?: return op
    val field = when (edit.name) {
        "SpawnEntity" -> "entity"
        "ModifyEntity" -> "patch"
        "Batch" -> {
            val inner = body["ops"]?.arr ?: return op
            val bound = JsonArray(inner.map { bindCommittedRefs(it, state) })
            return JsonObject(mapOf(edit.name to JsonObject(body + ("ops" to bound))))
        }
        else -> return op
    }
    val target = body[field] ?: return op
    var bound = target
    for (ref in refsOf("entity", EntityRefKind.BINDABLE)) {
        if (ref.path.size == 1) continue  // bound at intake
        bound = walkRefPath(bound, ref.path, 0) { leaf ->
            val name = leaf.str ?: return@walkRefPath leaf
            val id = state.entities.firstOrNull { it.name == name }?.id
                ?: throw WorldFormatException.invalid("no entity named '$name'")
            JsonPrimitive(id)
        }
    }
    if (bound === target) return op
    return JsonObject(mapOf(edit.name to JsonObject(body + (field to bound))))
}

// ---------------------------------------------------------------------------
// Strict reading
// ---------------------------------------------------------------------------

// The keys a patch's struct fields may carry, from the schema's $defs —
// strict ingestion refuses anything else, with a JSON pointer to it
// (spec/profiles.md "Strict Mode"). Registered `ext-*` keys pass, as
// everywhere.
private val TRANSFORM_KEYS: Set<String> = setOf("position", "rotation_degrees", "scale", "visible")
private val MATERIAL_KEYS: Set<String> = setOf(
    "alpha_mode", "base_color_texture", "color", "double_sided", "emissive",
    "emissive_texture", "metallic", "metallic_roughness_texture",
    "normal_map_texture", "reflectance", "roughness", "unlit",
)
private val LIGHT_KEYS: Set<String> = setOf(
    "color", "direction", "inner_angle", "intensity", "light_type",
    "outer_angle", "range", "shadows",
)
private val ENVIRONMENT_KEYS: Set<String> = setOf(
    "ambient_color", "ambient_intensity", "background_color", "fog_color", "fog_density",
)
private val CAMERA_KEYS: Set<String> = setOf("fov_degrees", "look_at", "position")

/**
 * Refuse keys the format would drop, with a path to each. Explicit
 * nulls are fine (they clear).
 *
 * @throws [WorldFormatException] naming every unknown key, `; `-joined.
 */
internal fun strictOp(op: JsonElement) {
    val unknown = mutableListOf<String>()

    fun check(element: JsonElement?, allowed: Set<String>, path: String) {
        val o = element?.obj ?: return
        for (key in o.keys) {
            if (key in allowed) continue
            if (key.startsWith("ext-") && key in REGISTERED_EXTENSIONS) continue
            unknown.add("$path/$key")
        }
    }
    fun checkEntity(entity: JsonElement?, path: String, isPatch: Boolean) {
        check(entity, if (isPatch) STRICT_ENTITY_KEYS - "id" else STRICT_ENTITY_KEYS, path)
        val o = entity?.obj
        check(o["transform"], TRANSFORM_KEYS, "$path/transform")
        check(o["material"], MATERIAL_KEYS, "$path/material")
        check(o["light"], LIGHT_KEYS, "$path/light")
    }

    val o = op.obj ?: return
    if (o.size != 1) return
    val (kind, body) = o.entries.first()
    val b = body.obj
    when (kind) {
        "SpawnEntity" -> {
            check(body, setOf("entity"), "/$kind")
            checkEntity(b["entity"], "/$kind/entity", isPatch = false)
        }
        "ModifyEntity" -> {
            check(body, setOf("id", "patch"), "/$kind")
            checkEntity(b["patch"], "/$kind/patch", isPatch = true)
        }
        "DeleteEntity" -> check(body, setOf("id"), "/$kind")
        "SetEnvironment" -> check(b["env"], ENVIRONMENT_KEYS, "/$kind/env")
        "SetCamera" -> check(b["camera"], CAMERA_KEYS, "/$kind/camera")
        "ModifyWorld" -> {
            val patch = b["patch"]
            check(patch, WORLD_PATCH_KEYS.toSet(), "/$kind/patch")
            val p = patch?.obj
            check(p["meta"], STRICT_META_KEYS, "/$kind/patch/meta")
            check(p["environment"], ENVIRONMENT_KEYS, "/$kind/patch/environment")
            check(p["camera"], CAMERA_KEYS, "/$kind/patch/camera")
        }
        "Batch" -> b["ops"]?.arr?.forEach(::strictOp)
    }
    if (unknown.isNotEmpty()) {
        throw WorldFormatException.invalid(
            unknown.joinToString("; ") { "$it is not a field of the format" })
    }
}

// ---------------------------------------------------------------------------
// JSON merge patch
// ---------------------------------------------------------------------------

/**
 * JSON merge patch (RFC 7396): objects merge key by key, `null`
 * removes, anything else replaces — the rule ingestion merges a patch's
 * struct fields by.
 */
fun mergePatch(current: JsonElement, change: JsonElement): JsonElement {
    val now = current.obj ?: return change
    val delta = change.obj ?: return change
    val out = now.toMutableMap()
    for ((key, value) in delta) {
        if (value is JsonNull) {
            out.remove(key)
        } else {
            out[key] = out[key]?.let { mergePatch(it, value) } ?: value
        }
    }
    return JsonObject(out)
}
