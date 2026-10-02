// A `.world` package, parsed from its text parts — the platform half
// (reading a folder, a zip, a content URI) lives with each target.
// Spec: spec/package.md.

package org.openworldformat

import kotlinx.serialization.json.JsonElement

/**
 * A loaded package: everything a viewer or inspector needs first.
 * The log's torn final line is the writer's crash, not the reader's —
 * skip it, exactly as the spec says (spec/session.md).
 */
class WorldPackage(
    manifestText: String,
    logText: String? = null,
    stateText: String? = null,
    val packageText: String? = null,
) {
    val manifest: WorldManifest = parseManifest(manifestText)

    /** Parsed log entries, in file order. */
    val entries: List<LogEntry> = run {
        if (logText == null) return@run emptyList()
        val lines = logText.split('\n').filter { it.isNotBlank() }
        lines.mapIndexedNotNull { n, line ->
            try {
                parseLogLine(line)
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
