// The fold keeps two derived indexes — name to id, and id to position in
// `entities` — so that no edit has to scan the array. They are only worth
// having if they cannot go stale, and a delete is where they would: it
// removes entities from the middle, so every later position moves.

import XCTest
@testable import OpenWorldFormat

final class IndexTests: XCTestCase {
    private func threeEntities() throws -> WorldManifest {
        try parseManifest(#"{"version": 3, "meta": {"name": "i"}, "entities": []}"#)
    }

    func testTheIndexesAgreeWithTheEntitiesAfterEveryKindOfEdit() throws {
        let entries = [
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 1, "name": "a"}}}]"#),
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 2, "name": "b"}}}]"#, revision: 2),
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 3, "name": "c"}}}]"#, revision: 3),
            // Removing from the middle moves c's position.
            try entryOf(#"[{"DeleteEntity": {"id": 2}}]"#, revision: 4),
            try entryOf(#"[{"ModifyEntity": {"id": 3, "patch": {"name": "c2"}}}]"#, revision: 5),
        ]
        let state = try foldLog(try threeEntities(), entries)

        XCTAssertEqual(state.entities.map(\.id), [1, 3])
        XCTAssertEqual(state.nameToId, ["a": 1, "c2": 3])
        for (position, entity) in state.entities.enumerated() {
            XCTAssertEqual(
                state.idToIndex[entity.id], position,
                "idToIndex disagrees with entities for \(entity.id)")
            XCTAssertEqual(state.nameToId[entity.name], entity.id)
        }
        XCTAssertNil(state.idToIndex[2], "the deleted entity must leave the index")
        XCTAssertNil(state.nameToId["b"], "and so must its name")
        XCTAssertNil(state.nameToId["c"], "a rename must not leave the old name behind")
    }

    func testAnEntityIsReachableByIdAfterASiblingBeforeItWasDeleted() throws {
        // The regression the rebuild exists for: with a stale index, id 3
        // would point at the position b used to hold, so this modify would
        // patch the wrong entity — or none.
        let entries = [
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 1, "name": "a"}}}]"#),
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 2, "name": "b"}}}]"#, revision: 2),
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 3, "name": "c"}}}]"#, revision: 3),
            try entryOf(#"[{"DeleteEntity": {"id": 1}}]"#, revision: 4),
            try entryOf(#"[{"DeleteEntity": {"id": 2}}]"#, revision: 5),
            try entryOf(#"[{"ModifyEntity": {"id": 3, "patch": {"name": "survivor"}}}]"#, revision: 6),
        ]
        let state = try foldLog(try threeEntities(), entries)
        XCTAssertEqual(state.entities.count, 1)
        XCTAssertEqual(state.entities[0].id, 3)
        XCTAssertEqual(state.entities[0].name, "survivor")
        XCTAssertEqual(state.idToIndex, [3: 0])
    }

    func testDeletingAParentTakesItsSubtreeOutOfBothIndexes() throws {
        let entries = [
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 1, "name": "root"}}}]"#),
            try entryOf(
                #"[{"SpawnEntity": {"entity": {"id": 2, "name": "child", "parent": 1}}}]"#,
                revision: 2),
            try entryOf(
                #"[{"SpawnEntity": {"entity": {"id": 3, "name": "grandchild", "parent": 2}}}]"#,
                revision: 3),
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 4, "name": "bystander"}}}]"#, revision: 4),
            try entryOf(#"[{"DeleteEntity": {"id": 1}}]"#, revision: 5),
        ]
        let state = try foldLog(try threeEntities(), entries)
        XCTAssertEqual(state.entities.map(\.id), [4])
        XCTAssertEqual(state.idToIndex, [4: 0])
        XCTAssertEqual(state.nameToId, ["bystander": 4])
    }

    func testANameFreedByADeleteCanBeTakenAgain() throws {
        let entries = [
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 1, "name": "lamp"}}}]"#),
            try entryOf(#"[{"DeleteEntity": {"id": 1}}]"#, revision: 2),
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 2, "name": "lamp"}}}]"#, revision: 3),
        ]
        let state = try foldLog(try threeEntities(), entries)
        XCTAssertEqual(state.nameToId, ["lamp": 2])
    }

    func testADuplicateNameIsStillRefusedThroughTheIndex() throws {
        let entries = [
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 1, "name": "lamp"}}}]"#),
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 2, "name": "lamp"}}}]"#, revision: 2),
        ]
        assertThrows(try foldLog(try threeEntities(), entries), containing: "already exists")
    }
}
