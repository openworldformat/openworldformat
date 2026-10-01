// Typed views over an entity's component fields.
//
// The fold treats components as opaque (they ride in `fields`); an app
// wants them typed. These accessors decode on demand and stay
// must-ignore-lenient: a component the accessors don't know is simply
// absent from the typed view, never an error.

import Foundation

/// Transform in world space (or parent-relative if parented).
public struct WorldTransform: Equatable, Sendable {
    public var position: SIMD3<Double>?
    public var rotationDegrees: SIMD3<Double>?
    public var scale: SIMD3<Double>?
    public var visible: Bool?

    public init(json: JSONValue) {
        let o = json.object ?? [:]
        position = Self.vec3(o["position"])
        rotationDegrees = Self.vec3(o["rotation_degrees"])
        scale = Self.vec3(o["scale"])
        visible = o["visible"]?.bool
    }

    private static func vec3(_ v: JSONValue?) -> SIMD3<Double>? {
        guard let a = v?.array, a.count >= 3 else { return nil }
        return SIMD3(a[0].double ?? 0, a[1].double ?? 0, a[2].double ?? 0)
    }
}

/// A parametric primitive, externally tagged:
/// `{"Sphere": {"radius": 0.75}}`, `{"Cuboid": {"x": 1, "y": 1, "z": 1}}`, …
/// Unknown shapes decode to nil — the viewer profile's must-ignore.
public enum Shape: Equatable, Sendable {
    case cuboid(x: Double, y: Double, z: Double)
    case sphere(radius: Double)
    case cylinder(radius: Double, height: Double)
    case cone(radius: Double, height: Double)
    case capsule(radius: Double, halfLength: Double)
    case torus(majorRadius: Double, minorRadius: Double)
    case plane(x: Double, z: Double)
    case pyramid(baseX: Double, baseZ: Double, height: Double)
    case tetrahedron(radius: Double)
    case icosahedron(radius: Double)
    case wedge(x: Double, y: Double, z: Double)

    public init?(json: JSONValue) {
        guard let o = json.object, o.count == 1, let (tag, params) = o.first else { return nil }
        let p = params.object ?? [:]
        func d(_ k: String, _ fallback: Double = 1) -> Double { p[k]?.double ?? fallback }
        switch tag {
        case "Cuboid": self = .cuboid(x: d("x"), y: d("y"), z: d("z"))
        case "Sphere": self = .sphere(radius: d("radius", 0.5))
        case "Cylinder": self = .cylinder(radius: d("radius", 0.5), height: d("height"))
        case "Cone": self = .cone(radius: d("radius", 0.5), height: d("height"))
        case "Capsule": self = .capsule(radius: d("radius", 0.5), halfLength: d("half_length"))
        case "Torus": self = .torus(majorRadius: d("major_radius", 0.5), minorRadius: d("minor_radius", 0.2))
        case "Plane": self = .plane(x: d("x"), z: d("z"))
        case "Pyramid": self = .pyramid(baseX: d("base_x"), baseZ: d("base_z"), height: d("height"))
        case "Tetrahedron": self = .tetrahedron(radius: d("radius", 0.5))
        case "Icosahedron": self = .icosahedron(radius: d("radius", 0.5))
        case "Wedge": self = .wedge(x: d("x"), y: d("y"), z: d("z"))
        default: return nil
        }
    }
}

/// A surface: color (rgba, linear), emissive, and the PBR knobs.
public struct MaterialDef: Equatable, Sendable {
    public var color: [Double]?
    public var emissive: [Double]?
    public var roughness: Double?
    public var metallic: Double?
    public var transparency: Double?
    public var doubleSided: Bool?
    /// `base_color_texture` and friends: the asset-referencing fields an
    /// engine resolves against the package's content store.
    public var fields: [String: JSONValue]

    public init(json: JSONValue) {
        let o = json.object ?? [:]
        color = o["color"]?.array?.compactMap(\.double)
        emissive = o["emissive"]?.array?.compactMap(\.double)
        roughness = o["roughness"]?.double
        metallic = o["metallic"]?.double
        transparency = o["transparency"]?.double
        doubleSided = o["double_sided"]?.bool
        fields = o.filter { !["color", "emissive", "roughness", "metallic", "transparency", "double_sided"].contains($0.key) }
    }
}

/// A light: directional, point or spot.
public struct LightDef: Equatable, Sendable {
    public var lightType: String?
    public var color: [Double]?
    public var intensity: Double?
    public var direction: [Double]?
    public var range: Double?
    public var angleDegrees: Double?
    public var shadows: Bool?

    public init(json: JSONValue) {
        let o = json.object ?? [:]
        lightType = o["light_type"]?.string
        color = o["color"]?.array?.compactMap(\.double)
        intensity = o["intensity"]?.double
        direction = o["direction"]?.array?.compactMap(\.double)
        range = o["range"]?.double
        angleDegrees = o["angle_degrees"]?.double
        shadows = o["shadows"]?.bool
    }
}

extension WorldEntity {
    /// The entity's transform, if it declares one.
    public var transform: WorldTransform? {
        fields["transform"].map(WorldTransform.init(json:))
    }

    /// The entity's parametric shape, if it declares one.
    public var shape: Shape? {
        fields["shape"].flatMap(Shape.init(json:))
    }

    /// The entity's material, if it declares one.
    public var material: MaterialDef? {
        fields["material"].map(MaterialDef.init(json:))
    }

    /// The entity's light, if it declares one.
    public var light: LightDef? {
        fields["light"].map(LightDef.init(json:))
    }
}
