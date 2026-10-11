// The ext-cinematography reference implementation (extension version
// 0.3), in Swift — the same surface as `openworldformat/cinematography`
// (js/src/cinematography.js) and the Rust and Python modules, running
// the same conformance outcome assertions.
//
// Spec: spec/extensions/cinematography.md. An entity carrying
// `ext-cinematography.camera` is a camera — a filmback and a lens — and
// one also carrying `ext-cinematography.shot` is a setup in the shot
// list. What this file provides is the contract's executable half:
//
//   - the normative crop math (frame size, horizontal and vertical FOV)
//     — Unreal's crop-to-aspect rule written out;
//   - the view (aim look-at, +Y up, else the entity's local −Z) and a
//     pinhole projection, so "is it in frame" is a predicate;
//   - entity origins in world space (the document's TRS hierarchy), the
//     point every assertion projects and the point `aim_entity` and
//     `focus_entity` resolve to;
//   - the shot list (shot.order, ties by entity id);
//   - the outcome runner for conformance/outcomes/*.json.
//
// Conformance is math, not pixels: implementations agree on the
// numbers, within the stated tolerance — never on rendered output.

import Foundation

/// The extension this file implements.
public let CINEMATOGRAPHY_EXTENSION = "ext-cinematography"

/// The extension version this file implements.
public let CINEMATOGRAPHY_VERSION = "0.3.0"

/// The default sensor (filmback): Super 35, [w, h] in mm.
public let DEFAULT_SENSOR_MM = SIMD2<Double>(24.89, 18.66)

/// The default focal length, in mm.
public let DEFAULT_FOCAL_LENGTH_MM = 35.0

/// An entity reference as a camera's `aim_entity` / `focus_entity`
/// hold it: the id in a document, a name at intake.
public enum CameraEntityRef: Equatable, Sendable {
    case id(Int)
    case name(String)

    init?(json: JSONValue?) {
        guard let json else { return nil }
        if let id = json.int { self = .id(id) }
        else if let name = json.string { self = .name(name) }
        else { return nil }
    }
}

/// An entity's `ext-cinematography.camera` component with every default
/// applied.
public struct CameraComponent: Equatable, Sendable {
    /// Sensor (filmback) [w, h], mm.
    public var sensorMm: SIMD2<Double>
    /// The lens, mm.
    public var focalLengthMm: Double
    /// Crop to this frame aspect (w/h); nil keeps the desqueezed sensor's.
    public var aspectRatio: Double?
    /// Anamorphic desqueeze.
    public var squeeze: Double
    /// Look at this world point, +Y up; nil looks down the entity's
    /// local −Z.
    public var aim: SIMD3<Double>?
    /// Look at this entity's origin, over `aim`, while it exists.
    public var aimEntity: CameraEntityRef?
    /// Focus distance, m.
    public var focusDistanceM: Double?
    /// Focus on this entity: the distance to its origin, over
    /// `focusDistanceM`, while it exists.
    public var focusEntity: CameraEntityRef?
    /// The f-stop.
    public var fStop: Double?

    /// The spec's `{}`: a 35 mm on Super 35, no crop, no squeeze, no aim.
    public init(
        sensorMm: SIMD2<Double> = DEFAULT_SENSOR_MM,
        focalLengthMm: Double = DEFAULT_FOCAL_LENGTH_MM,
        aspectRatio: Double? = nil,
        squeeze: Double = 1,
        aim: SIMD3<Double>? = nil,
        aimEntity: CameraEntityRef? = nil,
        focusDistanceM: Double? = nil,
        focusEntity: CameraEntityRef? = nil,
        fStop: Double? = nil
    ) {
        self.sensorMm = sensorMm
        self.focalLengthMm = focalLengthMm
        self.aspectRatio = aspectRatio
        self.squeeze = squeeze
        self.aim = aim
        self.aimEntity = aimEntity
        self.focusDistanceM = focusDistanceM
        self.focusEntity = focusEntity
        self.fStop = fStop
    }

