# Working on Till

How to build Till, sign it in to Square's and Clover's sandboxes, and run it end to end against a Monokulo on
your machine. The iOS app's own steps are in [ios/README.md](../ios/README.md) and [docs/ios-plan.md](ios-plan.md),
and releasing it is in [docs/release.md](release.md).

## Building the Android app

You need JDK 21, the Android SDK with platform 37.2 and NDK r27, and Rust
with the Android targets and `cargo-ndk`: the app's core is the Rust crate
`till-ffi`, built into the app. Android Studio includes the JDK: it's at
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
`-Ptill.square.applicationId=sq0idp-…` (a placeholder otherwise). It's
public, so `~/.gradle/gradle.properties` is a good home for it; the
application's secret is never used (the till signs in with PKCE). Clover's
are `-Ptill.clover.applicationId=…` (the app's App ID) and
`-Ptill.clover.environment=` `north_america` (the default), `europe`,
`latin_america` or `sandbox`; Clover's app secret is never used either (its
PKCE flow for low-trust apps needs none).

A release build signs in through the App Link host `till.monokulo.com`, so
the Square application's OAuth redirect URL is
`https://till.monokulo.com/square` and the Clover app's Site URL is
`https://till.monokulo.com/clover`; a debug build comes back through
`https://till.monokulo.com/square/debug/` and `/clover/debug/` (below), which
open its `monokulo-till-debug://` scheme. The site those pages are on is
[site/](../site/), published by GitHub Pages at `till.monokulo.com`.

### Signing a debug build in to Square's sandbox

```properties
# ~/.gradle/gradle.properties
till.square.applicationId=sandbox-sq0idb-…
till.square.sandbox=true
```

In the Developer Console, on the application's **OAuth** page for
**Sandbox**, set the redirect URL to
`https://till.monokulo.com/square/debug/` (trailing slash and
all). Square only sends a sign-in back to https; that page
([site/square/debug/](../site/square/debug/index.html)) hands it to the debug
build's `monokulo-till-debug://square`. `-Ptill.square.redirectUri=…` names
another such page. Square's sandbox sign-in only works with the test
seller's **Sandbox Square Dashboard** already open in the same browser:
open it in Chrome on the phone (from the console's *Sandbox test accounts*)
before tapping "Sign in to Square". The sandbox has no Point of Sale app, so
it tests the sign-in and the API, not a real "Other: Monero" tender.

Ring up sandbox sales as the till would find them (an order paid by an
`EXTERNAL` "Monero" payment) on a local page,
[tools/square-sandbox](../tools/square-sandbox/src/main.rs), using the
Developer Console's sandbox access token for the same test seller the till
signed in as. It lists the location's latest payments and refunds them too.

```sh
cargo run -p square-sandbox   # http://127.0.0.1:8739, then enter the token on the page
```

### Signing a debug build in to Clover's sandbox

```properties
# ~/.gradle/gradle.properties
till.clover.applicationId=…        # the sandbox app's App ID
till.clover.environment=sandbox
```

In the sandbox developer dashboard (sandbox.dev.clover.com), give the app
platform **Web** with the **Site URL** `https://till.monokulo.com/clover`
and the default OAuth response **Code**. Clover sends a sign-in only to https
under the Site URL, so a debug build asks for `/clover/debug/`; that page
([site/clover/debug/](../site/clover/debug/index.html)) hands it to the debug
build's `monokulo-till-debug://clover`, as Square's does. Request the
permissions Payments and Orders read and Merchant read and write (write only
to add the Monero payment type; without it the till asks the cashier to add one called
Monero in Clover and **Look again**). The sandbox signs in and answers the
API, but ringing up a Monero sale needs Clover Register on a dev kit or
Clover's emulator image.

| Module | What it is |
| --- | --- |
| `android/ffi` | The Rust core: builds `crates/till-ffi`, the one native library, for each ABI, and its Kotlin bindings (UniFFI). |
| `android/app` | The Compose app, the shift's foreground service, its notifications and the bubble over other apps, each drawing what the engine publishes. |

Everything the till does lives in the Rust core it shares with the iOS app
([docs/ios-plan.md](ios-plan.md)): `crates/monokulo-client`,
`crates/till-pos`, `crates/till-sales`, `crates/till-bell`, the screens'
words in `crates/till-screens`, and the engine, `crates/till-app`, which
Android's app follows ([docs/decisions/core.md](decisions/core.md)).

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
cd android && ./gradlew :app:installDebug -Ptill.square.baseUrl=http://10.0.2.2:8738

curl -X POST localhost:8737/__dev/code    # a connect code: paste it in, or share it to the app
curl -X POST localhost:8738/fake/sale -d '{"items":[["Flat white",2,450]]}'   # ring up a sale
curl -X POST localhost:8737/__dev/orders/<order id>/paid                       # and pay it
```

For Clover, choose Clover at step 4 and run the stand-in Clover instead (it
plays Clover's sign-in and API on one host, and approves the sign-in at once
for The Bean Room, merchant `MCLOVERDEV001`):

```sh
python3 -I tools/fake-clover/fake_clover.py    # monokulo: :8740
cd android && ./gradlew :app:installDebug -Ptill.clover.baseUrl=http://10.0.2.2:8740

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
cd android && ./gradlew :app:installDebug -Ptill.square.applicationId=sandbox-sq0idb-dev \
  -Ptill.square.sandbox=true -Ptill.square.baseUrl=http://10.0.2.2:8738 \
  -Ptill.bell.url=http://10.0.2.2:8445
```

For Clover, give the Bell a `[bell.clover]` with `application_id =
"FAKECLOVERAPP"` and `api_url = "http://127.0.0.1:8740"` and
`MONOKULO_BELL_CLOVER_AUTH_CODE=dev`, run fake-clover with
`FAKE_CLOVER_WEBHOOK_URL=http://127.0.0.1:8445/v1/clover/webhook
FAKE_CLOVER_AUTH_CODE=dev`, and build with
`-Ptill.clover.applicationId=FAKECLOVERAPP
-Ptill.clover.baseUrl=http://10.0.2.2:8740 -Ptill.bell.url=http://10.0.2.2:8445`.

Then Settings › Wake-ups › Monokulo Bell. The ding goes Bell → ntfy.sh →
ntfy on the emulator, so the emulator needs the internet; fake-square's log
shows the till's pace (`GET /v2/payments` every 30 s once connected, and one
within a second of each sale). `-Ptill.bell.url` takes what the
Settings field takes: a release build only an https address.

Versions live in `android/gradle/libs.versions.toml`; each was released at
least two weeks before it was added. The same two-week rule holds for the
Rust crates, with one exception: when a version in use is yanked or has a
RUSTSEC (or, for Android libraries, a published security) advisory, it moves
at once to the fixed release, after its changelog is read, and anything that
release pulls in is held at the oldest version it accepts.

## Running a Bell

The Monokulo Bell rings the tills at a point of sale's location within a
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
The till's side of it (Settings › Wake-ups, `crates/till-bell`) stays here.

