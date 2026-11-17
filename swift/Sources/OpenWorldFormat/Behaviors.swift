// Behaviors (spec/world.md "Behavior and triggers"), typed and posed.
//
// A behavior is a declarative constraint that runs from load — data, not
// code. The rule this file implements is the spec's: each behavior is a
// function of folded time, evaluated from the entity's authored transform;
// an entity's behaviors apply in array order, each from what the ones
// before it left; `spin`, `pulse` and `swing` act about a pivot — a point
// of the entity's local frame, metres before the transform's scale — that
// stays where the authored transform put it; and a kind this reader does
// not know is carried and never run. So a seek lands where the clock
// says, and two renderers agree.
//
// Where the spec states the motion — spin, swing, pulse, bob, the pivot —
// this follows it. Where it leaves the motion to "as their fields say" —
// orbit, look_at, path_follow, bounce — this follows the format's
// reference renderer (js/src/render.js), in closed form where that
// renderer steps frame by frame (path_follow, bounce): a behavior is a
// function of time, not of the frames that happened to be drawn. Where the
// schema documents a field that renderer reads otherwise, the schema wins
// (bounce's `damping` is the energy a bounce keeps). Each case says what
// it chose.
//
// Reading is lenient, as a viewer's must be: a field of the wrong type or
// an unknown value reads as absent, and the schema's default applies.

import Foundation

// MARK: - The vocabulary

/// An entity reference as a document holds it: the id, or a name before
/// ingestion binds it (spec/world.md "Identity").
public enum EntityReference: Equatable, Sendable {
    case id(Int)
    case name(String)

    public init?(json: JSONValue?) {
        if let id = json?.int { self = .id(id) }
        else if let name = json?.string { self = .name(name) }
        else { return nil }
    }
}

/// How `path_follow` runs out its waypoints.
public enum PathMode: String, Equatable, Sendable {
    /// Around and around: the last waypoint leads back to the first.
    case loop
    /// To the last waypoint and back to the first, and again.
    case pingPong = "ping_pong"
    /// To the last waypoint, and stay there.
    case once
}

/// One behavior, its defaults applied — the schema's `BehaviorDef`.
/// Angles are degrees, frequencies hertz, distances metres.
public enum Behavior: Equatable, Sendable {
    /// Circle a centre — the `center` entity's authored position, else
    /// `centerPoint`, else the origin — `radius` out, `speed` degrees a
    /// second about `axis` from `phase`, with `tilt` lifting the circle
    /// toward world +Y where it starts. The reference renderer's
    /// construction: the circle starts on `normalize(up × axis)`, `up`
    /// world +Y unless the axis is within 8° of it (then +X).
    case orbit(
        center: EntityReference?, centerPoint: SIMD3<Double>?, radius: Double,
        speed: Double, axis: SIMD3<Double>, phase: Double, tilt: Double)
    /// Turn about `axis` through `pivot`: the angle at time t is
    /// `speed × t + phase`.
    case spin(axis: SIMD3<Double>, speed: Double, phase: Double, pivot: SIMD3<Double>)
    /// Move along `axis` by `amplitude × sin(2π × frequency × t + phase)`.
    case bob(axis: SIMD3<Double>, amplitude: Double, frequency: Double, phase: Double)
    /// Face the `target` entity's origin: local +Z toward it, +Y up.
    case lookAt(target: EntityReference?)
    /// Scale by `min + (max − min) × (sin(2π × frequency × t + phase) + 1) / 2`
    /// about `pivot`.
    case pulse(minScale: Double, maxScale: Double, frequency: Double, phase: Double, pivot: SIMD3<Double>)
    /// Travel the waypoints at `speed` metres a second, facing the way it
    /// goes when `orientToPath`.
    case pathFollow(waypoints: [SIMD3<Double>], speed: Double, mode: PathMode, orientToPath: Bool)
    /// Drop from `height` above `surfaceY` and bounce, each bounce keeping
    /// `damping` of the energy, under `gravity`.
    case bounce(height: Double, gravity: Double, damping: Double, surfaceY: Double)
    /// A pendulum about `axis` through `pivot`: the angle at time t is
    /// `amplitude × sin(2π × frequency × t + phase)`.
    case swing(axis: SIMD3<Double>, amplitude: Double, frequency: Double, phase: Double, pivot: SIMD3<Double>)
    /// A kind this reader does not know, or no behavior at all (a bare
    /// name, a body that is not an object): carried, never run.
    case other(JSONValue)

