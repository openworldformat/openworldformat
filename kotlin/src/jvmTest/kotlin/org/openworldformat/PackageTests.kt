// The package's derived parts: snapshot naming and compaction
// (spec/package.md, spec/session.md "Snapshots") — the naming rules,
// and the file routine that performs them.

package org.openworldformat

import java.io.File
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class PackageTests {
    /** Snapshots name by entry when the log carries identity — the
     *  branch-aware name — and by revision when it doesn't. Opaque ids
     *  sanitize into filenames that stay inside `snapshots/`. */
    @Test
    fun snapshotsNameByEntryWhenThereIsOne() {
        assertEquals("snapshots/entry-e42.json", snapshotFilename("e42", 7))
        assertEquals(
            "snapshots/entry-sha256_4b754af__odd.json",
            snapshotFilename("sha256:4b754af+/odd", 7),
        )
        assertEquals("snapshots/rev-7.json", snapshotFilename(null, 7))
    }

    /** Compaction on paper: `base_revision` moves to the head; every
     *  other field the package.json held stays where it was. */
    @Test
    fun compactionMovesTheBaseToHead() {
        val packageJson = json(
            """{"format_version": 1, "name": "castle", "base_revision": 0,
                "head_revision": 41, "updated_ms": 1790000000123}"""
        )
        val compacted = compactPackage(packageJson, 41).obj!!
        assertEquals(41, compacted["base_revision"]!!.int)
        assertEquals(41, compacted["head_revision"]!!.int)
        assertEquals("castle", compacted["name"]!!.str)
        // A package with no package.json gets the minimal one.
        val minimal = compactPackage(JsonElementNull, 3).obj!!
        assertEquals(3, minimal["base_revision"]!!.int)
        assertEquals(1, minimal["format_version"]!!.int)
    }

    private lateinit var dir: File

    private fun packageFolder(): File {
        dir = kotlin.io.path.createTempDirectory("owf-compact").toFile()
        File(dir, "manifest.json").writeText(
            """{"version": 3, "meta": {"name": "tiny"}, "entities": [{"id": 1, "name": "a"}]}"""
        )
        File(dir, "ops.jsonl").writeText(
            """
            {"revision":1,"ops":[{"SpawnEntity":{"entity":{"id":2,"name":"b","parent":1}}}]}
            {"revision":1,"ops":[{"tool":"t","args":{}}]}
            {"revision":2,"ops":[{"ModifyEntity":{"id":2,"patch":{"name":"b2"}}}]}
            """.trimIndent() + "\n"
        )
        File(dir, "package.json").writeText(
            """{"format_version": 1, "name": "tiny", "base_revision": 0, "head_revision": 2}"""
        )
        return dir
    }

    @AfterTest
    fun cleanup() {
        if (::dir.isInitialized) dir.deleteRecursively()
    }

    /** Compaction on disk: the folded document becomes the base, the
     *  old log archives, a fresh log starts, and the fold of the
     *  compacted package is the document compaction promised not to
     *  change. */
    @Test
    fun compactWorldPackageDiscardsHistoryWithoutChangingState() {
        val dir = packageFolder()
        val before = loadWorldPackage(dir).folded()

        val written = compactWorldPackage(dir, headRevision = 2)

        // The new base holds the folded entities, parents inline.
        assertEquals("tiny", written["meta"]!!.obj!!["name"]!!.str)
        val entities = written["entities"]!!.arr!!.map { WorldEntity(it) }
        assertEquals(listOf(1, 2), entities.map { it.id })
        assertEquals(3, written["next_entity_id"]!!.int)

        // The file moves.
        assertTrue(File(dir, "ops.archive.jsonl").exists(), "the old log archives")
        assertEquals("", File(dir, "ops.jsonl").readText(), "a fresh log starts empty")
        val packageJson = json(File(dir, "package.json").readText()).obj!!
        assertEquals(2, packageJson["base_revision"]!!.int)

        // And nothing observable changed: folding the compacted package
        // reaches the same document.
        val after = loadWorldPackage(dir).folded()
        assertEquals(before.entities.map { it.toJson() }, after.entities.map { it.toJson() })
        assertEquals(before.name, after.name)
        assertNotNull(after.entities.firstOrNull { it.name == "b2" })
    }
}

/** The null JsonElement, spelled out for [PackageTests.compactionMovesTheBaseToHead]. */
private val JsonElementNull = kotlinx.serialization.json.JsonNull
