// computeInverse: undo is appending the inverse, and the inverse is
// computed against the state the edit is about to apply to
// (spec/session.md) — mirrored from the other references' tests.

import XCTest
@testable import OpenWorldFormat

final class InverseTests: XCTestCase {
    /// A state with a parent and a child to delete and restore.
    func treeState() throws -> FoldState {
        try foldLog(tinyManifest(), [
            try entryOf(
                #"[{"SpawnEntity": {"entity": {"id": 500, "name": "p", "transform": {"position": [1, 2, 3]}}}}, {"SpawnEntity": {"entity": {"id": 501, "name": "c", "parent": 500}}}]"#),
        ])
    }

    func testSpawnInversesToDelete() throws {
        let state = try foldLog(tinyManifest(), [])
        let spawn = try JSONValue(parsing: #"{"SpawnEntity": {"entity": {"id": 500, "name": "a"}}}"#)
        XCTAssertEqual(
            try computeInverse(spawn, state),
            .object(["DeleteEntity": .object(["id": .number(500)])]))
    }

    func testDeleteInversesToABatchOfParentsFirstSpawnsRestoringTheTree() throws {
        let state = try treeState()
        let parent = try XCTUnwrap(state.entities.first { $0.id == 500 })
        let child = try XCTUnwrap(state.entities.first { $0.id == 501 })
        let delete = try JSONValue(parsing: #"{"DeleteEntity": {"id": 500}}"#)
        let respawn = { (entity: WorldEntity) in
            JSONValue.object(["SpawnEntity": .object(["entity": entity.json])])
        }
        // Parents first, deep copies of the deleted tree.
        XCTAssertEqual(
            try computeInverse(delete, state),
            .object(["Batch": .object(["ops": .array([respawn(parent), respawn(child)])])]))

        // The round trip: delete, then append the inverse — the tree is
        // back, entity JSON for entity JSON.
        var after = state
        try applyEdit(&after, "DeleteEntity", .object(["id": .number(500)]))
        try applyEdit(&after, "Batch", .object(["ops": .array([respawn(parent), respawn(child)])]))
        XCTAssertEqual(after.entities.map(\.json), state.entities.map(\.json))
    }

    func testModifyInversesToTheCurrentValuesNullWhereAbsent() throws {
        let manifest = try parseManifest(
            #"{"version": 3, "entities": [{"id": 1, "name": "root"}, {"id": 2, "name": "lamp", "parent": 1, "light": {"light_type": "point", "intensity": 100.0}}]}"#)
        let state = try foldLog(manifest, [])
        let modify = try JSONValue(parsing:
            #"{"ModifyEntity": {"id": 2, "patch": {"name": "z", "parent": null, "light": null, "ext-physics": null}}}"#)
        XCTAssertEqual(
            try computeInverse(modify, state),
            .object(["ModifyEntity": .object([
                "id": .number(2),
                "patch": .object([
                    "name": .string("lamp"),
                    "parent": .number(1),
                    "light": .object(["light_type": .string("point"), "intensity": .number(100)]),
                    "ext-physics": .null,   // absent now, absent after the round trip
                ]),
            ])]))

        // An entity with no parent restores null for parent.
        let rootModify = try JSONValue(parsing:
            #"{"ModifyEntity": {"id": 1, "patch": {"parent": 2, "shape": {"Sphere": {"radius": 1}}}}}"#)
        XCTAssertEqual(
            try computeInverse(rootModify, state),
            .object(["ModifyEntity": .object([
                "id": .number(1),
                "patch": .object(["parent": .null, "shape": .null]),
            ])]))
    }

    func testSetEnvironmentRestoresThePreviousEnvOrClears() throws {
        let withEnv = try parseManifest(
            #"{"version": 3, "entities": [{"id": 1, "name": "root"}], "environment": {"fog_density": 0.2}}"#)
        let setEnv = try JSONValue(parsing: #"{"SetEnvironment": {"env": {}}}"#)
        XCTAssertEqual(
            try computeInverse(setEnv, try foldLog(withEnv, [])),
            .object(["SetEnvironment": .object(["env": .object(["fog_density": .number(0.2)])])]))
        // A setting that didn't exist comes back as absent: ModifyWorld clears it.
        XCTAssertEqual(
            try computeInverse(setEnv, try foldLog(tinyManifest(), [])),
            .object(["ModifyWorld": .object(["patch": .object(["environment": .null])])]))
    }

    func testSetCameraRestoresThePreviousCameraOrClears() throws {
        let withCamera = try parseManifest(
            #"{"version": 3, "entities": [{"id": 1, "name": "root"}], "camera": {"position": [1, 2, 3], "fov_degrees": 60.0}}"#)
        let setCamera = try JSONValue(parsing: #"{"SetCamera": {"camera": {}}}"#)
        XCTAssertEqual(
            try computeInverse(setCamera, try foldLog(withCamera, [])),
            .object(["SetCamera": .object(["camera": .object([
                "position": .array([.number(1), .number(2), .number(3)]),
                "fov_degrees": .number(60),
            ])])]))
        XCTAssertEqual(
            try computeInverse(setCamera, try foldLog(tinyManifest(), [])),
            .object(["ModifyWorld": .object(["patch": .object(["camera": .null])])]))
    }

    func testSetAmbienceRestoresThePreviousAmbienceOrEmpty() throws {
        let withAmbience = try parseManifest(
            #"{"version": 3, "entities": [{"id": 1, "name": "root"}], "ambience": [{"kind": "rain"}]}"#)
        let setAmbience = try JSONValue(parsing: #"{"SetAmbience": {"ambience": []}}"#)
        XCTAssertEqual(
            try computeInverse(setAmbience, try foldLog(withAmbience, [])),
            .object(["SetAmbience": .object(["ambience": .array([.object(["kind": .string("rain")])])])]))
        XCTAssertEqual(
            try computeInverse(setAmbience, try foldLog(tinyManifest(), [])),
            .object(["SetAmbience": .object(["ambience": .array([])])]))
    }

    func testAudioEmitterInversesAreSymmetric() throws {
        let state = try foldLog(tinyManifest(), [
            try entryOf(#"[{"SpawnAudioEmitter": {"name": "wind", "audio": {"kind": "wind", "turbulence": 0.3}}}]"#),
        ])
        let remove = try JSONValue(parsing: #"{"RemoveAudioEmitter": {"name": "wind"}}"#)
        XCTAssertEqual(
            try computeInverse(remove, state),
            .object(["SpawnAudioEmitter": .object([
                "name": .string("wind"),
                "audio": .object(["kind": .string("wind"), "turbulence": .number(0.3)]),
            ])]))
        let spawn = try JSONValue(parsing: #"{"SpawnAudioEmitter": {"name": "wind", "audio": {}}}"#)
        XCTAssertEqual(
            try computeInverse(spawn, state),
            .object(["RemoveAudioEmitter": .object(["name": .string("wind")])]))
        // Removing what isn't there is the same refusal the fold makes.
        let missing = try JSONValue(parsing: #"{"RemoveAudioEmitter": {"name": "missing"}}"#)
        assertThrows(try computeInverse(missing, state), containing: "no audio emitter named 'missing'")
    }

    func testABatchWalksForwardAndEmitsItsInversesReversed() throws {
        // Two spawns: each inverse is computed against the state its op
        // saw (walking forward over the trial), and the inverse batch
        // undoes them back-to-front.
        let before = try foldLog(tinyManifest(), [])
        let batch = try JSONValue(parsing:
            #"{"Batch": {"ops": [{"SpawnEntity": {"entity": {"id": 500, "name": "a"}}}, {"SpawnEntity": {"entity": {"id": 501, "name": "b", "parent": 500}}}]}}"#)
        XCTAssertEqual(
            try computeInverse(batch, before),
            .object(["Batch": .object(["ops": .array([
                .object(["DeleteEntity": .object(["id": .number(501)])]),
                .object(["DeleteEntity": .object(["id": .number(500)])]),
            ])])]))

        // The whole entry undone restores the state it started from.
        var after = before
        try applyEdit(&after, "Batch", .object(["ops": .array([
            .object(["SpawnEntity": .object(["entity": .object(["id": .number(500), "name": .string("a")])])]),
            .object(["SpawnEntity": .object(["entity": .object(["id": .number(501), "name": .string("b"), "parent": .number(500)])])]),
        ])]))
        try applyEdit(&after, "Batch", .object(["ops": .array([
            .object(["DeleteEntity": .object(["id": .number(501)])]),
            .object(["DeleteEntity": .object(["id": .number(500)])]),
        ])]))
        XCTAssertEqual(after.entities.map(\.json), before.entities.map(\.json))
    }

    func testABatchInverseUndoesAMixedEntryCompletely() throws {
        // Modify, then delete the modified entity. Walking forward, the
        // delete's inverse respawns the tree as the modify left it (a
        // nested Batch), and the modify's inverse restores the material
        // — emitted reversed, so undoing runs delete-first, modify-last.
        let state = try foldLog(tinyManifest(), [
            try entryOf(
                #"[{"SpawnEntity": {"entity": {"id": 500, "name": "p"}}}, {"SpawnEntity": {"entity": {"id": 501, "name": "c", "parent": 500, "material": {"color": [1, 0, 0]}}}}]"#),
        ])
        let parent = try XCTUnwrap(state.entities.first { $0.id == 500 })
        var strippedChild = try XCTUnwrap(state.entities.first { $0.id == 501 })
        strippedChild.fields.removeValue(forKey: "material")
        let batch = try JSONValue(parsing:
            #"{"Batch": {"ops": [{"ModifyEntity": {"id": 501, "patch": {"material": null}}}, {"DeleteEntity": {"id": 500}}]}}"#)
        let inverse = try computeInverse(batch, state)
        XCTAssertEqual(
            inverse,
            .object(["Batch": .object(["ops": .array([
                .object(["Batch": .object(["ops": .array([
                    .object(["SpawnEntity": .object(["entity": parent.json])]),
                    .object(["SpawnEntity": .object(["entity": strippedChild.json])]),
                ])])]),
                .object(["ModifyEntity": .object([
                    "id": .number(501),
                    "patch": .object(["material": .object(["color": .array([.number(1), .number(0), .number(0)])])]),
                ])]),
            ])])]))

        var after = state
        try applyEdit(&after, "Batch", .object(["ops": .array([
            .object(["ModifyEntity": .object(["id": .number(501), "patch": .object(["material": .null])])]),
            .object(["DeleteEntity": .object(["id": .number(500)])]),
        ])]))
        if case let .object(o) = inverse, case let .object(inner)? = o["Batch"] {
            try applyEdit(&after, "Batch", .object(inner))
        } else {
            XCTFail("inverse should be a Batch")
        }
        XCTAssertEqual(after.entities.map(\.json), state.entities.map(\.json))
    }

    func testUnknownOpsAndMissingEntitiesRefuse() throws {
        let state = try foldLog(tinyManifest(), [])
        assertThrows(
            try computeInverse(.object(["tool": .string("x"), "args": .object([:])]), state),
            containing: "unknown edit")
        assertThrows(
            try computeInverse(try JSONValue(parsing: #"{"DeleteEntity": {"id": 999}}"#), state),
            containing: "no entity 999")
        assertThrows(
            try computeInverse(try JSONValue(parsing: #"{"ModifyEntity": {"id": 999, "patch": {}}}"#), state),
            containing: "no entity 999")
    }
}
