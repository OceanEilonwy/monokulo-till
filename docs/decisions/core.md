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

## Tidying after phase 3 (2026-11-08)

34. **Decision 33's workarounds live in their crates now.**
    `MonokuloError::answered()` says whether Monokulo was answering (an
    exhaustive match, so a new variant must be classified); `FoundSale`
    leaves out a Square `pointOfSale`, as Kotlin did; `till_pos::time`
    gives `unix_seconds` and `instant`, written as Java's `Instant` wrote;
    and a refusal made before asking any point of sale is
    `PosError::NotAsked`, which names none, has status 0 and is shown as it
    is (`NoSource`'s "Sign in to Square first").
35. **The leaf crates take their shared dependencies from the workspace,
    one version each** (`async-trait`, `base64`, `futures-util`,
    `reqwest`, `sha2`, `wiremock`); reqwest's features stay per crate.
36. **aws-lc-rs cross-compiles for Android with NDK 27.** TLS adds about
    3.7 MB to arm64's library (1.7 MB to 5.4 MB, with a real HTTPS request
    linked in); `ring` stays the fallback if size matters. reqwest's
    `rustls` feature verifies through `rustls-platform-verifier`, which on
    Android must be initialised through JNI, with its
    `rustls-platform-verifier-android` Maven component, before the first
    request, or that request panics: phase 5b's Android switch does it. On
    iOS it uses the Security framework and needs nothing.

## Phase 5a: the till's screens (2026-11-08)

37. **A countdown is words at the clock's now, redrawn by the engine.** A
    view whose words count down says `ticks`, and the engine publishes it
    again each second while it does; the platform never writes a time
    left.
38. **Logic that only Android draws, but that is pure, moved too**: the
    bubble's look, focus, tap and label, `PaidBuzz`, the panel's ended
    words, `TillNav` (as `TillPlace`) and `PhaseChanges` are in
    `till-screens`, so the engine holds them. The loading mark's animation
    keyframes stay with each platform's drawing (SaleTextTest's
    `glintAlpha`/`coreAlpha`).
39. **A receipt's numbers are exact on screen too**: the `BigDecimal`s that
    wrote quantities, amounts, rates and subtotals become a small integer
    decimal that reads a double by its shortest digits, as
    `BigDecimal.valueOf` did; the order's summary adds in minor units.
40. **The Live Activity's state comes from the till's own sales only**: the
    sale just paid, else the newest live sale, else the sale just ended
    (as expired), else idle. `waiting_count` counts own live sales whose
    code counts down. `payment_uri` is the code without its `orf`
    parameter, sent only while waiting (never part-paid) with "Show code in
    island" on. iPhone only; `None` on iPad and Android.
41. **iOS's ongoing face is the Live Activity**, so `notification` gives iOS
    only the new-sale alert, and Android its ongoing notification. The
    alert's words differ on iPad ("Swipe in Till, or tap here, to show the
    payment code.", from PadTucked) and iPhone ("Tap here to show the
    payment code."); design.md §3's "Monero sale · $24.50 · Tap to show
    code" is the alternative. The iOS words await the owner's review.

## Phase 5b: the engine's foundation (2026-11-08)

42. **The saved state is a model in memory, written through key by key**,
    in `Stores.kt`'s keys, each value the UTF-8 of the string the Kotlin
    wrote, so an upgraded Android till keeps everything. A fixture holds
    every key and reads back byte for byte. Sign-ins, cursors, tender ids
    and the shift are read through each time, because the iOS extension
    changes them; a refused write leaves the model as it was, and a shift
    that can't be read is never saved over.
43. **`TokenLock` is an `flock` on `<data_dir>/sign-in.lock`**, Rust's own on
    both platforms, so there is no platform trait for it; on Android it
    never waits.
44. **The push side is a foreign `PushPlatform`, called one way and
    answered with `Push…` actions** that feed `till-bell`'s `PushFeed`; a
    ding also wakes the runner. iOS's implementation always finds one
    distributor, APNs.
45. **`background_check` is a free function, not an `Engine` method**: it
    builds a current-thread runtime, never runs with no shift on (android.md
    93), takes its deadline in milliseconds from now, keeps the orders made
    before the deadline (the idempotency key gives the next look the same
    order), and returns an alert face per new live sale.
46. **The engine adds `Update::Screen`, `Work` and `Service`**, publishes
    Onboarding only while setup is on screen, and alerts only sales that
    weren't already kept, so a restart doesn't alert again.
