// The session log (ops.jsonl): parsing and op classification.
//
// Ops are recognized by shape, edits first — the compatibility rule,
// executable: a log written before the history kinds existed parses
// as edits, and an edit serializes today exactly as it always did.
// Spec: spec/session.md.

package org.openworldformat

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

/** The edit kinds — the only ops that change the document. */
val EDIT_KEYS: Set<String> = setOf(
    "SpawnEntity",
    "DeleteEntity",
    "ModifyEntity",
    "SetEnvironment",
    "SetCamera",
    "SetAmbience",
    "SpawnAudioEmitter",
    "RemoveAudioEmitter",
    "ModifyWorld",
    "Batch",
)

/** The history kinds — they record, and fold to nothing. */
val HISTORY_KEYS: Set<String> = setOf("tool", "input", "state", "clock", "merge")

/**
 * The shape collision rule (spec/session.md "Compatibility"): an op
 * kind is either a PascalCase edit in [EDIT_KEYS] or a lowercase
 * history kind in [HISTORY_KEYS] — so a future edit kind can never
 * collide with a history kind, and a misspelling of either is
 * recognizable as neither.
 */
fun opKindShapeOk(kind: String): Boolean =
    if (kind.isNotEmpty() && kind[0] in 'A'..'Z') kind in EDIT_KEYS else kind in HISTORY_KEYS

/** One op, recognized by shape. */
sealed class ClassifiedOp {
    /** A document edit: one of [EDIT_KEYS], carrying its value. */
    data class Edit(val name: String, val value: JsonElement) : ClassifiedOp()

    /** History kinds — they record, and fold to nothing for the document. */
    data class Tool(val value: JsonElement) : ClassifiedOp()
    data class Input(val value: JsonElement) : ClassifiedOp()
    data class State(val value: JsonObject) : ClassifiedOp()
    data class Clock(val value: JsonObject) : ClassifiedOp()
    data class Merge(val value: JsonObject) : ClassifiedOp()

    /** An extension op (`ext-*`, single key, object value) — its own
     *  kind, and like every history kind it folds to nothing here. */
    data class Extension(val name: String, val value: JsonElement) : ClassifiedOp()

    /** Recognized by no rule — carried, ignored. */
    data object Unknown : ClassifiedOp()
}

/** Classify one op by its shape, edits first. */
fun classifyOp(op: JsonElement): ClassifiedOp {
    val o = op.obj ?: return ClassifiedOp.Unknown
    // An edit: exactly one key, and it's an edit kind.
    if (o.size == 1) {
        val (key, value) = o.entries.first()
        if (key in EDIT_KEYS) return ClassifiedOp.Edit(key, value)
    }
    if (o["tool"]?.str != null && o["args"] != null) return ClassifiedOp.Tool(op)
    val input = o["input"]?.obj
    if (input != null && input["actor"]?.str != null) {
        return ClassifiedOp.Input(input)
    }
    o["state"]?.obj?.let { return ClassifiedOp.State(it) }
    o["clock"]?.obj?.let { return ClassifiedOp.Clock(it) }
    o["merge"]?.obj?.let { return ClassifiedOp.Merge(it) }
    if (o.size == 1) {
        val (key, value) = o.entries.first()
        if (isExtensionKey(key) && value.obj != null) {
            return ClassifiedOp.Extension(key, value)
        }
    }
    return ClassifiedOp.Unknown
}

/**
 * One parsed ops.jsonl line: an entry, its ops classified on parse.
 *
 * Unreadable lines are the writer's crash, not the reader's — the
 * caller decides whether to skip (the spec says skip the last one,
 * count the rest).
 */
data class LogEntry(
    val revision: Int,
    val author: JsonElement?,
    val timestampMs: Double?,
    val ops: List<JsonElement>,
    val id: String?,
    val parent: String?,
    val classified: List<ClassifiedOp>,
    /** What the author says the batch is for — a commit message. Part of
     *  the entry's identity; it folds to nothing. */
    val message: String? = null,
)

fun LogEntry(
    revision: Int,
    author: JsonElement? = null,
    timestampMs: Double? = null,
    ops: List<JsonElement>,
    id: String? = null,
    parent: String? = null,
    message: String? = null,
): LogEntry = LogEntry(
    revision, author, timestampMs, ops, id, parent,
    ops.map(::classifyOp), message,
)

/**
 * Parse one log line into an entry with classified ops.
 *
 * Strict mode ([strict]) refuses what the tolerant fold carries: an
 * op of no recognized kind, and an `ext-*` op whose name the registry
 * (spec/extensions/registry.json) doesn't list. Non-strict behavior
 * is exactly the default.
 *
 * @throws [WorldFormatException] when the line isn't JSON, the entry
 *   has no numeric revision and ops array, or (strict) holds an
 *   unknown or unregistered op kind.
 */
fun parseLogLine(line: String, strict: Boolean = false): LogEntry {
    val o = OWF_JSON.parseToJsonElement(line).obj
        ?: throw WorldFormatException("log entry must be a JSON object")
    val revision = o["revision"]?.int
    val ops = o["ops"]?.arr
    if (revision == null || ops == null) {
        throw WorldFormatException("log entry needs a revision and an ops array")
    }
    if (strict) validateOpsStrict(ops.toList())
    return LogEntry(
        revision = revision,
        author = o["author"],
        timestampMs = o["timestamp_ms"]?.dbl,
        ops = ops.toList(),
        id = o["id"]?.str,
        // A parent of the wrong shape is the same as absent: id-bearing
        // logs carry parent as a string; anything else falls to the chain.
        parent = o["parent"]?.takeIf { it !is kotlinx.serialization.json.JsonNull }?.str,
        message = o["message"]?.str,
    )
}

/** An entry's edits, in order — the ops that change the document. */
fun editOps(entry: LogEntry): List<ClassifiedOp.Edit> {
    val classified = if (entry.classified.isEmpty() && entry.ops.isNotEmpty()) {
        entry.ops.map(::classifyOp)
    } else {
        entry.classified
    }
    return classified.filterIsInstance<ClassifiedOp.Edit>()
}
