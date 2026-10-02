// Test support: find the repository root (the conformance suite and
// the example packages live there), and the shared helpers.

package org.openworldformat

import java.io.File
import kotlinx.serialization.json.JsonElement

/** Walk up from the working directory until the repo root shows
 *  itself (a directory holding conformance/ and schema/). */
fun repoRoot(): File {
    var dir = File(System.getProperty("user.dir")).absoluteFile
    while (true) {
        if (File(dir, "conformance").isDirectory && File(dir, "schema").isDirectory) return dir
        dir = dir.parentFile ?: throw IllegalStateException("repository root not found above ${File(System.getProperty("user.dir"))}")
    }
}

fun readResource(path: String): String =
    File(repoRoot(), path).readText()

fun readEntries(path: String): List<LogEntry> =
    readResource(path)
        .split('\n')
        .filter { it.isNotBlank() }
        .map(::parseLogLine)

/** Build an entry from ops JSON, the way the other references' tests do. */
fun entryOf(ops: String, revision: Int = 1, timestampMs: Double = 0.0, id: String? = null, parent: String? = null): LogEntry {
    val o = OWF_JSON.parseToJsonElement(
        """{"revision": $revision, "author": {"name": "t"}, "ops": $ops, "timestamp_ms": $timestampMs}"""
    ).obj!!
    return LogEntry(
        revision = revision,
        author = o["author"],
        timestampMs = o["timestamp_ms"]?.dbl,
        ops = o["ops"]!!.arr!!.toList(),
        id = id,
        parent = parent,
    )
}

/** Assert a [WorldFormatException] whose message contains [fragment]. */
inline fun assertRefuses(fragment: String, block: () -> Unit) {
    val error = try {
        block()
        throw AssertionError("expected a WorldFormatException containing \"$fragment\"")
    } catch (e: WorldFormatException) {
        e
    }
    check(error.message?.contains(fragment) == true) {
        "\"${error.message}\" lacks \"$fragment\""
    }
}

/** Equality over JsonElement trees, for readable assertions. */
fun json(text: String): JsonElement = OWF_JSON.parseToJsonElement(text)
