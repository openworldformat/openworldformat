// The example packages, folded and replayed — mirrors
// js/test/example.test.mjs and python/tests/test_example.py (the
// physics replay stays in the Rust crate's solver; this is the fold's
// half of the contract).

import XCTest
@testable import OpenWorldFormat

final class DropTests: XCTestCase {
    let drop = root.appendingPathComponent("examples/the-drop-test")

    func testTheDropTestPackageFoldsOneEditEverythingElseIsHistory() throws {
        let package = try WorldPackage(directory: drop)
        XCTAssertEqual(package.base.entities.count, 6)
        XCTAssertEqual(package.manifest.entities.count, 7, "the head is the fold to main")
        let state = try package.folded()
        XCTAssertEqual(state.appliedEdits, 1)
        XCTAssertTrue(state.entities.contains { $0.name == "ball_late" })
        // The recorded run folds to nothing for the document…
        XCTAssertEqual(state.entities.count, 7)
    }

    func testTheSwitchScoreCrossedTheLogAsAClickWould() throws {
        let package = try WorldPackage(directory: drop)
        let folded = package.stateValues()
        XCTAssertEqual(folded.values["score.switch"], .number(10))
        XCTAssertEqual(folded.undeclared, [])
    }
}

final class SpeedrunForkTests: XCTestCase {
    let forkDir = root.appendingPathComponent("examples/speedrun-fork")

    func testAChallengeChainIsAHistoryTwoRunsForkOneCourse() throws {
        let package = try WorldPackage(directory: forkDir)
        let forkManifest = package.base
        let forkEntries = package.entries

        let history = try buildHistory(forkEntries)
        // Two tips — the two runs — and both are children of the course head.
        XCTAssertEqual(history.tips.sorted(), ["e3", "e4"])
        XCTAssertEqual(history.children["e2"]?.sorted(), ["e3", "e4"])

        // The trunk is the course: banner and checkpoint flag, no runs folded in.
        let trunk = try foldPath(forkManifest, forkEntries, tip: "e2")
        let trunkNames = Set(trunk.entities.map(\.name))
        XCTAssertTrue(trunkNames.contains("banner"))
        XCTAssertTrue(trunkNames.contains("checkpoint_flag"))

        // Each run folds the course plus its own inputs and its own time.
        let runs: [String: (field: String, time: Double)] = [
            "e3": ("run.kai", 9.42),
            "e4": ("run.noor", 7.91),
        ]
        let stateDoc = package.stateDocument
        for (tip, run) in runs {
            let runState = try foldPath(forkManifest, forkEntries, tip: tip)
            XCTAssertEqual(runState.entities.count, trunk.entities.count)  // no edits in a run
            let chain = runState.path!.compactMap { history.entry(id: $0)?.entry }
            let folded = foldState(stateDoc, chain)
            XCTAssertEqual(folded.values[run.field], .number(run.time))
            let other = run.field == "run.kai" ? "run.noor" : "run.kai"
            XCTAssertEqual(folded.values[other], .number(0))  // the other run never happened here
            let samples = chain.flatMap(\.ops).filter { op in
                if case .input = classifyOp(op) { return true }
                return false
            }
            XCTAssertEqual(samples.count, 5)  // the playthrough, recorded
        }
    }
}
