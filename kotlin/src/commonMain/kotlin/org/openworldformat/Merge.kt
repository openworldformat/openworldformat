// Merging a branch. The merge authority handles id collisions: ids
// the branch spawned that main allocated concurrently are
// reallocated, and every reference to them inside the merged batch is
// rewritten — numeric behavior refs included, name refs left alone
// (they bind at ingestion, against the merged fold-so-far). Name
// collisions get the spec's suffix: a spawned name that is taken — by
// main, or by an earlier spawn in the same merge — becomes
// `<name>-<n>`. Spec: spec/session.md "Entry identity, forks and
// branches" ("The merge rules, exactly"), and
// spec/rfcs/branching-histories.md.

package org.openworldformat

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** What [mergeBranch] produced: the rewritten entries (rebuilt through
 *  the [LogEntry] factory, so classification recomputes) and the id
 *  remap it applied — colliding branch id → fresh id. */
data class MergeBranchResult(
    val entries: List<LogEntry>,
    val remapped: Map<Int, Int>,
)

/**
 * Rewrite a branch's entries for merging into [state]: spawn ids that
 * collide with the fold's are reallocated in ascending order from the
 * fold's effective `next_entity_id` — the floor the fold carries so an
 * id main deleted stays spent — skipping every id the branch spawns,
 * never past [MAX_ENTITY_ID]; and every reference to a remapped id is
 * rewritten with it: `SpawnEntity.entity.id`, `.parent`,
 * `ModifyEntity.id`, `patch.parent`, `DeleteEntity.id`,
 * `ModifyWorld.patch.avatar.model_entity` and
 * `patch.creations[].entities[]`, `Batch` recursively, and the numeric
 * behavior refs (`Orbit.center`, `LookAt.target`). String refs and
 * history ops ride untouched. A spawned name already taken — by main,
 * or by an earlier spawn in the same merge — is renamed `<name>-<n>`,
 * `n` from 2 up, first unused. Merged entries keep their id, parent,
 * author and message: an entry's identity survives the merge.
 *
 * @throws [WorldFormatException] when fresh ids run out below the
 *   ceiling.
 */
fun mergeBranch(state: FoldState, entries: List<LogEntry>): MergeBranchResult {
    // The ids the branch spawns, batches included.
    val spawned = mutableSetOf<Int>()
    fun scan(op: JsonElement) {
        val classified = classifyOp(op)
        if (classified !is ClassifiedOp.Edit) return
        when (classified.name) {
            "SpawnEntity" -> plainInt(classified.value.obj?.get("entity")?.obj?.get("id"))?.let(spawned::add)
            "Batch" -> classified.value.obj?.get("ops")?.arr?.forEach(::scan)
        }
    }
    entries.forEach { entry -> entry.ops.forEach(::scan) }

    // Fresh ids for the collisions, reallocated in ascending order:
    // from the main fold's floor (an id main deleted stays spent),
    // skipping what the branch spawns — and never past the ceiling.
    // The walk runs as a Long so the ceiling check can bite instead of
    // overflowing; the floor itself is the scene's, with one past the
    // largest live id as the guard for the Int edge.
    val live = state.entities.mapTo(mutableSetOf()) { it.id }
    val sceneFloor = plainInt(state.scene["next_entity_id"])?.toLong() ?: 1L
    var next = maxOf(sceneFloor, (live.maxOrNull()?.toLong() ?: 0L) + 1L)
    val spawnedLong = spawned.mapTo(HashSet()) { it.toLong() }
    val remap = LinkedHashMap<Int, Int>()
    for (old in spawned.sorted()) {
        if (old !in live) continue  // no collision, no remap
        while (next in spawnedLong) next += 1L
        // The ceiling the five references share; ids here are Int, so
        // the Int bound bites first in any world this surface reads.
        if (next > MAX_ENTITY_ID || next > Int.MAX_VALUE) {
            throw WorldFormatException.invalid(
                "merge ran out of entity ids below the ceiling $MAX_ENTITY_ID (2^53-1)")
        }
        remap[old] = next.toInt()
        next += 1L
    }

    // The name rule's ledger: main's names, plus every name the merged
    // spawns mint as they mint it.
    val takenNames = state.entities.mapTo(HashSet()) { it.name }
    val rewritten = entries.map { entry ->
        LogEntry(
            revision = entry.revision,
            author = entry.author,
            timestampMs = entry.timestampMs,
            ops = entry.ops.map { renameCollidingSpawns(takenNames, rewriteMergeOp(it, remap)) },
            id = entry.id,
            parent = entry.parent,
            message = entry.message,
        )
    }
    return MergeBranchResult(rewritten, remap)
}

