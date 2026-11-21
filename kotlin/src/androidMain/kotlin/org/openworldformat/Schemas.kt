// The normative schemas' access path — the Android target: the copies
// in commonMain's resources ride in the artifact's classes.jar, AGP
// packs them into the app at the classpath root, and the runtime
// classloader serves them like any entry — no Context, no assets.
// (The same code lives in jvmMain.)

package org.openworldformat

/** A marker so the lookup goes through this package's own classloader. */
private object SchemasLocator

/** Reads one of the shipped schemas, as [schemaText] states. */
public actual fun schemaText(name: String): String {
    val stream = SchemasLocator::class.java.getResourceAsStream("/schemas/$name")
        ?: throw NoSuchElementException("no schema '$name' ships with this package")
    return stream.use { it.readBytes().decodeToString() }
}