    /// The externally tagged form, `{"Spin": {"speed": 45}}`.
    public init(json: JSONValue) {
        guard let object = json.object, object.count == 1, let (tag, body) = object.first,
              let p = body.object else {
            self = .other(json)
            return
        }
        func number(_ key: String, _ fallback: Double) -> Double { p[key]?.double ?? fallback }
        func vector(_ key: String, _ fallback: SIMD3<Double>) -> SIMD3<Double> { vec3(p[key]) ?? fallback }
        let x = SIMD3<Double>(1, 0, 0), y = SIMD3<Double>(0, 1, 0), origin = SIMD3<Double>(0, 0, 0)
        switch tag {
        case "Orbit":
            self = .orbit(
                center: EntityReference(json: p["center"]), centerPoint: vec3(p["center_point"]),
                radius: number("radius", 5), speed: number("speed", 36), axis: vector("axis", y),
                phase: number("phase", 0), tilt: number("tilt", 0))
        case "Spin":
            self = .spin(
                axis: vector("axis", y), speed: number("speed", 90), phase: number("phase", 0),
                pivot: vector("pivot", origin))
        case "Bob":
            self = .bob(
                axis: vector("axis", y), amplitude: number("amplitude", 0.5),
                frequency: number("frequency", 0.5), phase: number("phase", 0))
        case "LookAt":
            self = .lookAt(target: EntityReference(json: p["target"]))
        case "Pulse":
            self = .pulse(
                minScale: number("min_scale", 0.9), maxScale: number("max_scale", 1.1),
                frequency: number("frequency", 0.5), phase: number("phase", 0),
                pivot: vector("pivot", origin))
        case "PathFollow":
            self = .pathFollow(
                waypoints: (p["waypoints"]?.array ?? []).compactMap(vec3), speed: number("speed", 2),
                mode: p["mode"]?.string.flatMap(PathMode.init(rawValue:)) ?? .loop,
                orientToPath: p["orient_to_path"]?.bool ?? false)
        case "Bounce":
            self = .bounce(
                height: number("height", 3), gravity: number("gravity", 9.8),
                damping: number("damping", 0.7), surfaceY: number("surface_y", 0))
        case "Swing":
            self = .swing(
                axis: vector("axis", x), amplitude: number("amplitude", 30),
                frequency: number("frequency", 0.5), phase: number("phase", 0),
                pivot: vector("pivot", origin))
        default:
            self = .other(json)
        }
    }

    /// The kind as the spec names it — `"orbit"`, `"look_at"`, … — and
    /// `"unknown"` for one this reader carries without running.
    public var kind: String {
        switch self {
        case .orbit: return "orbit"
        case .spin: return "spin"
        case .bob: return "bob"
        case .lookAt: return "look_at"
        case .pulse: return "pulse"
        case .pathFollow: return "path_follow"
        case .bounce: return "bounce"
        case .swing: return "swing"
        case .other: return "unknown"
        }
    }

    /// Whether this reader runs it: every kind but one it doesn't know.
    public var runs: Bool {
        if case .other = self { return false }
        return true
    }
}

extension WorldEntity {
    /// The entity's behaviors, in array order — the order they apply in.
    public var behaviors: [Behavior] {
        (fields["behaviors"]?.array ?? []).map(Behavior.init(json:))
    }
}

/// The slowest rebound a bounce keeps: one slower relaunches at the first
/// rebound's speed, so a bounce runs from load and never settles (the
/// reference renderer's 0.1 m/s).
public let BOUNCE_MIN_REBOUND = 0.1

// MARK: - Posing a world

/// A world's motion: every entity's behaviors decoded once, posed at any
/// folded time. Build one per fold; ask it every frame.
public struct WorldMotion: Sendable {
    struct Node: Sendable {
        let parent: Int?
        let authored: Pose
        let behaviors: [Behavior]
    }

    let nodes: [Int: Node]
    let idsByName: [String: Int]

    /// The entities a behavior moves — those with at least one behavior
    /// this reader runs — in document order.
    public let animated: [Int]

    public init(_ entities: [WorldEntity]) {
        var nodes: [Int: Node] = [:]
        var idsByName: [String: Int] = [:]
        var animated: [Int] = []
        for entity in entities {
            let behaviors = entity.behaviors
            nodes[entity.id] = Node(parent: entity.parent, authored: Pose(entity.transform), behaviors: behaviors)
            idsByName[entity.name] = entity.id
            if behaviors.contains(where: \.runs) { animated.append(entity.id) }
        }
        self.nodes = nodes
        self.idsByName = idsByName
        self.animated = animated
    }

