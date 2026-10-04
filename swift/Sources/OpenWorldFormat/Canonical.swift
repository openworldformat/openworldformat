// Canonical JSON, the canonical text, and entry identity.
//
// A content hash is only as good as its bytes agreeing across
// languages, so the format pins one serialization for hashing
// (spec/session.md, "Entry identity, forks and branches"): no
// whitespace, object keys sorted by code point, arrays in order,
// standard JSON string escapes, and integral numbers printed with no
// decimal part. That last rule is why JSONEncoder's `sortedKeys` alone
// can't serve: it writes "1.0" for an integral double, and the hash
// must not depend on which side of the decimal a language keeps.
//
// The caveat, stated plainly: cross-language hash equality holds for
// integer-valued JSON. A non-integral double's shortest textual form is
// not pinned by the format, so hashes over floats may differ between
// references — hash integer-valued content, or accept the divergence.

import CryptoKit
import Foundation

/// Serialize a JSON value canonically: no whitespace, object keys
/// sorted recursively by code point, arrays in order, standard JSON
/// string escaping, and integral numbers with no decimal part ("7",
/// never "7.0").
public func canonicalJson(_ value: JSONValue) -> String {
    var out = ""
    write(value, into: &out)
    return out
}

func write(_ value: JSONValue, into out: inout String) {
    switch value {
    case .null:
        out += "null"
    case .bool(let b):
        out += b ? "true" : "false"
    case .number(let n):
        writeNumber(n, into: &out)
    case .string(let s):
        writeString(s, into: &out)
    case .array(let a):
        out += "["
        for (i, element) in a.enumerated() {
            if i > 0 { out += "," }
            write(element, into: &out)
        }
        out += "]"
    case .object(let o):
        out += "{"
        for (i, key) in o.keys.sorted().enumerated() {
            if i > 0 { out += "," }
            writeString(key, into: &out)
            out += ":"
            write(o[key]!, into: &out)
        }
        out += "}"
    }
}

/// Integral doubles print as integers; the rest take Swift's shortest
/// round-trip form. A non-finite double has no JSON form — null is what
/// a JavaScript JSON.stringify would have written, and this serializer
/// follows.
func writeNumber(_ n: Double, into out: inout String) {
    guard n.isFinite else {
        out += "null"
        return
    }
    if n == n.rounded() {
        out += String(format: "%.0f", n)
    } else {
        out += "\(n)"
    }
}

/// The two quotes, the standard escapes, and everything else as UTF-8 —
/// non-ASCII passes through unescaped, as the other references write it.
func writeString(_ s: String, into out: inout String) {
    out += "\""
    for scalar in s.unicodeScalars {
        switch scalar {
        case "\"": out += "\\\""
        case "\\": out += "\\\\"
        case "\u{08}": out += "\\b"
        case "\u{0C}": out += "\\f"
        case "\n": out += "\\n"
        case "\r": out += "\\r"
        case "\t": out += "\\t"
        default:
            if scalar.value < 0x20 {
                out += String(format: "\\u%04x", scalar.value)
            } else {
                out.unicodeScalars.append(scalar)
            }
        }
    }
    out += "\""
}

// MARK: - The canonical text of a manifest

/// A manifest as its canonical text — what an authority writes to
/// `manifest.json`, so that the same world is always the same bytes:
/// small diffs, ordinary git merges, and a `world_sha256` that changes
/// only when the world does (spec/package.md, "Canonical text").
///
/// Members sorted by code point, `null` members left out, entities in
/// ascending id order, two-space indentation, arrays of plain values on
/// one line, numbers in their shortest form, a trailing newline — the
/// same bytes the Rust and JS references' writers put down.
public func manifestText(_ manifest: WorldManifest) -> String {
    var world = manifest.json
    if case var .object(o) = world, case let .array(entities)? = o["entities"] {
        o["entities"] = .array(entities.sorted { ($0["id"]?.int ?? 0) < ($1["id"]?.int ?? 0) })
        world = .object(o)
    }
    return canonicalPretty(world, 0) + "\n"
}

/// The pretty canonical form: scalars compact, objects one member per
/// line with sorted non-null keys, arrays one element per line unless
/// every element is a plain value.
private func canonicalPretty(_ value: JSONValue, _ depth: Int) -> String {
    let pad = String(repeating: "  ", count: depth)
    let inner = String(repeating: "  ", count: depth + 1)
    func scalar(_ value: JSONValue) -> String {
        var out = ""
        write(value, into: &out)
        return out
    }
    func isPlain(_ value: JSONValue) -> Bool {
        switch value {
        case .null, .bool, .number, .string: return true
        case .array, .object: return false
        }
    }
    switch value {
    case .null, .bool, .number, .string:
        return scalar(value)
    case .array(let a):
        if a.isEmpty { return "[]" }
        if a.allSatisfy(isPlain) {
            return "[" + a.map(scalar).joined(separator: ", ") + "]"
        }
        return "[\n" + a.map { inner + canonicalPretty($0, depth + 1) }
            .joined(separator: ",\n") + "\n" + pad + "]"
    case .object(let o):
        let keys = o.keys.filter { o[$0] != .null }.sorted()
        if keys.isEmpty { return "{}" }
        return "{\n" + keys.map { key -> String in
            var name = ""
            writeString(key, into: &name)
            return inner + name + ": " + canonicalPretty(o[key]!, depth + 1)
        }.joined(separator: ",\n") + "\n" + pad + "}"
    }
}

// MARK: - Entry identity

/// An entry as a JSON object: revision, author, timestamp_ms, ops, id,
/// parent — the keys the entry carries, the absent ones omitted. With
/// `includeId: false` this is the hashing form: an entry's identity is
/// its content plus its parent, never its name for itself.
func entryObject(_ entry: LogEntry, includeId: Bool) -> JSONValue {
    var o: [String: JSONValue] = [:]
    o["revision"] = .number(Double(entry.revision))
    if let author = entry.author { o["author"] = author }
    if let timestampMs = entry.timestampMs { o["timestamp_ms"] = .number(timestampMs) }
    o["ops"] = .array(entry.ops)
    if includeId, let id = entry.id { o["id"] = .string(id) }
    if let parent = entry.parent { o["parent"] = .string(parent) }
    if let message = entry.message { o["message"] = .string(message) }
    return .object(o)
}

/// An entry's content-hash identity: the entry canonicalized WITHOUT
/// its `id` (its `parent` included), SHA-256 over the UTF-8, hex-prefixed.
/// Two forks that compute ids this way agree, which is the point —
/// the same content on both sides of a fork is the same entry.
public func computeEntryId(_ entry: LogEntry) -> String {
    let canonical = canonicalJson(entryObject(entry, includeId: false))
    let digest = SHA256.hash(data: Data(canonical.utf8))
    return "sha256:" + digest.map { String(format: "%02x", $0) }.joined()
}
