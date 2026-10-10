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
 * rewritten with it: every field schema/entity-refs.json marks — the
 * entity scope on `SpawnEntity.entity` and `ModifyEntity.patch`, the
 * avatar scope on `ModifyWorld.patch.avatar`, the creation scope on
 * each of `patch.creations[]` — plus the op addresses
 * `SpawnEntity.entity.id`, `ModifyEntity.id` and `DeleteEntity.id`
 * (addresses, not schema refs, so explicit), `Batch` recursively.
 * String refs and history ops ride untouched. A spawned name already
 * taken — by main, or by an earlier spawn in the same merge — is
 * renamed `<name>-<n>`, `n` from 2 up, first unused. Merged entries
 * keep their id, parent, author and message: an entry's identity
 * survives the merge.
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
                fields["patch"] = remapMarkedRefs("entity", patch, remap)
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
                patch["avatar"]?.obj?.let { avatar ->
                    rewritten["avatar"] = remapMarkedRefs("avatar", avatar, remap)
                }
                patch["creations"]?.arr?.let { creations ->
                    rewritten["creations"] = JsonArray(creations.map { creation ->
                        creation.obj?.let { remapMarkedRefs("creation", it, remap) } ?: creation
                    })
                }
                fields["patch"] = JsonObject(rewritten)
            }
            return singleKey("ModifyWorld", fields)
        }
        else -> return op
    }
}

/** Every marked ref of [scope] against the remap: numeric values are
 *  ids and rewrite; string values are names and ride — they bind at
 *  ingestion, against the merged fold-so-far. */
private fun remapMarkedRefs(scope: String, value: JsonElement, remap: Map<Int, Int>): JsonElement =
    eachRef(scope, value) { current ->
        plainInt(current)?.let(remap::get)?.let(::JsonPrimitive) ?: current
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

/** A spawned entity against the remap: its id — the op's address, not
 *  a schema ref, so explicit — and every marked entity-scope ref. */
private fun rewriteMergedEntity(entity: JsonElement, remap: Map<Int, Int>): JsonElement {
    val o = entity.obj ?: return entity
    val fields = o.toMutableMap()
    plainInt(o["id"])?.let(remap::get)?.let { fresh -> fields["id"] = JsonPrimitive(fresh) }
    return remapMarkedRefs("entity", JsonObject(fields), remap)
}

/** A number that isn't a string-written name — `.int` alone would read
 *  the string "5" as the number 5. */
internal fun plainInt(element: JsonElement?): Int? =
    (element as? JsonPrimitive)?.takeIf { !it.isString }?.int

/** `{"<kind>": {…}}`. */
private fun singleKey(kind: String, fields: Map<String, JsonElement>): JsonObject =
    JsonObject(mapOf(kind to JsonObject(fields)))
