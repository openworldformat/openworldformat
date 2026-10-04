// The conformance worlds: every one parses, and the entity counts are
// pinned — the same worlds the js, python and rust jobs fold, so a
// schema change that moves a count breaks all four references at once.

import XCTest
@testable import OpenWorldFormat

final class ConformanceTests: XCTestCase {
    /// (world, entity count) — pinned from conformance/*.json.
    static let counts: [String: Int] = [
        "behaviors": 13,
        "hierarchy_rotations": 9,
        "hierarchy_tours": 12,
        "instances": 7,
        "lights": 8,
        "materials": 21,
        "physics": 5,
        "shapes": 13,
        "soundtrack": 10,
        "textures": 8,
        "triggers": 10,
    ]

    /// The fold's identity rule, exercised on every world: ids and
    /// names are unique, parents exist.
    func testEveryConformanceWorldParsesWithPinnedCountsAndCleanIdentity() throws {
        let dir = root.appendingPathComponent("conformance")
        for (world, count) in Self.counts {
            let text = try read(dir.appendingPathComponent("\(world).json"))
            let manifest = try parseManifest(text)
            XCTAssertEqual(manifest.entities.count, count, "\(world).json entity count drifted")

            let ids = manifest.entities.map(\.id)
            XCTAssertEqual(Set(ids).count, ids.count, "\(world).json has duplicate entity ids")
            let names = manifest.entities.map(\.name)
            XCTAssertEqual(Set(names).count, names.count, "\(world).json has duplicate entity names")

            let known = Set(ids)
            for entity in manifest.entities {
                if let parent = entity.parent {
                    XCTAssertTrue(known.contains(parent), "\(world).json: \(entity.name)'s parent \(parent) is missing")
                }
            }
        }
    }

    /// Every world's shapes decode into the typed vocabulary: the
    /// viewer profile must be able to draw what the suite declares.
    func testEveryDeclaredShapeDecodesIntoTheTypedVocabulary() throws {
        let dir = root.appendingPathComponent("conformance")
        var shapes = 0
        for world in Self.counts.keys.sorted() {
            let manifest = try parseManifest(try read(dir.appendingPathComponent("\(world).json")))
            for entity in manifest.entities where entity.fields["shape"] != nil {
                let shape = try XCTUnwrap(entity.shape, "\(world).json: \(entity.name) declares a shape this package can't read")
                shapes += 1
            }
        }
        XCTAssertGreaterThan(shapes, 20)  // the suite is a shape museum; sanity that we read them
    }

    /// The package loader reads a bare world (manifest only: no log,
    /// no state) as a valid package with an empty history — the fold
    /// over no entries is the base.
    func testABareWorldLoadsAsAPackageWithNoHistory() throws {
        let tmp = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
            .appendingPathComponent("owf-bare-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: tmp) }
        try FileManager.default.copyItem(
            at: root.appendingPathComponent("conformance/shapes.json"),
            to: tmp.appendingPathComponent("manifest.json"))

        let package = try WorldPackage(directory: tmp)
        XCTAssertEqual(package.entries, [])
        XCTAssertNil(package.stateDocument)
        let state = try package.folded()
        XCTAssertEqual(state.appliedEdits, 0)
        XCTAssertEqual(state.entities.count, 13)  // shapes, pinned above
        XCTAssertEqual(try package.history().tips, [])  // nothing to branch
    }
}
