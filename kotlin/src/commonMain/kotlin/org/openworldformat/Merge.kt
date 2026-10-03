// Merging a branch. The merge authority handles id collisions: ids
// the branch spawned that main allocated concurrently are
// reallocated, and every reference to them inside the merged batch is
// rewritten — numeric behavior refs included, name refs left alone
// (they bind at ingestion, against the merged fold-so-far). Spec:
// spec/session.md "Entry identity, forks and branches", and
// spec/rfcs/branching-histories.md. Name collisions are out of scope
// here: the caller pre-renames.

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
 * collide with the fold's are reallocated from `max(state ids) + 1`
 * (or 1), skipping main's ids, the branch's own spawned ids and
 * earlier remaps — never past [MAX_ENTITY_ID] — and every reference to
 * a remapped id is rewritten with them: `SpawnEntity.entity.id`,
 * `.parent`, `ModifyEntity.id`, `patch.parent`, `DeleteEntity.id` and
 * numeric behavior refs. String refs and history ops ride untouched.
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

    val mainIds = state.entities.mapTo(mutableSetOf()) { it.id }
    val collisions = spawned.filter { it in mainIds }.sorted()

    // Fresh ids, skipping everything already spoken for. (Held as
    // Longs so the walk can run itself out past the ceiling.)
    val taken = (mainIds.asSequence() + spawned.asSequence()).mapTo(mutableSetOf()) { it.toLong() }
    val remappedIds = mutableSetOf<Long>()
    var next = ((mainIds.maxOrNull() ?: 0).toLong()) + 1L
    val remap = LinkedHashMap<Int, Int>()
    for (old in collisions) {
        while (next in taken || next in remappedIds) next += 1L
        // The ceiling the five references share; ids here are Int, so
        // the Int bound bites first in any world this surface reads.
        if (next > MAX_ENTITY_ID || next > Int.MAX_VALUE) {
            throw WorldFormatException.invalid(
                "merge ran out of entity ids below the ceiling $MAX_ENTITY_ID (2^53-1)")
        }
        remap[old] = next.toInt()
        remappedIds.add(next)
    }

    val rewritten = entries.map { entry ->
        LogEntry(
            revision = entry.revision,
            author = entry.author,
            timestampMs = entry.timestampMs,
            ops = entry.ops.map { rewriteMergeOp(it, remap) },
            id = entry.id,
            parent = entry.parent,
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
        else -> return op
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