    /// Entity `id`'s pose at folded time `t` (seconds from load), relative
    /// to its parent: its authored transform, then each behavior in array
    /// order. nil for an id the world doesn't hold.
    public func pose(_ id: Int, at t: Double) -> Pose? {
        var evaluator = Evaluator(motion: self, t: t)
        return evaluator.pose(id)
    }

    /// Every animated entity's pose at `t`, in one pass: a parent or a
    /// target two entities share is posed once.
    public func poses(at t: Double) -> [Int: Pose] {
        var evaluator = Evaluator(motion: self, t: t)
        var poses: [Int: Pose] = [:]
        for id in animated {
            poses[id] = evaluator.pose(id)
        }
        return poses
    }

    /// Entity `id`'s origin in world space at `t`: its posed position,
    /// through its posed parents. nil for an id the world doesn't hold.
    public func worldOrigin(_ id: Int, at t: Double) -> SIMD3<Double>? {
        var evaluator = Evaluator(motion: self, t: t)
        return evaluator.worldOrigin(id)
    }

    func id(of reference: EntityReference) -> Int? {
        switch reference {
        case .id(let id): return nodes[id] == nil ? nil : id
        case .name(let name): return idsByName[name]
        }
    }
}

/// One entity's pose at folded time `t`, among `entities`. Posing every
/// frame? Build a `WorldMotion` once instead.
public func poseOf(_ entity: WorldEntity, at t: Double, in entities: [WorldEntity]) -> Pose {
    WorldMotion(entities).pose(entity.id, at: t) ?? Pose(entity.transform)
}

/// One time `t`'s poses, each computed once. A look-at needs its target's
/// world origin and its own parents', which may be animated too; a cycle
/// of those (two entities looking at each other's moving parents) ends at
/// an authored pose rather than recursing.
private struct Evaluator {
    let motion: WorldMotion
    let t: Double
    var cache: [Int: Pose] = [:]
    var visiting: Set<Int> = []

    init(motion: WorldMotion, t: Double) {
        self.motion = motion
        self.t = t
    }

    mutating func pose(_ id: Int) -> Pose? {
        if let pose = cache[id] { return pose }
        guard let node = motion.nodes[id] else { return nil }
        if visiting.contains(id) { return node.authored }
        visiting.insert(id)
        defer { visiting.remove(id) }
        var pose = node.authored
        for behavior in node.behaviors {
            apply(behavior, to: &pose, parent: node.parent)
        }
        cache[id] = pose
        return pose
    }

    mutating func worldOrigin(_ id: Int) -> SIMD3<Double>? {
        guard let node = motion.nodes[id], let pose = pose(id) else { return nil }
        return toWorld(pose.position, through: chain(above: node.parent))
    }

    /// The posed ancestors, nearest first.
    mutating func chain(above parent: Int?) -> [Pose] {
        var poses: [Pose] = []
        var current = parent
        while let id = current, poses.count < 4096, let node = motion.nodes[id] {
            poses.append(pose(id) ?? node.authored)
            current = node.parent
        }
        return poses
    }

    func toWorld(_ point: SIMD3<Double>, through chain: [Pose]) -> SIMD3<Double> {
        chain.reduce(point) { $1.apply($0) }
    }

    /// The rotation a chain of ancestors turns by (the parent's world
    /// rotation), root first.
    func rotation(of chain: [Pose]) -> Quaternion {
        chain.reversed().reduce(Quaternion.identity) { $0 * $1.rotation }
    }

    /// Turn the pose to face `direction`, a world-space direction: local
    /// +Z toward it, +Y up, through the parent's rotation.
    func face(_ direction: SIMD3<Double>, pose: inout Pose, parentRotation: Quaternion) {
        pose.rotation = parentRotation.inverse * Quaternion(lookingAlong: direction)
    }

