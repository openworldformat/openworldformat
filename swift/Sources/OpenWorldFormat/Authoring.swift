// Authoring: what an authority does with a batch of ops an author sends
// (spec/session.md, "Authoring"). The fold reads committed ops; this is
// the step before — turning what an agent, a script or a person wrote
// into ops worth committing, or refusing them with reasons.
//
// Each op, in order, against a trial document that already holds the
// batch's earlier ops:
//
// 1. **Bind** — a string where an entity id goes is a name, resolved to
//    the id it names now; a spawn without an `id` gets the next one.
// 2. **Merge** — the object-valued struct fields of a patch
//    (`ModifyEntity`'s `transform`, `material`, `light`; `SetEnvironment`'s
//    `env`; `ModifyWorld`'s `meta`, `environment`, `camera`, `avatar`,
//    `soundtrack`) merge into the current value as a JSON merge patch
//    (RFC 7396). The committed op carries the merged whole.
// 3. **Read strictly** — no key the format would drop.
// 4. **Apply** — to the trial.
//
// Any failure refuses the whole batch, with a reason per failing op.
// Files are the host's: this package does no I/O — the authority that
// calls this writes the entry and the head (in `manifestText`'s
// canonical bytes).

import Foundation

/// The edit kinds in the order the other references list them — the
/// refusal messages quote this list.
let opKindList: [String] = [
    "SpawnEntity",
    "DeleteEntity",
    "ModifyEntity",
    "SetEnvironment",
    "SetCamera",
    "SetAmbience",
    "SpawnAudioEmitter",
    "RemoveAudioEmitter",
    "ModifyWorld",
    "Batch",
]

/// A batch the world can take.
public struct Ingested: Equatable, Sendable {
    /// The ops to commit: names bound to ids, struct patches merged.
    public var ops: [JSONValue]
    /// Entities the batch spawns, name → id.
    public var spawned: [String: Int]
    /// The world after the batch.
    public var state: FoldState
}

/// `ingest`'s reply: a batch taken whole, or refused whole — one reason
/// per failing op (`op 2: …`), never a partial write.
public enum IngestResult: Equatable, Sendable {
    case ingested(Ingested)
    case refused(errors: [String])
}

/// Ingest a batch — `[op, …]` or `{"ops": [op, …], …}` — against a fold
/// state. The state passed in is untouched (a refusal changes nothing,
/// and a success hands the new world back in `.ingested`).
public func ingest(_ state: FoldState, _ batch: JSONValue) -> IngestResult {
    let notABatch = "a batch is [op, …] or {\"ops\": [op, …]}"
    let ops: [JSONValue]
    switch batch {
    case .array(let array):
        ops = array
    case .object(let body):
        guard case .array(let array)? = body["ops"] else {
            return .refused(errors: [notABatch])
        }
        ops = array
    default:
        return .refused(errors: [notABatch])
    }
    if ops.isEmpty {
        return .refused(errors: ["the batch holds no ops"])
    }
    var trial = state
    var spawned: [String: Int] = [:]
    var committed: [JSONValue] = []
    var errors: [String] = []
    for (i, raw) in ops.enumerated() {
        do {
            var op = raw
            try bindOp(trial, &op, &spawned)
            try strictOp(op)
            guard case let .edit(kind, value) = classifyOp(op) else {
                throw OpenWorldFormatError.invalid("only edit ops can be sent")
            }
            // An op is an entry of one: apply, then bind the names it wrote.
            var step = trial
            try applyEdit(&step, kind, value)
            for id in touchedEntities([.edit(name: kind, value: value)]) {
                if let index = step.entities.firstIndex(where: { $0.id == id }) {
                    try resolveNames(&step, index)
                }
            }
            trial = step
            committed.append(op)
        } catch {
            errors.append("op \(i): \(message(of: error))")
        }
    }
    if !errors.isEmpty {
        return .refused(errors: errors)
    }
    trial.appliedEdits = state.appliedEdits + committed.count
    return .ingested(Ingested(ops: committed, spawned: spawned, state: trial))
}

/// The raw message of an error, without the `invalid:` prefix the fold's
/// thrown form carries — the per-op reasons read as the other references'
/// do.
private func message(of error: Error) -> String {
    if case let OpenWorldFormatError.parse(m) = error { return m }
    if case let OpenWorldFormatError.invalid(m) = error { return m }
    return (error as? LocalizedError)?.errorDescription ?? String(describing: error)
}

// MARK: - Bind

