# Monokular

The companion app to [Monokulo](https://github.com/OceanEilonwy/monokulo) that
helps you take Monero payments at the till using your existing POS apps.

When the cashier takes a Monero sale in their point of sale (Square first),
Monokular shows the customer the payment code and confirms when the money
arrives: in the Dynamic Island on iPhone, in Slide Over on iPad, and in a
floating bubble or Live Update notification on Android.

Monokulo is self-hosted, so Monokular always connects from a store's own
Monokulo dashboard (Store settings › Connections › Apps), never to a central
server. The till holds the Square sign-in, and builds each receipt itself:
what someone bought never reaches Monokulo.

## Status

The Android app works end to end against Monokulo's connected-apps API
(OceanEilonwy/monokulo#140) and a stand-in Square, and follows the design
mockups: setup in six steps (connect from a store's code, choose Square or
Clover and sign in to it, alerts and the bubble), a floating bubble over
Square (or over whatever the phone shows beside a Clover device) that lights
up with a sale and opens its code over it, a Live Update notification with the
payment's progress, and the sale full screen in the app, with the receipt
in the code, until it's paid or refunded. A Clover sale never paid is
refunded by the cashier in Clover's Orders app, which the till guides and
then sees (Clover's API can't refund it). With a Monokular Bell
([below](#running-a-bell)) chosen in Settings › Wake-ups, the point of sale
wakes the till through the Bell within a second or two of a sale and the till
checks it every 30 seconds instead of every 2; the dings come over UnifiedPush,
so the phone needs a distributor such as [ntfy](https://ntfy.sh/) (FCM
through the push relay isn't built). It hasn't met real Square or real
Clover yet (the Square spike, design doc §10; Clover's sandbox needs a dev
kit to ring up a sale), and iOS hasn't started. Choices made along
the way are in
[docs/decisions/android.md](docs/decisions/android.md); screenshots are in
[docs/screenshots/android](docs/screenshots/android).

- [Design document](docs/design.md): the sale flow with Square, connected
  apps, push (Web Push, UnifiedPush and a relay for APNs/FCM), the Live
  Activity, battery use, the Monokulo side and this repository's layout.
- [Design mockups](https://claude.ai/artifact/Kb9AtJFBF3ockyddomLc6t): iPhone,
  iPad and Android screens, onboarding, settings and the web connect pages.

## Building the Android app

You need JDK 21, the Android SDK with platform 37.2 and NDK r27, and Rust
with the Android targets and `cargo-ndk`: the receipt codec is the Rust crate
`orf-codec`, built into the app. Android Studio includes the JDK: it's at
`<android-studio>/jbr`.

```sh
cd android
echo "sdk.dir=$HOME/Android/Sdk" > local.properties    # once, not committed
sdkmanager "platforms;android-37.2" "ndk;27.0.12077973" # once, if missing
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo install cargo-ndk --version 4.1.2 --locked       # once
./gradlew build                                        # compile, lint, test, assemble
./gradlew :app:installDebug                            # onto a connected device
```

The Square application id is a build property:
`-Pmonokular.square.applicationId=sq0idp-…` (a placeholder otherwise). It's
public, so `~/.gradle/gradle.properties` is a good home for it; the
application's secret is never used (the till signs in with PKCE). Clover's
are `-Pmonokular.clover.applicationId=…` (the app's App ID) and
`-Pmonokular.clover.environment=` `north_america` (the default), `europe`,
`latin_america` or `sandbox`; Clover's app secret is never used either (its
PKCE flow for low-trust apps needs none).

### Signing a debug build in to Square's sandbox

```properties
# ~/.gradle/gradle.properties
monokular.square.applicationId=sandbox-sq0idb-…
monokular.square.sandbox=true
```

In the Developer Console, on the application's **OAuth** page for
**Sandbox**, set the redirect URL to
`https://monokular.monokulo.com/square/debug/` (trailing slash and
all). Square only sends a sign-in back to https; that page
([site/square/debug/](site/square/debug/index.html)) hands it to the debug
build's `monokular-debug://square`. `-Pmonokular.square.redirectUri=…` names
another such page. Square's sandbox sign-in only works with the test
seller's **Sandbox Square Dashboard** already open in the same browser:
open it in Chrome on the phone (from the console's *Sandbox test accounts*)
before tapping "Sign in to Square". The sandbox has no Point of Sale app, so
it tests the sign-in and the API, not a real "Other: Monero" tender.

Ring up sandbox sales as the till would find them (an order paid by an
`EXTERNAL` "Monero" payment) on a local page,
[tools/square-sandbox](tools/square-sandbox/src/main.rs), using the
Developer Console's sandbox access token for the same test seller the till
signed in as. It lists the location's latest payments and refunds them too.

```sh
cargo run -p square-sandbox   # http://127.0.0.1:8739, then enter the token on the page
```

### Signing a debug build in to Clover's sandbox

```properties
# ~/.gradle/gradle.properties
monokular.clover.applicationId=…        # the sandbox app's App ID
monokular.clover.environment=sandbox
```

In the sandbox developer dashboard (sandbox.dev.clover.com), give the app
platform **Web** with the **Site URL** `https://monokular.monokulo.com/clover`
and the default OAuth response **Code**. Clover sends a sign-in only to https
under the Site URL, so a debug build asks for `/clover/debug/`; that page
([site/clover/debug/](site/clover/debug/index.html)) hands it to the debug
build's `monokular-debug://clover`, as Square's does. Request the
permissions Payments and Orders read and Merchant read and write (write only
to add the Monero payment type; without it the till asks the cashier to add one called
Monero in Clover and **Look again**). The sandbox signs in and answers the
API, but ringing up a Monero sale needs Clover Register on a dev kit or
Clover's emulator image.

| Module | What it is |
| --- | --- |
| `android/core` | Pure Kotlin, no Android: connect codes, the Monokulo client, sale events. |
| `android/receipt` | ORF receipts in the payment code, through `crates/orf-codec-ffi` (UniFFI), inside the one native library `crates/monokular-ffi` builds. |
| `android/sales` | Square and Clover on the till (PKCE sign-in, Monero tenders, orders, refunds) and the till's sales. |
| `android/push` | Wake-ups: the Bell's API, the till's registration with it, and its dings over UnifiedPush. |
| `android/app` | The Compose app, the shift's foreground service, its notifications and the bubble over other apps. |

The Android modules above are moving into a Rust core shared with the
coming iOS app ([docs/ios-plan.md](docs/ios-plan.md)): `crates/monokulo-client`,
`crates/pos`, `crates/till`, `crates/bell` and the engine,
`crates/monokular-app`. Android keeps its Kotlin modules until the engine
can drive it ([docs/decisions/core.md](docs/decisions/core.md) 2).

### Running it end to end on an emulator

A debug build talks to a Monokulo on your machine and a stand-in Square or
Clover. Both stand-ins are in the
[monokulo repository](https://github.com/OceanEilonwy/monokulo) (`tools/fake-square`
and `tools/fake-clover`, beside the Bell), so the commands below marked
"monokulo" run in a checkout of it:

```sh
# monokulo: a Monokulo with an in-process engine, on :8737
cargo run -p monokulo --example apps_dev_server
# monokulo: a stand-in Square on :8738
python3 -I tools/fake-square/fake_square.py
# Here: the app, pointed at it
cd android && ./gradlew :app:installDebug -Pmonokular.square.baseUrl=http://10.0.2.2:8738

curl -X POST localhost:8737/__dev/code    # a connect code: paste it in, or share it to the app
curl -X POST localhost:8738/fake/sale -d '{"items":[["Flat white",2,450]]}'   # ring up a sale
curl -X POST localhost:8737/__dev/orders/<order id>/paid                       # and pay it
```

For Clover, choose Clover at step 4 and run the stand-in Clover instead (it
plays Clover's sign-in and API on one host, and approves the sign-in at once
for The Bean Room, merchant `MCLOVERDEV001`):

```sh
python3 -I tools/fake-clover/fake_clover.py    # monokulo: :8740
cd android && ./gradlew :app:installDebug -Pmonokular.clover.baseUrl=http://10.0.2.2:8740

curl -X POST localhost:8740/fake/sale -d '{"items":[["Flat white, oat milk",2,450]],"title":"Table 4"}'
curl -X POST localhost:8740/fake/refund -d '{"payment_id":"<id>"}'   # the cashier's refund in Clover
curl -X POST localhost:8740/fake/expire-tokens                           # Clover ends the access token early
```

`FAKE_CLOVER_DENY_TENDER_WRITE=1` refuses the Monero payment type's `POST`,
so step 4 shows "Add a payment type called Monero in Clover" and **Look
again**; `FAKE_CLOVER_REFRESH_CAP` and `/fake/revoke` play Clover signing
the till out.

#### Wake-ups

To try a Bell too, run one on `127.0.0.1:8445` against the stand-ins, as the
[On one machine](https://docs.monokulo.com/bell/#on-one-machine) example of
the Bell's page does (keep its `MONOKULO_BELL_VAPID_KEY` between runs, as
[Run one](https://docs.monokulo.com/bell/#run-one) says), ring the
sales up through it, give the emulator a UnifiedPush distributor, and point
the build at the Bell:

```sh
# monokulo: the stand-in Square, sending each sale to the Bell as a signed webhook
FAKE_SQUARE_WEBHOOK_URL=http://127.0.0.1:8445/v1/square/webhook FAKE_SQUARE_WEBHOOK_KEY=dev \
  python3 -I tools/fake-square/fake_square.py
# ntfy's F-Droid build (no Google services) as the distributor, on the public ntfy.sh, from
# github.com/binwiederhier/ntfy-android/releases; its certificate's SHA-256 must be the one ntfy publishes
apksigner verify --print-certs ntfy-<version>-fdroid.apk | grep SHA-256
adb -s emulator-5554 install ntfy-<version>-fdroid.apk      # then open it once
# The app, for the Bell's application and the Bell
cd android && ./gradlew :app:installDebug -Pmonokular.square.applicationId=sandbox-sq0idb-dev \
  -Pmonokular.square.sandbox=true -Pmonokular.square.baseUrl=http://10.0.2.2:8738 \
  -Pmonokular.bell.url=http://10.0.2.2:8445
```

For Clover, give the Bell a `[bell.clover]` with `application_id =
"FAKECLOVERAPP"` and `api_url = "http://127.0.0.1:8740"` and
`MONOKULO_BELL_CLOVER_AUTH_CODE=dev`, run fake-clover with
`FAKE_CLOVER_WEBHOOK_URL=http://127.0.0.1:8445/v1/clover/webhook
FAKE_CLOVER_AUTH_CODE=dev`, and build with
`-Pmonokular.clover.applicationId=FAKECLOVERAPP
-Pmonokular.clover.baseUrl=http://10.0.2.2:8740 -Pmonokular.bell.url=http://10.0.2.2:8445`.

Then Settings › Wake-ups › Monokular Bell. The ding goes Bell → ntfy.sh →
ntfy on the emulator, so the emulator needs the internet; fake-square's log
shows the till's pace (`GET /v2/payments` every 30 s once connected, and one
within a second of each sale). `-Pmonokular.bell.url` takes what the
Settings field takes: a release build only an https address.

Versions live in `android/gradle/libs.versions.toml`; each was released at
least two weeks before it was added. The same two-week rule holds for the
Rust crates, with one exception: when a version in use is yanked or has a
RUSTSEC (or, for Android libraries, a published security) advisory, it moves
at once to the fixed release, after its changelog is read, and anything that
release pulls in is held at the oldest version it accepts.

## Running a Bell

The Monokular Bell rings the tills at a point of sale's location within a
second or two of a payment there, instead of their waiting for their next
look: the point of sale's payment webhook (Square's or Clover's) comes to the
Bell, and the Bell sends each registered till a Web Push "ding". It lives in
the [monokulo repository](https://github.com/OceanEilonwy/monokulo) as the
crate and binary `monokulo-bell` (it moved there from this repository's
`crates/monokular-bell` on 2026-10-29, its history kept, and its secrets are
now `MONOKULO_BELL_*`; the old `MONOKULAR_BELL_*` names are still read for
one release, with a warning). Its API and how to run one, on a desktop or a
router, are on the docs site at <https://docs.monokulo.com/bell/>; its
design note and the choices behind it are monokulo's
[docs/bell.md](https://github.com/OceanEilonwy/monokulo/blob/main/docs/bell.md)
and
[docs/decisions/bell.md](https://github.com/OceanEilonwy/monokulo/blob/main/docs/decisions/bell.md).
The till's side of it (Settings › Wake-ups, `android/push`) stays here.

## License

GPL-3.0. See [LICENSE](LICENSE).
