// The fold: state at any revision is a pure fold of the log over the
// base. Only edits change the document; history kinds fold to nothing;
// a batch applies all-or-nothing; and an entry that no longer applies
// stops the fold, exactly as the specification's readers do.
//
// Swift's value semantics are the all-or-nothing rule for free: a
// trial is a struct copy, and committing is assignment — the copy the
// JS reference makes with structuredClone, paid only when asked for.

import Foundation

/// The document at a point in the log — what `foldLog` and `foldPath`
/// return. `path` is set by `foldPath` only: the entry ids folded.
public struct FoldState: Equatable, Sendable {
    /// The world's name (manifest meta).
    public var name: String
    /// Base plus every applied edit.
    public var entities: [WorldEntity]
    public var environment: EnvironmentDef?
    public var camera: CameraDef?
    public var ambience: [JSONValue]
    /// The manifest schema version the world was read at.
    public var version: Int
    /// The world's metadata (its name is `name`).
    public var meta: WorldMeta?
    /// The rest of the manifest — avatar, tours, soundtrack, creations,
    /// next_entity_id and any field this reader doesn't type — so the
    /// fold's state is always a whole manifest (`toManifest`).
    public var scene: [String: JSONValue]
    public var audioEmitters: [String: JSONValue]
    /// How many edit ops the folded entries carried.
    public var appliedEdits: Int
    /// The entry ids folded, in order (`foldPath` only).
    public var path: [String]?
}

/// Fold log entries over a manifest: the document at the last entry.
///
/// Behavior refs written by name bind immediately (spec/world.md,
/// "Identity"): the base's against the complete base, and each entry's
/// against the fold-so-far plus that entry's own edits — an entry is
/// atomic, so its names resolve after its edits apply and before it
/// commits. A name nothing answers fails the entry; the fold stops
/// there, exactly as any other refusal does.
///
/// - Throws: at the first entry that no longer applies —
///   `OpenWorldFormatError.invalid`.
public func foldLog(_ manifest: WorldManifest, _ entries: [LogEntry]) throws -> FoldState {
    var state = FoldState(
        name: manifest.name,
        entities: manifest.entities,   // value copy: the manifest is never mutated
        environment: manifest.environment,
        camera: manifest.camera,
        ambience: manifest.ambience,
        version: manifest.version,
        meta: manifest.meta,
        scene: sceneOf(manifest),
        audioEmitters: [:],
        appliedEdits: 0,
        path: nil
    )
    // Base refs resolve against the complete base.
    for index in state.entities.indices {
        try resolveNames(&state, index)
    }
    for entry in entries {
        let edits = editOps(entry)
        if edits.isEmpty { continue }   // history folds to nothing
        var trial = state               // all-or-nothing, per entry
        for edit in edits {
            guard case let .edit(name, value) = edit else { continue }
            try applyEdit(&trial, name, value)
        }
        // The entry's names bind now, against the state its edits just
        // made — delaying is what the spec forbids.
        for id in touchedEntities(edits) {
            if let index = trial.entities.firstIndex(where: { $0.id == id }) {
                try resolveNames(&trial, index)
            }
        }
        state = trial
        state.appliedEdits += edits.count
    }
    return state
}

// MARK: - The whole document

/// The manifest's untyped fields, with `next_entity_id` at its effective
/// value: ids are never reused, so it is at least one past the largest.
func sceneOf(_ manifest: WorldManifest) -> [String: JSONValue] {
    var scene = manifest.fields
    let past = (manifest.entities.map(\.id).max() ?? 0) + 1
    let declared = scene["next_entity_id"]?.int ?? 1
    scene["next_entity_id"] = .number(Double(max(declared, past)))
    return scene
}

/// The fold's state as a manifest — the whole document. The fold is
/// total (spec/session.md): `toManifest(foldLog(m, []))` is `m` again, up
/// to name binding, entity order and absent-versus-default fields; a
/// head-first package's `manifest.json` is `toManifest` of its fold to
/// `main`.
public func toManifest(_ state: FoldState) throws -> WorldManifest {
    var o = state.scene
    o["version"] = .number(Double(state.version))
    var meta = state.meta ?? WorldMeta(json: .object([:]))
    meta.name = state.name
    var m = meta.fields
    m["name"] = .string(state.name)
    if let description = meta.description { m["description"] = .string(description) }
    if !meta.tags.isEmpty { m["tags"] = .array(meta.tags.map(JSONValue.string)) }
    o["meta"] = .object(m)
    o["entities"] = .array(state.entities.map(\.json))
    if let environment = state.environment { o["environment"] = environment.json }
    if let camera = state.camera { o["camera"] = camera.json }
    if !state.ambience.isEmpty { o["ambience"] = .array(state.ambience) }
    let past = (state.entities.map(\.id).max() ?? 0) + 1
    let floor = state.scene["next_entity_id"]?.int ?? 1
    o["next_entity_id"] = .number(Double(max(floor, past)))
    return try WorldManifest(json: .object(o))
}

