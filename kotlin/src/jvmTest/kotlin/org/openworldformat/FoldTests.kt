// The fold, over the format's own examples — mirrors js/test/fold.test.mjs,
// python/tests/test_fold.py and the Swift suite, assertion for assertion.

package org.openworldformat

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.test.assertNotNull

class ManifestTests {
    @Test
    fun theExampleManifestParsesAtTheSupportedSchemaVersion() {
        val manifest = parseManifest(readResource("examples/hello-world/manifest.json"))
        assertEquals(SUPPORTED_SCHEMA_VERSION, manifest.version)
        assertTrue(manifest.entities.isNotEmpty())
    }

    @Test
    fun aNewerManifestIsRefusedLoudlyPerTheVersioningPolicy() {
        val base = OWF_JSON.parseToJsonElement(
            readResource("examples/hello-world/manifest.json")).obj!!
        val bumped = JsonObject(base + ("version" to json("4")))
        assertRefuses("newer than this reader") { WorldManifest(bumped) }
    }
}

class ClassifyTests {
    @Test
    fun opsAreRecognizedByShapeEditsFirst() {
        val edit = classifyOp(json("""{"SpawnEntity": {"entity": {"id": 1, "name": "a"}}}"""))
        assertTrue(edit is ClassifiedOp.Edit)
        assertEquals("SpawnEntity", edit.name)
        assertTrue(classifyOp(json("""{"tool": "x", "args": {}}""")) is ClassifiedOp.Tool)
        assertTrue(classifyOp(json("""{"input": {"actor": "v"}}""")) is ClassifiedOp.Input)
        assertTrue(classifyOp(json("""{"state": {"score.x": 1}}""")) is ClassifiedOp.State)
        assertTrue(classifyOp(json("""{"clock": {"playing": true, "position_s": 0}}""")) is ClassifiedOp.Clock)
        assertEquals(
            ClassifiedOp.Extension("ext-physics", json("""{"body": "static"}""")),
            classifyOp(json("""{"ext-physics": {"body": "static"}}""")),
        )
        assertEquals(ClassifiedOp.Unknown, classifyOp(json("""{"nope": 1}""")))
    }

    @Test
    fun anOldFormatLineEditsOnlyParsesAsEdits() {
        val entry = parseLogLine(
            """{"revision": 7, "author": {"peer": 3, "name": "maya"}, "ops": [{"DeleteEntity": {"id": 1}}], "timestamp_ms": 1}"""
        )
        val edits = editOps(entry)
        assertEquals(1, edits.size)
        assertEquals("DeleteEntity", edits[0].name)
    }
}

class FoldTests {
    private val manifestText = readResource("examples/hello-world/manifest.json")
    private val entries = readEntries("examples/hello-world/ops.jsonl")

    @Test
    fun theExampleLogFoldsTheLanternAppearsHistoryFoldsToNothing() {
        val base = parseManifest(manifestText)
        val before = base.entities.size
        val state = foldLog(base, entries)
        // Five entries, one edit op among them: everything else is history.
        assertEquals(1, state.appliedEdits)
        assertEquals(before + 1, state.entities.size)
        val lantern = assertNotNull(state.entities.firstOrNull { it.id == 100 })
        assertEquals("lantern", lantern.name)
        assertEquals(Vec3(-12.0, 0.0, 3.0), lantern.transform?.position)
        // History entries carried the revision without bumping anything:
        assertFalse(state.entities.any { it.id == 101 })
    }

    @Test
    fun modifyAppliesAPatchAbsentFieldsAreUnchangedNullClears() {
        val base = parseManifest(manifestText)
        val state = foldLog(
            base,
            listOf(
                entryOf(
                    """[{"ModifyEntity": {"id": 1, "patch": """ +
                        """{"shape": {"Sphere": {"radius": 0.5}}, "material": null}}}]"""
                )
            )
        )
        val ground = assertNotNull(state.entities.firstOrNull { it.id == 1 })
        assertEquals(Shape.Sphere(0.5), ground.shape)
        assertNull(ground.material)
        assertEquals(Vec3(0.0, 0.0, 0.0), ground.transform?.position)  // untouched
    }

    @Test
    fun deletingAnEntityDeletesItsDescendants() {
        val base = parseManifest(manifestText)
        val state = foldLog(
            base,
            listOf(
                entryOf(
                    """[{"SpawnEntity": {"entity": {"id": 200, "name": "p"}}}, """ +
                        """{"SpawnEntity": {"entity": {"id": 201, "name": "c1", "parent": 200}}}, """ +
                        """{"SpawnEntity": {"entity": {"id": 202, "name": "c2", "parent": 201}}}]"""
                ),
                entryOf("""[{"DeleteEntity": {"id": 200}}]""", revision = 2, timestampMs = 1.0),
            )
        )
        val ids = state.entities.map { it.id }.toSet()
        assertFalse(200 in ids)
        assertFalse(201 in ids)
        assertFalse(202 in ids)
    }

    @Test
    fun aBatchAppliesAllOrNothing() {
        val base = parseManifest(manifestText)
        val batch = entryOf(
            """[{"Batch": {"ops": [{"SpawnEntity": {"entity": {"id": 300, "name": "ok"}}}, """ +
                """{"DeleteEntity": {"id": 99999}}]}}]"""
        )
        assertRefuses("no entity 99999") { foldLog(base, listOf(batch)) }
    }

