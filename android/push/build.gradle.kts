plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "com.monokulo.till.push"
    compileSdk = 37
    compileSdkMinor = 2
    defaultConfig {
        minSdk = 26
    }
}

dependencies {
    api(project(":core"))
    implementation(libs.unifiedpush.connector)
    implementation(libs.kotlinx.coroutines.core)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.okhttp)
    testImplementation(libs.kotlin.test.junit)
    testImplementation(libs.junit)
    testImplementation(libs.okhttp.mockwebserver)
    testImplementation(libs.kotlinx.coroutines.test)
}
