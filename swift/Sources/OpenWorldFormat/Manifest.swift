// The world document (manifest.json): typed where the format types it,
// passthrough where it doesn't.
//
// Mirrors the schema's $defs the way the JS reference reads them: the
// identity fields (id, name, parent) are typed, every component and
// `ext-*` field rides in `fields` untouched. Spec: spec/world.md,
// schema version 3.

import Foundation

/// The manifest schema version this package reads.
public let SUPPORTED_SCHEMA_VERSION = 3

/// The package format version this package reads.
/// The package format this reader reads: 2, head-first — `manifest.json`
/// is the world at the tip of `main`, the base lives in
/// `snapshots/base.json` (spec/package.md).
public let SUPPORTED_FORMAT_VERSION = 2

/// Where a head-first package keeps the state its log folds from.
public let BASE_SNAPSHOT = "snapshots/base.json"

/// The fields `ModifyWorld`'s patch reaches (spec/session.md).
public let WORLD_PATCH_KEYS = [
    "meta", "environment", "camera", "avatar", "tours", "soundtrack", "ambience", "creations",
]

/// The entity id ceiling: 2^53 − 1, the largest integer every JSON
/// number holds exactly. `applyEdit`'s SpawnEntity refuses ids above
/// it, and a merge never hands one out — a world written by a 64-bit
/// allocator must not overflow the references that read it.
public let MAX_ENTITY_ID = 9007199254740991

/// A refusal. Parse errors say what the document lacks; `invalid` is
/// the fold's refusal prefix, the same string the other references
/// throw ("invalid: no entity 7") — tests across languages match on it.
public enum OpenWorldFormatError: Error, LocalizedError, Sendable, Equatable {
    case parse(String)
    case invalid(String)

    public var errorDescription: String? {
        switch self {
        case .parse(let message): return message
        case .invalid(let message): return "invalid: \(message)"
        }
    }
}

// MARK: - Meta

/// World metadata (`meta`).
public struct WorldMeta: Equatable, Sendable {
    public var name: String?
    public var description: String?
    public var tags: [String]
    public var fields: [String: JSONValue]

    public init(json: JSONValue) {
        let o = json.object ?? [:]
        name = o["name"]?.string
        description = o["description"]?.string
        tags = o["tags"]?.array?.compactMap(\.string) ?? []
        fields = o.filter { !["name", "description", "tags"].contains($0.key) }
    }
}

// MARK: - Environment and camera

/// Environment settings (`environment`): background, fog, ambient light.
public struct EnvironmentDef: Equatable, Sendable {
    public var backgroundColor: [Double]?
    public var fogColor: [Double]?
    public var fogDensity: Double?
    public var ambientColor: [Double]?
    public var ambientIntensity: Double?
    public var fields: [String: JSONValue]

    public init(json: JSONValue) {
        let o = json.object ?? [:]
        backgroundColor = o["background_color"]?.array?.compactMap(\.double)
        fogColor = o["fog_color"]?.array?.compactMap(\.double)
        fogDensity = o["fog_density"]?.double
        ambientColor = o["ambient_color"]?.array?.compactMap(\.double)
        ambientIntensity = o["ambient_intensity"]?.double
        fields = o.filter { !["background_color", "fog_color", "fog_density", "ambient_color", "ambient_intensity"].contains($0.key) }
    }

    init() {
        fields = [:]
    }

    /// The environment back to a JSON object: its typed fields and its
    /// passthrough, merged — the inverse of `init(json:)`.
    public var json: JSONValue {
        var o = fields
        if let v = backgroundColor { o["background_color"] = .array(v.map(JSONValue.number)) }
        if let v = fogColor { o["fog_color"] = .array(v.map(JSONValue.number)) }
        if let v = fogDensity { o["fog_density"] = .number(v) }
        if let v = ambientColor { o["ambient_color"] = .array(v.map(JSONValue.number)) }
        if let v = ambientIntensity { o["ambient_intensity"] = .number(v) }
        return .object(o)
    }
}

/// The camera (`camera`): where renders start.
public struct CameraDef: Equatable, Sendable {
    public var position: [Double]?
    public var lookAt: [Double]?
    public var fovDegrees: Double?
    public var fields: [String: JSONValue]

    public init(json: JSONValue) {
        let o = json.object ?? [:]
        position = o["position"]?.array?.compactMap(\.double)
        lookAt = o["look_at"]?.array?.compactMap(\.double)
        fovDegrees = o["fov_degrees"]?.double
        fields = o.filter { !["position", "look_at", "fov_degrees"].contains($0.key) }
    }

    init() {
        fields = [:]
    }

    /// The camera back to a JSON object: its typed fields and its
    /// passthrough, merged — the inverse of `init(json:)`.
    public var json: JSONValue {
        var o = fields
        if let v = position { o["position"] = .array(v.map(JSONValue.number)) }
        if let v = lookAt { o["look_at"] = .array(v.map(JSONValue.number)) }
        if let v = fovDegrees { o["fov_degrees"] = .number(v) }
        return .object(o)
    }
}

// MARK: - Entity

