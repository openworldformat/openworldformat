// mergeBranch: the merge authority's id rewrite (spec/session.md) —
// the golden scenario plus the rules around it.

import XCTest
@testable import OpenWorldFormat

final class MergeTests: XCTestCase {
    /// Main holds exactly entity 5; the branch reuses the id.
    let mainManifest = try! parseManifest(
        #"{"version": 3, "meta": {"name": "m"}, "entities": [{"id": 5, "name": "main-five"}]}"#)
    var main: FoldState { try! foldLog(mainManifest, []) }

    func testCollidingIdsRemapAndTheMergedBranchFolds() throws {
        // The golden scenario: the branch spawns 5 (colliding) plus a
        // child 6 parented to 5 with a behavior ref to 5. The remap
        // moves 5 to a fresh id and rewrites the parent and the ref.
        let branch = [
            try entryOf(
                #"[{"SpawnEntity": {"entity": {"id": 5, "name": "branch-five"}}}, {"SpawnEntity": {"entity": {"id": 6, "name": "child", "parent": 5, "behaviors": [{"Orbit": {"center": 5, "radius": 1.5}}]}}}]"#),
        ]
        let (rewritten, remapped) = try mergeBranch(main, branch)
        XCTAssertEqual(remapped, [5: 7])
        XCTAssertEqual(rewritten[0].ops, [
            .object(["SpawnEntity": .object(["entity": .object([
                "id": .number(7), "name": .string("branch-five"),
            ])])]),
            .object(["SpawnEntity": .object(["entity": .object([
                "id": .number(6), "name": .string("child"), "parent": .number(7),
                "behaviors": .array([.object(["Orbit": .object(["center": .number(7), "radius": .number(1.5)])])]),
            ])])]),
        ])
        // The rewritten entries classify again — history stays history,
        // edits stay edits.
        XCTAssertEqual(editOps(rewritten[0]).count, 2)

        // Folding main plus the rewritten branch succeeds, and the
        // child is parented to — and orbits — the remapped id.
        let merged = try foldLog(mainManifest, rewritten)
        let child = try XCTUnwrap(merged.entities.first { $0.id == 6 })
        XCTAssertEqual(child.parent, 7)
        XCTAssertEqual(
            child.fields["behaviors"],
            .array([.object(["Orbit": .object(["center": .number(7), "radius": .number(1.5)])])]))
    }

    func testModifyAndDeleteIdsRemapIncludingAPatchedParent() throws {
        let branch = [
            try entryOf(
                #"[{"SpawnEntity": {"entity": {"id": 5, "name": "b5"}}}, {"SpawnEntity": {"entity": {"id": 6, "name": "b6", "parent": 5}}}]"#),
            try entryOf(
                #"[{"ModifyEntity": {"id": 6, "patch": {"parent": 5}}}, {"DeleteEntity": {"id": 5}}]"#,
                revision: 2, timestampMs: 1),
        ]
        let (rewritten, remapped) = try mergeBranch(main, branch)
        XCTAssertEqual(remapped, [5: 7])
        XCTAssertEqual(rewritten[1].ops, [
            .object(["ModifyEntity": .object(["id": .number(6), "patch": .object(["parent": .number(7)])])]),
            .object(["DeleteEntity": .object(["id": .number(7)])]),
        ])
        // The whole rewritten branch folds over main.
        let merged = try foldLog(mainManifest, rewritten)
        XCTAssertEqual(merged.entities.map(\.id), [5])   // the delete took the subtree
    }

    func testABranchWithNoCollisionsPassesThroughUnchanged() throws {
        let branch = [
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 100, "name": "far-away"}}}]"#),
            LogEntry(revision: 2, ops: [
                .object(["tool": .string("gen"), "args": .object([:])]),
            ]),
        ]
        let (rewritten, remapped) = try mergeBranch(main, branch)
        XCTAssertEqual(remapped, [:])
        XCTAssertEqual(rewritten, branch)
    }

    func testHistoryOpsAndStringRefsAreUntouched() throws {
        let branch = [
            try entryOf(
                #"[{"tool": "gen", "args": {}}, {"SpawnEntity": {"entity": {"id": 5, "name": "b5", "behaviors": [{"Orbit": {"center": "main-five"}}]}}}]"#),
        ]
        let (rewritten, remapped) = try mergeBranch(main, branch)
        // The branch spawns only 5 here, so 6 is the first id nothing holds.
        XCTAssertEqual(remapped, [5: 6])
        // The history op passes through byte for byte.
        XCTAssertEqual(rewritten[0].ops[0], branch[0].ops[0])
        // The string ref survives the rewrite — name binding resolves
        // it when the merged log folds.
        XCTAssertEqual(
            rewritten[0].ops[1]["SpawnEntity"]?["entity"]?["behaviors"],
            .array([.object(["Orbit": .object(["center": .string("main-five")])])]))
        let merged = try foldLog(mainManifest, rewritten)
        XCTAssertEqual(
            merged.entities.first { $0.id == 6 }?.fields["behaviors"],
            .array([.object(["Orbit": .object(["center": .number(5)])])]))
    }

    func testFreshIdsSkipEverythingMainAndTheBranchHold() throws {
        // Main holds 5; the branch spawns 5, 6 and 7 — the fresh id for
        // the collision must skip 6 and 7 too, or it would shadow them.
        let branch = [
            try entryOf(
                #"[{"SpawnEntity": {"entity": {"id": 5, "name": "b5"}}}, {"SpawnEntity": {"entity": {"id": 6, "name": "b6"}}}, {"SpawnEntity": {"entity": {"id": 7, "name": "b7"}}}]"#),
        ]
        let (_, remapped) = try mergeBranch(main, branch)
        XCTAssertEqual(remapped, [5: 8])
    }

    func testTheMergeRefusesWhenNoIdBelowTheCeilingIsFree() throws {
        let capped = try parseManifest(
            #"{"version": 3, "entities": [{"id": 9007199254740991, "name": "cap"}]}"#)
        let cappedState = try foldLog(capped, [])
        let branch = [
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 9007199254740991, "name": "again"}}}]"#),
        ]
        assertThrows(try mergeBranch(cappedState, branch), containing: "id ceiling")
    }
}
