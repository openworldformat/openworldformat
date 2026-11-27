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
    /// What validation warns about in the new world — a behavior whose
    /// target the batch deleted — in the words the Rust crate's validation
    /// uses. Warnings never refuse (spec/session.md "Authoring"); an
    /// authority puts them on the receipt.
    public var warnings: [String]
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
            // The committed op carries ids in every marked field, like
            // the document it just changed.
            try bindCommittedRefs(&op, in: step)
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
    return .ingested(Ingested(
        ops: committed, spawned: spawned, state: trial, warnings: receiptWarnings(trial)
    ))
}

/// What the batch left that validation warns about and never refuses
/// (spec/session.md "Authoring": warnings come back on the receipt): a
/// behavior whose target the world no longer holds — a `LookAt` target or
/// an `Orbit` center whose entity a delete took — in the words the Rust
/// crate's validation uses, so a receipt reads the same from every
/// reference.
func receiptWarnings(_ state: FoldState) -> [String] {
    var warnings: [String] = []
    for entity in state.entities {
        guard case .array(let behaviors)? = entity.fields["behaviors"] else { continue }
        for behavior in behaviors {
            let kind: String
            let target: JSONValue?
            if case .object(let look)? = behavior["LookAt"] {
                (kind, target) = ("look_at", look["target"])
            } else if case .object(let orbit)? = behavior["Orbit"] {
                (kind, target) = ("orbit", orbit["center"])
            } else {
                continue
            }
            let gone: String
            switch target {
            case .number(let n)?:
                if let id = Int(exactly: n) {
                    if state.idToIndex[id] != nil { continue }
                    gone = "entity \(id)"
                } else {
                    gone = "entity \(n)"
                }
            case .string(let name)?:
                if state.nameToId[name] != nil { continue }
                gone = "'\(name)'"
            default:
                continue
            }
            warnings.append(
                "Entity '\(entity.name)' has a \(kind) behavior targeting \(gone), "
                    + "which is not in the world (it looks at nothing until retargeted or dropped)"
            )
        }
    }
    return warnings
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

/// Strict authoring for a known ext-cinematography 0.4 track. This is
/// validation only; it does not advertise or perform track evaluation.
private func validateCinematographyTrack(_ block: JSONValue?) throws {
    guard let block, block.object != nil, let trackValue = block["track"] else { return }
    let root = "/ext-cinematography/track"
    func fail(_ path: String, _ message: String) throws -> Never {
        throw OpenWorldFormatError.invalid("\(path): \(message)")
    }
    func object(_ value: JSONValue?, _ path: String) throws -> [String: JSONValue] {
        guard let object = value?.object else { try fail(path, "expected an object") }
        return object
    }
    func known(_ object: [String: JSONValue], _ allowed: Set<String>, _ path: String) throws {
        for key in object.keys where !allowed.contains(key) {
            try fail("\(path)/\(key)", "unknown track field")
        }
    }
    func number(_ object: [String: JSONValue], _ field: String, _ path: String) throws -> Double {
        let pointer = "\(path)/\(field)"
        guard let value = object[field] else { try fail(pointer, "\(field) is required") }
        guard let number = value.double, number.isFinite else {
            try fail(pointer, "expected a finite number")
        }
        return number
    }
    func enumValue(_ object: [String: JSONValue], _ field: String, _ path: String,
                   _ allowed: Set<String>, _ fallback: String) throws -> String {
        guard let value = object[field] else { return fallback }
        let pointer = "\(path)/\(field)"
        guard let string = value.string else { try fail(pointer, "expected a string enum") }
        guard allowed.contains(string) else {
            try fail(pointer, "expected \(allowed.sorted().joined(separator: ", "))")
        }
        return string
    }

    let track = try object(trackValue, root)
    try known(track, ["clip", "pre", "post", "channels"], root)
    var start: Double?
    var end: Double?
    let explicitClip = track["clip"] != nil
    if let clipValue = track["clip"] {
        let path = "\(root)/clip"
        let clip = try object(clipValue, path)
        try known(clip, ["start_s", "end_s"], path)
        let clipStart = try number(clip, "start_s", path)
        let clipEnd = try number(clip, "end_s", path)
        if clipStart > clipEnd { try fail("\(path)/end_s", "clip end_s must not precede start_s") }
        start = clipStart
        end = clipEnd
    }
    let pre = try enumValue(track, "pre", root, ["hold", "base", "loop"], "hold")
    let post = try enumValue(track, "post", root, ["hold", "base", "loop"], "hold")
    let channelsPath = "\(root)/channels"
    guard let channelsValue = track["channels"] else { try fail(channelsPath, "channels is required") }
    let channels = try object(channelsValue, channelsPath)
    try known(channels, ["translation", "rotation", "scale"], channelsPath)
    if channels.isEmpty { try fail(channelsPath, "at least one channel is required") }
    for name in ["translation", "rotation", "scale"] {
        guard let keysValue = channels[name] else { continue }
        let channelPath = "\(channelsPath)/\(name)"
        guard let keys = keysValue.array else {
            try fail(channelPath, "a channel must be an array of keys")
        }
        if keys.isEmpty { try fail(channelPath, "a channel must contain at least one key") }
        var previous: Double?
        for (index, keyValue) in keys.enumerated() {
            let keyPath = "\(channelPath)/\(index)"
            let key = try object(keyValue, keyPath)
            try known(key, ["time_s", "value", "interpolation", "easing"], keyPath)
            let time = try number(key, "time_s", keyPath)
            if !explicitClip {
                start = start.map { min($0, time) } ?? time
                end = end.map { max($0, time) } ?? time
            }
            if let previous, time <= previous {
                try fail("\(keyPath)/time_s", "key times must be strictly increasing")
            }
            previous = time
            let valuePath = "\(keyPath)/value"
            guard let vectorValue = key["value"] else { try fail(valuePath, "value is required") }
            guard let vector = vectorValue.array else {
                try fail(valuePath, "value must be an array of three numbers")
            }
            if vector.count != 3 { try fail(valuePath, "value must contain exactly three numbers") }
            for (axis, component) in vector.enumerated() {
                guard let number = component.double, number.isFinite else {
                    try fail("\(valuePath)/\(axis)", "value components must be finite numbers")
                }
            }
            _ = try enumValue(key, "interpolation", keyPath, ["linear", "step"], "linear")
            _ = try enumValue(key, "easing", keyPath,
                              ["linear", "ease_in", "ease_out", "ease_in_out"], "linear")
        }
    }
    if let start, let end, start == end {
        if pre == "loop" { try fail("\(root)/pre", "loop requires a nonzero clip duration") }
        if post == "loop" { try fail("\(root)/post", "loop requires a nonzero clip duration") }
    }
}

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
        try validateCinematographyTrack(body["entity"]?["ext-cinematography"])
    case "ModifyEntity":
        check(body, ["id", "patch"], "/\(kind)")
        checkEntity(body["patch"], "/\(kind)/patch", isPatch: true)
        try validateCinematographyTrack(body["patch"]?["ext-cinematography"])
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
