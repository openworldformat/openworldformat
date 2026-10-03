// A `.world` package, parsed from its text parts — the platform half
// (reading a folder, a zip, a content URI) lives with each target.
// Spec: spec/package.md.

package org.openworldformat

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/**
 * A loaded package: everything a viewer or inspector needs first.
 * The log's torn final line is the writer's crash, not the reader's —
 * skip it, exactly as the spec says (spec/session.md).
 *
 * [strict] threads the checked reading through both documents;
 * default (false) is exactly the tolerant fold.
 */
class WorldPackage(
    manifestText: String,
    logText: String? = null,
    stateText: String? = null,
    val packageText: String? = null,
    strict: Boolean = false,
) {
    val manifest: WorldManifest = parseManifest(manifestText, strict)

    /** Parsed log entries, in file order. */
    val entries: List<LogEntry> = run {
        if (logText == null) return@run emptyList()
        val lines = logText.split('\n').filter { it.isNotBlank() }
        lines.mapIndexedNotNull { n, line ->
            try {
                parseLogLine(line, strict)
            } catch (e: WorldFormatException) {
                // The last line may be torn mid-write; anything earlier
                // is real corruption.
                if (n == lines.lastIndex) null else throw e
            }
        }
    }

    val stateDocument: StateDocument? = stateText?.let { StateDocument(OWF_JSON.parseToJsonElement(it)) }

    val packageJson: JsonElement? = packageText?.let { OWF_JSON.parseToJsonElement(it) }

    /** The world at head revision: the fold of the whole log. */
    fun folded(): FoldState = foldLog(manifest, entries)

    /** The history of this package's log (tips, branches). */
    fun history(): History = buildHistory(entries)

    /** The state values at head revision, over the declaration. */
    fun stateValues(): StateFoldResult = foldState(stateDocument, entries)
}

/**
 * A snapshot's path inside the package: `snapshots/entry-<id>.json`
 * for an id-bearing log entry, `snapshots/rev-<N>.json` for a linear
 * log's revision — derived keyframes, never authoritative
 * (spec/session.md "Snapshots"). The entry id is sanitized to the
 * filename-safe alphabet: anything outside `[A-Za-z0-9._-]` becomes
 * `_` (ids are hashes and `line-<n>`s here, but an author may write
 * anything).
 */
fun snapshotFilename(entryId: String?, revision: Int): String {
    if (entryId == null) return "snapshots/rev-$revision.json"
    val sanitized = buildString {
        for (c in entryId) {
            append(
                if (c in 'A'..'Z' || c in 'a'..'z' || c in '0'..'9' || c == '.' || c == '_' || c == '-') c else '_'
            )
        }
    }
    return "snapshots/entry-$sanitized.json"
}

/**
 * The `package.json` after compaction: every field it held, with
 * `base_revision` set to [headRevision] — the new base is the folded
 * document. A missing or malformed package.json gets the minimal
 * object (format version 1). Spec: spec/session.md "Snapshots".
 */
fun compactPackage(packageJson: JsonElement, headRevision: Int): JsonElement {
    val o = packageJson.obj
        ?: return buildJsonObject {
            put("format_version", 1)
            put("base_revision", headRevision)
        }
    return buildJsonObject {
        o.forEach { (k, v) -> put(k, v) }
        put("base_revision", headRevision)
    }
}
