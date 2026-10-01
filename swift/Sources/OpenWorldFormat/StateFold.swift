// The typed state document (state.json) and its fold.
//
// State ops never touch entities — this pass is separate, and equally
// tolerant: keys nothing declares are carried, not refused. A save game
// is base + declaration + a player's log. Spec: spec/state.md.

import Foundation

/// A declared state field (`schema/state.schema.json`).
public struct StateField: Equatable, Sendable {
    /// One of int, float, bool, string, map, list, json.
    public var type: String
    public var initial: JSONValue?

    public init(json: JSONValue) {
        let o = json.object ?? [:]
        type = o["type"]?.string ?? "json"
        initial = o["initial"]
    }
}

/// The typed state document: `{format_version, fields}`.
public struct StateDocument: Equatable, Sendable {
    public var formatVersion: Int?
    public var fields: [String: StateField]

    public init(json: JSONValue) {
        let o = json.object ?? [:]
        formatVersion = o["format_version"]?.int
        fields = (o["fields"]?.object ?? [:]).mapValues(StateField.init(json:))
    }

    public init(fields: [String: StateField]) {
        self.formatVersion = 1
        self.fields = fields
    }
}

/// What `foldState` produced: the values at the last entry, and the
/// keys no declaration claimed — carried, and said so.
public struct StateFoldResult: Equatable, Sendable {
    public var values: [String: JSONValue]
    public var undeclared: [String]
}

/// Fold a session log's `state` ops over a state document: the values
/// at the last entry.
///
/// A declared field is set by its value, or reset to its initial by
/// null. A dotted key under a declared map field ("inventory.rope"
/// under the map "inventory") sets or removes that entry. A key
/// declared by no one is carried — null removes it — and named in
/// `undeclared`.
public func foldState(_ stateDoc: StateDocument?, _ entries: [LogEntry]) -> StateFoldResult {
    let fields = stateDoc?.fields ?? [:]
    var values: [String: JSONValue] = [:]
    for (key, field) in fields {
        values[key] = field.initial ?? .null
    }
    var undeclared: [String] = []

    for entry in entries {
        let classified = entry.classified.isEmpty && !entry.ops.isEmpty
            ? entry.ops.map(classifyOp)
            : entry.classified
        for op in classified {
            guard case let .state(delta) = op else { continue }
            for (key, value) in delta {
                if let field = fields[key] {
                    // A declared field: set it, or reset it to its initial.
                    values[key] = value == .null ? (field.initial ?? .null) : value
                    continue
                }
                // Maybe a subkey of a declared map field.
                if let dot = key.firstIndex(of: "."), dot > key.startIndex {
                    let base = String(key[key.startIndex..<dot])
                    let inner = String(key[key.index(after: dot)...])
                    if let field = fields[base], field.type == "map" {
                        var map = values[base]?.object ?? [:]
                        if value == .null { map.removeValue(forKey: inner) }
                        else { map[inner] = value }
                        values[base] = .object(map)
                        continue
                    }
                }
                // Declared by no one: carry it, and say so.
                if value == .null { values.removeValue(forKey: key) }
                else { values[key] = value }
                if !undeclared.contains(key) { undeclared.append(key) }
            }
        }
    }
    return StateFoldResult(values: values, undeclared: undeclared)
}
