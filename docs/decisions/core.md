# Decisions: the Rust core

Choices made while moving Monokular's platform-neutral code into Rust, for
the Android app and the iOS app ([ios-plan.md](../ios-plan.md)). Numbered
on from 1; the Android app's own choices are in [android.md](android.md).

## Phase 0: the scaffold (2026-11-08)

1. **All behaviour is in Rust, behind one engine, as in Tern.** The owner
   accepted the plan's recommendations: one `Engine` object whose single
   task owns the state, taking `Action`s and publishing `Update`s and
   `Effect`s through an `EngineListener`; copy chosen by logic is the
   core's too. This supersedes [design.md §9](../design.md#9-the-monokular-repository)'s
   "the rest of the core stays native". Fob's alternative (a wide object API
   and an FFI crate that mirrors every type) would cost thousands of lines
   of conversions here.
2. **Android stays on its Kotlin modules until the engine can drive it.**
   The plan had each phase switch one Android module to the Rust crate.
   That doesn't work for this code: the Kotlin modules call one another
   directly (`sales`' `Till` takes `core`'s `MonokuloApi`, `Order`,
   `SaleEvent` and `receipt`'s `ReceiptCode`), so switching one module
   alone would mean exporting each leaf crate's types to Kotlin through
   UniFFI, which rule 5 rules out, and writing Kotlin glue that phase 5
   throws away. So phases 1 to 4 port each module's tests and code into its
   crate and leave Android as it is; phase 5 moves Android onto the engine
   and deletes the Kotlin modules with their tests. Parity holds because
   every Kotlin unit test is ported before the code it tests, and the
   on-device tests stay as the end-to-end check for phase 5.
3. **One library, `monokular-ffi`.** It is a cdylib (Android) and a
   staticlib (iOS) holding the engine and `orf-codec-ffi`, re-exporting
   both crates' UniFFI scaffolding. `orf-codec-ffi` is a plain Rust library
   now, and the `uniffi-bindgen` binary moved to `monokular-ffi`. Bindings
   are generated per crate, so the Kotlin packages stay `uniffi.orf_codec_ffi`
   (no Kotlin import changed) beside the new `uniffi.monokular_app`, and
   both load `monokular_ffi`. The `receipt` module's Gradle tasks build it
   until phase 5, when they move to a `:ffi` module with the engine's
   Kotlin side; the plan put that move in phase 0, but `receipt` is still
   the library's only consumer.
4. **The types the crates share live in `monokulo-client`**: `PointOfSale`
   (and its wire names) and `Clock`. `Clock::now()` is Unix seconds, like
   the Kotlin `now: () -> Long` lambdas it replaces; `FixedClock` is the
   tests' clock. Receipts are `orf_codec::Receipt`, so the Kotlin
   `receipt.Receipt` model is not ported.
5. **The leaf crates never depend on UniFFI.** `monokulo-client`, `pos`,
   `till` and `bell` are plain Rust; only `monokular-app` (behind its
   `uniffi` feature) and the two FFI crates use it. `rust.yml` checks this
   with `cargo tree`, and tests `monokular-app` without the feature too.
   Shared dependency versions are in the workspace's
   `[workspace.dependencies]`.
6. **The workspace is formatted with rustfmt**, and CI checks it. The
   existing crates were formatted in one commit of their own.
7. **iOS and iPadOS 27 is the minimum** (the owner's decision: a
   cutting-edge product), with no code for older versions
   ([ios-plan.md §5](../ios-plan.md#5-the-ios-app)).
