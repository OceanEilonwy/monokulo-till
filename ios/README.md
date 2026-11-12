# Till for iPhone and iPad

The SwiftUI app over the Rust core ([docs/ios-plan.md](../docs/ios-plan.md)).
It needs Xcode 27 (iOS and iPadOS 26.7 is the minimum), Rust with the
`aarch64-apple-ios` and `aarch64-apple-ios-sim` targets, and XcodeGen.

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim   # once
brew install xcodegen                                       # once
ios/Scripts/build-core.sh          # the core as TillFFI.xcframework, and its Swift bindings
cd ios && xcodegen generate        # Till.xcodeproj (not committed)
open Till.xcodeproj
```

`build-core.sh --sim` builds only the simulator's slice, as CI does on pull
requests. Rebuild the core after changing anything under `crates/`.

Every run of `ios.yml` keeps screenshots of the key screens and states, on
the iPhone and the iPad simulators, as the `till-ios-screenshots` artifact:
the run's summary links it; unzip it and open `index.html`. They come from
`TillTests/ScreenshotTests.swift`, which draws the app's own screens from
the engine's previews, and `LayoutSnapshotTests.swift`'s iPad window sizes.
To take them locally, name a folder for them:

```sh
TEST_RUNNER_SNAPSHOT_DIR=/tmp/till-shots xcodebuild test -project Till.xcodeproj -scheme Till \
    -destination 'platform=iOS Simulator,name=iPad (A16)' CODE_SIGNING_ALLOWED=NO
python3 Scripts/screenshot-index.py /tmp/till-shots
```

| Folder | What it is |
| --- | --- |
| `Packages/TillCore` | SwiftPM package: the core's XCFramework, its generated Swift bindings and small Swift helpers |
| `Till` | the app |
| `TillWidgets` | the Live Activity's widget extension, which doesn't link the core |
| `Shared` | the Live Activity's attributes, compiled into the app and the widget |
| `Support` | the app's Info.plist (its URL scheme) and entitlements (Associated Domains) |
| `TillTests` | tests of the Swift side (the behaviour is tested in Rust) |
| `Scripts` | building the core |
