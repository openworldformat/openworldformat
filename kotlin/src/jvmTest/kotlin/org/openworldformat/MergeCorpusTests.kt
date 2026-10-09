// The shared merge corpus (conformance/merge/, spec/session.md "The
// merge rules, exactly"): every case runs — remap table, rewritten
// entries and merged head all compared against the committed expected
// results. The other four references run the same cases in their own
// suites, so the five merges cannot drift apart.

package org.openworldformat

import java.io.File
import kotlin.math.abs
import kotlin.math.floor
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

class MergeCorpusTests {
    private val cases = File(repoRoot(), "conformance/merge/cases")
        .listFiles()!!
        .filter { it.name.endsWith(".json") }
        .sortedBy { it.name }
        .map { OWF_JSON.parseToJsonElement(it.readText()).obj!! }

    @Test
    fun theCorpusIsPresentAndCoversTheHandWrittenRules() {
        val names = cases.map { it.getValue("name").str!! }.toSet()
        for (required in listOf("spent-id", "modify-world", "batch", "names")) {
            assertTrue(required in names, "missing hand-written case $required")
        }
        assertTrue(cases.size >= 100, "the corpus holds the generated cases too")
    }

    @Test
    fun everyCaseMergesToTheCommittedResult() {
        for (case in cases) {
            val name = case.getValue("name").str!!
            val base = WorldManifest(case.getValue("base"))
            val main = logEntries(case.getValue("main").arr!!)
            val branch = logEntries(case.getValue("branch").arr!!)
            val expected = case.getValue("expected").obj!!

            val state = foldLog(base, main)
            val result = mergeBranch(state, branch)

            // The remap table, ascending by old id.
            val expectedRemap = expected.getValue("remapped").arr!!.map { pair ->
                pair.arr!![0].int!! to pair.arr!![1].int!!
            }
            assertEquals(expectedRemap, result.remapped.toList(), "$name: the remap table")

            // The rewritten entries.
            val expectedEntries = expected.getValue("entries").arr!!
            assertEquals(expectedEntries.size, result.entries.size, "$name: merged entry count")
            for (index in result.entries.indices) {
                assertEquals(
                    expectedEntries[index], result.entries[index].toLogJson(),
                    "$name: merged entry $index")
            }

            // The merged head, as canonical text.
            val head = foldLog(base, main + result.entries)
            assertEquals(
                expected.getValue("head").str!!, manifestText(toManifest(head)),
                "$name: the merged head")
        }
    }

    /** Case-file entries, read the way [parseLogLine] reads a line. */
    private fun logEntries(array: JsonArray): List<LogEntry> = array.map { element ->
        val o = element.obj!!
        LogEntry(
            revision = o["revision"]!!.int!!,
            author = o["author"],
            timestampMs = o["timestamp_ms"]?.dbl,
            ops = o["ops"]!!.arr!!.toList(),
            id = o["id"]?.str,
            parent = o["parent"]?.takeIf { !it.isNull }?.str,
            message = o["message"]?.str,
        )
    }

    /** A merged entry back to its written shape, for the tree comparison. */
    private fun LogEntry.toLogJson(): JsonObject = buildJsonObject {
        put("revision", JsonPrimitive(revision))
        author?.let { put("author", it) }
        timestampMs?.let { ms ->
            // Integral milliseconds write as integers (Doubles stringify
            // scientifically past 1e7) — [computeEntryId]'s rule.
            put(
                "timestamp_ms",
                if (ms == floor(ms) && abs(ms) < 9.007199254740992E15) {
                    JsonPrimitive(ms.toLong())
                } else {
                    JsonPrimitive(ms)
                })
        }
        put("ops", JsonArray(ops))
        id?.let { put("id", JsonPrimitive(it)) }
        parent?.let { put("parent", JsonPrimitive(it)) }
        message?.let { put("message", JsonPrimitive(it)) }
    }
}
