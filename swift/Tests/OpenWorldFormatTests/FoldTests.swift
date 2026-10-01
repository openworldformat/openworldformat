// The fold, over the format's own examples — mirrors js/test/fold.test.mjs
// and python/tests/test_fold.py, assertion for assertion.

import XCTest
@testable import OpenWorldFormat

let root = URL(fileURLWithPath: #filePath)  // …/openworldformat/swift/Tests/OpenWorldFormatTests/FoldTests.swift
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()

let hello = root.appendingPathComponent("examples/hello-world")

func read(_ url: URL) throws -> String {
    try String(contentsOf: url, encoding: .utf8)
}

func readEntries(_ directory: URL) throws -> [LogEntry] {
    try read(directory.appendingPathComponent("ops.jsonl"))
        .split(separator: "\n")
        .filter { !$0.isEmpty }
        .map(String.init)
        .compactMap { try? parseLogLine($0) }
}

func entryOf(_ ops: String, revision: Int = 1, timestampMs: Double = 0) throws -> LogEntry {
    let json = try JSONValue(parsing: """
    {"revision": \(revision), "author": {"name": "t"}, "ops": \(ops), "timestamp_ms": \(timestampMs)}
    """)
    let o = json.object ?? [:]
    return LogEntry(
        revision: revision,
        author: o["author"],
        timestampMs: o["timestamp_ms"]?.double,
        ops: o["ops"]?.array ?? [])
}

func assertThrows<T>(_ expression: @autoclosure () throws -> T, containing fragment: String,
                     file: StaticString = #filePath, line: UInt = #line) {
    do {
        _ = try expression()
        XCTFail("expected an error containing \"\(fragment)\"", file: file, line: line)
    } catch {
        let message = (error as? OpenWorldFormatError)?.errorDescription ?? "\(error)"
        XCTAssertTrue(message.contains(fragment), "\"\(message)\" lacks \"\(fragment)\"", file: file, line: line)
    }
}

final class ManifestTests: XCTestCase {
    func testTheExampleManifestParsesAtTheSupportedSchemaVersion() throws {
        let manifest = try parseManifest(try read(hello.appendingPathComponent("manifest.json")))
        XCTAssertEqual(manifest.version, SUPPORTED_SCHEMA_VERSION)
        XCTAssertGreaterThan(manifest.entities.count, 0)
    }

    func testANewerManifestIsRefusedLoudlyPerTheVersioningPolicy() throws {
        let base = try JSONValue(parsing: try read(hello.appendingPathComponent("manifest.json")))
        var newer = base.object ?? [:]
        newer["version"] = .number(Double(SUPPORTED_SCHEMA_VERSION + 1))
        assertThrows(try WorldManifest(json: .object(newer)), containing: "newer than this reader")
    }
}

final class ClassifyTests: XCTestCase {
    func testOpsAreRecognizedByShapeEditsFirst() {
        XCTAssertEqual(classifyOp(.object(["SpawnEntity": .object(["entity": .object(["id": .number(1), "name": .string("a")])])])), .edit(name: "SpawnEntity", value: .object(["entity": .object(["id": .number(1), "name": .string("a")])])))
        XCTAssertEqual(classifyOp(.object(["tool": .string("x"), "args": .object([:])])).kindIfTool, true)
        XCTAssertEqual(classifyOp(.object(["input": .object(["actor": .string("v")])])).kindIfInput, true)
        XCTAssertEqual(classifyOp(.object(["state": .object(["score.x": .number(1)])])).kindIfState, true)
        XCTAssertEqual(classifyOp(.object(["clock": .object(["playing": .bool(true), "position_s": .number(0)])])).kindIfClock, true)
        XCTAssertEqual(classifyOp(.object(["ext-physics": .object(["body": .string("static")])])), .extensionOp(name: "ext-physics", value: .object(["body": .string("static")])))
        XCTAssertEqual(classifyOp(.object(["nope": .number(1)])), .unknown)
    }

    func testAnOldFormatLineEditsOnlyParsesAsEdits() throws {
        let entry = try parseLogLine("""
        {"revision": 7, "author": {"peer": 3, "name": "maya"}, "ops": [{"DeleteEntity": {"id": 1}}], "timestamp_ms": 1}
        """)
        let edits = editOps(entry)
        XCTAssertEqual(edits.count, 1)
        guard case let .edit(name, _) = edits[0] else { return XCTFail("expected an edit") }
        XCTAssertEqual(name, "DeleteEntity")
    }
}

extension ClassifiedOp {
    var kindIfTool: Bool { if case .tool = self { return true }; return false }
    var kindIfInput: Bool { if case .input = self { return true }; return false }
    var kindIfState: Bool { if case .state = self { return true }; return false }
    var kindIfClock: Bool { if case .clock = self { return true }; return false }
}

final class FoldTests: XCTestCase {
    let manifestText = try! read(hello.appendingPathComponent("manifest.json"))
    var entries: [LogEntry] { try! readEntries(hello) }

    func testTheExampleLogFoldsTheLanternAppearsHistoryFoldsToNothing() throws {
        let base = try parseManifest(manifestText)
        let before = base.entities.count
        let state = try foldLog(base, entries)
        // Five entries, one edit op among them: everything else is history.
        XCTAssertEqual(state.appliedEdits, 1)
        XCTAssertEqual(state.entities.count, before + 1)
        let lantern = try XCTUnwrap(state.entities.first { $0.id == 100 })
        XCTAssertEqual(lantern.name, "lantern")
        XCTAssertEqual(lantern.transform?.position, SIMD3(-12.0, 0.0, 3.0))
        // History entries carried the revision without bumping anything:
        XCTAssertFalse(state.entities.contains { $0.id == 101 })
    }

    func testModifyAppliesAPatchAbsentFieldsAreUnchangedNullClears() throws {
        let base = try parseManifest(manifestText)
        let state = try foldLog(base, [try entryOf(#"[{"ModifyEntity": {"id": 1, "patch": {"shape": {"Sphere": {"radius": 0.5}}, "material": null}}}]"#)])
        let ground = try XCTUnwrap(state.entities.first { $0.id == 1 })
        XCTAssertEqual(ground.shape, .sphere(radius: 0.5))
        XCTAssertNil(ground.material)
        XCTAssertEqual(ground.transform?.position, SIMD3(0.0, 0.0, 0.0))  // untouched
    }

    func testDeletingAnEntityDeletesItsDescendants() throws {
        let base = try parseManifest(manifestText)
        let state = try foldLog(base, [
            try entryOf(#"[{"SpawnEntity": {"entity": {"id": 200, "name": "p"}}}, {"SpawnEntity": {"entity": {"id": 201, "name": "c1", "parent": 200}}}, {"SpawnEntity": {"entity": {"id": 202, "name": "c2", "parent": 201}}}]"#),
            try entryOf(#"[{"DeleteEntity": {"id": 200}}]"#, revision: 2, timestampMs: 1),
        ])
        let ids = Set(state.entities.map(\.id))
        XCTAssertFalse(ids.contains(200))
        XCTAssertFalse(ids.contains(201))
        XCTAssertFalse(ids.contains(202))
    }

    func testABatchAppliesAllOrNothing() throws {
        let base = try parseManifest(manifestText)
        let batch = try entryOf(#"[{"Batch": {"ops": [{"SpawnEntity": {"entity": {"id": 300, "name": "ok"}}}, {"DeleteEntity": {"id": 99999}}]}}]"#)
        assertThrows(try foldLog(base, [batch]), containing: "no entity 99999")
    }

    func testStateFoldsOverTheDeclarationToleratingTheUndeclared() throws {
        let stateDoc = StateDocument(json: try JSONValue(parsing: try read(hello.appendingPathComponent("state.json"))))
        let result = foldState(stateDoc, entries)
        // The example's log sets score.tour to 1; the declaration's initial was 0.
        XCTAssertEqual(result.values["score.tour"], .number(1))
        XCTAssertEqual(result.undeclared, [])

        let richer = StateDocument(fields: [
            "score.main": StateField(json: .object(["type": .string("int"), "initial": .number(0)])),
            "inventory": StateField(json: .object(["type": .string("map"), "initial": .object([:])])),
            "has.map": StateField(json: .object(["type": .string("bool"), "initial": .bool(false)])),
        ])
        let ops = [
            #"{"state": {"score.main": 5}}"#,
            #"{"state": {"inventory.rope": 1, "inventory.torch": 2}}"#,
            #"{"state": {"inventory.rope": null}}"#,
            #"{"state": {"has.map": true}}"#,
            #"{"state": {"has.map": null}}"#,
            #"{"state": {"unknown.key": 7}}"#,
            #"{"state": {"unknown.key": null}}"#,
        ]
        let entries = try ops.enumerated().map { i, op in try entryOf("[\(op)]", timestampMs: Double(i)) }
        let folded = foldState(richer, entries)
        XCTAssertEqual(folded.values["score.main"], .number(5))
        XCTAssertEqual(folded.values["inventory"], .object(["torch": .number(2)]))
        XCTAssertEqual(folded.values["has.map"], .bool(false))  // null reset the initial
        XCTAssertNil(folded.values["unknown.key"])              // set, carried, then removed
        XCTAssertEqual(folded.undeclared, ["unknown.key"])
    }

    func testAForkedHistoryFoldsPerTipSamePrefixDifferentWorlds() throws {
        let forked = root.appendingPathComponent("examples/forked-exploration")
        let manifest = try parseManifest(try read(forked.appendingPathComponent("manifest.json")))
        let entries = try readEntries(forked)

        let history = try buildHistory(entries)
        // Two tips: the trunk's garden end, and the moat variant.
        XCTAssertEqual(history.tips.sorted(), ["e3", "e5"])
        // The fork point has both children.
        XCTAssertEqual(history.children["e2"]?.sorted(), ["e3", "e4"])

        let trunk = try foldPath(manifest, entries, tip: "e3")
        var names = Set(trunk.entities.map(\.name))
        XCTAssertTrue(names.contains("garden"))
        XCTAssertFalse(names.contains("moat"))
        XCTAssertEqual(trunk.path, ["e1", "e2", "e3"])

        let variant = try foldPath(manifest, entries, tip: "e5")
        names = Set(variant.entities.map(\.name))
        XCTAssertTrue(names.contains("moat"))
        XCTAssertFalse(names.contains("garden"))
        XCTAssertEqual(variant.path, ["e1", "e2", "e4", "e5"])

        // Default tip is the last entry in file order; the merge record
        // folds to nothing, so the variant's document is unchanged by it.
        let e4 = try foldPath(manifest, entries, tip: "e4")
        XCTAssertEqual(variant.entities.count, e4.entities.count)

        // Unknown tips refuse loudly.
        assertThrows(try foldPath(manifest, entries, tip: "e99"), containing: "no entry 'e99'")
    }

    func testALogWithNoIdsIsAChainAndMixedLogsWork() throws {
        let manifest = try parseManifest(manifestText)
        // The hello-world log has no ids: one tip, the last line.
        let history = try buildHistory(entries)
        XCTAssertEqual(history.tips, ["line-\(entries.count - 1)"])
        let state = try foldPath(manifest, entries)
        XCTAssertEqual(state.entities.count, manifest.entities.count + 1)  // the lantern

        // Mixed: an id-bearing branch grafted onto a synthesized chain.
        let chain = [
            LogEntry(revision: 1, ops: [
                .object(["SpawnEntity": .object(["entity": .object(["id": .number(900), "name": .string("a")])])]),
            ]),
            LogEntry(revision: 1, ops: [
                .object(["SpawnEntity": .object(["entity": .object(["id": .number(901), "name": .string("b")])])]),
            ]),
            LogEntry(revision: 2, ops: [
                .object(["SpawnEntity": .object(["entity": .object(["id": .number(902), "name": .string("c")])])]),
            ], id: "x", parent: "line-1"),
        ]
        let mixed = try buildHistory(chain)
        XCTAssertEqual(mixed.tips, ["x"])
        XCTAssertEqual(mixed.children["line-1"], ["x"])
        let folded = try foldPath(manifest, chain, tip: "x")
        XCTAssertEqual(folded.entities.count, manifest.entities.count + 3)
        XCTAssertEqual(folded.path, ["line-0", "line-1", "x"])
    }

    func testIdentityRefusesDuplicatesAndMissingParents() throws {
        let dup = [
            LogEntry(revision: 1, ops: [], id: "a"),
            LogEntry(revision: 1, ops: [], id: "a"),
        ]
        assertThrows(try buildHistory(dup), containing: "duplicate entry id 'a'")

        let orphan = [
            LogEntry(revision: 1, ops: [], id: "a"),
            LogEntry(revision: 1, ops: [], id: "b", parent: "nope"),
        ]
        assertThrows(try buildHistory(orphan), containing: "names parent 'nope', which isn't in the log")
    }
}
