// The Kotlin package ships the repository's normative schemas
// (commonMain's resources, schemas/) so a consumer validates against
// its installed dependency — schemaText("world.schema.json") —
// instead of a checkout of this repository. The copies are
// byte-identical snapshots of schema/, never hand-edited: when the
// schema changes, copy the whole tree again (world, state,
// extensions/). These tests fail when a copy drifts and when a new
// schema appears in schema/ that the package does not ship. Mirrors
// js/test/schema-files.test.mjs.

package org.openworldformat

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject

class SchemaFilesTests {
    /** Every .schema.json under a directory, as names the package
     *  ships ("world.schema.json", "extensions/ext-h3.schema.json"). */
    private fun schemaNames(root: File): List<String> =
        root.walkTopDown()
            .filter { it.isFile && it.name.endsWith(".schema.json") }
            .map { it.relativeTo(root).invariantSeparatorsPath }
            .sorted()
            .toList()

    private val repository = schemaNames(File(repoRoot(), "schema"))
    private val shipped = schemaNames(File(repoRoot(), "kotlin/src/commonMain/resources/schemas"))

    /** The package's tree holds exactly the repository's, and each
     *  copy reads back byte for byte through the accessor — the same
     *  classloader read a consumer's runtime does. */
    @Test
    fun theShippedSchemasAreTheRepositorysByteForByte() {
        assertEquals(
            repository, shipped,
            "commonMain's schemas/ and schema/ hold different files — refresh the copies",
        )
        for (name in repository) {
            assertEquals(
                File(repoRoot(), "schema/$name").readText(), schemaText(name),
                "schema/$name drifted from schema/ — copy it again, never hand-edit it",
            )
        }
    }

    /** Every schema parses as a JSON Schema, and a name the package
     *  doesn't ship is refused. */
    @Test
    fun everySchemaReadsThroughTheAccessorAsAJsonSchema() {
        for (name in repository) {
            val draft = OWF_JSON.parseToJsonElement(schemaText(name))
                .jsonObject["\$schema"]
            assertTrue(draft is JsonPrimitive && draft.isString, "$name is a JSON Schema")
        }
        assertFailsWith<NoSuchElementException> { schemaText("no-such.schema.json") }
    }
}
