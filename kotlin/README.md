# openworldformat (Kotlin · JVM + Android)

The Open World Format's Kotlin package. The core — parse a `.world`
manifest, fold its session log, ingest an author's batch — is pure
common Kotlin with one dependency (kotlinx-serialization), no engine,
no renderer, compiled for the **JVM** and for **Android** from the
same `commonMain`. It is the fold contract's fifth independently-built
implementation, sharing no code with the JS, Python, Rust or Swift
ones — every conformance world the other references fold, this
package folds too, with the same entity counts pinned in its tests.

## Add

Not on Maven Central yet — the first release, 0.4.1, is prepared but
unpublished (`org.openworldformat` doesn't exist there). Until it
lands, consume a checkout: `./gradlew publishToMavenLocal` in `kotlin/`
plus `mavenLocal()` in the consumer, or a Gradle source dependency on
this repository. After that release the coordinates are:

```kotlin
// build.gradle.kts
repositories { mavenCentral() }
dependencies {
    implementation("org.openworldformat:openworldformat:0.4.1")
}
```

Gradle's module metadata resolves `openworldformat` to the right
variant (`-jvm` on desktop/tooling, `-android` in an app) — add the
root coordinates, not a target suffix. What publishToMavenLocal
produces are the three modules the metadata picks from:
`openworldformat` (the root), `openworldformat-jvm` and
`openworldformat-android`, each with sources and javadoc jars, a POM
and a `.module`. Requires minSdk 26 on Android.

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
val variant = foldPath(pkg.base, pkg.entries, tip = "e5")

// Save-game state: typed fields, folded over the declaration.
val values = pkg.stateValues().values   // ["score.tour" to 1]

// Authoring: a batch by name, partial patches merged, refused whole.
val batch = Json.parseToJsonElement(
    """[{"ModifyEntity": {"id": "lantern",
         "patch": {"transform": {"position": [-12, 0.5, 3]}}}}]""",
)
when (val result = ingest(state, batch)) {
    is IngestResult.Ingested -> {
        result.ops        // to commit: names bound to ids, patches merged whole
        result.state      // the world the batch makes
        manifestText(toManifest(result.state))  // the head's canonical bytes
    }
    is IngestResult.Refused ->
        result.errors     // one reason per failing op; nothing was written
}
```

Piece by piece, if a package is more than folders for you:

```kotlin
val base = parseManifest(baseText)       // snapshots/base.json
val entries = logText.split('\n').filter { it.isNotBlank() }.map(::parseLogLine)
val world = foldLog(base, entries)       // the world at head
val same = toManifest(world)             // a whole manifest again
```

`pkg.manifest` is the world now — a head-first package's
`manifest.json`, all a viewer needs; `pkg.base` is `snapshots/base.json`,
what the log folds from. The fold is total (`toManifest`), `ModifyWorld`
applies and inverts, `compactWorldPackage` moves the base up to the head,
and an entry's `message` is part of its identity (spec draft 0.3).
Authoring is `ingest` (spec/session.md, "Authoring"): names bound to
ids, ids allocated for spawns that left theirs out, struct patches
merged as a JSON merge patch (`mergePatch`, RFC 7396), read strictly,
all-or-nothing, with `warnings` on the result for what validation warns
about in the new world (a behavior whose target the batch deleted) — and
`manifestText` writes the head's canonical bytes, the same bytes the
other references write.

On Android, content-URI sources go through the app's own copy layer,
then the same `loadWorldPackage(File)`.

## Schemas

The normative JSON Schemas ship with the package, as KMP resources
in `commonMain`, so a consumer validates against its installed
dependency instead of a checkout of the format repository:

```kotlin
val world = OWF_JSON.parseToJsonElement(schemaText("world.schema.json"))
// or keep the text, and hand it to the JSON Schema validator of
// choice — the package itself stays a reader, not a checker
```

`schemaText` takes the name as the repository's `schema/` holds it:
`"state.schema.json"`, `"extensions/ext-physics.schema.json"`,
`"extensions/ext-cinematography.schema.json"`,
`"extensions/ext-h3.schema.json"` — byte-identical copies, never
hand-edited; `SchemaFilesTests` fails when they drift, so refresh
them when the schema changes. The copies ride at the classpath root
(`schemas/…`): the JVM jar carries them, and the Android artifact's
`classes.jar` does too, so `schemaText` reads them on either target
with no `Context`. (The entity-reference list,
`schema/entity-refs.json`, is already embedded as source:
`EntityRefs.kt`.)

## What it carries, what it doesn't

- **Carries** — the document types (identity typed, components and
  `ext-*` fields riding untouched in `JsonElement` per must-ignore),
  op classification (edits first, the compatibility rule), the fold
  (all-or-nothing per entry and per batch), branching histories
  (`buildHistory`, `foldPath`), the state fold (declared,
  dotted-map-subkey and undeclared keys), the package loader (a torn
  last line skipped per spec), and the Authoring profile's batch
  ingestion (`ingest`, `mergePatch`) with the canonical head text
  (`manifestText`).
- **Doesn't** — the `ext-physics` solver (the Rust crate owns the
  reference one), schema validation (the schemas themselves ship —
  `schemaText`; the checker is the consumer's, AJV in CI here), and
  any rendering, audio or asset resolution. The viewer profile's
  must-ignore is the reader's side.

## Build and test

```bash
./gradlew jvmTest            # 78 tests: examples, conformance worlds, the shared merge corpus, authoring, the schemas
./gradlew compileAndroidMain # the Android target compiles
./gradlew publishToMavenLocal
```

## Releasing (the Maven Central path)

Unlike SPM (git tags) this one is an upload, and unlike npm, PyPI and
crates.io there is no trusted publishing: the Central Portal
authenticates with a user token and a GPG key held in GitHub secrets.

**What only the owner can do, once, before the first release:**

1. Create a Central Portal account and verify the
   `org.openworldformat` namespace on it — DNS TXT on
   openworldformat.org, which this project owns.
2. Generate a Portal user token (a username/password pair).
3. Create a GPG key and publish its public half to a keyserver
   (Central checks signatures against published keys).
4. On GitHub, create the repository secrets the workflow reads —
   `MAVEN_CENTRAL_USERNAME` and `MAVEN_CENTRAL_PASSWORD` (the token
   pair), `MAVEN_GPG_PRIVATE_KEY` (the armored private key) and
   `MAVEN_GPG_PASSPHRASE` — and, if a second look is wanted before an
   upload, the `maven-central` environment with required reviewers.
   Secret values live in GitHub, never in this repository.

**The release flow** (`.github/workflows/publish-kotlin.yml`): main
carries `0.4.1-SNAPSHOT`; a release commit drops the suffix
(`chore(kotlin): 0.4.1`) and the tag points at it:

```bash
git tag -a kotlin-v0.4.1 -m "openworldformat (Kotlin) 0.4.1 — <what>"
git push origin kotlin-v0.4.1
```

then main moves to `0.4.2-SNAPSHOT`. The workflow refuses a tag that
doesn't name the build's version and refuses any version still
carrying `-SNAPSHOT`, so a stray tag on main publishes nothing. It
runs the CI checks plus a `publishToMavenLocal` (the artifact set
Central will judge), then PUTs the signed artifacts to the staging
service Gradle's maven-publish speaks and hands the deployment to the
Portal, which validates and releases automatically — validation is
asynchronous, and a failure parks the deployment in the Portal's UI
for the owner rather than going out. Until the first release lands,
consumers use `mavenLocal()` or a source dependency as above. The
package's 0.x minor is its own binding-compatibility line, not the
spec draft's number ([versioning policy](../spec/versioning.md)).

## License

Apache-2.0, like the rest of the repository.
