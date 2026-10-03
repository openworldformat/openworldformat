// The format's standing rules, pinned: the id ceiling, the shape
// collision rule, state precedence, and the modify patch semantics.

import XCTest
@testable import OpenWorldFormat

final class RulesTests: XCTestCase {
    func testSpawnRefusesIdsAboveTheCeiling() throws {
        XCTAssertEqual(MAX_ENTITY_ID, 9_007_199_254_740_991)   // 2^53 - 1
        // The ceiling itself spawns.
        _ = try foldLog(tinyManifest(), [
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 9007199254740991, "name": "cap"}}}]"#),
        ])
        // One past it refuses.
        let over = try entryOf(#"[{"SpawnEntity": {"entity": {"id": 9007199254740992, "name": "over"}}}]"#)
        assertThrows(
            try foldLog(tinyManifest(), [over]),
            containing: "entity 9007199254740992 exceeds the id ceiling 9007199254740991 (2^53-1)")
    }

    func testTheShapeCollisionRuleEditsPascalCaseHistoryLowercase() {
        for key in EDIT_KEYS {
            XCTAssertTrue(opKindShapeOk(key), "\(key) is an edit kind: PascalCase")
        }
        for kind in ["tool", "input", "state", "clock", "merge"] {
            XCTAssertTrue(opKindShapeOk(kind), "\(kind) is a history kind: lowercase")
        }
        XCTAssertFalse(opKindShapeOk("spawnentity"))
        XCTAssertFalse(opKindShapeOk("Spawnentity"))
        XCTAssertFalse(opKindShapeOk("Tool"))
        XCTAssertFalse(opKindShapeOk("ext-physics"))
        // And the classifier agrees: a lowercased edit key is no edit.
        XCTAssertEqual(
            classifyOp(.object(["spawnentity": .object(["entity": .object(["id": .number(1), "name": .string("a")])])])),
            .unknown)
    }

    func testADeclaredDottedFieldTakesPrecedenceOverMapSubKeys() throws {
        // Both "inventory" (map) and "inventory.rope" (int) are
        // declared: the exact declared key wins, and the map stays
        // untouched (spec/state.md).
        let stateDoc = StateDocument(fields: [
            "inventory": StateField(json: .object(["type": .string("map"), "initial": .object([:])])),
            "inventory.rope": StateField(json: .object(["type": .string("int"), "initial": .number(0)])),
        ])
        let entries = [try entryOf(#"[{"state": {"inventory.rope": 5}}]"#)]
        let folded = foldState(stateDoc, entries)
        XCTAssertEqual(folded.values["inventory.rope"], .number(5))
        XCTAssertEqual(folded.values["inventory"], .object([:]))
        XCTAssertEqual(folded.undeclared, [])
    }

    func testAModifyPatchesNullClearsAndAnEmptyPatchChangesNothing() throws {
        let manifest = try parseManifest(
            #"{"version": 3, "entities": [{"id": 1, "name": "lamp", "light": {"light_type": "point", "intensity": 100.0}}]}"#)
        let before = try foldLog(manifest, [])

        let cleared = try foldLog(manifest, [
            try entryOf(#"[{"ModifyEntity": {"id": 1, "patch": {"light": null}}}]"#),
        ])
        XCTAssertNil(cleared.entities.first { $0.id == 1 }?.fields["light"])

        let noop = try foldLog(manifest, [
            try entryOf(#"[{"ModifyEntity": {"id": 1, "patch": {}}}]"#),
        ])
        XCTAssertEqual(noop.entities, before.entities)   // absent = unchanged, all of it
    }
}