/** Rewrite one op against the remap; history ops pass through. */
private fun rewriteMergeOp(op: JsonElement, remap: Map<Int, Int>): JsonElement {
    if (remap.isEmpty()) return op
    val classified = classifyOp(op)
    if (classified !is ClassifiedOp.Edit) return op
    val obj = classified.value.obj ?: return op
    fun remappedId(id: JsonElement?): Int? = plainInt(id)?.let(remap::get)
    when (classified.name) {
        "SpawnEntity" -> {
            val entity = obj["entity"] ?: return op
            return singleKey("SpawnEntity", mapOf("entity" to rewriteMergedEntity(entity, remap)))
        }
        "ModifyEntity" -> {
            val fields = obj.toMutableMap()
            remappedId(obj["id"])?.let { fresh -> fields["id"] = JsonPrimitive(fresh) }
            obj["patch"]?.obj?.let { patch ->
                val rewritten = patch.toMutableMap()
                remappedId(patch["parent"])?.let { fresh -> rewritten["parent"] = JsonPrimitive(fresh) }
                patch["behaviors"]?.let { b ->
                    val fresh = rewriteBehaviorRefs(b, remap)
                    if (fresh !== b) rewritten["behaviors"] = fresh
                }
                fields["patch"] = JsonObject(rewritten)
            }
            return singleKey("ModifyEntity", fields)
        }
        "DeleteEntity" -> {
            val fields = obj.toMutableMap()
            remappedId(obj["id"])?.let { fresh -> fields["id"] = JsonPrimitive(fresh) }
            return singleKey("DeleteEntity", fields)
        }
        "Batch" -> {
            val ops = obj["ops"]?.arr ?: return op
            return singleKey("Batch", mapOf("ops" to JsonArray(ops.map { rewriteMergeOp(it, remap) })))
        }
        "ModifyWorld" -> {
            val fields = obj.toMutableMap()
            obj["patch"]?.obj?.let { patch ->
                val rewritten = patch.toMutableMap()
                // `patch.avatar.model_entity`, when it is an id — a
                // string there is a name, and names ride.
                patch["avatar"]?.obj?.let { avatar ->
                    remappedId(avatar["model_entity"])?.let { fresh ->
                        rewritten["avatar"] = JsonObject(avatar + ("model_entity" to JsonPrimitive(fresh)))
                    }
                }
                // `patch.creations[].entities[]`: numeric elements are
                // ids and rewrite; string elements are names and ride.
                patch["creations"]?.arr?.let { creations ->
                    rewritten["creations"] = JsonArray(creations.map { creation ->
                        val c = creation.obj ?: return@map creation
                        val entities = c["entities"]?.arr ?: return@map creation
                        JsonObject(c + ("entities" to JsonArray(entities.map { id ->
                            remappedId(id)?.let(::JsonPrimitive) ?: id
                        })))
                    })
                }
                fields["patch"] = JsonObject(rewritten)
            }
            return singleKey("ModifyWorld", fields)
        }
        else -> return op
    }
}

/**
 * The spec's name rule, in place on the way out: a spawned name that
 * is taken — by main, or by an earlier spawn in the same merge —
 * becomes `<name>-<n>`, `n` from 2 up, first unused. Only the
 * `SpawnEntity` changes; references are ids by then, so a rename
 * breaks nothing in the log.
 */
private fun renameCollidingSpawns(taken: MutableSet<String>, op: JsonElement): JsonElement {
    val classified = classifyOp(op)
    if (classified !is ClassifiedOp.Edit) return op
    val obj = classified.value.obj ?: return op
    return when (classified.name) {
        "SpawnEntity" -> {
            val entity = obj["entity"]?.obj ?: return op
            val name = entity["name"]?.str ?: return op
            if (taken.add(name)) return op  // first use keeps its name
            var n = 2
            while ("$name-$n" in taken) n += 1
            val fresh = "$name-$n"
            taken.add(fresh)
            singleKey("SpawnEntity", mapOf("entity" to JsonObject(entity + ("name" to JsonPrimitive(fresh)))))
        }
        "Batch" -> {
            val ops = obj["ops"]?.arr ?: return op
            singleKey("Batch", mapOf("ops" to JsonArray(ops.map { renameCollidingSpawns(taken, it) })))
        }
        else -> op
    }
}

/** A spawned entity against the remap: its id, its parent, and its
 *  numeric behavior refs. */
private fun rewriteMergedEntity(entity: JsonElement, remap: Map<Int, Int>): JsonElement {
    val o = entity.obj ?: return entity
    val fields = o.toMutableMap()
    fun remappedId(id: JsonElement?): Int? = plainInt(id)?.let(remap::get)
    remappedId(o["id"])?.let { fresh -> fields["id"] = JsonPrimitive(fresh) }
    remappedId(o["parent"])?.let { fresh -> fields["parent"] = JsonPrimitive(fresh) }
    o["behaviors"]?.let { b ->
        val fresh = rewriteBehaviorRefs(b, remap)
        if (fresh !== b) fields["behaviors"] = fresh
    }
    return JsonObject(fields)
}

/**
 * Numeric `Orbit.center` / `LookAt.target` refs against the remap;
 * name refs (and every other behavior shape) ride untouched.
 */
internal fun rewriteBehaviorRefs(behaviors: JsonElement, remap: Map<Int, Int>): JsonElement {
    val arr = behaviors.arr ?: return behaviors
    if (remap.isEmpty()) return behaviors
    var changed = false
    val out = arr.map { behavior ->
        val o = behavior.obj
        if (o == null || o.size != 1) return@map behavior
        val (kind, params) = o.entries.first()
        val refKey = when (kind) {
            "Orbit" -> "center"
            "LookAt" -> "target"
            else -> null
        } ?: return@map behavior
        val paramsObj = params.obj ?: return@map behavior
        val fresh = plainInt(paramsObj[refKey])?.let(remap::get) ?: return@map behavior
        changed = true
        singleKey(kind, paramsObj.toMutableMap().apply { put(refKey, JsonPrimitive(fresh)) })
    }
    if (!changed) return behaviors
    return JsonArray(out)
}

/** A number that isn't a string-written name — `.int` alone would read
 *  the string "5" as the number 5. */
internal fun plainInt(element: JsonElement?): Int? =
    (element as? JsonPrimitive)?.takeIf { !it.isString }?.int

/** `{"<kind>": {…}}`. */
private fun singleKey(kind: String, fields: Map<String, JsonElement>): JsonObject =
    JsonObject(mapOf(kind to JsonObject(fields)))
