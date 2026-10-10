// The one list of entity-reference fields (spec/world.md "Identity",
// schema/entity-refs.json): the Kotlin reference embeds a copy
// (commonMain's EntityRefs.kt) and this test fails when the copy
// drifts from the canonical file — and pins that the passes actually
// walk it. The JS suite's intake-binding cases don't mirror: this
// reference's surface is the fold — intake is the log — with no
// ingest entry point to bind names pre-apply.

package org.openworldformat

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

class EntityRefsTests {
    /** The embedded copy deep-equals the canonical file's refs. */
    @Test
    fun theEmbeddedListIsTheCanonicalList() {
        val canonical = json(readResource("schema/entity-refs.json")).obj!!["refs"]!!.arr!!
        val embedded = JsonArray(ENTITY_REFS.map { ref ->
            JsonObject(mapOf(
                "scope" to JsonPrimitive(ref.scope),
                "path" to JsonArray(ref.path.map { JsonPrimitive(it) }),
                "kind" to JsonPrimitive(ref.kind.marker),
            ))
        })
        assertEquals(canonical, embedded)
    }

    /** The list holds every field the merge table rewrites. */
    @Test
    fun theCanonicalListHoldsEveryFieldTheMergeTableRewrites() {
        val paths = ENTITY_REFS.map { "${it.scope}:${it.path.joinToString("/")}" }
        for (expected in listOf(
            "entity:parent",
            "entity:behaviors/*/Orbit/center",
            "entity:behaviors/*/LookAt/target",
            "avatar:model_entity",
            "creation:entities/*",
        )) {
            assertTrue(expected in paths, "missing $expected")
        }
    }

    /** Merge rewriting walks the list for every scope: the spawn's
     *  parent and both behavior refs move with the remapped id, and so
     *  do the avatar's model_entity and each creation's entities. */
    @Test
    fun mergeRewritingWalksTheListForEveryScope() {
        val manifest = parseManifest("""{"version": 3, "meta": {"name": "t"}, "entities": []}""")
        val main = listOf(entryOf("""[{"SpawnEntity": {"entity": {"id": 1, "name": "main-one"}}}]"""))
        val state = foldLog(manifest, main)
        val branch = listOf(
            entryOf(
                """[{"SpawnEntity": {"entity": {"id": 1, "name": "branch-one", "parent": 1, """ +
                    """"behaviors": [{"Orbit": {"center": 1, "radius": 2.0, "speed": 10.0}}, """ +
                    """{"LookAt": {"target": 1}}]}}}, """ +
                    """{"ModifyWorld": {"patch": {"avatar": {"model_entity": 1}, """ +
                    """"creations": [{"id": 1, "name": "c", "entities": [1]}]}}}]""",
                revision = 2,
                timestampMs = 1.0,
            ),
        )
        val (entries, remapped) = mergeBranch(state, branch)
        assertEquals(mapOf(1 to 2), remapped)
        val spawn = entries[0].ops[0].obj!!["SpawnEntity"]!!.obj!!["entity"]!!.obj!!
        assertEquals(json("2"), spawn["parent"])
        assertEquals(json("2"), spawn["behaviors"]!!.arr!![0].obj!!["Orbit"]!!.obj!!["center"])
        assertEquals(json("2"), spawn["behaviors"]!!.arr!![1].obj!!["LookAt"]!!.obj!!["target"])
        val patch = entries[0].ops[1].obj!!["ModifyWorld"]!!.obj!!["patch"]!!.obj!!
        assertEquals(json("2"), patch["avatar"]!!.obj!!["model_entity"])
        assertEquals(json("[2]"), patch["creations"]!!.arr!![0].obj!!["entities"])
    }
}