/// Names to ids, ids for spawns that left theirs out, merged struct
/// patches — on the raw JSON, before it is read.
func bindOp(_ trial: FoldState, _ op: inout JSONValue, _ spawned: inout [String: Int]) throws {
    guard case var .object(o) = op else {
        throw OpenWorldFormatError.invalid("an op is an object like {\"SpawnEntity\": {…}}")
    }
    guard o.count == 1, let (kind, body) = o.first else {
        throw OpenWorldFormatError.invalid(
            "an op holds exactly one kind, one of: \(opKindList.joined(separator: ", "))")
    }
    var bodyO = body.object ?? [:]

    /// A name becomes the id it names now; ids pass through.
    func resolve(_ reference: JSONValue) throws -> JSONValue {
        guard case let .string(name) = reference else { return reference }
        let id = spawned[name] ?? trial.entities.first(where: { $0.name == name })?.id
        guard let id else {
            throw OpenWorldFormatError.invalid("no entity is named \"\(name)\"")
        }
        return .number(Double(id))
    }

    /// The top-level bindable entity refs (today: `parent`) bind at op
    /// intake, before the op applies; the behavior refs bind after,
    /// against the spawned world (`resolveNames`) — the same list,
    /// walked per pass.
    func bindTopLevelRefs(_ value: inout JSONValue) throws {
        for ref in refsOf("entity", kind: "bindable") where ref.path.count == 1 {
            try walkRefPath(&value, ref.path, 0) { leaf in
                leaf = try resolve(leaf)
            }
        }
    }

    switch kind {
    case "SpawnEntity":
        guard var entity = bodyO["entity"]?.object else {
            throw OpenWorldFormatError.invalid("SpawnEntity needs an \"entity\" object")
        }
        if entity["id"] == nil || entity["id"] == .null {
            // The next id: past every id the trial holds, and every id
            // this batch already handed out (a Batch binds before it
            // applies, so the trial hasn't seen its own spawns yet).
            let declared = trial.scene["next_entity_id"]?.int ?? 1
            let handedOut = spawned.values.map { $0 + 1 }.max() ?? 0
            entity["id"] = .number(Double(max(declared, handedOut)))
        } else if entity["id"]?.double == nil {
            throw OpenWorldFormatError.invalid(
                "a new entity's id is a number, or left out to get one")
        }
        let id = entity["id"]!.int!
        var entityValue = JSONValue.object(entity)
        try bindTopLevelRefs(&entityValue)
        if case let .object(o) = entityValue { entity = o }
        if let name = entity["name"]?.string {
            spawned[name] = id
        }
        bodyO["entity"] = .object(entity)

    case "ModifyEntity":
        guard let reference = bodyO["id"] else {
            throw OpenWorldFormatError.invalid("ModifyEntity needs an \"id\" (or a name)")
        }
        bodyO["id"] = try resolve(reference)
        let current = trial.entities.first(where: { $0.id == bodyO["id"]?.int })?.fields
        if var patch = bodyO["patch"]?.object {
            var patchValue = JSONValue.object(patch)
            try bindTopLevelRefs(&patchValue)
            if case let .object(o) = patchValue { patch = o }
            if let current {
                for field in ["transform", "material", "light"] {
                    if case .object(let change)? = patch[field],
                       case .object(let now)? = current[field] {
                        patch[field] = mergePatch(.object(now), .object(change))
                    }
                }
            }
            bodyO["patch"] = .object(patch)
        }

    case "DeleteEntity":
        guard let reference = bodyO["id"] else {
            throw OpenWorldFormatError.invalid("DeleteEntity needs an \"id\" (or a name)")
        }
        bodyO["id"] = try resolve(reference)

    case "SetEnvironment":
        if case .object(let change)? = bodyO["env"], let now = trial.environment {
            bodyO["env"] = mergePatch(now.json, .object(change))
        }

    case "ModifyWorld":
        if var patch = bodyO["patch"]?.object {
            // The avatar's marked refs (today: `model_entity`) bind
            // like any other — a name where an entity id goes resolves
            // at intake (spec/world.md: refs MUST resolve at ingestion).
            if var avatar = patch["avatar"], avatar.object != nil {
                try eachRef("avatar", &avatar) { leaf in
                    leaf = try resolve(leaf)
                }
                patch["avatar"] = avatar
            }
            let now = (try? toManifest(trial))?.json.object ?? [:]
            for field in ["meta", "environment", "camera", "avatar", "soundtrack"] {
                if case .object(let change)? = patch[field],
                   case .object(let current)? = now[field] {
                    patch[field] = mergePatch(.object(current), .object(change))
                }
            }
            bodyO["patch"] = .object(patch)
        }

    case "Batch":
        guard var inner = bodyO["ops"]?.array else {
            throw OpenWorldFormatError.invalid("Batch needs an \"ops\" array")
        }
        for i in inner.indices {
            try bindOp(trial, &inner[i], &spawned)
        }
        bodyO["ops"] = .array(inner)

    case let other:
        guard EDIT_KEYS.contains(other) else {
            throw OpenWorldFormatError.invalid(
                "\"\(other)\" isn't an op kind; the format has: "
                    + opKindList.joined(separator: ", "))
        }
    }
    // A body that wasn't an object passes through untouched — no case
    // could have bound anything into it, and inventing an empty one
    // would paper over the malformed op.
    if body.object != nil {
        o[kind] = .object(bodyO)
        op = .object(o)
    }
}

