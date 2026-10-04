// The live-authoring rules this reference reads (spec/session.md "The fold
// is total", spec/package.md "Head-first") — mirrors
// js/test/authoring.test.mjs and the Rust conformance suite: every world
// survives an empty fold, every example's manifest is the fold to main,
// and ModifyWorld reaches every scene field and undoes.

import CryptoKit
import XCTest
@testable import OpenWorldFormat

/// A world compared as a world: entities by id, empty and null fields as
/// absent, next_entity_id by its effective value. JSONValue numbers are
/// doubles, so 2 and 2.0 already compare equal.
func normalized(_ manifest: WorldManifest) -> JSONValue {
    var o = manifest.json.object ?? [:]
    for (key, value) in o where value == .null || value.array?.isEmpty == true {
        o.removeValue(forKey: key)
    }
    let past = (manifest.entities.map(\.id).max() ?? 0) + 1
    let declared = o["next_entity_id"]?.int ?? 1
    o["next_entity_id"] = .number(Double(max(declared, past)))
    o["entities"] = .array(manifest.entities.sorted { $0.id < $1.id }.map(\.json))
    return .object(o)
}

let examples: [URL] = {
    let dir = root.appendingPathComponent("examples")
    let names = (try? FileManager.default.contentsOfDirectory(atPath: dir.path)) ?? []
    return names.sorted()
        .map { dir.appendingPathComponent($0) }
        .filter { FileManager.default.fileExists(atPath: $0.appendingPathComponent("manifest.json").path) }
}()

final class HeadFirstTests: XCTestCase {
    func testTheFoldIsTotalEveryWorldSurvivesAnEmptyFold() throws {
        let conformance = root.appendingPathComponent("conformance")
        var worlds: [(String, URL)] = try FileManager.default.contentsOfDirectory(atPath: conformance.path)
            .filter { $0.hasSuffix(".json") }
            .map { ($0, conformance.appendingPathComponent($0)) }
        for example in examples {
            worlds.append(("\(example.lastPathComponent)/base", example.appendingPathComponent("snapshots/base.json")))
            worlds.append(("\(example.lastPathComponent)/head", example.appendingPathComponent("manifest.json")))
        }
        for (name, url) in worlds {
            let manifest = try parseManifest(try read(url))
            let state = try foldLog(manifest, [])
            // Names bind at ingestion: compare against the bound entities.
            var bound = manifest
            bound.entities = state.entities
            XCTAssertEqual(normalized(try toManifest(state)), normalized(bound), name)
        }
    }

    func testEveryExampleIsHeadFirstItsManifestIsTheFoldToMain() throws {
        XCTAssertEqual(SUPPORTED_FORMAT_VERSION, 2)
        for example in examples {
            let package = try WorldPackage(directory: example)
            let name = example.lastPathComponent
            XCTAssertEqual(package.packageJSON?["format_version"]?.int, 2, name)
            XCTAssertEqual(normalized(try toManifest(try package.folded())), normalized(package.manifest), name)
            let bytes = try Data(contentsOf: example.appendingPathComponent("manifest.json"))
            let sha = SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
            XCTAssertEqual(package.packageJSON?["world_sha256"]?.string, sha, "\(name): world_sha256")
        }
    }

    func testModifyWorldReachesEverySceneFieldAndUndoes() throws {
        let manifest = try parseManifest(try read(hello.appendingPathComponent("manifest.json")))
        let state = try foldLog(manifest, [])
        let op = try JSONValue(parsing: #"""
            {"ModifyWorld": {"patch": {
              "meta": {"name": "hello-again", "description": "renamed"},
              "environment": null,
              "tours": [{"name": "walk", "waypoints": []}],
              "soundtrack": null}}}
            """#)
        let inverse = try computeInverse(op, state)
        var changed = state
        guard case let .edit(kind, value) = classifyOp(op) else { return XCTFail("not an edit") }
        try applyEdit(&changed, kind, value)
        let m = try toManifest(changed)
        XCTAssertEqual(m.name, "hello-again")
        XCTAssertNil(m.environment, "null clears")
        XCTAssertEqual(m.fields["tours"]?.array?.count, 1)
        guard case let .edit(ik, iv) = classifyOp(inverse) else { return XCTFail("not an edit") }
        try applyEdit(&changed, ik, iv)
        XCTAssertEqual(normalized(try toManifest(changed)), normalized(try toManifest(state)))
        let clear = try JSONValue(parsing: #"{"meta": null}"#)
        XCTAssertThrowsError(try applyEdit(&changed, "ModifyWorld", .object(["patch": clear])))
    }

    func testAnEntrysMessageIsPartOfItsIdentityInEveryReference() throws {
        let entry = try parseLogLine(#"{"id":"x","parent":"e6","revision":7,"author":{"name":"claude"},"timestamp_ms":1790000000123,"message":"a lantern by the gate","ops":[{"DeleteEntity":{"id":21}}]}"#)
        XCTAssertEqual(entry.message, "a lantern by the gate")
        XCTAssertEqual(computeEntryId(entry), "sha256:a7cd0955d35a2ff15b16ad7cc3440fb064d675ceceeca865a7ace463a5d177f2")
    }
}
