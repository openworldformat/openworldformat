// Materials and lights as conformance/materials.json and lights.json
// declare them: the alpha modes, a translucent colour with no mode — which
// draws blended (spec/world.md "Conventions") — unlit, double-sided,
// reflectance, and a spot light's cone.

import XCTest
@testable import OpenWorldFormat

final class MaterialTests: XCTestCase {
    private func world(_ name: String) throws -> WorldManifest {
        try parseManifest(try read(root.appendingPathComponent("conformance/\(name).json")))
    }

    private func material(_ world: WorldManifest, _ name: String) throws -> MaterialDef {
        try XCTUnwrap(world.entities.first { $0.name == name }?.material, "\(name) has a material")
    }

    func testEachMaterialDrawsTheAlphaModeTheConventionsSay() throws {
        let world = try world("materials")
        // A translucent colour with no mode draws blended.
        let translucent = try material(world, "translucent")
        XCTAssertNil(translucent.alphaMode)
        XCTAssertEqual(translucent.alpha, 0.5)
        XCTAssertEqual(translucent.drawnAlphaMode, .blend)
        // A colour at 1 with no mode is opaque.
        XCTAssertEqual(try material(world, "m0_r01").drawnAlphaMode, .opaque)
        // A mode set says what it says.
        XCTAssertEqual(try material(world, "blend").drawnAlphaMode, .blend)
        XCTAssertEqual(try material(world, "add").drawnAlphaMode, .add)
        XCTAssertEqual(try material(world, "mask").drawnAlphaMode, .mask(cutoff: 0.5))
        XCTAssertEqual(try material(world, "opaque_scaled").drawnAlphaMode, .opaque)
        // The other features read typed, and leave the passthrough.
        XCTAssertEqual(try material(world, "unlit").unlit, true)
        XCTAssertEqual(try material(world, "double_sided").doubleSided, true)
        XCTAssertEqual(try material(world, "reflectance").reflectance, 0.9)
        XCTAssertNil(try material(world, "blend").fields["alpha_mode"])
        XCTAssertEqual(try material(world, "emissive").emissive, [3.0, 0.8, 0.2, 1.0])
    }

    func testAModeThisReaderDoesNotKnowLeavesTheAlphaToDecide() throws {
        let screen = MaterialDef(json: try JSONValue(parsing: #"{"color": [1, 1, 1, 0.3], "alpha_mode": "screen"}"#))
        XCTAssertNil(screen.alphaMode)
        XCTAssertEqual(screen.drawnAlphaMode, .blend)
        let opaque = MaterialDef(json: try JSONValue(parsing: #"{"alpha_mode": "opaque", "color": [1, 1, 1, 0.3]}"#))
        XCTAssertEqual(opaque.drawnAlphaMode, .opaque)
        XCTAssertEqual(MaterialDef(json: .object([:])).drawnAlphaMode, .opaque)
        XCTAssertNil(AlphaMode(json: try JSONValue(parsing: #"{"mask": "half"}"#)))
        XCTAssertNil(AlphaMode(json: .null))
    }

    func testASpotLightsConeIsItsInnerAndOuterAngleInRadians() throws {
        let world = try world("lights")
        let spot = try XCTUnwrap(world.entities.first { $0.name == "spot_light" }?.light)
        XCTAssertEqual(spot.lightType, "spot")
        XCTAssertEqual(spot.innerAngle, 0.25)
        XCTAssertEqual(spot.outerAngle, 0.45)
        XCTAssertEqual(spot.direction, [0, -1, 0])
        let point = try XCTUnwrap(world.entities.first { $0.name == "point_light" }?.light)
        XCTAssertNil(point.outerAngle)
    }
}
