// Loading and compacting a `.world` package folder — the JVM target.
// (The same code lives in androidMain; the common fold never touches
// files.)

package org.openworldformat

import java.io.File
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

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

/**
 * Compact a package folder: discard history by making the folded
 * document the new base. Writes the folded `manifest.json` (entities
 * inline, `next_entity_id` = max id + 1 capped at [MAX_ENTITY_ID] —
 * the Int bound this surface holds ids by bites first — meta,
 * environment and camera as the fold holds them, everything else the
 * base carried riding along), sets `package.json`'s `base_revision`
 * to [headRevision] (created minimally, format version 1, when the
 * package has none), renames `ops.jsonl` → `ops.archive.jsonl` and
 * starts a fresh empty `ops.jsonl`. Compaction changes nothing
 * observable; it truncates structural replay. Spec: spec/session.md
 * "Snapshots".
 *
 * @return the compacted manifest, as written.
 */
fun compactWorldPackage(directory: File, headRevision: Int): JsonObject {
    val pkg = loadWorldPackage(directory)
    val state = pkg.folded()
    val base = pkg.manifest

    val maxId = state.entities.maxOfOrNull { it.id } ?: 0
    val nextEntityId = ((maxId.toLong()) + 1L)
        .coerceAtMost(MAX_ENTITY_ID)
        .coerceAtMost(Int.MAX_VALUE.toLong())
        .toInt()

    val compacted = buildJsonObject {
        put("version", base.version)
        // `WorldManifest.fields` excludes the keys it typed, so meta is
        // rebuilt from the typed copy: passthrough first (ext-provenance
        // and any future extension ride along), then the named fields
        // with the fold's name — compaction must not erase metadata.
        base.meta?.let { meta ->
            put("meta", buildJsonObject {
                meta.fields.forEach { (k, v) -> put(k, v) }
                put("name", state.name.ifEmpty { meta.name ?: "" })
                meta.description?.let { put("description", JsonPrimitive(it)) }
                if (meta.tags.isNotEmpty()) {
                    put("tags", JsonArray(meta.tags.map(::JsonPrimitive)))
                }
            })
        }
        state.environment?.let { put("environment", it.toJson()) }
        state.camera?.let { put("camera", it.toJson()) }
        put("ambience", JsonArray(state.ambience))
        put("entities", JsonArray(state.entities.map { it.toJson() }))
        put("next_entity_id", nextEntityId)
        // Everything else the base carried (soundtrack, tours, avatar,
        // creations, …) rides along; meta and next_entity_id are ours.
        for ((k, v) in base.fields) {
            if (k == "meta" || k == "next_entity_id") continue
            put(k, v)
        }
    }

    File(directory, "manifest.json").writeText(compacted.toString())
    File(directory, "package.json").writeText(
        compactPackage(pkg.packageJson ?: kotlinx.serialization.json.JsonNull, headRevision).toString())
    val ops = File(directory, "ops.jsonl")
    if (ops.exists()) {
        check(ops.renameTo(File(directory, "ops.archive.jsonl"))) {
            "couldn't rename ops.jsonl in ${directory.name}"
        }
    }
    File(directory, "ops.jsonl").writeText("")
    return compacted
}