47. **Still to do in the stores**: `TokenStore::load`, `ShiftStore`,
    `CursorStore` and `TenderStore` can't report a failure (17), so the
    engine logs and never saves over; Settings' `live_activity` and
    `code_in_island` need new keys; the alert words are stand-ins until the
    engine calls `till-screens`' `notification`; Tor's Orbot probe isn't
    wired (connecting sets `tor = false`).

## iOS phases I0 and I1 (2026-11-08)

48. **CI builds the iOS app on GitHub's `xcode-27` image** (macOS 27,
    arm64, Xcode 27.2 on the first run); `macos-26`'s newest Xcode is 26.6.
    The tests run on the newest iPhone and iPad simulators with an iOS 27
    runtime.
49. **Every crate's Swift bindings compile into one module, `TillCore`**, so
    no two crates may export a type of the same name; `rust.yml` checks it
    on Linux (`till-app`'s `Check` became `BackgroundCheck`, and its
    `CloseChoice` is `till-screens`' own). The static library is linked
    once, into the app, and reaches the tests through the app's
    `@_exported import TillCore`.
50. **The iOS look ports Android's**: the colours of ios-plan §4, dark mode
    from Android's dark tokens, Manrope 800 loaded from the bundled font
    (its licence ships with it) without `UIAppFonts`, the mark and loading
    mark from `Mark.kt`'s path data and its 17 glint delays, and the QR at
    correction level M in ink `#1A1917` on white, cropped to its modules.
51. **The glass button keeps its 44 pt minimum outside the button**, so it
    doesn't widen a large button's own tap area.

## Phase 5b: Android's glue, and the till in the engine (2026-11-08)

52. **The Rust core is Android's `:ffi` module**: it builds `till-ffi` and
    its bindings and exposes JNA as `api`, finishing decision 3's move out
    of `receipt`.
53. **The certificate verifier starts through a JNI entry in `till-ffi`**:
    `Tls.init(context)` from `Application.onCreate` calls
    `rustls_platform_verifier::android::init_with_env` (Android only), and
    the Kotlin component comes from rustls' Maven archive at the version
    Cargo.lock names. An on-device test made a real HTTPS request through
    it (on an API 37 emulator).
54. **Orbot's probe reaches the engine as `Action::TorProbed`**, with no new
    `Effect`, so iOS needs nothing. A till connects over Tor only after a
    probe found Orbot running, replacing 47's "connecting sets tor =
    false".
55. **The engine owns the till's navigation**: `TillNav`'s follow (a new sale
    rises only out of a shift at rest, which the platform reports with
    `Action::Landed`), the paid sale's step back after 5 s, `backTarget`,
    and Back from Settings. `Update::Nav` comes before each `Screen`; the
    shift's filter and fold and the close sheet are engine state.
56. **A countdown is redrawn by a one-second engine timer**, only while some
    published view says `ticks` and the app is in front.
57. **Android's Keystore platform keeps `SecureStore`'s file, key alias and
    sealing, value for value**, so an upgraded till keeps its data; a
    secret that can't be opened is `Unavailable`, never none. Still open:
    in debug builds sign-ins return on the debug scheme while the engine
    matches `redirect_uri` (either the setups gain a return URI or Kotlin
    rewrites it), and the per-sale alert faces are still stand-ins.

## iOS phase I2: onboarding (2026-11-08)

58. **"Open in Till" links are `https://till.monokulo.com/connect#<the
    connect link, percent-encoded>`**: in the fragment, so the token never
    reaches a server. Monokulo's dashboard has to write them, and the site
    serves `/.well-known/apple-app-site-association` (team ID still a
    `TEAMID` placeholder) but no `/connect` page yet.
59. **iOS rewrites a sign-in returned on `monokulo-till(-debug)://` to the
    setup's `redirect_uri` before sending `SignInReturned`**, and the
    sign-in session ends at the `redirect_uri` the sign-in page names.
60. **Two choices Swift makes from the view, for want of engine actions**:
    "Next" is `SignIn` until the point-of-sale card says connected, then
    `Next`; "Skip" is `WithoutPos` until then, then `Next`. A paste or a
    scan always sends `ConnectLink`, so the engine can't yet tell
    "nothing to paste" from "not a code": both want their own actions.
61. **Asking for alerts asks `[.alert, .sound, .badge]` only**; a
    time-sensitive alert comes from each alert's interruption level and its
    entitlement (I3, I5). A refused request opens notification Settings,
    and the permission is read again whenever the app comes to the front.
    The Associated Domains entitlement has `webcredentials` beside
    `applinks`. New iOS words ("Dashboard open on this iPhone?", a shorter
    alerts lead) await the owner's review; the Square and Clover tiles are
    stand-ins, not their brands' art.

## Phase 5b: the engine's gaps closed (2026-11-08)

62. **Stores report a failed read as a `StoreError`, and nothing is saved
    over what couldn't be read** (`TokenStore::load`, `CursorStore`,
    `TenderStore::tender_id` and `ShiftStore::load` return `Result`; the
    shift's guard is `Till::shift_unread`). An unreadable sign-in counts as
    signed in, so its error shows rather than "Sign in again". This closes
    17's and 47's store items.
63. **The setups take an optional `return_uri`** (UniFFI default none),
    accepted beside `redirect_uri`, while the token exchange names the
    redirect: Android's debug scheme can use it; iOS rewrites in Swift
    (59). This closes 57's first item.
64. **A new sale is alerted only while the app is in the back, in a single
    notification update**: Android's ongoing face carries `alert` once,
    iOS's view carries one `SaleAlert`. Android's ended-sale notices come
    as `Effect::Notify` with `till-screens`' `Notice`, and
    `background_check` answers with `SaleAlert`s on iOS and an
    `OngoingFace` on Android. `NotificationFace` is gone. This closes 57's
    second item.
65. **The Live Activity is published on iPhone only, when it changes, and
    only while its switch is on, setup is done and a shift is on**; an own
    sale that ended shows as expired for 5 s, as a paid one does (the
    length is a choice; design.md §6 doesn't give one).
66. **The island switches are kept under `island.activity` (absent means
    on; read only on iPhone) and `island.code` (off unless set)**, and
    survive `forget_all`.

## iOS phase I3: the till on iPad (2026-11-08)

67. **iOS reports `Landed` 0.3 s after each move between the till's
    screens settles** (a later move cancels it); the length is a choice. A
    sale shows its live screen while its code isn't done and its page
    otherwise, as on Android, and the enlarged code gives way to the page
    once it is.
68. **Wake-ups is a page pushed inside Settings in Swift**, since `Screen`
    has no case for it; Back from it is the system's. The engine never sets
    the own-Bell field's `own_picked` or `typed`, so that field is Swift's
    state until "Use this Bell"; the engine wants actions for picking and
    typing it.
69. **iOS can't switch its own alerts on**, so Settings' alerts switch
    opens Till's notification settings and springs back. A new-sale alert
    is a time-sensitive `UNNotificationRequest` with the order id as its
    identifier (`ClearNotification` removes it; tapping it sends
    `OpenSale`); the time-sensitive entitlement needs the capability on
    the App ID before release.
70. **New iOS words await the owner's review**: "This shift", the swipe's
    "Close", "Disconnect this device?", "Open alert settings", "Scan with
    any Monero wallet", "Tap anywhere to shrink" and the section titles.

## The engine's follow-ups for the bubble and onboarding (2026-11-08)

71. **The bubble's panel has its own record, `PanelView`** (built by
    `till_screens::bubble::panel`): confirmations and how many are needed,
    the sale's time, the time left apart from the hint, a `PanelHero`
    (code, paid, refunded, reversed, expired), and the whole order with its
    count words, so Android no longer works them out from words.
    PanelTest's words, landing and pulse cases moved to `bubble.rs`; its
    geometry and motion stay Kotlin.
72. **The one-second timer (56) also runs while Android's panel is up over
    other apps**, as `Action::PanelShown` reports; the bubble is down while
    the till is disconnected (android.md 208).
73. **The bubble's place is the engine's to keep**, under `Stores.kt`'s
    `bubble.left` and `bubble.y`, set by `Action::MoveBubble` when a drag
    lets go: the engine owns and caches that store, so a second writer
    would leave it stale.
74. **A copy says what it copies** (`Effect::CopyText`'s `CopyKind`:
    address, amount or other), and Android's ongoing sale notification
    carries the code and address.
75. **Onboarding's Next and Skip are the engine's to decide**, from the
    till's state, replacing 60: on step 4 Next signs in until a point of
    sale is connected and then goes on, and Skip is "Not now" until then
    and "Skip the test" after. Pasting (`PasteConnectLink`), scanning
    (`ScanConnectCode`) and App Links (`ConnectLink`) each have their own
    action and problem words ("Nothing to paste", "not a connect link",
    "not a connect code"). iOS sends them.

## Phase 5b done: Android on the engine (2026-11-08)

76. **The bubble panel's whole order is `OrderPageView`, the app order
    page's record**, and its ring is `PanelView.sale.ring`, replacing
    `confirmations`, `confirmations_required` and `items` (amends 71). The
    views carry a `Ring` (step and confirmations); Kotlin keeps the arc's
    maths.
77. **Android copies every `Effect::CopyText` in `TillService.follow`**,
    labelled by its `CopyKind`, whether or not Till is on screen; the
    service owns Android's effects while the app is away and the activity
    while it's in front. Android's new-sale alert is the ongoing
    notification's heads-up, never a notification of its own.
78. **Android passes `returnUri` only when its debug return differs from the
    redirect**, and sends every incoming link as `SignInReturned` (closes
    57's first item on Android). iOS's own-Bell field goes through
    `EditOwnBell` (closes 68's second half).
79. **Previews draw from the saved shift's JSON with no engine**
    (`preview_shift`, `preview_sale`, `preview_close`, `preview_panel`,
    and the `TillPreview` record, named so because SwiftUI declares
    `Preview`): the on-device tests write their sales as saved-shift JSON,
    and nothing on Android uses a Kotlin sale model any more.
80. **The order page is published whole (`OrderPageView`), the tray header
    carries its `count` and the fold its `hidden` rows, and the engine opens
    a fold hiding the row a Back returns to** (android.md 269, 289). Engine
    records are taken apart into stable Compose types where a re-sent view
    would otherwise redraw.
81. **The Kotlin `core`, `sales`, `push` and `receipt` modules are gone**:
    Android is `:ffi` and `:app`, closing decision 2's move. All 257
    on-device tests pass on an API 36 emulator (two adjusted: one scrolls to
    a fold row a phone screen can't show, one allows edge antialiasing of
    up to 8 levels).
82. **The full on-device suite runs on the GPU with the guest's Vulkan
    off**: `emulator -avd … -gpu host -feature -Vulkan` (the RX 6800 XT
    through gfxstream's OpenGL) passed all 257 tests in 14 minutes, where
    plain `-gpu host` died with "null ctx" after 78 (emulator 37.2), and
    SwiftShader is several times slower. A real phone over USB works too.

## iOS phase I6: Clover on iOS (2026-11-08)

83. **A sign-in page that names no `redirect_uri` ends at the chosen point
    of sale's redirect** (step 4's choice while onboarding shows, else
    Settings'), and Square's when nothing is chosen. Only Refund leads a
    sale page; "Show the code again" is never the primary button. The
    refund-in-Clover sheet's body is plain text, "Not yet" a glass button,
    and its not-seen note is announced to VoiceOver. Still open: iOS fixes
    Clover's environment at `north_america` (Android has a build setting,
    android.md 137), and the debug build has no Clover stand-in redirect
    like Android's `/clover/debug/`.

## iOS phase I7: the TestFlight pipeline (2026-11-08)

84. **iOS signing is automatic and cloud-managed, through an App Store
    Connect Team API key with the Admin role**; no distribution p12 is kept
    (replacing the plan's `IOS_DIST_P12_*`). `release-ios.yml` archives,
    exports locally to check every bundle's `aps-environment` is
    `production` and its version and build, then exports again with
    upload. The entitlements file keeps `development`; the App Store
    export re-signs it as production. The build number is
    `github.run_number`; a `v*` tag must equal `MARKETING_VERSION`.
85. **Secrets and variables**: `ASC_KEY_ID`, `ASC_ISSUER_ID` and `ASC_KEY_P8`
    (the raw `.p8` text) in the `testflight` environment, and
    `APPLE_TEAM_ID` as a repository variable, since the team ID is public in
    the AASA, which `pages.yml` now fills from it (and publishes
    `.well-known`, which `upload-pages-artifact` had been leaving out).
    Pages still deploys without it, with a warning. The one-time Apple
    steps are in `docs/release.md`.
86. **The iOS app icon is the Android launcher mark**, rendered from
    `ios/Support/AppIcon.svg`; `ITSAppUsesNonExemptEncryption` is left to
    the owner's export-compliance answer. TestFlight builds use APNs
    production, so the relay's APNs key must send to production.

## iOS phase I4: iPhone and the Live Activity (2026-11-08)

87. **`LiveActivityState` carries `order_id`** so a tap opens
    `monokulo-till://sale/{id}`; Monokulo's pushes don't send it, so after a
    push the tap only brings Till forward. The content state's JSON uses
    the relay's keys (`paymentURI`, `expiresAt` in Unix seconds); an
    unknown or missing field falls back to idle, 0 or none. It goes stale
    at `expiresAt` only while waiting, seen or confirming.
88. **One Live Activity at a time**: one left by an earlier run is reused,
    none ends them all, and changes run in the engine's order; starting
    while Till is in the background fails, so it retries on `Opened`. Its
    static attributes (store name, location) are read when it starts. The
    widget extension doesn't link the core: the attributes live in
    `ios/Shared/TillActivity.swift`, compiled into both.
89. **The widget writes its own words, which await the owner's review**:
    "New sale · #1042 · +N more", "Tap to show code" or "Tap to enlarge",
    "1/2" while confirming, "Paid · sale #1042", and "refund in Square",
    which is wrong for a Clover till and wants the point of sale in the
    content state. iPhone is told apart from Slide Over by device idiom.
    "Show code in island" asks first in an alert and is disabled while the
    Live Activity is off.

## iOS phase I5: wake-ups over APNs (2026-11-08)

90. **With `EngineConfig.relay_url` set (iOS), the engine carries the push
    side itself**: iOS hands over the APNs token (`Action::PushToken`, hex),
    and the engine registers it with the relay (`POST {relay}/v1/register`,
    platform `apns`, app `till`), keeps the device's Web Push keys under
    `push.keys` (surviving `forget_all`), and decrypts RFC 8291 dings
    itself (aws-lc-rs, already in the tree through rustls). Android leaves
    it unset and keeps its `PushPlatform`.
91. **The Notification Service Extension opens a relayed ding with the core
    (`is_relayed_ding`), looks for the sale with `background_check` for up
    to 25 of its 30 s, and rewrites the alert in the core's words**; when
    time runs out the relay's own alert stands. A ding that found no sale
    is delivered passive and silent (hiding it entirely needs Apple's
    notification-filtering entitlement). In front, a relayed ding shows no
    banner; the core re-reads the kept shift first, since the extension
    may already have saved the sale.
92. **The app and the extension share the App Group container (the engine's
    `data_dir`, where the token lock lives) and the Keychain group
    `com.monokulo.till.shared`**, with items `AfterFirstUnlockThisDeviceOnly`:
    readable while locked, and never carried to another device by a backup
    with a single-use Square refresh token.
93. **The Live Activity's push token is kept but not yet registered**: the
    core has no client for Monokulo's push targets, so Live Activity
    updates while Till is in the background wait for one. The relay URL is
    `https://relay.monokulo.com`, a placeholder until it's deployed.

## The engine owns what Android still decided (2026-11-09)

94. **`SaleView.back_to_pos` names the point of sale it opens**
    (`BackToPos { press, mark: PosMark }`): Android opened Square whatever
    the label said. It opens Square Point of Sale (`com.squareup`) for
    `PosMark::Square`. For `PosMark::Clover` it steps back
    (`moveTaskToBack`): Till runs on the Clover device, over the Register
    it was opened from, and we haven't confirmed a launcher package for
    Clover's Register app. Today the engine offers the button only on a
    till on Square (decision 135 of android.md), so the Clover branch is
    for when that changes. iOS never gets the button.
95. **The engine owns the bubble's paid moment** (android.md 158 moves
    here). The overlay ran a second five-second timer beside the engine's
    `PAID_DWELL`. Now the engine keeps a `bubble::PaidMoment` (`None`,
    `Held`, `HeldDown`, `Lowering`; the pure `bubble::paid_next`, with
    Android's `PanelTest` cases ported). A payment starts `Held`. When the
    dwell ends with the panel still up on that sale and no code waiting,
    the engine sends `Effect::LowerPanel`, and `BubbleView.look` stays
    `Paid` until Android says the panel's window has gone. No new action
    was needed for that: `Action::PanelShown { up: false }` is already sent
    exactly when the collapse lands, or when the panel goes without one.
    A panel lowered during the dwell (`HeldDown`) keeps the bubble green
    to the dwell's end, with no collapse then. `bubble::landed` is gone:
    the engine's own paid change, checked against the bubble's focus,
    replaces it. Android keeps only the collapse's animation.
96. **The engine follows `close::outcome` for every close-sheet answer**,
    wherever the sheet was asked. "No: refund it" refunds the sale and
    opens it (`CloseThen::Sale`), "Not sure" closes it and leaves it for
    the shift, "Yes" does nothing. Android's shift used to send
    `OpenSale` itself after "No" from a row, and iOS didn't, so the two
    differed. Now both send `Action::Close` alone and land in the same
    place. `CloseAnswer.then` tells the platform where an answer lands, so
    Android's stage can grow the sale out of its row without its own copy
    of the rule.
