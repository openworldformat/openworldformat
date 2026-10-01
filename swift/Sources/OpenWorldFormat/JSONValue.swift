// A dynamic JSON tree — the passthrough half of the format.
//
// The fold treats components, extension fields and state values as
// opaque: they ride along untouched (must-ignore). JSONValue is that
// side of the format in Swift: everything the typed structs don't name,
// held exactly as the document held it, nulls included — a stored
// `.null` is "present and null", a missing key is "absent", and the
// patch semantics (absent = unchanged, null = clear, value = set) need
// exactly that distinction.

import Foundation

/// One JSON value: null, bool, number, string, array or object.
/// Numbers are doubles, as in JavaScript and the other references.
public enum JSONValue: Equatable, Sendable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    /// Decode from JSON text.
    public init(parsing text: String) throws {
        try self.init(decoding: Data(text.utf8))
    }

    /// Decode from JSON data.
    public init(decoding data: Data) throws {
        self = try JSONDecoder().decode(JSONValue.self, from: data)
    }

    /// Encode to JSON data.
    public func encoded() throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(self)
    }
}

extension JSONValue: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let b = try? container.decode(Bool.self) {
            self = .bool(b)
        } else if let n = try? container.decode(Double.self) {
            self = .number(n)
        } else if let s = try? container.decode(String.self) {
            self = .string(s)
        } else if let a = try? container.decode([JSONValue].self) {
            self = .array(a)
        } else {
            // A keyed container is the only shape left; a decode failure
            // here is the malformed case, and it should throw.
            self = .object(try container.decode([String: JSONValue].self))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .null: try container.encodeNil()
        case .bool(let b): try container.encode(b)
        case .number(let n): try container.encode(n)
        case .string(let s): try container.encode(s)
        case .array(let a): try container.encode(a)
        case .object(let o): try container.encode(o)
        }
    }
}

extension JSONValue {
    /// The object's keys and values, if this is an object.
    public var object: [String: JSONValue]? {
        if case let .object(o) = self { return o }
        return nil
    }

    /// The array's elements, if this is an array.
    public var array: [JSONValue]? {
        if case let .array(a) = self { return a }
        return nil
    }

    /// The string, if this is a string.
    public var string: String? {
        if case let .string(s) = self { return s }
        return nil
    }

    /// The double, if this is a number.
    public var double: Double? {
        if case let .number(n) = self { return n }
        return nil
    }

    /// The integer, if this is a number with no fractional part.
    public var int: Int? {
        if case let .number(n) = self {
            return Int(exactly: n)
        }
        return nil
    }

    /// The bool, if this is a bool.
    public var bool: Bool? {
        if case let .bool(b) = self { return b }
        return nil
    }

    /// Key lookup on objects. A stored `.null` is present — `nil` means
    /// the key isn't there, which is what patch semantics distinguish.
    public subscript(key: String) -> JSONValue? {
        object?[key]
    }

    /// The color this value names: rgb or rgba, alpha defaulting to 1.
    /// The format serializes colors as `[r, g, b]` or `[r, g, b, a]`,
    /// linear, 0–1.
    public var rgba: (r: Double, g: Double, b: Double, a: Double)? {
        guard let a = array, (3...4).contains(a.count) else { return nil }
        let c = a.map { $0.double ?? 0 }
        return (c[0], c[1], c[2], c.count == 4 ? c[3] : 1)
    }
}
