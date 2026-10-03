// Canonical JSON and entry identity.
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

private func write(_ value: JSONValue, into out: inout String) {
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
private func writeNumber(_ n: Double, into out: inout String) {
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
private func writeString(_ s: String, into out: inout String) {
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
