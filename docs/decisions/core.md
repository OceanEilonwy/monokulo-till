# Decisions: the Rust core

Choices made while moving Till's platform-neutral code into Rust, for
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

## Phase 1: the Monokulo client (2026-11-08)

8. **The event stream is read by `monokulo-client`'s own small SSE
   parser** (`sse.rs`), a `Stream` that ends at its first error; the caller
   reconnects, as `TillRunner` did. `reqwest-eventsource` reconnects by
   itself and pins reqwest 0.12. Sixty seconds without a byte ends the
   stream, standing in for OkHttp's read timeout.
9. **Money is written by the platform, through `MoneyFormat`.** `amounts`
   keeps only ISO 4217 exponents and decimal-to-minor parsing (half to
   even). Six of AmountsTest's nine tests assert the JVM locale's symbols
   and go with the Android side of `format_money`; three are ported.
10. **The Orbot probe stays on the platform, behind `TorProbe`.** The core
    keeps the onion rule and the `socks5h` proxy choice
    (`tor::http_client`).
11. **The contract test reads `MONOKULO_OPENAPI`, and passes unchecked
    without it**, so `cargo test` works offline. `rust.yml` generates the
    document at `monokulo.sha`, sharing the Android job's cache.
12. **`MonokuloError` gives the cases `MonokuloException` told apart by
    status 0 or 403 their own variants**; `status()` keeps the old numbers.
    The `url` crate drops an explicit default port and normalises `..` in
    connect links, where `java.net.URI` kept them.

## Phase 2: the points of sale (2026-11-08)

13. **Refreshes take a `TokenLock`** (docs/ios-plan.md §5): this process's
    mutex, then the lock once a refresh looks due, then the stored sign-in
    read again, refreshed only if still due, saved, and the lock released.
    A new sign-in is saved under it too. The lock is waited for on a
    blocking thread; one acquired after its caller gave up is released at
    once. Android uses `NoTokenLock`.
14. **`PosError` keeps Kotlin's split**: `Answered` is `PosException`,
    whose words the till shows as they are; `Unreachable` and `Unreadable`
    are the other exceptions, shown after "Couldn't reach the POS: ".
15. **Receipt arithmetic is exact, in integers**, with no decimal crate;
    doubles come from parsing the decimal text, as `BigDecimal.toDouble()`
    gave. Square's receipt is an `Option` (an unreadable order time loses
    the receipt, not the customer's name), and a line of quantity 0 falls
    back to the list price where Kotlin divided by zero.
16. **Times are written as Java's `Instant.toString()` wrote them, and
    query components %20-encoded as OkHttp did**, so saved cursors and
    requests stay byte for byte the same. The fixtures are copied into
    `crates/pos/tests/fixtures`, to outlive the Kotlin tests.
17. **Open for phase 5:** `TokenStore::load` returns an `Option`, as the
    Kotlin did, so a Keychain failure reads as signed out. Rule 1's lesson
    from Tern says it should be a `Result`; it changes when the engine's
    store is written. reqwest's `rustls` feature brings `aws-lc-rs`, which
    has yet to be cross-compiled for Android and iOS (`ring` is the
    fallback).

## Phase 4: the Bell's wake-ups (2026-11-08)

18. **A pass is cancelled at its waits, never while the Bell's answer is
    being kept.** Coroutine cancellation and `NonCancellable` become a
    `CancellationToken` checked at a pass's waits; a registration's POST or
    DELETE always finishes.
19. **The platform's push side is traits in `bell`** (`PushSubscriptions`,
    `Distributors`, `PosSide`), with `PushFeed` holding what the callbacks
    mean. An iOS device has one distributor, APNs, so the choosing and
    departure logic (decision 118 of android.md) never runs there; the
    statuses and words that name "the UnifiedPush app" need iOS words in
    phase 5. Its `watch` and `broadcast` receivers can't cross UniFFI, so
    the engine wraps them in callbacks.
20. **A failure on the device is a `DeviceError` result**, retried as
    "something on this device failed" (android.md 120). `WakeUpStore`
    can't fail: the engine keeps the model in memory and saves it itself.
21. **`BellWakeUps`' scenarios run for both Square and Clover on tokio's
    paused clock**, and check every status change through a test-only
    history: stricter than the Kotlin's conflating `StateFlow`.

## The rename to Till (2026-11-08)