    /// The component an entity carries, defaults per absent field —
    /// nil when the entity declares no camera.
    public static func of(_ entity: WorldEntity) -> CameraComponent? {
        guard let camera = entity.fields[CINEMATOGRAPHY_EXTENSION]?["camera"]?.object else {
            return nil
        }
        let sensor = camera["sensor_mm"]?.array?.compactMap(\.double)
        return CameraComponent(
            sensorMm: sensor.flatMap { $0.count == 2 ? SIMD2($0[0], $0[1]) : nil } ?? DEFAULT_SENSOR_MM,
            focalLengthMm: camera["focal_length_mm"]?.double ?? DEFAULT_FOCAL_LENGTH_MM,
            aspectRatio: camera["aspect_ratio"]?.double,
            squeeze: camera["squeeze"]?.double ?? 1,
            aim: vec3(camera["aim"]),
            aimEntity: CameraEntityRef(json: camera["aim_entity"]),
            focusDistanceM: camera["focus_distance_m"]?.double,
            focusEntity: CameraEntityRef(json: camera["focus_entity"]),
            fStop: camera["f_stop"]?.double)
    }
}

/// A camera's frame: the largest rectangle of the aspect inside the
/// desqueezed sensor, centred, and the fields of view it gives the lens.
public struct CameraFrame: Equatable, Sendable {
    public var widthMm: Double
    public var heightMm: Double
    public var hfovDegrees: Double
    public var vfovDegrees: Double
    public var aspect: Double
}

/// The normative crop math (spec "Derived values"): with sensor `w × h`,
/// squeeze `s`, focal length `f` and aspect `a` — desqueezed sensor
/// aspect `A = w·s / h`; frame `W = w·s · min(1, a/A)`,
/// `H = h · min(1, A/a)` (no aspect: the whole desqueezed sensor); FOVs
/// `2·atan(W / 2f)`, `2·atan(H / 2f)`. Cropping never widens the frame
/// past the sensor, it only trims.
public func frameOf(_ camera: CameraComponent) -> CameraFrame {
    let w = camera.sensorMm.x, h = camera.sensorMm.y
    let s = camera.squeeze, f = camera.focalLengthMm
    let full = w * s / h
    var widthMm = w * s, heightMm = h
    if let a = camera.aspectRatio, a > 0 {
        widthMm = w * s * min(1, a / full)
        heightMm = h * min(1, full / a)
    }
    let deg = { (rad: Double) in rad * 180 / .pi }
    return CameraFrame(
        widthMm: widthMm,
        heightMm: heightMm,
        hfovDegrees: deg(2 * atan(widthMm / (2 * f))),
        vfovDegrees: deg(2 * atan(heightMm / (2 * f))),
        aspect: widthMm / heightMm)
}

/// A camera's view: where it stands and which way it looks.
public struct CameraView: Equatable, Sendable {
    public var position: SIMD3<Double>
    public var forward: SIMD3<Double>
    public var right: SIMD3<Double>
    public var up: SIMD3<Double>
}

/// A world point in a camera's normalized frame: `x` and `y` in
/// frame-half units (inside when `|x| <= 1` and `|y| <= 1`), `z` the
/// distance along the look direction (in front when positive).
public struct Projection: Equatable, Sendable {
    public var x: Double
    public var y: Double
    public var z: Double

    /// In front of the camera and inside both frame halves.
    public var inFrame: Bool { z > 0 && abs(x) <= 1 && abs(y) <= 1 }
}

private func vec3(_ json: JSONValue?) -> SIMD3<Double>? {
    guard let a = json?.array?.compactMap(\.double), a.count == 3 else { return nil }
    return SIMD3(a[0], a[1], a[2])
}

private func normalized(_ v: SIMD3<Double>) -> SIMD3<Double> {
    let l = (v * v).sum().squareRoot()
    return v / l
}

