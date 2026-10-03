// Canonical JSON and entry identity — the golden vectors every
// reference agrees on, byte for byte, plus the serializer's own rules.

import XCTest
@testable import OpenWorldFormat

final class CanonicalTests: XCTestCase {
    /// The entry every language hashes: the task's shared vector.
    static let goldenEntry = """
    {"revision": 7, "timestamp_ms": 1790000000123, "author": {"peer": 3, "name": "maya"}, \
    "ops": [{"SpawnEntity": {"entity": {"id": 1, "name": "beacon"}}}], "parent": "e6"}
    """

    /// The same entry, canonicalized.
    static let goldenCanonical = """
    {"author":{"name":"maya","peer":3},\
    "ops":[{"SpawnEntity":{"entity":{"id":1,"name":"beacon"}}}],\
    "parent":"e6","revision":7,"timestamp_ms":1790000000123}
    """

    func testTheGoldenVectorCanonicalizesByteForByte() throws {
        let entry = try parseLogLine(Self.goldenEntry)
        XCTAssertEqual(canonicalJson(entryObject(entry, includeId: true)), Self.goldenCanonical)
        XCTAssertEqual(canonicalJson(entryObject(entry, includeId: false)), Self.goldenCanonical)
    }

    func testTheGoldenEntryIdHash() throws {
        let entry = try parseLogLine(Self.goldenEntry)
        XCTAssertEqual(
            computeEntryId(entry),
            "sha256:4b754af615abd0b36a11f6bec2753eedb7be6492080d8ed8ba6e50464dfaffa2")
    }

    func testTheIdIsExcludedFromTheHashTheParentIsNot() throws {
        let base = try parseLogLine(Self.goldenEntry)
        let withId = LogEntry(
            revision: base.revision,
            author: base.author,
            timestampMs: base.timestampMs,
            ops: base.ops,
            id: "e7",
            parent: base.parent)
        XCTAssertEqual(computeEntryId(withId), computeEntryId(base))
        // The id rides in the canonical form — just not in the hash input.
        XCTAssertTrue(canonicalJson(entryObject(withId, includeId: true)).contains("\"id\":\"e7\""))

        let reparented = LogEntry(
            revision: base.revision,
            author: base.author,
            timestampMs: base.timestampMs,
            ops: base.ops,
            id: "e7",
            parent: "e0")
        XCTAssertNotEqual(computeEntryId(reparented), computeEntryId(base))
        XCTAssertTrue(computeEntryId(reparented).hasPrefix("sha256:"))
    }

    func testKeysSortRecursivelyArraysKeepTheirOrder() throws {
        let value = try JSONValue(parsing: #"{"z": 1, "a": {"d": false, "b": [2, 1]}, "n": null}"#)
        XCTAssertEqual(canonicalJson(value), #"{"a":{"b":[2,1],"d":false},"n":null,"z":1}"#)
    }

    func testIntegralNumbersCarryNoDecimalPart() {
        XCTAssertEqual(canonicalJson(.number(7)), "7")
        XCTAssertEqual(canonicalJson(.number(0)), "0")
        XCTAssertEqual(canonicalJson(.number(-42)), "-42")
        XCTAssertEqual(canonicalJson(.number(1790000000123)), "1790000000123")
        XCTAssertEqual(canonicalJson(.number(1e21)), "1000000000000000000000")
        // Non-integral doubles keep their shortest form — the documented
        // cross-language caveat lives here.
        XCTAssertEqual(canonicalJson(.number(0.5)), "0.5")
        XCTAssertEqual(canonicalJson(.number(-0.25)), "-0.25")
    }

    func testStringsUseTheStandardEscapes() {
        XCTAssertEqual(canonicalJson(.string("a\"b\\c\nd")), "\"a\\\"b\\\\c\\nd\"")
        XCTAssertEqual(canonicalJson(.string("\u{08}\u{0C}\r\t")), "\"\\b\\f\\r\\t\"")
        XCTAssertEqual(canonicalJson(.string("x\u{01}y")), "\"x\\u0001y\"")
        // Non-ASCII passes through, as the other references write it.
        XCTAssertEqual(canonicalJson(.string("héllo")), "\"héllo\"")
    }
}