    mutating func apply(_ behavior: Behavior, to pose: inout Pose, parent: Int?) {
        let t = self.t
        switch behavior {
        case let .spin(axis, speed, phase, pivot):
            let q0 = pose.rotation
            pose.rotation = q0 * Quaternion(axis: unit(axis, or: SIMD3(0, 1, 0)), radians: radians(speed * t + phase))
            hold(pivot, of: &pose, from: q0, pose.scale)

        case let .swing(axis, amplitude, frequency, phase, pivot):
            let q0 = pose.rotation
            let angle = radians(amplitude) * sin(2 * .pi * frequency * t + radians(phase))
            pose.rotation = q0 * Quaternion(axis: unit(axis, or: SIMD3(1, 0, 0)), radians: angle)
            hold(pivot, of: &pose, from: q0, pose.scale)

        case let .pulse(minScale, maxScale, frequency, phase, pivot):
            let s0 = pose.scale
            let wave = sin(2 * .pi * frequency * t + radians(phase))
            pose.scale = s0 * (minScale + (maxScale - minScale) * (wave + 1) / 2)
            hold(pivot, of: &pose, from: pose.rotation, s0)

        case let .bob(axis, amplitude, frequency, phase):
            // From what the behaviors before it left — the authored
            // position, or where a spin about a pivot moved it. The axis
            // is a direction and the amplitude the distance, per the schema.
            pose.position += unit(axis, or: SIMD3(0, 1, 0))
                * (amplitude * sin(2 * .pi * frequency * t + radians(phase)))

        case let .orbit(center, centerPoint, radius, speed, axis, phase, tilt):
            // The centre entity's authored position, as the reference
            // renderer reads it; a centre that isn't there falls back to
            // the point, then the origin.
            let c = center.flatMap { motion.id(of: $0) }.flatMap { motion.nodes[$0]?.authored.position }
                ?? centerPoint ?? SIMD3(0, 0, 0)
            let a = unit(axis, or: SIMD3(0, 1, 0))
            let up: SIMD3<Double> = abs(a.y) < 0.99 ? SIMD3(0, 1, 0) : SIMD3(1, 0, 0)
            let right = unit(cross3(up, a), or: SIMD3(1, 0, 0))
            let forward = unit(cross3(a, right), or: SIMD3(0, 0, 1))
            let angle = radians(speed * t + phase)
            var d = right * cos(angle) + forward * sin(angle)
            d.y += sin(radians(tilt)) * cos(angle)
            pose.position = c + d * radius

        case let .lookAt(target):
            // A target that isn't there — never bound, or deleted since —
            // leaves the pose as it was (spec/session.md: the behavior has
            // nothing to look at until it is retargeted or dropped).
            guard let target, let targetId = motion.id(of: target),
                  let aim = worldOrigin(targetId) else { break }
            let ancestors = chain(above: parent)
            let eye = toWorld(pose.position, through: ancestors)
            face(aim - eye, pose: &pose, parentRotation: rotation(of: ancestors))

        case let .pathFollow(waypoints, speed, mode, orientToPath):
            guard let at = pathPoint(waypoints, speed: speed, mode: mode, t: t) else { break }
            pose.position = at.position
            if orientToPath, let direction = at.direction {
                let parentRotation = rotation(of: chain(above: parent))
                face(parentRotation.act(direction), pose: &pose, parentRotation: parentRotation)
            }

        case let .bounce(height, gravity, damping, surfaceY):
            pose.position.y = bounceHeight(height: height, gravity: gravity, damping: damping, surfaceY: surfaceY, t: t)

        case .other:
            break  // carried, never run: the entity draws where it was authored
        }
    }
}

/// Keep a local point where it stood: the pose's rotation or scale just
/// changed from (`q0`, `s0`), so move it by what that change did to
/// `pivot` — a point of the entity's local frame, metres before its scale.
/// At the origin nothing moves.
private func hold(_ pivot: SIMD3<Double>, of pose: inout Pose, from q0: Quaternion, _ s0: SIMD3<Double>) {
    guard pivot != SIMD3(0, 0, 0) else { return }
    pose.position += q0.act(s0 * pivot) - pose.rotation.act(pose.scale * pivot)
}

// MARK: - Closed forms

/// Where `path_follow` stands at time `t`, and which way it is going
/// (nil on a segment of no length): `speed × t` metres along the path —
/// around the closed loop of every waypoint for `loop` (the last leads
/// back to the first, as the reference renderer runs it), there and back
/// along the open path for `ping_pong`, and to its end for `once`. nil
/// with fewer than two waypoints: nothing to follow.
func pathPoint(
    _ waypoints: [SIMD3<Double>], speed: Double, mode: PathMode, t: Double
) -> (position: SIMD3<Double>, direction: SIMD3<Double>?)? {
    let n = waypoints.count
    guard n >= 2 else { return nil }
    let segments = mode == .loop ? n : n - 1
    let lengths = (0..<segments).map { length(waypoints[($0 + 1) % n] - waypoints[$0]) }
    let total = lengths.reduce(0, +)
    guard total > 1e-9, total.isFinite else { return (waypoints[0], nil) }

    let distance = speed * t
    let ahead: Double = speed >= 0 ? 1 : -1
    var u: Double  // metres along the path from the first waypoint
    var travel = ahead  // +1 along the path's order, −1 against it
    switch mode {
    case .loop:
        u = positiveRemainder(distance, total)
    case .pingPong:
        let w = positiveRemainder(distance, 2 * total)
        if w <= total {
            u = w
        } else {
            u = 2 * total - w
            travel = -ahead
        }
    case .once:
        u = min(max(distance, 0), total)
    }
    for i in 0..<segments {
        let last = i == segments - 1
        if u <= lengths[i] || last {
            let a = waypoints[i], b = waypoints[(i + 1) % n]
            let fraction = lengths[i] > 0 ? min(u / lengths[i], 1) : 0
            let direction = lengths[i] > 0 ? (b - a) / lengths[i] * travel : nil
            return (a + (b - a) * fraction, direction)
        }
        u -= lengths[i]
    }
    return (waypoints[0], nil)
}