    @Test
    fun stateFoldsOverTheDeclarationToleratingTheUndeclared() {
        val stateDoc = StateDocument(json(readResource("examples/hello-world/state.json")))
        val result = foldState(stateDoc, entries)
        // The example's log sets score.tour to 1; the declaration's initial was 0.
        assertEquals(json("1"), result.values["score.tour"])
        assertEquals(emptyList(), result.undeclared)

        val richer = StateDocument(
            fields = mapOf(
                "score.main" to StateField(json("""{"type": "int", "initial": 0}""")),
                "inventory" to StateField(json("""{"type": "map", "initial": {}}""")),
                "has.map" to StateField(json("""{"type": "bool", "initial": false}""")),
            )
        )
        val ops = listOf(
            """{"state": {"score.main": 5}}""",
            """{"state": {"inventory.rope": 1, "inventory.torch": 2}}""",
            """{"state": {"inventory.rope": null}}""",
            """{"state": {"has.map": true}}""",
            """{"state": {"has.map": null}}""",
            """{"state": {"unknown.key": 7}}""",
            """{"state": {"unknown.key": null}}""",
        )
        val entries2 = ops.mapIndexed { i, op -> entryOf("[$op]", timestampMs = i.toDouble()) }
        val folded = foldState(richer, entries2)
        assertEquals(json("5"), folded.values["score.main"])
        assertEquals(json("""{"torch": 2}"""), folded.values["inventory"])
        assertEquals(json("false"), folded.values["has.map"])  // null reset the initial
        assertNull(folded.values["unknown.key"])               // set, carried, then removed
        assertEquals(listOf("unknown.key"), folded.undeclared)
    }

    @Test
    fun aForkedHistoryFoldsPerTipSamePrefixDifferentWorlds() {
        val manifest = parseManifest(readResource("examples/forked-exploration/manifest.json"))
        val forkedEntries = readEntries("examples/forked-exploration/ops.jsonl")

        val history = buildHistory(forkedEntries)
        // Two tips: the trunk's garden end, and the moat variant.
        assertEquals(listOf("e3", "e5"), history.tips.sorted())
        // The fork point has both children.
        assertEquals(listOf("e3", "e4"), history.children["e2"]?.sorted())

        val trunk = foldPath(manifest, forkedEntries, tip = "e3")
        var names = trunk.entities.map { it.name }.toSet()
        assertTrue("garden" in names)
        assertFalse("moat" in names)
        assertEquals(listOf("e1", "e2", "e3"), trunk.path)

        val variant = foldPath(manifest, forkedEntries, tip = "e5")
        names = variant.entities.map { it.name }.toSet()
        assertTrue("moat" in names)
        assertFalse("garden" in names)
        assertEquals(listOf("e1", "e2", "e4", "e5"), variant.path)

        // Default tip is the last entry in file order; the merge record
        // folds to nothing, so the variant's document is unchanged by it.
        val e4 = foldPath(manifest, forkedEntries, tip = "e4")
        assertEquals(variant.entities.size, e4.entities.size)

        // Unknown tips refuse loudly.
        assertRefuses("no entry 'e99'") { foldPath(manifest, forkedEntries, tip = "e99") }
    }

    @Test
    fun aLogWithNoIdsIsAChainAndMixedLogsWork() {
        val manifest = parseManifest(manifestText)
        // The hello-world log has no ids: one tip, the last line.
        val history = buildHistory(entries)
        assertEquals(listOf("line-${entries.size - 1}"), history.tips)
        val state = foldPath(manifest, entries)
        assertEquals(manifest.entities.size + 1, state.entities.size)  // the lantern

        // Mixed: an id-bearing branch grafted onto a synthesized chain.
        val chain = listOf(
            entryOf("""[{"SpawnEntity": {"entity": {"id": 900, "name": "a"}}}]"""),
            entryOf("""[{"SpawnEntity": {"entity": {"id": 901, "name": "b"}}}]"""),
            entryOf("""[{"SpawnEntity": {"entity": {"id": 902, "name": "c"}}}]""", revision = 2, id = "x", parent = "line-1"),
        )
        val mixed = buildHistory(chain)
        assertEquals(listOf("x"), mixed.tips)
        assertEquals(listOf("x"), mixed.children["line-1"])
        val folded = foldPath(manifest, chain, tip = "x")
        assertEquals(manifest.entities.size + 3, folded.entities.size)
        assertEquals(listOf("line-0", "line-1", "x"), folded.path)
    }

    @Test
    fun identityRefusesDuplicatesAndMissingParents() {
        val dup = listOf(
            entryOf("[]", id = "a"),
            entryOf("[]", id = "a"),
        )
        assertRefuses("duplicate entry id 'a'") { buildHistory(dup) }

        val orphan = listOf(
            entryOf("[]", id = "a"),
            entryOf("[]", id = "b", parent = "nope"),
        )
        assertRefuses("names parent 'nope', which isn't in the log") { buildHistory(orphan) }
    }
}
