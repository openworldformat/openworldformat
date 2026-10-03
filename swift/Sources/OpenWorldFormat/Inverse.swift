// Undo, the format's way: appending the inverse — the log never
// rewinds (spec/session.md). `computeInverse` derives that op from the
// state the edit is about to apply to, so undo is a fold forward, like
// every other read.
//
// The camera defaults below are the format's own — what a SetCamera's
// inverse restores when no camera was ever set.

import Foundation

/// The camera a world with no `camera` renders from.
let cameraDefaults = JSONValue.object([
    "position": .array([.number(5), .number(5), .number(5)]),
    "look_at": .array([.number(0), .number(0), .number(0)]),
    "fov_degrees": .number(45),
])

/// The inverse of one edit op, computed against the state the edit is
/// about to apply to.
///
/// Spawn and delete mirror each other (delete's inverse restores the
/// whole subtree, parents first, as deep copies); a modify's inverse
/// restores every patched key to its current value — `.null` where the
/// entity had none, the same patch semantics run backwards; the
/// scene-wide setters restore what was there (or the format's defaults,
/// when nothing was); a batch inverts its inner edits in reverse order
/// against a running trial, so each inverse sees the state its op saw.
/// History ops have no document effect, so they have nothing to undo.
///
/// - Throws: `OpenWorldFormatError.invalid` when the op isn't one edit
///   kind, or names an entity (or audio emitter) the state doesn't hold.
public func computeInverse(_ op: JSONValue, _ state: FoldState) throws -> JSONValue {
    guard case let .edit(name, value) = classifyOp(op) else {
        throw OpenWorldFormatError.invalid("unknown edit \(op.object?.keys.first ?? "op")")
    }
    switch name {
    case "SpawnEntity":
        guard let id = value["entity"]?["id"]?.int else {
            throw OpenWorldFormatError.invalid("SpawnEntity needs an entity with id and name")
        }
        return .object(["DeleteEntity": .object(["id": .number(Double(id))])])

    case "DeleteEntity":
        guard let id = value["id"]?.int else {
            throw OpenWorldFormatError.invalid("DeleteEntity needs an id")
        }
        return .object(["Batch": .object(["ops": .array(try respawnSubtree(id, in: state))])])

    case "ModifyEntity":
        guard let id = value["id"]?.int else {
            throw OpenWorldFormatError.invalid("ModifyEntity needs an id")
        }
        guard let entity = state.entities.first(where: { $0.id == id }) else {
            throw OpenWorldFormatError.invalid("no entity \(id)")
        }
        var inversePatch: [String: JSONValue] = [:]
        let patch = value["patch"]?.object ?? [:]
        for key in patch.keys {
            switch key {
            case "name":
                inversePatch["name"] = .string(entity.name)
            case "parent":
                inversePatch["parent"] = entity.parent.map { .number(Double($0)) } ?? .null
            default:
                inversePatch[key] = entity.fields[key] ?? .null
            }
        }
        return .object(["ModifyEntity": .object([
            "id": .number(Double(id)),
            "patch": .object(inversePatch),
        ])])

    case "SetEnvironment":
        let env = state.environment?.json ?? .object([:])
        return .object(["SetEnvironment": .object(["env": env])])

    case "SetCamera":
        let camera = state.camera?.json ?? cameraDefaults
        return .object(["SetCamera": .object(["camera": camera])])

    case "SetAmbience":
        return .object(["SetAmbience": .object(["ambience": .array(state.ambience)])])

    case "SpawnAudioEmitter":
        guard let name = value["name"]?.string else {
            throw OpenWorldFormatError.invalid("SpawnAudioEmitter needs a name")
        }
        return .object(["RemoveAudioEmitter": .object(["name": .string(name)])])

    case "RemoveAudioEmitter":
        guard let name = value["name"]?.string else {
            throw OpenWorldFormatError.invalid("RemoveAudioEmitter needs a name")
        }
        guard let audio = state.audioEmitters[name] else {
            throw OpenWorldFormatError.invalid("no audio emitter named '\(name)'")
        }
        return .object(["SpawnAudioEmitter": .object(["name": .string(name), "audio": audio])])

    case "Batch":
        let ops = value["ops"]?.array ?? []
        var trial = state
        var inverses: [JSONValue] = []
        for inner in ops {
            guard case let .edit(n, v) = classifyOp(inner) else { continue }
            inverses.append(try computeInverse(inner, trial))
            try applyEdit(&trial, n, v)
        }
        let inverseOps = JSONValue.array(inverses.reversed())
        return .object(["Batch": .object(["ops": inverseOps])])

    default:
        throw OpenWorldFormatError.invalid("unknown edit \(name)")
    }
}

/// The subtree rooted at `id`, parents first, as SpawnEntity ops — deep
/// copies (the JSON is rebuilt, not shared), collected from the state
/// before the deletion takes the tree with it.
private func respawnSubtree(_ id: Int, in state: FoldState) throws -> [JSONValue] {
    let byId = Dictionary(uniqueKeysWithValues: state.entities.map { ($0.id, $0) })
    guard let root = byId[id] else {
        throw OpenWorldFormatError.invalid("no entity \(id)")
    }
    var ops: [JSONValue] = [.object(["SpawnEntity": .object(["entity": root.json])])]
    var frontier = [id]
    var seen: Set<Int> = [id]
    while !frontier.isEmpty {
        var next: [Int] = []
        for current in frontier {
            for entity in state.entities
            where entity.parent == current && !seen.contains(entity.id) {
                seen.insert(entity.id)
                next.append(entity.id)
                ops.append(.object(["SpawnEntity": .object(["entity": byId[entity.id]!.json])]))
            }
        }
        frontier = next
    }
    return ops
}
