plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.serialization)
}

val monokuloOpenApi: String = providers.gradleProperty("monokulo.openapi").orNull
    ?: File(rootDir, "core/build/monokulo/openapi-${providers.gradleProperty("monokulo.sha").get()}.json").absolutePath

android {
    namespace = "com.monokulo.till.sales"
    compileSdk = 37
    compileSdkMinor = 2
    defaultConfig {
        minSdk = 26
    }
    testOptions {
        // The Rust core's library for this machine (see :ffi).
        unitTests.all { test ->
            test.systemProperty(
                "jna.library.path",
                File(rootDir.parentFile, "target/release").absolutePath,
            )
            test.dependsOn(":ffi:cargoHost")
            // Monokulo's OpenAPI document at monokulo.sha, as :core generates
            // it, for the problems TillTest answers with.
            test.dependsOn(":core:monokuloOpenApi")
            test.systemProperty("monokulo.openapi", monokuloOpenApi)
        }
    }
}

dependencies {
    api(project(":core"))
    api(project(":receipt"))
    implementation(libs.okhttp)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.kotlinx.coroutines.core)
    testImplementation(libs.jna)
    testImplementation(libs.kotlin.test.junit)
    testImplementation(libs.junit)
    testImplementation(libs.okhttp.mockwebserver)
    testImplementation(libs.kotlinx.coroutines.test)
}
