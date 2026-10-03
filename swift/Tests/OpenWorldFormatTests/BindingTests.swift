// Immediate name binding (spec/world.md, "Identity"): behavior refs
// written by name resolve to ids at ingestion, against the fold-so-far
// — never later, because a rename would strand them.

import XCTest
@testable import OpenWorldFormat

/// The smallest world the binding tests fold over.
func tinyManifest() throws -> WorldManifest {
    try parseManifest(#"{"version": 3, "meta": {"name": "t"}, "entities": [{"id": 1, "name": "root"}]}"#)
}

final class BindingTests: XCTestCase {
    func testANameRefBindsImmediatelyAndSurvivesTheLaterRename() throws {
        // The rename is the hazard the rule exists for: the satellite's
        // ref bound to an id when its entry applied, so renaming "a"
        // afterwards can't strand it.
        let entries = [
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 500, "name": "a"}}}]"#),
            try entryOf(
                #"[{"SpawnEntity": {"entity": {"id": 501, "name": "satellite", "behaviors": [{"Orbit": {"center": "a", "radius": 2.0}}]}}}]"#,
                revision: 2, timestampMs: 1),
            try entryOf(#"[{"ModifyEntity": {"id": 500, "patch": {"name": "b"}}}]"#, revision: 3, timestampMs: 2),
        ]
        let state = try foldLog(tinyManifest(), entries)
        let satellite = try XCTUnwrap(state.entities.first { $0.id == 501 })
        XCTAssertEqual(
            satellite.fields["behaviors"],
            .array([.object(["Orbit": .object(["center": .number(500), "radius": .number(2)])])]))
    }

    func testARefWrittenAfterTheRenameUsingTheOldNameFailsItsEntry() throws {
        // The same three entries with the rename first: the ref is late,
        // and immediate binding refuses it at ingestion — it never
        // floats to fold time to misbind or dangle.
        let entries = [
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 500, "name": "a"}}}]"#),
            try entryOf(#"[{"ModifyEntity": {"id": 500, "patch": {"name": "b"}}}]"#, revision: 2, timestampMs: 1),
            try entryOf(
                #"[{"SpawnEntity": {"entity": {"id": 501, "name": "satellite", "behaviors": [{"Orbit": {"center": "a"}}]}}}]"#,
                revision: 3, timestampMs: 2),
        ]
        assertThrows(try foldLog(tinyManifest(), entries), containing: "no entity named 'a'")
    }

    func testAnUnknownNameAtIngestionFailsTheFold() throws {
        let entries = [
            try entryOf(
                #"[{"SpawnEntity": {"entity": {"id": 501, "name": "satellite", "behaviors": [{"LookAt": {"target": "nope"}}]}}}]"#),
        ]
        assertThrows(try foldLog(tinyManifest(), entries), containing: "no entity named 'nope'")
    }

    func testLookAtBindsAndModulationsTargetDoesNot() throws {
        // `modulations[].target` names a property, not an entity — the
        // rule never touches it.
        let entries = [
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 500, "name": "a"}}}]"#),
            try entryOf(
                #"[{"SpawnEntity": {"entity": {"id": 501, "name": "watcher", "behaviors": [{"LookAt": {"target": "a"}}], "modulations": [{"target": "emissive", "signal": "bass"}]}}}]"#,
                revision: 2, timestampMs: 1),
        ]
        let state = try foldLog(tinyManifest(), entries)
        let watcher = try XCTUnwrap(state.entities.first { $0.id == 501 })
        XCTAssertEqual(
            watcher.fields["behaviors"],
            .array([.object(["LookAt": .object(["target": .number(500)])])]))
        XCTAssertEqual(
            watcher.fields["modulations"],
            .array([.object(["target": .string("emissive"), "signal": .string("bass")])]))
    }

    func testRefsBindWithinTheSameEntryAfterItsEditsApply() throws {
        // One entry, two ops: the entry is atomic, so the satellite's
        // ref resolves against the state its own spawn of "a" made.
        let entries = [
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 500, "name": "a"}}}, {"SpawnEntity": {"entity": {"id": 501, "name": "satellite", "behaviors": [{"Orbit": {"center": "a"}}]}}}]"#),
        ]
        let state = try foldLog(tinyManifest(), entries)
        let satellite = try XCTUnwrap(state.entities.first { $0.id == 501 })
        XCTAssertEqual(
            satellite.fields["behaviors"],
            .array([.object(["Orbit": .object(["center": .number(500)])])]))
    }

    func testBaseNameRefsResolveAgainstTheCompleteBase() throws {
        let manifest = try parseManifest(
            #"{"version": 3, "entities": [{"id": 3, "name": "hub"}, {"id": 4, "name": "orbiter", "behaviors": [{"Orbit": {"center": "hub"}}]}]}"#)
        let state = try foldLog(manifest, [])
        XCTAssertEqual(
            state.entities.first { $0.id == 4 }?.fields["behaviors"],
            .array([.object(["Orbit": .object(["center": .number(3)])])]))

        // A base ref nothing answers fails the fold, not just the entry.
        let broken = try parseManifest(
            #"{"version": 3, "entities": [{"id": 4, "name": "orbiter", "behaviors": [{"Orbit": {"center": "ghost"}}]}]}"#)
        assertThrows(try foldLog(broken, []), containing: "no entity named 'ghost'")
    }

    func testTheBehaviorsConformanceWorldsNameRefsResolveOnFold() throws {
        let manifest = try parseManifest(try read(root.appendingPathComponent("conformance/behaviors.json")))
        let state = try foldLog(manifest, [])
        XCTAssertEqual(
            state.entities.first { $0.name == "orbiter_entity" }?.fields["behaviors"],
            .array([.object(["Orbit": .object(["center": .number(3), "radius": .number(3.0), "speed": .number(40.0), "axis": .array([.number(0), .number(1), .number(0)]), "phase": .number(0), "tilt": .number(15.0)])])]))
        XCTAssertEqual(
            state.entities.first { $0.name == "watcher" }?.fields["behaviors"],
            .array([.object(["LookAt": .object(["target": .number(4)])])]))
    }
}
