// Merging a branch: the merge authority's rewrite (spec/session.md,
// "Entry identity, forks and branches").
//
// Ids the branch spawns that main holds concurrently collide; each gets
// a fresh id and every reference to it inside the branch's entries is
// rewritten. Name collisions are out of scope here — names are unique
// per world, so the caller pre-renames before merging; this rewrite
// moves ids only. History ops carry no entity ids and pass through
// untouched, as do string refs, which immediate name binding
// (spec/world.md) resolves when the merged log folds.

import Foundation

/// Remap a branch's colliding entity ids and rewrite the branch to use
/// the remapped ids.
///
/// Fresh ids come from past the state's maximum (or 1), skipping the
/// ids main holds, every id the branch spawns — colliding or not, a
/// non-colliding branch id keeps its id — and each remap already handed
/// out; nothing past `MAX_ENTITY_ID` is ever handed out.
///
/// Rewritten, per edit op (batches recursed): `SpawnEntity`'s entity
/// `id` and `parent`, `ModifyEntity`'s `id` and `patch.parent`,
/// `DeleteEntity`'s `id`, and numeric behavior refs (`Orbit.center`,
/// `LookAt.target`) pointing at a remapped id. Each rewritten entry is
/// rebuilt through `LogEntry(revision:author:timestampMs:ops:id:parent:)`
/// so its `classified` recomputes.
///
/// - Throws: `OpenWorldFormatError.invalid` when no id below the
///   ceiling is free for a colliding id.
public func mergeBranch(
    _ state: FoldState,
    _ entries: [LogEntry]
) throws -> (entries: [LogEntry], remapped: [Int: Int]) {
    // What main holds, and everything the branch spawns.
    let mainIds = Set(state.entities.map(\.id))
    var branchSpawned: Set<Int> = []
    for entry in entries {
        for op in entry.ops {
            collectSpawned(op, into: &branchSpawned)
        }
    }

    // Fresh ids for the collisions.
    let collisions = branchSpawned.intersection(mainIds).sorted()
    var nextId = (state.entities.map(\.id).max() ?? 0) + 1
    var taken = mainIds.union(branchSpawned)
    var remapped: [Int: Int] = [:]
    for colliding in collisions {
        var fresh = nextId
        while taken.contains(fresh) { fresh += 1 }
        guard fresh <= MAX_ENTITY_ID else {
            throw OpenWorldFormatError.invalid(
                "no free entity id below the id ceiling \(MAX_ENTITY_ID) (2^53-1)")
        }
        remapped[colliding] = fresh
        taken.insert(fresh)
        nextId = fresh + 1
    }

    // Rewrite — value types all the way down, so the rebuild is the copy.
    var rewritten: [LogEntry] = []
    for entry in entries {
        var changed = false
        var ops: [JSONValue] = []
        for op in entry.ops {
            let (rewrittenOp, opChanged) = rewriteOp(op, remapped: remapped)
            changed = changed || opChanged
            ops.append(rewrittenOp)
        }
        rewritten.append(changed
            ? LogEntry(
                revision: entry.revision,
                author: entry.author,
                timestampMs: entry.timestampMs,
                ops: ops,
                id: entry.id,
                parent: entry.parent,
                message: entry.message)
            : entry)
    }
    return (entries: rewritten, remapped: remapped)
}

/// The ids a SpawnEntity spawns, batches recursed.
private func collectSpawned(_ op: JSONValue, into ids: inout Set<Int>) {
    guard case let .edit(name, value) = classifyOp(op) else { return }
    switch name {
    case "SpawnEntity":
        if let id = value["entity"]?["id"]?.int { ids.insert(id) }
    case "Batch":
        for inner in value["ops"]?.array ?? [] {
            collectSpawned(inner, into: &ids)
        }
    default:
        break
    }
}

/// Rewrite one op's remapped ids. Returns the op (unchanged when
/// nothing it references was remapped) and whether it changed.
private func rewriteOp(_ op: JSONValue, remapped: [Int: Int]) -> (JSONValue, Bool) {
    guard case let .edit(name, value) = classifyOp(op) else {
        return (op, false)   // history ops carry no entity ids
    }
    switch name {
    case "SpawnEntity":
        guard var entity = value["entity"]?.object else { return (op, false) }
        var changed = false
        if let id = entity["id"]?.int, let fresh = remapped[id] {
            entity["id"] = .number(Double(fresh))
            changed = true
        }
        if let parent = entity["parent"]?.int, let fresh = remapped[parent] {
            entity["parent"] = .number(Double(fresh))
            changed = true
        }
        if let behaviors = entity["behaviors"]?.array {
            let (rewrittenBehaviors, behaviorsChanged) = rewriteBehaviors(behaviors, remapped: remapped)
            if behaviorsChanged {
                entity["behaviors"] = .array(rewrittenBehaviors)
                changed = true
            }
        }
        guard changed else { return (op, false) }
        return (.object(["SpawnEntity": .object(["entity": .object(entity)])]), true)

    case "ModifyEntity":
        guard var o = value.object else { return (op, false) }
        var changed = false
        if let id = o["id"]?.int, let fresh = remapped[id] {
            o["id"] = .number(Double(fresh))
            changed = true
        }
        if var patch = o["patch"]?.object {
            var patchChanged = false
            if let parent = patch["parent"]?.int, let fresh = remapped[parent] {
                patch["parent"] = .number(Double(fresh))
                patchChanged = true
            }
            if let behaviors = patch["behaviors"]?.array {
                let (rewrittenBehaviors, behaviorsChanged) = rewriteBehaviors(behaviors, remapped: remapped)
                if behaviorsChanged {
                    patch["behaviors"] = .array(rewrittenBehaviors)
                    patchChanged = true
                }
            }
            if patchChanged {
                o["patch"] = .object(patch)
                changed = true
            }
        }
        guard changed else { return (op, false) }
        return (.object(["ModifyEntity": .object(o)]), true)

    case "DeleteEntity":
        guard var o = value.object,
              let id = o["id"]?.int,
              let fresh = remapped[id]
        else { return (op, false) }
        o["id"] = .number(Double(fresh))
        return (.object(["DeleteEntity": .object(o)]), true)

    case "Batch":
        guard let inner = value["ops"]?.array else { return (op, false) }
        var changed = false
        var ops: [JSONValue] = []
        for sub in inner {
            let (rewrittenSub, subChanged) = rewriteOp(sub, remapped: remapped)
            changed = changed || subChanged
            ops.append(rewrittenSub)
        }
        guard changed else { return (op, false) }
        return (.object(["Batch": .object(["ops": .array(ops)])]), true)

    default:
        // The scene-wide setters and audio emitters carry no entity ids.
        return (op, false)
    }
}

/// Remap numeric behavior refs; string refs stay for name binding to
/// resolve at fold time.
private func rewriteBehaviors(
    _ behaviors: [JSONValue],
    remapped: [Int: Int]
) -> ([JSONValue], Bool) {
    var changed = false
    var out: [JSONValue] = []
    for behavior in behaviors {
        guard let b = behavior.object, b.count == 1, let (kind, params) = b.first,
              var p = params.object
        else {
            out.append(behavior)
            continue
        }
        let refKey: String
        switch kind {
        case "Orbit": refKey = "center"
        case "LookAt": refKey = "target"
        default: refKey = ""
        }
        if !refKey.isEmpty, let id = p[refKey]?.int, let fresh = remapped[id] {
            p[refKey] = .number(Double(fresh))
            changed = true
            out.append(.object([kind: .object(p)]))
        } else {
            out.append(behavior)
        }
    }
    return (out, changed)
}
