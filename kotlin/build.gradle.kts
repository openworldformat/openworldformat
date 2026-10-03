// The Open World Format's Kotlin package: the fold in common code,
// compiled for the JVM and for Android. The fifth reference — same
// contract as js/, python/, rust/ and swift/, no engine, no renderer.
plugins {
    kotlin("multiplatform") version "2.2.10"
    kotlin("plugin.serialization") version "2.2.10"
    id("com.android.kotlin.multiplatform.library") version "9.4.1"
    `maven-publish`
}

group = "org.openworldformat"
version = "0.2.0-SNAPSHOT"

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

// Maven Central metadata — the publish itself (signing, Central
// Portal upload) is a release-time step; see kotlin/README.md.
publishing {
    publications.withType<MavenPublication>().configureEach {
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
}