22. **The app is Till, "Monokulo Till" in full, and the core is named for
    it.** The crates named so far in this file are now: `monokular-app`
    `till-app`, `monokular-ffi` `till-ffi` (`libtill_ffi`, Kotlin package
    `uniffi.till_app`), `till` `till-sales` (so the till's state machine
    doesn't take the bare product name; Android's module was `sales`),
    `pos` `till-pos` and `bell` `till-bell`. `monokulo-client` keeps its
    name: it is Monokulo's client, not part of Till. The owner chose to
    change the outside names at the same time, though it breaks upgrades of
    test installs: Android's application ID and the iOS bundle ID are
    `com.monokulo.till`, the sign-in and App Link host `till.monokulo.com`,
    the URL schemes `monokulo-till` and `monokulo-till-debug`, the slug Till
    gives Monokulo `till`, and the wake-up service the Monokulo Bell (its
    saved choice's tag `"monokulo"`). The decisions above keep the names
    they were written with.
23. **The screens are their own crate, `till-screens`, as Fob's
    `fob-screens`**, not a module of the engine: pure functions with no
    tasks or I/O, tested alone, and started before the engine exists. Its
    records cross to the apps through its own `uniffi` feature, which
    `till-app` forwards.

## Phase 5a: the first screens (2026-11-08)

24. **A screen takes an input struct, a `Device` (`Android`, `Iphone`,
    `Ipad`) and, where times are shown, a `DeviceClock`** (now, the UTC
    offset, 12- or 24-hour), and returns a view record of finished words.
    Inputs are plain Rust over the leaf crates' types; only the views cross
    UniFFI. `Device`, not `Platform`, because `till_app::Platform` exists;
    no type is named like a SwiftUI view (`Button`, `Section`), so the
    Swift bindings can share a module with the app's code. Times are
    written as Android wrote US time, with no full locale: the same choice
    as money (9).
25. **On iOS, "no distributor" means alerts are off.** APNs is the only
    distributor and the Bell's wake-ups arrive as alerts, so the Android
    statuses about UnifiedPush apps become "Alerts are off" with an action
    that opens iOS's alert settings. Settings on iOS has no bubble, shift
    switch or Tor, shows Wake-ups as a row, and has a Dynamic Island
    section on iPhone only. `till-bell`'s `PUSH_*` reasons still name
    UnifiedPush and need iOS words. The iOS words are new and await the
    owner's review.
26. **What a screen moved from Compose, and what stayed**: the parts state
    chooses (buttons, store facts, the point-of-sale card and steps, scan
    errors, the Tor, alerts and bubble footnotes, check rows, step numbers)
    are the screen's; headings, leads written once, and the wallet,
    Disconnect and Dynamic Island footnotes stay in each app's screen.
27. **`StoreNow`, `PosState`, `PosSetup` and `CloverTender` are defined in
    `till-screens` for now**, and may merge with `till-sales`' `LiveStore`
    once phase 3 lands. `PosText`'s words that read a sale (`of`,
    `elsewhere`, `placeOf`) wait for phase 3. The engine's `Update` carries
    `Onboarding`, `Settings` and `WakeUps` views, not yet sent: phase 5b
    fills the inputs and publishes each only when it changed.

## Phase 3: the till (2026-11-08)

28. **The saved shift stays the Android app's JSON, byte for byte**
    (`shift_json`): hand-written serde with Kotlin's defaults left out,
    fields in the Kotlin classes' order, Java's doubles written through
    `raw_value` (JDK 19's shortest digits; Android's runtime may differ in
    rare cases), and a receipt mirror matching Kotlin's `Receipt`. One
    difference: a receipt's `extensions` with both a tip and a discount
    writes the discount first, as serde_json sorts keys.
29. **The till's state is a `watch` channel**, changed through
    `send_if_modified` and saved afterwards under a mutex. Kotlin's
    `collectLatest` loops become a `select!` between the work and the next
    change that matters. The shift's conflated wake-up channel is one slot
    and a `Notify`, with a biased `select!` so a wake-up landing as a tick
    ends is never lost.
30. **`TillRunner` takes a `RunnerSide` trait in place of `Graph`**: the
    engine chooses the source (point of sale, signed in, location), hands
    in dings with `ding()` (ignored outside a shift), and implements
    `ShiftStore` and `PosSwitchSide`. The runner's `close` and `reopen`
    run at once rather than as launched tasks.
31. **`ReceiptCode` lives in `till-sales` as `PaymentCode::of`**, with its
    tests; `OrderStatus` stands in for `onStatus`'s default arguments.
32. **Paused-clock tests that make real HTTP requests use a client with no
    idle pool** (`pool_max_idle_per_host(0)`), or the pool's idle timer
    moves virtual time while a request is in flight.
33. **Follow-ups the port worked around locally**, to move into the crates
    they belong to: `FoundSale` should leave out `pointOfSale` for Square
    as Kotlin does (`till-sales` has its own serializer for now);
    "was this Monokulo answering?" belongs on `MonokuloError`
    (`till-sales` has `monokulo_answered`); `till-pos`'s RFC 3339 parsing
    is private, so `till-sales` uses jiff directly; `NoSource`'s refusal
    has to name a point of sale (Square) because `PosError::Answered`
    needs one. Failures' words after "Couldn't reach the store: " are
    reqwest's, not OkHttp's.
