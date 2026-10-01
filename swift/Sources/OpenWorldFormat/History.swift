// Branching histories. An entry's identity is its own `id` if present,
// else a synthesized `line-<n>`; its parent likewise, else the previous
// entry. A log with no ids is a chain in file order; a branch is just
// a different tip. Spec: spec/session.md.

import Foundation

/// An entry with identity resolved: an id and a parent, always.
public struct IdentifiedEntry: Equatable, Sendable {
    public let id: String
    public let parent: String?
    public let revision: Int
    public let timestampMs: Double?
    public let entry: LogEntry
}

/// The history of a log: entries with identity, and its shape.
public struct History: Equatable, Sendable {
    /// Entries in file order, identity resolved.
    public let ordered: [IdentifiedEntry]
    /// parent id → its children's ids, in file order.
    public let children: [String: [String]]
    /// The entries no other entry claims as parent — the branch ends.
    public let tips: [String]

    public func entry(id: String) -> IdentifiedEntry? {
        ordered.first { $0.id == id }
    }
}

/// Resolve identity for every entry, validating as it goes.
func withIdentity(_ entries: [LogEntry]) throws -> (ordered: [IdentifiedEntry], byId: [String: IdentifiedEntry]) {
    var byId: [String: IdentifiedEntry] = [:]
    var ordered: [IdentifiedEntry] = []
    var previous: String? = nil
    for (n, raw) in entries.enumerated() {
        let id = raw.id ?? "line-\(n)"
        if byId[id] != nil {
            throw OpenWorldFormatError.invalid("duplicate entry id '\(id)'")
        }
        let parent = raw.parent ?? previous
        if let parent, byId[parent] == nil {
            throw OpenWorldFormatError.invalid("entry '\(id)' names parent '\(parent)', which isn't in the log")
        }
        let entry = IdentifiedEntry(
            id: id, parent: parent, revision: raw.revision,
            timestampMs: raw.timestampMs, entry: raw)
        byId[id] = entry
        ordered.append(entry)
        previous = id
    }
    return (ordered, byId)
}

/// Build a log's history: entries with identity, children, and tips.
///
/// - Throws: `OpenWorldFormatError.invalid` on a duplicate id or a
///   parent that isn't in the log.
public func buildHistory(_ entries: [LogEntry]) throws -> History {
    let (ordered, _) = try withIdentity(entries)
    var children: [String: [String]] = Dictionary(uniqueKeysWithValues: ordered.map { ($0.id, []) })
    for entry in ordered {
        if let parent = entry.parent, children[parent] != nil {
            children[parent]?.append(entry.id)
        }
    }
    let tips = ordered.filter { (children[$0.id] ?? []).isEmpty }.map(\.id)
    return History(ordered: ordered, children: children, tips: tips)
}

/// Fold one path of the history: the document at `tip` (default: the
/// last entry in file order), reached by walking parent links to the
/// base and folding that chain. A branch is just a different tip.
///
/// - Throws: `OpenWorldFormatError.invalid` on an unknown tip, or the
///   first entry that no longer applies.
@discardableResult
public func foldPath(
    _ manifest: WorldManifest,
    _ entries: [LogEntry],
    tip: String? = nil
) throws -> FoldState {
    let (ordered, byId) = try withIdentity(entries)
    let target = tip ?? ordered.last?.id
    guard let target else {
        throw OpenWorldFormatError.invalid("no entry 'null' in this log")
    }
    guard byId[target] != nil else {
        throw OpenWorldFormatError.invalid("no entry '\(target)' in this log")
    }
    var chain: [LogEntry] = []
    var ids: [String] = []
    var cursor: String? = target
    while let id = cursor {
        let entry = byId[id]!
        chain.append(entry.entry)
        ids.append(entry.id)
        cursor = entry.parent
    }
    chain.reverse()
    ids.reverse()
    var state = try foldLog(manifest, chain)
    state.path = ids
    return state
}