private func cross(_ a: SIMD3<Double>, _ b: SIMD3<Double>) -> SIMD3<Double> {
    SIMD3(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)
}

private func dot(_ a: SIMD3<Double>, _ b: SIMD3<Double>) -> Double {
    (a * b).sum()
}

/// Rotate `v` by intrinsic XYZ Euler degrees — R = Rx·Ry·Rz, the
/// transform convention of spec/world.md "Conventions".
private func rotateIntrinsicXyz(_ v: SIMD3<Double>, _ degrees: SIMD3<Double>) -> SIMD3<Double> {
    let rx = degrees.x * .pi / 180, ry = degrees.y * .pi / 180, rz = degrees.z * .pi / 180
    var x = v.x, y = v.y, z = v.z
    // Rz first (rightmost), then Ry, then Rx — R = Rx·Ry·Rz applied to v.
    var c = cos(rz), s = sin(rz)
    (x, y) = (c * x - s * y, s * x + c * y)
    c = cos(ry); s = sin(ry)
    (x, z) = (c * x + s * z, -s * x + c * z)
    c = cos(rx); s = sin(rx)
    (y, z) = (c * y - s * z, s * y + c * z)
    return SIMD3(x, y, z)
}

/// The entity a reference names, among `entities`: by id (a bound
/// reference, what a document holds) or by name (an authoring-time
/// one, before ingestion binds it). nil when nothing answers.
public func entityOf(_ reference: CameraEntityRef?, in entities: [WorldEntity]) -> WorldEntity? {
    switch reference {
    case .id(let id)?: return entities.first { $0.id == id }
    case .name(let name)?: return entities.first { $0.name == name }
    case nil: return nil
    }
}

/// An entity's origin in world space: its `transform.position` composed
/// through its parents — the document's TRS hierarchy, `p = P + R·(S ⊙ p)`
/// up the chain (spec/world.md "Conventions"). An unparented entity's
/// origin is its position as written; a parent the world doesn't hold
/// ends the chain. This is the point every outcome assertion projects
/// and the point `aim_entity` and `focus_entity` resolve to.
public func worldOrigin(_ entity: WorldEntity, in entities: [WorldEntity]) -> SIMD3<Double> {
    var point = entity.transform?.position ?? SIMD3(0, 0, 0)
    var current = entity
    for _ in 0..<4096 {
        guard let parentId = current.parent,
              let parent = entities.first(where: { $0.id == parentId }) else { break }
        let t = parent.transform
        let p = t?.position ?? SIMD3(0, 0, 0)
        let r = t?.rotationDegrees ?? SIMD3(0, 0, 0)
        let s = t?.scale ?? SIMD3(1, 1, 1)
        point = p + rotateIntrinsicXyz(point * s, r)
        current = parent
    }
    return point
}

/// The point a camera aims at: `aim_entity`'s world origin while that
/// entity is among `entities`, else `aim`, else nil (the camera looks
/// down its local −Z). Without entities only `aim` can answer.
public func aimPointOf(_ camera: CameraComponent, in entities: [WorldEntity]? = nil) -> SIMD3<Double>? {
    if let entities, let target = entityOf(camera.aimEntity, in: entities) {
        return worldOrigin(target, in: entities)
    }
    return camera.aim
}

/// A camera's focus distance, metres: the distance from the camera
/// entity's world origin to `focus_entity`'s while that entity exists,
/// else `focus_distance_m`, else nil.
public func focusDistanceOf(
    _ entity: WorldEntity, _ camera: CameraComponent, in entities: [WorldEntity]? = nil
) -> Double? {
    if let entities, let target = entityOf(camera.focusEntity, in: entities) {
        let d = worldOrigin(target, in: entities) - worldOrigin(entity, in: entities)
        return (d * d).sum().squareRoot()
    }
    return camera.focusDistanceM
}

