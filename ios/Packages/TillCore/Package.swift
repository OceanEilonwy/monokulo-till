// swift-tools-version: 6.2
// The Rust core for the iOS app: the XCFramework and the Swift bindings
// that ios/Scripts/build-core.sh writes, and the few Swift helpers that sit
// on them. Run that script before building (docs/ios-plan.md §5).

import PackageDescription

let package = Package(
    name: "TillCore",
    platforms: [.iOS("26.7")],
    products: [
        .library(name: "TillCore", targets: ["TillCore"]),
    ],
    targets: [
        .binaryTarget(name: "TillFFI", path: "TillFFI.xcframework"),
        .target(
            name: "TillCore",
            dependencies: ["TillFFI"],
            path: "Sources/TillCore",
            linkerSettings: [
                // What the core needs from the system: rustls'
                // platform verifier checks certificates with Security, and
                // reqwest reads proxy settings through SystemConfiguration.
                // SQLCipher encrypts the ledger with CommonCrypto, which is
                // in libSystem, and asks Security and CoreFoundation for
                // random bytes (docs/decisions/core.md 150). The wallet
                // scans on the GPU through Metal, whose errors come as
                // Foundation's (core.md decision 243).
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("Foundation"),
                .linkedFramework("Metal"),
            ]
        ),
    ]
)
