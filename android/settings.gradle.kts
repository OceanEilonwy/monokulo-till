pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        // rustls-platform-verifier's Kotlin component, published beside the crate (docs/decisions/core.md 36).
        maven("https://raw.githubusercontent.com/rustls/rustls-platform-verifier/maven-archive/android-release-support/maven/") {
            content { includeGroup("org.rustls") }
        }
    }
}

rootProject.name = "till"

// core: pure Kotlin (JVM), no Android: model, connect links, protocol.
// sales: finding Monero sales in the POS (Square), and the till's sales.
// ffi: the Rust core (till-ffi) and its Kotlin bindings.
// receipt: ORF receipts in the payment code (orf-codec, from Rust).
// push: wake-ups from a Monokulo Bell over UnifiedPush (docs/design.md §3, §5).
// app: the Compose app.
include(":ffi", ":core", ":receipt", ":sales", ":push", ":app")
