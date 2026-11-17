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
    /// A right prism on a regular polygon (spec/world.md "The geometry of the
    /// shapes"): `sides` 3…8, `radius` the circumradius, axis along +Y.
    case prism(sides: Int, radius: Double, height: Double)

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
        case "Prism": self = .prism(sides: Int(d("sides", 6).rounded()), radius: d("radius", 0.5), height: d("height"))
        default: return nil
        }
    }
}

/// How a material draws its colour's alpha (`alpha_mode`).
public enum AlphaMode: Equatable, Sendable {
    /// The alpha is ignored: drawn opaque.
    case opaque
    /// Blended over what is behind it by its alpha.
    case blend
    /// Added to what is behind it.
    case add
    /// Multiplied with what is behind it.
    case multiply
    /// Cut at `cutoff`: opaque where the alpha reaches it, absent below.
    case mask(cutoff: Double)

    /// `"opaque"`, `"blend"`, `"add"`, `"multiply"` or `{"mask": cutoff}`;
    /// nil for anything else — absent, null, or a mode this reader does
    /// not know, where the colour's alpha decides (`drawnAlphaMode`).
    public init?(json: JSONValue?) {
        switch json {
        case .string("opaque")?: self = .opaque
        case .string("blend")?: self = .blend
        case .string("add")?: self = .add
        case .string("multiply")?: self = .multiply
        case .object(let o)? where o.count == 1:
            guard let cutoff = o["mask"]?.double else { return nil }
            self = .mask(cutoff: cutoff)
        default: return nil
        }
    }
}

/// A surface: colour, emissive, and the PBR knobs. `color` is
/// sRGB-encoded RGBA in 0…1, its alpha the opacity; `emissive` is
/// linear, and above 1 it glows (spec/world.md "Conventions").
public struct MaterialDef: Equatable, Sendable {
    public var color: [Double]?
    public var emissive: [Double]?
    public var roughness: Double?
    public var metallic: Double?
    public var transparency: Double?
    public var doubleSided: Bool?
    /// `alpha_mode` as declared: nil when absent, null, or a mode this
    /// reader does not know. `drawnAlphaMode` is what to draw.
    public var alphaMode: AlphaMode?
    /// Ignores lighting: drawn flat in its colour.
    public var unlit: Bool?
    /// Reflectance at normal incidence, 0…1 (0.5 is 4%).
    public var reflectance: Double?
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
        alphaMode = AlphaMode(json: o["alpha_mode"])
        unlit = o["unlit"]?.bool
        reflectance = o["reflectance"]?.double
        fields = o.filter {
            !["color", "emissive", "roughness", "metallic", "transparency", "double_sided",
              "alpha_mode", "unlit", "reflectance"].contains($0.key)
        }
    }

    /// The colour's alpha — its opacity — or 1 when the colour declares
    /// none.
    public var alpha: Double {
        guard let color, color.count > 3 else { return 1 }
        return color[3]
    }

    /// How the material draws (spec/world.md "Conventions"): `alpha_mode`
    /// when declared; without it, a colour whose alpha is below 1 draws
    /// blended, as `blend` would, and one at 1 is opaque.
    public var drawnAlphaMode: AlphaMode {
        alphaMode ?? (alpha < 1 ? .blend : .opaque)
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
    /// A spot light's inner cone: radians from its direction, full
    /// intensity inside it.
    public var innerAngle: Double?
    /// A spot light's outer cone: radians from its direction, no light
    /// past it.
    public var outerAngle: Double?
    public var shadows: Bool?

    public init(json: JSONValue) {
        let o = json.object ?? [:]
        lightType = o["light_type"]?.string
        color = o["color"]?.array?.compactMap(\.double)
        intensity = o["intensity"]?.double
        direction = o["direction"]?.array?.compactMap(\.double)
        range = o["range"]?.double
        angleDegrees = o["angle_degrees"]?.double
        innerAngle = o["inner_angle"]?.double
        outerAngle = o["outer_angle"]?.double
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
