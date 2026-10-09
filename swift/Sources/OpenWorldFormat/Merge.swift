// Merging a branch: the merge authority's rewrite (spec/session.md,
// "Entry identity, forks and branches").
//
// Ids the branch spawns that main holds concurrently collide; each gets
// a fresh id from main's effective next_entity_id — the fold's monotonic
// floor, which keeps spent ids spent — and every reference to one inside
// the branch's entries is rewritten. Name collisions take a suffix: a
// branch spawn whose name is taken — by main, or by an earlier spawn in
// the same merge — becomes `<name>-<n>`, n from 2 up, first unused.
// History ops carry no entity ids and pass through untouched, as do
// string refs, which immediate name binding (spec/world.md) resolves
// when the merged log folds.

import Foundation

/// Remap a branch's colliding entity ids and rewrite the branch to use
/// the remapped ids.
///
/// Colliding ids are reallocated in ascending order. Fresh ids start at
/// the state's effective `next_entity_id` — the larger of the declared
/// value and one past every id main ever held, deleted ones included —
/// and count up, skipping every id the branch spawns (a non-colliding
/// branch id keeps its id); nothing past `MAX_ENTITY_ID` is ever handed
/// out.
///
/// Rewritten, per edit op (batches recursed): `SpawnEntity`'s entity
/// `id` and `parent`, `ModifyEntity`'s `id` and `patch.parent`,
/// `DeleteEntity`'s `id`, `ModifyWorld`'s `patch.avatar.model_entity`
/// and the numeric elements of `patch.creations[].entities[]`, and the
/// numeric behavior refs (`Orbit.center`, `LookAt.target`). Then the
/// name rule: a spawn whose name is taken — by main, or by an earlier
/// spawn in the same merge — is renamed `<name>-<n>`, `n` from 2 up,
/// first unused; only the `SpawnEntity` changes. A rewritten entry is
/// rebuilt through `LogEntry(revision:author:timestampMs:ops:id:parent:message:)`
/// so its `classified` recomputes — its id, parent, author and message
/// survive: an entry's identity survives the merge.
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

    // Fresh ids for the collisions, reallocated in ascending order: from
    // the fold's floor — an id main deleted stays spent — skipping every
    // id the branch spawns, and never past the ceiling.
    var remapped: [Int: Int] = [:]
    var nextId = state.scene["next_entity_id"]?.int ?? 1
    for colliding in branchSpawned.intersection(mainIds).sorted() {
        while branchSpawned.contains(nextId) { nextId += 1 }
        guard nextId <= MAX_ENTITY_ID else {
            throw OpenWorldFormatError.invalid(
                "no free entity id below the id ceiling \(MAX_ENTITY_ID) (2^53-1)")
        }
        remapped[colliding] = nextId
        nextId += 1
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
        rewritten.append(changed ? withOps(entry, ops: ops) : entry)
    }
    renameCollidingSpawns(state, &rewritten)
    return (entries: rewritten, remapped: remapped)
}

/// An entry with new ops: everything else — id, parent, author, message —
/// survives (an entry's identity survives the merge), and `classified`
/// recomputes from the new ops.
private func withOps(_ entry: LogEntry, ops: [JSONValue]) -> LogEntry {
    LogEntry(
        revision: entry.revision,
        author: entry.author,
        timestampMs: entry.timestampMs,
        ops: ops,
        id: entry.id,
        parent: entry.parent,
        message: entry.message)
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

/// The spec's name rule: a spawned name that is taken — by main, or by
/// an earlier spawn in the same merge — becomes `<name>-<n>`, `n` from 2
/// up, first unused. Only the `SpawnEntity` changes: references are ids
/// by then, so a rename breaks nothing in the log. In place on the
/// rewritten entries.
private func renameCollidingSpawns(_ state: FoldState, _ entries: inout [LogEntry]) {
    var taken = Set(state.entities.map(\.name))
    for index in entries.indices {
        var ops = entries[index].ops
        var changed = false
        renameCollidingOps(&ops, taken: &taken, changed: &changed)
        if changed {
            entries[index] = withOps(entries[index], ops: ops)
        }
    }
}

/// The name walk, batches recursed. `taken` grows with every spawn the
/// merge commits; main's names are seeded by the caller.
private func renameCollidingOps(_ ops: inout [JSONValue], taken: inout Set<String>, changed: inout Bool) {
    for index in ops.indices {
        guard case let .edit(name, value) = classifyOp(ops[index]) else { continue }
        switch name {
        case "Batch":
            guard var inner = value["ops"]?.array else { continue }
            var innerChanged = false
            renameCollidingOps(&inner, taken: &taken, changed: &innerChanged)
            if innerChanged {
                var o = value.object ?? [:]
                o["ops"] = .array(inner)
                ops[index] = .object(["Batch": .object(o)])
                changed = true
            }
        case "SpawnEntity":
            guard var entity = value["entity"]?.object,
                  let entityName = entity["name"]?.string
            else { continue }
            if taken.contains(entityName) {
                var n = 2
                while taken.contains("\(entityName)-\(n)") { n += 1 }
                entity["name"] = .string("\(entityName)-\(n)")
                taken.insert("\(entityName)-\(n)")
                var o = value.object ?? [:]
                o["entity"] = .object(entity)
                ops[index] = .object(["SpawnEntity": .object(o)])
                changed = true
            } else {
                taken.insert(entityName)
            }
        default:
            continue
        }
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

    case "ModifyWorld":
        // The world patch holds entity ids too: the avatar's model
        // entity, and every creation's entity list (numeric elements;
        // string elements are names, not ids, and ride).
        guard var o = value.object, var patch = o["patch"]?.object else { return (op, false) }
        var changed = false
        if var avatar = patch["avatar"]?.object,
           let modelEntity = avatar["model_entity"]?.int,
           let fresh = remapped[modelEntity] {
            avatar["model_entity"] = .number(Double(fresh))
            patch["avatar"] = .object(avatar)
            changed = true
        }
        if let creations = patch["creations"]?.array {
            var rewrittenCreations = creations
            var creationsChanged = false
            for index in rewrittenCreations.indices {
                guard var creation = rewrittenCreations[index].object,
                      let ids = creation["entities"]?.array
                else { continue }
                let rewrittenIds = ids.map { element -> JSONValue in
                    guard let id = element.int, let fresh = remapped[id] else { return element }
                    return .number(Double(fresh))
                }
                if rewrittenIds != ids {
                    creation["entities"] = .array(rewrittenIds)
                    rewrittenCreations[index] = .object(creation)
                    creationsChanged = true
                }
            }
            if creationsChanged {
                patch["creations"] = .array(rewrittenCreations)
                changed = true
            }
        }
        guard changed else { return (op, false) }
        o["patch"] = .object(patch)
        return (.object(["ModifyWorld": .object(o)]), true)

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