// MARK: - Immediate name binding

/// The entity ids an entry's edit ops touch — spawn or modify, batches
/// recursed — the entities whose name refs bind when the entry commits.
func touchedEntities(_ edits: [ClassifiedOp]) -> [Int] {
    var ids: [Int] = []
    func walk(_ name: String, _ value: JSONValue) {
        switch name {
        case "SpawnEntity":
            if let id = value["entity"]?["id"]?.int { ids.append(id) }
        case "ModifyEntity":
            if let id = value["id"]?.int { ids.append(id) }
        case "Batch":
            for op in value["ops"]?.array ?? [] {
                if case let .edit(n, v) = classifyOp(op) { walk(n, v) }
            }
        default:
            break
        }
    }
    for edit in edits {
        if case let .edit(name, value) = edit { walk(name, value) }
    }
    return ids
}

/// Resolve one entity's behavior refs written by name to ids, against
/// the fold-so-far (`state.entities`): `Orbit.center` and
/// `LookAt.target` when the ref is a string. Refs already numeric stay;
/// `modulations[].target` is a property name, not an entity ref, and is
/// never touched.
func resolveNames(_ state: inout FoldState, _ entityIndex: Int) throws {
    guard let behaviors = state.entities[entityIndex].fields["behaviors"]?.array else { return }
    var resolved: [JSONValue] = []
    var changed = false
    for behavior in behaviors {
        guard let b = behavior.object, b.count == 1, let (kind, params) = b.first,
              var p = params.object
        else {
            resolved.append(behavior)
            continue
        }
        let refKey: String
        switch kind {
        case "Orbit": refKey = "center"
        case "LookAt": refKey = "target"
        default: refKey = ""
        }
        if !refKey.isEmpty, let name = p[refKey]?.string {
            p[refKey] = .number(Double(try entityId(named: name, in: state)))
            changed = true
            resolved.append(.object([kind: .object(p)]))
        } else {
            resolved.append(behavior)
        }
    }
    if changed {
        state.entities[entityIndex].fields["behaviors"] = .array(resolved)
    }
}

/// The id an entity name answers to, among the state's entities.
private func entityId(named name: String, in state: FoldState) throws -> Int {
    guard let entity = state.entities.first(where: { $0.name == name }) else {
        throw OpenWorldFormatError.invalid("no entity named '\(name)'")
    }
    return entity.id
}

// MARK: - Applying one edit

