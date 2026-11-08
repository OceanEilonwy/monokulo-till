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

/**
 * `rustls-platform-verifier-android`'s version in Cargo.lock: its Maven
 * component must be exactly that version (docs/decisions/core.md 36), so
 * Gradle takes it from the lock file rather than a second place to update.
 */
abstract class RustlsVerifierVersion : ValueSource<String, RustlsVerifierVersion.Params> {
    interface Params : ValueSourceParameters {
        val lockFile: RegularFileProperty
    }

    override fun obtain(): String {
        val lines = parameters.lockFile.get().asFile.readLines()
        val at = lines.indexOfFirst { it.trim() == "name = \"rustls-platform-verifier-android\"" }
        check(at >= 0) { "rustls-platform-verifier-android isn't in Cargo.lock" }
        return lines.drop(at + 1).first { it.trimStart().startsWith("version = ") }.substringAfter('"').substringBefore('"')
    }
}

val rustlsVerifierVersion = providers.of(RustlsVerifierVersion::class.java) {
    parameters.lockFile.set(File(repo, "Cargo.lock"))
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
    // The Kotlin half of rustls-platform-verifier, which `Tls.init` hands the app's context to.
    implementation("org.rustls:rustls-platform-verifier:${rustlsVerifierVersion.get()}")
    // JNA as an AAR for the app (it carries JNA's own native library); the
    // plain jar for unit tests on the JVM, in each module that runs them.
    api("${libs.jna.get()}@aar")
}
