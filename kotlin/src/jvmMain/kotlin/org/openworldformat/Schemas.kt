// The normative schemas' access path — the JVM target: the copies in
// commonMain's resources sit on this target's classpath (schemas/…),
// so the classloader reads them out of the jar exactly as a consumer's
// does. (The same code lives in androidMain; the APK keeps the
// classpath entries too, AGP packaging them from the artifact.)

package org.openworldformat

/** A marker so the lookup goes through this package's own classloader. */
private object SchemasLocator

/** Reads one of the shipped schemas, as [schemaText] states. */
public actual fun schemaText(name: String): String {
    val stream = SchemasLocator::class.java.getResourceAsStream("/schemas/$name")
        ?: throw NoSuchElementException("no schema '$name' ships with this package")
    return stream.use { it.readBytes().decodeToString() }
}
