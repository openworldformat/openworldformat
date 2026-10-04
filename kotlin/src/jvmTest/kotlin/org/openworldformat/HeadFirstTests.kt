// The live-authoring rules this reference reads (spec/session.md "The fold
// is total", spec/package.md "Head-first") — mirrors
// js/test/authoring.test.mjs and the Rust conformance suite: every world
// survives an empty fold, every example's manifest is the fold to main,
// and ModifyWorld reaches every scene field and undoes.

package org.openworldformat

import java.io.File
import java.security.MessageDigest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull

/** JSON has one number type: 2 and 2.0 are the same value. */
private fun numeric(e: JsonElement): JsonElement = when (e) {
    is JsonObject -> JsonObject(e.mapValues { numeric(it.value) })
    is JsonArray -> JsonArray(e.map(::numeric))
    is JsonPrimitive -> if (!e.isString && e.doubleOrNull != null) JsonPrimitive(e.doubleOrNull!!) else e
    else -> e
}

/** A world compared as a world: entities by id, empty and null fields
 *  as absent, next_entity_id by its effective value, numbers as numbers. */
private fun normalized(m: WorldManifest): JsonElement {
    val o = m.toJson().filterValues { it != JsonNull && !(it is JsonArray && it.isEmpty()) }.toMutableMap()
    val past = (m.entities.maxOfOrNull { it.id } ?: 0) + 1
    val declared = m.fields["next_entity_id"]?.int ?: 1
    o["next_entity_id"] = JsonPrimitive(maxOf(declared, past))
    o["entities"] = JsonArray(m.entities.sortedBy { it.id }.map { it.toJson() })
    return numeric(JsonObject(o))
}

private fun examples(): List<File> =
    File(repoRoot(), "examples").listFiles()!!
        .filter { File(it, "manifest.json").exists() }
        .sortedBy { it.name }

class HeadFirstTests {
    @Test
    fun theFoldIsTotalEveryWorldSurvivesAnEmptyFold() {
        val worlds = File(repoRoot(), "conformance").listFiles()!!
            .filter { it.name.endsWith(".json") }
            .map { it.name to it }
            .toMutableList()
        for (example in examples()) {
            worlds += "${example.name}/base" to File(example, BASE_SNAPSHOT)
            worlds += "${example.name}/head" to File(example, "manifest.json")
        }
        for ((name, file) in worlds) {
            val manifest = parseManifest(file.readText())
            val state = foldLog(manifest, emptyList())
            // Names bind at ingestion: compare against the bound entities.
            val bound = manifest.copy(entities = state.entities)
            assertEquals(normalized(bound), normalized(toManifest(state)), name)
        }
    }

    @Test
    fun everyExampleIsHeadFirstItsManifestIsTheFoldToMain() {
        assertEquals(2, SUPPORTED_FORMAT_VERSION)
        for (example in examples()) {
            val pkg = loadWorldPackage(example)
            assertEquals(2, pkg.packageJson?.obj?.get("format_version")?.int, example.name)
            assertEquals(normalized(pkg.manifest), normalized(toManifest(pkg.folded())), example.name)
            val sha = MessageDigest.getInstance("SHA-256")
                .digest(File(example, "manifest.json").readBytes())
                .joinToString("") { "%02x".format(it) }
            assertEquals(sha, pkg.packageJson?.obj?.get("world_sha256")?.str, "${example.name}: world_sha256")
        }
    }

    @Test
    fun modifyWorldReachesEverySceneFieldAndUndoes() {
        val manifest = parseManifest(readResource("examples/hello-world/manifest.json"))
        val state = foldLog(manifest, emptyList())
        val op = json(
            """{"ModifyWorld": {"patch": {
                 "meta": {"name": "hello-again", "description": "renamed"},
                 "environment": null,
                 "tours": [{"name": "walk", "waypoints": []}],
                 "soundtrack": null}}}"""
        )
        val inverse = computeInverse(op, state)
        val changed = foldLog(manifest, listOf(entryOf("[$op]")))
        val m = toManifest(changed)
        assertEquals("hello-again", m.name)
        assertNull(m.environment, "null clears")
        assertEquals(1, m.fields["tours"]?.arr?.size)
        val back = foldLog(m, listOf(entryOf("[$inverse]")))
        assertEquals(normalized(toManifest(state)), normalized(toManifest(back)))
        assertFailsWith<WorldFormatException> {
            foldLog(m, listOf(entryOf("""[{"ModifyWorld": {"patch": {"meta": null}}}]""")))
        }
    }

    @Test
    fun anEntrysMessageIsPartOfItsIdentityInEveryReference() {
        val entry = parseLogLine("""{"id":"x","parent":"e6","revision":7,"author":{"name":"claude"},"timestamp_ms":1790000000123,"message":"a lantern by the gate","ops":[{"DeleteEntity":{"id":21}}]}""")
        assertEquals("a lantern by the gate", entry.message)
        assertEquals("sha256:a7cd0955d35a2ff15b16ad7cc3440fb064d675ceceeca865a7ace463a5d177f2", computeEntryId(entry))
    }
}
