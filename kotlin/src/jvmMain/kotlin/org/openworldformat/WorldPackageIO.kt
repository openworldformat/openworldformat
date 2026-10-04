// Loading and compacting a `.world` package folder — the JVM target.
// (The same code lives in androidMain; the common fold never touches
// files.)

package org.openworldformat

import java.io.File
import kotlinx.serialization.json.JsonObject

/**
 * Load a head-first package folder: `manifest.json` (required),
 * `snapshots/base.json` (required when the log holds edits),
 * `ops.jsonl`, `state.json` and `package.json` (optional). A zip is the
 * transport form — unpack it first.
 */
fun loadWorldPackage(directory: File): WorldPackage {
    fun text(name: String): String? {
        val file = File(directory, name)
        return if (file.exists()) file.readText() else null
    }
    val manifestText = text("manifest.json")
        ?: throw WorldFormatException("no manifest.json in ${directory.name}")
    return WorldPackage(
        manifestText = manifestText,
        logText = text("ops.jsonl"),
        stateText = text("state.json"),
        packageText = text("package.json"),
        baseText = text(BASE_SNAPSHOT),
    )
}

/**
 * Compact a head-first package folder: the head becomes the base.
 * Copies `manifest.json` (already the head) to `snapshots/base.json`,
 * sets `package.json`'s `base_revision` to [headRevision] (created
 * minimally when the package has none), renames `ops.jsonl` →
 * `ops.archive.jsonl` and starts a fresh empty `ops.jsonl`.
 * `manifest.json` is untouched: compaction changes nothing observable,
 * it truncates structural replay. Spec: spec/session.md "Snapshots".
 *
 * @return the new base, as written.
 */
fun compactWorldPackage(directory: File, headRevision: Int): JsonObject {
    val pkg = loadWorldPackage(directory)
    val headText = File(directory, "manifest.json").readText()
    File(directory, "snapshots").mkdirs()
    File(directory, BASE_SNAPSHOT).writeText(headText)
    File(directory, "package.json").writeText(
        compactPackage(pkg.packageJson ?: kotlinx.serialization.json.JsonNull, headRevision).toString())
    val ops = File(directory, "ops.jsonl")
    if (ops.exists()) {
        check(ops.renameTo(File(directory, "ops.archive.jsonl"))) {
            "couldn't rename ops.jsonl in ${directory.name}"
        }
    }
    File(directory, "ops.jsonl").writeText("")
    return OWF_JSON.parseToJsonElement(headText) as JsonObject
}
