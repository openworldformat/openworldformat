// The authoring outcome suite (conformance/authoring/cases.json) through
// this reference's `ingest`: each batch over the base world, and what
// must come back — mirrors rust/tests/authoring_outcomes.rs, the
// normative runner. `ops[].path` is a JSON pointer into the committed
// op, `world[].path` one into the named entity; numbers compare by value.

package org.openworldformat

import kotlin.math.abs
import kotlin.math.max
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.test.fail
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** A JSON pointer into a value (`/a/0/b`), or null where it leads nowhere. */
private fun at(value: JsonElement, pointer: String): JsonElement? {
    var node: JsonElement = value
    for (raw in pointer.split("/").drop(1)) {
        val key = raw.replace("~1", "/").replace("~0", "~")
        node = when (node) {
            is JsonArray -> node.getOrNull(key.toIntOrNull() ?: return null) ?: return null
            is JsonObject -> node[key] ?: return null
            else -> return null
        }
    }
    return node
}

/** Numbers compare by value — the suite writes 2, a float field reads back 2.0. */
private fun same(a: JsonElement, b: JsonElement): Boolean {
    val x = a.dbl
    val y = b.dbl
    return when {
        x != null && y != null && a is JsonPrimitive && b is JsonPrimitive && !a.isString && !b.isString ->
            abs(x - y) <= 1e-6 * max(max(abs(x), abs(y)), 1.0)
        a is JsonArray && b is JsonArray -> a.size == b.size && a.zip(b).all { (p, q) -> same(p, q) }
        a is JsonObject && b is JsonObject -> a.size == b.size && a.all { (k, v) -> b[k]?.let { same(v, it) } == true }
        else -> a == b
    }
}

class AuthoringOutcomesTests {
    private val suite: JsonObject = json(readResource("conformance/authoring/cases.json")).obj!!

    private fun checkWorld(name: String, state: FoldState, predicates: List<JsonElement>) {
        for (p in predicates) {
            val predicate = p.obj!!
            val entityName = predicate["entity"]!!.str!!
            val entity = state.entities.firstOrNull { it.name == entityName }
            if (predicate["absent"]?.bool == true) {
                assertNull(entity, "$name: entity '$entityName' should be absent")
                continue
            }
            assertNotNull(entity, "$name: no entity '$entityName'")
            val path = predicate["path"]!!.str!!
            val json = entity.toJson()
            val got = at(json, path) ?: fail("$name: '$entityName' has nothing at $path: $json")
            val want = predicate["equals"]!!
            assertTrue(same(got, want), "$name: '$entityName' at $path is $got, expected $want")
        }
    }

    @Test
    fun authoringOutcomesHold() {
        val base = foldLog(parseManifest(suite["base"]!!.toString()), emptyList())
        val cases = suite["cases"]!!.arr!!
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val case = c.obj!!
            val name = case["name"]!!.str!!
            val expect = case["expect"]!!.obj!!
            val world = expect["world"]?.arr ?: emptyList()
            val result = ingest(base, case["batch"]!!)
            when (expect["result"]!!.str) {
                "committed" -> {
                    val done = result as? IngestResult.Ingested
                        ?: fail("$name: refused: ${(result as IngestResult.Refused).errors}")
                    assertNull(expect["errors_contain"], "$name: committed, but the suite expects an error")
                    for (want in expect["warnings_contain"]?.arr ?: emptyList()) {
                        val w = want.str!!
                        assertTrue(done.warnings.any { it.contains(w) }, "$name: no warning contains \"$w\": ${done.warnings}")
                    }
                    for ((n, id) in expect["spawned"]?.obj ?: emptyMap()) {
                        assertEquals(id.int, done.spawned[n], "$name: spawned $n -> ${done.spawned}")
                    }
                    for (p in expect["ops"]?.arr ?: emptyList()) {
                        val predicate = p.obj!!
                        val i = predicate["op"]!!.int!!
                        val path = predicate["path"]!!.str!!
                        val got = at(done.ops[i], path) ?: fail("$name: op $i has nothing at $path: ${done.ops[i]}")
                        val want = predicate["equals"]!!
                        assertTrue(same(got, want), "$name: op $i at $path is $got, expected $want")
                    }
                    checkWorld(name, done.state, world)
                }
                "refused" -> {
                    val refused = result as? IngestResult.Refused ?: fail("$name: committed, expected refused")
                    for (want in expect["errors_contain"]?.arr ?: emptyList()) {
                        val w = want.str!!
                        assertTrue(refused.errors.any { it.contains(w) }, "$name: no error contains \"$w\": ${refused.errors}")
                    }
                    checkWorld(name, base, world)
                }
                else -> fail("$name: unknown expectation ${expect["result"]}")
            }
        }
    }
}
