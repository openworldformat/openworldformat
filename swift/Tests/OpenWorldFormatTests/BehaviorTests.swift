// Behaviors, posed (spec/world.md "Behavior and triggers") — the first
// ten tests mirror js/test/behaviors.test.mjs number for number: the
// reference renderer and this package pose the pivots, the phases and the
// swing alike. The rest pin what the spec leaves to "as their fields
// say" the way this package reads it (orbit, look_at, path_follow,
// bounce), the conformance world's pivots, and the Euler convention every
// pose rests on.

import XCTest
@testable import OpenWorldFormat

final class BehaviorTests: XCTestCase {
    /// An entity at an authored transform, carrying `behaviors` (JSON).
    private func entity(
        _ id: Int = 1, _ name: String = "e", at position: [Double], scale: [Double]? = nil,
        rotation: [Double]? = nil, parent: Int? = nil, behaviors: String = "[]"
    ) throws -> WorldEntity {
        var transform = #""position": \#(position)"#
        if let scale { transform += #", "scale": \#(scale)"# }
        if let rotation { transform += #", "rotation_degrees": \#(rotation)"# }
        let parentField = parent.map { #", "parent": \#($0)"# } ?? ""
        return try WorldEntity(json: JSONValue(parsing:
            #"{"id": \#(id), "name": "\#(name)"\#(parentField), "transform": {\#(transform)}, "behaviors": \#(behaviors)}"#))
    }

    private func pose(_ entity: WorldEntity, _ t: Double) -> Pose {
        poseOf(entity, at: t, in: [entity])
    }

    private func near(
        _ actual: SIMD3<Double>, _ expected: SIMD3<Double>, tolerance: Double = 1e-9,
        _ message: String = "", file: StaticString = #filePath, line: UInt = #line
    ) {
        let d = actual - expected
        XCTAssertLessThanOrEqual((d * d).sum().squareRoot(), tolerance,
                                 "\(actual) is not \(expected) \(message)", file: file, line: line)
    }

    private func behaviorsWorld() throws -> WorldManifest {
        try parseManifest(try read(root.appendingPathComponent("conformance/behaviors.json")))
    }

    // MARK: - The pivots, the phases and the swing (js/test/behaviors.test.mjs)

    func testASpinAboutItsBaseKeepsTheBaseOnTheHubAndSweepsTheTip() throws {
        // A blade 2 m tall standing on a hub at y = 0.5 (conformance/behaviors.json).
        let blade = try entity(at: [9, 1.5, 0], behaviors: #"[{"Spin": {"axis": [0, 0, 1], "speed": 90, "pivot": [0, -1, 0]}}]"#)
        for t in [0, 0.5, 1, 2.3, 7.25] {
            near(pose(blade, t).apply(SIMD3(0, -1, 0)), SIMD3(9, 0.5, 0), "at t = \(t)")
        }
        // A quarter turn about +Z takes the tip from above the hub to its left.
        near(pose(blade, 1).apply(SIMD3(0, 1, 0)), SIMD3(7, 0.5, 0))
        near(pose(blade, 2).apply(SIMD3(0, 1, 0)), SIMD3(9, -1.5, 0))
    }

    func testASpinWithoutAPivotTurnsInPlaceAndPhaseIsTimeAlreadySpent() throws {
        let plain = try entity(at: [1, 2, 3], behaviors: #"[{"Spin": {"axis": [0, 1, 0], "speed": 90}}]"#)
        near(pose(plain, 3.3).position, SIMD3(1, 2, 3))
        let ahead = try entity(at: [1, 2, 3], behaviors: #"[{"Spin": {"axis": [0, 1, 0], "speed": 90, "phase": 90}}]"#)
        XCTAssertLessThan(pose(ahead, 0).rotation.angle(to: pose(plain, 1).rotation), 1e-9,
                          "phase 90° at t = 0 is 90°/s at t = 1")
    }

    func testTwoSpinsOnOneEntityComposeFromTheAuthoredRotationEachFrame() throws {
        let both = try entity(at: [0, 0, 0], behaviors: #"""
            [{"Spin": {"axis": [0, 1, 0], "speed": 90}}, {"Spin": {"axis": [1, 0, 0], "speed": 30}}]
            """#)
        let expected = Quaternion(axis: SIMD3(0, 1, 0), radians: .pi / 2) * Quaternion(axis: SIMD3(1, 0, 0), radians: .pi / 6)
        // The same time twice: absolute in time, not accumulated.
        _ = pose(both, 1)
        XCTAssertLessThan(pose(both, 1).rotation.angle(to: expected), 1e-9)
    }

    func testASwingHangsFromItsTopAndRocksToItsAmplitude() throws {
        // A sign 1.2 m tall under a hook at y = 3 (conformance/behaviors.json).
        let sign = try entity(at: [9, 2.4, -4], behaviors: #"""
            [{"Swing": {"axis": [0, 0, 1], "amplitude": 30, "frequency": 0.25, "pivot": [0, 0.6, 0]}}]
            """#)
        for t in [0, 0.3, 1, 2, 3.7] {
            near(pose(sign, t).apply(SIMD3(0, 0.6, 0)), SIMD3(9, 3, -4), "at t = \(t)")
        }
        // sin(2π · 0.25 · 1) = 1: the full 30° about +Z, the bottom swings to +X.
        let (s, c) = (sin(Double.pi / 6), cos(Double.pi / 6))
        near(pose(sign, 1).apply(SIMD3(0, -0.6, 0)), SIMD3(9 + 1.2 * s, 3 - 1.2 * c, -4))
        // sin(2π · 0.25 · 3) = −1: the other way.
        near(pose(sign, 3).apply(SIMD3(0, -0.6, 0)), SIMD3(9 - 1.2 * s, 3 - 1.2 * c, -4))
        // At rest it hangs straight: the authored transform.
        near(pose(sign, 2).apply(SIMD3(0, -0.6, 0)), SIMD3(9, 1.8, -4))
    }

    func testASwingTakesItsDefaultsAThirtyDegreeHingeAcrossXAtHalfAHertz() throws {
        let swing = try entity(at: [0, 0, 0], behaviors: #"[{"Swing": {}}]"#)
        let expected = Quaternion(axis: SIMD3(1, 0, 0), radians: .pi / 6)  // sin(π/2) = 1 at t = 0.5
        XCTAssertLessThan(pose(swing, 0.5).rotation.angle(to: expected), 1e-9)
    }

    func testAPulseAboutItsBaseBreathesOnTheGround() throws {
        // A unit cube on the ground, pivot at its base.
        let block = try entity(at: [-9, 0.5, 0], behaviors: #"""
            [{"Pulse": {"min_scale": 0.5, "max_scale": 1.5, "frequency": 0.25, "pivot": [0, -0.5, 0]}}]
            """#)
        for t in [0, 1, 2, 3, 0.37] {
            near(pose(block, t).apply(SIMD3(0, -0.5, 0)), SIMD3(-9, 0, 0), "at t = \(t)")
        }
        // sin(2π · 0.25 · 1) = 1: scale 1.5, the top at 1.5 m.
        near(pose(block, 1).apply(SIMD3(0, 0.5, 0)), SIMD3(-9, 1.5, 0))
        near(pose(block, 1).scale, SIMD3(1.5, 1.5, 1.5))
        // sin(… · 3) = −1: scale 0.5.
        near(pose(block, 3).apply(SIMD3(0, 0.5, 0)), SIMD3(-9, 0.5, 0))
    }

    func testAPulseWithoutAPivotScalesAboutItsCentreWithTheSchemaDefaults() throws {
        let ball = try entity(at: [2, 2, 2], behaviors: #"[{"Pulse": {"frequency": 0.25}}]"#)
        near(pose(ball, 1).scale, SIMD3(1.1, 1.1, 1.1))
        near(pose(ball, 3).scale, SIMD3(0.9, 0.9, 0.9))
        near(pose(ball, 3).position, SIMD3(2, 2, 2))
    }

    func testThePivotIsMeasuredBeforeTheScaleAndTurnsWithTheAuthoredRotation() throws {
        // A blade scaled ×2 and authored turned 90° about Y: the base is still
        // [0, −1, 0] in its own frame, 2 m below its centre in the parent's.
        let blade = try entity(at: [0, 2, 0], scale: [2, 2, 2], rotation: [0, 90, 0], behaviors: #"""
            [{"Spin": {"axis": [0, 0, 1], "speed": 90, "pivot": [0, -1, 0]}}]
            """#)
        for t in [0, 0.5, 1.5] {
            near(pose(blade, t).apply(SIMD3(0, -1, 0)), SIMD3(0, 0, 0), "at t = \(t)")
        }
    }

    func testAKindThisReaderDoesNotKnowIsCarriedNeverRun() throws {
        let wobble = Behavior(json: try JSONValue(parsing: #"{"Wobble": {"rate": 2}}"#))
        XCTAssertEqual(wobble, .other(try JSONValue(parsing: #"{"Wobble": {"rate": 2}}"#)))
        XCTAssertFalse(wobble.runs)
        XCTAssertEqual(wobble.kind, "unknown")
        // A bare name or a non-object body is no behavior either.
        XCTAssertFalse(Behavior(json: .string("Spin")).runs)
        XCTAssertFalse(Behavior(json: try JSONValue(parsing: #"{"Spin": 3}"#)).runs)
        // The entity still draws, where it was authored.
        let block = try entity(at: [1, 2, 3], rotation: [0, 45, 0], behaviors: #"[{"Wobble": {"rate": 2}}, "Spin"]"#)
        XCTAssertEqual(pose(block, 4.2), Pose(block.transform))
        XCTAssertEqual(WorldMotion([block]).animated, [])
    }

    func testABobAddsToWhatTheBehaviorsBeforeItLeftSoAWheelCanBobOnItsAxle() throws {
        // A quarter turn about the base moves the centre a metre left; a bob at its
        // crest lifts it half a metre from there — not from the authored position.
        let wheel = try entity(at: [9, 1.5, 0], behaviors: #"""
            [{"Spin": {"axis": [0, 0, 1], "speed": 90, "pivot": [0, -1, 0]}},
             {"Bob": {"axis": [0, 1, 0], "amplitude": 0.5, "frequency": 0.25}}]
            """#)
        near(pose(wheel, 1).position, SIMD3(8, 1, 0))
        // Alone, from the authored position, and back to it at the trough.
        let alone = try entity(at: [2, 3, 4], behaviors: #"[{"Bob": {"amplitude": 0.5, "frequency": 0.25}}]"#)
        near(pose(alone, 1).position, SIMD3(2, 3.5, 4))
        near(pose(alone, 3).position, SIMD3(2, 2.5, 4))
    }

    // MARK: - Reading

    func testEveryKindReadsWithTheSchemaDefaults() throws {
        func read(_ text: String) throws -> Behavior { Behavior(json: try JSONValue(parsing: text)) }
        let (x, y, o) = (SIMD3<Double>(1, 0, 0), SIMD3<Double>(0, 1, 0), SIMD3<Double>(0, 0, 0))
        XCTAssertEqual(try read(#"{"Orbit": {}}"#),
                       .orbit(center: nil, centerPoint: nil, radius: 5, speed: 36, axis: y, phase: 0, tilt: 0))
        XCTAssertEqual(try read(#"{"Spin": {}}"#), .spin(axis: y, speed: 90, phase: 0, pivot: o))
        XCTAssertEqual(try read(#"{"Bob": {}}"#), .bob(axis: y, amplitude: 0.5, frequency: 0.5, phase: 0))
        XCTAssertEqual(try read(#"{"LookAt": {"target": 7}}"#), .lookAt(target: .id(7)))
        XCTAssertEqual(try read(#"{"LookAt": {"target": "lamp"}}"#), .lookAt(target: .name("lamp")))
        XCTAssertEqual(try read(#"{"Pulse": {}}"#), .pulse(minScale: 0.9, maxScale: 1.1, frequency: 0.5, phase: 0, pivot: o))
        XCTAssertEqual(try read(#"{"PathFollow": {"waypoints": [[0, 0, 0], [1, 0, 0]]}}"#),
                       .pathFollow(waypoints: [o, x], speed: 2, mode: .loop, orientToPath: false))
        XCTAssertEqual(try read(#"{"Bounce": {}}"#), .bounce(height: 3, gravity: 9.8, damping: 0.7, surfaceY: 0))
        XCTAssertEqual(try read(#"{"Swing": {}}"#), .swing(axis: x, amplitude: 30, frequency: 0.5, phase: 0, pivot: o))
        // Lenient, as a viewer reads: a field of the wrong type or an unknown
        // value reads as absent.
        XCTAssertEqual(try read(#"{"Spin": {"speed": "fast", "pivot": [1, 2]}}"#), .spin(axis: y, speed: 90, phase: 0, pivot: o))
        XCTAssertEqual(try read(#"{"PathFollow": {"waypoints": [[0, 0, 0], [1, 0, 0]], "mode": "zigzag"}}"#),
                       .pathFollow(waypoints: [o, x], speed: 2, mode: .loop, orientToPath: false))
        XCTAssertEqual(try read(#"{"PathFollow": {"mode": "ping_pong"}}"#).kind, "path_follow")
    }

    // MARK: - The conformance world

    func testTheConformanceWorldsBehaviorsAllRunAndItsPivotsHold() throws {
        let world = try behaviorsWorld()
        for entity in world.entities {
            for behavior in entity.behaviors {
                XCTAssertTrue(behavior.runs, "\(entity.name): \(behavior) is a kind this package should run")
            }
        }
        let kinds = Set(world.entities.flatMap { $0.behaviors.map(\.kind) })
        XCTAssertEqual(kinds, ["orbit", "spin", "bob", "look_at", "pulse", "path_follow", "bounce", "swing"])
        let motion = WorldMotion(world.entities)
        XCTAssertEqual(motion.animated, [4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 15, 17, 18])

        let id = { (name: String) in world.entities.first { $0.name == name }!.id }
        for t in [0, 0.4, 1.7, 5, 33.3] {
            let poses = motion.poses(at: t)
            // The blade wheels about its base, on the axle.
            near(poses[id("blade")]!.apply(SIMD3(0, -1, 0)), SIMD3(9, 0.5, 0), "blade at t = \(t)")
            // The sign swings from its hook.
            near(poses[id("sign")]!.apply(SIMD3(0, 0.6, 0)), SIMD3(9, 3, -4), "sign at t = \(t)")
            // The block breathes on the ground.
            near(poses[id("breather")]!.apply(SIMD3(0, -0.5, 0)), SIMD3(-9, 0, 0), "breather at t = \(t)")
        }
    }

    func testPosesAreFunctionsOfFoldedTimeSoASeekLandsWhereTheClockSays() throws {
        let world = try behaviorsWorld()
        let motion = WorldMotion(world.entities)
        let times = [100, 0, 3.7, 12.25, 0.016, 100]
        let first = times.map { motion.poses(at: $0) }
        for (t, batch) in zip(times, first) {
            // One pass agrees with one entity at a time, and with itself later.
            for id in motion.animated {
                XCTAssertEqual(batch[id], motion.pose(id, at: t), "entity \(id) at t = \(t)")
            }
        }
        XCTAssertEqual(first[0], first[5])
        // Something moved between two times — the world is running.
        XCTAssertNotEqual(first[2], first[3])
    }

    // MARK: - What the spec leaves to "as their fields say"

    func testAnOrbitIsTheReferenceRenderersCircle() throws {
        let world = try behaviorsWorld()
        let motion = WorldMotion(world.entities)
        let id = { (name: String) in world.entities.first { $0.name == name }!.id }
        // About +X from a fixed point, a quarter turn of phase in: straight up.
        near(motion.pose(id("orbiter_point"), at: 0)!.position, SIMD3(0, 6, 0))
        // At −60°/s, 1.5 s later the phase is spent: on −Z.
        near(motion.pose(id("orbiter_point"), at: 1.5)!.position, SIMD3(0, 4, -2))
        // About +Y around the hub (named, as the conformance world writes it):
        // the circle starts on +Z, lifted by the tilt there, and passes +X
        // at a quarter turn, where the tilt lifts it no more.
        let tilt = sin(15 * Double.pi / 180)
        near(motion.pose(id("orbiter_entity"), at: 0)!.position, SIMD3(0, 1.5 + 3 * tilt, 3))
        near(motion.pose(id("orbiter_entity"), at: 2.25)!.position, SIMD3(3, 1.5, 0), tolerance: 1e-9)
        // 40°/s comes round every 9 s.
        near(motion.pose(id("orbiter_entity"), at: 9 + 1.1)!.position,
             motion.pose(id("orbiter_entity"), at: 1.1)!.position, tolerance: 1e-9)
        // A centre that isn't there falls back to the point, then the origin.
        let lost = try entity(at: [0, 0, 0], behaviors: #"""
            [{"Orbit": {"center": "nobody", "center_point": [1, 2, 3], "radius": 1, "speed": 0}}]
            """#)
        near(pose(lost, 0).position, SIMD3(1, 2, 4))
    }

    func testALookAtTurnsPlusZTowardItsTargetThroughItsParent() throws {
        let world = try behaviorsWorld()
        let motion = WorldMotion(world.entities)
        let id = { (name: String) in world.entities.first { $0.name == name }!.id }
        for t in [0, 1.3, 4.5] {
            let watcher = motion.pose(id("watcher"), at: t)!
            let target = motion.worldOrigin(id("orbiter_entity"), at: t)!
            let toward = target - SIMD3(6, 1, 0)
            near(watcher.rotation.act(SIMD3(0, 0, 1)), toward / (toward * toward).sum().squareRoot(), "at t = \(t)")
            XCTAssertEqual(watcher.rotation.act(SIMD3(1, 0, 0)).y, 0, accuracy: 1e-9, "the watcher stays level")
        }

        // Under a parent turned 90° about Y: the child's local turn undoes the
        // parent's, so in the world it still faces its target, at +X.
        let parent = try entity(1, "turntable", at: [1, 0, 0], rotation: [0, 90, 0])
        let child = try entity(2, "eye", at: [0, 0, 2], parent: 1, behaviors: #"[{"LookAt": {"target": 3}}]"#)
        let target = try entity(3, "lamp", at: [10, 0, 0])
        let motion2 = WorldMotion([parent, child, target])
        near(motion2.worldOrigin(2, at: 0)!, SIMD3(3, 0, 0))
        let worldRotation = Pose(parent.transform).rotation * motion2.pose(2, at: 0)!.rotation
        near(worldRotation.act(SIMD3(0, 0, 1)), SIMD3(1, 0, 0))
        // A target that isn't there leaves the authored turn.
        let blind = try entity(4, "blind", at: [0, 0, 0], rotation: [0, 30, 0], behaviors: #"[{"LookAt": {"target": "nobody"}}]"#)
        XCTAssertEqual(pose(blind, 1), Pose(blind.transform))
    }

    func testPathFollowLoopsTheClosedPathPingPongsTheOpenOneAndOnceStops() throws {
        let world = try behaviorsWorld()
        let motion = WorldMotion(world.entities)
        let id = { (name: String) in world.entities.first { $0.name == name }!.id }
        let leg = 20.0.squareRoot()  // (−4, 5) → (0, 7) and (0, 7) → (4, 5)

        // Loop: the closing leg is travelled, not jumped — 2·√20 + 4 metres
        // in, it is halfway home, facing the way it goes.
        let loop = id("path_loop")
        near(motion.pose(loop, at: 0)!.position, SIMD3(-4, 0.5, 5))
        near(motion.pose(loop, at: 0)!.rotation.act(SIMD3(0, 0, 1)), SIMD3(4, 0, 2) / leg)
        let halfwayHome = (2 * leg + 4) / 2
        near(motion.pose(loop, at: halfwayHome)!.position, SIMD3(0, 0.5, 5), tolerance: 1e-9)
        near(motion.pose(loop, at: halfwayHome)!.rotation.act(SIMD3(0, 0, 1)), SIMD3(-1, 0, 0))
        near(motion.pose(loop, at: (2 * leg + 8) / 2)!.position, SIMD3(-4, 0.5, 5), tolerance: 1e-9)

        // Ping-pong: to the far end in 8/3 s, and back the way it came.
        let pong = id("path_ping_pong")
        near(motion.pose(pong, at: 2)!.position, SIMD3(2, 1.5, 5))
        near(motion.pose(pong, at: 4)!.position, SIMD3(0, 1.5, 5))
        near(motion.pose(pong, at: 16.0 / 3)!.position, SIMD3(-4, 1.5, 5), tolerance: 1e-9)

        // Once: along the open path, then at its end for good.
        let once = id("path_once")
        let first = SIMD3<Double>(4, 1, 0) / 17.0.squareRoot()
        near(motion.pose(once, at: 1)!.position, SIMD3(-4, 2.5, 5) + first * 1.5)
        near(motion.pose(once, at: 100)!.position, SIMD3(4, 2.5, 5))
        near(motion.pose(once, at: 1000)!.position, SIMD3(4, 2.5, 5))

        // Facing the way it goes on the way back, too.
        let facing = try entity(at: [0, 0, 0], behaviors: #"""
            [{"PathFollow": {"waypoints": [[0, 0, 0], [4, 0, 0]], "speed": 1, "mode": "ping_pong", "orient_to_path": true}}]
            """#)
        near(pose(facing, 1).rotation.act(SIMD3(0, 0, 1)), SIMD3(1, 0, 0))
        near(pose(facing, 5).rotation.act(SIMD3(0, 0, 1)), SIMD3(-1, 0, 0))
        // One waypoint is nothing to follow.
        let stuck = try entity(at: [1, 1, 1], behaviors: #"[{"PathFollow": {"waypoints": [[5, 5, 5]]}}]"#)
        XCTAssertEqual(pose(stuck, 3), Pose(stuck.transform))
    }

    func testABounceDropsFromItsHeightKeepsDampingOfTheEnergyAndRelaunches() throws {
        // The conformance bouncer: dropped from 2 m above a surface at 0.5 —
        // where it is authored — keeping 85% of the energy each bounce.
        let ball = try entity(at: [0, 2.5, -5], behaviors: #"""
            [{"Bounce": {"height": 2, "gravity": 9.8, "damping": 0.85, "surface_y": 0.5}}]
            """#)
        XCTAssertEqual(pose(ball, 0).position, SIMD3(0, 2.5, -5))
        let fall = (2 * 2 / 9.8).squareRoot()
        XCTAssertEqual(pose(ball, fall).position.y, 0.5, accuracy: 1e-9)
        let v1 = (2 * 9.8 * 2).squareRoot() * 0.85.squareRoot()
        XCTAssertEqual(pose(ball, fall + v1 / 9.8).position.y, 0.5 + 2 * 0.85, accuracy: 1e-9)
        for step in 0...3000 {
            let y = pose(ball, Double(step) / 100).position.y
            XCTAssertGreaterThanOrEqual(y, 0.5 - 1e-9)
            XCTAssertLessThanOrEqual(y, 2.5 + 1e-9)
        }
        XCTAssertEqual(pose(ball, 1).position.x, 0)  // only the height moves

        // A quarter of the energy kept, from 1 m under g = 10: rebounds at
        // √20·(1/2)^k, five of them at 0.1 m/s or more, then the first again.
        func y(_ t: Double, damping: Double = 0.25) -> Double {
            bounceHeight(height: 1, gravity: 10, damping: damping, surfaceY: 0, t: t)
        }
        let v0 = 20.0.squareRoot()
        let cycle = (1...5).map { 2 * v0 * pow(0.5, Double($0)) / 10 }.reduce(0, +)
        let drop = v0 / 10
        XCTAssertEqual(y(drop + v0 * 0.5 / 10), 0.25, accuracy: 1e-9)  // the first apex: a quarter of the height
        XCTAssertEqual(y(drop + cycle + v0 * 0.5 / 10), 0.25, accuracy: 1e-9)  // and again, relaunched
        let fifth = v0 * pow(0.5, 5)
        let beforeFifth = (1...4).map { 2 * v0 * pow(0.5, Double($0)) / 10 }.reduce(0, +)
        XCTAssertEqual(y(drop + beforeFifth + fifth / 10), fifth * fifth / 20, accuracy: 1e-9)
        // Nothing kept: it lands and stays. Everything kept: every apex is the drop.
        XCTAssertEqual(y(5, damping: 0.0001), 0)
        XCTAssertEqual(y(drop + 2 * v0 / 10 * 7 + v0 / 10, damping: 1), 1, accuracy: 1e-9)
    }

    // MARK: - The convention every pose rests on

    func testTheEulerConventionIsIntrinsicXyzAsTheRotationsWorldPinsIt() throws {
        let world = try parseManifest(try read(root.appendingPathComponent("conformance/hierarchy_rotations.json")))
        let pose = { (name: String) in Pose(world.entities.first { $0.name == name }!.transform) }
        // Case A: [90, 90, 0] runs the beam along +X; its far tip at (−3, 2, 0).
        near(pose("case_a_beam").apply(SIMD3(0, 0, 1)), SIMD3(-3, 2, 0), tolerance: 1e-12)
        // Case B: all three angles engaged.
        near(pose("case_b_beam").apply(SIMD3(1, 0, 0)), SIMD3(0.3536, 2.9268, 0.1268), tolerance: 1e-4)
        // Case C: world = parent · local.
        let parent = pose("case_c_parent"), child = pose("case_c_child")
        near(parent.apply(child.apply(SIMD3(0, 0, 0))), SIMD3(4, 2, -1), tolerance: 1e-12)
        near(parent.apply(child.apply(SIMD3(0, 1, 0))), SIMD3(5, 2, -1), tolerance: 1e-12)
        // The cinematography reference composes origins the same way.
        let motion = WorldMotion(world.entities)
        for entity in world.entities {
            near(motion.worldOrigin(entity.id, at: 0)!, worldOrigin(entity, in: world.entities), tolerance: 1e-12, entity.name)
        }
    }

    func testQuaternionsComposeInvertAndRoundTripThroughTheirColumns() throws {
        for degrees in [SIMD3<Double>(0, 0, 0), SIMD3(30, 45, 60), SIMD3(-170, 89, 12), SIMD3(180, 0, 180)] {
            let q = Quaternion(eulerDegrees: degrees)
            let back = Quaternion(columns: q.act(SIMD3(1, 0, 0)), q.act(SIMD3(0, 1, 0)), q.act(SIMD3(0, 0, 1)))
            XCTAssertLessThan(q.angle(to: back), 1e-7, "\(degrees)")
            XCTAssertLessThan((q.inverse * q).angle(to: .identity), 1e-12)
        }
        // Looking along a direction puts +Z on it and keeps +X level.
        let look = Quaternion(lookingAlong: SIMD3(3, 4, 0))
        near(look.act(SIMD3(0, 0, 1)), SIMD3(0.6, 0.8, 0), tolerance: 1e-12)
        XCTAssertEqual(look.act(SIMD3(1, 0, 0)).y, 0, accuracy: 1e-12)
        // Straight up is nudged off the up axis, never NaN.
        let up = Quaternion(lookingAlong: SIMD3(0, 2, 0)).act(SIMD3(0, 0, 1))
        XCTAssertGreaterThan(up.y, 0.9999)
        XCTAssertFalse(up.x.isNaN || up.y.isNaN || up.z.isNaN)
        // A zero axis is no turn.
        XCTAssertEqual(Quaternion(axis: SIMD3(0, 0, 0), radians: 1), .identity)
    }
}
