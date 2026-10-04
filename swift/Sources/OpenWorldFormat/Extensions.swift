// The extension registry's mirror, and the strict-mode allowlists it
// carves out.
//
// The registry itself is a file — spec/extensions/registry.json — not
// code: this module mirrors its names so strict mode can refuse an
// unregistered `ext-*` key by pointing at it. The LLM-provenance
// extension gets a typed view here too (spec/extensions/provenance.md);
// every other extension rides the passthrough untouched.

import Foundation

/// The extensions registered in spec/extensions/registry.json — the
/// `ext-*` names strict mode accepts wherever it checks keys.
public let REGISTERED_EXTENSIONS: Set<String> = [
    "ext-physics",
    "ext-strict-determinism",
    "ext-visibility",
    "ext-cinematography",
    "ext-provenance",
]

// MARK: - Provenance

/// The LLM-provenance extension's five fields, as
/// spec/extensions/provenance.md names them.
public let EXT_PROVENANCE_FIELDS: [String] = [
    "prompt",
    "model",
    "generation_duration_ms",
    "biome",
    "semantic_category",
]

/// `meta["ext-provenance"]`, typed: the lineage of a generated world —
/// what was typed, what ran, how long it took, and the two generation
/// hints. The extension is a namespace, so the passthrough `fields`
/// stay the storage; this is the read.
public struct ExtProvenance: Equatable, Sendable {
    public var prompt: String?
    public var model: String?
    public var generationDurationMs: Double?
    public var biome: String?
    public var semanticCategory: String?

    public init(
        prompt: String? = nil,
        model: String? = nil,
        generationDurationMs: Double? = nil,
        biome: String? = nil,
        semanticCategory: String? = nil
    ) {
        self.prompt = prompt
        self.model = model
        self.generationDurationMs = generationDurationMs
        self.biome = biome
        self.semanticCategory = semanticCategory
    }

    /// Read the extension out of a meta object — `meta["ext-provenance"]`'s
    /// five fields; an absent or empty extension reads as all-nil.
    public init(json: JSONValue) {
        let o = json["ext-provenance"]?.object ?? [:]
        prompt = o["prompt"]?.string
        model = o["model"]?.string
        generationDurationMs = o["generation_duration_ms"]?.double
        biome = o["biome"]?.string
        semanticCategory = o["semantic_category"]?.string
    }

    /// The `ext-provenance` object, to write back under
    /// `meta["ext-provenance"]` — absent fields stay absent, exactly as
    /// the extension writes them.
    public var json: JSONValue {
        var o: [String: JSONValue] = [:]
        if let v = prompt { o["prompt"] = .string(v) }
        if let v = model { o["model"] = .string(v) }
        if let v = generationDurationMs { o["generation_duration_ms"] = .number(v) }
        if let v = biome { o["biome"] = .string(v) }
        if let v = semanticCategory { o["semantic_category"] = .string(v) }
        return .object(o)
    }
}

extension WorldMeta {
    /// `meta["ext-provenance"]`, typed — a computed view; the
    /// passthrough `fields` remain the storage.
    public var extProvenance: ExtProvenance? {
        guard fields["ext-provenance"] != nil else { return nil }
        return ExtProvenance(json: .object(fields))
    }
}

// MARK: - Strict mode's allowlists

/// Strict mode's top-level manifest keys — schema/world.schema.json's
/// own properties — plus the registered `ext-*` names.
let strictManifestKeys: Set<String> = [
    "version", "meta", "entities", "environment", "camera", "avatar",
    "ambience", "tours", "soundtrack", "creations", "next_entity_id",
]

/// Strict mode's `meta` keys.
let strictMetaKeys: Set<String> = [
    "name", "description", "time_of_day", "tags", "source",
    "variation_group", "variation", "style_ref", "compliance",
]

/// Strict mode's entity keys.
let strictEntityKeys: Set<String> = [
    "id", "name", "parent", "transform", "chunk", "shape", "material",
    "light", "audio", "behaviors", "modulations", "triggers",
    "mesh_asset", "instance_of", "creation_id",
]

/// Refuse what no rule names: strict mode's manifest walk. An `ext-*`
/// key must be registered; a legacy provenance key is refused with a
/// pointer at the extension that owns it now; anything else unknown is
/// refused outright. Non-strict parses never call this.
func checkStrictManifest(_ json: JSONValue) throws {
    guard let o = json.object else { return }   // the non-object case is parseManifest's own error
    for key in o.keys {
        try checkStrictKey(key, allowed: strictManifestKeys, scope: "manifest")
    }
    if let meta = o["meta"]?.object {
        for key in meta.keys {
            if isExtensionKey(key) {
                try checkStrictExtension(key, scope: "meta")
                continue
            }
            if strictMetaKeys.contains(key) { continue }
            if EXT_PROVENANCE_FIELDS.contains(key) {
                throw OpenWorldFormatError.parse(
                    "strict: meta['\(key)'] is legacy provenance — it lives in "
                        + "meta[\"ext-provenance\"] now (spec/extensions/provenance.md)")
            }
            throw OpenWorldFormatError.parse("strict: '\(key)' is not a meta key")
        }
    }
    if let entities = o["entities"]?.array {
        for entity in entities {
            guard let entityObject = entity.object else { continue }   // WorldEntity(json:) reports its own
            for key in entityObject.keys {
                try checkStrictKey(key, allowed: strictEntityKeys, scope: "entity")
            }
        }
    }
}

/// One key against one allowlist: registered `ext-*` passes, everything
/// else must be named.
private func checkStrictKey(_ key: String, allowed: Set<String>, scope: String) throws {
    if isExtensionKey(key) {
        try checkStrictExtension(key, scope: scope)
        return
    }
    guard allowed.contains(key) else {
        let article = scope.first.map { "aeiou".contains($0) ? "an" : "a" } ?? "a"
        throw OpenWorldFormatError.parse("strict: '\(key)' is not \(article) \(scope) key")
    }
}

/// An `ext-*` name the registry hasn't registered, refused by naming
/// the registry it isn't in.
private func checkStrictExtension(_ key: String, scope: String) throws {
    guard REGISTERED_EXTENSIONS.contains(key) else {
        throw OpenWorldFormatError.parse(
            "strict: '\(key)' (\(scope)) is not in the extension registry "
                + "(spec/extensions/registry.json)")
    }
}
