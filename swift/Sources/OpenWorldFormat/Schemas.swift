// The normative schemas the package ships: byte-identical copies of
// the repository's schema/ (world.schema.json, state.schema.json and
// the three extension schemas), declared in Package.swift as SwiftPM
// resources — the `.copy` rule is byte-identical by construction — so
// a consumer validates a .world against its installed dependency
// instead of a checkout of the format repository.
//
// Never hand-edited past a schema change: SchemaFilesTests fails when
// a copy drifts from schema/, so refresh the whole tree then. (The
// entity-reference list, schema/entity-refs.json, is already embedded
// as source: EntityRefs.swift.)

import Foundation

/// The location of one of the normative JSON Schemas the package
/// ships — `schemaURL("world.schema.json")`, `schemaURL("state.schema.json")`
/// and the extension schemas (`"extensions/ext-physics.schema.json"`,
/// `"extensions/ext-cinematography.schema.json"`,
/// `"extensions/ext-h3.schema.json"`): hand the URL to a validator,
/// or read it and parse it. The name is the schema's path as the
/// repository's schema/ holds it.
///
/// - Returns: nil when the name is no schema this package ships.
public func schemaURL(_ name: String) -> URL? {
    guard let url = Bundle.module.resourceURL?
        .appendingPathComponent("schemas")
        .appendingPathComponent(name)
    else { return nil }
    return FileManager.default.fileExists(atPath: url.path) ? url : nil
}
