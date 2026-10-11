// The ext-cinematography reference, in Swift — mirrors
// js/test/cinematography.test.mjs, assertion for assertion: the crop
// math, the view, frame membership, origins through parents, aim and
// focus by entity, the shot list — and the conformance outcome
// assertions, which are the extension's definition of "correct".

import XCTest
@testable import OpenWorldFormat

final class CinematographyTests: XCTestCase {
    private func cinemaWorld() throws -> WorldManifest {
        try parseManifest(try read(root.appendingPathComponent("conformance/cinematography.json")))
    }

    private func byName(_ manifest: WorldManifest, _ name: String) -> WorldEntity {
        manifest.entities.first { $0.name == name }!
    }

    private func entity(_ id: Int, _ name: String, _ ext: String) throws -> WorldEntity {
        try WorldEntity(json: JSONValue(parsing:
            #"{"id": \#(id), "name": "\#(name)", "ext-cinematography": \#(ext)}"#))
    }

    func testCameraOfAppliesTheDefaultsPerAbsentFieldAndNilForNonCameras() throws {
        let manifest = try cinemaWorld()
        XCTAssertNil(CameraComponent.of(byName(manifest, "maya")))
        let bare = CameraComponent.of(try entity(1, "c", #"{"camera": {}}"#))!
        XCTAssertEqual(bare.sensorMm, DEFAULT_SENSOR_MM)
        XCTAssertEqual(bare.focalLengthMm, 35)
        XCTAssertEqual(bare.squeeze, 1)
        XCTAssertNil(bare.aspectRatio)
        XCTAssertNil(bare.aimEntity)
        XCTAssertNil(bare.focusEntity)
        // The references read as the document holds them: an id, or a
        // name before ingestion binds it.
        let tracked = CameraComponent.of(byName(manifest, "4A"))!
        XCTAssertEqual(tracked.aimEntity, .id(4))
        XCTAssertEqual(tracked.focusEntity, .id(4))
        let named = CameraComponent.of(try entity(2, "n", #"{"camera": {"aim_entity": "maya"}}"#))!
        XCTAssertEqual(named.aimEntity, .name("maya"))
        // The extension version this file implements.
        XCTAssertEqual(CINEMATOGRAPHY_VERSION, "0.3.0")
    }

    func testFrameOfIsTheNormativeCropMathCroppingTrimsNeverWidens() throws {
        let manifest = try cinemaWorld()
        // Super 35 with no aspect is the whole sensor.
        let full = frameOf(CameraComponent.of(byName(manifest, "2A"))!)
        XCTAssertEqual(full.aspect, 24.89 / 18.66, accuracy: 1e-9)
        // A 2.39 crop on Super 35 trims the height only.
        let cropped = frameOf(CameraComponent.of(byName(manifest, "1A"))!)
        XCTAssertEqual(cropped.widthMm, 24.89)
        XCTAssertLessThan(cropped.heightMm, 18.66)
        XCTAssertEqual(cropped.aspect, 2.39, accuracy: 1e-9)
        // The 2× anamorphic desqueezes, then crops the width.
        let ana = frameOf(CameraComponent.of(byName(manifest, "3A"))!)
        XCTAssertLessThan(ana.widthMm, 24.89 * 2)
        XCTAssertEqual(ana.heightMm, 18.66)
        XCTAssertEqual(ana.aspect, 2.39, accuracy: 1e-9)
    }

    func testViewOfLooksAtTheAimWithPlusYUpElseDownTheLocalMinusZ() throws {
        let manifest = try cinemaWorld()
        let wide = viewOf(byName(manifest, "1A"), CameraComponent.of(byName(manifest, "1A"))!)
        XCTAssertEqual(wide.position, SIMD3(0, 1.6, 6))
        // Aiming at the origin from +Z looks down −Z, tilted slightly down.
        XCTAssertEqual(wide.forward.x, 0, accuracy: 1e-9)
        XCTAssertLessThan(wide.forward.z, 0)
        XCTAssertLessThan(wide.forward.y, 0)
        XCTAssertGreaterThan(abs(wide.up.y), 0.9)  // +Y up

        // No aim: local −Z carried by the entity's rotation. A camera
        // yawed 90° right (intrinsic XYZ) looks down −X.
        let turnedEntity = try WorldEntity(json: JSONValue(parsing: #"""
            {"id": 9, "name": "t", "transform": {"position": [0, 0, 0], "rotation_degrees": [0, 90, 0]},
             "ext-cinematography": {"camera": {}}}
            """#))
        let turned = viewOf(turnedEntity, CameraComponent.of(turnedEntity)!)
        XCTAssertEqual(turned.forward.x, -1, accuracy: 1e-9)
        XCTAssertEqual(turned.forward.y, 0, accuracy: 1e-9)
        XCTAssertEqual(turned.forward.z, 0, accuracy: 1e-9)
    }

    func testProjectPutsTheAimPointAtTheFramesCentre() throws {
        let manifest = try cinemaWorld()
        let single = byName(manifest, "2A")
        let camera = CameraComponent.of(single)!
        let p = project(viewOf(single, camera), frameOf(camera), camera, SIMD3(-1.5, 1.2, 0))
        XCTAssertEqual(p.x, 0, accuracy: 1e-9)
        XCTAssertEqual(p.y, 0, accuracy: 1e-9)
        XCTAssertGreaterThan(p.z, 0)
        XCTAssertTrue(p.inFrame)
    }

    func testWorldOriginComposesThePositionThroughTheParents() throws {
        let manifest = try cinemaWorld()
        // The mug sits on a tray turned 30° on the counter: its origin
        // is its local position rotated and carried, not as written.
        let mug = worldOrigin(byName(manifest, "mug"), in: manifest.entities)
        let rad = Double.pi / 6
        XCTAssertEqual(mug.x, 0.8 + 0.3 * cos(rad), accuracy: 1e-9)
        XCTAssertEqual(mug.y, 1.05, accuracy: 1e-9)
        XCTAssertEqual(mug.z, -1 - 0.3 * sin(rad), accuracy: 1e-9)
        // An unparented entity's origin is its position.
        XCTAssertEqual(worldOrigin(byName(manifest, "maya"), in: manifest.entities), SIMD3(-1.5, 0.85, 0))
    }

    func testAimEntityCentresTheTargetAndFocusEntityMeasuresToItThroughParents() throws {
        let manifest = try cinemaWorld()
        let entities = manifest.entities
        let tracked = byName(manifest, "4A")
        let camera = CameraComponent.of(tracked)!
        let maya = worldOrigin(byName(manifest, "maya"), in: entities)
        XCTAssertEqual(aimPointOf(camera, in: entities), maya)
        let p = project(viewOf(tracked, camera, in: entities), frameOf(camera), camera, maya)
        XCTAssertEqual(p.x, 0, accuracy: 1e-9)
        XCTAssertEqual(p.y, 0, accuracy: 1e-9)
        XCTAssertGreaterThan(p.z, 0)
        XCTAssertEqual(focusDistanceOf(tracked, camera, in: entities)!, 4.3843, accuracy: 1e-3)
        // The insert centres the parented mug's world origin and focuses on it.
        let insert = byName(manifest, "insert")
        let macro = CameraComponent.of(insert)!
        let q = project(viewOf(insert, macro, in: entities), frameOf(macro), macro,
                        worldOrigin(byName(manifest, "mug"), in: entities))
        XCTAssertEqual(q.x, 0, accuracy: 1e-9)
        XCTAssertEqual(q.y, 0, accuracy: 1e-9)
        XCTAssertGreaterThan(q.z, 0)
        XCTAssertEqual(focusDistanceOf(insert, macro, in: entities)!, 1.4956, accuracy: 1e-3)
        // Without the world only `aim` can answer: the tracked camera has none.
        XCTAssertNil(aimPointOf(camera))
    }

    func testAGoneTargetLeavesAimThenTheRotationAndFocusDistanceM() throws {
        let manifest = try cinemaWorld()
        let orphan = try WorldEntity(json: JSONValue(parsing: #"""
            {"id": 40, "name": "orphan", "transform": {"position": [0, 1, 5]},
             "ext-cinematography": {"camera": {"aim_entity": 999, "aim": [0, 1, 0],
                                               "focus_entity": 999, "focus_distance_m": 2.5}}}
            """#))
        let camera = CameraComponent.of(orphan)!
        XCTAssertEqual(aimPointOf(camera, in: manifest.entities), SIMD3(0, 1, 0))
        XCTAssertEqual(focusDistanceOf(orphan, camera, in: manifest.entities), 2.5)
        XCTAssertEqual(viewOf(orphan, camera, in: manifest.entities).forward.z, -1, accuracy: 1e-9)
    }

    func testShotListOrdersByShotOrderTiesByEntityIdAndSkipsShotLessCameras() throws {
        let manifest = try cinemaWorld()
        XCTAssertEqual(shotList(manifest.entities).map(\.name), ["1A", "2A", "2B", "3A", "4A"])  // bts and insert carry no shot
        // Absent orders sort last; ties break by id.
        let world = [
            try entity(7, "b", #"{"shot": {}}"#),
            try entity(3, "a", #"{"shot": {}}"#),
            try entity(5, "c", #"{"shot": {"order": 1}}"#),
        ]
        XCTAssertEqual(shotList(world).map(\.name), ["c", "a", "b"])
    }

    func testTheCinematographyConformanceOutcomesPassUnderTheReferenceMath() throws {
        let manifest = try cinemaWorld()
        let outcomes = try JSONValue(parsing:
            try read(root.appendingPathComponent("conformance/outcomes/cinematography.json")))
        let result = runCinematographyOutcomes(manifest.entities, outcomes)
        XCTAssertEqual(result.failures, [])
        XCTAssertTrue(result.ok)
    }
}
