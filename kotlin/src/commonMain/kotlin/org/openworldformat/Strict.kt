// Strict mode: the checked reading. The default is must-ignore (the
// tolerance the specification's readers owe every log); strict is the
// validator's stance — keys outside the format's vocabulary, ops of no
// recognized kind, and `ext-*` names the registry doesn't list are
// errors that name what to fix. Spec: spec/world.md, spec/session.md,
// spec/extensions/registry.json, spec/extensions/provenance.md.

package org.openworldformat

import kotlinx.serialization.json.JsonElement

/** The manifest's own top-level keys (schema/world.schema.json). */
internal val STRICT_MANIFEST_KEYS: Set<String> = setOf(
    "version", "meta", "entities", "environment", "camera",
    "avatar", "ambience", "tours", "soundtrack", "creations", "next_entity_id",
)

/** The keys `meta` defines. */
internal val STRICT_META_KEYS: Set<String> = setOf(
    "name", "description", "time_of_day", "tags", "source",
    "variation_group", "variation", "style_ref", "compliance",
)

/** The keys an entity defines. */
internal val STRICT_ENTITY_KEYS: Set<String> = setOf(
    "id", "name", "parent", "transform", "chunk", "shape", "material",
    "light", "audio", "behaviors", "modulations", "triggers",
    "mesh_asset", "instance_of", "creation_id",
)

/** One key against one vocabulary — the `ext-*` rule first, because a
 *  registered extension field is legal anywhere extension fields go.
 *
 *  @throws [WorldFormatException] naming the key, and for legacy
 *    producer fields, where they moved (`meta["ext-provenance"]`).
 */
internal fun checkStrictKey(where: String, key: String, allowed: Set<String>) {
    if (isExtensionKey(key)) {
        if (key !in REGISTERED_EXTENSIONS) {
            throw WorldFormatException.invalid(
                "'$key' ($where) is not in the extension registry — see spec/extensions/registry.json")
        }
        return
    }
    if (key in allowed) return
    if (where == "meta" && key in EXT_PROVENANCE_FIELDS) {
        throw WorldFormatException.invalid(
            "meta key '$key' moved to meta[\"ext-provenance\"] (spec/extensions/provenance.md)")
    }
    throw WorldFormatException.invalid("unknown $where key '$key'")
}

/** Strict-manifest check over a parsed document. */
internal fun validateManifestStrict(element: JsonElement) {
    val o = element.obj
        ?: throw WorldFormatException.invalid("manifest must be a JSON object")
    for (key in o.keys) checkStrictKey("manifest", key, STRICT_MANIFEST_KEYS)
    o["meta"]?.obj?.keys?.forEach { checkStrictKey("meta", it, STRICT_META_KEYS) }
    o["entities"]?.arr?.forEach { entity ->
        val eo = entity.obj
            ?: throw WorldFormatException.invalid("an entity must be a JSON object")
        for (key in eo.keys) checkStrictKey("entity", key, STRICT_ENTITY_KEYS)
    }
}

/** Strict-log check over one entry's ops: every op must classify, and
 *  every extension op must be registered. */
internal fun validateOpsStrict(ops: List<JsonElement>) {
    for (op in ops) {
        when (val classified = classifyOp(op)) {
            ClassifiedOp.Unknown ->
                throw WorldFormatException.invalid("op of no recognized kind isn't allowed in strict mode")
            is ClassifiedOp.Extension ->
                if (classified.name !in REGISTERED_EXTENSIONS) {
                    throw WorldFormatException.invalid(
                        "'${classified.name}' is not in the extension registry — see spec/extensions/registry.json")
                }
            else -> {}
        }
    }
}