/// Apply one edit op to a fold state. Indexes are derived per call —
/// the trial state is the truth, and worlds are tens of entities.
func applyEdit(_ state: inout FoldState, _ edit: String, _ value: JSONValue) throws {
    switch edit {
    case "SpawnEntity":
        guard let raw = value["entity"] else {
            throw OpenWorldFormatError.invalid("SpawnEntity needs an entity with id and name")
        }
        let entity: WorldEntity
        do {
            entity = try WorldEntity(json: raw)
        } catch {
            throw OpenWorldFormatError.invalid("SpawnEntity needs an entity with id and name")
        }
        guard entity.id <= MAX_ENTITY_ID else {
            throw OpenWorldFormatError.invalid(
                "entity \(entity.id) exceeds the id ceiling \(MAX_ENTITY_ID) (2^53-1)")
        }
        let byId = Dictionary(uniqueKeysWithValues: state.entities.map { ($0.id, $0) })
        let names = Set(state.entities.map(\.name))
        if byId[entity.id] != nil {
            throw OpenWorldFormatError.invalid("entity \(entity.id) already exists")
        }
        if names.contains(entity.name) {
            throw OpenWorldFormatError.invalid("an entity named '\(entity.name)' already exists")
        }
        if let parent = entity.parent, byId[parent] == nil {
            throw OpenWorldFormatError.invalid("entity \(entity.id)'s parent \(parent) isn't in the document")
        }
        state.entities.append(entity)
        let floor = state.scene["next_entity_id"]?.int ?? 1
        state.scene["next_entity_id"] = .number(Double(max(floor, entity.id + 1)))

    case "DeleteEntity":
        guard let id = value["id"]?.int else {
            throw OpenWorldFormatError.invalid("DeleteEntity needs an id")
        }
        guard state.entities.contains(where: { $0.id == id }) else {
            throw OpenWorldFormatError.invalid("no entity \(id)")
        }
        // Descendants go with it: collect the subtree, then remove.
        var doomed: Set<Int> = [id]
        var grew = true
        while grew {
            grew = false
            for e in state.entities {
                if let parent = e.parent, doomed.contains(parent), !doomed.contains(e.id) {
                    doomed.insert(e.id)
                    grew = true
                }
            }
        }
        state.entities.removeAll { doomed.contains($0.id) }

    case "ModifyEntity":
        guard let id = value["id"]?.int else {
            throw OpenWorldFormatError.invalid("ModifyEntity needs an id")
        }
        guard let index = state.entities.firstIndex(where: { $0.id == id }) else {
            throw OpenWorldFormatError.invalid("no entity \(id)")
        }
        let patch = value["patch"]?.object ?? [:]
        let entity = state.entities[index]
        let byId = Dictionary(uniqueKeysWithValues: state.entities.map { ($0.id, $0) })
        var names = Set(state.entities.map(\.name))

        // Absent = unchanged; null = clear; value = set.
        if let nameJSON = patch["name"] {
            guard let newName = nameJSON.string else {
                throw OpenWorldFormatError.invalid("an entity can't have no name")
            }
            if newName != entity.name {
                if names.contains(newName) {
                    throw OpenWorldFormatError.invalid("an entity named '\(newName)' already exists")
                }
                names.remove(entity.name)
                state.entities[index].name = newName
                names.insert(newName)
            }
        }
        if let parentJSON = patch["parent"] {
            let newParent = parentJSON == .null ? nil : parentJSON.int
            if let p = newParent, byId[p] == nil {
                throw OpenWorldFormatError.invalid("parent \(p) isn't in the document")
            }
            // A parent cycle would make the entity its own ancestor.
            var seen: Set<Int> = [entity.id]
            var ancestor = newParent
            while let a = ancestor {
                if seen.contains(a) {
                    throw OpenWorldFormatError.invalid("entity \(entity.id) can't be its own ancestor")
                }
                seen.insert(a)
                ancestor = byId[a]?.parent
            }
            state.entities[index].parent = newParent
        }
        let componentFields = [
            "transform", "shape", "material", "light", "audio", "behaviors",
            "mesh_asset", "modulations", "instance_of", "triggers",
        ]
        for field in componentFields where patch[field] != nil {
            if patch[field] == .null {
                state.entities[index].fields.removeValue(forKey: field)
            } else {
                state.entities[index].fields[field] = patch[field]
            }
        }
        // Extension fields ride along: any `ext-*` key patches like the
        // known ones — set, or clear on null — so a physics component
        // (or any future extension's) survives a modify round-trip.
        for (field, v) in patch where field.hasPrefix("ext-") {
            if v == .null {
                state.entities[index].fields.removeValue(forKey: field)
            } else {
                state.entities[index].fields[field] = v
            }
        }

    case "SetEnvironment":
        state.environment = value["env"].map(EnvironmentDef.init(json:))

    case "SetCamera":
        state.camera = value["camera"].map(CameraDef.init(json:))

    case "SetAmbience":
        state.ambience = value["ambience"]?.array ?? []

    case "SpawnAudioEmitter":
        guard let name = value["name"]?.string else {
            throw OpenWorldFormatError.invalid("SpawnAudioEmitter needs a name")
        }
        state.audioEmitters[name] = value["audio"] ?? .null

    case "RemoveAudioEmitter":
        guard let name = value["name"]?.string else {
            throw OpenWorldFormatError.invalid("RemoveAudioEmitter needs a name")
        }
        guard state.audioEmitters[name] != nil else {
            throw OpenWorldFormatError.invalid("no audio emitter named '\(name)'")
        }
        state.audioEmitters.removeValue(forKey: name)

    case "ModifyWorld":
        // The scene-wide fields, patched like an entity: absent
        // unchanged, null clears, a value sets (spec/session.md).
        let patch = value["patch"]?.object ?? [:]
        if let meta = patch["meta"] {
            guard let name = meta["name"]?.string,
                  !name.trimmingCharacters(in: .whitespaces).isEmpty else {
                throw OpenWorldFormatError.invalid(
                    "ModifyWorld.meta must be a meta object with a name; it can't be cleared")
            }
            state.meta = WorldMeta(json: meta)
            state.name = name
        }
        if let env = patch["environment"] {
            state.environment = env == .null ? nil : EnvironmentDef(json: env)
        }
        if let camera = patch["camera"] {
            state.camera = camera == .null ? nil : CameraDef(json: camera)
        }
        if let ambience = patch["ambience"] {
            state.ambience = ambience.array ?? []
        }
        for field in ["avatar", "soundtrack", "tours", "creations"] {
            guard let v = patch[field] else { continue }
            if v == .null || v.array?.isEmpty == true {
                state.scene.removeValue(forKey: field)
            } else {
                state.scene[field] = v
            }
        }

    case "Batch":
        let ops = value["ops"]?.array ?? []
        var trial = state               // all-or-nothing, per batch
        for op in ops {
            guard case let .edit(name, inner) = classifyOp(op) else {
                continue                // history inside a batch folds to nothing too
            }
            try applyEdit(&trial, name, inner)
        }
        state = trial

    default:
        throw OpenWorldFormatError.invalid("unknown edit \(edit)")
    }
}
