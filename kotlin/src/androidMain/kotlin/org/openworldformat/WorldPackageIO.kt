// Loading a `.world` package folder — the Android target, for app
// storage; content-URI sources go through the app's own copy layer.
// (The same ~30 lines live in jvmMain; the common fold never touches
// files.)

package org.openworldformat

import java.io.File

/**
 * Load a package folder: `manifest.json` (required), `ops.jsonl`
 * (optional), `state.json` and `package.json` (optional). A zip is
 * the transport form — unpack it first.
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
    )
}