// MARK: - Strict reading

/// The keys a patch's struct fields may carry, from the schema's $defs —
/// strict ingestion refuses anything else, with a JSON pointer to it
/// (spec/profiles.md, "Strict Mode"). Registered `ext-*` keys pass, as
/// everywhere.
private let transformKeys: Set<String> = ["position", "rotation_degrees", "scale", "visible"]
private let materialKeys: Set<String> = [
    "alpha_mode", "base_color_texture", "color", "double_sided", "emissive",
    "emissive_texture", "metallic", "metallic_roughness_texture",
    "normal_map_texture", "reflectance", "roughness", "unlit",
]
private let lightKeys: Set<String> = [
    "color", "direction", "inner_angle", "intensity", "light_type",
    "outer_angle", "range", "shadows",
]
private let environmentKeys: Set<String> = [
    "ambient_color", "ambient_intensity", "background_color", "fog_color",
    "fog_density",
]
private let cameraKeys: Set<String> = ["fov_degrees", "look_at", "position"]

/// Refuse keys the format would drop, with a path to each. Explicit
/// nulls are fine (they clear).
func strictOp(_ op: JSONValue) throws {
    var unknown: [String] = []

    func check(_ object: JSONValue?, _ allowed: Set<String>, _ path: String) {
        guard case let .object(o)? = object else { return }
        for key in o.keys {
            if allowed.contains(key) { continue }
            if key.hasPrefix("ext-") && REGISTERED_EXTENSIONS.contains(key) { continue }
            unknown.append("\(path)/\(key)")
        }
    }
    func checkEntity(_ entity: JSONValue?, _ path: String, isPatch: Bool) {
        var keys = strictEntityKeys
        if isPatch { keys.remove("id") }
        check(entity, keys, path)
        check(entity?["transform"], transformKeys, "\(path)/transform")
        check(entity?["material"], materialKeys, "\(path)/material")
        check(entity?["light"], lightKeys, "\(path)/light")
    }

    guard case let .object(o) = op, o.count == 1, let (kind, body) = o.first else { return }
    switch kind {
    case "SpawnEntity":
        check(body, ["entity"], "/\(kind)")
        checkEntity(body["entity"], "/\(kind)/entity", isPatch: false)
    case "ModifyEntity":
        check(body, ["id", "patch"], "/\(kind)")
        checkEntity(body["patch"], "/\(kind)/patch", isPatch: true)
    case "DeleteEntity":
        check(body, ["id"], "/\(kind)")
    case "SetEnvironment":
        check(body["env"], environmentKeys, "/\(kind)/env")
    case "SetCamera":
        check(body["camera"], cameraKeys, "/\(kind)/camera")
    case "ModifyWorld":
        check(body["patch"], Set(WORLD_PATCH_KEYS), "/\(kind)/patch")
        check(body["patch"]?["meta"], strictMetaKeys, "/\(kind)/patch/meta")
        check(body["patch"]?["environment"], environmentKeys, "/\(kind)/patch/environment")
        check(body["patch"]?["camera"], cameraKeys, "/\(kind)/patch/camera")
    case "Batch":
        for inner in body["ops"]?.array ?? [] {
            try strictOp(inner)
        }
    default:
        break
    }
    if !unknown.isEmpty {
        throw OpenWorldFormatError.invalid(
            unknown.map { "\($0) is not a field of the format" }.joined(separator: "; "))
    }
}

// MARK: - JSON merge patch

/// JSON merge patch (RFC 7396): objects merge key by key, `null`
/// removes, anything else replaces — the rule ingestion merges a patch's
/// struct fields by.
public func mergePatch(_ current: JSONValue, _ change: JSONValue) -> JSONValue {
    guard case let .object(now) = current, case let .object(delta) = change else {
        return change
    }
    var out = now
    for (key, value) in delta {
        if value == .null {
            out.removeValue(forKey: key)
        } else {
            out[key] = out[key].map { mergePatch($0, value) } ?? value
        }
    }
    return .object(out)
}
