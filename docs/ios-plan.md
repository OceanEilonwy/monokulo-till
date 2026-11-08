# Till for iPhone and iPad: plan

Status: accepted 2026-11-08. Phases 0 to 4 are built, and 5a has begun; decisions
along the way are in [decisions/core.md](decisions/core.md).

The app is **Till** ("Monokulo Till" in full) since 2026-11-08; it was
Monokular. The repository keeps its name until the owner renames it.

This plan covers two pieces of work, in order:

1. **A Rust core.** Move everything the Android app does that isn't drawing
   or an Android service into Rust, the way Tern and Fob are built. Android
   keeps shipping the whole time and moves onto the core one module at a
   time.
2. **The iOS app.** A SwiftUI app on the same core for iPhone (Live Activity
   and Dynamic Island) and iPad (Slide Over). It follows the design canvas
   [Till — Live Activity & Slide Over](https://claude.ai/artifact/Kb9AtJFBF3ockyddomLc6t)
   and the product rules Android has settled since that canvas was drawn.

It also sets up CI for both: Rust on Linux, Android on Linux, and iOS on a
macOS runner.

---

## 1. Why a Rust core now

[design.md §9](design.md#9-the-till-repository) chose native cores
(a SwiftPM `MonokularKit` beside the Kotlin `core`), and only `orf-codec` in
Rust: "a shared Rust core for it isn't worth the build complexity for this
amount of logic". Two things have changed since then:

- **There is much more logic.** About 6.5k lines of Android's 16k lines of
  main source are platform-neutral: the Monokulo client, the Square and
  Clover sources, the till's state machine (`Till`, `Standing`, `Shift`),
  the Bell registration machine, the persisted model, and the copy chosen by
  logic (`SaleText`, `PosText`, `ShiftList`, `SalePage`, `WakeUpsText`).
  More than 300 decisions in [decisions/android.md](decisions/android.md)
  pin this behaviour down, many of them word for word. Writing it all again
  in Swift would mean two implementations drifting apart, decision by
  decision.
- **The build cost is already paid.** The Android app already builds Rust
  with cargo-ndk and UniFFI (decision 8). Tern and Fob show the pattern
  working end to end.

This plan replaces that choice in §9. When phase 0 lands, §9 is updated to
match, and the change is recorded as a decision.

What stays native on both platforms: screens and motion, the bubble and
overlay (Android), Live Activity and Slide Over (iOS), notifications, the
foreground service, the keystore or Keychain, the camera, launching the
browser for OAuth, and push registration with the OS.

---

## 2. Shape of the core (following Tern)

Tern and Fob solve the same problem in two different ways:

- **Tern** has one `Engine` object. A single tokio task owns the app's
  state. The UI sends `Action`s and gets back `Update`s (one per screen
  slice, sent only when that slice changed) and `Effect`s (open a URL, show
  a notification, and so on) through a listener callback. All behaviour is
  in Rust, including copy, what Back does, and which screen is on top. The
  bridge is a cargo feature on the core crate, so there are no mirrored
  types to keep in step.
- **Fob** has a large object API (`FobWallet` with many `async fn`s) and a
  separate ffi crate of about 3,200 lines that mirrors the core's types.

**Till follows Tern.** `AppViewModel`'s screen graph, `back()`, and the
connect, sign-in, refund and close flows are exactly what Tern's engine
holds. The till is driven by events (timer ticks, dings, the event stream),
which suits one task owning the state. Mirroring types the way Fob does
would cost thousands of lines here.

### Crates

| Crate | Holds | Ported from |
| --- | --- | --- |
| `orf-codec` | unchanged | — |
| `monokulo-client` | Monokulo HTTP API, connect codes, redeem handshake, proof of work, SSE stream, problem messages, Tor routing policy | `android/core` |
| `till-pos` | `SaleSource` trait, PKCE, Square and Clover (auth, API, source, receipts), each behind a cargo feature | `android/sales` (POS parts) |
| `till-sales` | `Till`, `TillSale`, `Standing`, `Shift` (wake-up loop), `TillRunner`, `LiveStore`, `PosSwitch` | `android/sales`, `android/app` |
| `till-bell` | Bell address, Bell API, the `BellWakeUps` registration machine, which push payloads count as dings | `android/push` |
| `till-screens` | what each screen shows: one pure function per screen, from the core's records and the clock to finished words and rows (Screens, below), and the logic-driven copy | `android/app` copy and screen logic |
| `till-app` | the `Engine`: state, `Action`/`Update`/`Effect`, screen graph, persisted model and migrations; publishes `till-screens`' records | `android/app` logic |
| `till-ffi` | cdylib and staticlib, about ten lines: turns on `till-app`'s `uniffi` feature and re-exports its scaffolding, plus `orf-codec`'s | replaces `orf-codec-ffi` |

The leaf crates (`monokulo-client`, `till-pos`, `till-sales`, `till-bell`,
`till-screens`) never depend on
UniFFI. CI checks this (§7).

### The FFI surface

```text
Engine::new(config: EngineConfig, platform: Arc<dyn Platform>, listener: Arc<dyn EngineListener>)
Engine::send(action: Action)                   // fire and forget; never blocks the UI thread
Engine::background_check(deadline_ms) -> Check // blocking; for the iOS Notification Service Extension and Android's ding path

trait EngineListener { on_update(Update); on_effect(Effect); on_log(Log) }
trait Platform {
    secret_get(key) -> Result<Option<Vec<u8>>, SecretError>  // a failure is an error, never "none" (Tern's lesson)
    secret_put(key, value) -> Result<(), SecretError>
    secret_remove(key) -> Result<(), SecretError>
    format_money(minor: i64, currency: String) -> String   // the device's locale (design.md §6)
}
```

- **Networking happens in Rust**: reqwest with rustls and
  `rustls-platform-verifier`, so certificate authorities a store's owner
  installed on the device still work on both platforms. This avoids Tern's
  `SSL_CERT_DIR` workaround. The event stream uses
  `reqwest-eventsource` or a small SSE parser, and Tor through Orbot uses
  reqwest's SOCKS5 proxy. The Orbot liveness probe stays in Kotlin, and iOS
  has no Tor (§11).
- **Time** is a `Clock` trait inside the core, faked in tests. It doesn't
  cross the FFI.
- **Money is formatted by the platform** through one synchronous callback,
  so `$24.50` follows the device's locale exactly as it does today
  (decision 34) without bundling ICU data. Copy that contains an amount gets
  the formatted string from `format_money`.
- **Storage.** The persisted model (connection, POS tokens, cursors,
  tenders, shift, wake-ups, pending sign-in) is serialised in Rust and kept
  through `secret_*`: the Android Keystore today (`SecureStore`), and the
  Keychain in a shared access group on iOS, so the extension can read it.
  There is no SQLite: the data is small, as it is today.
- **Errors** follow Fob and decision 12: one `uniffi::Error` per surface,
  with a `reason` field, never `message`.
- **Panics** are caught at the boundary and sent to `on_log`.

### Screens

Every screen on the canvas has a Rust implementation in its own crate,
`till-screens`, one module per screen, as Fob's `fob-screens` does beside
`fob-app`. The crate is pure: no tasks, no I/O, and UniFFI only behind its
`uniffi` feature, which `till-ffi` turns on (through `till-app`) so the
records cross to Kotlin and Swift without a mirror layer. A screen module
is a pure function from the engine's state (and the clock)
to a view record: every word, amount, icon name, colour role and enabled
action already decided, so Compose and SwiftUI only lay it out and send
back the screen's `Action`s. A screen's tests are host tests against that
function, word for word.

| Module | View record | Boards (iOS) | Replaces (Android) |
| --- | --- | --- | --- |
| `shift` | rows with standing, tone, line, target; filters and their counts; summary | PhoneShift, PadShift | `ShiftList.kt`, parts of `SaleText.kt` |
| `sale` | amount, XMR, code, status line, countdown, order card, buttons | PhoneSale, PadSale, PadFront | `SaleText.kt`, `ui/OrderPage.kt`'s words |
| `close` | the question, the three answers and their lines | PhoneClose, PadClose | `SalePage.kt`'s `Closing` |
| `sale_page` | heading, icon, note, order, timeline, actions, per standing | Det*, PadDetail | `SalePage.kt` |
| `code` | the enlarged code and its line | PhoneCode, PadCode | `ui/Code*`'s words |
| `onboarding` | each step's state and words: connect, confirm, point of sale, sign-in, test sale, alerts | Onb*, PadOnb*, PhonePos | `ui/Onboarding.kt`'s logic, `PosText.kt` |
| `settings` | store, point of sale, alerts, wake-ups, island rows | PhoneSettings, PadSettings | `ui/Settings.kt`'s logic, `PosText.kt` |
| `wake_ups` | the choice, status and explanations | PhoneWakeUps | `WakeUpsText.kt` |
| `live_activity` | the content state (design.md §6) | Main, PhoneOrder, PhoneOrderQR | — (iOS only) |
| `notification` | alert and ongoing notification faces | PadTucked, Android's shade | `notify/SaleNotifications.kt`'s `Face`, `Ongoing` |

The engine publishes each screen's record as its own `Update` variant,
only when it changed (push, as Tern does, rather than Fob's pull), so a
screen draws whole on its first frame and redraws only on change. The
platform keeps only what is its own: layout, motion, gestures, the
keyboard while typing.

### Copy

Copy chosen by logic moves to Rust (`SaleText`, `PosText`, `PosWords`,
`WakeUpsText`, `Problems`, `ShiftList` labels, `SalePage` notes, the
notification faces). Its tests, which check it word for word against the
acceptance criteria, move with it. Copy written once inside a screen
(headings, onboarding paragraphs) stays in that screen. There is no
localisation today. If it comes, it will be a Fluent catalogue in the core.

---

## 3. Phases: the Rust core, with Android shipping throughout

Phases 1 to 4 each port one area into its crate, in the same order:

1. Port the Kotlin unit tests to Rust first, reusing the JSON fixtures.
2. Port the code until those tests pass.

Android keeps using its Kotlin modules until phase 5, which moves it onto
the engine and deletes the Kotlin modules and their tests
([decisions/core.md](decisions/core.md) 2: the modules call one another
directly, so switching one at a time would mean exporting every crate's
types to Kotlin). `./gradlew build`, and the on-device tests in
`app/src/androidTest`, must stay green at every step. Each phase is its own
PR to `main`. Phases 1, 2 and 4 are independent of one another; 3 needs 1
and 2; 5a's screens each need the phases their state comes from; 5b needs
them all.

| Phase | Work | Test oracle (Kotlin tests to port) |
| --- | --- | --- |
| **0. Scaffold** (built) | Workspace crates; the `till-ffi` library replaces `orf-codec-ffi`'s (Android loads one `.so`), still built by the `receipt` module; `Platform` and `EngineListener` with a stub engine; `PointOfSale` and `Clock` in `monokulo-client`; CI guards (§7); decisions recorded and design.md §9 updated | ReceiptCodeTest |
| **1. Monokulo client** | `monokulo-client`: models, `ConnectCode`, `MonokuloApi`, SSE, `Proof`, `Problems`, Tor policy, `Amounts` (through `format_money`). The OpenAPI contract test moves to Rust, still against the pinned `monokulo.sha` | MonokuloApiTest (21), MonokuloContractTest (11), AmountsTest (9), ConnectionTest (6), SaleEventTest (5), ConnectCodeTest (4), TorTest (3), PointOfSaleTest (2) |
| **2. Points of sale** | `till-pos`: Square and Clover auth, API, sources and receipts. HTTP tests use `wiremock` in place of MockWebServer | SquareTest (12), CloverSourceTest (17), CloverAuthTest (8), CloverReceiptsTest (7) |
| **3. The till** | `till-sales`: `Till` and `Standing`, `Shift`, `TillRunner`, `LiveStore`, `PosSwitch`. Android's `TillService` keeps the foreground service and calls the engine | TillTest (43), ShiftTest (4), LiveStoreTest (3), PosSwitchTest (5), NoSourceTest (1) |
| **4. Wake-ups** | `till-bell`: `BellWakeUps`, `BellApi`, `BellAddress`, push events. UnifiedPush's receiver and `WakePushService` stay in Kotlin and forward to the engine | BellWakeUpsTest (59), BellApiTest (14), BellAddressTest (8), PushEventsTest (5) |
| **5a. Screens** | The `till-screens` crate (§2, Screens): one pure view function per screen, with the copy objects. `wake_ups` and `onboarding` can start once phases 1, 2 and 4 land; the till's screens need phase 3 | ShiftListTest (29), SaleTextTest (14), WakeUpsTextTest (10), PosTextTest (6) |
| **5b. The engine** | Android moves onto the engine: the Gradle tasks move from `receipt` to a `:ffi` module, and the Kotlin `core`, `sales`, `push` and `receipt` modules are deleted. `till-app`: the screen graph and flows from `AppViewModel`, the persisted model (`Stores.kt`, read in its current JSON so existing tills keep their connection), and publishing 5a's screens as `Update`s. `AppViewModel` shrinks to a `StateFlow` per `Update`, as Tern's `Store` does | NavTest's move rules (8), and the on-device tests unchanged |

**Migration of saved state.** Before phase 5 lands, a test fixture records
every key `Stores.kt` writes, captured from the current Kotlin code. The
Rust model must read those exact values. An upgraded till must not ask the
cashier to connect again.

**Size.** The Android library is about 1.2 to 1.6 MB per ABI today
(decision 11). With reqwest, rustls and tokio it will grow. Measure it at
phase 1, then decide whether to drop `armeabi-v7a`.

After phase 5, Android is a thin Compose and services layer over the core,
the same as Tern for Android.

---

## 4. The design canvases

**Primary**: [Till — Live Activity & Slide Over](https://claude.ai/artifact/Kb9AtJFBF3ockyddomLc6t)
(2026-10-19). The iOS boards:

| Page | Boards |
| --- | --- |
| Sale flow, iPhone | Main (1 · idle, logo in the island), PhoneOrder (2 · island expands on a sale), PhoneOrderQR (2 · variant: code in the island), PhoneFront (3 · full-screen code), PhonePaid (4), PhoneUnpaid (5 · never paid, refund in Square) |
| Sale flow, iPad | PadIdle (1 · in Slide Over), PadStashed (1b · swiped off the edge), PadTucked (2a · time-sensitive notification), PadOrder (2b · code in the panel), PadFront (3 · full screen for the customer, with items and a four-step progress bar), PadPaid (4) |
| Onboarding | iPhone: OnbWelcome, OnbStore, OnbConfirm, OnbSquare, OnbAlerts, OnbReady (6 steps). iPad: PadOnb1–6 with a PadOnbRail step rail (step 6: put Till in Slide Over) |
| Settings | PhoneSettings (Store, Square, Alerts, Dynamic Island, Disconnect), PadSettings (in Slide Over) |
| Shared pieces | Logo, LoadingMark (animated), QR |

The look, taken from the boards:

- **Colours**: ground `#f4f1ea`, ink `#1a1917`, secondary `#5a564c`,
  accent `#ff6600` (text on it `#111111`), link `#b34700`, paid green
  `#1a7f37`, danger `#b00020`, hairline `#ece6da`, border `#dcd5c7`, and
  expired `#f1e3cf` / `#7d5e00`.
- **Type**: Manrope 800 for amounts and display headings; SF for
  everything else.
- **Controls**: native navigation with glass buttons, 44 pt targets, and
  inset grouped lists for settings.

**Later canvases**, mostly drawn for Android, that change the product the
iOS app must match: [Till till tweaks](https://claude.ai/artifact/7AP51MTdKr7TXgRjDYArNN)
(the till redesign, decisions 60 and on), [Till on Clover](https://claude.ai/artifact/Nd5BPGhNqqpqbeQtZe5icf),
[Till waiting sales](https://claude.ai/artifact/EsWkkS3yER5ryB8tgvRGJe),
[Waiting Tray & Bubble Handoff](https://claude.ai/artifact/WiZwopcdX7qrCYaAuknYdb),
[Till Design Choices](https://claude.ai/artifact/MnkQZHnEy3Sa6hRxZUpJXn)
and [Till Motion](https://claude.ai/artifact/E9kQMmhYVnsBE4jvfhAKi6).

**The till redesign on iOS** (added 2026-11-08, the canvas's "Till
(redesign)" page). The canvas first predated the till redesign, so a design
round added: PadShift, PadSale, PadClose, PadDetail and PadCode (iPad, in
Slide Over); PhoneShift, PhoneSale, PhoneClose, PhoneCode, PhonePos
(onboarding step 4: Square or Clover) and PhoneWakeUps (iPhone); and a page
per standing (DetPaid, DetLatePaid, DetClosed, DetExpired, DetReversed,
DetRefunded, DetOtherTill). They are built from three shared parts,
ShiftList, SaleBody and SaleDetail. Their words are Android's
(`SalePage.kt`, `PosText.kt`, `WakeUpsText.kt`). These boards replace
PadIdle, PadOrder, PhoneFront and PhoneUnpaid where they differ.

The design round found two iOS limits:

- **No "Back to Square".** iOS apps can only open other apps through links
  those apps support, and Square doesn't offer a plain "open the app" link.
  The sale offers "Copy address" instead; the cashier swipes back to
  Square.
- **"Show the customer" from Slide Over** probably opens the full-screen
  code (PadFront) as a second window, because an app in Slide Over may not
  be able to make itself full screen. Check this on a real iPad in I0.

An "Older iPadOS" page (classic Slide Over, and a full-screen till that
the cashier switches to) was drawn and then removed when iOS 27 became the
minimum (§5).

---

## 5. The iOS app

### Targets and layout

```text
ios/
  project.yml                XcodeGen; no .xcodeproj in git
  Scripts/build-core.sh      cargo per Apple target, then the XCFramework, then Swift bindings
  Packages/TillCore/    SwiftPM: binaryTarget (the XCFramework) + generated Swift + small Swift helpers
  Till/                 the app (SwiftUI): App, Features/{Onboarding,Till,Sale,Settings}, Services, Resources
  TillWidgets/          Live Activity: compact, minimal, expanded, lock screen. Doesn't link the core
  TillNotifications/    Notification Service Extension: calls Engine::background_check
  TillTests/            Swift tests of the Swift glue only (the logic is tested in Rust)
  TillUITests/          snapshot tests of every board, for CI screenshots
```

- **Minimum version: iOS and iPadOS 27** (owner's decision, 2026-11-08: a
  cutting-edge product). There are no `if #available` branches and no
  fallbacks for older versions: Liquid Glass, Slide Over through the window
  menu, and the current ActivityKit and SwiftUI APIs are used directly.
  The project needs Xcode 27. Before I2, check two things on iPadOS 27:
  which devices it supports, and the exact steps to put an app in Slide
  Over, for onboarding step 6. Also check that Square Point of Sale runs
  on iOS 27.
- **Rust targets**: `aarch64-apple-ios` and `aarch64-apple-ios-sim`. Drop
  x86_64 simulators; the runners are Apple silicon.
- The core is built as a **static library** inside the XCFramework, and is
  linked into the app and the Notification Service Extension. The widget
  extension does not link it: Live Activity code has no network, and its
  content state is a small Swift `Codable` that the engine's view records
  fill in. If linking the library twice costs too much space, switch to one
  dynamic framework embedded in the app.
- **The extension's memory limit is about 24 MB.** `background_check` runs
  on a current-thread tokio runtime, with no engine or listener. Measure it
  in phase I5.
- **Entitlements**: App Groups and a shared Keychain access group (app plus
  extension), Push Notifications, Time Sensitive Notifications, and
  Associated Domains (`applinks:till.monokulo.com` for connect links
  and the Square redirect).
- **Square sign-in** uses `ASWebAuthenticationSession` with PKCE. The core
  builds the URL and exchanges the code; Swift only opens the browser.
- **The QR scanner** (PadOnb2) uses AVFoundation's metadata output, with no
  extra library. QR codes are drawn with Core Image's
  `CIQRCodeGenerator`.

### Square tokens shared by the app and the extension

Square refresh tokens can be single-use (design.md §11, still open). If the
app and the Notification Service Extension refresh at the same moment, one
of them loses the session. The core takes an exclusive `flock` on a file in
the App Group container around every refresh, and reads the token again
after acquiring the lock. This is added in phase 2 (behind a `TokenLock`
trait, with a no-op version on Android) so iOS doesn't need its own fix.

### Phases

| Phase | Work | Boards | Needs |
| --- | --- | --- | --- |
| **I0. Skeleton** (written, checked by CI) | XcodeGen project, `build-core.sh`, the `TillCore` package, an app that starts the engine and shows a raw `Update`; CI job `ios.yml` (§7) | — | Core phase 0 |
| **I1. Look and parts** (written, checked by CI) | Colour and type tokens, Manrope bundled, Logo, the LoadingMark animation (as specified in docs/motion.md), QR view, glass buttons | Logo, LoadingMark, QR | — |
| **I2. Onboarding** (written, checked by CI) | Six steps on iPhone, full screen with the rail on iPad; connect by scanning or by universal link; Square sign-in; the test sale; alerts permission | OnbWelcome…OnbReady with PhonePos as step 4, PadOnb1–6, PadOnbRail | Core phases 1–2; the AASA file (§7) |
| **I3. iPad till, timer only** (written, checked by CI) | The shift in Slide Over as home; stashed; a sale while tucked away raises a time-sensitive local notification; the sale, close sheet, sale pages and enlarged code in the panel; full screen for the customer; paid; settings with Point of sale and Wake-ups | PadShift, PadSale, PadClose, PadDetail, PadCode, PadStashed, PadTucked, PadFront, PadPaid, PadSettings | Core phases 3 and 5 |
| **I4. iPhone and the Live Activity** | The shift, sale, close sheet, sale pages and enlarged code on iPhone; the Live Activity started each shift (idle, waiting, seen, confirming, paid, expired, as in design.md §6); full-screen code; unpaid with "Refund the Square sale"; Settings › Dynamic Island, including "Show code in island" (off by default) | Main, PhoneOrder, PhoneOrderQR, PhoneShift, PhoneSale, PhoneClose, PhoneCode, Det*, PhonePaid, PhoneSettings, PhoneWakeUps | I3 |
| **I5. Wake-ups over APNs** | Register the Live Activity's push token and the APNs device token with Monokulo push targets and the Bell, through the relay; the Notification Service Extension handles dings with `background_check` and rewrites the alert as "Monero sale · $24.50 · Tap to show code" | — | Core phase 4; a deployed relay and Bell with APNs keys (design.md §10 item 6) |
| **I6. Clover on iOS** (written, checked by CI) | The choice of point of sale in onboarding and Settings, and the Clover refund guide. The logic is already in the core; this phase is screens only | Till on Clover canvas | I3 |
| **I7. Release** | TestFlight from CI; App Store metadata; review notes for a crypto-payments companion (design.md §11) | — | Apple Developer account |

iPad comes before iPhone for the same reason as on Android (design.md §10):
iPad works with a timer alone, while iPhone without the Bell only finds a
sale when the cashier opens the app.

---

## 6. Parity between Android and iOS

- One Rust test suite covers all behaviour. A behaviour change is made once,
  in the core, with its test.
- `protocol/` (design.md §9) is still worth creating: `vectors/` from
  Monokulo's xtask, and `square/` with recorded responses from the Square
  spike. The Rust tests read them, so Swift doesn't need to.
- Screens are checked against the boards with snapshot tests on iOS
  (`swift-snapshot-testing`, iPhone 17 and iPad Air 11-inch), next to the
  Android screenshots in `docs/screenshots/`.

---

## 7. CI

All workflows keep the current conventions: `permissions: contents: read`,
path filters, `concurrency` with `cancel-in-progress`, and pinned action
majors.

### `rust.yml` (ubuntu-latest), extended

- `cargo fmt --check`
- `cargo clippy --locked --workspace --all-targets -- -D warnings` (as today)
- `cargo test --locked --workspace --exclude receipt-qr` (receipt-qr needs a
  downloaded corpus to run, as today)
- **Guard**: `cargo tree -p till -p pos -p bell -p monokulo-client -e normal`
  must not contain `uniffi`. Tern's `ci.yml` has the same check.
- **The contract test** gets Monokulo's OpenAPI document at the pinned sha,
  with the same cache as `android.yml`. It moves here from the Android job
  in phase 1.
- Optionally, `cargo check -p till-ffi --target aarch64-apple-ios` on
  Linux. This catches crates that won't compile for iOS before anyone needs
  a Mac runner. `cargo check` doesn't link, so it needs no Apple SDK, but
  crates with C code (ring, for example) may still need one; if they do,
  this check moves to `ios.yml`.

### `android.yml` (ubuntu-latest), as today

Unchanged except the module that builds the Rust library. Its path filter
already includes `crates/**`.

### `ios.yml` (the `xcode-27` runner, new)

macOS minutes cost about ten times Linux minutes, so this job runs only on
`ios/**`, `crates/**`, `Cargo.*` and its own file.

1. `actions/checkout`; select Xcode 27. It runs on GitHub's `xcode-27`
   image (macOS 27, arm64), since `macos-26`'s newest Xcode is 26.6.
2. `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`; cache
   `~/.cargo` and `target` keyed on `Cargo.lock`.
3. `ios/Scripts/build-core.sh --sim` builds only the simulator slice for
   PRs, plus Swift bindings from `uniffi-bindgen generate --language swift`.
4. `brew install xcodegen` (pinned), then `xcodegen generate`.
5. `xcodebuild test` on the iPhone and iPad simulators, running
   `TillTests` and `TillUITests`.
6. Upload the snapshot images and `.xcresult` as artifacts (14 days), and an
   unsigned simulator `.app`.

The bindings are generated in CI, never committed, the same as on Android
(decision 8).

### `release-ios.yml` (the `xcode-27` runner; on `v*` tags and manual runs; written)

- Builds the full XCFramework (device and simulator slices).
- Signs automatically with Xcode's cloud-managed distribution certificate
  through an App Store Connect API key (`ASC_KEY_ID`, `ASC_ISSUER_ID`,
  `ASC_KEY_P8`, and the `APPLE_TEAM_ID` variable), so no p12 is kept.
- Runs `xcodebuild archive`, exports once to check the signed entitlements,
  then `xcodebuild -exportArchive` with `destination: upload`. This uploads
  to TestFlight without fastlane.
- Uses the GitHub environment `testflight`, with required reviewers. The
  owner's one-time steps are in [release.md](release.md).

### `pages.yml`, small addition

- Serve `site/.well-known/apple-app-site-association` (the app's paths for
  connect links and the Square redirect), with `include-hidden-files` so
  the dot directory is published, and its team filled in from the
  `APPLE_TEAM_ID` variable as it deploys (written).
- Android's `assetlinks.json` can live beside it.

---

## 8. Order of work

```text
core 0 ── core 1 ── core 2 ── core 3 ── core 4 ── core 5
   │                   │         │                  │
   I0 ── I1 ─────────── I2       │                  │
                                 └──── (§4 boards) ─┴── I3 ── I4 ── I5 (needs relay + Bell deployed)
                                                          └── I6
                                                                         I7 when I4 is accepted
```

- I0 and I1 can start as soon as core phase 0 lands, so the Mac toolchain
  and the look are settled early.
- Onboarding (I2) needs only the client and the points of sale.
- The till screens (I3) wait for the till and the engine.

---

## 9. Risks

- **The Square spike is still open** (design.md §10 item 1): how Other
  tenders appear, refunds, PKCE redirects to a universal link, and refresh
  token rules. These affect both apps equally, and are cheaper to settle
  before core phase 2 hardens the Square source.
- **Parity regressions during the port.** Mitigated by porting tests before
  code, and by keeping the on-device Android suite as the end-to-end check
  on every phase.
- **Library size** on Android (§3), and memory in the Notification Service
  Extension (§5).
- **App Store review** of a crypto-payments companion.
- **macOS runner cost**, kept down by path filters and simulator-only
  builds on PRs.

## 10. Owner decisions needed

1. ~~Accept the Tern-style engine over Fob's object API.~~ Accepted
   2026-11-08.
2. ~~Logic-driven copy in Rust, or per platform.~~ In Rust, accepted
   2026-11-08.
3. ~~A design round for the §4 gap boards before I3.~~ Done and accepted:
   the canvas's "Till (redesign)" page (§4).
4. ~~iOS 26.1 as the minimum.~~ Decided: iOS and iPadOS 27 (§5).
5. The Apple Developer team, the bundle identifier (decided: `com.monokulo.till`, as Android's
   application ID), and the App Store Connect API key for CI.
6. Whether iOS should ship without the Bell at first. iPad works on the
   timer alone; iPhone finds sales only when the app is open until I5.

## 11. Not in scope

- **Tor on iOS.** There's no Orbot. Embedding Arti is possible later, in
  the core.
- **FCM.** It is unchanged by this plan (design.md §7).
- **Push-to-start for the Live Activity.** design.md §6 explains why there
  is none.
