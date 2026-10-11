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
            "entity:ext-cinematography/camera/aim_entity",
            "entity:ext-cinematography/camera/focus_entity",
            "avatar:model_entity",
            "creation:entities/*",
        )) {
            assertTrue(expected in paths, "missing $expected")
        }
    }

    /** An extension's marked refs bind at the fold like a behavior's:
     *  a camera spawned aiming at "maya" by name holds her id once the
     *  entry applies, and a patch's reference binds the same way. */
    @Test
    fun anExtensionsMarkedRefsBindAtTheFoldByName() {
        val manifest = parseManifest(
            """{"version": 3, "meta": {"name": "t"}, "entities": [{"id": 1, "name": "maya"}]}""",
        )
        val state = foldLog(manifest, listOf(
            entryOf(
                """[{"SpawnEntity": {"entity": {"id": 2, "name": "4A", """ +
                    """"ext-cinematography": {"camera": {"aim_entity": "maya"}}}}}, """ +
                    // An `ext-*` patch replaces the block, so the patch carries the whole camera.
                    """{"ModifyEntity": {"id": 2, "patch": """ +
                    """{"ext-cinematography": {"camera": {"aim_entity": "maya", "focus_entity": "maya"}}}}}]""",
            ),
        ))
        val camera = state.entities.first { it.name == "4A" }
            .fields["ext-cinematography"]!!.obj!!["camera"]!!.obj!!
        assertEquals(json("1"), camera["aim_entity"])
        assertEquals(json("1"), camera["focus_entity"])
    }

    /** Merge rewriting walks the extension's marked refs: the camera's
     *  aim and focus follow the actor's fresh id, in the spawn and in a
     *  later patch. */
    @Test
    fun mergeRewritingWalksTheExtensionsMarkedRefs() {
        val manifest = parseManifest("""{"version": 3, "meta": {"name": "t"}, "entities": []}""")
        val main = listOf(entryOf("""[{"SpawnEntity": {"entity": {"id": 1, "name": "maya"}}}]"""))
        val state = foldLog(manifest, main)
        val branch = listOf(
            entryOf(
                """[{"SpawnEntity": {"entity": {"id": 1, "name": "kai"}}}, """ +
                    """{"SpawnEntity": {"entity": {"id": 2, "name": "4A", """ +
                    """"ext-cinematography": {"camera": {"aim_entity": 1, "focus_entity": 1}}}}}, """ +
                    """{"ModifyEntity": {"id": 2, "patch": """ +
                    """{"ext-cinematography": {"camera": {"aim_entity": 1}}}}}]""",
                revision = 2,
                timestampMs = 1.0,
            ),
        )
        val (entries, remapped) = mergeBranch(state, branch)
        assertEquals(mapOf(1 to 3), remapped)
        val spawned = entries[0].ops[1].obj!!["SpawnEntity"]!!.obj!!["entity"]!!.obj!!
        val camera = spawned["ext-cinematography"]!!.obj!!["camera"]!!.obj!!
        assertEquals(json("3"), camera["aim_entity"])
        assertEquals(json("3"), camera["focus_entity"])
        val patch = entries[0].ops[2].obj!!["ModifyEntity"]!!.obj!!["patch"]!!.obj!!
        val patched = patch["ext-cinematography"]!!.obj!!["camera"]!!.obj!!
        assertEquals(json("3"), patched["aim_entity"])
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
