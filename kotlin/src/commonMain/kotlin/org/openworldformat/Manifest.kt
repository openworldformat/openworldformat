// The world document (manifest.json): typed where the format types
// it, passthrough where it doesn't — the same split as the other
// references. Spec: spec/world.md, schema version 3.

package org.openworldformat

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.Json

/** The five keys `meta["ext-provenance"]` defines
 *  (spec/extensions/provenance.md). */
val EXT_PROVENANCE_FIELDS: List<String> = listOf(
    "prompt",
    "model",
    "generation_duration_ms",
    "biome",
    "semantic_category",
)

/**
 * LLM lineage metadata (`meta["ext-provenance"]`): the core schema is
 * governed independently of any single producer, so the fields that
 * track how a world was generated live in the extension's namespace.
 * Spec: spec/extensions/provenance.md.
 */
data class ExtProvenance(
    val prompt: String?,
    val model: String?,
    val generationDurationMs: Double?,
    val biome: String?,
    val semanticCategory: String?,
) {
    companion object {
        /** Read an `ext-provenance` object; null when absent or not an
         *  object — must-ignore, like every extension field. */
        operator fun invoke(json: JsonElement?): ExtProvenance? {
            val o = json?.obj ?: return null
            return ExtProvenance(
                prompt = o["prompt"]?.str,
                model = o["model"]?.str,
                generationDurationMs = o["generation_duration_ms"]?.dbl,
                biome = o["biome"]?.str,
                semanticCategory = o["semantic_category"]?.str,
            )
        }
    }

    /** Back to JSON, writing only the keys that are set. */
    fun toJson(): JsonObject = buildJsonObject {
        prompt?.let { put("prompt", it) }
        model?.let { put("model", it) }
        generationDurationMs?.let { put("generation_duration_ms", it) }
        biome?.let { put("biome", it) }
        semanticCategory?.let { put("semantic_category", it) }
    }
}

/** World metadata (`meta`). */
data class WorldMeta(
    val name: String?,
    val description: String?,
    val tags: List<String>,
    val fields: JsonObject,
) {
    companion object {
        operator fun invoke(json: JsonElement?): WorldMeta? {
            val o = json?.obj ?: return null
            return WorldMeta(
                name = o["name"]?.str,
                description = o["description"]?.str,
                tags = o["tags"]?.arr?.mapNotNull { it.str }.orEmpty(),
                fields = JsonObject(o.filterKeys { it !in setOf("name", "description", "tags") }),
            )
        }
    }

    /** The world's LLM lineage, when `meta["ext-provenance"]` is present. */
    val extProvenance: ExtProvenance?
        get() = ExtProvenance(fields["ext-provenance"])
}

/** Environment settings (`environment`): background, fog, ambient light. */
data class EnvironmentDef(
    val backgroundColor: List<Double>?,
    val fogColor: List<Double>?,
    val fogDensity: Double?,
    val ambientColor: List<Double>?,
    val ambientIntensity: Double?,
    val fields: JsonObject,
) {
    companion object {
        private val known = setOf("background_color", "fog_color", "fog_density", "ambient_color", "ambient_intensity")

        operator fun invoke(json: JsonElement?): EnvironmentDef? {
            val o = json?.obj ?: return null
            return EnvironmentDef(
                backgroundColor = o["background_color"]?.arr?.mapNotNull { it.dbl },
                fogColor = o["fog_color"]?.arr?.mapNotNull { it.dbl },
                fogDensity = o["fog_density"]?.dbl,
                ambientColor = o["ambient_color"]?.arr?.mapNotNull { it.dbl },
                ambientIntensity = o["ambient_intensity"]?.dbl,
                fields = JsonObject(o.filterKeys { it !in known }),
            )
        }
    }

    /** Back to a JSON object — known keys when present, then whatever
     *  rode along. (Compaction and inverse ops need the fold's own
     *  environment back as JSON.) */
    fun toJson(): JsonObject = buildJsonObject {
        backgroundColor?.let { put("background_color", JsonArray(it.map(::JsonPrimitive))) }
        fogColor?.let { put("fog_color", JsonArray(it.map(::JsonPrimitive))) }
        fogDensity?.let { put("fog_density", it) }
        ambientColor?.let { put("ambient_color", JsonArray(it.map(::JsonPrimitive))) }
        ambientIntensity?.let { put("ambient_intensity", it) }
        fields.forEach { (k, v) -> put(k, v) }
    }
}

