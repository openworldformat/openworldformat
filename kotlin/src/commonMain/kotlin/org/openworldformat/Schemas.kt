// The normative schemas the package ships (the repository's schema/):
// byte-identical copies of world.schema.json, state.schema.json and
// the three extension schemas, riding in commonMain's resources so
// the JVM jar and the Android artifact both carry them at the
// classpath root (schemas/…) — schemaText(name) reads one through
// the platform classloader, so a consumer validates a .world against
// its installed dependency instead of a checkout of the format
// repository.
//
// Never hand-edited past a schema change: SchemaFilesTests fails when
// a copy drifts from schema/, so refresh the whole tree then. (The
// entity-reference list, schema/entity-refs.json, is already embedded
// as source: EntityRefs.kt.)

package org.openworldformat

/**
 * The text of one of the normative JSON Schemas the package ships —
 * `schemaText("world.schema.json")`, `schemaText("state.schema.json")`
 * and the extension schemas (`"extensions/ext-physics.schema.json"`,
 * `"extensions/ext-cinematography.schema.json"`,
 * `"extensions/ext-h3.schema.json"`): hand it to the JSON Schema
 * validator of choice, parsed or as text. The name is the schema's
 * path as the repository's schema/ holds it; the copies are
 * byte-identical to it, never hand-edited.
 *
 * @throws NoSuchElementException when the name is no schema this
 * package ships
 */
public expect fun schemaText(name: String): String
