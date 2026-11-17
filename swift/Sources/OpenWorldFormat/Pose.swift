// Poses: where an entity stands, as numbers a renderer applies — the
// transform convention of spec/world.md "Conventions", stated once in
// code. Positions are metres, Y up; rotations are intrinsic XYZ Euler
// angles in degrees, R = Rx·Ry·Rz, so a vector turns about Z first, then
// Y, then X; a parented entity's transform is relative to its parent,
// and a point of its local frame lands at P + R·(S ⊙ p) in the parent's.
//
// Pure Swift, no simd import, so the package stays Foundation-only. An
// Apple app hands `Quaternion.vector` to `simd_quatd(vector:)` (or
// `simd_quatf`), which takes the same (x, y, z, w) order.

import Foundation

/// A rotation as a unit quaternion: `(x, y, z)` the vector part, `w` the
/// scalar part.
public struct Quaternion: Equatable, Sendable {
    public var x: Double
    public var y: Double
    public var z: Double
    public var w: Double

    public init(x: Double, y: Double, z: Double, w: Double) {
        self.x = x
        self.y = y
        self.z = z
        self.w = w
    }

    /// No turn.
    public static let identity = Quaternion(x: 0, y: 0, z: 0, w: 1)

    /// A right-handed turn of `radians` about `axis`, which needn't be
    /// unit length. A zero axis is no turn.
    public init(axis: SIMD3<Double>, radians: Double) {
        let length = (axis * axis).sum().squareRoot()
        guard length > 0, length.isFinite, radians.isFinite else {
            self = .identity
            return
        }
        let s = sin(radians / 2) / length
        self.init(x: axis.x * s, y: axis.y * s, z: axis.z * s, w: cos(radians / 2))
    }

    /// Intrinsic XYZ Euler angles in degrees, the format's convention:
    /// R = Rx·Ry·Rz (spec/world.md "Conventions";
    /// conformance/hierarchy_rotations.json pins it).
    public init(eulerDegrees degrees: SIMD3<Double>) {
        let d = Double.pi / 180
        self = Quaternion(axis: SIMD3(1, 0, 0), radians: degrees.x * d)
            * Quaternion(axis: SIMD3(0, 1, 0), radians: degrees.y * d)
            * Quaternion(axis: SIMD3(0, 0, 1), radians: degrees.z * d)
    }

    /// The rotation whose matrix has these orthonormal columns — where it
    /// sends the local X, Y and Z axes.
    public init(columns x: SIMD3<Double>, _ y: SIMD3<Double>, _ z: SIMD3<Double>) {
        // Shepperd's method: divide by the largest of the four diagonal
        // combinations, never by something near zero.
        let trace = x.x + y.y + z.z
        if trace > 0 {
            let s = (trace + 1).squareRoot() * 2
            self.init(x: (y.z - z.y) / s, y: (z.x - x.z) / s, z: (x.y - y.x) / s, w: s / 4)
        } else if x.x > y.y && x.x > z.z {
            let s = (1 + x.x - y.y - z.z).squareRoot() * 2
            self.init(x: s / 4, y: (y.x + x.y) / s, z: (z.x + x.z) / s, w: (y.z - z.y) / s)
        } else if y.y > z.z {
            let s = (1 + y.y - x.x - z.z).squareRoot() * 2
            self.init(x: (y.x + x.y) / s, y: s / 4, z: (z.y + y.z) / s, w: (z.x - x.z) / s)
        } else {
            let s = (1 + z.z - x.x - y.y).squareRoot() * 2
            self.init(x: (z.x + x.z) / s, y: (z.y + y.z) / s, z: s / 4, w: (x.y - y.x) / s)
        }
    }

