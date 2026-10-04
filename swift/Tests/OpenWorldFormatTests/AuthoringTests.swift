// The Authoring profile's ingestion (spec/session.md "Authoring") —
// mirrors js/test/authoring.test.mjs and the Rust crate's authoring
// tests: a batch is taken whole or refused whole, names bind to ids,
// spawns left without ids get consecutive ones, partial struct patches
// merge as RFC 7396, and the head's canonical text is the same bytes
// the other references write.

import XCTest
@testable import OpenWorldFormat

/// The yard the other references' authoring tests stand in.
private func yard() throws -> FoldState {
    let manifest = try parseManifest(#"""
        {"version": 3, "meta": {"name": "yard"}, "entities": [
          {"id": 1, "name": "ground", "shape": {"Plane": {"x": 20, "z": 20}}},
          {"id": 2, "name": "crate",
           "transform": {"position": [0, 0.5, 0], "scale": [2, 2, 2]},
           "material": {"color": [0.6, 0.4, 0.2, 1], "roughness": 0.8}}],
         "next_entity_id": 3}
        """#)
    return try foldLog(manifest, [])
}

final class AuthoringTests: XCTestCase {
    func testIngestBindsNamesAllocatesIdsAndMergesPartialPatches() throws {
        let batch = try JSONValue(parsing: #"""
            [
              {"SpawnEntity": {"entity": {"name": "lamp", "parent": "crate"}}},
              {"ModifyEntity": {"id": "crate",
                "patch": {"transform": {"position": [3, 0.5, 0]},
                          "material": {"base_color_texture": "brick.png"}}}},
              {"ModifyWorld": {"patch": {"meta": {"description": "a yard"}}}}
            ]
            """#)
        guard case let .ingested(done) = ingest(try yard(), batch) else {
            return XCTFail("the batch should have been taken")
        }
        XCTAssertEqual(done.spawned["lamp"], 3)
        let lamp = done.state.entities.first { $0.name == "lamp" }
        XCTAssertEqual(lamp?.parent, 2)
        let crate = done.state.entities.first { $0.id == 2 }
        XCTAssertEqual(crate?.fields["transform"]?["scale"]?.array,
                       [.number(2), .number(2), .number(2)], "scale kept")
        XCTAssertEqual(crate?.fields["material"]?["roughness"]?.double, 0.8, "roughness kept")
        XCTAssertEqual(crate?.fields["material"]?["base_color_texture"]?.string, "brick.png")
        XCTAssertEqual(try toManifest(done.state).meta?.description, "a yard")
        XCTAssertEqual(done.state.name, "yard", "meta merges, the name stays")
        // What commits is the whole value, so the fold needs no merging.
        XCTAssertEqual(done.ops[1]["ModifyEntity"]?["patch"]?["transform"]?["scale"]?.array,
                       [.number(2), .number(2), .number(2)])
    }

    func testOneBadOpRefusesTheBatchWithAReasonPerOp() throws {
        let batch = try JSONValue(parsing: #"""
            [
              {"ModifyEntity": {"id": "crate",
                "patch": {"material": {"colour": [1, 0, 0, 1]}}}},
              {"DeleteEntity": {"id": "nobody"}},
              {"MoveEntity": {"id": 2}},
              {"SpawnEntity": {"entity": {"id": 1, "name": "again"}}},
              {"ModifyEntity": {"id": "ground",
                "patch": {"transform": {"position": [0, 1, 0]}}}}
            ]
            """#)
        guard case let .refused(errors) = ingest(try yard(), batch) else {
            return XCTFail("the batch should have been refused")
        }
        XCTAssertEqual(errors.count, 4, errors.joined(separator: "\n"))
        XCTAssertTrue(errors[0].hasPrefix("op 0: /ModifyEntity/patch/material/colour"), errors[0])
        XCTAssertTrue(errors[1].contains("no entity is named \"nobody\""), errors[1])
        XCTAssertTrue(errors[2].contains("isn't an op kind"), errors[2])
        XCTAssertTrue(errors[3].contains("already exists"), errors[3])
    }

    func testNotABatchOrAnEmptyOneIsRefused() throws {
        let state = try yard()
        guard case let .refused(notABatch) = ingest(state, .number(3)) else {
            return XCTFail("a number is not a batch")
        }
        XCTAssertEqual(notABatch, ["a batch is [op, …] or {\"ops\": [op, …]}"])
        guard case let .refused(noOps) = ingest(state, .array([])) else {
            return XCTFail("an empty batch is refused")
        }
        XCTAssertEqual(noOps, ["the batch holds no ops"])
        guard case let .refused(badOps) = ingest(state, .object(["ops": .string("no")])) else {
            return XCTFail("ops must be an array")
        }
        XCTAssertEqual(badOps, ["a batch is [op, …] or {\"ops\": [op, …]}"])
    }

    func testSpawnsLeftWithoutAnIdGetConsecutiveOnes() throws {
        let batch = try JSONValue(parsing: #"""
            [{"Batch": {"ops": [
              {"SpawnEntity": {"entity": {"name": "a"}}},
              {"SpawnEntity": {"entity": {"name": "b"}}}]}}]
            """#)
        guard case let .ingested(done) = ingest(try yard(), batch) else {
            return XCTFail("the batch should have been taken")
        }
        XCTAssertEqual(done.spawned, ["a": 3, "b": 4])
        XCTAssertEqual(try toManifest(done.state).fields["next_entity_id"]?.int, 5)
    }

    func testSetEnvironmentMergesItsEnvTheSameWay() throws {
        let manifest = try parseManifest(#"""
            {"version": 3, "meta": {"name": "yard"}, "entities": [],
             "environment": {"background_color": [0.1, 0.1, 0.1], "fog_density": 0.2},
             "next_entity_id": 1}
            """#)
        let batch = try JSONValue(parsing: #"""
            [{"SetEnvironment": {"env": {"fog_density": 0.9}}}]
            """#)
        guard case let .ingested(done) = ingest(try foldLog(manifest, []), batch) else {
            return XCTFail("the batch should have been taken")
        }
        XCTAssertEqual(done.state.environment?.fogDensity, 0.9)
        XCTAssertEqual(done.state.environment?.backgroundColor, [0.1, 0.1, 0.1], "kept by the merge")
        // The committed op carries the merged whole.
        XCTAssertEqual(done.ops[0]["SetEnvironment"]?["env"]?["background_color"]?.array,
                       [.number(0.1), .number(0.1), .number(0.1)])
    }

    func testMergePatchIsRfc7396() throws {
        let current = try JSONValue(parsing: #"{"a": 1, "b": {"c": 2, "d": 3}}"#)
        let change = try JSONValue(parsing: #"{"b": {"c": null, "e": 4}, "f": 5}"#)
        XCTAssertEqual(mergePatch(current, change),
                       try JSONValue(parsing: #"{"a": 1, "b": {"d": 3, "e": 4}, "f": 5}"#))
    }

    func testEveryExampleHeadIsInCanonicalText() throws {
        for example in examples {
            let text = try read(example.appendingPathComponent("manifest.json"))
            XCTAssertEqual(manifestText(try parseManifest(text)), text,
                           "\(example.lastPathComponent): canonical text")
        }
    }

    func testTheCanonicalTextIsThePinnedForm() throws {
        // Members sorted, nulls left out, plain arrays inline, entities by id.
        let manifest = try parseManifest(#"""
            {"version": 3, "meta": {"name": "t", "description": null},
             "entities": [{"name": "b", "id": 2},
                          {"id": 1, "name": "a",
                           "transform": {"position": [0.1, 2, -3.5]}}]}
            """#)
        let expected = """
            {
              "entities": [
                {
                  "id": 1,
                  "name": "a",
                  "transform": {
                    "position": [0.1, 2, -3.5]
                  }
                },
                {
                  "id": 2,
                  "name": "b"
                }
              ],
              "meta": {
                "name": "t"
              },
              "version": 3
            }
            """ + "\n"
        XCTAssertEqual(manifestText(manifest), expected)
    }
}
