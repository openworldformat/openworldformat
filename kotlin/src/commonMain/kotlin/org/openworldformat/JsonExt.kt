// Package-wide types and the JSON accessors the fold speaks.
//
// kotlinx.serialization's JsonElement is the passthrough half of the
// format (the role JSONValue plays in the Swift package): components,
// extension fields and state values ride along untouched, nulls
// included — a stored JsonNull is "present and null", a missing key
// is "absent", and the patch semantics (absent = unchanged, null =
// clear, value = set) need exactly that distinction.

package org.openworldformat

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull

/** The manifest schema version this package reads. */
const val SUPPORTED_SCHEMA_VERSION: Int = 3

/**
 * The package format version this package reads: 2, head-first —
 * `manifest.json` is the world at the tip of `main`, the base lives in
 * `snapshots/base.json` (spec/package.md).
 */
const val SUPPORTED_FORMAT_VERSION: Int = 2

/** Where a head-first package keeps the state its log folds from. */
const val BASE_SNAPSHOT: String = "snapshots/base.json"

/** The fields `ModifyWorld`'s patch reaches (spec/session.md). */
val WORLD_PATCH_KEYS: List<String> = listOf(
    "meta", "environment", "camera", "avatar", "tours", "soundtrack", "ambience", "creations",
)

/**
 * The entity id ceiling: 2^53 − 1, the largest integer every JSON
 * number representation holds exactly (spec/security.md's robustness
 * bound, made numeric). A world written by a 64-bit allocator never
 * overflows the references that read it. This common surface holds
 * ids as [Int] — JS-safe by construction — so an id above the ceiling
 * can't even be held here; the check in the fold keeps the five
 * references symmetric.
 */
const val MAX_ENTITY_ID: Long = 9007199254740991

/** The extensions the registry names (spec/extensions/registry.json)
 *  — strict mode reads membership against this set. */
val REGISTERED_EXTENSIONS: Set<String> = setOf(
    "ext-physics",
    "ext-strict-determinism",
    "ext-visibility",
    "ext-cinematography",
    "ext-provenance",
)

/** A refusal: parse errors say what the document lacks; fold refusals
 *  carry the "invalid: " prefix the other references throw. */
class WorldFormatException(message: String) : RuntimeException(message) {
    companion object {
        fun invalid(message: String) = WorldFormatException("invalid: $message")
    }
}

/** The lenient parser the fold reads through. */
val OWF_JSON: Json = Json

// ---------------------------------------------------------------------------
// JsonElement accessors — null-typed lookups, presence-preserving
// ---------------------------------------------------------------------------

/** The object, if this is one. */
val JsonElement.obj: JsonObject?
    get() = this as? JsonObject

/** The array, if this is one. */
val JsonElement.arr: JsonArray?
    get() = this as? JsonArray

/** The string, if this is a non-null string primitive. */
val JsonElement.str: String?
    get() = (this as? JsonPrimitive)?.takeIf { it !is JsonNull && it.isString }?.content

/** The double, if this is a number. */
val JsonElement.dbl: Double?
    get() = (this as? JsonPrimitive)?.takeIf { it !is JsonNull }?.doubleOrNull

/** The integer, if this is a number with no fractional part. */
val JsonElement.int: Int?
    get() = dbl?.let { d -> if (d == Math.floor(d) && abs(d) < 9.007199254740992E15) d.toInt() else null }

/** The boolean, if this is one. */
val JsonElement.bool: Boolean?
    get() = (this as? JsonPrimitive)?.takeIf { it !is JsonNull }?.booleanOrNull

/** Key lookup that reads like the other references: `op["args"]`. */
operator fun JsonObject?.get(key: String): JsonElement? = this?.get(key)

/** Explicit-null check — the distinction the patch semantics live on. */
val JsonElement?.isNull: Boolean
    get() = this is JsonNull

/** A 3-component vector, as the format serializes it: [x, y, z]. */
data class Vec3(val x: Double, val y: Double, val z: Double) {
    companion object {
        fun from(json: JsonElement?): Vec3? {
            val a = json?.arr ?: return null
            if (a.size < 3) return null
            return Vec3(a[0].dbl ?: 0.0, a[1].dbl ?: 0.0, a[2].dbl ?: 0.0)
        }
    }
}

/** `ext-` followed by lowercase letters, digits and dashes (`ext-physics`). */
fun isExtensionKey(key: String): Boolean {
    if (!key.startsWith("ext-") || key.length <= 4) return false
    return key.drop(4).all { c -> c in 'a'..'z' || c in '0'..'9' || c == '-' }
}

private fun abs(d: Double) = if (d < 0) -d else d
