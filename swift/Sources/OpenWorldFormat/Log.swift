// The session log (ops.jsonl): parsing and op classification.
//
// Ops are recognized by shape, edits first — the compatibility rule,
// executable: a log written before the history kinds existed parses as
// edits, and an edit serializes today exactly as it always did. Spec:
// spec/session.md.

import Foundation

/// The edit kinds — the only ops that change the document.
public let EDIT_KEYS: Set<String> = [
    "SpawnEntity",
    "DeleteEntity",
    "ModifyEntity",
    "SetEnvironment",
    "SetCamera",
    "SetAmbience",
    "SpawnAudioEmitter",
    "RemoveAudioEmitter",
    "ModifyWorld",
    "Batch",
]

/// One op, recognized by shape.
public enum ClassifiedOp: Equatable, Sendable {
    /// A document edit: one of `EDIT_KEYS`, carrying its value.
    case edit(name: String, value: JSONValue)
    /// History kinds — they record, and fold to nothing for the document.
    case tool(JSONValue)
    case input(JSONValue)
    case state([String: JSONValue])
    case clock([String: JSONValue])
    case merge([String: JSONValue])
    /// An extension op (`ext-*`, single key, object value) — its own
    /// kind, and like every history kind it folds to nothing here.
    case extensionOp(name: String, value: JSONValue)
    /// Recognized by no rule — carried, ignored.
    case unknown
}

/// Classify one op by its shape, edits first.
public func classifyOp(_ op: JSONValue) -> ClassifiedOp {
    guard case let .object(o) = op else { return .unknown }
    // An edit: exactly one key, and it's an edit kind.
    if o.count == 1, let (key, value) = o.first, EDIT_KEYS.contains(key) {
        return .edit(name: key, value: value)
    }
    if o["tool"]?.string != nil && o["args"] != nil {
        return .tool(op)
    }
    if let input = o["input"]?.object, input["actor"]?.string != nil {
        return .input(.object(input))
    }
    if let s = o["state"]?.object { return .state(s) }
    if let c = o["clock"]?.object { return .clock(c) }
    if let m = o["merge"]?.object { return .merge(m) }
    if o.count == 1, let (key, value) = o.first, isExtensionKey(key),
       value.object != nil {
        return .extensionOp(name: key, value: value)
    }
    return .unknown
}

/// `ext-` followed by lowercase letters, digits and dashes (`ext-physics`).
func isExtensionKey(_ key: String) -> Bool {
    guard key.hasPrefix("ext-"), key.count > 4 else { return false }
    return key.dropFirst(4).allSatisfy { c in
        (c >= "a" && c <= "z") || (c >= "0" && c <= "9") || c == "-"
    }
}

/// The shape collision rule (spec/session.md, "Compatibility"): edit
/// kinds are PascalCase, history kinds are lowercase — so no future
/// kind of either side can ever be mistaken for one of the other. A
/// kind passes by being an edit key written in PascalCase, or one of
/// the five lowercase history kinds.
public func opKindShapeOk(_ kind: String) -> Bool {
    if EDIT_KEYS.contains(kind) {
        return kind.first?.isUppercase == true
    }
    return ["tool", "input", "state", "clock", "merge"].contains(kind)
}

/// One parsed ops.jsonl line: an entry, its ops classified on parse.
public struct LogEntry: Equatable, Sendable {
    public var revision: Int
    public var author: JSONValue?
    public var timestampMs: Double?
    public var ops: [JSONValue]
    public var id: String?
    public var parent: String?
    /// What the author says the batch is for — a commit message. It is
    /// part of the entry's identity and folds to nothing.
    public var message: String?
    public var classified: [ClassifiedOp]

    public init(
        revision: Int,
        author: JSONValue? = nil,
        timestampMs: Double? = nil,
        ops: [JSONValue],
        id: String? = nil,
        parent: String? = nil,
        message: String? = nil
    ) {
        self.message = message
        self.revision = revision
        self.author = author
        self.timestampMs = timestampMs
        self.ops = ops
        self.id = id
        self.parent = parent
        self.classified = ops.map(classifyOp)
    }
}

/// Parse one log line into an entry with classified ops.
///
/// Unreadable lines are the writer's crash, not the reader's — the
/// caller decides whether to skip (the spec says skip the last one,
/// count the rest).
///
/// Strict mode (`strict: true`) refuses what the default carries: an
/// op no shape rule recognizes, and an extension op whose name the
/// registry hasn't registered. Must-ignore stays the default.
///
/// - Throws: `OpenWorldFormatError.parse` when the line isn't JSON, the
///   entry has no numeric revision and ops array — or, when strict, an
///   op is no known kind or names an unregistered extension.
public func parseLogLine(_ line: String, strict: Bool = false) throws -> LogEntry {
    let json = try JSONValue(parsing: line)
    guard case let .object(o) = json else {
        throw OpenWorldFormatError.parse("log entry must be a JSON object")
    }
    guard let revision = o["revision"]?.int, let ops = o["ops"]?.array else {
        throw OpenWorldFormatError.parse("log entry needs a revision and an ops array")
    }
    if strict {
        for op in ops {
            switch classifyOp(op) {
            case .unknown:
                let named = op.object?.keys.sorted().first.map { " '\($0)'" } ?? ""
                throw OpenWorldFormatError.parse("strict: unrecognized op\(named)")
            case .extensionOp(let name, _):
                guard REGISTERED_EXTENSIONS.contains(name) else {
                    throw OpenWorldFormatError.parse(
                        "strict: '\(name)' is not in the extension registry "
                            + "(spec/extensions/registry.json)")
                }
            default:
                break
            }
        }
    }
    let id = o["id"]?.string
    // A parent of the wrong shape is the same as absent: `id`-bearing
    // logs carry parent as a string; anything else falls to the chain.
    let parent = o["parent"]?.string
    return LogEntry(
        revision: revision,
        author: o["author"],
        timestampMs: o["timestamp_ms"]?.double,
        ops: ops,
        id: id,
        parent: parent,
        message: o["message"]?.string
    )
}

/// An entry's edits, in order — the ops that change the document.
public func editOps(_ entry: LogEntry) -> [ClassifiedOp] {
    let classified = entry.classified.isEmpty && !entry.ops.isEmpty
        ? entry.ops.map(classifyOp)
        : entry.classified
    return classified.filter {
        if case .edit = $0 { return true }
        return false
    }
}
