#!/usr/bin/env bash
# Builds the Rust core (till-ffi) for iOS and lays it out as the TillCore
# Swift package expects (docs/ios-plan.md §5):
#
#   ios/Packages/TillCore/TillFFI.xcframework           the static library, per platform
#   ios/Packages/TillCore/Sources/TillCore/Generated/   the Swift bindings, one file per crate
#
# Neither is committed: like Android (docs/decisions/android.md 8), the
# bindings are generated from the crates on every build, so they can't drift.
#
#   build-core.sh          device and simulator (release builds)
#   build-core.sh --sim    the simulator only (CI on pull requests)
#
# Needs macOS with Xcode, and Rust with the aarch64-apple-ios and
# aarch64-apple-ios-sim targets.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
package="$repo/ios/Packages/TillCore"
generated="$package/Sources/TillCore/Generated"
work="$repo/target/ios"

targets=(aarch64-apple-ios-sim)
if [[ "${1:-}" != "--sim" ]]; then
    targets+=(aarch64-apple-ios)
fi

# The library is built for the app's minimum version (ios-plan §5).
export IPHONEOS_DEPLOYMENT_TARGET=27.0

cd "$repo"
for target in "${targets[@]}"; do
    cargo build --locked --release -p till-ffi --target "$target"
done

# The bindings come from a build for this Mac: UniFFI reads its metadata from
# the library's symbols, which every build of the same source shares.
cargo build --locked --release -p till-ffi
rm -rf "$work" "$generated"
mkdir -p "$work/bindings" "$work/headers" "$generated"
cargo run --locked --release -q -p till-ffi --features bindgen --bin uniffi-bindgen -- \
    generate --library "$repo/target/release/libtill_ffi.dylib" --language swift --out-dir "$work/bindings"

# One Swift file per crate (till_app, till_screens, orf_codec_ffi) into the
# package; their C headers and module maps into the XCFramework, one module
# map holding every crate's module (till_appFFI, …), which the Swift files
# import by those names.
cp "$work"/bindings/*.swift "$generated/"
cp "$work"/bindings/*.h "$work/headers/"
cat "$work"/bindings/*.modulemap > "$work/headers/module.modulemap"

libraries=()
for target in "${targets[@]}"; do
    libraries+=(-library "$repo/target/$target/release/libtill_ffi.a" -headers "$work/headers")
done
rm -rf "$package/TillFFI.xcframework"
xcodebuild -create-xcframework "${libraries[@]}" -output "$package/TillFFI.xcframework"

echo "TillFFI.xcframework and the Swift bindings are in $package"
