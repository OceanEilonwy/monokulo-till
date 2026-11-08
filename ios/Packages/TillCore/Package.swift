// swift-tools-version: 6.2
// The Rust core for the iOS app: the XCFramework and the Swift bindings
// that ios/Scripts/build-core.sh writes, and the few Swift helpers that sit
// on them. Run that script before building (docs/ios-plan.md §5).

import PackageDescription

let package = Package(
    name: "TillCore",
    platforms: [.iOS("27.0")],
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
                // What the core's networking needs from the system: rustls'
                // platform verifier checks certificates with Security, and
                // reqwest reads proxy settings through SystemConfiguration.
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("CoreFoundation"),
            ]
        ),
    ]
)
