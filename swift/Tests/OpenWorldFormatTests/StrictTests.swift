// Strict mode — the validator's half of must-ignore — and the
// extension surfaces it polices: the registry mirror and the
// LLM-provenance extension's typed view.

import XCTest
@testable import OpenWorldFormat

/// Build a manifest body as JSON text, one mutation at a time.
func manifestText(_ mutate: (inout [String: JSONValue]) -> Void = { _ in }) throws -> String {
    var o: [String: JSONValue] = [
        "version": .number(3),
        "meta": .object(["name": .string("t")]),
        "entities": .array([.object(["id": .number(1), "name": .string("root")])]),
    ]
    mutate(&o)
    return String(data: try JSONValue.object(o).encoded(), encoding: .utf8)!
}

final class StrictTests: XCTestCase {
    func testTheRegistryMirrorListsTheFiveRegisteredExtensions() {
        XCTAssertEqual(REGISTERED_EXTENSIONS, [
            "ext-physics", "ext-strict-determinism", "ext-visibility",
            "ext-cinematography", "ext-provenance",
        ] as Set)
        XCTAssertEqual(
            EXT_PROVENANCE_FIELDS,
            ["prompt", "model", "generation_duration_ms", "biome", "semantic_category"])
    }

    func testStrictModeAcceptsEverythingTheSchemaNames() throws {
        // One manifest using every allowed key at every level strict
        // checks — including a registered extension at each.
        let text = try manifestText {
            $0["environment"] = .object([:])
            $0["camera"] = .object([:])
            $0["avatar"] = .object([:])
            $0["tours"] = .array([])
            $0["soundtrack"] = .object([:])
            $0["creations"] = .array([])
            $0["next_entity_id"] = .number(2)
            $0["ext-physics"] = .object(["gravity": .number(9.8)])
            $0["meta"] = .object([
                "name": .string("t"),
                "description": .string("d"),
                "time_of_day": .string("dawn"),
                "tags": .array([.string("a")]),
                "source": .string("s"),
                "variation_group": .string("g"),
                "variation": .number(1),
                "style_ref": .string("r"),
                "compliance": .object([:]),
                "ext-provenance": .object(["model": .string("m")]),
            ])
            $0["entities"] = .array([.object([
                "id": .number(1),
                "name": .string("root"),
                "parent": .number(0),
                "transform": .object([:]),
                "chunk": .array([.number(0), .number(0)]),
                "shape": .object([:]),
                "material": .object([:]),
                "light": .object([:]),
                "audio": .object([:]),
                "behaviors": .array([]),
                "modulations": .array([]),
                "triggers": .array([]),
                "mesh_asset": .object([:]),
                "instance_of": .object([:]),
                "creation_id": .number(1),
                "ext-physics": .object([:]),
            ])])
        }
        let manifest = try parseManifest(text, strict: true)
        XCTAssertEqual(manifest.meta?.extProvenance?.model, "m")
    }

    func testStrictModeRefusesUnknownTopLevelKeys() throws {
        let text = try manifestText { $0["mystery"] = .number(1) }
        assertThrows(try parseManifest(text, strict: true), containing: "not a manifest key")
        // Non-strict carries it, exactly as before.
        _ = try parseManifest(text)
    }

    func testStrictModeRefusesUnknownMetaKeys() throws {
        let text = try manifestText { $0["meta"] = .object(["name": .string("t"), "mystery": .number(1)]) }
        assertThrows(try parseManifest(text, strict: true), containing: "not a meta key")
    }

    func testStrictModePointsLegacyProvenanceFieldsAtTheExtension() throws {
        for key in EXT_PROVENANCE_FIELDS {
            let text = try manifestText {
                $0["meta"] = .object(["name": .string("t"), key: .string("x")])
            }
            assertThrows(try parseManifest(text, strict: true), containing: "legacy provenance")
            assertThrows(try parseManifest(text, strict: true), containing: "ext-provenance")
        }
    }