/// A bounce's height at time `t`: dropped from rest at `height` above
/// `surfaceY` — where the entity is authored in the conformance world —
/// each rebound keeping `damping` of the energy (its speed times
/// √damping, per the schema), and a rebound slower than
/// `BOUNCE_MIN_REBOUND` relaunching at the first rebound's speed, so it
/// never settles: the reference renderer's motion, in closed form. A
/// bounce whose first rebound is already that slow comes to rest on the
/// surface.
func bounceHeight(height h: Double, gravity g: Double, damping: Double, surfaceY: Double, t: Double) -> Double {
    guard h > 0, g > 0, h.isFinite, g.isFinite, t.isFinite else { return surfaceY + max(h, 0) }
    guard t > 0 else { return surfaceY + h }
    let v0 = (2 * g * h).squareRoot()  // the speed at the first impact
    let fall = v0 / g
    if t < fall { return surfaceY + h - g * t * t / 2 }

    let r = min(max(damping, 0), 1).squareRoot()  // the speed a rebound keeps
    guard v0 * r >= BOUNCE_MIN_REBOUND else { return surfaceY }
    var tau = t - fall
    let unit = 2 * v0 / g  // an arc at speed v0·r^k lasts unit·r^k
    func arc(_ speed: Double, _ local: Double) -> Double {
        surfaceY + max(0, speed * local - g * local * local / 2)
    }
    if r >= 1 {
        tau = tau.truncatingRemainder(dividingBy: unit)
        return arc(v0, tau)
    }

    // Arcs 1…K rebound at v0·r^k ≥ the minimum; then the cycle repeats.
    var arcs = max(1, Int(floor(log(BOUNCE_MIN_REBOUND / v0) / log(r))))
    while v0 * pow(r, Double(arcs + 1)) >= BOUNCE_MIN_REBOUND { arcs += 1 }
    while arcs > 1 && v0 * pow(r, Double(arcs)) < BOUNCE_MIN_REBOUND { arcs -= 1 }
    // The time the first m arcs take: unit·r·(1 − r^m)/(1 − r).
    func elapsed(_ m: Int) -> Double { unit * r * (1 - pow(r, Double(m))) / (1 - r) }
    tau = tau.truncatingRemainder(dividingBy: elapsed(arcs))
    // The arc holding tau: the largest m < K with elapsed(m) ≤ tau.
    var m = Int(floor(log(max(1e-300, 1 - tau * (1 - r) / (unit * r))) / log(r)))
    m = min(max(m, 0), arcs - 1)
    while m > 0 && elapsed(m) > tau { m -= 1 }
    while m + 1 < arcs && elapsed(m + 1) <= tau { m += 1 }
    return arc(v0 * pow(r, Double(m + 1)), tau - elapsed(m))
}

// MARK: - Small vector math

private func vec3(_ json: JSONValue?) -> SIMD3<Double>? {
    guard let a = json?.array, a.count == 3 else { return nil }
    let c = a.compactMap(\.double)
    return c.count == 3 ? SIMD3(c[0], c[1], c[2]) : nil
}

private func length(_ v: SIMD3<Double>) -> Double {
    (v * v).sum().squareRoot()
}

private func unit(_ v: SIMD3<Double>, or fallback: SIMD3<Double>) -> SIMD3<Double> {
    let l = length(v)
    return l > 1e-12 && l.isFinite ? v / l : fallback
}

private func cross3(_ a: SIMD3<Double>, _ b: SIMD3<Double>) -> SIMD3<Double> {
    SIMD3(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)
}

private func radians(_ degrees: Double) -> Double {
    degrees * .pi / 180
}

private func positiveRemainder(_ a: Double, _ m: Double) -> Double {
    let r = a.truncatingRemainder(dividingBy: m)
    return r < 0 ? r + m : r
}
