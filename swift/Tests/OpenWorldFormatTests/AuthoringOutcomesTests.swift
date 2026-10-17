// The authoring outcome suite (conformance/authoring/cases.json) through
// this reference's `ingest`: each batch over the base world, and what must
// come back — mirrors rust/tests/authoring_outcomes.rs, the normative
// runner. `ops[].path` is a JSON pointer into the committed op,
// `world[].path` one into the named entity; numbers compare by value.

import Foundation
import XCTest
@testable import OpenWorldFormat

/// A JSON pointer into a value (`/a/0/b`), or nil where it leads nowhere.
private func at(_ value: JSONValue, _ pointer: String) -> JSONValue? {
    var node = value
    for raw in pointer.split(separator: "/", omittingEmptySubsequences: false).dropFirst() {
        let key = raw.replacingOccurrences(of: "~1", with: "/").replacingOccurrences(of: "~0", with: "~")
        switch node {
        case .array(let items):
            guard let i = Int(key), items.indices.contains(i) else { return nil }
            node = items[i]
        case .object(let object):
            guard let child = object[key] else { return nil }
            node = child
        default:
            return nil
        }
    }
    return node
}

/// Numbers compare by value — the suite writes 2, a float field reads back 2.0.
private func same(_ a: JSONValue, _ b: JSONValue) -> Bool {
    switch (a, b) {
    case (.number(let x), .number(let y)):
        return abs(x - y) <= 1e-6 * max(abs(x), abs(y), 1)
    case (.array(let x), .array(let y)):
        return x.count == y.count && zip(x, y).allSatisfy { same($0, $1) }
    case (.object(let x), .object(let y)):
        return x.count == y.count && x.allSatisfy { key, value in y[key].map { same(value, $0) } ?? false }
    default:
        return a == b
    }
}

final class AuthoringOutcomesTests: XCTestCase {
    private func checkWorld(_ name: String, _ state: FoldState, _ predicates: [JSONValue]) {
        for p in predicates {
            let entityName = p["entity"]?.string ?? "?"
            let id = state.nameToId[entityName]
            if p["absent"]?.bool == true {
                XCTAssertNil(id, "\(name): entity '\(entityName)' should be absent")
                continue
            }
            guard let id, let index = state.idToIndex[id] else {
                XCTFail("\(name): no entity '\(entityName)'")
                continue
            }
            let json = state.entities[index].json
            let path = p["path"]?.string ?? ""
            guard let got = at(json, path) else {
                XCTFail("\(name): '\(entityName)' has nothing at \(path): \(json)")
                continue
            }
            let want = p["equals"] ?? .null
            XCTAssertTrue(same(got, want), "\(name): '\(entityName)' at \(path) is \(got), expected \(want)")
        }
    }

    func testAuthoringOutcomesHold() throws {
        let url = root.appendingPathComponent("conformance/authoring/cases.json")
        let suite = try JSONValue(parsing: try String(contentsOf: url, encoding: .utf8))
        // The base as text again, the way a package would hold it.
        let base = try foldLog(try parseManifest(canonicalJson(suite["base"] ?? .null)), [])
        let cases = suite["cases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let expect = c["expect"] ?? .null
            let world = expect["world"]?.array ?? []
            switch (ingest(base, c["batch"] ?? .null), expect["result"]?.string) {
            case (.ingested(let done), "committed"?):
                XCTAssertNil(expect["errors_contain"], "\(name): committed, but the suite expects an error")
                for want in expect["warnings_contain"]?.array ?? [] {
                    let want = want.string ?? ""
                    XCTAssertTrue(
                        done.warnings.contains { $0.contains(want) },
                        "\(name): no warning contains \"\(want)\": \(done.warnings)"
                    )
                }
                for (n, id) in expect["spawned"]?.object ?? [:] {
                    XCTAssertEqual(done.spawned[n], id.int, "\(name): spawned \(n) -> \(done.spawned)")
                }
                for p in expect["ops"]?.array ?? [] {
                    let i = p["op"]?.int ?? 0
                    let path = p["path"]?.string ?? ""
                    guard let got = at(done.ops[i], path) else {
                        XCTFail("\(name): op \(i) has nothing at \(path): \(done.ops[i])")
                        continue
                    }
                    let want = p["equals"] ?? .null
                    XCTAssertTrue(same(got, want), "\(name): op \(i) at \(path) is \(got), expected \(want)")
                }
                checkWorld(name, done.state, world)
            case (.refused(let errors), "refused"?):
                for want in expect["errors_contain"]?.array ?? [] {
                    let want = want.string ?? ""
                    XCTAssertTrue(
                        errors.contains { $0.contains(want) },
                        "\(name): no error contains \"\(want)\": \(errors)"
                    )
                }
                checkWorld(name, base, world)
            case (.ingested, let other):
                XCTFail("\(name): committed, expected \(other ?? "?")")
            case (.refused(let errors), let other):
                XCTFail("\(name): refused (\(errors)), expected \(other ?? "?")")
            }
        }
    }
}