/// A camera's view: with an aim point — `aim_entity`'s origin when
/// `entities` is given and the entity exists, else `aim` — the camera
/// looks at it, +Y up; without one it looks down the entity's local −Z
/// (the glTF / three.js / RealityKit convention), its frame carried by
/// the entity's rotation. The camera stands at its own world origin.
public func viewOf(
    _ entity: WorldEntity, _ camera: CameraComponent, in entities: [WorldEntity]? = nil
) -> CameraView {
    let position = entities.map { worldOrigin(entity, in: $0) }
        ?? entity.transform?.position ?? SIMD3(0, 0, 0)
    if let aim = aimPointOf(camera, in: entities) {
        let forward = normalized(aim - position)
        let right = normalized(cross(forward, SIMD3(0, 1, 0)))
        let up = cross(right, forward)
        return CameraView(position: position, forward: forward, right: right, up: up)
    }
    let rotation = entity.transform?.rotationDegrees ?? SIMD3(0, 0, 0)
    return CameraView(
        position: position,
        forward: rotateIntrinsicXyz(SIMD3(0, 0, -1), rotation),
        right: rotateIntrinsicXyz(SIMD3(1, 0, 0), rotation),
        up: rotateIntrinsicXyz(SIMD3(0, 1, 0), rotation))
}

/// Project a world point through a camera (see `Projection`).
public func project(
    _ view: CameraView, _ frame: CameraFrame, _ camera: CameraComponent, _ point: SIMD3<Double>
) -> Projection {
    let d = point - view.position
    let z = dot(d, view.forward)
    let scale = camera.focalLengthMm / z
    return Projection(
        x: (dot(d, view.right) * scale) / (frame.widthMm / 2),
        y: (dot(d, view.up) * scale) / (frame.heightMm / 2),
        z: z)
}

/// A setup in the shot list: an entity carrying `ext-cinematography.shot`.
public struct ShotEntry: Equatable, Sendable {
    /// The entity's id.
    public var id: Int
    /// The entity's name — the shot's name.
    public var name: String
    /// The `shot` object, as declared.
    public var shot: [String: JSONValue]
}

/// The shot list: every entity carrying `ext-cinematography.shot`,
/// ordered by `shot.order` (absent last), ties by entity id.
public func shotList(_ entities: [WorldEntity]) -> [ShotEntry] {
    let shots = entities.compactMap { entity -> ShotEntry? in
        guard let shot = entity.fields[CINEMATOGRAPHY_EXTENSION]?["shot"]?.object else { return nil }
        return ShotEntry(id: entity.id, name: entity.name, shot: shot)
    }
    let rank = { (s: ShotEntry) -> Double in s.shot["order"]?.double ?? .greatestFiniteMagnitude }
    return shots.sorted { a, b in
        let (ra, rb) = (rank(a), rank(b))
        return ra != rb ? ra < rb : a.id < b.id
    }
}

/// An outcomes run's verdict: every assertion held, else one line per
/// failure.
public struct OutcomeResult: Equatable, Sendable {
    public var ok: Bool
    public var failures: [String]
}

