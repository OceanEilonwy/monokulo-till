plugins {
    alias(libs.plugins.android.library)
}

// The Rust core, `till-ffi`: the one library the app loads, carrying the
// engine and orf-codec (docs/decisions/core.md 3), and its UniFFI bindings
// (`uniffi.till_app`, `uniffi.till_screens`, `uniffi.orf_codec_ffi`). Both
// the native libraries and the Kotlin bindings are built here from the
// crates, so they can't drift from them: building needs Rust, cargo-ndk and
// the NDK. The other modules take it as an `api` dependency.
val repo: File = rootDir.parentFile
val hostLibrary = File(repo, "target/release/" + System.mapLibraryName("till_ffi"))
val abis = listOf("arm64-v8a", "armeabi-v7a", "x86_64")

/** A cargo command that writes one directory, so AGP can take it as generated sources. */
abstract class CargoTask : Exec() {
    @get:OutputDirectory
    abstract val outDir: DirectoryProperty
}

android {
    namespace = "com.monokulo.till.ffi"
    compileSdk = 37
    compileSdkMinor = 2
    ndkVersion = "27.0.12077973"
    defaultConfig {
        minSdk = 26
    }
    testOptions {
        unitTests.all { test ->
            // Unit tests run on this machine's JVM, against its own build of
            // the library.
            test.systemProperty("jna.library.path", hostLibrary.parentFile.absolutePath)
            test.dependsOn("cargoHost")
        }
    }
}

val cargoHost = tasks.register<Exec>("cargoHost") {
    description = "Builds till-ffi for this machine: for the bindings and the unit tests."
    workingDir = repo
    commandLine("cargo", "build", "--release", "--locked", "-p", "till-ffi")
    inputs.dir(File(repo, "crates"))
    inputs.files(File(repo, "Cargo.lock"), File(repo, "Cargo.toml"))
    outputs.file(hostLibrary)
}

val uniffiBindings = tasks.register<CargoTask>("uniffiBindings") {
    description = "Generates till-ffi's Kotlin bindings (uniffi.till_app, uniffi.till_screens, uniffi.orf_codec_ffi)."
    dependsOn(cargoHost)
    workingDir = repo
    outDir.set(layout.buildDirectory.dir("rust/kotlin"))
    inputs.file(hostLibrary)
    argumentProviders.add(CommandLineArgumentProvider { listOf(outDir.get().asFile.absolutePath) })
    commandLine(
        "cargo", "run", "--release", "--locked", "-q", "-p", "till-ffi", "--features", "bindgen",
        "--bin", "uniffi-bindgen", "--", "generate", "--library", hostLibrary.absolutePath,
        "--language", "kotlin", "--no-format", "--out-dir",
    )
}

val cargoNdk = tasks.register<CargoTask>("cargoNdk") {
    description = "Builds till-ffi for each Android ABI."
    workingDir = repo
    outDir.set(layout.buildDirectory.dir("rust/jniLibs"))
    inputs.dir(File(repo, "crates"))
    inputs.files(File(repo, "Cargo.lock"), File(repo, "Cargo.toml"))
    // This module's NDK, whatever the environment says.
    val ndk = androidComponents.sdkComponents.ndkDirectory.map { it.asFile.absolutePath }
    doFirst {
        environment("ANDROID_NDK_HOME", ndk.get())
        environment("NDK_HOME", ndk.get())
    }
    commandLine(listOf("cargo", "ndk") + abis.flatMap { listOf("-t", it) } + listOf("--platform", "26", "-o"))
    argumentProviders.add(
        CommandLineArgumentProvider {
            listOf(outDir.get().asFile.absolutePath, "build", "--profile", "android", "--locked", "-p", "till-ffi")
        },
    )
}

androidComponents {
    onVariants { variant ->
        variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoNdk, CargoTask::outDir)
        variant.sources.kotlin?.addGeneratedSourceDirectory(uniffiBindings, CargoTask::outDir)
    }
}

dependencies {
    // JNA as an AAR for the app (it carries JNA's own native library); the
    // plain jar for unit tests on the JVM, in each module that runs them.
    api("${libs.jna.get()}@aar")
}