    func testStrictModeRefusesUnknownEntityKeys() throws {
        let text = try manifestText {
            $0["entities"] = .array([.object(["id": .number(1), "name": .string("root"), "mystery": .number(1)])])
        }
        assertThrows(try parseManifest(text, strict: true), containing: "not an entity key")
    }

    func testStrictModeRefusesUnregisteredExtensionsByNamingTheRegistry() throws {
        // Top level, meta, entity — anywhere strict checks.
        let topLevel = try manifestText { $0["ext-mystery"] = .object([:]) }
        assertThrows(try parseManifest(topLevel, strict: true), containing: "not in the extension registry")

        let metaLevel = try manifestText { $0["meta"] = .object(["name": .string("t"), "ext-mystery": .object([:])]) }
        assertThrows(try parseManifest(metaLevel, strict: true), containing: "not in the extension registry")

        let entityLevel = try manifestText {
            $0["entities"] = .array([.object(["id": .number(1), "name": .string("root"), "ext-mystery": .object([:])])])
        }
        assertThrows(try parseManifest(entityLevel, strict: true), containing: "not in the extension registry")
    }

    func testStrictLogLinesRefuseUnknownOpsAndUnregisteredExtensions() throws {
        let lowercased = #"{"revision": 1, "ops": [{"spawnentity": {"entity": {"id": 1, "name": "a"}}}]}"#
        assertThrows(try parseLogLine(lowercased, strict: true), containing: "unrecognized op 'spawnentity'")
        // Non-strict carries it as unknown, exactly as before.
        let carried = try parseLogLine(lowercased)
        guard case .unknown = carried.classified[0] else { return XCTFail("expected unknown") }

        let unregistered = #"{"revision": 1, "ops": [{"ext-mystery": {"x": 1}}]}"#
        assertThrows(try parseLogLine(unregistered, strict: true), containing: "not in the extension registry")

        // Known history kinds and a registered extension parse strict.
        let fine = #"{"revision": 1, "ops": [{"tool": "gen", "args": {}}, {"state": {"score": 1}}, {"clock": {"playing": true}}, {"ext-physics": {"body": "static"}}]}"#
        let entry = try parseLogLine(fine, strict: true)
        XCTAssertEqual(entry.classified.count, 4)
    }

    func testProvenanceRoundTripsThroughTheMetaPassthrough() throws {
        let raw = JSONValue.object([
            "prompt": .string("a quiet beach at dawn"),
            "model": .string("gen-1"),
            "generation_duration_ms": .number(1200),
            "biome": .string("coast"),
            "semantic_category": .string("scene"),
        ])
        let text = try manifestText { $0["meta"] = .object(["name": .string("t"), "ext-provenance": raw]) }
        let manifest = try parseManifest(text, strict: true)
        let meta = try XCTUnwrap(manifest.meta)

        // The passthrough stays the storage; the typed view reads it.
        XCTAssertEqual(meta.fields["ext-provenance"], raw)
        let provenance = try XCTUnwrap(meta.extProvenance)
        XCTAssertEqual(provenance.prompt, "a quiet beach at dawn")
        XCTAssertEqual(provenance.model, "gen-1")
        XCTAssertEqual(provenance.generationDurationMs, 1200)
        XCTAssertEqual(provenance.biome, "coast")
        XCTAssertEqual(provenance.semanticCategory, "scene")
        // And writes it back, absent fields staying absent.
        XCTAssertEqual(provenance.json, raw)
        XCTAssertEqual(ExtProvenance(json: .object(meta.fields)), provenance)
        XCTAssertEqual(ExtProvenance().json, .object([:]))
    }

    func testAManifestWithoutProvenanceReadsNil() throws {
        let manifest = try parseManifest(try manifestText())
        XCTAssertNil(manifest.meta?.extProvenance)
    }
}
