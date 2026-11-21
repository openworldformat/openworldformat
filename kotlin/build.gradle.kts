// The Open World Format's Kotlin package: the fold in common code,
// compiled for the JVM and for Android. The fifth reference — same
// contract as js/, python/, rust/ and swift/, no engine, no renderer.
plugins {
    kotlin("multiplatform") version "2.2.10"
    kotlin("plugin.serialization") version "2.2.10"
    id("com.android.kotlin.multiplatform.library") version "9.4.1"
    `maven-publish`
    signing
}

group = "org.openworldformat"
version = "0.4.1-SNAPSHOT"

kotlin {
    jvm()

    // The Android target, via AGP 9's built-in-Kotlin KMP plugin (the
    // old com.android.library bridge is gone since AGP 9.0).
    androidLibrary {
        namespace = "org.openworldformat"
        compileSdk = 36
        minSdk = 26
    }

    sourceSets {
        commonMain.dependencies {
            // JsonElement trees: the format's passthrough half, held
            // exactly as the document held it (nulls included).
            api("org.jetbrains.kotlinx:kotlinx-serialization-json:1.9.0")
        }
        jvmTest.dependencies {
            implementation(kotlin("test"))
        }
    }
}

// Maven Central metadata and release wiring — the upload itself runs
// in .github/workflows/publish-kotlin.yml on a `kotlin-vX.Y.Z` tag;
// see kotlin/README.md's Releasing section.
publishing {
    publications.withType<MavenPublication>().configureEach {
        // Central's validation asks for a javadoc jar beside every
        // module; the docs here are the README, so the jar is a stub.
        // (The sources jars the Kotlin and Android plugins already
        // attach need no help.) Distinct local file names per task —
        // the repository name the publication writes is its own.
        val module = if (name == "kotlinMultiplatform") "openworldformat"
                     else "openworldformat-$name"
        artifact(tasks.register("${name}JavadocJar", Jar::class) {
            archiveFileName.set("$module-$version-javadoc.jar")
            archiveClassifier.set("javadoc")
        })
        pom {
            name = "openworldformat"
            description =
                "The Open World Format: parse a .world manifest and fold " +
                    "its session log — pure Kotlin, no engine required."
            url = "https://openworldformat.org"
            licenses {
                license {
                    name = "Apache-2.0"
                    url = "https://www.apache.org/licenses/LICENSE-2.0"
                }
            }
            scm {
                connection = "scm:git:git@github.com:openworldformat/openworldformat.git"
                developerConnection = "scm:git:git@github.com:openworldformat/openworldformat.git"
                url = "https://github.com/openworldformat/openworldformat"
            }
            developers {
                developer {
                    id = "yw"
                    name = "Yi Wang"
                }
            }
        }
    }

    // The Central Portal, over Sonatype's OSSRH staging API — the
    // compatibility service Gradle's maven-publish can PUT to (the
    // Portal's own API speaks bundle uploads, which maven-publish
    // doesn't; Sonatype publishes no official Gradle plugin). The
    // repository materializes only when a release run supplies a
    // portal token, so everyday builds never see it — and the
    // publish…PublicationToCentralPortalRepository tasks it names
    // don't exist to run by accident.
    val centralUser = System.getenv("MAVEN_CENTRAL_USERNAME")
    val centralPassword = System.getenv("MAVEN_CENTRAL_PASSWORD")
    if (!centralUser.isNullOrBlank() && !centralPassword.isNullOrBlank()) {
        repositories {
            maven {
                name = "CentralPortal"
                url = uri(
                    "https://ossrh-staging-api.central.sonatype.com" +
                        "/service/local/staging/deploy/maven2/"
                )
                credentials {
                    username = centralUser
                    password = centralPassword
                }
            }
        }
    }
}

// Signatures: Central refuses unsigned artifacts. The key rides in
// memory, from the same MAVEN_GPG_* environment the release workflow
// sets; without it (everyday builds) nothing is signed and nothing
// can reach Central anyway.
signing {
    val gpgKey = System.getenv("MAVEN_GPG_PRIVATE_KEY")
    if (!gpgKey.isNullOrBlank()) {
        useInMemoryPgpKeys(gpgKey, System.getenv("MAVEN_GPG_PASSPHRASE") ?: "")
        publishing.publications.withType<MavenPublication>().configureEach {
            sign(this)
        }
    }
}
