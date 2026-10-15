// The Authoring profile's ingestion (spec/session.md "Authoring") —
// mirrors swift/Tests/OpenWorldFormatTests/AuthoringTests.swift and
// js/test/authoring.test.mjs: a batch is taken whole or refused whole,
// names bind to ids, spawns left without ids get consecutive ones,
// partial struct patches merge as RFC 7396, and the head's canonical
// text is the same bytes the other references write.

package org.openworldformat

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonPrimitive

/** The yard the other references' authoring tests stand in. */
private fun yard(): FoldState = foldLog(
    parseManifest(
        """{"version": 3, "meta": {"name": "yard"}, "entities": [
              {"id": 1, "name": "ground", "shape": {"Plane": {"x": 20, "z": 20}}},
              {"id": 2, "name": "crate",
               "transform": {"position": [0, 0.5, 0], "scale": [2, 2, 2]},
               "material": {"color": [0.6, 0.4, 0.2, 1], "roughness": 0.8}}],
            "next_entity_id": 3}""",
    ),
    emptyList(),
)

/** A path into a JSON tree, the way the other references' tests read
 *  one: `op.at("ModifyEntity", "patch", "transform", "scale")`. */
private fun JsonElement.at(vararg path: String): JsonElement? {
    var current: JsonElement? = this
    for (key in path) current = current?.obj?.get(key)
    return current
}

class AuthoringTests {
    @Test
    fun ingestBindsNamesAllocatesIdsAndMergesPartialPatches() {
        val batch = json(
            """[
              {"SpawnEntity": {"entity": {"name": "lamp", "parent": "crate"}}},
              {"ModifyEntity": {"id": "crate",
                "patch": {"transform": {"position": [3, 0.5, 0]},
                          "material": {"base_color_texture": "brick.png"}}}},
              {"ModifyWorld": {"patch": {"meta": {"description": "a yard"}}}}
            ]""",
        )
        val done = assertIs<IngestResult.Ingested>(ingest(yard(), batch), "the batch should have been taken")
        assertEquals(3, done.spawned["lamp"])
        val lamp = done.state.entities.first { it.name == "lamp" }
        assertEquals(2, lamp.parent)
        val crate = done.state.entities.first { it.id == 2 }
        assertEquals(json("[2, 2, 2]"), crate.fields.at("transform", "scale"), "scale kept")
        assertEquals(0.8, crate.fields.at("material", "roughness")?.dbl, "roughness kept")
        assertEquals("brick.png", crate.fields.at("material", "base_color_texture")?.str)
        assertEquals("a yard", toManifest(done.state).meta?.description)
        assertEquals("yard", done.state.name, "meta merges, the name stays")
        // What commits is the whole value, so the fold needs no merging.
        assertEquals(json("[2, 2, 2]"), done.ops[1].at("ModifyEntity", "patch", "transform", "scale"))
    }

    @Test
    fun oneBadOpRefusesTheBatchWithAReasonPerOp() {
        val state = yard()
        val before = toManifest(state)
        val batch = json(
            """[
              {"ModifyEntity": {"id": "crate",
                "patch": {"material": {"colour": [1, 0, 0, 1]}}}},
              {"DeleteEntity": {"id": "nobody"}},
              {"MoveEntity": {"id": 2}},
              {"SpawnEntity": {"entity": {"id": 1, "name": "again"}}},
              {"ModifyEntity": {"id": "ground",
                "patch": {"transform": {"position": [0, 1, 0]}}}}
            ]""",
        )
        val refused = assertIs<IngestResult.Refused>(ingest(state, batch), "the batch should have been refused")
        val errors = refused.errors
        assertEquals(4, errors.size, errors.joinToString("\n"))
        assertTrue(errors[0].startsWith("op 0: /ModifyEntity/patch/material/colour"), errors[0])
        assertTrue("no entity is named \"nobody\"" in errors[1], errors[1])
        assertTrue("isn't an op kind" in errors[2], errors[2])
        assertTrue("already exists" in errors[3], errors[3])
        // The state it was given is untouched.
        assertEquals(before, toManifest(state))
    }

