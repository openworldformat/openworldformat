// The package helpers: snapshot filenames and compaction's
// package.json (spec/session.md, "Snapshots"; spec/package.md).

import XCTest
@testable import OpenWorldFormat

final class PackageTests: XCTestCase {
    func testSnapshotFilenamesCarryTheEntryIdOrTheRevision() {
        XCTAssertEqual(snapshotFilename(entryId: "e3", revision: 9), "snapshots/entry-e3.json")
        XCTAssertEqual(snapshotFilename(entryId: nil, revision: 42), "snapshots/rev-42.json")
        // A hash id sanitizes to something a filesystem holds.
        XCTAssertEqual(
            snapshotFilename(entryId: "sha256:4b754af6", revision: 1),
            "snapshots/entry-sha256_4b754af6.json")
        // Everything outside [A-Za-z0-9._-] folds to _.
        XCTAssertEqual(snapshotFilename(entryId: "a b/c:é", revision: 1), "snapshots/entry-a_b_c__.json")
        XCTAssertEqual(snapshotFilename(entryId: "v1.2-x_y", revision: 1), "snapshots/entry-v1.2-x_y.json")
    }

    func testCompactionSetsTheBaseRevisionAtTheHeadAndMovesNothing() throws {
        let packageJson = try JSONValue(parsing:
            #"{"format_version": 1, "name": "t", "base_revision": 0, "head_revision": 3, "refs": {"main": "e3"}}"#)
        let compacted = compactPackage(packageJson, headRevision: 3)
        var expected = packageJson.object ?? [:]
        expected["base_revision"] = .number(3)
        XCTAssertEqual(compacted, .object(expected))

        // A non-object input still yields the one field the routine owns.
        XCTAssertEqual(compactPackage(.null, headRevision: 2), .object(["base_revision": .number(2)]))
    }
}
