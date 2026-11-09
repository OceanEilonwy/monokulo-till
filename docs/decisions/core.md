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
   (Amended by 99: the symbol policy moved back into Rust.)
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
    the label said. It opens Square Point of Sale (`com.squareup` on
    Android, `square-commerce-v1://` on iOS) for `PosMark::Square`. The
    engine offers it on a till on Square, for that till's own sales, on
    every device: Android beside "Copy address", iOS on Paid on iPhone and
    iPad alike, shown only where `canOpenURL` says Square is installed and
    Square isn't already beside Till (amended 2026-11-09, decision 141;
    it used to leave iOS out, and iPad drew it anyway). There is no "Back
    to Clover": Clover runs on its own device and Till on a separate phone
    or iPad beside it (android.md 135, decision 142), so there is no
    Clover app to switch to and the engine never offers the button on a
    till on Clover. `PosMark::Clover` stays in the record only because the
    mark is shared; Android's `backToPos` does nothing for it.
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
97. **`PanelView` says its face and its buttons**: `face: PanelFace`
    (`Code`, `Paid`, `Ended`, from the hero and whether the code came down
    unpaid), `status_left` ("· 14:52 left"), `received` ("… XMR received"
    on the paid face) and `buttons: Vec<PanelButton>` (a `PanelAction`, a
    `Press` and whether it is filled), shaped like the sale page's
    `PageButton`. Android had worked all of these out in `PanelSale.of`
    and `Faces.kt`, and wrote "Copy address" itself instead of using the
    sale's `copy_address`. It now draws what the engine lists, and the
    Kotlin `PanelFace` enum is gone.
98. **`Update::Nav` says whether the system's Back is the app's**
    (`back_enabled`): false on Welcome, the shift, and step 4 during
    setup, where Back leaves the app (android.md 77); true everywhere
    else, including step 4 opened from Settings. `MainActivity` worked
    this out from the screen and step 4's view. It now reads the flag, and
    keeps only the stage's own rule that a second Back while the shift is
    still arriving stays in the app (android.md 260 c), which is about the
    animation. iOS has no system Back and ignores the field.
99. **The engine picks an amount's symbol; the platform gives only locale
    data** (amends 9). Android's `Money.kt` held the policy: the narrow
    symbol, country letters taken off shared signs ("US$" becomes "$"),
    the clash with the till's own dollar ("US$4.50" in Australia), and a
    symbol-less currency after the number ("4.50 CHF"). iOS printed
    whatever `NumberFormatter` gave, so the two differed. The policy is
    now `monokulo_client::amounts::written`. `Platform::format_money`
    returns the locale's pattern with `¤` where the symbol goes, and the
    new `Platform::currency_symbols` returns how the locale and `en-001`
    write a currency's symbol, plus the locale's own currency.
    `MoneyTest`'s cases are ported to `monokulo-client/tests/money.rs`
    against stand-in locales holding the JVM's data for each one.
100. **`Ring` is ready to draw**: besides its step and confirmations it
     carries `fill` (0 to 1: a quarter a step, or `SEEN_FILL` plus the
     confirmations' share of the rest), `spins` and `green`, which
     Android's `PhaseRing.kt` used to work out (`ringFill`, `ringSpins`,
     `ringGreen`, with `PhaseRingTest` ported to `view::ring_tests`).
     `ring_of(step, confirmations, required)` is exported for a badge
     drawn without a sale. Android keeps the sweep, the turn and the
     colour fade. iOS's `RingGlyph` now draws `fill` in the accent, or in
     the paid green once `green`, instead of quarters by step.