    @Test
    fun notABatchOrAnEmptyOneIsRefused() {
        val state = yard()
        val notABatch = listOf("a batch is [op, …] or {\"ops\": [op, …]}")
        assertEquals(IngestResult.Refused(notABatch), ingest(state, JsonPrimitive(3)), "a number is not a batch")
        assertEquals(
            IngestResult.Refused(listOf("the batch holds no ops")),
            ingest(state, JsonArray(emptyList())),
            "an empty batch is refused",
        )
        assertEquals(IngestResult.Refused(notABatch), ingest(state, json("""{"ops": "no"}""")), "ops must be an array")
    }

    @Test
    fun spawnsLeftWithoutAnIdGetConsecutiveOnes() {
        val batch = json(
            """[{"Batch": {"ops": [
              {"SpawnEntity": {"entity": {"name": "a"}}},
              {"SpawnEntity": {"entity": {"name": "b"}}}]}}]""",
        )
        val done = assertIs<IngestResult.Ingested>(ingest(yard(), batch), "the batch should have been taken")
        assertEquals(mapOf("a" to 3, "b" to 4), done.spawned)
        assertEquals(5, toManifest(done.state).fields["next_entity_id"]?.int)
    }

    @Test
    fun setEnvironmentMergesItsEnvTheSameWay() {
        val manifest = parseManifest(
            """{"version": 3, "meta": {"name": "yard"}, "entities": [],
             "environment": {"background_color": [0.1, 0.1, 0.1], "fog_density": 0.2},
             "next_entity_id": 1}""",
        )
        val batch = json("""[{"SetEnvironment": {"env": {"fog_density": 0.9}}}]""")
        val done = assertIs<IngestResult.Ingested>(
            ingest(foldLog(manifest, emptyList()), batch),
            "the batch should have been taken",
        )
        assertEquals(0.9, done.state.environment?.fogDensity)
        assertEquals(listOf(0.1, 0.1, 0.1), done.state.environment?.backgroundColor, "kept by the merge")
        // The committed op carries the merged whole.
        assertEquals(json("[0.1, 0.1, 0.1]"), done.ops[0].at("SetEnvironment", "env", "background_color"))
    }

    @Test
    fun mergePatchIsRfc7396() {
        val current = json("""{"a": 1, "b": {"c": 2, "d": 3}}""")
        val change = json("""{"b": {"c": null, "e": 4}, "f": 5}""")
        assertEquals(json("""{"a": 1, "b": {"d": 3, "e": 4}, "f": 5}"""), mergePatch(current, change))
    }

    /** Every example's head is its canonical text, byte for byte — the
     *  bytes an authority writes after a batch. */
    @Test
    fun everyExampleHeadIsInCanonicalText() {
        val examples = File(repoRoot(), "examples").listFiles()!!
            .filter { File(it, "manifest.json").isFile }
            .sortedBy { it.name }
        assertTrue(examples.isNotEmpty(), "examples/ holds packages")
        for (example in examples) {
            val text = File(example, "manifest.json").readText()
            assertEquals(text, manifestText(parseManifest(text)), "${example.name}: canonical text")
        }
    }

    /** Members sorted, nulls left out, plain arrays inline, entities by id. */
    @Test
    fun theCanonicalTextIsThePinnedForm() {
        val manifest = parseManifest(
            """{"version": 3, "meta": {"name": "t", "description": null},
             "entities": [{"name": "b", "id": 2},
                          {"id": 1, "name": "a",
                           "transform": {"position": [0.1, 2, -3.5]}}]}""",
        )
        val expected = """
            {
              "entities": [
                {
                  "id": 1,
                  "name": "a",
                  "transform": {
                    "position": [0.1, 2, -3.5]
                  }
                },
                {
                  "id": 2,
                  "name": "b"
                }
              ],
              "meta": {
                "name": "t"
              },
              "version": 3
            }
        """.trimIndent() + "\n"
        assertEquals(expected, manifestText(manifest))
    }
}
