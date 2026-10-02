// The world document (manifest.json): typed where the format types
// it, passthrough where it doesn't — the same split as the other
// references. Spec: spec/world.md, schema version 3.

package org.openworldformat

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.Json

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
 * @throws [WorldFormatException] when the text isn't JSON, has no
 *   schema version, names a version newer than this reader (the
 *   versioning policy's hard line), or has no entities array.
 */
fun parseManifest(json: String): WorldManifest = WorldManifest(OWF_JSON.parseToJsonElement(json))

/** [buildJsonObject] — kept here so the companion reads cleanly. */
private fun buildJsonObject(builder: kotlinx.serialization.json.JsonObjectBuilder.() -> Unit): JsonObject =
    kotlinx.serialization.json.buildJsonObject(builder)
