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
    public var audioEmitters: [String: JSONValue]
    /// How many edit ops the folded entries carried.
    public var appliedEdits: Int
    /// The entry ids folded, in order (`foldPath` only).
    public var path: [String]?
}

/// Fold log entries over a manifest: the document at the last entry.
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
        audioEmitters: [:],
        appliedEdits: 0,
        path: nil
    )
    for entry in entries {
        let edits = editOps(entry)
        if edits.isEmpty { continue }   // history folds to nothing
        var trial = state               // all-or-nothing, per entry
        for edit in edits {
            guard case let .edit(name, value) = edit else { continue }
            try applyEdit(&trial, name, value)
        }
        state = trial
        state.appliedEdits += edits.count
    }
    return state
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