    /// The rotation that turns local +Z toward `direction`, keeping local
    /// +Y as near world `up` as it can — how the format's reference
    /// renderer points an entity at something (three.js `Object3D.lookAt`
    /// for an object: its +Z faces the target, the side a glTF asset
    /// fronts). A zero direction faces +Z; a direction along `up` is
    /// nudged off it, as that renderer does.
    public init(lookingAlong direction: SIMD3<Double>, up: SIMD3<Double> = SIMD3(0, 1, 0)) {
        var z = direction
        if (z * z).sum() == 0 { z = SIMD3(0, 0, 1) }
        z = normalized(z)
        var x = cross(up, z)
        if (x * x).sum() == 0 {
            if abs(up.z) == 1 { z.x += 0.0001 } else { z.z += 0.0001 }
            z = normalized(z)
            x = cross(up, z)
        }
        x = normalized(x)
        self.init(columns: x, cross(z, x), z)
    }

    /// `(x, y, z, w)` — the order `simd_quatd(vector:)` takes.
    public var vector: SIMD4<Double> { SIMD4(x, y, z, w) }

    /// The composition: `(a * b).act(v) == a.act(b.act(v))`.
    public static func * (a: Quaternion, b: Quaternion) -> Quaternion {
        Quaternion(
            x: a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
            y: a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
            z: a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
            w: a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z)
    }

    /// The turn back.
    public var inverse: Quaternion {
        let n = x * x + y * y + z * z + w * w
        guard n > 0 else { return .identity }
        return Quaternion(x: -x / n, y: -y / n, z: -z / n, w: w / n)
    }

    /// `v` turned by this rotation.
    public func act(_ v: SIMD3<Double>) -> SIMD3<Double> {
        let u = SIMD3(x, y, z)
        let t = cross(u, v) * 2
        return v + t * w + cross(u, t)
    }

    /// The angle, in radians, of the turn from this rotation to `other`:
    /// 0 for the same rotation, whichever sign either quaternion carries.
    public func angle(to other: Quaternion) -> Double {
        let dot = abs(x * other.x + y * other.y + z * other.z + w * other.w)
        let a = (x * x + y * y + z * z + w * w).squareRoot()
        let b = (other.x * other.x + other.y * other.y + other.z * other.z + other.w * other.w).squareRoot()
        return 2 * acos(min(1, dot / (a * b)))
    }
}

/// An entity's pose: position, rotation and scale relative to its parent
/// — its transform as the numbers a renderer sets.
public struct Pose: Equatable, Sendable {
    public var position: SIMD3<Double>
    public var rotation: Quaternion
    public var scale: SIMD3<Double>

    public init(
        position: SIMD3<Double> = SIMD3(0, 0, 0),
        rotation: Quaternion = .identity,
        scale: SIMD3<Double> = SIMD3(1, 1, 1)
    ) {
        self.position = position
        self.rotation = rotation
        self.scale = scale
    }

    /// The authored pose: `transform` as written, the defaults where it is
    /// silent — the origin, no turn, unit scale.
    public init(_ transform: WorldTransform?) {
        self.init(
            position: transform?.position ?? SIMD3(0, 0, 0),
            rotation: transform?.rotation ?? .identity,
            scale: transform?.scale ?? SIMD3(1, 1, 1))
    }

    /// A point of the entity's local frame — the frame its shape is
    /// defined in — in its parent's frame: P + R·(S ⊙ p).
    public func apply(_ point: SIMD3<Double>) -> SIMD3<Double> {
        position + rotation.act(scale * point)
    }
}

extension WorldTransform {
    /// `rotation_degrees` as a rotation — intrinsic XYZ, the format's
    /// convention — or no turn when the transform declares none.
    public var rotation: Quaternion {
        rotationDegrees.map(Quaternion.init(eulerDegrees:)) ?? .identity
    }
}

private func normalized(_ v: SIMD3<Double>) -> SIMD3<Double> {
    let length = (v * v).sum().squareRoot()
    return length > 0 ? v / length : v
}

private func cross(_ a: SIMD3<Double>, _ b: SIMD3<Double>) -> SIMD3<Double> {
    SIMD3(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)
}
