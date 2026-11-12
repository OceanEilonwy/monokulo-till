plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// The web host of the app's App Links, where the points of sale's sign-ins
// come back (/square, /clover, and their sandboxes' /debug/ pages): the
// deployment's, from -Ptill.link.host (or ~/.gradle/gradle.properties),
// never in the source; a host that never resolves otherwise.
val linkHost: String = providers.gradleProperty("till.link.host").getOrElse("till.invalid")
// The ABIs the Rust core is built for (ffi's default): phones, old 32-bit
// phones, and the emulator.
val abis = listOf("arm64-v8a", "armeabi-v7a", "x86_64")
// A release (-Ptill.splits, from CI's release workflow) comes as an APK for
// each ABI, smaller, and a universal one holding all three. Any other build is
// one APK holding them all.
val splitting = providers.gradleProperty("till.splits").isPresent

android {
    namespace = "com.monokulo.till"
    compileSdk = 37
    compileSdkMinor = 2

    defaultConfig {
        applicationId = "com.monokulo.till"
        minSdk = 26
        targetSdk = 37
        // The version a v* tag must name. CI's release workflow passes the
        // version code it computes from it (.github/workflows/release-android.yml).
        versionCode = providers.gradleProperty("till.versionCode").map { it.toInt() }.getOrElse(1)
        versionName = "0.1.0"
        manifestPlaceholders["tillLinkHost"] = linkHost
        // The project's Square application (docs/design.md §1). Public: the
        // till signs in with PKCE, so no secret is ever built in. Replace the
        // placeholder once the Square application exists.
        buildConfigField("String", "SQUARE_APPLICATION_ID", "\"${providers.gradleProperty("till.square.applicationId").getOrElse("sq0idp-REPLACE-ME")}\"")
        buildConfigField("boolean", "SQUARE_SANDBOX", providers.gradleProperty("till.square.sandbox").getOrElse("false"))
        // Where Square sends the sign-in back, and where it comes back into
        // the app: the same App Link in a release build.
        buildConfigField("String", "SQUARE_REDIRECT_URI", "\"https://$linkHost/square\"")
        buildConfigField("String", "SQUARE_RETURN_URI", "\"https://$linkHost/square\"")
        buildConfigField("String", "SQUARE_BASE_URL", "\"\"")
        // The project's Clover application and the region it is in (sandbox,
        // north_america, europe or latin_america). Public too: Clover's
        // low-trust sign-in is PKCE with no secret (decision 127). Replace the
        // placeholder once the Clover application exists.
        buildConfigField("String", "CLOVER_APPLICATION_ID", "\"${providers.gradleProperty("till.clover.applicationId").getOrElse("REPLACE-ME-CLOVER")}\"")
        buildConfigField("String", "CLOVER_ENVIRONMENT", "\"${providers.gradleProperty("till.clover.environment").getOrElse("north_america")}\"")
        // Clover sends the sign-in to a path under the application's Site URL, https only: the App Link.
        buildConfigField("String", "CLOVER_REDIRECT_URI", "\"https://$linkHost/clover\"")
        buildConfigField("String", "CLOVER_RETURN_URI", "\"https://$linkHost/clover\"")
        buildConfigField("String", "CLOVER_BASE_URL", "\"\"")
        // The project's Monokulo Bell (docs/design.md §3), which Settings ›
        // Wake-ups offers by name: the deployment's, from -Ptill.bell.url (or
        // ~/.gradle/gradle.properties), never in the source; an address that
        // never answers otherwise.
        buildConfigField("String", "BELL_URL", "\"${providers.gradleProperty("till.bell.url").getOrElse("https://bell.invalid")}\"")
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    // Only the ABIs till-ffi is built for, in the universal APK too: with the
    // other ABIs' libraries the dependencies carry (JNA's), a 32-bit x86 device
    // would install it and crash at the first call into Rust, not be refused
    // at install.
    packaging {
        jniLibs {
            excludes += listOf("lib/armeabi/**", "lib/x86/**", "lib/mips/**", "lib/mips64/**")
        }
    }

    // One versionCode for all of them: an APK of another ABI, or the
    // universal one, installs over any other.
    splits {
        abi {
            isEnable = splitting
            reset()
            include(*abis.toTypedArray())
            isUniversalApk = true
        }
    }

    // Only a build given the keystore (CI's release secrets) is signed; any
    // other release build stays unsigned.
    signingConfigs {
        System.getenv("TILL_KEYSTORE")?.let { keystore ->
            create("release") {
                storeFile = file(keystore)
                storePassword = System.getenv("TILL_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("TILL_KEY_ALIAS")
                keyPassword = System.getenv("TILL_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
            // A debug build can talk to a Monokulo and a stand-in Square on
            // the developer's machine (docs/decisions/android.md).
            // -Ptill.square.baseUrl=http://10.0.2.2:8738 for tools/fake-square.
            val squareBase = providers.gradleProperty("till.square.baseUrl").getOrElse("")
            buildConfigField("String", "SQUARE_BASE_URL", "\"$squareBase\"")
            // The sign-in comes back on the debug scheme. Square only sends
            // it to https, so it goes through site/square/debug/, which opens
            // the scheme; the stand-in Square sends it to the scheme itself.
            // -Ptill.square.redirectUri=… names another such page.
            val squareRedirect = providers.gradleProperty("till.square.redirectUri")
                .getOrElse(if (squareBase.isEmpty()) "https://$linkHost/square/debug/" else "monokulo-till-debug://square")
            buildConfigField("String", "SQUARE_REDIRECT_URI", "\"$squareRedirect\"")
            buildConfigField("String", "SQUARE_RETURN_URI", "\"monokulo-till-debug://square\"")
            // The same for Clover: -Ptill.clover.baseUrl=http://10.0.2.2:8740 for
            // tools/fake-clover, which plays both Clover's web and API hosts; else
            // site/clover/debug/ opens the scheme.
            val cloverBase = providers.gradleProperty("till.clover.baseUrl").getOrElse("")
            buildConfigField("String", "CLOVER_BASE_URL", "\"$cloverBase\"")
            val cloverRedirect = providers.gradleProperty("till.clover.redirectUri")
                .getOrElse(if (cloverBase.isEmpty()) "https://$linkHost/clover/debug/" else "monokulo-till-debug://clover")
            buildConfigField("String", "CLOVER_REDIRECT_URI", "\"$cloverRedirect\"")
            buildConfigField("String", "CLOVER_RETURN_URI", "\"monokulo-till-debug://clover\"")
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release")
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }
}

dependencies {
    // The Rust core and its bindings (docs/ios-plan.md §2).
    implementation(project(":ffi"))
    implementation(libs.unifiedpush.connector)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.browser)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.lifecycle.service)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.animation)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.camerax.camera2)
    implementation(libs.camerax.lifecycle)
    implementation(libs.camerax.view)
    implementation(libs.zxing.core)
    debugImplementation(libs.compose.ui.tooling)
    // Compose UI tests on the emulator (decision 152): the test activity comes with the manifest artifact.
    debugImplementation(libs.compose.ui.test.manifest)
    testImplementation(libs.kotlin.test.junit)
    testImplementation(libs.junit)
    // The on-device tests write their sales as a saved shift for the engine's previews.
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.kotlin.test.junit)
    androidTestImplementation(libs.compose.ui.test.junit4)
    // The runner and Espresso ui-test-junit4 brings reflect on InputManager.getInstance, gone in Android 16: these run there.
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.espresso.core)
}
