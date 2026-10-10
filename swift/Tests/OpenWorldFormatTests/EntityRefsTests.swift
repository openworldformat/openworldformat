// The one list of entity-reference fields (spec/world.md "Identity",
// schema/entity-refs.json): the Swift reference embeds a copy
// (Sources/OpenWorldFormat/EntityRefs.swift) and this test fails when
// the copy drifts from the canonical file — and pins that the passes
// actually walk it. Mirrors js/test/entity-refs.test.mjs.

import XCTest
@testable import OpenWorldFormat

final class EntityRefsTests: XCTestCase {
    /// schema/entity-refs.json, decoded: the canonical generated list.
    private struct CanonicalList: Decodable {
        let refs: [Entry]
        struct Entry: Decodable {
            let scope: String
            let path: [String]
            let kind: String
        }
    }

    private func canonicalRefs() throws -> [EntityRef] {
        let text = try read(root.appendingPathComponent("schema/entity-refs.json"))
        let decoded = try JSONDecoder().decode(CanonicalList.self, from: Data(text.utf8))
        return decoded.refs.map { EntityRef(scope: $0.scope, path: $0.path, kind: $0.kind) }
    }

    func testTheEmbeddedListIsTheCanonicalList() throws {
        XCTAssertEqual(ENTITY_REFS, try canonicalRefs())
    }

    func testTheCanonicalListHoldsEveryFieldTheMergeTableRewrites() throws {
        let paths = ENTITY_REFS.map { "\($0.scope):\($0.path.joined(separator: "/"))" }
        for expected in [
            "entity:parent",
            "entity:behaviors/*/Orbit/center",
            "entity:behaviors/*/LookAt/target",
            "avatar:model_entity",
            "creation:entities/*",
        ] {
            XCTAssertTrue(paths.contains(expected), "missing \(expected)")
        }
    }

    func testNameBindingWalksTheListAStringParentBindsAtIntake() throws {
        let manifest = try parseManifest(
            #"{"version": 3, "meta": {"name": "t"}, "entities": [{"id": 1, "name": "sun"}]}"#)
        let batch = try JSONValue(parsing: #"""
            [
              {"SpawnEntity": {"entity": {"id": 2, "name": "planet", "parent": "sun"}}},
              {"SpawnEntity": {"entity": {"name": "moon", "parent": "planet"}}}
            ]
            """#)
        guard case let .ingested(done) = ingest(try foldLog(manifest, []), batch) else {
            return XCTFail("the batch should have been taken")
        }
        XCTAssertEqual(done.ops[0]["SpawnEntity"]?["entity"]?["parent"]?.int, 1)
        XCTAssertEqual(done.ops[1]["SpawnEntity"]?["entity"]?["parent"]?.int, 2)
        // A raw log's string parent is not a committed form: the fold
        // refuses it at apply (committed ops carry ids).
        assertThrows(
            try foldLog(manifest, [
                entryOf(#"[{"SpawnEntity": {"entity": {"id": 4, "name": "x", "parent": "sun"}}}]"#),
            ]),
            containing: "parent")
    }

    func testIngestBindsTheAvatarsMarkedRefsModelEntityByName() throws {
        let manifest = try parseManifest(
            #"{"version": 3, "meta": {"name": "t"}, "entities": [{"id": 1, "name": "hero"}]}"#)
        let batch = try JSONValue(parsing: #"""
            [{"ModifyWorld": {"patch": {"avatar": {"model_entity": "hero"}}}}]
            """#)
        guard case let .ingested(done) = ingest(try foldLog(manifest, []), batch) else {
            return XCTFail("the batch should have been taken")
        }
        XCTAssertEqual(done.ops[0]["ModifyWorld"]?["patch"]?["avatar"]?["model_entity"]?.int, 1)
        XCTAssertEqual(done.state.scene["avatar"]?["model_entity"]?.int, 1)
    }

    func testMergeRewritingWalksTheListForEveryScope() throws {
        let manifest = try parseManifest(#"{"version": 3, "meta": {"name": "t"}, "entities": []}"#)
        let state = try foldLog(manifest, [
            entryOf(#"[{"SpawnEntity": {"entity": {"id": 1, "name": "main-one"}}}]"#),
        ])
        let branch = [
            try entryOf(#"""
                [
                  {"SpawnEntity": {"entity": {"id": 1, "name": "branch-one", "parent": 1,
                    "behaviors": [{"Orbit": {"center": 1, "radius": 2, "speed": 10}},
                                  {"LookAt": {"target": 1}}]}}},
                  {"ModifyWorld": {"patch": {"avatar": {"model_entity": 1},
                    "creations": [{"id": 1, "name": "c", "entities": [1]}]}}}
                ]
                """#, revision: 2),
        ]
        let (entries, remapped) = try mergeBranch(state, branch)
        XCTAssertEqual(remapped, [1: 2])
        let spawn = entries[0].ops[0]
        XCTAssertEqual(spawn["SpawnEntity"]?["entity"]?["parent"]?.int, 2)
        XCTAssertEqual(spawn["SpawnEntity"]?["entity"]?["behaviors"]?.array?[0]["Orbit"]?["center"]?.int, 2)
        XCTAssertEqual(spawn["SpawnEntity"]?["entity"]?["behaviors"]?.array?[1]["LookAt"]?["target"]?.int, 2)
        let world = entries[0].ops[1]
        XCTAssertEqual(world["ModifyWorld"]?["patch"]?["avatar"]?["model_entity"]?.int, 2)
        XCTAssertEqual(
            world["ModifyWorld"]?["patch"]?["creations"]?.array?[0]["entities"]?.array,
            [.number(2)])
    }

    func testStrictReadersAdmitMarkedWorlds() throws {
        // ext-cinematography is registered (the registry rule).
        _ = try parseManifest(
            try read(root.appendingPathComponent("conformance/cinematography.json")), strict: true)
    }
}
