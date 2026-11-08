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
