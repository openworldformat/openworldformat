// The shared merge corpus (conformance/merge/cases/, spec/session.md
// "The merge rules, exactly"): every case runs — the remap table, the
// rewritten entries and the merged head against the committed expected
// results. The other four references run the same cases in their own
// suites, so the five merges cannot drift apart.

import XCTest
@testable import OpenWorldFormat

final class MergeCorpusTests: XCTestCase {
    /// One case file, decoded.
    private struct MergeCase {
        let name: String
        let base: JSONValue
        let main: [LogEntry]
        let branch: [LogEntry]
        let expectedRemapped: [[Int]]
        let expectedEntries: [JSONValue]
        let expectedHead: String
    }

    private static let caseDir = root.appendingPathComponent("conformance/merge/cases")

    private static func loadCases() throws -> [MergeCase] {
        let files = try FileManager.default.contentsOfDirectory(atPath: caseDir.path)
            .filter { $0.hasSuffix(".json") }
            .sorted()
        return try files.map { file in
            let json = try JSONValue(parsing: read(caseDir.appendingPathComponent(file)))
            let expected = json["expected"]
            return try MergeCase(
                name: json["name"]?.string ?? file,
                base: json["base"] ?? .null,
                main: (json["main"]?.array ?? []).map(corpusEntry),
                branch: (json["branch"]?.array ?? []).map(corpusEntry),
                expectedRemapped: (expected?["remapped"]?.array ?? [])
                    .map { ($0.array ?? []).compactMap(\.int) },
                expectedEntries: expected?["entries"]?.array ?? [],
                expectedHead: expected?["head"]?.string ?? "")
        }
    }

    /// One corpus log entry: the same projection `parseLogLine` reads,
    /// from a JSON tree rather than a line of text.
    private static func corpusEntry(_ json: JSONValue) throws -> LogEntry {
        guard let revision = json["revision"]?.int, let ops = json["ops"]?.array else {
            throw OpenWorldFormatError.parse("corpus entry needs a revision and an ops array")
        }
        return LogEntry(
            revision: revision,
            author: json["author"],
            timestampMs: json["timestamp_ms"]?.double,
            ops: ops,
            id: json["id"]?.string,
            parent: json["parent"]?.string,
            message: json["message"]?.string)
    }

    func testTheCorpusIsPresentAndCoversTheHandwrittenRules() throws {
        let cases = try Self.loadCases()
        let names = Set(cases.map(\.name))
        for required in ["spent-id", "modify-world", "batch", "names"] {
            XCTAssertTrue(names.contains(required), "missing hand-written case \(required)")
        }
        XCTAssertGreaterThanOrEqual(cases.count, 100, "the corpus holds the generated cases too")
    }

    func testEveryCaseMatchesTheCommittedExpectations() throws {
        for mergeCase in try Self.loadCases() {
            do {
                let base = try WorldManifest(json: mergeCase.base)
                let state = try foldLog(base, mergeCase.main)
                let (entries, remapped) = try mergeBranch(state, mergeCase.branch)

                let remapTable = remapped.sorted { $0.key < $1.key }.map { [$0.key, $0.value] }
                XCTAssertEqual(remapTable, mergeCase.expectedRemapped,
                               "case \(mergeCase.name): the remap table")
                // Compared before the head fold: value types mean folding
                // can't mutate the merged entries under the assertion —
                // the JS generator snapshots for exactly that reason —
                // and the order keeps it true by construction.
                XCTAssertEqual(entries.map { entryObject($0, includeId: true) },
                               mergeCase.expectedEntries,
                               "case \(mergeCase.name): the rewritten entries")

                let head = try foldLog(base, mergeCase.main + entries)
                XCTAssertEqual(manifestText(try toManifest(head)), mergeCase.expectedHead,
                               "case \(mergeCase.name): the merged head, as canonical text")
            } catch {
                XCTFail("case \(mergeCase.name): \(error)")
            }
        }
    }
}
