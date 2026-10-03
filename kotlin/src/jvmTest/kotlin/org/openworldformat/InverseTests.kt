// computeInverse — undo by appending (spec/session.md "The op
// kinds"): every case, and the round-trips that pin them.

package org.openworldformat

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class InverseTests {
    private val base = parseManifest(readResource("examples/hello-world/manifest.json"))

    @Test
    fun aSpawnInversesToDelete() {
        val spawn = json("""{"SpawnEntity": {"entity": {"id": 100, "name": "lantern"}}}""")
        val state = foldLog(base, listOf(entryOf("""[${spawn}]""")))
        assertEquals(
            json("""{"DeleteEntity": {"id": 100}}"""),
            computeInverse(spawn, state),
        )
    }

    /** The golden round-trip: spawn a parent and child, delete the
     *  parent — the inverse is a Batch of spawns restoring the tree,
     *  parents first, verbatim. */
    @Test
    fun aDeleteInversesToABatchRestoringTheSubtreeParentsFirst() {
        val spawns = listOf(
            entryOf(
                """[{"SpawnEntity": {"entity": {"id": 700, "name": "p", "transform": {"position": [1, 2, 3]}}}},""" +
                    """{"SpawnEntity": {"entity": {"id": 701, "name": "c1", "parent": 700}}},""" +
                    """{"SpawnEntity": {"entity": {"id": 702, "name": "c2", "parent": 701}}}]""",
                revision = 1, timestampMs = 0.0,
            ),
        )
        val state = foldLog(base, spawns)
        val delete = json("""{"DeleteEntity": {"id": 700}}""")
        val inverse = computeInverse(delete, state)

        val batchOps = inverse.obj!!["Batch"]!!.obj!!["ops"]!!.arr!!
        assertEquals(3, batchOps.size)
        val restored = batchOps.map { op ->
            val entity = op.obj!!["SpawnEntity"]!!.obj!!["entity"]!!
            WorldEntity(entity)  // verbatim copies: reparsable as-is
        }
        // Parents first: the tree's order, not the deletion's set order.
        assertEquals(listOf(700, 701, 702), restored.map { it.id })
        assertEquals(700, restored[1].parent)
        assertEquals(Vec3(1.0, 2.0, 3.0), restored[0].transform?.position)

        // The round-trip: delete, then append the inverse — the tree
        // is back, exactly as it was.
        val gone = foldLog(base, spawns + entryOf("""[{"DeleteEntity": {"id": 700}}]""", revision = 2, timestampMs = 1.0))
        assertTrue(gone.entities.none { it.id in setOf(700, 701, 702) })
        val back = foldLog(base, spawns + entryOf("""[{"DeleteEntity": {"id": 700}}]""", revision = 2, timestampMs = 1.0) + entryOf("[$inverse]", revision = 3, timestampMs = 2.0))
        assertEquals(
            state.entities.filter { it.id >= 700 },
            back.entities.filter { it.id >= 700 },
        )
    }

    @Test
    fun aModifyInversesToTheOldValuesAbsentBecomesNull() {
        val spawn = """[{"SpawnEntity": {"entity": {"id": 800, "name": "old", "light": {"light_type": "point"}, "ext-physics": {"mass": 1}}}}]"""
        val state = foldLog(base, listOf(entryOf(spawn)))
        val modify = json("""{"ModifyEntity": {"id": 800, "patch": {"name": "new", "parent": 1, "light": null, "shape": {"Sphere": {"radius": 1}}}}}""")
        val inverse = computeInverse(modify, state)
        assertEquals(
            json("""{"ModifyEntity": {"id": 800, "patch": {"name": "old", "parent": null, "light": {"light_type": "point"}, "shape": null}}}"""),
            inverse,
        )
        // Round-trip: apply then inverse — every old value restored.
        val edited = foldLog(base, listOf(entryOf(spawn), entryOf("[$modify]", revision = 2, timestampMs = 1.0)))
        val restored = foldLog(base, listOf(entryOf(spawn), entryOf("[$modify]", revision = 2, timestampMs = 1.0), entryOf("[$inverse]", revision = 3, timestampMs = 2.0)))
        assertEquals(
            state.entities.firstOrNull { it.id == 800 },
            restored.entities.firstOrNull { it.id == 800 },
        )
        // And the state between them really did change.
        assertEquals("new", edited.entities.firstOrNull { it.id == 800 }?.name)
        assertNull(edited.entities.firstOrNull { it.id == 800 }?.light)
    }

    @Test
    fun settingsInverseToTheirPreviousValueOrTheEmptyForm() {
        // Environment: the fold holds one (the example's) — the inverse
        // restores it; cleared, it inverses to the empty object.
        val state = foldLog(base, emptyList())
        val env = computeInverse(json("""{"SetEnvironment": {"env": {"fog_density": 0.2}}}"""), state)
        assertEquals(
            state.environment?.toJson(),
            env.obj!!["SetEnvironment"]!!.obj!!["env"],
        )
        val cleared = foldLog(base, listOf(entryOf("""[{"SetEnvironment": {"env": null}}]""")))
        assertEquals(
            json("{}"),
            computeInverse(json("""{"SetEnvironment": {"env": {}}}"""), cleared).obj!!["SetEnvironment"]!!.obj!!["env"],
        )

        // Camera: previous, or the format defaults.
        val cam = computeInverse(json("""{"SetCamera": {"camera": {"fov_degrees": 70}}}"""), state)
        assertEquals(state.camera?.toJson(), cam.obj!!["SetCamera"]!!.obj!!["camera"])
        val defaultCam = computeInverse(
            json("""{"SetCamera": {"camera": {}}}"""),
            foldLog(base, listOf(entryOf("""[{"SetCamera": {"camera": null}}]"""))),
        ).obj!!["SetCamera"]!!.obj!!["camera"]!!
        assertEquals(json("""[5, 5, 5]"""), defaultCam.obj!!["position"])
        assertEquals(json("""[0, 0, 0]"""), defaultCam.obj!!["look_at"])
        assertEquals(json("45"), defaultCam.obj!!["fov_degrees"])
        assertEquals(
            json("""{"position": [5, 5, 5], "look_at": [0, 0, 0], "fov_degrees": 45}"""),
            defaultCam,
        )

        // Ambience: previous, or the empty list.
        val withAmbience = foldLog(base, listOf(entryOf("""[{"SetAmbience": {"ambience": [{"Wind": {"strength": 2}}]}}]""")))
        assertEquals(
            json("""[{"Wind": {"strength": 2}}]"""),
            computeInverse(json("""{"SetAmbience": {"ambience": []}}"""), withAmbience).obj!!["SetAmbience"]!!.obj!!["ambience"],
        )
        assertEquals(
            json("[]"),
            computeInverse(json("""{"SetAmbience": {"ambience": []}}"""), state).obj!!["SetAmbience"]!!.obj!!["ambience"],
        )
    }

    @Test
    fun audioEmittersInverseSymmetrically() {
        val withEmitter = foldLog(base, listOf(entryOf("""[{"SpawnAudioEmitter": {"name": "wind", "audio": {"Wind": {"strength": 1}}}}]""")))
        assertEquals(
            json("""{"RemoveAudioEmitter": {"name": "wind"}}"""),
            computeInverse(json("""{"SpawnAudioEmitter": {"name": "wind", "audio": {"Wind": {"strength": 1}}}}"""), withEmitter),
        )
        // Remove's inverse restores what the fold held — computed
        // against the state before the removal.
        assertEquals(
            json("""{"SpawnAudioEmitter": {"name": "wind", "audio": {"Wind": {"strength": 1}}}}"""),
            computeInverse(json("""{"RemoveAudioEmitter": {"name": "wind"}}"""), withEmitter),
        )
        // Removing something that isn't there is the same refusal
        // the fold makes.
        assertRefuses("no audio emitter named 'gone'") {
            computeInverse(json("""{"RemoveAudioEmitter": {"name": "gone"}}"""), withEmitter)
        }
    }

    /** A batch's inverse: the member inverses in reverse order, each
     *  computed against its own point in the batch (a forward walk
     *  over a trial copy, emitted back-to-front). */
    @Test
    fun aBatchInversesInReverseOrderAgainstTheTrial() {
        val ops = """[{"Batch": {"ops": [
            {"SpawnEntity": {"entity": {"id": 900, "name": "x"}}},
            {"SpawnEntity": {"entity": {"id": 901, "name": "y", "parent": 900}}},
            {"ModifyEntity": {"id": 901, "patch": {"name": "y2"}}}
        ]}}]"""
        // The inverse is computed against the state *before* the batch
        // applies — the state the edit is about to change.
        val state = foldLog(base, emptyList())
        val batchOp = json(ops).arr!!.first()
        val inverse = computeInverse(batchOp, state)
        val inner = inverse.obj!!["Batch"]!!.obj!!["ops"]!!.arr!!
        // Reverse order, last-in-first-out.
        assertEquals("ModifyEntity", inner[0].obj!!.keys.first())
        assertEquals(json("""{"ModifyEntity": {"id": 901, "patch": {"name": "y"}}}"""), inner[0])
        assertEquals("DeleteEntity", inner[1].obj!!.keys.first())
        assertEquals(901, inner[1].obj!!["DeleteEntity"]!!.obj!!["id"]!!.int)
        assertEquals(900, inner[2].obj!!["DeleteEntity"]!!.obj!!["id"]!!.int)

        // Round-trip: the batch, then its inverse — back to the base.
        val after = foldLog(base, listOf(entryOf(ops), entryOf("[$inverse]", revision = 2, timestampMs = 1.0)))
        assertTrue(after.entities.none { it.id in setOf(900, 901) })
    }

    @Test
    fun nothingToInverseIsARefusal() {
        val state = foldLog(base, emptyList())
        assertRefuses("no inverse for a non-edit op") { computeInverse(json("""{"tool": "x", "args": {}}"""), state) }
        assertRefuses("no inverse for a non-edit op") { computeInverse(json("""{"nope": 1}"""), state) }
        assertRefuses("no entity 424242") { computeInverse(json("""{"DeleteEntity": {"id": 424242}}"""), state) }
        assertRefuses("no entity 424242") { computeInverse(json("""{"ModifyEntity": {"id": 424242, "patch": {}}}"""), state) }
    }
}