/// Run a conformance outcomes document (`{"expect": [...]}`) against a
/// world's entities: derive, then check every assertion — `fov`,
/// `in_frame`, `out_of_frame`, `centred`, `focus`, `shot_list`. This is
/// the extension's conformance: math, not pixels.
public func runCinematographyOutcomes(_ entities: [WorldEntity], _ outcomes: JSONValue) -> OutcomeResult {
    var failures: [String] = []
    func cameraEntity(_ name: String) -> (entity: WorldEntity, camera: CameraComponent, frame: CameraFrame, view: CameraView)? {
        guard let entity = entities.first(where: { $0.name == name }),
              let camera = CameraComponent.of(entity) else {
            failures.append("no camera named \(name)")
            return nil
        }
        return (entity, camera, frameOf(camera), viewOf(entity, camera, in: entities))
    }
    func originOf(_ name: String) -> SIMD3<Double> {
        guard let entity = entities.first(where: { $0.name == name }) else {
            failures.append("no entity named \(name)")
            return SIMD3(0, 0, 0)
        }
        return worldOrigin(entity, in: entities)
    }
    func f3(_ v: Double) -> String { String(format: "%.3f", v) }
    func f4(_ v: Double) -> String { String(format: "%.4f", v) }

    for assertion in outcomes["expect"]?.array ?? [] {
        if let fov = assertion["fov"]?.object {
            let name = fov["camera"]?.string ?? ""
            let tolerance = fov["tolerance"]?.double ?? 0.001
            guard let c = cameraEntity(name) else { continue }
            if let want = fov["hfov_degrees"]?.double, abs(c.frame.hfovDegrees - want) > tolerance {
                failures.append("\(name): hfov \(c.frame.hfovDegrees) != \(want) (tolerance \(tolerance))")
            }
            if let want = fov["vfov_degrees"]?.double, abs(c.frame.vfovDegrees - want) > tolerance {
                failures.append("\(name): vfov \(c.frame.vfovDegrees) != \(want) (tolerance \(tolerance))")
            }
            if let want = fov["aspect"]?.double, abs(c.frame.aspect - want) > tolerance {
                failures.append("\(name): aspect \(c.frame.aspect) != \(want) (tolerance \(tolerance))")
            }
        } else if let target = assertion["in_frame"]?.object ?? assertion["out_of_frame"]?.object {
            let expectInside = assertion["in_frame"] != nil
            let name = target["camera"]?.string ?? "", entityName = target["entity"]?.string ?? ""
            guard let c = cameraEntity(name) else { continue }
            let p = project(c.view, c.frame, c.camera, originOf(entityName))
            if expectInside && !p.inFrame {
                failures.append("\(name): \(entityName) projects outside the frame (\(f3(p.x)), \(f3(p.y)))")
            }
            if !expectInside && p.inFrame {
                failures.append("\(name): \(entityName) projects inside the frame (\(f3(p.x)), \(f3(p.y)))")
            }
        } else if let centred = assertion["centred"]?.object {
            // The camera aims at the entity: its origin projects at the
            // frame's centre, in front — what `aim_entity` promises.
            let name = centred["camera"]?.string ?? "", entityName = centred["entity"]?.string ?? ""
            let tolerance = centred["tolerance"]?.double ?? 0.001
            guard let c = cameraEntity(name) else { continue }
            let p = project(c.view, c.frame, c.camera, originOf(entityName))
            if !(p.z > 0 && abs(p.x) <= tolerance && abs(p.y) <= tolerance) {
                failures.append("\(name): \(entityName) projects at (\(f4(p.x)), \(f4(p.y))), z \(f3(p.z)) — not the frame's centre (tolerance \(tolerance))")
            }
        } else if let focus = assertion["focus"]?.object {
            // The derived focus distance — `focus_entity`'s origin from
            // the camera, else `focus_distance_m` — within `tolerance`.
            let name = focus["camera"]?.string ?? ""
            let tolerance = focus["tolerance"]?.double ?? 0.001
            guard let c = cameraEntity(name) else { continue }
            let want = focus["distance_m"]?.double ?? 0
            if let actual = focusDistanceOf(c.entity, c.camera, in: entities) {
                if abs(actual - want) > tolerance {
                    failures.append("\(name): focus distance \(actual) != \(want) (tolerance \(tolerance))")
                }
            } else {
                failures.append("\(name): no focus distance")
            }
        } else if let expected = assertion["shot_list"]?.array?.compactMap(\.string) {
            let actual = shotList(entities).map(\.name)
            if actual != expected {
                failures.append("shot list \(actual) != \(expected)")
            }
        } else {
            failures.append("unknown assertion \(assertion)")
        }
    }
    return OutcomeResult(ok: failures.isEmpty, failures: failures)
}
