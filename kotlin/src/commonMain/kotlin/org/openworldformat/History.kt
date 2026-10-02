// Branching histories. An entry's identity is its own `id` if
// present, else a synthesized `line-<n>`; its parent likewise, else
// the previous entry. A log with no ids is a chain in file order; a
// branch is just a different tip. Spec: spec/session.md.

package org.openworldformat

/** An entry with identity resolved: an id and a parent, always. */
data class IdentifiedEntry(
    val id: String,
    val parent: String?,
    val revision: Int,
    val timestampMs: Double?,
    val entry: LogEntry,
)

/** The history of a log: entries with identity, and its shape. */
data class History(
    /** Entries in file order, identity resolved. */
    val ordered: List<IdentifiedEntry>,
    /** parent id → its children's ids, in file order. */
    val children: Map<String, List<String>>,
    /** The entries no other entry claims as parent — the branch ends. */
    val tips: List<String>,
) {
    fun entry(id: String): IdentifiedEntry? = ordered.firstOrNull { it.id == id }
}

/** Resolve identity for every entry, validating as it goes. */
private fun withIdentity(entries: List<LogEntry>): Pair<List<IdentifiedEntry>, Map<String, IdentifiedEntry>> {
    val byId = LinkedHashMap<String, IdentifiedEntry>()
    val ordered = mutableListOf<IdentifiedEntry>()
    var previous: String? = null
    for ((n, raw) in entries.withIndex()) {
        val id = raw.id ?: "line-$n"
        if (byId.containsKey(id)) {
            throw WorldFormatException.invalid("duplicate entry id '$id'")
        }
        val parent = raw.parent ?: previous
        if (parent != null && !byId.containsKey(parent)) {
            throw WorldFormatException.invalid(
                "entry '$id' names parent '$parent', which isn't in the log")
        }
        val entry = IdentifiedEntry(id, parent, raw.revision, raw.timestampMs, raw)
        byId[id] = entry
        ordered.add(entry)
        previous = id
    }
    return ordered to byId
}

/**
 * Build a log's history: entries with identity, children, and tips.
 *
 * @throws [WorldFormatException] on a duplicate id or a parent that
 *   isn't in the log.
 */
fun buildHistory(entries: List<LogEntry>): History {
    val (ordered, _) = withIdentity(entries)
    val children = ordered.associate { it.id to mutableListOf<String>() }
    for (entry in ordered) {
        entry.parent?.let { parent ->
            (children[parent] as? MutableList<String>)?.add(entry.id)
        }
    }
    val tips = ordered.filter { (children[it.id] ?: emptyList()).isEmpty() }.map { it.id }
    return History(ordered, children, tips)
}

/**
 * Fold one path of the history: the document at [tip] (default: the
 * last entry in file order), reached by walking parent links to the
 * base and folding that chain. A branch is just a different tip.
 *
 * @throws [WorldFormatException] on an unknown tip, or the first
 *   entry that no longer applies.
 */
fun foldPath(manifest: WorldManifest, entries: List<LogEntry>, tip: String? = null): FoldState {
    val (ordered, byId) = withIdentity(entries)
    val target = tip ?: ordered.lastOrNull()?.id
    if (target == null || !byId.containsKey(target)) {
        throw WorldFormatException.invalid("no entry '$target' in this log")
    }
    val chain = mutableListOf<LogEntry>()
    val ids = mutableListOf<String>()
    var cursor: String? = target
    while (cursor != null) {
        val entry = byId.getValue(cursor)
        chain.add(entry.entry)
        ids.add(entry.id)
        cursor = entry.parent
    }
    chain.reverse()
    ids.reverse()
    val state = foldLog(manifest, chain)
    return state.copy(path = ids)
}
