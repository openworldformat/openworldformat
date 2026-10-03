// mergeBranch: the merge authority's id rewrite (spec/session.md) —
// the golden scenario plus the rules around it, mirrored from the
// other references' tests.

package org.openworldformat

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class MergeTests {
    /** Main holds exactly entity 5; the branch reuses the id. */
    private val mainManifest =
        parseManifest("""{"version": 3, "meta": {"name": "m"}, "entities": [{"id": 5, "name": "main-five"}]}""")
    private val main get() = foldLog(mainManifest, emptyList())

    /** The golden scenario: the branch spawns 5 (colliding) plus a
     *  child 6 parented to 5 with a behavior ref to 5. The remap moves
     *  5 past everything either line holds (7), and rewrites the
     *  parent and the numeric ref with it. */
    @Test
    fun collidingIdsRemapAndTheMergedBranchFolds() {
        val branch = listOf(
            entryOf(
                """[{"SpawnEntity": {"entity": {"id": 5, "name": "branch-five"}}}, """ +
                    """{"SpawnEntity": {"entity": {"id": 6, "name": "child", "parent": 5, """ +
                    """"behaviors": [{"Orbit": {"center": 5, "radius": 1.5}}]}}}]""",
            ),
        )
        val result = mergeBranch(main, branch)
        assertEquals(mapOf(5 to 7), result.remapped)

        // The rewritten branch folds over main, and every reference to
        // the moved id moved with it.
        val merged = foldLog(mainManifest, result.entries)
        assertEquals(5, merged.entities.first { it.name == "main-five" }.id)
        assertEquals(7, merged.entities.first { it.name == "branch-five" }.id)
        val child = merged.entities.first { it.name == "child" }
        assertEquals(6, child.id)
        assertEquals(7, child.parent)
        val orbit = child.fields["behaviors"]!!.arr!!.first().obj!!["Orbit"]!!.obj!!
        assertEquals(7, orbit["center"]!!.int)
    }

    /** Modify and Delete ids remap too — and a patch that reparents
     *  onto a remapped id follows it. */
    @Test
    fun modifyAndDeleteIdsRemapIncludingAPatchedParent() {
        val branch = listOf(
            entryOf(
                """[{"SpawnEntity": {"entity": {"id": 5, "name": "b5"}}}, {"SpawnEntity": {"entity": {"id": 6, "name": "b6", "parent": 5}}}]""",
            ),
            entryOf(
                """[{"ModifyEntity": {"id": 6, "patch": {"parent": 5}}}, {"DeleteEntity": {"id": 5}}]""",
                revision = 2, timestampMs = 1.0,
            ),
        )
        val result = mergeBranch(main, branch)
        assertEquals(mapOf(5 to 7), result.remapped)
        val second = result.entries[1].ops
        assertEquals(
            json("""{"ModifyEntity": {"id": 6, "patch": {"parent": 7}}}"""),
            second[0],
        )
        assertEquals(json("""{"DeleteEntity": {"id": 7}}"""), second[1])
        // The whole rewritten branch folds over main — the delete takes
        // the respawned subtree with it.
        val merged = foldLog(mainManifest, result.entries)
        assertEquals(listOf(5), merged.entities.map { it.id })
    }

    /** No collisions: the branch passes through unchanged, history ops
     *  and all. */
    @Test
    fun aBranchWithNoCollisionsPassesThroughUnchanged() {
        val branch = listOf(
            entryOf("""[{"SpawnEntity": {"entity": {"id": 100, "name": "far-away"}}}]"""),
            entryOf("""[{"tool": "gen", "args": {}}]""", revision = 2, timestampMs = 1.0),
        )
        val result = mergeBranch(main, branch)
        assertTrue(result.remapped.isEmpty())
        assertEquals(branch, result.entries)
    }

    /** String refs and history ops ride untouched: the name binds at
     *  ingestion against the merged fold-so-far. */
    @Test
    fun historyOpsAndStringRefsAreUntouched() {
        val branch = listOf(
            entryOf(
                """[{"tool": "gen", "args": {}}, {"SpawnEntity": {"entity": {"id": 5, "name": "b5", """ +
                    """"behaviors": [{"Orbit": {"center": "main-five"}}]}}}]""",
            ),
        )
        val result = mergeBranch(main, branch)
        // The branch spawns only 5 here, so 6 is the first free id.
        assertEquals(mapOf(5 to 6), result.remapped)
        assertEquals(
            json("""{"tool": "gen", "args": {}}"""),
            result.entries[0].ops[0],
        )
        val behaviors =
            result.entries[0].ops[1].obj!!["SpawnEntity"]!!.obj!!["entity"]!!.obj!!["behaviors"]!!
        assertEquals(json("""[{"Orbit": {"center": "main-five"}}]"""), behaviors)

        // Folded, the string ref resolves against main's entity.
        val merged = foldLog(mainManifest, result.entries)
        val orbit = merged.entities.first { it.id == 6 }.fields["behaviors"]!!.arr!!.first().obj!!["Orbit"]!!.obj!!
        assertEquals(5, orbit["center"]!!.int)
    }

    /** Fresh ids skip everything main and the branch hold — a fresh id
     *  that shadowed a branch id would silently graft one entity onto
     *  another's references. */
    @Test
    fun freshIdsSkipEverythingMainAndTheBranchHold() {
        val branch = listOf(
            entryOf(
                """[{"SpawnEntity": {"entity": {"id": 5, "name": "b5"}}}, {"SpawnEntity": {"entity": {"id": 6, "name": "b6"}}}, {"SpawnEntity": {"entity": {"id": 7, "name": "b7"}}}]""",
            ),
        )
        val result = mergeBranch(main, branch)
        assertEquals(mapOf(5 to 8), result.remapped)
    }

    /** The ceiling is the ceiling: this surface holds ids as Int, so
     *  the Int bound bites before 2^53 − 1 — either way, no id past
     *  the bound is ever handed out, even to heal a collision. */
    @Test
    fun theMergeRefusesWhenNoIdBelowTheCeilingIsFree() {
        val capped = parseManifest(
            """{"version": 3, "entities": [{"id": 2147483647, "name": "cap"}]}"""
        )
        val cappedState = foldLog(capped, emptyList())
        val branch = listOf(
            entryOf("""[{"SpawnEntity": {"entity": {"id": 2147483647, "name": "again"}}}]"""),
        )
        assertRefuses("ceiling") { mergeBranch(cappedState, branch) }
    }
}