/** The camera (`camera`): where renders start. */
data class CameraDef(
    val position: List<Double>?,
    val lookAt: List<Double>?,
    val fovDegrees: Double?,
    val fields: JsonObject,
) {
    companion object {
        private val known = setOf("position", "look_at", "fov_degrees")

        operator fun invoke(json: JsonElement?): CameraDef? {
            val o = json?.obj ?: return null
            return CameraDef(
                position = o["position"]?.arr?.mapNotNull { it.dbl },
                lookAt = o["look_at"]?.arr?.mapNotNull { it.dbl },
                fovDegrees = o["fov_degrees"]?.dbl,
                fields = JsonObject(o.filterKeys { it !in known }),
            )
        }
    }

    /** Back to a JSON object — known keys when present, then whatever
     *  rode along. */
    fun toJson(): JsonObject = buildJsonObject {
        position?.let { put("position", JsonArray(it.map(::JsonPrimitive))) }
        lookAt?.let { put("look_at", JsonArray(it.map(::JsonPrimitive))) }
        fovDegrees?.let { put("fov_degrees", it) }
        fields.forEach { (k, v) -> put(k, v) }
    }
}

/**
 * One entity: identity typed, everything else carried. [fields] holds
 * every component the schema names (transform, shape, material,
 * light, audio, behaviors, modulations, triggers, mesh_asset,
 * instance_of, creation_id, chunk) plus every `ext-*` field, exactly
 * as the document wrote them — the fold patches this bag, and
 * must-ignore is the reader's side of the same rule.
 */
data class WorldEntity(
    val id: Int,
    val name: String,
    val parent: Int?,
    val fields: JsonObject,
) {
    companion object {
        operator fun invoke(json: JsonElement): WorldEntity {
            val o = json.obj
                ?: throw WorldFormatException("an entity must be a JSON object")
            val id = o["id"]?.int
                ?: throw WorldFormatException("an entity needs a numeric id")
            val name = o["name"]?.str
                ?: throw WorldFormatException("entity $id needs a string name")
            return WorldEntity(
                id = id,
                name = name,
                parent = o["parent"]?.int,
                fields = JsonObject(o.filterKeys { it != "id" && it != "name" && it != "parent" }),
            )
        }
    }

    /** Back to a JSON object, canonical shape. */
    fun toJson(): JsonObject = buildJsonObject {
        fields.forEach { (k, v) -> put(k, v) }
        put("id", JsonPrimitive(id))
        put("name", JsonPrimitive(name))
        parent?.let { put("parent", JsonPrimitive(it)) }
    }
}

/** The world document — everything needed to save or load a world. */
data class WorldManifest(
    val version: Int,
    val meta: WorldMeta?,
    val entities: List<WorldEntity>,
    val environment: EnvironmentDef?,
    val camera: CameraDef?,
    val ambience: List<JsonElement>,
    /** Everything else (soundtrack, tours, avatar, creations,
     *  next_entity_id, chunk, …) rides along untouched. */
    val fields: JsonObject,
) {
    /** The world's name, per `meta.name` — "" when absent. */
    val name: String get() = meta?.name ?: ""

    companion object {
        operator fun invoke(json: JsonElement): WorldManifest {
            val o = json.obj
                ?: throw WorldFormatException("manifest must be a JSON object")
            val version = o["version"]?.int
                ?: throw WorldFormatException("manifest has no schema version — refusing to guess")
            if (version > SUPPORTED_SCHEMA_VERSION) {
                throw WorldFormatException(
                    "manifest schema version $version is newer than this reader " +
                        "($SUPPORTED_SCHEMA_VERSION); a newer reader must read it — " +
                        "see the versioning policy")
            }
            val entitiesJson = o["entities"]?.arr
                ?: throw WorldFormatException("manifest has no entities array")
            return WorldManifest(
                version = version,
                meta = WorldMeta(o["meta"]),
                entities = entitiesJson.map { WorldEntity(it) },
                environment = EnvironmentDef(o["environment"]),
                camera = CameraDef(o["camera"]),
                ambience = o["ambience"]?.arr?.toList().orEmpty(),
                fields = JsonObject(
                    o.filterKeys {
                        it !in setOf("version", "meta", "entities", "environment", "camera", "ambience")
                    }),
            )
        }
    }
}

/**
 * Parse and sanity-check a world document.
 *
 * Strict mode ([strict]) checks keys against the schema's vocabulary:
 * top-level, `meta` and entity keys limited to what the format names
 * plus *registered* `ext-*` fields — the legacy producer fields
 * (`prompt`, `model`, `generation_duration_ms`, `biome`,
 * `semantic_category`) are refused with a pointer at
 * `meta["ext-provenance"]`. Non-strict behavior is exactly the
 * default: must-ignore for everything unrecognized.
 *
 * @throws [WorldFormatException] when the text isn't JSON, has no
 *   schema version, names a version newer than this reader (the
 *   versioning policy's hard line), has no entities array, or
 *   (strict) holds a key outside the vocabulary.
 */
fun parseManifest(json: String, strict: Boolean = false): WorldManifest {
    val element = OWF_JSON.parseToJsonElement(json)
    if (strict) validateManifestStrict(element)
    return WorldManifest(element)
}

/** [buildJsonObject] — kept here so the companion reads cleanly. */
private fun buildJsonObject(builder: kotlinx.serialization.json.JsonObjectBuilder.() -> Unit): JsonObject =
    kotlinx.serialization.json.buildJsonObject(builder)
