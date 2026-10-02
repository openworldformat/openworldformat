# openworldformat (Kotlin · JVM + Android)

The Open World Format's Kotlin package. The core — parse a `.world`
manifest, fold its session log — is pure common Kotlin with one
dependency (kotlinx-serialization), no engine, no renderer, compiled
for the **JVM** and for **Android** from the same `commonMain`. It is
the fold contract's fifth independently-built implementation, sharing
no code with the JS, Python, Rust or Swift ones — every conformance
world the other references fold, this package folds too, with the
same entity counts pinned in its tests.

## Add

```kotlin
// build.gradle.kts
repositories { mavenCentral() }
dependencies {
    implementation("org.openworldformat:openworldformat:0.1.0")
}
```

Gradle's module metadata resolves `openworldformat` to the right
variant (`-jvm` on desktop/tooling, `-android` in an app) — add the
root coordinates, not a target suffix. Requires minSdk 26 on Android.

## Use

```kotlin
import org.openworldformat.*

val pkg = loadWorldPackage(File("/path/to/a/.world"))  // a folder
val state = pkg.folded()          // the world at head revision
state.name                        // "hello-world"
state.entities                    // base + every applied edit
state.appliedEdits               // how many edits the log held

for (entity in state.entities) {
    val position = entity.transform?.position   // Vec3(x, y, z)
    val shape = entity.shape                    // Shape.Sphere(0.75), …
    val material = entity.material              // color, roughness, …
}

// Branches: the same log, a different tip.
val history = pkg.history()       // tips, children
val variant = foldPath(pkg.manifest, pkg.entries, tip = "e5")

// Save-game state: typed fields, folded over the declaration.
val values = pkg.stateValues().values   // ["score.tour" to 1]
```

Piece by piece, if a package is more than folders for you:

```kotlin
val manifest = parseManifest(manifestText)
val entries = logText.split('\n').filter { it.isNotBlank() }.map(::parseLogLine)
val world = foldLog(manifest, entries)   // the world at head
```

On Android, content-URI sources go through the app's own copy layer,
then the same `loadWorldPackage(File)`.

## What it carries, what it doesn't

- **Carries** — the document types (identity typed, components and
  `ext-*` fields riding untouched in `JsonElement` per must-ignore),
  op classification (edits first, the compatibility rule), the fold
  (all-or-nothing per entry and per batch), branching histories
  (`buildHistory`, `foldPath`), the state fold (declared,
  dotted-map-subkey and undeclared keys), and the package loader
  (a torn last line skipped per spec).
- **Doesn't** — the `ext-physics` solver (the Rust crate owns the
  reference one), schema validation (the JSON Schema is normative;
  AJV in CI is the checker), and any rendering, audio or asset
  resolution. The viewer profile's must-ignore is the reader's side.

## Build and test

```bash
./gradlew jvmTest            # 18 tests over the repo's own examples
./gradlew compileAndroidMain # the Android target compiles
./gradlew publishToMavenLocal
```

## Releasing (the Maven Central path)

Unlike SPM (git tags) this one is an upload, not a tag. The POM
metadata is already configured here; the remaining release-time
steps: verify the `org.openworldformat` namespace on Central Portal
(DNS TXT on openworldformat.org, which this project owns), GPG-sign,
drop the `-SNAPSHOT`, and `publish…PublicationToCentralPortal`.
Until then the version stays `0.1.0-SNAPSHOT` and consumers can use
`mavenLocal()` or a Gradle sourceDependency on this repository.

## License

Apache-2.0, like the rest of the repository.
