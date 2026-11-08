plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.serialization)
}

// The receipt codec is the Rust crate `orf-codec`, through `orf-codec-ffi`'s
// UniFFI bindings (docs/decisions/android.md 8), which `:ffi` builds.
android {
    namespace = "com.monokulo.till.receipt"
    compileSdk = 37
    compileSdkMinor = 2
    defaultConfig {
        minSdk = 26
    }
    testOptions {
        unitTests.all { test ->
            // Unit tests run on this machine's JVM, against its own build of the library.
            test.systemProperty("jna.library.path", File(rootDir.parentFile, "target/release").absolutePath)
            test.dependsOn(":ffi:cargoHost")
        }
    }
}

dependencies {
    api(project(":ffi"))
    implementation(libs.kotlinx.serialization.json)
    testImplementation(libs.jna)
    testImplementation(libs.kotlin.test.junit)
    testImplementation(libs.junit)
}
