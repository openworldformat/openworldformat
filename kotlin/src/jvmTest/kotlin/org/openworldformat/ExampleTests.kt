// The example packages, folded and replayed — mirrors
// js/test/example.test.mjs and python/tests/test_example.py (the
// physics replay stays in the Rust crate's solver).

package org.openworldformat

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class DropTests {
    private fun loadDrop(): WorldPackage =
        loadWorldPackage(File(repoRoot(), "examples/the-drop-test"))

    @Test
    fun theDropTestPackageFoldsOneEditEverythingElseIsHistory() {
        val pkg = loadDrop()
        assertEquals(6, pkg.base.entities.size)
        assertEquals(7, pkg.manifest.entities.size, "the head is the fold to main")
        val state = pkg.folded()
        assertEquals(1, state.appliedEdits)
        assertTrue(state.entities.any { it.name == "ball_late" })
        // The recorded run folds to nothing for the document…
        assertEquals(7, state.entities.size)
    }

    @Test
    fun theSwitchScoreCrossedTheLogAsAClickWould() {
        val folded = loadDrop().stateValues()
        assertEquals(json("10"), folded.values["score.switch"])
        assertEquals(emptyList(), folded.undeclared)
    }
}

class SpeedrunForkTests {
    private val pkg =
        loadWorldPackage(File(repoRoot(), "examples/speedrun-fork"))

    @Test
    fun aChallengeChainIsAHistoryTwoRunsForkOneCourse() {
        val manifest = pkg.base
        val entries = pkg.entries

        val history = buildHistory(entries)
        // Two tips — the two runs — and both are children of the course head.
        assertEquals(listOf("e3", "e4"), history.tips.sorted())
        assertEquals(listOf("e3", "e4"), history.children["e2"]?.sorted())

        // The trunk is the course: banner and checkpoint flag, no runs folded in.
        val trunk = foldPath(manifest, entries, tip = "e2")
        val trunkNames = trunk.entities.map { it.name }.toSet()
        assertTrue("banner" in trunkNames)
        assertTrue("checkpoint_flag" in trunkNames)

        // Each run folds the course plus its own inputs and its own time.
        val runs = mapOf(
            "e3" to ("run.kai" to 9.42),
            "e4" to ("run.noor" to 7.91),
        )
        for ((tip, run) in runs) {
            val (field, time) = run
            val runState = foldPath(manifest, entries, tip = tip)
            assertEquals(trunk.entities.size, runState.entities.size)  // no edits in a run
            val chain = runState.path!!.mapNotNull { id -> history.entry(id)?.entry }
            val folded = foldState(pkg.stateDocument, chain)
            assertEquals(json("$time"), folded.values[field])
            val other = if (field == "run.kai") "run.noor" else "run.kai"
            assertEquals(json("0.0"), folded.values[other])  // the other run never happened here
            val samples = chain.flatMap { it.ops }.count { op ->
                classifyOp(op) is ClassifiedOp.Input
            }
            assertEquals(5, samples)  // the playthrough, recorded
        }
    }
}
