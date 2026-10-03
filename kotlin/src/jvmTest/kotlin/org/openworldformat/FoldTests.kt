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

    /** ext-provenance (spec/extensions/provenance.md): the producer
     *  fields live in the extension's namespace, read through meta and
     *  written back only with the keys that are set. */
    @Test
    fun extProvenanceCarriesTheLineageFields() {
        assertEquals(
            listOf("prompt", "model", "generation_duration_ms", "biome", "semantic_category"),
            EXT_PROVENANCE_FIELDS,
        )
        val manifest = parseManifest(
            """{"version": 3, "entities": [], "meta": {"name": "gen",
                "ext-provenance": {"prompt": "a lighthouse at dusk", "model": "big-1",
                "generation_duration_ms": 4200, "biome": "coast", "semantic_category": "landmark"}}}"""
        )
        val provenance = assertNotNull(manifest.meta?.extProvenance)
        assertEquals("a lighthouse at dusk", provenance.prompt)
        assertEquals("big-1", provenance.model)
        assertEquals(4200.0, provenance.generationDurationMs)
        assertEquals("coast", provenance.biome)
        assertEquals("landmark", provenance.semanticCategory)
        assertEquals(4200.0, provenance.generationDurationMs!!)
        assertEquals(
            json("""{"biome": "coast", "generation_duration_ms": 4200.0, "model": "big-1", "prompt": "a lighthouse at dusk", "semantic_category": "landmark"}"""),
            provenance.toJson(),
        )
        // Absent extension, absent provenance; the core example reads null.
        assertNull(parseManifest(readResource("examples/hello-world/manifest.json")).meta?.extProvenance)
    }

    /** Strict mode: the checked reading over the manifest's
     *  vocabulary — unknown keys, legacy producer fields, unregistered
     *  extensions. Non-strict stays exactly as tolerant as ever. */
    @Test
    fun strictManifestChecksTheVocabulary() {
        val registered = """{"version": 3, "entities": [], "ext-physics": {"gravity": [0, -9.8, 0]}}"""
        assertNotNull(parseManifest(registered, strict = true))  // registered ext-*: fine

        assertRefuses("unknown manifest key 'robots'") {
            parseManifest("""{"version": 3, "entities": [], "robots": []}""", strict = true)
        }
        assertRefuses("moved to meta[\"ext-provenance\"]") {
            parseManifest("""{"version": 3, "entities": [], "meta": {"name": "x", "prompt": "hi"}}""", strict = true)
        }
        assertRefuses("unknown entity key 'colour'") {
            parseManifest("""{"version": 3, "entities": [{"id": 1, "name": "a", "colour": [1, 0, 0]}]}""", strict = true)
        }
        assertRefuses("not in the extension registry") {
            parseManifest("""{"version": 3, "entities": [], "ext-magic": {}}""", strict = true)
        }
        assertRefuses("not in the extension registry") {
            parseManifest("""{"version": 3, "entities": [{"id": 1, "name": "a", "ext-magic": {}}]}""", strict = true)
        }
        // The registry itself: the five names spec/extensions/registry.json lists.
        assertEquals(
            setOf("ext-physics", "ext-strict-determinism", "ext-visibility", "ext-cinematography", "ext-provenance"),
            REGISTERED_EXTENSIONS,
        )
        // And non-strict reads every one of those documents just fine.
        assertNotNull(parseManifest("""{"version": 3, "entities": [], "robots": []}"""))
    }

    /** Strict log lines: unknown kinds and unregistered extension ops
     *  are errors; the tolerant default carries them, as ever. */
    @Test
    fun strictLogLinesRefuseUnknownAndUnregisteredOps() {
        assertRefuses("isn't allowed in strict mode") {
            parseLogLine("""{"revision": 1, "ops": [{"spawnentity": {"entity": {"id": 1, "name": "a"}}}]}""", strict = true)
        }
        assertRefuses("not in the extension registry") {
            parseLogLine("""{"revision": 1, "ops": [{"ext-magic": {"spell": 1}}]}""", strict = true)
        }
        assertNotNull(
            parseLogLine("""{"revision": 1, "ops": [{"ext-physics": {"body": "static"}}]}""", strict = true)
        )
        assertNotNull(
            parseLogLine("""{"revision": 1, "ops": [{"tool": "x", "args": {}}]}""", strict = true)
        )
        // Non-strict: the same lines parse (the unknown op classifies
        // as Unknown and folds to nothing).
        assertEquals(1, parseLogLine("""{"revision": 1, "ops": [{"spawnentity": {}}]}""").ops.size)
    }

    /** Strict threads through the package — both documents parse in
     *  the constructor, so the refusal fires on construction. */
    @Test
    fun worldPackageThreadsStrictThroughBothDocuments() {
        assertRefuses("unknown manifest key 'robots'") {
            WorldPackage(
                manifestText = """{"version": 3, "entities": [], "robots": []}""",
                logText = """{"revision": 1, "ops": [{"spawnentity": {}}]}""" + "\n",
                strict = true,
            )
        }
        assertRefuses("isn't allowed in strict mode") {
            // The offending op is on the first line: the torn-last-line
            // tolerance must not swallow a strict refusal.
            WorldPackage(
                manifestText = """{"version": 3, "entities": []}""",
                logText = """{"revision": 1, "ops": [{"spawnentity": {}}]}""" + "\n" +
                    """{"revision": 1, "ops": [{"clock": {}}]}""" + "\n",
                strict = true,
            )
        }
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

    /** The collision rule (spec/session.md "Compatibility"): edits
     *  PascalCase, history kinds lowercase — and a misspelling of
     *  either is neither, so it never quietly folds. */
    @Test
    fun theShapeCollisionRuleHolds() {
        for (kind in EDIT_KEYS) {
            assertTrue(kind.first() in 'A'..'Z', "$kind isn't PascalCase")
            assertTrue(opKindShapeOk(kind), "$kind should satisfy the rule")
        }
        for (kind in HISTORY_KEYS) {
            assertTrue(kind.first() in 'a'..'z', "$kind isn't lowercase")
            assertTrue(opKindShapeOk(kind), "$kind should satisfy the rule")
        }
        // Misspellings: neither edit nor history kind.
        assertFalse(opKindShapeOk("spawnentity"))
        assertFalse(opKindShapeOk("Spawnentity"))
        assertFalse(opKindShapeOk("Tool"))
        assertFalse(opKindShapeOk(""))
        // …and a lowercase edit key classifies as nothing at all.
        assertTrue(classifyOp(json("""{"spawnentity": {"entity": {"id": 1, "name": "a"}}}""")) is ClassifiedOp.Unknown)
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

    /** Immediate name binding (spec/world.md "Identity"): a ref
     *  written by name binds to the numeric id at ingestion, against
     *  the fold-so-far — so a later rename can't re-point it. */
    @Test
    fun nameRefsBindToIdsAtIngestionAndSurviveLaterRenames() {
        val base = parseManifest(manifestText)
        val entries = listOf(
            entryOf("""[{"SpawnEntity": {"entity": {"id": 500, "name": "a"}}}]""", revision = 1, timestampMs = 0.0),
            // The satellite's Orbit center is written by name: "a".
            entryOf(
                """[{"SpawnEntity": {"entity": {"id": 501, "name": "satellite", """ +
                    """"behaviors": [{"Orbit": {"center": "a", "radius": 2.0}}]}}}]""",
                revision = 2, timestampMs = 1.0,
            ),
            // …and "a" is renamed afterwards. The ref stays bound to 500.
            entryOf("""[{"ModifyEntity": {"id": 500, "patch": {"name": "b"}}}]""", revision = 3, timestampMs = 2.0),
        )
        val state = foldLog(base, entries)
        val satellite = assertNotNull(state.entities.firstOrNull { it.id == 501 })
        assertEquals(json("500"), satellite.fields["behaviors"]!!.arr!![0].obj!!["Orbit"]!!.obj!!["center"])
        assertEquals("b", state.entities.firstOrNull { it.id == 500 }?.name)
    }

    @Test
    fun aNameRefThatResolvesToNothingFailsTheFold() {
        val base = parseManifest(manifestText)
        val entries = listOf(
            entryOf(
                """[{"SpawnEntity": {"entity": {"id": 501, "name": "satellite", """ +
                    """"behaviors": [{"LookAt": {"target": "ghost"}}]}}}]""",
                revision = 1,
            ),
        )
        assertRefuses("no entity named 'ghost'") { foldLog(base, entries) }
    }

    /** A saved manifest may still carry name refs an author wrote:
     *  the base resolves against itself when the fold initializes. */
    @Test
    fun baseManifestsResolveNameRefsAtLoad() {
        val manifest = parseManifest(readResource("conformance/behaviors.json"))
        val state = foldLog(manifest, emptyList())
        val orbiter = assertNotNull(state.entities.firstOrNull { it.name == "orbiter_entity" })
        assertEquals(json("3"), orbiter.fields["behaviors"]!!.arr!![0].obj!!["Orbit"]!!.obj!!["center"])
        val watcher = assertNotNull(state.entities.firstOrNull { it.name == "watcher" })
        assertEquals(json("4"), watcher.fields["behaviors"]!!.arr!![0].obj!!["LookAt"]!!.obj!!["target"])
    }

    /** The modulations' target names a *property*, never an entity —
     *  name binding must not touch it. */
    @Test
    fun modulationsTargetingPropertiesRideUntouched() {
        val base = parseManifest(manifestText)
        val entries = listOf(
            entryOf(
                """[{"SpawnEntity": {"entity": {"id": 501, "name": "lamp", "modulations": """ +
                    """[{"target": "emissive", "signal": "energy"}]}}}]""",
                revision = 1,
            ),
        )
        val state = foldLog(base, entries)
        val lamp = assertNotNull(state.entities.firstOrNull { it.id == 501 })
        assertEquals(json("\"emissive\""), lamp.fields["modulations"]!!.arr!![0].obj!!["target"])
    }

    /** State precedence (spec/state.md): a key declared exactly always
     *  wins over the same dotted key read as a map sub-key. */
    @Test
    fun anExactlyDeclaredKeyBeatsTheMapSubkey() {
        val stateDoc = StateDocument(
            fields = mapOf(
                "inventory" to StateField(json("""{"type": "map", "initial": {}}""")),
                "inventory.rope" to StateField(json("""{"type": "int", "initial": 0}""")),
            )
        )
        val folded = foldState(stateDoc, listOf(entryOf("""[{"state": {"inventory.rope": 5}}]""")))
        assertEquals(json("5"), folded.values["inventory.rope"])
        assertEquals(json("{}"), folded.values["inventory"])  // the map stayed empty
        assertEquals(emptyList(), folded.undeclared)
    }

    /** The patch semantics, pinned: absent = unchanged, null = clear,
     *  empty patch = no-op at all. */
    @Test
    fun anEmptyPatchIsANoOpAndNullClears() {
        val base = parseManifest(manifestText)
        val withLight = foldLog(
            base,
            listOf(entryOf("""[{"SpawnEntity": {"entity": {"id": 400, "name": "torch", "light": {"light_type": "point"}}}}]""")),
        )
        val torch = assertNotNull(withLight.entities.firstOrNull { it.id == 400 })
        assertNotNull(torch.light)

        // {} — nothing changes, the entity is bit-for-bit what it was.
        val both = listOf(
            entryOf("""[{"SpawnEntity": {"entity": {"id": 400, "name": "torch", "light": {"light_type": "point"}}}}]""", revision = 1, timestampMs = 0.0),
            entryOf("""[{"ModifyEntity": {"id": 400, "patch": {}}}]""", revision = 2, timestampMs = 1.0),
        )
        val afterNoOp = foldLog(base, both)
        assertEquals(torch, afterNoOp.entities.firstOrNull { it.id == 400 })

        // null — the light is removed, the entity remains.
        val cleared = foldLog(
            base,
            both + entryOf("""[{"ModifyEntity": {"id": 400, "patch": {"light": null}}}]""", revision = 3, timestampMs = 2.0),
        )
        val dimmed = assertNotNull(cleared.entities.firstOrNull { it.id == 400 })
        assertNull(dimmed.light)
        assertNull(dimmed.fields["light"])
    }

    /** The id ceiling: 2^53 − 1, the constant the five references
     *  share; ids here are Int (JS-safe by construction). */
    @Test
    fun theEntityIdCeilingIsTheJsonSafeIntegerMax() {
        assertEquals(9007199254740991L, MAX_ENTITY_ID)
    }
}
