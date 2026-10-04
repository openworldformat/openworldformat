// The conformance worlds: every one parses, and the entity counts are
// pinned — the same worlds the js, python, rust and swift jobs fold,
// so a schema change that moves a count breaks all five references
// at once.

package org.openworldformat

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class ConformanceTests {
    /** (world, entity count) — pinned from the conformance worlds. */
    private val counts = mapOf(
        "behaviors" to 13,
        "hierarchy_rotations" to 9,
        "hierarchy_tours" to 12,
        "instances" to 7,
        "lights" to 8,
        "materials" to 21,
        "physics" to 5,
        "shapes" to 13,
        "soundtrack" to 10,
        "textures" to 8,
        "triggers" to 10,
    )

    /** The fold's identity rule, exercised on every world: ids and
     *  names are unique, parents exist. */
    @Test
    fun everyConformanceWorldParsesWithPinnedCountsAndCleanIdentity() {
        for ((world, count) in counts) {
            val manifest = parseManifest(readResource("conformance/$world.json"))
            assertEquals(count, manifest.entities.size, "$world.json entity count drifted")

            val ids = manifest.entities.map { it.id }
            assertEquals(ids.size, ids.toSet().size, "$world.json has duplicate entity ids")
            val names = manifest.entities.map { it.name }
            assertEquals(names.size, names.toSet().size, "$world.json has duplicate entity names")

            val known = ids.toSet()
            for (entity in manifest.entities) {
                val parent = entity.parent ?: continue
                assertTrue(known.contains(parent), "$world.json: ${entity.name}'s parent $parent is missing")
            }
        }
    }

    /** Every world's shapes decode into the typed vocabulary: the
     *  viewer profile must be able to draw what the suite declares. */
    @Test
    fun everyDeclaredShapeDecodesIntoTheTypedVocabulary() {
        var shapes = 0
        for (world in counts.keys.sorted()) {
            val manifest = parseManifest(readResource("conformance/$world.json"))
            for (entity in manifest.entities) {
                if (entity.fields["shape"] == null) continue
                assertNotNull(entity.shape, "$world.json: ${entity.name} declares a shape this package can't read")
                shapes += 1
            }
        }
        assertTrue(shapes > 20)  // the suite is a shape museum; sanity that we read them
    }

    /** A bare world (manifest only: no log, no state) loads as a
     *  package with an empty history — the fold over no entries is
     *  the base. */
    @Test
    fun aBareWorldLoadsAsAPackageWithNoHistory() {
        val pkg = WorldPackage(
            manifestText = readResource("conformance/shapes.json"),
        )
        assertEquals(emptyList(), pkg.entries)
        assertEquals(null, pkg.stateDocument)
        val state = pkg.folded()
        assertEquals(0, state.appliedEdits)
        assertEquals(13, state.entities.size)  // shapes, pinned above
        assertEquals(emptyList(), pkg.history().tips)  // nothing to branch
    }
}
