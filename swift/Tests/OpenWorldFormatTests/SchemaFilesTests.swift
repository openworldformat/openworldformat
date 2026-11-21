// The SwiftPM package ships the repository's normative schemas
// (Sources/OpenWorldFormat/Resources/schemas/, Package.swift's
// `.copy` rule) so a consumer validates against its installed
// dependency — schemaURL("world.schema.json") — instead of a checkout
// of this repository. The copies are byte-identical snapshots of
// schema/, never hand-edited: when the schema changes, copy the whole
// tree again (world, state, extensions/). These tests fail when a
// copy drifts and when a new schema appears in schema/ that the
// package does not ship. Mirrors js/test/schema-files.test.mjs.

import XCTest
@testable import OpenWorldFormat

/// Every .schema.json under a directory, as names relative to it
/// ("world.schema.json", "extensions/ext-h3.schema.json"), sorted.
private func schemaFiles(_ directory: URL) throws -> [String] {
    let fm = FileManager.default
    guard let entries = try? fm.contentsOfDirectory(
        at: directory, includingPropertiesForKeys: [.isDirectoryKey]
    ) else { return [] }
    var files: [String] = []
    for entry in entries.sorted(by: { $0.lastPathComponent < $1.lastPathComponent }) {
        var isDirectory = ObjCBool(false)
        if fm.fileExists(atPath: entry.path, isDirectory: &isDirectory), isDirectory.boolValue {
            files += try schemaFiles(entry).map { "\(entry.lastPathComponent)/\($0)" }
        } else if entry.lastPathComponent.hasSuffix(".schema.json") {
            files.append(entry.lastPathComponent)
        }
    }
    return files
}

final class SchemaFilesTests: XCTestCase {
    private var schemaDir: URL { root.appendingPathComponent("schema") }

    /// Where the package's own copies live: beside one that resolves.
    private func bundleDir() throws -> URL {
        try XCTUnwrap(
            schemaURL("world.schema.json")?.deletingLastPathComponent(),
            "world.schema.json does not resolve — the package ships no schemas"
        )
    }

    func testTheShippedSchemasAreTheRepositorysByteForByte() throws {
        XCTAssertEqual(
            try schemaFiles(bundleDir()), try schemaFiles(schemaDir),
            "the package's schemas/ and schema/ hold different files — refresh the copies"
        )
        for name in try schemaFiles(schemaDir) {
            XCTAssertEqual(
                try read(bundleDir().appendingPathComponent(name)),
                try read(schemaDir.appendingPathComponent(name)),
                "schema/\(name) drifted from schema/ — copy it again, never hand-edit it"
            )
        }
    }

    func testEverySchemaResolvesThroughTheAccessor() throws {
        for name in try schemaFiles(schemaDir) {
            let url = try XCTUnwrap(schemaURL(name), "\(name) does not resolve through schemaURL")
            let schema = try JSONSerialization.jsonObject(with: Data(read(url).utf8)) as? [String: Any]
            let draft = try XCTUnwrap(schema?["$schema"] as? String, "\(name) is a JSON Schema")
            XCTAssertTrue(draft.contains("json-schema.org"), name)
        }
        XCTAssertNil(schemaURL("no-such.schema.json"), "a name the package doesn't ship")
    }
}