101. **`Update::Nav` says which way the change goes**: `back` (toward the
     shift, from a code to its sale, from Settings, or backwards through
     setup) and `steps` (both screens are steps of setup, the shift last,
     so they slide). Both are worked out from the screen last published
     (`engine::till::direction`, with `NavTest`'s cases ported). `Nav.kt`
     no longer keeps the depth and setup-order tables: `HeldMove` reads
     the engine's `Way` when a change starts, uses it for the slide and
     for which screen goes underneath, and treats a back gesture's preview
     as going back, since the engine hasn't heard of it yet. iOS's
     navigation is its own and ignores the fields.
102. **The engine says when to look for Orbot** (`Effect::ProbeTor`,
     Android only): as the app comes forward (`Action::Opened`) and each
     time step 3 or Settings comes up from another screen, as
     `MainActivity`'s `TorProbes` and `onStart` did (decision 54). Android
     runs the probe in `TillService.follow`, whose collector lives as
     long as the process, so the probe sent while the activity is still
     starting isn't dropped. iOS has no Tor, never gets the effect, and
     its `perform` names it with `LowerPanel` as one it ignores.
103. **Shared text goes to the engine whole** (`Action::SharedText`).
     `connect_code::shared_link` picks the first word that is a store's
     connect code, else the first word that is any link (which then gets
     "not a link" as before), and nothing happens when there's no link.
     Android used to pick the first word containing "://" itself. iOS has
     no share path; it opens connect links as URLs (`Action::ConnectLink`).
104. **`ShiftTop.mark` is the till's point of sale's mark** (the same
     `PosMark` as `BackToPos`, from the till's point of sale; `None`
     without one). Android's shift badge used to dig the mark out of
     `SettingsView`'s point-of-sale section. iOS's top bar shows the
     Monokulo logo, not the point of sale's mark, so it doesn't use it.
105. **Every copy goes through the engine** as an action that comes back
     as `Effect::CopyText`. Settings' "Copy wallet address" is the new
     `Action::CopyWalletAddress` (the store's whole address as last read),
     on Android and iOS alike. The ongoing notification's "Copy address"
     now sends `Action::CopyAddress` with the sale's order id instead of
     carrying the address in its intent. Android's `TillService.follow`
     puts every copy on the clipboard, and iOS's `perform` does the same.
     One edge: if the receiver wakes a process whose kept state can't be
     read yet, the copy is dropped with the other actions that wait for
     it.
106. **The engine says how long ended notifications stay and when they
     are bundled.** `Effect::Notify` carries `timeout_ms`
     (`notification::notice_timeout`: five minutes for "Paid", none for an
     unpaid one). The exported `ended_summary(showing)` now returns
     `Option<EndedSummary>`: `None` below two, else the summary's notice and
     its own timeout. Android had a hard-coded `ENDED_MS` and its own
     "two or more" rule. Android still counts how many of the group are
     showing, since only the system knows. These aren't fields on
     `NotificationView` because the count is the platform's, and a field
     there would be a guess. iOS posts only the new-sale alert (decision
     41), so none of this applies to it.
107. **The whole order's words come from the engine**: `OrderPageView.back`
     ("Back to the sale") and the "Show the code" bar's line
     (`code::show_code_line`, "$79.21 · 0.31684 XMR · 14:52 left"), carried
     as `CodeView.show_code` for the app (so it's there even without a
     receipt) and `PanelView.show_code` for the panel. Android used to join
     amount and under itself, the panel joined its own variant, and
     `MainActivity` wrote "Back to the sale". The panel's own "Back to the
     code" and "Show the code" headings stay in its screen, as words
     written once. iOS's order screen has neither bar nor labelled back.
108. **The on-device tests' sales are the engine's fixtures.**
     `till_app::preview_sales` exports `PreviewSale` (with UniFFI
     defaults: $8.40 of this till's on Square, waiting, unless the test
     says otherwise; the design's sample receipt behind `receipt: true`),
     `PreviewPhase`, `preview_sales_json` (the saved shift's JSON, written
     by `shift_json::encode`), `preview_sale_live` and
     `preview_phase_step`. Android's `SavedShift.kt`, a 124-line hand copy
     of `TillSale`'s serde schema, is gone, along with the app's
     kotlinx-serialization plugin and test dependency, which only it used.
     `CustomerNameTitleTest` stays: besides the wording (which
     `sale_text`'s tests cover), it checks that the bar is one TalkBack
     heading, which only a device can.
109. **`Update::Work` is shown on both platforms, where no view already
     shows it.** Neither app drew it: Android didn't read it, and iOS kept
     `busy` and `problem` without drawing them. So a refused write from
     Settings, a store that couldn't be forgotten, or kept state that
     couldn't be read went unsaid. `Update::Work` now also carries
     `shown_busy` and `shown_problem`, which leave out what step 3's and
     step 4's own views already show (their busy button and problem), so
     nothing is said twice. Each app draws them over the top of the
     screen with a part it already had: Android a thin progress bar and
     its `Notice` (`WorkBar`), iOS a `ProgressView` and its `ProblemNote`.

## The iPad redesign, phase 1: the core (2026-11-09)

The engine side of [ipad-redesign.md](../ipad-redesign.md) (its decisions
4 to 14, 17 and 20): counters, cash, customer names, the onboarding steps
and copy, tones and progress.

110. **Every found sale carries the device that took it.** `FoundSale`
     has `device_id` (as before) and `device_name`, both from Square's
     `device_details` (`device_name` on one line, blank as none). Clover's
     payments name only the device's id (`device.id`); its name comes from
     Clover's devices endpoint (decision 142), and only until that answers
     does a Clover sale's menu line say "Device" and the end of the id.
     Both stay on the device, in the saved shift.
111. **New view fields cross UniFFI with defaults.** Every field added to a
     record the platforms construct or that the iOS rework reads (`Press`,
     `PosStep`, `ShiftRow`, `ShiftView`, `SettingsView`, `SaleView`,
     `SalePageView`, `OnboardingView`, `LiveActivityState`) has a UniFFI
     default, and no existing enum variant gained a field, so Swift's
     positional patterns and both platforms' tests keep compiling. New
     choices are new records (`CashButton`, `CounterMenu`, `TestStatus`)
     rather than new variants of enums the apps switch over (`PageAction`,
     `StateIcon`, `ShiftEntry`). The only new variants are the two new
     onboarding screens (`Screen::Counter`, `Screen::ShowSales`) and their
     pages, and the new `Action`s.
112. **The order's name: the fulfillment recipient first, then its
     `ticket_name`.** A recipient is a person the seller named on a
     pickup, delivery or shipment; a ticket name is typed at the counter
     and is often a label ("Table 4", or a default an open ticket got), so
     it's the fallback. Both are one line and at most 80 characters, and
     neither is ever sent to Monokulo: orders carry the amount, currency,
     receipt number and idempotency key alone (a test checks a taken cash
     sale's order body).
113. **This counter's device comes from the test sale, and only its sales
     alert.** When step 4's test sale names its Square device, the engine
     keeps it (`counter.device`: id, name, when) and C4 asks "Only sales on
     Front iPad (recommended)" or "Every sale at Front Counter"
     (`counter.every`, `Action::SetAlertFor`). With the test skipped, or at
     a point of sale with no device, there is no device and every sale is
     this till's. `Till::with_device_id` (through `TillOptions`, which
     `RunnerSide::till_options` and the background check read from the
     model) no longer drops another device's sales: it still makes their
     orders, by the same idempotency key, so they can be listed and their
     code shown, but without the receipt, which stays with that counter's
     till. A sale another till found (no `found`) counts as another
     counter's while this one has a device. Another counter's sale, while
     alerts are for this device (`Counters::quiet`), never alerts (iOS
     alert, background check, Android's heads-up), never rises out of the
     shift (it's treated as left), never buzzes or shows paid, and isn't in
     the bubble's, the ongoing notification's or the Live Activity's sales.
     The apps read `ShiftRow.other_counter` and `ShiftRow.device`. One
     edge: a sale this counter took that another till announced first
     arrives quiet and doesn't alert when this till adds it a moment later.
     Changing the point of sale forgets the device, as does forgetting the
     store.
114. **The title menu chooses whose sales the list shows.**
     `ShiftView.counters` (`CounterMenu`) is there while this counter has a
     device or the sales come from more than one place: its title is the
     choice shown, its items this counter ("This counter · alerts"), each
     other device with sales this shift ("No alerts here", or "Alerts here
     too" with every device), "Other tills" for sales other tills found,
     and "All counters", each with its waiting count. `Action::ChooseCounter`
     chooses; until then the list opens on "All counters" when "Show other
     counters' sales" is on (the default, `counter.others`), else on this
     counter. The summary, chips and rows follow the choice; the menu's
     counts are over every sale.
115. **Settings C5.** `SettingsView.counters` (Square or Clover, signed
     in; Clover's since decision 142): "This
     device: Front iPad" with when the test sale named it, "Every device at
     Front Counter" naming the devices seen, "Find this device again with a
     test sale" (`Action::FindDevice`: the next sale found that names a
     device becomes this counter's; tapped again while waiting, it stops),
     and "Show their sales in the list".
116. **Cash.** Square's completed `CASH` payments are found as well as the
     Monero tender (`FoundSale::cash`). With "Treat cash sales as Monero"
     on (the default, `cash.monero`) a cash sale gets its order and code as
     it's found and follows a Monero sale's flow. Off, it's listed quietly
     as an offered sale (`TillSale::offered`, order id `cash:` and the
     payment's id, no order) under its own header ("Cash · paying in
     Monero?", drawn as an `Earlier` entry), with `CashButton`s "Take it in
     Monero" (`Till::take_in_monero` makes the order then; the sale with the
     order replaces the offered one and opens on its code) and "Keep as
     cash". A cash sale with a code offers "Paid in cash"
     (`TillSale::paid_in_cash_at`, `Standing::PaidInCash`): its code comes
     down and nothing is refunded. A cash sale is never refunded through
     the point of sale (`Till::refund` returns at once), its close sheet
     has no "No: refund it", and its page has no Refund. Clover: cash would
     be a second query by Clover's cash tender; not done, and Clover is
     unchanged.
117. **Cash paid in Monero is counted for the end of the day.** A paid cash
     sale's row says "Paid in Monero · cash in Square", and
     `ShiftView.cash_in_monero` is "2 cash sales paid in Monero · $32.90",
     as Square's books say Cash for them.
118. **The customer's name leads.** `ShiftRow.name` (the name, else "Sale
     #m2Qe"), `ShiftRow.amount`, and the sale's number at the end of `sub`
     when a name leads; `title` is still name and amount on one line for
     Android. The sale's title was already the name. iOS's alert is "Sam ·
     $24.50", "Monero sale. Tap to show Sam the code." (a cash sale's
     says so), and `LiveActivityState.name` carries it, set on the device
     (the relay's pushes never have it). Android's notifications keep the
     sale's number, as they show on the lock screen.
119. **Green is for paid alone, and progress is in thirds.** Seen and
     confirming rows are muted, the ring's arc is green only once paid,
     "Watching" and Settings' "Connected" are plain, and a paid row is
     green. `SaleView.progress` (`PaymentProgress`) is the Till widget's thirds,
     waiting 0, seen 1, confirming 2, paid 3, with its label ("Seen on the
     network"), count ("1 of 3") and what a screen reader says.
120. **How a new sale shows up is the engine's to keep.** `ShowSales`
     (Alert, the default; FloatingWindow; SideBySide), kept as
     `sales.show`, chosen in O4 (`Screen::ShowSales`, iPad only, after
     Alerts, in place of the old Slide Over step) and in iPad Settings
     (`SettingsView.show_sales`), by `Action::ChooseShowSales`. O4's page
     carries the sample alert its "Send a test alert" posts. The platform
     does the showing. Android keeps its bubble step.
121. **Onboarding's steps.** No step label on iOS: iPad's rail (Welcome,
     Connect your store, Point of sale, Alerts, How sales show up, Ready),
     iPhone's `OnboardingView.progress` (value of total, none on Welcome).
     Android keeps "Step N of 6". Step 4 is three stages in setup
     (`PosStep.stage`): O1 the choice alone with "Continue with Square" and
     "Set up later" (with its footnote, `PosStep.skip_footnote`), O2 the
     card and the sign-in with Back to O1, O3 the test sale with its status
     bar (`PosStep.status`: "Waiting for the test sale…", "Continue unlocks
     when it arrives"). `Press.busy` says a button shows progress in itself:
     the sign-in while it runs, and step 3's Continue while the store is
     read (review finding I6). After the test sale, C4
     (`Screen::Counter`) when it named a Square device. iPad's Ready no
     longer says Slide Over.
122. **iOS alerts are a status row.** `SettingsView.alerts_status`: "New
     sales and payments · On", or "Off" in warning, with "Open Notification
     Settings" and its footnote (review finding I15). Android keeps its
     switch.

## Alerts once per sale, and names on Android (2026-11-09)

Three problems from the owner's iPad test: a missed alert, a double one,
and Android's notifications without the customer's name.

123. **One new-sale alert per sale per till, kept in a shared record.**
     Whether a sale alerts was worked out three ways that never agreed:
     the engine alerted a sale `Phases` saw arrive Waiting, the background
     look (iOS's Notification Service Extension) one missing from the
     shift it read at its start, and iOS gave the two alerts different
     identifiers (the app's is the order id; the extension's is the
     push's own, which it can't change), so neither replaced the other.
     Square sends `payment.created` and then `payment.updated` for one
     payment, the Bell dings for each, and iOS runs the extension once
     per push, side by side: both looks list the payment before either
     has made its order and kept the shift, so both alert. That is the
     likeliest cause of the double the owner saw (a test reproduces it
     against the old rule); the app's own timer or a ding it handles
     racing an extension run is the same race. Now every path claims the
     alert first (`alerted::Alerted`): the order ids already alerted are
     kept under `alerts.posted` (the newest 500, through the platform's
     secrets, so in the Keychain group the extension shares, and kept
     across restarts and `forget_all`), and a claim reads, checks and
     writes it under an exclusive `flock` on `alerts.lock` in the data
     directory, as the sign-in's refresh does (decision 13). Only a
     claimed sale is alerted. The engine claims every live sale that
     isn't another counter's (`phases::NewSales`) whether or not it posts
     anything: one seen on screen counts as told. Its first look still
     only learns, claiming without an alert what the till started with.
     The background look claims what is new to the shift or was quiet in
     it. A second extension run that loses the claim shows the relay's
     own words, passive and silent, as for a ding that found no sale:
     iOS must show something, and hiding it needs Apple's filtering
     entitlement. The app's own alert replaces, and its clear removes,
     any extension alert that keeps the same sale (`TillShared
     .deliveredAlerts`). This also closes 113's edge: a sale another till
     announced first is kept quiet and unclaimed, so it alerts once this
     till's own point of sale finds it on this counter's device, and only
     then; found here first, Monokulo's event for it changes nothing.
124. **Android's notifications name the customer, on the lock screen
     too.** This reverses 118's "Android's notifications keep the sale's
     number, as they show on the lock screen", and with it android.md
     decision 78's public version: the owner accepts that a customer's
     name shows on the lock screen, as it already does in iOS's alert,
     because the name is how staff tell orders apart. A new sale's face
     says what iOS's alert says ("Sam · $24.50", "Monero sale. Tap to
     show Sam the code.", the sale's number leading with no name), with
     the sale's number and the place as its sub-text; seen and confirming
     faces have the name as their sub-text, and "Paid · Sam" and the
     ended notices lead with it. What was bought still never leaves the
     private version.

## The iPad redesign, phase 3: alert first and the floating window (2026-11-09)

The iOS side of [ipad-redesign.md](../ipad-redesign.md) decisions 1, 2
and 8, and what the engine adds for it.

125. **The floating window draws the Live Activity's state.** On iPad,
     while a new sale shows up in a floating window (`ShowSales::
     FloatingWindow`), the engine publishes `Update::LiveActivity` as it
     does on iPhone, with the plain payment URI while the sale waits
     (as "Show code in island" would), and none otherwise. The app sends
     it to its Picture in Picture window rather than ActivityKit. The
     state already leaves out another counter's sales and goes from paid
     back to idle after the paid dwell, which is the window's Paid and
     back to Watching. A cash sale offered in Monero isn't in it: the
     window reads the first of this counter's from the shift's cash
     section. iOS's alert for a new sale is still claimed as decision
     123 says, but not posted while the window is up, as the window
     shows the sale itself: the claim counts it as told.

## The iPad redesign's last gaps (2026-11-09)

What was still decided in Swift after phase 3: the cash glyph, the
floating window's cash sale, copy written in the app, and the progress
label while a payment confirms.

126. **Cash sales have their own glyph.** `StateIcon::Cash` (the
     banknote) is a cash sale offered in Monero, kept as cash or paid in
     real cash, on its row and its page's hero, in place of the clock and
     the X they borrowed. `ShiftRow.cash` says a sale was rung up as cash
     whatever it became, so a cash sale paid in Monero keeps its green
     tick and iOS marks its amount with a small banknote, as Square's
     books call it cash. This is the one exception to decision 111's
     "no new variants of enums the apps switch over": a glyph is what
     the enum is for, and both apps were changed with it (Android draws
     a banknote of its own in the row's badge).
127. **The empty till's words come from the till's state.** iOS's idle
     panel says "Ready for the next sale" (Android keeps "No sale right
     now") with how a sale is rung up and reaches the cashier: "Ring it
     up in Square and choose Monero. An alert brings you straight to the
     code." with alerts (the default), "The floating window shows the
     code." with the floating window, "The code appears here." side by
     side and on iPhone. `ShiftView.status` carries its chips
     (`StatusChip`: kind, words and tone): "Watching Square",
     "Reconnecting…" or "Not watching"; "Alerts on" or "Alerts off" in
     warning where the platform says whether Till may alert (iOS); and
     "Instant with the Bell" while a Bell wakes the till. The Swift chips
     read the top row's tone, which decision 119 had made plain, so they
     never said Watching; that's gone with them. `ShiftView.ready` (iPad,
     a shift with sales) is E2 beside the list: the heading and "The
     last one was paid 4 minutes ago. Ring up the next in Square and
     choose Monero.", from the newest paid sale ("just now", "a minute
     ago", minutes within the hour, then "at 9:41 AM"), no line while a
     sale waits, and the shift ticks while the minutes move. Android
     draws the chips as one muted line in its idle panel; it has no
     column for E2.
128. **The floating window's cash sale is published.** `ShiftView
     .offered_cash` is this counter's newest cash sale offered in Monero
     and not kept as cash, with its title and the strip's line. It's
     worked out over every sale, not the list: the old way, matching the
     cash section's header and a `cash:` order id in the shift's entries,
     lost the sale while another filter or counter was chosen and broke
     with any change to the header. This replaces decision 125's last
     sentence but one. Android has no floating window and doesn't draw it.
129. **The progress line counts confirmations.** `PaymentProgress.line`
     is the one line under the bar, never wrapped: the thirds' words and
     count ("Seen on the network · 1 of 3"), and while confirming, where
     the store says how many confirmations it needs, how many it has
     ("Confirming · 4 of 10 confirmations", held at the most needed, as
     the row's "Confirming · 4 of 10" is). The bar stays in thirds;
     `said` reads the same. iOS shrinks the line a little before cutting
     it short.
130. **iOS takes the words written once from the engine too.** This ends
     decision 26's split on iOS: the onboarding pages' headings and leads
     (`OnboardingView.heading`, `lead`; O1 "Which point of sale do you
     use?", O2 "Sign in to Square" with "Square's own page opens…", O3
     "Try a test sale" and O3's connected line, `PosStep.connected`),
     the label over a page's card (`section`), Welcome's features and
     "Get started", step 2's words around the camera (`ScanWords`),
     Confirm's footnote, "Wrong store: disconnect" and "Start Till";
     Settings' title and section titles, Disconnect's words, the Dynamic
     Island's switches and question, "Show their sales in the list";
     the sale's "Close the sale", "Customer view", "Share" and what the
     share sheet calls it, "Order", the customer's "Scan with any Monero
     wallet", the order page's title and "Total", Paid's "Back to
     Square", and "This shift". Android keeps its own copies for now; the
     fields are there when it wants them. Left in Swift: the system's
     verbs (Done, Cancel, Back, Change, Close on a swipe, Settings on the
     gear), what only a screen reader hears where the engine has no such
     line, the mock notification's "Till" and "now", and the floating
     window's frame words ("Watching", "Paid · $24.50", the countdown),
     which it shares with the Live Activity widget: that draws Monokulo's
     pushes, which carry no words, so the vocabulary stays with the
     widget's code.

## Review fixes before the redesign reaches main (2026-11-09)

Three findings from a review of the product decisions. The other four
(ending a shift on iOS and a new shift each day, iPad's ended and paid
notices, the paid moment for another counter's sale opened on purpose, and
a new code for an expired sale) move into the core's rebuild as a fact
journal and are not done here.

140. **What the device draws itself keeps the receipt in its code.**
     Decisions 125 and 128 gave the iPad's floating window, and 40 the
     iPhone's "Show code in island", the plain payment URI, because content
     that goes through the push relay must never carry what was bought. A
     window drawn on the device, and a Live Activity the app alone updates,
     never go through the relay. So `LiveActivityInput.receipt` (true for
     the floating window always, and for the Live Activity while no push
     token updates it: `LIVE_ACTIVITY_PUSHED`, false until the core can
     register Monokulo push targets, decision 93) gives `payment_uri` the
     sale's own code with its receipt. Once Monokulo pushes the activity
     through the relay, the app's updates use the plain URI too, so a push
     and an update never disagree. Below about 200 pt the window draws no
     code anyway (ipad-redesign.md decision 2). Settings' island footnote
     and question now say that the code, with what was bought, shows on the
     lock screen, instead of the push service the code doesn't yet go
     through.
141. **Back to Square on every iOS device.** Decision 94 left iOS out while
     ipad-redesign.md decision 1 drew it on iPad from Swift's own rule. The
     engine now offers `SaleView.back_to_pos` and `SalePageView.back_to` on
     a till on Square, for its own sales, on Android, iPhone and iPad alike
     (`back_to` is empty otherwise). iOS shows it on Paid where `canOpenURL`
     says Square is installed and Till and Square aren't side by side
     (`BackToSquare.shown(offered:showSales:squareInstalled:)`), with no
     iPad check. Decision 94 is amended to match.
142. **Clover runs on its own device; Till on a separate phone or iPad
     beside it** (the owner, 2026-11-09). There is no Clover app on Till's
     device to switch to, so there is no "Back to Clover" (android.md 135,
     decision 94, whose Register wording was wrong). Like a Square counter,
     a Clover counter's device is learned from the test sale (step 4 no
     longer keeps it for Square alone) or "Find this device again", and
     Settings offers the counters section (Alert for, Find this device
     again, Show their sales in the list) on Clover too, in Clover's words;
     cash stays Square's (decision 116). Clover's payments name only a
     device id, so when a sale or this counter names a device Clover's list
     hasn't, the engine reads `GET /v3/merchants/{mId}/devices` once a run
     (`CloverApi::devices`) and keeps the names under `clover.devices`
     (forgotten with the store); `Counters.names` gives them to
     `counters::device_name`, so the menu says "Front counter" rather than
     "Device 3F2A". A device with no name is called by its model and the
     end of its serial ("Clover Mini 4F2A"). The devices endpoint needs
     Merchant read, which the till's Clover app already asks for
     (development.md), so no new permission; a refused read leaves the
     devices unnamed and is logged.

## The ledger: the till-ledger crate (2026-11-09)

The core of [ledger.md](../ledger.md): the crate `till-ledger`, its facts,
merge, views, effects and database.

143. **Sales are facts in an encrypted journal with per-sale
     projections.** One SQLCipher database per device (`till.db`), opened
     through rusqlite with SQLCipher bundled: CommonCrypto on Apple
     platforms (`bundled-sqlcipher`), a vendored OpenSSL elsewhere
     (`bundled-sqlcipher-vendored-openssl`), and refused when `PRAGMA
     cipher_version` is empty. The key and salt are 48 random bytes made
     in Rust (`KeyMaterial::generate`) and kept as one secret,
     `ledger.key`, through the platform's secret store; the first 32
     bytes of the file stay plain (`cipher_plaintext_header_size`) so iOS
     recognises the WAL database (0xdead10cc), and the key is given raw,
     so no key derivation runs at open. A missing or wrong key with a
     database moves it aside (`till.db.lost-<ms>`) and starts a new one
     that learns without alerting; an error from the store fails and
     changes nothing. Only facts that change a projection are
     journalled; each write is one `BEGIN IMMEDIATE`, and effects are
     claimed in the same transaction (`alerts`: `due` for one the
     extension leaves to the app, `pending`, `posted`; a claim pending
     30 s is delivered again). Considered and rejected: a plain upserted
     table without a journal (the owner wants the journal for audit and
     replay) and column encryption instead of SQLCipher (the owner chose
     SQLCipher).
144. **The shared merge is a join.** Every till fed the same facts in any
     order, with repeats, holds the same `SharedSale`, so ledger.md §5's
     rules are read order-free: Paid absorbs every other phase, except
     that a double spend gives way only to a Paid Monokulo calls final,
     in either order; an expiry or cancellation not final beats only
     Waiting, so a late Seen, Confirming or Partial shows (Monokulo
     doesn't expire an order whose payment it has seen), and a final one
     beats progress; otherwise Seen, then Confirming by confirmations,
     then Partial by amount received. The POS's payment is joined by its
     `updated_at`, its order read by the payment's `updated_at` at the
     read, a refund by how far it got. When a sale was seen, paid, ended
     or refunded is this device's first sighting, kept in its local
     marks, so `shared` is equal across tills.
145. **New code.** A sale's nth order has the idempotency key of the
     first with `#n` appended; the first is unchanged
     (`Till::idempotency_key`). A Paid on any attempt decides the sale
     and shows the others as superseded; two paid attempts warn ("Paid
     twice"). This reverses android.md 43 by the owner's decision.
146. **Closes, paid in cash and keep as cash stay local** (`Mark` facts
     in `LocalMarks`) until Monokulo lets apps cancel an order. A reopen
     after a close undoes it; a Paid after a close or a refund shows as
     Paid, flagged.
147. **Cash orders only from a till set to the sale's device.**
     `may_make_order`: a Monero sale's order from any till; a cash sale's,
     with cash as Monero on, only from a till set to its device or to
     every device; the cashier's own "Take it in Monero" and New code
     from any till.
148. **The trading day starts at 04:00 and live sales always show.** The
     shift's window is from its start mark, or 04:00 local if later, to
     its stop mark; waiting, seen, confirming and part-paid sales are
     listed whatever the window, which decides the totals, the cash paid
     in Monero and which ended sales are listed.
149. **Retention is 30 days.** Sales rung up longer ago are deleted with
     their journal rows and projections (`ledger.retention_days`
     changes it). An ended sale from before the shift is compacted into
     one `Snapshot` row, at most hourly, and old app heartbeats are
     dropped. Keeping the database out of backups is the platforms'
     part.

## The ledger on the devices (2026-11-09)

The platform side of [ledger.md](../ledger.md) §4, §11 and §14: SQLCipher
in the one library on every target, and what only iOS and Android can do
for the ledger. The ledger crate's decisions (PR 59) come before these.

150. **SQLCipher is compiled into the library everywhere.** The workspace
     takes `rusqlite` 0.41.1 (libsqlite3-sys 0.39.1) with default
     features off and `bundled-sqlcipher`, so there is one libsqlite3-sys
     and never the system's SQLite. Its crypto differs by target: on
     Apple targets (the XCFramework's `aarch64-apple-ios` and
     `aarch64-apple-ios-sim`, and the Mac build the Swift bindings come
     from) it is CommonCrypto, in libSystem, with Security and
     CoreFoundation, which the TillCore package already links for the
     app and the extension; on Linux (CI, and the library Android's JVM
     tests load) the system's libcrypto; on Android, which has no
     libcrypto an app may link, till-ffi adds
     `bundled-sqlcipher-vendored-openssl` for that target only, so
     OpenSSL's libcrypto is built from source into each ABI's `.so`
     (a cold build's cost, which rust-cache then keeps). Features unify
     per target, so if till-ledger also asks for the vendored OpenSSL
     off Apple, Linux builds use it too; it is still one libsqlite3-sys.
     `OPENSSL_DIR` must stay unset on the Mac, or libsqlite3-sys would
     take Homebrew's OpenSSL for the iOS targets. till-ffi exports
     `sqlcipher_check(dir)` (its own bindings, `till_ffi`), which opens a
     scratch database with a raw key and salt, a 32-byte plain-text
     header and WAL, writes and reads a row, checks the file's header is
     plain and the row isn't, that another key reads nothing, and returns
     `PRAGMA cipher_version`. Rust CI, Android's JVM tests and iOS's
     simulator tests call it. It goes once the ledger's open does the same
     check (ledger.md §4).
151. **The platforms only pass on the process's life; the core decides.**
     The engine has `suspend()` (blocks until any write is done and the
     ledger is closed), `resume()` and `ledger_changed()`, stubs until the
     integration: iOS calls `suspend` as the app goes to the background,
     off the main thread under a background task assertion, and `resume`
     as it comes to the front, both in order on one queue; Android calls
     them from the activity's `onStop` and `onStart` on a thread of their
     own. The extension posts the Darwin notification
     `com.monokulo.till.ledger-changed` after every round, whether or not
     it wrote, and the app calls `ledger_changed` when it hears it; the
     engine works out whether anything changed. Android has one process
     writing the ledger and never calls it. Around each write the core
     takes `till_app::lifecycle::hold`, which begins and ends a hold
     through the platform's `BackgroundWork` (`set_background_work`, once
     per process before the engine or `background_check`): on iOS
     `ProcessInfo.performExpiringActivity`, which the app and the
     extension alike may call from any thread; Android gives none. The
     key and salt are made in Rust and kept through `Platform::secret_get`
     and `secret_put`, which on iOS are already `AfterFirstUnlockThisDevice
     Only` items in the access group the extension shares, so the
     extension reads them while the device is locked after the first
     unlock.
152. **Nothing of the core's is backed up.** iOS marks the core's data
     directory (the App Group's container) excluded from backup each time
     it's looked up, rather than `till.db` and its WAL and shared-memory
     files one by one, which come and go and would lose the flag. That
     matches Android, where the core's files are in `noBackupFilesDir`,
     `allowBackup` is off and the backup rules exclude every domain, and
     costs nothing: the Keychain items are this device's only, so a
     restored container could open nothing.