/// One entity: identity typed, everything else carried.
///
/// `fields` holds every component the schema names (transform, shape,
/// material, light, audio, behaviors, modulations, triggers,
/// mesh_asset, instance_of, creation_id, chunk) plus every `ext-*`
/// field, exactly as the document wrote them — the fold patches this
/// bag (absent = unchanged, null = clear, value = set), and must-ignore
/// is the reader's side of the same rule.
public struct WorldEntity: Equatable, Sendable {
    public var id: Int
    public var name: String
    public var parent: Int?
    public var fields: [String: JSONValue]

    public init(id: Int, name: String, parent: Int? = nil, fields: [String: JSONValue] = [:]) {
        self.id = id
        self.name = name
        self.parent = parent
        self.fields = fields
    }

    public init(json: JSONValue) throws {
        guard let o = json.object else {
            throw OpenWorldFormatError.parse("an entity must be a JSON object")
        }
        guard let id = o["id"]?.int else {
            throw OpenWorldFormatError.parse("an entity needs a numeric id")
        }
        guard let name = o["name"]?.string else {
            throw OpenWorldFormatError.parse("entity \(id) needs a string name")
        }
        var fields = o
        fields.removeValue(forKey: "id")
        fields.removeValue(forKey: "name")
        fields.removeValue(forKey: "parent")
        self.id = id
        self.name = name
        self.parent = o["parent"]?.int
        self.fields = fields
    }

    /// Back to a JSON object, canonical shape (id, name, parent, fields).
    public var json: JSONValue {
        var o = fields
        o["id"] = .number(Double(id))
        o["name"] = .string(name)
        if let parent { o["parent"] = .number(Double(parent)) }
        return .object(o)
    }
}

// MARK: - Manifest

/// The world document — everything needed to save or load a world.
public struct WorldManifest: Equatable, Sendable {
    public var version: Int
    public var meta: WorldMeta?
    public var entities: [WorldEntity]
    public var environment: EnvironmentDef?
    public var camera: CameraDef?
    public var ambience: [JSONValue]
    /// Everything else (soundtrack, tours, avatar, creations,
    /// next_entity_id, chunk, …) rides along untouched.
    public var fields: [String: JSONValue]

    /// The world's name, per `meta.name` — "" when absent.
    public var name: String { meta?.name ?? "" }

    public init(json: JSONValue) throws {
        guard let o = json.object else {
            throw OpenWorldFormatError.parse("manifest must be a JSON object")
        }
        guard let version = o["version"]?.int else {
            throw OpenWorldFormatError.parse("manifest has no schema version — refusing to guess")
        }
        guard version <= SUPPORTED_SCHEMA_VERSION else {
            throw OpenWorldFormatError.parse(
                "manifest schema version \(version) is newer than this reader (\(SUPPORTED_SCHEMA_VERSION)); "
                    + "a newer reader must read it — see the versioning policy")
        }
        guard let entityArray = o["entities"]?.array else {
            throw OpenWorldFormatError.parse("manifest has no entities array")
        }
        self.version = version
        self.meta = o["meta"].map(WorldMeta.init(json:))
        self.entities = try entityArray.map(WorldEntity.init(json:))
        self.environment = o["environment"].map(EnvironmentDef.init(json:))
        self.camera = o["camera"].map(CameraDef.init(json:))
        self.ambience = o["ambience"]?.array ?? []
        var fields = o
        for key in ["version", "meta", "entities", "environment", "camera", "ambience"] {
            fields.removeValue(forKey: key)
        }
        self.fields = fields
    }

    /// The manifest back to a JSON object, canonical shape.
    public var json: JSONValue {
        var o = fields
        o["version"] = .number(Double(version))
        if let meta {
            var m = meta.fields
            if let name = meta.name { m["name"] = .string(name) }
            if let description = meta.description { m["description"] = .string(description) }
            if !meta.tags.isEmpty { m["tags"] = .array(meta.tags.map(JSONValue.string)) }
            o["meta"] = .object(m)
        }
        o["entities"] = .array(entities.map(\.json))
        if let environment { o["environment"] = environment.json }
        if let camera { o["camera"] = camera.json }
        if !ambience.isEmpty { o["ambience"] = .array(ambience) }
        return .object(o)
    }
}

/// Parse and sanity-check a world document.
///
/// Strict mode (`strict: true`) is the validator's half of must-ignore:
/// where the default parse carries a key the schema doesn't name, strict
/// refuses it — unknown top-level, `meta` and entity keys, and any
/// `ext-*` name the registry hasn't registered. It is opt-in because
/// must-ignore stays the format's default (spec/README.md).
///
/// - Throws: `OpenWorldFormatError.parse` when the text isn't JSON, has
///   no schema version, names a version newer than this reader (the
///   versioning policy's hard line), has no entities array — or, when
///   strict, names a key no rule knows.
public func parseManifest(_ json: String, strict: Bool = false) throws -> WorldManifest {
    let value = try JSONValue(parsing: json)
    if strict { try checkStrictManifest(value) }
    return try WorldManifest(json: value)
}
