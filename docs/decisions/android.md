# Decisions: the Android app, first build

Made while building the Android app and its server side overnight on
2026-10-22, without the owner to ask. Each says what was chosen, why, and
what else was possible, so any of them can be reversed knowingly.

## Server (monokulo, OceanEilonwy/monokulo#140)

1. **Apps connect at `POST /apps/connect`, not `/connect/{slug}`.** A store
   has one shop plugin and finds it by the shop's host; a till has neither.
   A separate path also means no plugin slug can collide with it.
2. **App keys ride the existing `/pay/{pk}/…` routes.** `store_key::check`
   accepts an `ak_` key for its own store, so app orders go through the
   plugins' order API unchanged, pass the embed policy and get the store's
   rate-limit budget. Alternative: separate `/apps/…` routes duplicating the
   order API.
3. **Device name: the merchant's wins, else the app's.** The dashboard asks
   for an optional name before showing the code; the app sends its own
   (the phone's model) for when the merchant left it blank.
4. **The event stream has no history.** Announcements are in memory (a
   broadcast channel fed by the order API and the order-event subscriber).
   An app re-reads the status of the orders it shows after `ready` or
   `lagged`. A durable log would need a table and retention for little gain.
5. **No push targets yet.** They come with the `push` crate; until then the
   app keeps the stream open while a shift runs (a foreground service).
6. **Disconnecting an app is one click.** It only stops that device, and
   its orders can still be paid, unlike a plugin, whose disconnect rotates
   the store's key.
7. **The code page can't be shown again.** The token is only ever held
   hashed, so after leaving the page the merchant makes a new code. A
   "Check it connected" page reads by the code's hash.

## App

8. **The receipt codec is built at build time from Rust.** The `receipt`
   module runs `cargo ndk` (arm64-v8a, armeabi-v7a, x86_64) and UniFFI's
   bindgen from Gradle, so the Kotlin bindings can never drift from the
   crate. The cost: building the app needs Rust and the NDK. Committing the
   generated Kotlin was the alternative.
9. **NDK r27 (27.0.12077973), the current LTS**, over r30 which is also
   installed here.
10. **`receipt` knows only ORF.** It turns a receipt into the code
    (`ReceiptCode.of`); turning a Square order into a receipt lives with the
    Square code in `sales`, so another POS adds its own mapping without
    touching the codec module.
11. **A `android` cargo profile** (LTO, one codegen unit, stripped) for the
    libraries the app ships: about 1.2 to 1.6 MB per ABI. Host release
    builds keep the defaults, because UniFFI's bindgen reads metadata from
    symbols LTO and stripping remove.
12. **`orf-codec-ffi`'s error field is `reason`**, not `message`, which
    clashes with Kotlin's `Throwable.message` in the generated bindings.
13. **A Square "Other: Monero" tender is read as an `EXTERNAL` payment whose
    `external_details.source` is the tender's name** (case-insensitive),
    status `COMPLETED`. Square's docs don't say how Point of Sale's custom
    Other tenders appear in the Payments API: the Square spike must check
    this first. If they don't appear as payments, `SquareSource` changes to
    searching orders' tenders; nothing else does.
14. **The idempotency key is `sq:` plus the first 40 hex digits of SHA-256
    of Square's payment id.** Every till finding the sale gets the same
    order, and Monokulo never learns Square's id.
15. **The payments cursor looks back 2 minutes** past the last handled sale
    (Square can list a payment late) and remembers the last 500 handled
    ids, so the overlap never makes a second order.
16. **A sale is handled once it has its Monokulo order**, not when it's
    found: a failure (Monokulo down) is retried on the next tick.
17. **Receipts from Square orders:** the time in the location's own time
    zone, cut to whole seconds (the codec refuses fractions); the unit price
    is the one that makes quantity × price the line's gross total when it
    exists in whole cents, else the list price; tips and discounts go in
    `extensions` (`tip`, `discount`); line breaks in names become spaces;
    a variation is shown unless it's Square's default "Regular".
18. **A till shows only its own sales when Square names the device**
    (`device_details.device_id` matching the till's chosen device);
    otherwise every Monokular at the location shows every sale.
19. **Square tokens are refreshed when less than a day is left.** PKCE
    refresh tokens are single-use, so the new one is saved before anything
    else; a refresh Square refuses (4xx) clears the sign-in and asks the
    cashier to sign in again.
20. **`Square-Version: 2026-09-16`**, the version Square's reference shows.
21. **Refunds use `monokular-refund-<payment id>` as their idempotency
    key**, so a retried refund refunds once.
22. **Connecting confirms the host first, then shows the store.** The design
    has the confirm screen show the store and wallet before connecting, but
    Monokulo only says which store a code is for when it's redeemed. So the
    app asks "Connect to pay.yourshop.com?", redeems, then shows store,
    server, wallet and network with "Wrong store: disconnect". A preview
    endpoint would need a code that can be read without being spent.
23. **The till runs in a `specialUse` foreground service** during a shift
    (Android 14+ needs a type; `dataSync` is capped at 6 hours a day on
    Android 15). Play needs the subtype explained; the manifest says it.
    Its notification is the current sale (`setRequestPromotedOngoing`, a
    Live Update on Android 16+).
24. **Secrets are AES-GCM encrypted with an Android Keystore key** and kept
    in app preferences, written with `commit` so a spent Square refresh
    token is never the one left on disk. No extra library (Tink,
    EncryptedSharedPreferences) is needed for this.
25. **The QR scanner is CameraX with ZXing**, not ML Kit, so the app has no
    Google Play services dependency and can go on F-Droid. Codes are drawn
    with ZXing's `QR_COMPACT`, which switches modes inside a code so a
    receipt's digits pack in numeric mode.
26. **Debug builds** (`app.monokular.debug`) take connect codes over plain
    http, return from Square on `monokular-debug://square`, and can point at
    a stand-in Square: for running against Monokulo on the developer's
    machine. Release builds take only https and the `monokular.monokulo.com` App
    Link.
27. **The Square application id is a build property**
    (`-Pmonokular.square.applicationId=…`), defaulting to a placeholder,
    because the project's Square application doesn't exist yet.
28. **Push isn't wired up yet.** Monokulo has no push targets and there's no
    Bell, so the app relies on its foreground service and the event stream
    during a shift. The UnifiedPush module from the first PR, which nothing
    called, was removed (decision 56).
29. **A till without Square** ("Not now") still runs: it shows the sales
    other tills make, from the event stream, without receipts.

## App: matching the design (2026-10-23)

Made while bringing the app to the design mockups (the Android, onboarding,
settings and shared-pieces boards), again without the owner to ask. Where
the mockups' notes are older than `docs/design.md` (Square webhooks to
Monokulo, the `monokular.app/connect#host=` link), the design document
wins; nothing here changes what goes over the wire.

30. **Colours are the mockups' own, by role** (`ui/Theme.kt`): ground
    `#f4f1ea`, ink `#1a1917`, muted `#5a564c`, cards white, accent
    `#ff6600` with `#111111` on it, link `#b34700`, outline `#8c8577`,
    paid `#1a7f37`. The mockups are light only; dark mode keeps the same
    roles on a warm dark ground (`#1c1b19`), each text pair at least 4.5:1.
    The QR card and the bubble keep their light colours in both: they sit
    over another app, or face a customer's camera.
31. **Manrope is bundled** as a font resource (its variable TTF from
    google/fonts, 165 KB, SIL OFL 1.1; the licence ships in
    `assets/licenses/`), used for display type only, Roboto for the rest,
    as in the mockups. No downloadable fonts, so no Google Play services
    and nothing fetched at run time.
32. **The logo and the loading mark are drawn in Compose from the web's
    SVG paths** (`ui/Mark.kt`), not a vector drawable, so the ring's colour
    and the idle bubble's greyscale are parameters. The loading mark's
    glints follow the web's keyframes (4.2 s, the core 0.7 s) and stand
    still when Android's animations are off (animator scale 0), as
    `prefers-reduced-motion` does on the web. The bubble's pulse does too.
33. **"Sale #1042" is Square's receipt number** (the order's
    `merchant_order_id`), with a `#` added when Square's has none; with no
    number it's "Monero sale". "Square · Front Counter" is this till's
    Square location; a sale another till found says "From" and that
    device's name, from the event.
34. **Amounts are formatted for the till's locale** ("$24.50"), and the
    XMR amount is shown exactly, not rounded to 4 places as in the mockups:
    a customer typing it by hand must send exactly that.
35. **The bubble is an overlay window in the till's service**
    (`overlay/`), shown during a shift when Android allows "display over
    other apps" and the cashier didn't choose notifications only, and
    hidden while Monokular itself is on screen. It's a Compose view in a
    `TYPE_APPLICATION_OVERLAY` window; touches are handled in screen
    coordinates (the window moves under the finger), a drag snaps to the
    nearer side with the amount on the inside, and the side and height are
    remembered. TalkBack hears one button named as in the mockups' aria
    labels ("Monokular, new sale, tap to show the code"), announced when it
    changes. design.md lists `overlay/` and `notifications/` as Gradle
    modules; they're packages in `app` for now, because they share the
    theme and live in the app's service.
36. **The panel over Square**: on a phone it fills the screen with the
    mockup's scrim (a tap on it minimises, as does the bubble docked above
    it); on a window 600 dp or wider it floats top right with no scrim,
    like AndTabFloat, so Square stays usable beside it. It shows the sale
    the bubble is about: the newest still waiting, or the one just paid
    while it shows as paid (5 s), then the next one waiting. Android 12+
    lets overlays cover neither the status bar nor the navigation bar, so
    the scrim stops short of them. The till tweaks (65, 66, 74, 75) take
    the scrim away and give the panel its grabber and its order page.
37. **The notification follows the mockups**: a progress style with four
    segments (Waiting, Seen, Confirming, Paid), "Show code" and "Full
    screen" while waiting (AndSale), "Show code" and "Copy address" once
    seen (AndShade), the code as its large icon, the code's countdown in
    its header, and the amount as the Live Update chip's text. Android
    draws its own chip and can't put labels under the segments; the text
    says where the payment is instead. It's private on the lock screen,
    where a public version shows only the amount. A heads-up comes once
    per sale, never again when an older sale comes back to the top. The
    chip's text is gone since 68, and 78 has the notification's words now.
38. **"Show code" goes through a tiny activity** (`ShowCodeActivity`)
    that opens the panel or the app and finishes: a service action would
    leave the notification shade open over the panel, and starting the
    app from a service is a notification trampoline Android 12+ blocks.
39. **Paid and "No payment yet" are notifications of their own**
    (AndPaid: "The notification is replaced by a short 'Paid' one"): the
    shift's ongoing notification goes back to watching, a "Paid · Sale
    #1042" one appears for 5 minutes, and a "No payment yet" one stays
    until it's opened. Both are silent while Monokular is on screen
    saying the same thing.
40. **Setup follows the six onboarding steps**, adapted to Android:
    1 Welcome, 2 connect a store (the camera, or "Paste the link": the
    dashboard's code is a link on the store's own host, which Android
    can't open in the app), 3 the server then the store (decision 22), 4
    Square, 5 alerts (Android 13's notification permission) and then the
    bubble (AndPermission, with "Open settings" and "Use notifications
    only"), 6 Ready, whose "Start Monokular" starts the shift. The steps
    carry the mockups' "Step n of 6"; AndPermission has none, as drawn.
41. **The Square step watches for a test sale** as the mockup does: the
    till starts watching once a location is chosen, Continue waits for the
    test sale and "Skip the test" goes on without it. The test sale stays
    in the shift's list but isn't put full screen when setup ends. The
    mockup's path in Square ("Settings › Checkout › Payment types") is its
    own guess, still to check in Square POS.
42. **The till's screen is the sale full screen (AndFull), else the idle
    pane (AndTabSplit's Monokular side)** with this shift's sales under
    it. Close puts a sale back in that list; a paid sale shows "Paid" and
    returns to watching after 5 s (the iPad flow's note); an unpaid one
    (PhoneUnpaid) waits for "Refund the Square sale" or Done. Sales that
    ended while the cashier was elsewhere don't come back full screen,
    except an unpaid one that still needs refunding. "Back to Square"
    opens Square Point of Sale (`com.squareup`) when it's installed, else
    steps back to whatever was behind Monokular. The overflow menu has
    "Copy the XMR amount" and Settings; the mockup doesn't say what it
    holds. Replaced by the till tweaks: the shift (60, 64, 72), one sale
    screen by standing (70), the close sheet (62) and `TillNav` (80); the
    overflow menu is gone (a Settings cog, and the XMR amount copies when
    tapped).
43. **PhoneUnpaid's "Show a new code" isn't built.** A new code for the
    same Square sale needs a second order for one Square payment, which
    the idempotency key (decision 14) exists to prevent; the cashier
    rings the sale up again instead. Done takes its place.
44. **Settings follow PhoneSettings where Android has the same thing**:
    Store (with "Online"), Square (account, location, payment type, sign
    out), then Shift, Alerts and Bubble in place of the Dynamic Island,
    and Wake-ups. "New sale alerts" opens Android's notification settings
    (an app can't turn its own permission off). The wallet address was
    first shown without Copy, because Monokulo gave the app only its short
    form; decision 49 replaces that.
45. **Wide windows split the sale in two** (840 dp and up: the code on
    one side; amount, status, progress and buttons on the other); a
    narrow split-screen or free-form window gets the phone layout, with
    the code sized to the window and everything scrolling when it's short.
46. **"Back to Square" isn't shown in split screen**, where Square is
    already beside Monokular and stepping back would only hide it.
47. **AndPermission's label says "choose Monokular and turn this on"**:
    since Android 11 the "display over other apps" intent opens the list
    of apps, not Monokular's own page, so the cashier has to find it.
48. **A sale this till finds that another till already made keeps the
    phase the store's events gave it.** Found while testing two tills at
    one location: the second till's Square check gets the same order back
    (decision 14) and showed it as waiting again, though it was paid. Now
    it keeps the phase it had and only gains what a refund needs. A till
    that restarts and re-finds a sale from its 2-minute look-back
    (decision 15) still shows it as waiting until the event stream
    reconnects and resyncs it, because the order response carries no
    status; changing that would change the API, so it's left.
49. **The wallet address in Settings can be copied** (replaces the end of
    decision 44). Settings reads the store from Monokulo as it opens
    (`GET /api/v1/store`, decision 50), whose wallet carries its whole
    primary address. The row shows it shortened on the device, first 8
    characters, "…", last 8, as the mockup does, and the Copy icon after it
    copies the whole address: a 48 dp button, "Copy wallet address" to
    TalkBack, that reaches into the row's padding so the row stays as tall
    as the others. Until the first read of the run answers there's no
    wallet row, so nothing to copy; a store without a wallet has none
    either. The copy goes through the same helper as the sale
    notification's "Copy address": an "Address copied" toast before
    Android 13, Android's own clipboard preview from 13 on, and the clip
    isn't marked `EXTRA_IS_SENSITIVE`. A receiving address is meant to be
    shared and isn't a secret like a key, and hiding it from the preview
    would stop the cashier checking what was copied.

## App: Monokulo's HTTP API (2026-10-23)

Monokulo replaced the `/pay/{pk}/…` routes with its HTTP API (`/api/v1`,
HAL-FORMS, problem+json), and `POST /apps/connect` now answers only the
key and the API's entry. Checked against Monokulo
f7fb5c9bee755abc07dbac34dfec5f5043b6a66f.

50. **The store's details are read live, never saved.** The connection the
    app keeps is the `ak_` key and the API's entry, nothing else. The
    store's name, base currency and wallet (`GET /api/v1/store`) and this
    device's name (`GET /api/v1/app`) are read after connecting (the
    "Connected to …" screen is drawn from that read, with Continue waiting
    for it and Try again if it fails), each time Settings opens, and at app
    start (the till's and its notification's store name, and the base
    currency the notifications step shows). They're held in memory for the
    run (`LiveStore`), so Settings shows the last read, with "Reading…"
    where there's none yet, while the next is under way. Monokulo's docs
    ask for exactly this, since the merchant can rename the store or move
    it to another wallet at any time; a saved copy would show the old
    wallet as where payments go. The cost is a read per Settings visit and
    blank rows when the store can't be reached; keeping a "last seen" copy
    on the device was the alternative, and isn't worth showing a wallet
    that may no longer be the one.
51. **The app follows the API's links and forms.** It reads the entry
    (kept as decision 54 says) and finds the store, events and its own
    connection there,
    and makes orders and disconnects with the `create-order` and
    `disconnect` forms; a link or form the entry leaves out isn't tried.
    Links resolve against the URL the app actually called, so an onion
    connection stays on the onion host. An order's `self` link is kept
    with the sale and used to read it again after the event stream
    reopens; a sale another till made comes only as a sale event, and is
    read again at the event's `mk:order` link. The app builds no path. The
    order now
    carries its status, so a sale a restarted till re-finds no longer shows
    as waiting until the stream resyncs (the gap decision 48 left). One
    order's own stream (`mk:order-events`) isn't used: the store-wide
    stream already brings every sale's changes.
52. **A connection saved by the first app isn't read any more.** That
    form (`endpoint`, `public_key`, `store`, `wallet`, …) predates the API
    and was never released; reading it as the key at the endpoint's
    `/api/v1` was interim code, removed with the final API (2026-10-24).
    Such a till shows Welcome and connects again. A connection saved by
    the app on main before then (`app_key` and `api`) carries on unchanged
    (decision 55).
53. **The client is checked against Monokulo's own OpenAPI document,
    generated, not committed.** Monokulo generates it from its code and a
    test's real exchanges (`cargo xtask api openapi`) and never commits
    it, so this repo doesn't either: `protocol/openapi.yaml` is gone, and
    core's `monokuloOpenApi` Gradle task fetches Monokulo at the commit in
    `android/gradle.properties` (`monokulo.sha`) and generates it before
    core's tests (`tools/monokulo-openapi/generate.sh`). The contract test
    answers the client with Monokulo's recorded examples and checks what it
    sends against the schemas; `-Pmonokulo.openapi=FILE` uses a document
    already generated. Generating it needs only Rust (`cargo xtask api
    openapi` builds Monokulo's library and one example, no Node, no
    browser code; rustup installs the toolchain Monokulo names); CI caches
    the build per commit. Bumping `monokulo.sha` is how the app moves to a
    newer Monokulo. The document now has the app handshake with its
    problems, the entry as each key sees it, and the event stream's events
    (`x-events`) with a recorded stream, so the contract test answers all
    of them from Monokulo's own recordings and builds none of its own.

## App: the final API (2026-10-24)

Checked against Monokulo 3b0cc7c73160611bd8e1a4dd44624d0d9f59820f, which
has every fix the app was waiting for; the interim code is gone.

54. **The entry is kept as Monokulo says, per key.** `GET /api/v1`
    answers `Cache-Control: private, max-age=3600`, `Vary: Authorization`
    and an `ETag`. The client keeps it per key and entry address for
    `max-age`, so making an order is one request; once it has expired it
    asks again with `If-None-Match` and keeps its copy for another
    `max-age` on a `304`. Anything followed from it that answers `401`,
    `404` or `405` drops the copy, and the store, app, order and
    disconnect calls read the entry again and try once more. An unknown
    key is `401` at the entry, which is how the app hears it was
    disconnected. Errors are read only as problems (`type`, `detail`);
    `POST /apps/connect`'s are told apart by `type` (`invalid-connect-code`,
    `rate-limited` with how long to wait, and the rest by their `detail`),
    and a `challenge-required` is solved and sent again with
    `Monokulo-Proof`. The connect link is redeemed at its own address,
    path and all, so a Monokulo served under a path works end to end.
55. **Tor, through Orbot, preferred when it's there.** A Monokulo with an
    onion address gives `onion_api` in the handshake and `mk:onion-api` in
    the entry; the app keeps it with the connection, with a `tor` switch.
    Monokular has no Tor of its own: onion addresses go through Orbot's
    SOCKS proxy (`127.0.0.1:9050`, which resolves the name), everything
    else straight out, and plain http is allowed to `.onion` only. On
    connecting, the app goes over Tor when the store has an onion address
    and Orbot is running; Settings offers "Reach the store over Tor"
    whenever the store has one, and says when Orbot isn't running. Over
    Tor the app reads the onion entry and follows its relative links, so
    nothing goes to the web address. A till that connected at an onion
    address is always on Tor. A till connected before this keeps its
    `app_key` and `api`, carries on off Tor, and learns the onion entry
    from `mk:onion-api` the next time it reads the store; a store that no
    longer has one takes it off Tor. Embedding Tor (tor-android or Arti)
    was the alternative: tens of megabytes and a second Tor beside the
    Orbot a privacy-minded merchant already runs.
56. **The UnifiedPush module is gone.** Nothing called it: Monokulo has
    no push targets yet (decision 28). It comes back, wired up, with them.

## App: following Monokulo dc7f085 (2026-10-26)

57. **An expired sale is followed until Monokulo says it's final.**
    Monokulo now keeps watching an expired or cancelled order for a
    payment that arrives late, which turns it paid, and says so with
    `is_final: false` meanwhile. Each till sale keeps the `is_final` it
    last read (`settled`); after the stream reopens, the till reads again
    every sale that isn't paid or settled, so a late payment made while
    the stream was down still reaches the screen. A `sale.paid` for an
    expired sale moves it to paid as any other event does. A sale the
    cashier already refunded in Square that is then paid says "Paid after
    the refund" and stays on screen, asking the cashier to ring it up
    again or return the payment, instead of going back to watching.
    Nothing else in the 204 commits since 1d39b92 changes what the app
    sends or reads: the new `send-test-webhook` form is for a shop's
    server (`sk_`) only, and the rest are wording and recorded examples.

## App: Square's sandbox (2026-10-26)

58. **A debug build's Square sign-in comes back through an https page.**
    Square sends a sign-in back only to an https redirect URL (http only to
    localhost, in the sandbox), so `monokular-debug://square` can't be
    registered. A debug build registers
    `https://monokular.monokulo.com/square/debug/` (this repo's
    `site/`, on GitHub Pages under that custom domain; the project doesn't
    own monokular.app) as its redirect URL, which opens the debug
    scheme with the same query; the token exchange sends the same https
    URL, as Square checks. The page loads nothing else and the code is no
    use without the PKCE verifier, which stays on the device. Pointed at
    the stand-in Square, a debug build still uses the scheme directly.
    `SQUARE_RETURN_URI` is what the app accepts the sign-in on; a release
    build's is its App Link, `https://monokular.monokulo.com/square`, the
    same as its redirect URL, which needs the release signing key's
    `assetlinks.json` on the site once that key exists. Monokulo's own
    pages were the alternative and are ruled out: Monokulo has no Square
    code (`docs/design.md` §1).
59. **The sandbox's sign-in leaves out `session=false`,** which Square's
    sandbox doesn't support; production keeps it, so each sign-in is the
    seller's own.

## App: the till redesign (2026-10-27)

Made by the team's tech lead and product owner while building the approved
designs (the "Monokular till tweaks" canvas: the panel over Square, the
order full screen, the sale and its close sheet, the shift and its detail
pages, the enlarged code, the status bar chip), with the owner away.

60. **Where a sale stands is one sealed type** (`sales/Standing.kt`):
    `Live(phase)`, `Paid(late, afterRefund)`, `Closed`, `Expired`,
    `Reversed`, `Refunded` and `OtherTill`, read off a `TillSale` with
    `standing`. It joins what Monokulo says about the payment (the phase)
    with what the cashier did on this till (closed it, refunded it), so a
    shift row's words, its icon and the page it opens are all decided in one
    place and tested there. Order of precedence: paid beats everything (a
    late payment after a close or a refund is `Paid` with the flags);
    another till's sale that isn't live or paid is `OtherTill` whatever was
    done here; then refunded, reversed, expired (or cancelled), closed. The
    code running out therefore outranks the cashier's close: a closed sale
    whose code then expires shows "Code expired", which has no "Show the
    code again". `live` (standing is `Live`) is what the bubble lights for,
    what the ongoing notification follows and what the shift opens as a
    code; `needsAction` is closed, expired or reversed on a sale this till
    found and hasn't refunded, and a sale paid after it was refunded (the
    cashier must ring it up again or return the payment), which is what
    the shift's "Needs action" chip counts. Another till's sale is never
    "closed on" that till in the app's words: Monokulo has no event for a
    till closing a sale, so this till can't know it; the row and page say
    what this till does know (its code ran out, its payment was reversed,
    it was closed here) and name the till that has its Square sale.
61. **The cashier's marks live on the sale, as times.** `TillSale` gains
    `closedAt` and `refundedAt` (`refunded` is now `refundedAt != null`),
    and `createdAt`, `seenAt`, `paidAt` and `endedAt` for the detail pages'
    timelines, all Unix seconds. The old `closed` set in the view model (a
    sale the cashier had closed, which only hid it) is gone: a close is a
    fact about the sale that the bubble, the notification and the shift
    all need, so it belongs in the model, not the screen. What remains in
    the view model is `left`: sales whose full screen the cashier stepped
    back from, which stay live and only don't come back up on their own.
    Times come from the source that knows them: the POS's `created_at` for
    a sale this till rang up, the event's `at` for a sale another till made
    and for each step the stream reports, and the till's clock (`now`,
    injectable for tests) for the cashier's own actions and for a resync,
    which says what happened but not when. A step already timed keeps its
    time.
62. **"Not sure: close it for now" is `Till.close`**: it stamps `closedAt`
    and nothing else. The code comes down (the sale isn't live), the
    bubble, panel and chip move on, and the sale goes to the shift under
    Needs action. Nothing is said to Monokulo, which keeps watching the
    order for a late payment exactly as before; `sale.paid` arriving later
    turns the row into "Paid after you closed it". "Show the code again" on
    the closed page is `Till.reopen`, which clears `closedAt`, so the code,
    bubble and chip come back. "No: refund it in Square" closes the sale
    and then refunds it (`close`, then `refund`): the sale's timeline shows
    both, and a refund the POS refuses leaves a Closed sale with its Refund
    button, and the till's problem text saying why. `Till.dismiss` and the
    sale screen's "Close sale" link, which took a sale off the till
    altogether, are gone: a sale Square has as Monero must stay in the shift
    until it's paid or refunded.
63. **What was bought reaches the screens from the sale itself.**
    `TillSale.receipt` keeps the `Receipt` this till built from Square's
    order (the one that went into the code), so the panel's order line,
    the order page, the sale's order card, the detail pages and the
    notification's item line all read it on the device. It is never sent to
    Monokulo (the till only ever sends amount, currency, reference and the
    idempotency key, as `TillTest` checks) and never logged: the app logs
    nothing, and `TillSale` must not be put through a logger or a crash
    report. A sale another till found has no receipt here, and its pages
    say so ("what was bought stays on that till"). Counts add quantities
    up ("15 items"); the first three lines are named, then "+N more".
64. **The shift list survives the process.** The till's sales were in
    memory only, so Android ending the service lost every sale, including
    closed and expired ones the cashier still had to refund: money Square
    had taken as Monero that nobody was reminded of. The till now saves its
    sales (JSON, through a `ShiftStore` the app backs with `SecureStore`,
    so the receipts inside are encrypted at rest like everything else the
    app keeps) on every change and loads them when it starts; the resync
    that follows the stream opening then brings their phases up to date.
    The list is this shift's: it starts when the cashier starts watching
    (`Stores.shiftStartedAt`, the "since 9:02 am" in the shift's summary)
    and is cleared when the next shift starts after a stop. It is capped at
    500 sales, newest first.
65. **The panel's grabber and the bubble lower it; nothing else is a
    collapse button.** The phone panel is still one `MATCH_PARENT` overlay
    window with the lit bubble docked above it. Dragging the grabber down
    moves the panel with the finger (`translationY`) and, past a quarter
    of its height or with a downward fling, lowers it (`panelOpen = false`);
    short of that it springs back. Tapping the docked bubble lowers it, as
    before; on a tablet, where the real bubble stays, its tap now toggles
    the panel instead of only opening it. A tap on the empty part of the
    window still lowers the panel: that window takes every touch, so the
    alternative is a dead zone over Square. The grabber is a 48 dp button
    to TalkBack ("Lower to the bubble"), which can't drag. On the tablet's
    floating panel, whose window is only as big as itself, the drag fades
    the panel instead of moving it.
66. **The order page is a page of the overlay, and a screen of the app.**
    From the panel, its order line swaps the panel's content for "The
    order, full screen" inside the same overlay window (a sealed
    `PanelPage`, Code or Order), and its back arrow or "Show the code" swaps
    it back. An overlay window can't take the system Back key (it isn't
    focusable), which is why the design draws both ways back on screen.
    Starting Monokular's activity for it would have taken Square off the
    screen, which the panel exists to avoid. The header row's "Open the
    shift" and the panel's "Full screen" do start the activity
    (`EXTRA_SHIFT`, `EXTRA_ORDER`), as "Full screen" already did. In the
    app, the sale screen's order card shows up to four lines (three and
    "+N more" when there are more) and then opens the same page as
    `Screen.Order`, so a long order can't push the sale's buttons off the
    screen; both places draw one `OrderPage` composable.
67. **The enlarged code is a screen of the app** (`Screen.Code(orderId)`),
    white in both themes, with the window's `screenBrightness` set to full
    and `FLAG_KEEP_SCREEN_ON` while it's up, both undone when it goes;
    its status bar icons are dark for the white ground. A tap anywhere or
    the shrink button goes back, as does the system's Back. The sale screen
    keeps the screen on too while a live code is showing; the panel over
    Square doesn't change brightness (an overlay window's brightness isn't
    honoured, and the design says the panel's code never changes size).
68. **The status bar chip is the small icon alone.** On Android 16+, a
    promoted ongoing notification's chip shows its small icon and, when
    set, `setShortCriticalText`; the amount that was there is removed, so
    the chip is the icon only, as designed. The icon is one of four
    monochrome vector drawables chosen by phase (`ic_mark_q1`, `_q2`,
    `_q3`: the mark's open ring with its pupil, the ring's arc filled a
    quarter, a half, three quarters, the rest at 30% alpha, which the
    status bar keeps; `ic_tick`: a tick), and the notification's
    `setColor` is the accent, or the paid green for the paid face. The
    system draws the chip and decides how it uses the colour; the app
    doesn't depend on it being orange. The ongoing notification shows the
    paid face, promoted, for a few seconds (`Bubble.PAID_MS`) and then goes
    back to watching or to the next live sale, so the chip goes; a sale
    that is closed, expires or is reversed isn't live, so the notification
    moves on and the chip goes at once. The separate "Paid" and unpaid
    notifications (decision 39) stay, unpromoted, in the shade. Before
    Android 16 there is no chip: the same per-phase icon is the ongoing
    notification's status bar icon, and the shade shows the same
    notification. The watching, paid and unpaid notifications use the plain
    ring-and-pupil mark (`ic_mark`); the launcher icon is unchanged.
69. **Three theme roles added, none renamed.** The designs are drawn in dark
    mode in the dark tokens' own colours (ground `#1C1B19`, card `#2A2825`,
    divider and chip `#3A3733`, edge `#4A4743`, muted `#A8A29A`, ink
    `#ECE8DF`, link `#FF8A3D`, paid `#4CC27A`, warn `#E8C267`, warnGround
    `#3D3320`, danger `#FF6B6B`), so they map onto the existing roles. Two
    had no role: `dangerGround` (`#3D2422` dark, `#F6DCD9` light) behind
    the tinted X, a reversed sale's icon and its note, `onDangerGround`
    (`#FFD2CE` dark, `#6E1A1A` light) for the note's text on it, and
    `paidGround` (`#1E3A28` dark, `#DCEFE2` light) behind the paid tick.
    The close sheet's "Yes" icon sits on `warnGround` rather than a fourth
    tint the design draws once (`#3A2A14`). The "Needs
    action" count sits on `warn` in `ground`-coloured text, which reads in
    both themes; the selected filter chip is `ink` on `ground` inverted.
    The enlarged code's screen is white in both themes, like the QR card.
70. **One sale screen, its content by standing.** `Screen.Sale(orderId)`
    shows the code (the design's SaleA) while the sale is live, and the
    matching detail page (Paid, Paid after closing, Closed, Code expired,
    Payment reversed, Refunded, Closed on another till) otherwise, so a
    sale that gets paid while on screen turns into its Paid page in place,
    and steps back to the shift after five seconds as before; opened from
    the shift, a detail page stays. The close sheet is a Material bottom
    sheet on the sale screen whose open state survives rotation. Ended and
    closed sales no longer come up full screen on their own (decision 42's
    "except an unpaid one that still needs refunding"): the shift's "Needs
    action" count and the unpaid notification, which opens the sale's
    page, do that job now. The sale screen's tap on the XMR amount to copy
    it stays. For a sale another till found, the close sheet has two rows,
    "No: close it" (refund it in Square on that till) and "Yes: keep the
    code up": there is nothing this till can refund, so "No" and "Not
    sure" would do the same thing.
71. **Times the app doesn't know aren't shown.** The design's "Monokulo
    watches for a late payment until 4:22 pm" and "Paid, 10 confirmations"
    name things the API doesn't give the app (only `is_final`, and no
    confirmation count). The pages say Monokulo is still watching for a
    late payment, or has stopped (`settled`), and "Paid", with the times
    the app does have.
72. **The shift's summary sums the sales paid since the shift started**
    per currency, and shows each currency's total joined with " · " (one,
    in practice); a sale the last shift carried over (decision 86), paid
    after its refund, is listed but not counted. Times of day are
    written as the locale does ("3:58 PM" in the US, "15:58" in Britain),
    with a plain space before am or pm; the design's lower case isn't
    forced on a locale that writes otherwise.
73. **The product owner's acceptance criteria** (the team's
    `acceptance.md`) are the test plan, and its own calls where the designs
    are silent (D1 to D12) stand except where a decision above says
    otherwise: the shift list is saved (64), so the marks on a sale survive
    the app being killed rather than being "simply absent"; and the shift
    starts when the cashier turns watching on, not each time the service
    starts (64).

Made while the two streams were built and merged (the overlay, order
page, enlarged code and chip; the shift, sale screen, close sheet and
detail pages), recorded by the developers and folded in by the lead.

74. **The panel's face follows the sale's standing.** Live: the code and
    the ring row. Paid, of any kind: the green tick, `SaleText.status` (so
    a late one says "Paid after you closed it"), "0.31684 XMR received",
    Done, and it lowers after five seconds. Anything else, where the code
    was: a clock on `warnGround` (expired, cancelled, closed here), a
    warning on `dangerGround` for a reversed payment ("Payment reversed ·
    don't hand it over"), the refund arrow on `chip` for a refunded one
    ("Refunded in Square", "Nothing more to do."), the line "Open the shift
    to refund it in Square." or "Refund it in Square on Back till." (or "on
    the till that took it" with no device name), and one button, "Open the
    shift". The lit bubble keeps the loading mark, its glints running while
    a sale is live (the owner asked for it back over the phase ring this
    decision first drew there), in its own window and docked above the
    panel; the phase shows in the panel's ring row. The panel's XMR line copies the amount when tapped, like
    the sale screen's; the enlarged code's doesn't, since a tap anywhere
    there shrinks it.
75. **The grabber's rule and its size** (adds to 65). `Panel.lowers(drag,
    height, velocity, fling)`: the panel lowers once dragged past a quarter
    of its height, or flung down at 800 dp/s or more
    (`Panel.FLING_DP_PER_S`); short of that it springs back (a snap under
    "Remove animations"). The phone panel moves with `Modifier.offset`; the
    tablet's floating one fades. The grabber measures 80 × 48 dp itself,
    the pill centred in it, and the card's top padding and the header's gap
    are gone, so the header starts 48 dp from the card's top: 12 dp lower
    than drawn, the price of a target a finger can't miss into the header
    row, which opens the shift and takes the cashier out of Square. The
    tap beside the panel lowers it through a pointer input, not a
    `clickable`, so TalkBack hears exactly two "Lower to the bubble"
    buttons: the docked bubble and the grabber.
76. **The order page's words and when it resets.** The panel's order page
    goes back to the code whenever the panel lowers, the bubble's sale
    changes, the sale stops being live or it has no receipt; on a tablet it
    shows inside the 360 dp card, at most 640 dp tall. Its subtitle leaves
    out "Square receipt …" when the sale has no Square reference, and its
    time is the receipt's issue time. Tip and Discount rows come from the
    receipt's extensions (`tip.amount`, `discount.amount`, as
    `SquareReceipts` writes them), the discount with a real minus (U+2212);
    the subtotal is the sum of the line totals. Offered no receipt (which
    shouldn't happen: the panel and the sale don't show the order line
    without one) it says "What was bought stays on the till that rang it
    up." The detail pages list every line with no cap and no link to the
    order page, whose bottom bar is "Show the code" and only makes sense
    live; the sale screen caps at four (three and "+N more", decision 66).
77. **The enlarged code leaves by itself.** `CodeScreen` calls back once
    the sale stops being live (paid, expired, closed), so `Screen.Code`
    never shows a dead code and the sale screen shows the matching page;
    `AppViewModel.back(from)` steps back only if that screen is still up,
    since `TillNav` may already have moved on, and a second Back would
    leave the sale. Full brightness is `screenBrightness =
    BRIGHTNESS_OVERRIDE_FULL`, keep-on is `View.keepScreenOn`, the status
    and navigation bar icons go dark, all restored on dispose. Its colours
    are the light tokens' roles and `Fixed` ones on white, so the screen is
    the same in both themes.
78. **The notification's words and its paid face.** Title "New Monero sale
    · Front Counter"; text "$13.88 · 3 items · waiting for payment" (no
    XMR, as the shade is drawn); the items on a second line of the content
    text, every line listed and left to Android to ellipsise, because
    `ProgressStyle` takes the style and the sub-text is the sale's number.
    The public version is the title and "$13.88 · waiting for payment": no
    item ever leaves the private one. The paid face is posted on the
    ongoing notification itself (`SaleNotifications.justPaid`: silent,
    four green segments filled, the tick as its large icon) for
    `Bubble.PAID_MS`, after which `Ongoing.of` goes back to the next live
    sale or to watching; the separate "Paid · Sale #…" notification (39)
    still alerts, unpromoted, with `ic_mark`. One ongoing notification, as
    before: the foreground service's, which becomes the sale.
79. **A refund's problem belongs to its sale.** `TillState.refundProblems`
    maps an order to the POS's reason ("Couldn't refund it in Square: …"),
    shown above that sale's Refund button until the refund is tried again.
    It replaced `TillState.problem` for refunds because `check()` resets
    that every two seconds, so the notice vanished before the cashier
    could read it.
80. **Where the till goes is `TillNav`**, pure and tested, which replaced
    `TillFocus`, `focus` and `showing`. `TillNav.follow(screen, sales,
    left)`: on the shift, the newest live sale the cashier hasn't left
    comes up; a sale on screen stays whatever it becomes; its code or order
    page gives way to the sale once it isn't live. `TillNav.paidOnScreen`
    and the view model step back to the shift five seconds after a sale is
    paid while its code was up; opened from the shift once paid, the page
    stays. Leaving a sale brings up the next live one at once, as the old
    screen did. A sale id the till doesn't list yet (a notification tapped
    before the service loaded the shift) shows the shift until it appears;
    a disconnected till shows the shift's Disconnected state whatever was
    on screen.
81. **When a shift starts.** `setShift(true)` after watching was off clears
    the saved sales but for those that still need action (decision 86)
    and stamps `shiftStartedAt`; `TillRunner.start` stamps
    it when it's null (setup's first run, or an install updated from before
    the shift was saved), so setup's test sale stays in the first shift.
    With watching off the summary reads "Not watching" over Start watching,
    and the rows still list what the till has.
82. **The pages' notes, in the app's own knowledge.** Closed: "The code is
    still live until 4:22 PM, and Monokulo keeps watching for a late
    payment after that: if one arrives, this sale turns paid." while
    `expiresAt` is ahead (the code's own expiry is known), else "Monokulo
    keeps watching for a late payment for a while: …", and "Monokulo has
    stopped watching it." once `settled`; Expired the same way with "The
    code ran out at 4:22 PM". A refunded sale's timeline says "Closed:
    customer not paying" whenever it was closed before the refund (the
    sheet's No, or the Closed page's Refund: both mean the customer wasn't
    paying), else "Code expired", "Cancelled" or "Reversed". The close
    sheet's "Yes" line is "14:52 left to pay." while waiting or part paid
    and "The payment is being confirmed." once seen (the design draws only
    waiting). Another till's live row has no "· on Back till" suffix; its
    ended rows do.
83. **The X only while live, and a paid sale is never refunded.**
    `Closing.choices(sale)` is empty once a sale isn't live, so the tinted
    X never shows on a paid or refunded sale. Three layers keep "No: refund
    it in Square" from refunding a payment that landed as it was tapped:
    `Till.refund` returns before the POS call when the phase is Paid or
    `refundedAt` is set; `AppViewModel.closeSale` applies the sheet's
    answer only while `Closing.applies(sale)` (the sale is still live in
    the runner's state); and `CloseSheet` hands the choice up only when its
    hide animation finished normally, so a sale that turned paid and
    swapped the screen out mid-animation drops the answer. `TillTest` and
    `ShiftListTest` cover it.
84. **Small things outside the plan's file lists.** `@Serializable` on
    `PaymentCode` (receipt) and `FoundSale` (sales) for the saved shift;
    the pure `ShiftList`, `SalePage`, `Closing` and `CloseChoice` in the
    app module beside `SaleText`; `Fixed.White` (the QR card, the enlarged
    code's screen, a glyph on ink) and `Fixed.Scrim` (the sheet's 62%
    near-black) in the theme, so the app has no colour literal outside it;
    `CircleButton` in Parts; `ProgressSegments` removed with the old sale
    screen; `MainActivity`'s own clipboard code replaced by
    `SaleNotifications.copyXmr`, which the panel uses too. New tests:
    `PanelTest` (the grabber rule, the header's place, the ended faces),
    `OrderPageTest` (counts, rates, totals, tip, discount, JPY, the
    subtitle, the countdown line), `NotificationFaceTest` (`Face.of`,
    `Ongoing.of`), `ShiftListTest` (filters, the Needs action count, the
    summary, which row opens what, the pages' words, the sheet's
    outcomes, `TillNav`) and `TillTest`'s saved shift, cap, close, reopen
    and refund guard.
85. **QA's first pass over the panel and the chip** (refines 65, 68, 74).
    The chip: Android 16 prints a chronometer, or a `when` in the future,
    as a promoted notification's chip text, so the ongoing notification
    sets no time at all (`setShowWhen(false)`, no chronometer) and the
    chip is the icon alone; the code's countdown is on the panel, the
    sale screen and the enlarged code. Android 16 draws the promoted
    chip itself, a system-themed pill with a white icon whatever
    `setColor` says, and refuses promotion to a colorized notification,
    so the chip can't be the board's orange pill or green tick: the
    shape of the icon (the ring filling, then the tick) carries the
    phase, and `setColor` still tints the icon in the shade. The panel's
    paid face: the overlay sees the sale it shows turn paid itself
    (`Panel.landed`) rather than waiting for the service's `paid()`, so a
    second sale waiting behind it comes up only after `Bubble.PAID_MS`
    and the cashier sees the payment land; `PanelTest` covers it. The
    panel's window on a phone draws edge to edge (`fitInsetsTypes = 0`,
    the cutout always), under the status and navigation bars, and pads
    itself with the safe drawing insets, so The order's ground runs under
    the bars. With the phone on its side the bubble docks beside the
    panel and the panel puts the code and its status to the right of the
    header, amount, order line and buttons, the code at its one size; the
    grabber heads that left column alone (still 48 dp, centred over the
    header) so the code starts 18 dp from the panel's top, the column's
    gaps close to 10 dp and the order line reads "15 items · Flat white,
    Oat latte, +3 more" on one line, which at the default font fits a
    phone's height with a few dp to spare; the card still scrolls when a
    large font needs more. The panel's paid hero and the bubble's paid
    look draw the detail pages' hero, the paid tick on the paid ground:
    the panel in the theme's colours, the bubble in the light theme's
    whatever the theme, as the rest of the bubble is; the solid green
    `PaidTick` stays only in onboarding's previews. The "Paid" and "No
    payment yet" notifications share a group, and once two are up a
    silent summary with the mark (`ic_mark`) heads it, opening the shift,
    gone when the Paid ones would be: Android's own bundle used the
    launcher icon, a solid square in the status bar.
86. **The end of a shift loses nothing to do.** Turning watching off
    closes every live sale (`Till.closeLive`, as the sheet's "Not sure"
    does), so no code stays up with nothing watching it; the shift's rows
    still list under Start watching, from the saved shift when the app
    starts with watching off (`TillRunner.showSaved`, a till that isn't
    running), and their pages still refund. Turning watching on again
    starts a new shift that keeps the last one's Needs action sales
    (`ShiftList.carried`) and drops the rest, so nothing needs a warning
    or a question first. A restart or a settings change that restarts the
    service closes nothing.
87. **Only a payment moves a sale that ended unpaid.** A double spend
    voids the payment, and the order's status then reads `pending` again
    with nothing saying it was reversed, so a resync once put a reversed
    sale back to Waiting with its code, bubble and chip. `Till` now keeps
    a reversed sale reversed (expired doesn't replace it either: it still
    mustn't be handed over) and never moves an expired or cancelled one
    back to a live phase; a paid status, late, still pays either.
88. **The launch intent acts once.** `MainActivity.onCreate` ran
    `handle(intent)` on every recreate, so a rotation or a theme change
    replayed the intent the activity was born with: a shared connect link
    reopened onboarding over the enlarged code, a notification's
    `EXTRA_ORDER` or `EXTRA_SHIFT` reopened that sale or the shift over
    whatever the cashier was doing. The intent is now handled only on a
    fresh create (`savedInstanceState == null`) and never when the system
    relaunches the activity from Recents (`FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY`),
    where Android hands back the original intent too; `onNewIntent` calls
    `setIntent` first so a later recreate sees the newest one. The view
    model keeps the screen across a recreate; after the process is killed
    the intent isn't replayed either, so the app comes back on its default
    screen rather than on a sale the cashier may have left, which is the
    plainer outcome. In the shift's top row the store and location are
    their own text and shorten first, so " · Watching", " · Not watching"
    or " · Disconnected" is never cut at large font sizes; the TalkBack
    label is unchanged.
89. **A phone is a phone whichever way it's held** (QA's third pass; refines
    36 and 65). `Overlay.wide()` read the display's current width, so a
    phone on its side (952 dp across) counted as a tablet: raising the panel
    from the bubble in landscape put up the floating card, a 360 dp window
    at the top right that holds only the header, the amount and the order
    line, clipped at the screen's bottom with no code and no buttons, the
    free bubble still lit beside it. The answer was also captured in the
    state flow, so it went stale until the next sale event: back upright
    the next raise was still the floating card, and a sale event in
    landscape with the phone panel up added the free bubble beside the
    docked one. Now `Panel.wide(smallestScreenWidthDp)` (600 dp and up,
    tested) decides, the same whichever way a phone is turned; `show`
    judges it on every pass rather than carrying it in the flow, the
    service forwards `onConfigurationChanged` so a fold opening or split
    screen is judged again, and a panel of the other kind is taken down
    and put up again rather than left clipped. Beside it, QA saw once a
    failed refund's Closed page with "Rung up" but no "Closed" row:
    `Till.add` read the known sale, built the new one and replaced it in
    two steps on `Dispatchers.Default`, so a close or refund stamped
    meanwhile on another thread could be overwritten by the resync; it is
    one `change` now, and a re-found sale keeps `refundedAt` as it kept
    `closedAt`. Not reproduced, so the race is closed on reasoning, not
    on a failing test.

## App: Wake-ups, the Monokular Bell (2026-10-28)

Made by the team's tech lead and product owner while wiring the app to the
Bell (`docs/design.md` §3, §5, §7; `docs/bell.md`), with the owner away.
The product owner's acceptance criteria (D1–D16 there) are taken as
written except where an entry below says otherwise.

90. **UnifiedPush's connector, `org.unifiedpush.android:connector` 3.3.5**
    (Apache-2.0, released 2026-08-22 on Maven Central), in a `push` module
    that depends on `core` alone (decision 56's module, back as the design's
    `push/`). `UnifiedPush.register(context, "bell", null, vapid)` hands the
    Bell's `vapid_public_key` (base64url, as `/v1/info` gives it) to the
    distributor, which then takes pushes signed by that key only; the
    connector throws `VapidNotValidException` for a key that isn't a P-256
    point. One instance for the Bell in use; another Bell, or the same
    Bell with a rotated key (D16: the key is kept in `BellRegistration`),
    unregisters the instance and registers again for the new key. FCM is
    not a path this release (D14): the embedded FCM distributor needs
    Firebase keys and a deployed relay, and neither exists; the app ships
    no Firebase dependency. The connector is wrapped behind
    `PushSubscriptions`, so the registration logic and its tests never see
    Android.
91. **Without a distributor, Wake-ups says so and stays on the timer.** The
    connector's dialogs and its `unifiedpush://link` deep link are not
    used: `UnifiedPush.resolveDefaultDistributor(context)` decides without
    an Activity. None installed: status `NoDistributor`, the card names
    ntfy, the choice is kept, and the app looks again at every app start
    and Settings open, so installing one and coming back connects without
    choosing again. One, or a default: saved (`saveDistributor`) and used.
    Several and no default: status `ChooseDistributor(packages)`, and the
    status card lists them as radio rows in the app's own style (D8); the
    pick is `saveDistributor`, and registration goes on. A saved one that
    is gone, or that unregisters the app (`onUnregistered`), takes the Bell
    registration back (`DELETE`) and is `NoDistributor`.
92. **The subscription's keys are the connector's, on this device.** The
    connector's `DefaultKeyManager` makes the P-256 pair and auth secret per
    instance, keeps the private key in the app's SharedPreferences encrypted
    AES-GCM under a key in the Android Keystore, and decrypts each push
    (RFC 8291) before `PushService.onMessage`: the app sees
    `PushMessage(content, decrypted)`. The private key never leaves the
    device and is never ours to hold; `allowBackup=false` and the backup
    rules keep it out of backups, and a restored till re-registers at its
    first start anyway (A12.3). A ding is taken only when `decrypted` is
    true and the content is the JSON object `{"wake":"square"}` and
    nothing else; anything else is dropped, never logged. Holding the
    private key in the Keystore itself (EC key agreement there needs
    API 31) is a follow-up, not this.
93. **A ding reaches the shift through the app's graph, and is dropped
    outside one.** `WakePushService` (the connector's `PushService`, in the
    app's process) gives each ding to `Graph.push.dings`, a `SharedFlow<Unit>`
    without replay; `TillRunner` collects it while a shift runs and calls
    `Shift.wake(WakeCause.Ding)`, and `BellWakeUps` collects it for "Last
    wake-up" and to end `MissingWakeups`. No shift (watching off, or the
    process was started by the ding alone): nothing checks Square, by the
    design's "outside a shift nothing runs" (D9), and no foreground service
    is started from a push (Android 14's rules make that a fight not worth
    having for a sale the cashier isn't watching for). The registration
    lives with the choice, not the shift (D4): registering costs a Square
    request and one of the Bell's 32 places per location, a ding outside a
    shift costs nothing, and the Bell never learns when shifts start.
94. **The pace follows the status, and a missed wake-up is detected by the
    timer.** `WakeUpStatus.interval` is 30 s for `Connected` and 2 s for
    every other status: one function, one test over all of them. `Shift.run`
    reads it afresh on every pass, so a change needs no restart, and tells
    the runner why it checked (`WakeCause`: `Timer`, `Ding`, `Opened`) and
    how many sales it found (`Till.check` answers the count). A *timer*
    check that finds a sale while `Connected`, with no ding in the 60 s
    before it, is a wake-up the Bell missed (D12): status
    `MissingWakeups`, pace 2 s, and one `POST /v1/register` again (the Bell
    may have deleted the registration after failed dings; it keeps the id
    for the same subscription). The next ding returns it to `Connected`.
    Replaces the lead's first cut, a ten-minute "doubt" that went back to
    30 s on its own: an unproven Bell should not slow the till. The app
    opening wakes the shift as before, in every status.
95. **Registering, in order, and registering again.** Choosing a Bell: the
    choice is saved, the old registration (another Bell's) is taken back,
    then `GET /v1/info` (10 s timeout; failure → `Unreachable(reason)`,
    with "no connection", "it didn't answer", "it answered 503" or "it
    isn't a Bell" for an answer that isn't `/v1/info`'s). Its `environment`
    must match the build's (`sandbox` or `production`), or it is
    `Refused("it's for Square's sandbox" | "it's for Square's production
    accounts")`, final. Its `square_application_id` must be the one the
    till signed in through, or `SignInNeeded(id)` (decision 96). No Square
    sign-in at all (or the till runs without Square): `NoSquare`. Then the
    distributor (decision 91), `PushSubscriptions.subscribe(vapid)`, and on
    `Subscribed`, `POST /v1/register` with `SquareAuth.accessToken()`, the
    location and the subscription: `201` → `Connected`, kept as
    `BellRegistration` in the encrypted store. Problems, read as RFC 9457
    `type`s with the Bell's `detail` as the fallback: `square-refused` →
    `Refused("Square didn't confirm this location. Sign in to Square
    again.")`; `too-many-registrations`, `endpoint-registered`,
    `invalid-request` → `Refused` with the acceptance criteria's words, no
    retry on their own; `rate-limited` → `Refused(…, retryAt)`, retried
    after `Retry-After`; `square-unavailable`, `internal`, other 5xx and no
    answer → `Unreachable`, retried with a growing wait, 1 min doubling to
    15 min (D5). Every app start and shift start with a Bell chosen
    registers again (`recheck()`: the Bell answers the same id); so do a
    new Square sign-in, a changed location, and a new endpoint from the
    distributor (`DELETE` of the old id first, since the Bell keys
    registrations by endpoint). The retries live in the graph's coroutine
    scope: no WorkManager and no alarm (D13); a process that dies retries
    at its next start. The connector's `NETWORK` failure is `Unreachable`;
    `ACTION_REQUIRED` and `VAPID_REQUIRED` are `Unreachable("the
    UnifiedPush app refused")`, an endpoint without keys `Unreachable("the
    UnifiedPush app gave no keys")`.
96. **A Bell with another Square application means signing in through it.**
    The till's Square application is `Stores.squareApplicationId ?:
    BuildConfig.SQUARE_APPLICATION_ID`, read by `Graph.squareConfig()`
    (D7: the application in use is saved with the sign-in). For
    `SignInNeeded`, the status card says "This Bell uses a different Square
    application. Sign in to Square through it to use the Bell" with a
    "Sign in to Square" row; the sign-in is started with that application's
    id (kept with the pending PKCE state), and only once it finishes is the
    id saved as the till's and the old sign-in replaced, so an abandoned
    sign-in leaves the working one alone (D6). The location is chosen again
    if the new account has several; the sales cursor is the location's and
    carries over. Back to the project's Bell is the same flow the other way;
    Timer only keeps whatever sign-in there is.
97. **Unregistering is awaited.** `DELETE /v1/register/{id}` (a `404` is as
    good as a `204`; 10 s at most), then `PushSubscriptions.unsubscribe()`,
    then the registration is forgotten, on: Timer only chosen (the choice
    is saved first; the "My own Bell" address is kept for next time, D11);
    another Bell chosen (before the new one's `POST`); signing out of Square
    (the `DELETE` before the sign-out, since it is awaited and needs only
    the id and the address; the choice stays, status `NoSquare`, and the
    next sign-in registers again); disconnecting the device (before
    `forgetAll`, which puts the choice back to Timer only); the distributor
    gone (decision 91). A Bell that can't be reached for the `DELETE` is
    tried once and the registration dropped locally; the Bell deletes a
    dead endpoint by itself after its push service answers 404/410, or
    after its failure limit.
98. **A Bell's address is `BellAddress`**: `https`, a path kept (a Bell
    under one), a trailing slash dropped, a missing scheme meaning https,
    shown as its host and path without the scheme; a user name, a query or
    a fragment is refused, and plain `http` is taken in a debug build only
    (a Bell on the developer's machine) (D2). An onion address is refused,
    and a Bell is reached directly, never through Orbot (D10): Square's
    webhooks force every Bell onto a public https address anyway, the ding
    goes through a public push service the till talks to regardless, and
    the Bell is told what it learns in the card's footnote; the lead's
    first cut, onion Bells through Orbot with an "Orbot isn't running"
    state, bought nothing for its two more states. The project's Bell is
    `BuildConfig.BELL_URL` from `-Pmonokular.bell.url` (default
    `https://bell.monokulo.com`, a placeholder until one is deployed), which
    takes whatever the field takes (a host, or `http://10.0.2.2:8445` for a
    debug build): a whole address rather than D1's host, so a Bell under a
    path works too. The subscription's endpoint is the distributor's and
    must be public https for the Bell to take it.
99. **Settings › Wake-ups, in the app's own parts, as the acceptance
    criteria's A1 lays it out.** Not a setup step; a fresh install and every
    updated one is Timer only (D3). Under the section label, one `Group` of
    three `ChoiceRow`s (new in `Parts.kt`: label, muted detail, a tick in
    the ink colour at the end of the chosen one, `selectableGroup` on the
    group and `Role.RadioButton` with `selected` on each row, the whole row
    tappable): "Timer only" / "Checks Square every 2 seconds", "Monokular
    Bell" / its host, "My own Bell" / its saved address or "Enter its
    address". With "My own Bell" chosen, a fourth row holds an
    `OutlinedTextField` "Bell address" (`KeyboardType.Uri`) and an
    `OutlineButton` "Use this Bell", enabled only when the field differs
    from the saved address and parses, with `BellAddress.problem` as a
    `Muted` line under the field. With a Bell chosen, a second `Group`:
    `FactRow("Checking Square", "Every 30 s" | "Every 2 s")`, `FactRow("Bell",
    …)` with the status word ("Connected" in `mk.paid`, "Connecting…", "Not
    connected", "Needs a UnifiedPush app", "Sign in needed", "Wake-ups not
    arriving"), "Last wake-up" while `Connected` or `MissingWakeups`, the
    distributor rows for `ChooseDistributor`, and an `ActionRow("Sign in to
    Square")` for `SignInNeeded`. The footnotes are A1.7's words, status by
    status, plus the privacy footnote on every Bell choice, and the Shift
    footnote changes with the pace (A1.8). All of it from one exhaustive
    `when` over `WakeUpStatus` in a pure `WakeUpsText`, tested string by
    string (D15). `Theme.kt` tokens only; both themes; 200% font scale wraps.
100. **The connector's service reaches the graph through the
     application, and a ding alone never makes Wake-ups.** `push` can't see
     `MonokularApp`, so `WakePushService` asks the application for a
     `WakePushService.Host` (`MonokularApp` is one) and hands each callback
     to its `UnifiedPushSubscriptions` (`Graph.push`), which turns them into
     `state` and `dings` and returns: no work, no service started, nothing
     logged. `Graph.wakeUps` is made on first use (the view model, Settings
     or a shift starting), and that first use is decision 95's "app start"
     (it registers again when a Bell is chosen); a process the distributor
     started for a ding outside a shift touches only `Graph.push`, so it
     costs the Bell no registration and Square no request. The connector's
     own manifest brings its distributor `<queries>`, `WAKE_LOCK`, its
     message receiver, its link activity (unused: decision 91) and an
     exported `RaiseToForegroundService` that a distributor binds to raise
     the app's priority while it delivers; the app declares only
     `WakePushService` (`exported="false"`, `PUSH_EVENT`), A12.1. The one
     thing in the app's logcat from Wake-ups is the connector's own debug
     line, "Found distributor with package name …", once per look at the
     installed distributors (each pass and each change of apps: QA counted
     125 in a 35-minute run); it names a package and nothing of the till's,
     so it stands (A9.5).
101. **The shift says why it checked, counts what is new, and re-reads its
    pace every 2 s** (dev-2; refines 94). `WakeCause` gains `Started`, the
    shift's first check: sales rung while nothing watched (a ding outside a
    shift is dropped, decision 93) are found by it, and a `Timer` find there
    would read as a wake-up the Bell missed. `Till.check()` answers how many
    of the POS's sales it found that the previous check hadn't, another
    device's included (the Bell rings for every sale at the location, not
    for one till's): a sale whose order failed and is found again isn't
    counted twice, and a POS that can't be read found 0. The timer waits in
    ticks of 2 s at most and reads `interval()` at each, so the pace going
    from 30 s to 2 s mid-wait checks within a tick rather than at the end of
    the 30 s (A5.24); the ticks cost a coroutine resuming, no request. The
    wait is a `select` over the conflated wake channel and the tick's
    timeout, not `withTimeoutOrNull { receive() }`, which can take a wake-up
    as it times out and drop it. The pace Settings shows is read off the
    status, the same `WakeUpStatus.interval` the shift reads, so the plan's
    `RunnerState.interval` would only have copied it and isn't added.
102. **The app sees a distributor come or go by itself.** The connector has
     no callback for a distributor uninstalled, and decision 91's "looks
     again at every Settings open" would have put a `recheck()` (a
     `POST /v1/register`, and a request to Square) on every visit to
     Settings. `UnifiedPushSubscriptions` registers, for the life of the
     process, a receiver for packages added and removed (a system
     broadcast, delivered for the distributors the connector's `<queries>`
     make visible), and `BellWakeUps` looks again only when it is waiting
     for a distributor (`NoDistributor`, `ChooseDistributor`) or when the
     one it subscribed through is no longer what `resolve()` gives:
     installing ntfy connects while Settings is open (A7.30), and
     uninstalling it takes the Bell registration back at once (A7.32); the
     emulator script's step 17 is where that is seen. The
     distributor is `Distributors.resolve()`: the saved one while it is
     installed, else the connector's `resolveDefaultDistributor` (the
     default or the only one, saved), else the list for the cashier.
103. **Settings › Wake-ups' choices, where A1 is silent** (dev-2; refines 99).
    "My own Bell" tapped with no address saved yet ticks the row and opens
    the address field, but nothing is chosen until "Use this Bell": the
    saved choice, its registration and its status card stand meanwhile
    (hidden while the row waits, with the footnote "Enter the address of
    the Bell you run, then press Use this Bell. Until then Wake-ups stay as
    they were."); with an address saved, the tap chooses it at once. The
    field starts with the saved address as the row shows it, with its
    scheme only when that isn't https (a debug build's http Bell), so it
    reads back as the same address and "Use this Bell" stays off. The Bell
    row says "Sign in needed" for `NoSquare` as well as `SignInNeeded`
    (the emulator script's step 16 expects it; the footnotes tell them
    apart), and "Choose a UnifiedPush app" for `ChooseDistributor`, whose
    footnote is "Deliver wake-ups through one of the UnifiedPush apps
    above. Until you choose, Monokular checks every 2 seconds."; its rows
    are `ChoiceRow`s in the status card named by Android's label for each
    package, none ticked. `NoDistributor` adds a "Get ntfy" row in the link
    colour that opens `https://ntfy.sh/` in a Custom Tab, as Square's
    sign-in opens. Settings opening calls nothing: `UnifiedPushSubscriptions`
    hears a package installed or removed and `BellWakeUps` looks again by
    itself (decision 91), and the card follows its status live.
104. **The Bell client is three suspending calls behind `BellClient`, and
     its errors are two exceptions.** `BellApi` takes the graph's client,
     with `Proxy.NO_PROXY` (decision 98: never Orbot, whatever the store
     does) and a 10 s `callTimeout` per call. A problem is `BellException
     (status, type, detail, retryAfter)`, `type` being the slug after the
     `#` of the Bell's problem address (`…/docs/bell.md#rate-limited`) and
     `retryAfter` from `Retry-After` or the body's `retry_after`; an answer
     that isn't the Bell's (a `/v1/info` that isn't its shape, an unknown
     `environment`, a `4xx` page without a problem) is
     `NotABellException`; no answer is the `IOException` OkHttp gives, a
     timeout an `InterruptedIOException`. `register` answers
     `BellRegistered(registrationId, squareApplicationId)`, and
     `BellWakeUps` makes the `BellRegistration` from it with what it sent.
     `BellApiTest` records the requests at a `MockWebServer`: the three
     routes only, no `Authorization` header on any, the token in
     `register`'s body and nowhere else, that body exactly the three fields
     and the subscription's public half (A9.1, A9.2).
105. **The view model's side of signing in through a Bell, and of taking
    the registration back** (dev-2; refines 96, 97). `Stores.pendingSignIn`
    is a `PendingSignIn(pkce, applicationId)`: the application rides with
    the PKCE state (kept across the app being ended), null for the till's
    own. The attempt's `SquareAuth` is built with that application, so
    `authorizeUrl` and `finish` use its `client_id`; only after `finish`
    succeeds is `Stores.squareApplicationId` set (and the old token
    replaced by `finish` itself), the running till restarted as the new
    application, the location chosen again when the account has several,
    and the cashier taken back to Settings, where `chooseLocation`'s
    `recheck()` carries the registration on to `Connected`. A sign-in that
    fails goes back to Settings with Square's words, shown above the
    Wake-ups cards, and leaves the old sign-in working. "Sign out of
    Square" now runs as the view model's other network work does:
    `unregister()` awaited, then the sign-out.
    "Disconnect this device" awaits `unregister(forget = true)` before
    Monokulo's disconnect (the Bell's `DELETE` comes first in its log, as
    the emulator script's step 22 expects), and "Connect again" after the
    store disconnected the till does too; redeeming a new connect code
    takes back any registration left before `forgetAll`. A location change
    calls `recheck()`, since the Bell rings the till for the location it
    registered.
106. **One pass at a time brings the Bell into line with the choice.**
     `BellWakeUps` doesn't keep a state machine beside the store: a pass
     reads the choice and the registration and does what decision 95
     orders, under one lock, and `status` is where it stopped. Choosing
     cancels a pass under way and starts one; `recheck()` joins one under
     way; a retry is a pass after a wait (`delay` on the graph's scope, so
     tests run it on virtual time). Once `POST /v1/register` is sent, it
     and saving its answer can't be cancelled, so a choice changed meanwhile
     always finds the registration to take back. `unregister` cancels the
     pass, then takes the lock: the `DELETE` (once, 10 s at most), the
     distributor's subscription ended, the registration forgotten. A pass
     that starts `Connected` (a shift starting, an app start) stays
     `Connected` while it registers again, so the pace doesn't drop to 2 s
     for a second; one that fails leaves it. The connector is asked for the
     subscription once per process (its docs: register at every start) and
     again only for another key or another distributor; an endpoint other
     than the registered one, at a start or from the distributor later, is
     a `DELETE` of the old registration and a `POST` of the new.
108. **A till that can't use its registration doesn't keep it.** A pass
     that ends at `NoSquare` (no sign-in, or Square ended it), at
     `SignInNeeded`, at the other environment's `Refused` or at
     `NoDistributor` takes back a registration it finds (`DELETE`): the
     Bell would otherwise ring a till that can't check Square for it, or
     keep a location for a till that left. Refusals from the `POST` itself
     leave it (the Bell says it left the registration as it was), and so do
     `Unreachable` and `MissingWakeups`, which retry. A missed wake-up
     (decision 94) also needs 60 s since the till connected, not only since
     the last ding: the first timer check after connecting can find a sale
     rung up just before, which no ding was ever going to announce.
110. **The distributor's failures have words too.** Decision 95's
     `Unreachable` reasons for the Bell are joined by the distributor's: "the
     UnifiedPush app refused" (`ACTION_REQUIRED`, `VAPID_REQUIRED`, an
     internal error, or a key the connector takes for no P-256 point), "the
     UnifiedPush app gave no keys" (an endpoint without them), "the
     UnifiedPush app has no connection" (`NETWORK`) and "the UnifiedPush app
     didn't answer" (no endpoint within 30 s: the distributor may have to
     reach its server first). All are retried as an unreachable Bell is.
     They fill A1.7's `reason` and nothing else changes on the screen.
112. **What the connector brings, checked.** `org.unifiedpush.android:connector`
     3.3.5's POM gives the Apache License 2.0; it depends on
     `com.google.crypto.tink:tink` 1.23.0 (the JVM Tink, with `protobuf-java`
     and `gson`, not `tink-android`, so there is nothing to force) and on
     nothing from Google's services: `./gradlew :app:dependencies` has no
     `com.google.firebase` or `com.google.android.gms` (A7.34). It keeps
     its registrations in a SQLite database and its keys in
     SharedPreferences, both under domains the backup rules already exclude
     with everything else (`allowBackup="false"` too), so the rules are
     unchanged (A12.3) and a restored or new phone subscribes and registers
     afresh at its first start. Its AAR carries no ProGuard rules; the
     release build's R8 pass keeps what the manifest names, but the emulator
     script runs a debug build, so a ding through a minified build is still
     to be seen before a release.
114. **The connector forgets the distributor with its last registration, so
     the app keeps it.** In 3.3.5, `UnifiedPush.unregister(context, instance)`
     removes the instance and then, with no registration left, the saved
     distributor too (`DistributorStore.remove()`; the distributor is still
     told to unregister, from the token it kept). The app has one instance,
     `bell`, so every Timer only, sign-out, Bell switch and rotated VAPID key
     would forget the distributor: the rotated-key path (unsubscribe, then
     subscribe for the new key) ended in "Needs a UnifiedPush app" with ntfy
     installed, and the cashier's pick among several (A7.31) was lost.
     `UnifiedPushSubscriptions.unsubscribe()` reads the saved distributor
     first and saves it again after unregistering; `subscribe()` with none
     saved resolves one (the default, or the only one installed) before it
     fails with `NoDistributor`. The build has no Robolectric, so this rests
     on the connector's bytecode, recorded here, and on two emulator checks
     in the script: pick a distributor among two, Timer only, the Bell again,
     and no second pick; unsubscribe inside ntfy, `NoDistributor`, and the
     next shift start connects again by itself. A distributor uninstalled
     with two others left is taken back as decision 108 says: the pass
     `DELETE`s the registration before showing the choice, so the Bell isn't
     ringing a dead endpoint while the cashier picks.
116. **A missed wake-up is judged at the timer's find, not later.** A
     timer-started check that finds a sale with no ding in the 60 s before
     is `MissingWakeups` at once (decision 94). The other way, waiting
     `MISSED_AFTER` past the find for a ding, would spare one case: a 30 s
     check landing on a sale whose webhook is still in flight. That case
     costs one idempotent `POST` and a few seconds at 2 s, since the ding on
     its way puts the till back to `Connected` when it lands. Waiting costs
     every real missed wake-up another minute at 30 s, which is what the
     cashier would notice, and a timer job of its own. So the verdict stays
     at the find. QA measured the cost on the emulator against fake-square,
     whose webhook follows the sale within milliseconds: twice in 35 minutes
     a 30 s check landed within 200 ms of a sale, before its ding, and the
     till showed "Wake-ups not arriving" and `POST`ed again, back to
     `Connected` within the second. Against Square, whose webhook takes
     seconds, the coincidence is likelier (a few per cent of sales) and
     costs the same: accepted as this decision says, and the first thing to
     revisit if the flicker is seen on a till.
118. **A distributor uninstalled ends the subscription twice over, so its
     end is judged by what is installed.** In 3.3.5,
     `UnifiedPush.getSavedDistributor()` finding the saved distributor gone
     broadcasts a local `UNREGISTERED` for each of the instance's tokens
     without removing them first, and the connector's receiver answers a
     plain `UNREGISTERED` with `UnifiedPush.unregister(context, instance)`,
     by instance. So `resolve()` in decision 102's look at a change of apps
     itself brings a `Lost`, after or before the pass that look started
     through the distributor left (the worse ordering also ends the `bell`
     registration that pass just made). `BellWakeUps` remembers the
     distributor a change of apps took away; at a `Lost` it takes the Bell
     registration back as decision 91 says and then asks `resolve()`: the
     same distributor still there, or none, is `NoDistributor` (the
     distributor itself ended it, decision 91); another, or several to
     choose from, is a pass that ends `Connected` or `ChooseDistributor`,
     subscribing afresh. An app updated (`PACKAGE_REMOVED` then
     `PACKAGE_ADDED`, both with `EXTRA_REPLACING`) is no change at all and
     no longer drives a look. Tested on virtual time in both orders of the
     change and the `Lost`; the emulator script's step 27 (two distributors,
     the used one uninstalled) is where it is seen on a device. The remembered
     departure lasts until a `Lost` uses it or the app ends the subscription
     itself, which is owed no `Lost`: the last distributor uninstalled has
     the pass unsubscribe before the connector's `Lost` lands, and
     `UnifiedPushSubscriptions` drops a `Lost` with nothing subscribed, so
     without that the departure outlived it and a later distributor's own
     end (the cashier ending the subscription in it) read as the old
     departure and subscribed again instead of showing `NoDistributor`.
     Tested: connected through ntfy, ntfy uninstalled (`NoDistributor`),
     NextPush installed (`Connected`), then NextPush's `Lost` is
     `NoDistributor` with no `POST` after it.
120. **A pass never takes the till down.** The graph's scope has a
     `SupervisorJob` and no exception handler, so anything a pass throws
     that isn't the Bell's or the network's (a store that can't be read or
     written, the Keystore under the encrypted preferences, the connector's
     SQLite or the package manager, Square's client failing some other way)
     would reach the thread's uncaught handler mid-shift, as `TillRunner`
     already guards against for Square. A pass catches every `Exception`
     but cancellation and settles `Unreachable` with decision 110's list
     joined by "something on this device failed", retried as an unreachable
     Bell is; the `Lost` handling does the same, a `DELETE` that fails any
     way is dropped locally as an unreachable Bell's is, and a failing look
     at installed apps becomes the next pass. Nothing about the failure is
     logged: its message can carry what the store held. Tested with a Bell
     client throwing `IllegalStateException`: the status settles, the scope
     lives, the retry and a later `choose()` connect.
122. **The registration remembers the distributor it was made through, and
     drops what nothing reads.** `BellRegistration` kept the location, the
     Square application the Bell answered with and the time it was made,
     and `BellInfo`/`BellRegistered` decoded the Bell's version and
     application id for them; nothing read any of it, so they are gone (the
     Bell's answers still decode: the client ignores unknown keys). In their
     place it keeps `distributor`, the package the subscription was made
     through, read as `subscribedThrough ?: registration.distributor` by
     decision 102's look and by the pass before it subscribes, which records
     a departure when `resolve()` gives another distributor. That covers a
     distributor uninstalled while the app was away: at the next start the
     connector's `getDistributor()` broadcasts the `UNREGISTERED` for the old
     token during the start's `resolve()` (3.3.5 broadcasts per token and
     removes nothing), the pass connects through the distributor left, and
     the `Lost` arrives after it with this process never having subscribed
     through the old one; without the stored package that `Lost` read as the
     new distributor's own end, `NoDistributor` with one installed until the
     next shift start. Now it is a pass that ends `Connected` through the new
     distributor with the registration posted again. Tested: a registration
     made through ntfy, a start with only NextPush, then the `Lost`.
     Registrations stored by earlier builds of this branch lack the field
     and read as none (the branch was never released).
123. **A distributor that answers late, or after being opened, connects
     the till at once** (lead, after QA round 1). QA's one major finding: a
     ntfy just installed sits in Android's stopped state until it is opened,
     so the connector's `REGISTER` to it goes unanswered, the pass settles
     "the UnifiedPush app didn't answer" after decision 110's 30 s, and the
     till waited out decision 95's first minute (83 s from the install to
     `Connected`) although the cashier had opened ntfy at once. Three
     things, none of them a change to the retry schedule: (1) an endpoint
     the connector delivers while no pass runs and no registration holds it
     starts a pass, so a distributor that answers after the 30 s connects
     the till the moment it does (`pushChanged` no longer needs a
     registration to act on `Subscribed`; a `Lost` still does; a
     subscription the last pass already took is nothing new, so a `POST`
     the Bell refused isn't tried again on the strength of the endpoint it
     was made with, which keeps decision 95's rule for refusals); (2) the
     activity coming to the foreground and Settings opening call
     `recheck()` when the status is `Unreachable`, and only then, so coming
     back from the distributor (or from the network settings) tries the Bell
     now rather than at the end of the wait, while `Connected` isn't
     `POST`ed again on every return and a `Refused` keeps decision 95's
     rules; (3) the reason reads "the UnifiedPush app didn't answer: open it
     once", which says what to do and stops the words from reading as a
     fault in the app just installed. Tested on virtual time: a late
     endpoint after `PUSH_SILENT` ends `Connected` with the subscription
     held registered and no second subscribe, and the wait that was set is
     cancelled with it; a `recheck()` 5 s into the wait connects. The
     foreground call is in the activity and view model, the one place the
     app knows it came forward; the push module stays without a lifecycle.
     Reading the status there makes the activity's first start decision
     100's first use of `Graph.wakeUps`, so the app-start registration now
     happens when the app is opened rather than at Settings or a shift; a
     process the distributor starts for a ding still touches `Graph.push`
     only.
124. **Settings › Wake-ups after QA round 1** (lead; refines 99, 103). Four
     findings on the screen, each taken: "Every 30 s" wrapped as "Every 30"
     and "s" at font scale 2.0, so the pace's space is a non-breaking one
     (` `); a debug build's http Bell showed the same detail as an
     https one of the same host ("10.0.2.2:8445" for both), so a Bell's row
     detail keeps its scheme when it isn't https, for the Monokular Bell row
     and My own Bell alike (`WakeUpsText.bell`), and the address field
     starts with the same text; the rate-limited refusal said "trying again
     in 58 seconds" for the whole minute, since the footnote doesn't count
     down, so it says "trying again in about a minute" (or "about N
     minutes" from `Retry-After`), which stays true while it shows; and
     tapping My own Bell with no address saved moved the tick and hid the
     status card while the saved Bell was still registered and `Connected`,
     so now the tap opens the address field and its footnote ("… Until then
     Wake-ups stay as they were.") above the saved choice's status card,
     and the tick stays with the saved choice until "Use this Bell", as
     A3.13 ties it to a saved mode. Decision 103's "ticked here" is
     superseded by this.
125. **What QA round 1 left, and why.** Recorded rather than fixed in this
     PR: a `DELETE` that the Bell was down for (decision 97's one try,
     then dropped locally) is not remembered, so the Bell keeps the old
     row until its failure limit trips, and ntfy.sh never fails a topic, so
     for a till that left while its Bell was down the row stays until the
     Bell's operator clears it; remembering the undelivered id and address
     in the store and sending it before the next `POST` to that Bell is the
     fix, and needs a store field, so it is a follow-up and not this
     round's change. Two findings are the till's, not Wake-ups': ten sales
     in one second stack ten code screens on the back stack (a new sale
     should replace an open code screen), and "Sign in to Square" from
     Settings › Square after a sign-out goes through onboarding's Connect
     Square step and lands on an already-seen sale's code (the Wake-ups
     card's own sign-in row returns to Settings correctly); both predate
     this branch, Wake-ups only makes the burst arrive faster, and both are
     follow-ups. Decision 116's coincidence and decision 100's log line are
     recorded there.

## App: Clover, the second point of sale (2026-10-28)

Made by the team's Kotlin lead and developers while adding Clover beside
Square (`docs/design.md` §1, §3; the Bell's side is `docs/decisions/bell.md`
B43 to B57), with the owner away. The research behind them, with its
sources, assumptions A1 to A6 and risks R1 to R10, is the team's
`clover-research.md`, summarised in the PR; the product owner's acceptance
criteria (D1 to D9 there) are taken as written except where an entry below
says otherwise.

126. **The point of sale is chosen in onboarding step 4 and in Settings, and
     kept as `pos.kind`; `PointOfSale` lives in `core`**, because `push` and
     `sales` both need it and `push` doesn't depend on `sales`. A till from
     before this build (Square tokens, no `pos.kind`) reads as Square, and
     its `square.skipped` reads as `pos.skipped`, so nobody is asked again.
127. **Clover signs in with its PKCE flow for low-trust apps, on the
     device, with no secret anywhere; the redirect is the App Link
     `https://monokular.monokulo.com/clover`** (a debug build:
     `/clover/debug/`, which [site/clover/debug/](../../site/clover/debug/index.html)
     forwards to `monokular-debug://clover`; with a stand-in, the scheme
     directly). Clover takes only https under the app's Site URL, so a
     custom scheme was never an option. Clover documents no `state`: the
     app sends one and checks it only when Clover echoes it; the binding is
     the one pending attempt (`PendingSignIn.pointOfSale`) plus the
     verifier, so a redirect with nothing pending answers "That Clover
     sign-in wasn't started here. Try again." The redirect's `merchant_id`
     is the merchant: there is no location to pick, one merchant per token.
128. **Clover's tokens keep Square's shape, now `PosTokens`, with the expiry
     converted from Clover's Unix seconds to RFC 3339; a 30-minute access
     token is refreshed under five minutes left, and the single-use refresh
     token is saved before anything else.** A `4xx` refresh signs the till
     out once with "Clover's sign-in has ended. Sign in again." and never
     loops (R1: Clover caps live refresh tokens per merchant per app, so two
     tills at one merchant may sign each other out); a `5xx` is transient;
     a `429` (refresh or API) holds the next look for its `retry-after` and
     keeps the sign-in.
129. **The Monero payment type is found by label (trimmed, case-insensitive,
     an enabled and visible one first), else created over REST with
     `labelKey` `com.clover.tender.external_payment`, and payments are known
     by its id**, remembered per merchant (`clover.tender.<merchant>`). A
     REST tender must use a system `labelKey`, so the id, not the key, tells
     Monero apart. Creating needs Merchant write; finding first means a
     merchant who added "Monero" in Clover needs no write (R9). A refused
     `POST` doesn't stop the sign-in: step 4's card and Settings ask the
     cashier to add a payment type called Monero in Clover and offer **Look
     again**, and nothing retries the `POST` on a timer. A remembered id
     deleted in Clover is forgotten and found again or made, only from that
     look.
130. **A Clover sale never paid is refunded by the cashier in Clover's Orders
     app, guided and then confirmed by the till.** Clover's REST API can't
     refund, void or delete an external-tender payment (R4), so the source
     offers `Refunds.InPos` (a type that can only look, rather than a
     `refund()` that does nothing: a departure from acceptance T4.1's
     method name), the sale gets `refundPendingAt` and stands as
     `RefundInPos` under Needs action, and the **Refund in Clover** sheet
     says which order, when and how much, with **I've refunded it** (asks
     Clover now; "Clover doesn't show a refund yet" if not) and **Not yet**.
     A refund is seen when the payment's non-voided refunds reach its
     amount, the order's `paymentState` is `REFUNDED`, or the payment is
     voided. The timer looks at pending refunds at most every 30 s, not on
     every pass as acceptance D9 says: without a Bell a pass is every 2 s,
     one request per pending sale, and the sheet's button asks at once. The
     alternative, recording the payment on an open order only once the
     Monero confirms (design B), is kept in the research for if a
     REST-made tender never shows in Clover Register (A1).
131. **`PosException` is a point of sale's common error; `SquareException`
     and `CloverException` extend it**, so `Till` and the view model catch
     one type.
132. **A Bell registration names its point of sale and its `location_id`
     (a Square location, or a Clover merchant: its only location), and
     `PosSide` replaces `SquareSide`.** The till reads the Bell's
     `points_of_sale` entry for its own point of sale, in this order:
     absent, refused ("it doesn't ring Clover tills"); another environment,
     refused in words naming it ("it's for Clover in Europe"); another
     application, `SignInNeeded(pos, applicationId)` and a sign-in through
     it, as decision 96 has for Square. `square-refused` and
     `clover-refused` match the till's own point of sale only, so a
     problem slug for the other is read by its status alone.
133. **A ding is exactly `{"wake":"<the till's current point of sale>"}`;
     anything else is dropped unlogged** (decision 92 widened): a Bell rings
     a till only for the point of sale it registered, so `{"wake":"square"}`
     on a Clover till is stale or forged.
134. **The Clover receipt groups the line items Clover repeats one per unit,
     reads `unitQty/1000` for measured ones, and shows one tax line, the
     order's total less its lines** (what the paper receipt shows; per-line
     `taxRates` would need another expand, acceptance D8). A total below the
     lines is shown as a discount, as Square's is, with no tax line. The
     merchant's name comes from `/v3/merchants`, its currency, time zone
     and country from `/properties` and its address; with no country the
     sale gets the plain code. `FoundSale.reference` is null (Clover prints
     no receipt number on the payment, acceptance D7), so the receipt reads
     "Clover order …". An order with 100 or more line items gets the plain
     code: Clover's `expand` returns at most 100, so the list may be cut,
     and total less lines would print the missing items as tax. Payments
     are looked at no further back than 89 days (Clover's 90-day filter
     cap), at most 20 pages of 100 a look.
135. **No "Back to Clover" button**: the Clover point of sale is its own
     device, not an app on the cashier's phone; the button stays Square's
     (`TillRunner`'s `onBackToPos` is Square's only), and the bubble beside
     a Clover device floats "over other apps".
136. **Every word on screen that names the point of sale comes from one
     `PosText` object, with an exhaustive `when` where the two differ**
     (acceptance D5; Wake-ups' reasons from `push`'s `PosWords`): "Refund it
     in Clover", "Checking Clover", "Merchant" where Square says "Location".
     Onboarding step 4 is "Connect your point of sale", the two as radio
     rows with the chosen one's card beneath, on the one screen that
     `Screen.PointOfSale` replaces `Screen.Square` with (acceptance T1.1
     describes one screen; there is no separate chooser).
137. **Build properties `monokular.clover.applicationId` (a placeholder
     otherwise), `monokular.clover.environment` (`north_america` by default;
     `europe`, `latin_america`, `sandbox`) and, for a debug build,
     `monokular.clover.baseUrl`** (a stand-in playing Clover's web and API
     hosts on one); `CLOVER_REDIRECT_URI` and `CLOVER_RETURN_URI` mirror
     Square's. A debug build's Settings field for the stand-in's address is
     kept as `debug.clover.base`.
138. **Changing the point of sale** (Settings › Point of sale › Change the
     point of sale, acceptance D6) first takes the Bell registration back,
     awaited, while the old sign-in is still there, then signs out of the
     old point of sale (tokens, location or merchant, cursor, remembered
     payment type), keeps the Wake-ups choice and the shift's saved sales,
     and goes to the new one's sign-in; choosing the one chosen changes
     nothing. The sign-out happens even when the old sign-in had already
     ended (a refused refresh, Square's sign in again), so no location,
     merchant, cursor or payment type id of a point of sale the till isn't
     signed in to is left behind; only the Bell step waits on a sign-in.
     It is `PosSwitch`, a pure helper tested on a recording fake,
     as `TillNav` is. Only a change forgets the old point of sale's cursor
     (where its search for sales had got to); Settings' "Sign out of …"
     (`PosSwitch.signOut`) keeps it, as Square's sign-out on Wake-ups did,
     so signing in to the same point of sale again reads on from there
     instead of reading the merchant's earlier payments again (Monokulo's
     idempotency key would make that re-read harmless, but it costs
     requests and Clover's rate limit for nothing).
     `Till` keeps the old sales safe from the new source: a sale another
     point of sale took is only ever marked to refund there
     (`refundPendingAt`), never sent to the till's point of sale, whose
     payment ids it doesn't know, and the timer never asks the new one
     about it; its "I've refunded it" takes the cashier's word, since
     nothing can ask the point of sale that took it, and the sheet says
     so ("Tap I've refunded it once it's done") instead of "Monokular will
     notice".
139. **A `401` from Clover before the token's expiry (Clover ended it early)
     is met by one refresh and the request asked once more**; a refresh
     another request already made is used instead, and a fresh token
     refused too signs the till out at once, so no later pass spends a
     refresh token (R1). `CloverAuth.refreshAfterRefusal`.
140. **A Clover order's title ("Table 4") is carried on the sale
     (`FoundSale.title`, defaulted so saved shifts still read)** from the
     payments already read with `expand=order`, no extra request; blank
     titles are none, Square leaves it null. The sale page shows it as its
     first fact, Order, wherever the page has facts: once the sale is
     closed, paid, expired, reversed or refunded (a sale waiting for
     payment shows its code and order card, with no facts, as before); and
     the Refund in Clover sheet names "order Table 4"
     rather than the order's id, which is what the cashier finds in Clover's
     Orders app.
141. **A sale found before the till's point of sale changed keeps the name
     of the one that took it, and is refunded there** (acceptance D6's
     "their source name shown as it was"). `FoundSale.pointOfSale` says
     which point of sale found it (a sale saved before Clover joined reads
     as Square's), so `PosText.of(sale, till)` is that, or the till's own
     for another till's sale (Monokulo doesn't say), and names every sale's
     words, receipt label and notification; the app keeps no list of its
     own. The till can't refund such a sale or see its refund (it is signed
     in elsewhere), so its page drops Refund and says to refund it there,
     and its close sheet offers what another till's does; one already told
     to refund it there keeps Refund, which opens the refund sheet named by
     the sale's point of sale, for the cashier's word (decision 138). The
     close sheet's "No" says the cashier refunds it when the sale's point
     of sale isn't the one the till's source watches or that one can't
     refund (`Closing.refundsThere`), `Till.refund`'s own rule, and any sale
     that then stands as `RefundInPos` opens the sheet.
     A sale screen's top is `SaleHeader`, from the sale and the till's
     point of sale: the receipt's name ("Clover order …"), the bar's second
     line, which names the old point of sale ("Clover · this till") rather
     than the location the till watches now, since that sale wasn't rung up
     there, and "Back to Square" only for a sale of the Square till it is
     (`PosText.elsewhere`, `PosText.placeOf`; the notification and the
     bubble's header say the same place). QA round 1 saw a Clover sale's
     order card turn "Square receipt" on a Square till: the bar and the
     button were this build's (fixed here); the receipt's name never reads
     anything but `FoundSale.pointOfSale`, which a saved shift keeps
     (`ShiftListTest` reads one back). The likeliest cause is the one QA's
     notes record: other streams' builds of `app.monokular.debug`, which
     name every receipt Square's and know no Clover, were being installed
     over this one on the same emulator during the run. On an emulator of
     its own against both stand-ins, a Clover sale left up through a change
     to Square reads "Clover · this till", "Clover order …" and no Back to
     Square, and still does after the app is stopped and opened again.
     A step 4 opened from Settings has Back and Done in place of the step
     number and the test sale, and the shift's top row says a Clover
     merchant's name once when it is the store's.
142. **Till's idempotency key keeps its `sq:` prefix for every point of
     sale**: changing it would change Square's keys for sales in flight,
     and two points of sale's payment ids never meet at one Monokulo
     connection.
143. **A till not signed in to a point of sale refuses a Square sale's
     refund rather than marking it refunded with nothing sent**:
     `TillRunner`'s `NoSource` (Square's, so a Clover sale is marked to
     refund in Clover as decision 141 has) throws `PosException` "Sign in to
     Square first", which the sale page shows as its refund problem. Before,
     its `Refunds.ByTill` did nothing and `Till` marked the sale refunded,
     on Wake-ups too.
144. **QA round 1 on the Clover till.** A registration taken back (a
     change of point of sale, a sign-out, another Bell) takes its Last
     wake-up with it, so Settings › Wake-ups starts again at "None yet"
     rather than showing the old point of sale's ding under "Checking
     Square". The screenshots are acceptance T6.2's set, from QA's run
     against tools/fake-clover (the dark Clover card from a run of the same
     build):
     [pos-choice](../screenshots/android/pos-choice.png)
     ([dark](../screenshots/android/pos-choice-dark.png)),
     [clover-signin](../screenshots/android/clover-signin.png)
     ([dark](../screenshots/android/clover-signin-dark.png)),
     [clover-connected](../screenshots/android/clover-connected.png)
     ([dark](../screenshots/android/clover-connected-dark.png)),
     [settings-pos-clover](../screenshots/android/settings-pos-clover.png)
     ([dark](../screenshots/android/settings-pos-clover-dark.png)),
     [refund-in-clover](../screenshots/android/refund-in-clover.png)
     ([dark](../screenshots/android/refund-in-clover-dark.png)) and
     [wakeups-clover](../screenshots/android/wakeups-clover.png)
     ([dark](../screenshots/android/wakeups-clover-dark.png)); beside them
     the sale with its Clover receipt
     ([clover-sale](../screenshots/android/clover-sale.png)), the refund
     not seen yet and then seen
     ([clover-refund-not-seen](../screenshots/android/clover-refund-not-seen.png),
     [clover-refunded](../screenshots/android/clover-refunded.png)) and
     Settings after changing to Square
     ([settings-pos-square](../screenshots/android/settings-pos-square.png)).
145. **An item sold by measure counts as one item.** The heads-up
     notification, the sale's order card and the order page count what was
     bought from the receipt (decision 63: "15 items"). The count added
     every line's quantity and rounded, which Square's whole quantities
     never showed; Clover's measured lines did, and "2 × Flat white,
     2.5 × Granola" read "5 items". A whole quantity now counts as that many
     items and a quantity that isn't whole as one, whatever it measures, so
     the same order reads "3 items": what the customer takes away. A
     measured quantity that happens to be whole (2.000 kg) counts as two,
     since a receipt line carries no unit; recorded rather than worth a
     field. `OrderText.lines` ("8 lines") is unchanged. (QA round 2.)
146. **A sign-out forgets the application the sign-in went through.**
     `Stores.squareApplicationId` and `cloverApplicationId` (decisions 96,
     132) exist for the tokens: a refresh goes to the application that
     issued them, so the id must live exactly as long as they do. Only
     `forgetAll` cleared them, so after signing in through another Bell's
     application, changing Wake-ups back to the project's Bell and signing
     out of the point of sale, the plain "Sign in to Clover" went through
     the other Bell's application again and the project's Bell asked for a
     sign-in a second time. `signOutHere` now forgets the id with the tokens
     (a change of point of sale and Settings' "Sign out of …" both go
     through it), so the next plain sign-in is through the build's own
     application, or the one the chosen Bell's row asks for. Changing the
     Bell choice alone leaves the id, as the tokens it refreshes are still
     in use; that Bell's status says "Sign in needed" until the sign-in
     through its row. (QA round 2.)
## App: motion (2026-10-29)

Made by the motion team (the lead designer, the tech lead and two
developers) while making the till's transitions fluid, with the owner
away. The owner's brief: no animation for its own sake, blazing fast and
memory efficient, no hacky animation code, the code always scannable, the
design unchanged.

147. **Motion is specified once, in `docs/motion.md`, and what it rejects
    stays rejected.** The spec names the principles, the presets (one
    `ui/Motion.kt`, springs and durations by purpose, no numbers at call
    sites), the spatial model (where each surface comes from and goes
    back to), each opportunity with its interruption and reduced-motion
    behaviour, and a list of motions considered and cut with the reason
    (window animation styles, a hand-animated brightness ramp, anything
    on the QR's modules, staggered lists, count-ups, chip colour fades, a
    paid celebration, new animation libraries). A cut motion is not
    brought back without a new argument written beside its old one; a
    new motion is added to the spec before it is built. The spec's
    "before" clips (`docs/reports/motion/before/`) were recorded on the
    base commit so the showcase can put the after beside them. Two
    things the designer saw while recording are findings, not motion
    work, and go to QA: the lit bubble's pulse ring is clipped by its
    own window by about 4 dp (the window's padding is 12 dp, the pulse
    reaches 16 dp past the circle), and on a tablet the floating panel
    can cover the free bubble when the bubble sits at the end edge at
    the panel's height.
148. **The motion presets are one `@Immutable` value under the theme, and
    reduced motion is its `Still` twin.** `ui/Motion.kt` (beside
    `Theme.kt`) holds `class Motion` with the specs `docs/motion.md` §2
    names (`surface`, `settle`, `flick`, `sweep`, `fade`, `fadeQuick`,
    `pulse`, `glints`, `Dwell.paid`), two instances, `Motion.Full` and
    `Motion.Still` (spatial specs `snap()`, fades `fadeQuick`, loops
    off), and `LocalMotion`, a `staticCompositionLocalOf` that
    `MonokularTheme` provides from `rememberReducedMotion()`: a
    `ContentObserver` on `Settings.Global.ANIMATOR_DURATION_SCALE`, so
    "Remove animations" takes effect without a restart and the three
    composables that each do a Binder read on first composition today
    (`LoadingMark`, `Circle`, `PanelCard`) read the local instead. Call
    sites write `motion.surface`, as they write `mk.ink`; a number at a
    call site is a review failure. Chosen over Material 3's
    `MotionScheme` (1.4.0 has it) because every moving thing here is a
    custom composable or an overlay window, not an M3 component, and over
    a `MotionDurationScale` in the coroutine context because Compose
    already applies the animator scale to its frame clock on API 33+
    (which makes every spring and tween snap at scale 0 by itself); what
    the local decides is the *shape* of a motion under reduced motion, a
    crossfade instead of a transform, and whether a loop runs, which a
    scale can't say. Springs for anything with a position (they retarget
    from the current value and velocity); tweens only for opacity.
149. **Overlay windows are made per open, and all their motion runs inside
    the window.** Measured on the base commit (`docs/reports/motion/
    baseline.md`): the panel window costs about 2 MB PSS and one
    `ViewRootImpl` while it's up and gives both back when lowered;
    keeping it alive hidden would hold that, plus a full-screen surface,
    for every minute of a shift with a sale up, and a hidden touch-modal
    window would need its flags flipped on every show. So `addPanel` and
    `removePanel` stay the window's lifetime, but the panel's content is
    added at progress 0 and the window is removed only when the collapse
    (`docs/motion.md` M1, M2) has landed, by `removeView`, not
    `removeViewImmediate`, which stays only in `stop()`. One `Animatable`
    in 0..1 per panel window drives `graphicsLayer {}` (scale, alpha,
    pivot at the docked bubble), seeded with the grabber's release
    velocity, so a second tap mid-grow reverses from where it is. The
    only window-attribute animation is the bubble's x on release (it is
    a window, and `updateViewLayout` already runs per move event): an
    `Animatable` on `AndroidUiDispatcher.Main` with `Motion.flick` and a
    `VelocityTracker`'s velocity, one relayout per frame for about
    300 ms, replacing `ValueAnimator` and its duration formula. Window
    animation styles were rejected with the spec (not interruptible, no
    origin). The tablet's floating window stays `WRAP_CONTENT` and grows
    from its corner nearest the bubble (spec §5): a strip window spanning
    the panel and the bubble's rect would give a true origin and keep
    Square's touches outside the strip, at the cost of owning the taps
    between them; left as the option to take if the corner pivot reads
    wrong on the device.
150. **Screens change inside one `SharedTransitionLayout` and one
    `AnimatedContent`, and predictive back seeks the same transition.**
    `MainActivity`'s `when (screen)` becomes `AnimatedContent(targetState
    = screen)` with a `contentKey` that makes `Sale(a)` to `Sale(b)` and
    `Sale(id)` to `Code(id)` transitions, and a `transitionSpec` by pair
    (`docs/motion.md` M7); the code is a `sharedElement` with
    `ScaleToBounds` (laid out once at the target size and scaled as one
    layer, which is what principle 3 needs: no re-layout, no re-encode,
    a scannable code every frame), the white card and the sale's row are
    `sharedBounds`. `SharedTransitionScope` is still
    `@ExperimentalSharedTransitionApi` in animation 1.12.1; the opt-in
    lives in the one file that uses it, and the fallback if the API moves
    at a BOM bump is a hand-rolled bounds animation on `LookaheadScope`
    + `approachLayout` (stable), which is more code for the same frames.
    Back: `PredictiveBackHandler` (activity-compose 1.13.0, over
    navigationevent 1.0.0; the manifest already has
    `enableOnBackInvokedCallback`) feeds a `SeekableTransitionState` so
    the gesture scrubs the shrink and a cancel springs back; the
    `BackHandler` rules of decision 77 are untouched, the gesture changes
    no state before commit. No BOM bump: compose-bom 2026.09.00 resolves
    to animation and foundation 1.12.1 (`animateItem`,
    `SeekableTransitionState`, `SharedTransitionScope`), material3 1.4.0
    and activity-compose 1.13.0, at minSdk 26. The enlarged code's
    brightness is set once, on the first frame, as today: the display's
    own controller ramps a window override, and a hand ramp is a relayout
    per step; the lead checks the ramp on a device and only a visible
    jump earns at most six steps over the transition (M8).
151. **One paid dwell, 5 s, everywhere.** `Bubble.PAID_MS` (4 s: the green
    bubble and the chip's tick), the panel's `delay(5_000)` before it
    lowers and `TillNav.PAID_STEP_BACK_MS` (the sale screen stepping back)
    become one `Motion.Dwell.paid` of 5 s, the acceptance criteria's
    number, and the bubble's fade back to lit or idle is driven by the
    panel's collapse landing, not by its own timer, so the docked bubble
    no longer turns white a second before the panel it is docked to
    moves on (`before-overlay2.mp4` at 0:24). Done and Back end the dwell
    early and run the same exit. The dwell is reading time, so reduced
    motion leaves it alone.
152. **Motion is tested on the JVM where it is a rule and on the emulator
    where it is a frame.** The rules (`Panel.lowers`, the collapse's
    velocity seeding, the ring's sweep target, which spec `Motion.Still`
    substitutes for which) are pure functions in `Motion`, `Panel` and
    `PhaseRing`'s companions, tested in `app/src/test` as `Panel.lowers`
    is today, with production values (no test-only specs). What happens
    over time (the code's node keeps its size through the enlarge, the
    ring's arc passes through a mid value and never a quarter mark, the
    panel's progress reverses on a second tap, nothing recomposes
    `QrCode` or `PhaseRing` per frame) gets Compose UI tests under
    `app/src/androidTest` with `composeTestRule.mainClock` (`autoAdvance
    = false`, `advanceTimeBy`), on `androidx.compose.ui:ui-test-junit4`
    and `ui-test-manifest`, androidx and test-only, run with
    `connectedDebugAndroidTest` against the round's emulators; no
    Robolectric (a new third-party dependency, and it stands in a fake
    `Choreographer` for exactly the thing being tested). CI stays
    `./gradlew build` on Linux this round; an emulator job for the
    instrumented tests is the lead's follow-up. Before and after numbers
    are taken with the same scripts on the same emulator instance in one
    session, compared as counts and ratios, never as milliseconds
    against a phone (`baseline.md`'s caveats), and the final numbers on
    a physical device.
153. **Six performance fixes ride with the motion work, each owned by the
    track that touches the file.** From `docs/reports/motion/baseline.md`:
    the lit bubble's pulse redraws its window at ~56 fps for as long as a
    sale is live, in two windows when the panel is up (dev1, with M4: the
    pulse runs only in the free bubble while a code waits and the panel
    is down); every countdown text starts its own one-second timer, six
    live rows meaning six recompositions a second (dev2, with M12: one
    clock per screen, tabular figures); `PanelActions` is rebuilt with
    seven lambdas on every state change and neither it nor `TillActions`
    is `@Immutable`, so the panel and the till's screens never skip
    (dev1 and dev2); the same code is encoded three times per sale and
    thrown away on every screen change, where a shared element wants one
    `ImageBitmap` (dev2 owns `Qr.kt`: a cache of the last few codes;
    dev1 uses it); the root of `MainActivity` recomposes on every runner
    update and would hand `AnimatedContent` new lambdas per poll (dev2:
    `derivedStateOf` on what each screen reads); the notification's
    large-icon code is encoded on the main thread in the service's
    collector (lead). Not a motion issue but measured on the way: JNA's
    `libjnidispatch.so` ships for four ABIs the app doesn't build for,
    about 0.5 MB of the release APK, an `abiFilters` line for another
    day.
154. **A preset is generic over what moves, and its thresholds are its
    own** (refines 148). A Compose spec is typed by its value
    (`FiniteAnimationSpec<Float>` for a progress, `<IntOffset>` for a
    row's placement, `<IntSize>` for a pill's size, `<Rect>` for a shared
    bounds transform, `<Color>` for a ground), and `SpringSpec` has no
    variance, so one `Motion.surface` value can't serve them all. Each
    preset is a `Preset` (`ui/Motion.kt`) with `fraction` (0..1, finished
    within a thousandth: a spring on a full-screen surface never jumps
    its last pixel, where Compose's default hundredth would be a visible
    28 px on a Pixel 9 Pro), `px` (within half a pixel) and
    `spec(visibilityThreshold)` for the rest, so a call site names the
    kind of value and never a number. `Motion.Still` is a second instance
    of the same class rather than a flag read at call sites: its spatial
    presets are `snap()`, its `fade` the 120 ms tween, and its loops are
    null, so `rememberInfiniteTransition` can't be started by mistake
    (it needs the spec). The dwell is `Motion.Dwell.PAID_MS`, the repo's
    style for a constant with a unit; `Bubble.PAID_MS` and
    `TillNav.PAID_STEP_BACK_MS` are gone. Tested on the JVM with
    production values: the presets' numbers, the order of their settling
    times (a fade quicker than a move, a row before a window, a window
    before the ring's sweep, the sweep under a second), that the sweep
    never passes a quarter mark on its way, and `Still`'s substitutions.
155. **The foundation carries what both tracks need, so neither waits.**
    `codeBitmap(text)` in `ui/Qr.kt`, the last four codes as one
    `ImageBitmap` each (153's cache, moved from dev2 to the foundation
    because dev1's panel container and dev2's enlarge both share the
    element); the instrumented test harness (`ui-test-junit4`,
    `ui-test-manifest`, `AndroidJUnitRunner`, with `androidx.test:runner`
    1.7.0 and `espresso-core` 3.7.0 pinned because the ones
    `ui-test-junit4` brings reflect on `InputManager.getInstance` and die
    on API 36 before a test runs; `app/src/androidTest/` with the
    foundation's two tests: the theme provides the motion the system asks
    for, and one code is one bitmap); explicit `compose-foundation`
    and `compose-animation` dependencies from the same BOM, which the
    tracks' `AnimatedContent`, `SharedTransitionLayout` and `animateItem`
    import; and the lead's own fix from 153, the notification's large-icon
    code encoded off the main thread.
156. **Shared files have one owner each, and the other track asks.** dev1
    (overlay): `overlay/*`, `ui/PhaseRing.kt` (M6 is used by every
    surface but lives in one file; the overlay track has less on its
    list), `ui/Mark.kt` (PR #18's file), `TillService.kt`. dev2 (screens):
    `MainActivity.kt`, `AppViewModel.kt`, `SaleText.kt`, `ui/Till.kt`,
    `ui/Parts.kt`, `ui/Qr.kt`, `ui/Theme.kt` and the rest of `ui/`. The
    lead: `ui/Motion.kt`, `notify/`, the docs, the measurements and the
    integration branch. A change wanted in the other track's file is a
    message to its owner; the integration branch takes each track's PR by
    rebase, dev1 first.
157. **The phone's panel grows once it has drawn, and the free bubble
    comes back only where the collapse landed** (refines 149 and amendment
    6; dev1). The panel window goes up at progress 0 with its docked
    bubble placed exactly over the free one (hidden until its slot is
    placed, so its first frame isn't a second bubble somewhere else); the
    free bubble window goes on the frame after one is drawn that way, and
    only then does the grow start, so the docked bubble always leaves from
    where the free one was. Started on add, the emulator showed two
    bubbles for a frame or two while the grow ran ahead of the handoff.
    Lowered, the order is the other way from §11's first wording: the
    panel collapses first, the docked bubble riding the same progress back
    to the free bubble's place, and the free bubble is added at rest there
    once the collapse lands; the panel window goes after the free bubble
    has drawn. Adding the free bubble before the collapse would show it at
    the edge while the docked one is still in its slot. The docked
    bubble's slide rides the panel's `raise` (one `Animatable`, one
    spring) rather than a second `settle` spring, so the card's origin and
    the bubble never part; it draws over the card (`zIndex`), which grows
    out of it. The order is a pure state machine (`overlay/Windows.kt`,
    `Handoff.next`) tested on the JVM; "drawn" is a pre-draw listener that
    asks for one more frame once the slot is placed, so reduced motion,
    which asks for no frames, still hands over.
158. **The overlay owns the paid moment, and the collapse ends it** (M9's
    overlay half; refines 151; dev1). One `Job` in `Overlay` holds the
    dwell (`Motion.Dwell.PAID_MS`); the panel's own `delay` in `SalePanel`
    is gone, so there is one timer in `overlay/`. A pure `PaidMoment`
    (`None`, `Held`, `Lowering`; `overlay/Windows.kt`, tested on the JVM)
    says what happens: at the dwell's end the panel collapses into the
    still-green bubble when nothing else waits, and `justPaid` turns false
    only when that collapse lands, so the bubble fades to lit or idle after
    it, never under a paid panel; with another sale waiting the moment
    ends at the dwell and the next code comes up in the panel, the bubble
    fading to lit as it does; with no panel up the bubble is green for the
    dwell. Done, the grabber or a tap beside the panel during the dwell
    end it with the same collapse. The chip's tick keeps its own timer in
    `TillService`, started at the same instant (a notification can't
    follow a collapse). One behaviour goes with the panel's timer: a panel
    the cashier raises on a sale that was paid earlier (nothing else
    live) no longer lowers itself after five seconds; it stays until Done,
    as any panel the cashier raised does. The dwell is for a payment that
    lands, not for looking back at one.
159. **The panel's code folds into its hero in place; the order crossfades
    in for now** (M5, dev1, round 1). Live to paid or ended, the code's
    slot stays: its height follows one `motion.surface` progress in a
    layout lambda, the white card scales down into the 88 dp circle at the
    same centre in its own layer (laid out once at 264 or 280 dp, never
    re-laid out, held to that size on every frame by a device test) and
    fades under `fadeQuick` while the hero fades in; the next sale's code
    grows back out of the same circle. The words under it and the buttons
    crossfade (`AnimatedContent`, the words' height settling). What the
    middle shows is `Panel.face(sale)`, tested on the JVM. The code and
    the order swap by crossfade (the docked bubble going with the code),
    and on a tablet the card's height settles to the order's; the phone's
    card-to-window container transform with the corner radius 32 to 0
    from M5 is not in this round: it needs the card and the order to share
    one container whose bounds are animated in layout, which reshapes
    `PhonePanel` more than a round allows next to M1's handoff, and a
    crossfade already says "the same panel, another face".
160. **A screen's clock is a `LongState`, and a countdown is its own
     composable** (dev2, M12, amendment 10). `rememberNowSeconds()` at a
     screen's root ticks one `mutableLongStateOf` on each second's
     boundary and is handed down as `LongState`, not `State<Long>`:
     `State<Long>.value` boxes a Unix time on every read. The read happens
     inside `Countdown` (a restartable `Text` wrapper) or a live shift
     row, never in an inline `Column`/`Row`, which would make the read the
     enclosing screen's. Each screen's clock is a default parameter, so
     tests drive it and the panel's `OrderContent` makes its own. Under
     Still the status words swap with the quick fade, not a snap: section
     7 makes every crossfade `fadeQuick`, and a snap between "Waiting" and
     "Seen" is the hard cut it exists to avoid.
161. **Recompositions are counted in the instrumented tests, by the
     compiler's trace markers** (dev2). `Composer.setTracer` in
     `androidTest/Recompositions.kt` counts each composable body that
     runs (a skipped one doesn't), so "the shift doesn't tick", "the code
     isn't recomposed by the grow" are assertions that fail in CI on the
     emulator, not a Layout Inspector session someone has to repeat. Test
     code only; it opts in to `InternalComposeTracingApi`.
162. **A shared surface is enabled only during its own move, and the
     grow moves don't fade whole screens** (dev2, M7, M8). `Nav.move` is
     a pure rule from the pair of screens (and whether the till, not the
     cashier, changed it); `Modifier.sharedSurface(key, during)` passes a
     `SharedContentConfig` whose `isEnabled` asks the running transition,
     at layout time, whether the move is `during`, so a sale that came up
     by itself rises instead of growing out of the row that appears for
     it in the same frame, and the sale root's surface stays off while
     its code or order card grows. Every surface is `scaleToBounds`
     (FillWidth, top): laid out once at its screen's size and scaled as a
     layer, never remeasured per frame. In `RowGrow`, `OrderGrow` and
     `CodeGrow` the screens themselves don't fade: the surface does the
     one crossfade inside its bounds and the screen leaving stays under
     it until it lands. On the software-GL emulator three full-screen
     alpha layers a frame (both screens fading plus the surface) cost
     more than the rest of the frame. The QR image's surface doesn't
     fade at all: the same bitmap on both sides, so one solid code shows
     on every frame. Settings' fade-through overlaps (exit `fadeQuick`,
     enter `fade` from 0.96) rather than waiting for the exit: a "then"
     would be a delay.
163. **Back is the activity's, so the gesture can preview it** (dev2,
     M7, decision 77 unchanged). The enlarged code's and the order page's
     own `BackHandler`s moved to the root, which calls the same `back()`.
     From a sale, its code, its order page and Settings, a
     `PredictiveBackHandler` seeks the stage's `SeekableTransitionState`
     toward `AppViewModel.backTarget()` (where `back()` would go, with no
     side effects), springs back from wherever it is on cancel, and calls
     `back(from)` only after the commit animation lands. Under Still it is
     the plain `BackHandler`.
164. **The emulator can't pass "no frame over twice the old p95" for a
     full-screen container transform, and that isn't a regression to fix
     there** (dev2, measured on emulator-5556, SwiftShader, host load
     20–50). A cut draws one or two frames a switch; a grow draws ten to
     fifteen, each rasterising both screens and the surface in software.
     Composition and layout per frame stay at the cut's level (anim p50
     2–19 ms against 5–12, layout p50 0.3–4.6 ms); the raster doubles.
     The code enlarge passes (p95 138–151 ms against 151, max 217 against
     a 302 limit); shift to sale doesn't (p95 434–630 against 209). Native
     heap steps up about 10 MB on the first transitions and then holds
     (+1.3 MB over ten more round trips): on SwiftShader the transition's
     layers are native memory. Both are to be measured on a device before
     anything is changed for them.
165. **Round 1 is integrated by rebase, dev1 first, and the screens track's
    decisions are renumbered 103–107** (lead). Both tracks wrote decisions
    from 100 in their own worktrees; the integration branch keeps dev1's
    100–102 as written (its code comments cite them) and carries dev2's
    five after them, so dev2's "100" is 160, its "104" is 164. Nothing
    else conflicted: the two tracks touched no file in common.
166. **The tablet's free bubble stops pulsing while the panel is up beside
    it** (lead, review of O3; refines 149's pulse rule). `Panel.pulses`
    already said "not with the panel up", but every free bubble was handed
    `panelUp = false`: on a phone that bubble is gone while the panel is up,
    on a tablet it stays beside the floating panel and redrew its window
    every frame for as long as the cashier read the panel. The bubble's
    window now collects the overlay's `panelOpen` and passes it to
    `BubbleFace(panelUp)`; the docked bubble's `pulse = false` is the same
    rule from the other side. A device test holds a Lit, Waiting bubble with
    the panel up to no halo.
167. **An overlay window's coroutines end with the window** (lead, review
    of O6). `BubbleFrame`'s edge snap runs in the overlay's scope (it moves
    the window's own layout params, which the window's composition can't
    outlive), so a bubble removed mid-snap kept animating a detached view,
    one caught exception a frame, until the spring landed. The frame
    cancels its snap and recycles its velocity tracker in
    `onDetachedFromWindow`. The rule for the rest of `overlay/`: a job that
    moves a window is cancelled where the window goes, and anything in a
    `Presence` already dies with its composition.
168. **After numbers come from before and after run alternately on one
    emulator in one session** (lead, measurement). The host's load ran
    from 20 to 170 during the round (three other emulators, Gradle, a Rust
    build), so a number taken at one hour against the baseline's taken at
    another says nothing. `ab.sh` installs the baseline's debug build and
    the integrated one in turn over the same data and runs the same flow
    twice each; the pair under the same load is what the verdict rests on,
    and the baseline's files are left as they were. A flow that draws more
    frames because it now animates (a grow where there was a cut) is read
    by its per-frame cost and its longest frame, not by its frame count.
169. **A back scrub is linear in distance; the spring belongs to the
    release** (designer, round 1 review of M7). The gesture seeks the
    stage's transition by a fraction of its duration, and the surface's
    bounds move on `Motion.surface`, a spring that covers most of its
    distance in its first third: on the phone a finger a third of the way
    across showed the shift complete with the sale already folded to its
    row, on the tablet a quarter of the way showed nothing moved. Neither
    is a preview. While the stage is being seeked, every spatial child
    (the shared surfaces' bounds, the slides) follows a linear track of one
    `Motion` duration, so progress 0.3 is 30 % of the way; the release,
    commit or cancel, animates the remaining fraction with
    `Motion.surface` seeded from the gesture (`SeekableTransitionState.
    animateTo(target, spec)`), which is where the physics was wanted. An
    uninterrupted move drives the same linear track with the same spring
    on its fraction, so the two paths cannot drift apart. Frames in
    `docs/reports/motion/review-r1/phone-back-scrub-hold.png`.
170. **The code's own surface is above every ground, and the ground
    carries no code** (designer, round 1 review of M8; sharpens
    principle 3). On both emulators, light and dark, the first frames of
    the enlarge show the enlarged screen's layout scaled into the card's
    width and clipped by it, a cropped code with no quiet zone, and then
    the whole-layer code jumps ahead of the white ground and hangs over
    its edge (`review-r1/phone-code-grow-frames.png`, `tablet-code-grow-
    frames.png`). "Scans on any frame" means the one bitmap, square, with
    its quiet zone, inside a ground that moves in step with it. The rule:
    `code/<id>` has a higher `zIndexInOverlay` than `code-card/<id>`; a
    ground's shared layer never includes the bitmap (what it scales is
    white, the heading and the foot); the two bounds run on the same
    track so they land together; the ground's corner radius follows the
    same progress from the card's to the screen's rather than snapping
    square on the first frame. A device test seeks the transition to a
    quarter, a half and three quarters and checks the code's node is
    square, un-clipped by its ground and no smaller than at rest.
171. **Overlay timing is read from framestats and screencaps, never from a
    recording on the software emulator** (designer, measurement). Every
    `screenrecord` of the phone and the tablet showed the panel appearing
    0.8–1.2 s after the tap, yet `dumpsys gfxinfo framestats` put the
    panel window's first frame 50–84 ms after the input command returned
    and its last at 404–443 ms, and a `screencap` 300 ms after a lower
    caught the collapse mid-way. The emulator's recorder lags overlay
    layers by up to a second and the in-app layers by less, so a
    recording can order events but not time them; it is used for what
    moves, and `framestats` for when.
172. **The panel's face is one `Transition`, and each side of it keeps
     the sale it showed** (dev1, round 2 review of M5). The fold, the
     code's and the hero's fades, the words and the buttons all follow one
     `updateTransition` of the face and its sale, keyed by the face, so
     they start on one frame; the side that is leaving keeps the sale it
     was shown for, so the code's status row fades out still saying
     "Confirming" with its ring in its own colour, never "Paid" (it said
     Paid for the length of its fade before: the old content read the new
     sale). The words fade through rather than across, with a new preset,
     `Motion.fadeThrough` (`fade`'s length, starting once `fadeQuick` is
     done): two different labels in one place ("Copy address" and "Done")
     drawn over each other at half alpha read as a smudge, and a
     crossfade is for two faces of the same thing. The words' size
     settles with `clip = false`, so the next sale's "Waiting for payment"
     is never cut to the width of the "Paid" before it. The panel ticks
     one clock (`rememberNowSeconds` at its card), so the last caller of
     the per-text `secondsLeft(expiresAt)` timer went, and the timer with
     it.
173. **A phone's order grows out of the panel's card, a clip in its draw**
     (dev1, round 2; completes M5, replaces the crossfade of 159). The
     order is laid out once at the window's size and drawn inside a
     container that is a rounded rectangle from the code card's bounds
     (corners 32 dp) at 0 to the whole window (square) at 1, on one
     `motion.surface` progress of the page's `Transition`; the ground is
     drawn in the same clip, the code and its docked bubble fade out under
     `fadeQuick` and the order fades in with `fade`, and Back to the code
     is the same, reversed. The container is a clip, not a bounds
     animation, so no frame of it measures or lays anything out, and the
     path is made once per size and rewound each frame (built through
     `android.graphics.Path.addRoundRect` with four floats, which
     allocates nothing, where Compose's `RoundRect` would allocate one a
     frame). The container has no shadow: it starts exactly over the card,
     whose own shadow fades with the code under it. Under Still the
     progress snaps and the two crossfade (`fadeQuick`). The tablet keeps
     159's card whose height settles to the order's.
174. **A code whose time ran out is ended by the clock** (dev1, round 2
     review; both tracks' surfaces). Monokulo expires an order at its
     `expires_at`, but the till only heard so from the stream, so a stack
     that never sent it (the dev server's fake orders) or a stream that was
     down left a sale "Waiting · 0:00 left" for as long as the shift ran:
     the bubble pulsed for it, the panel's focus moved to it after every
     payment, and its dead code was offered as the next thing to scan. The
     runner now waits for the next live code's `expiresAt` and calls
     `Till.lapse()`, which moves every live sale past its time to Expired
     as Monokulo's own word would; the rule that only a payment moves an
     ended sale (87) means a server late to agree can't bring it back, and
     a late payment still pays it. One wait at a time (`collectLatest` on
     the next expiry), no polling. The dev server's fake orders still
     don't expire on their own; that is the other repository's, and the
     till no longer depends on it.
175. **A tablet's panel sits beside the bubble, and grows out of the
     bubble's side of it** (dev1, round 2; R1-O4, replaces the overlap QA
     logged under 147). With the bubble on the end edge the floating panel's
     window now stands off that edge by the bubble's window and an 8 dp gap
     (`Panel.floatingInsetDp`), so the card is next to the bubble and no
     frame has the bubble under it; with the bubble on the other edge the
     panel keeps to its own edge as before. The card grows from its side
     nearest the bubble at the bubble's height (held to the card), not from
     a corner, so the origin is literally beside the bubble. The plan said
     to slide the bubble to the panel's near corner on `settle` and back on
     lower; that would be a second window-attribute animation, which
     performance rule 9 allows only for the bubble's release, and it would
     move the thing the cashier placed. Moving the panel's window once, when
     it is made, costs nothing per frame. A bubble dragged to the other edge
     while the panel is up moves the panel's window once, on the snap's
     landing.
176. **A payment buzzes once, from the surface in front** (dev1, round 2,
     P2; §6). `PaidBuzz.bubble(appVisible)` is the overlay's half of
     the rule (the overlay reads it from the screens track's `PaidBuzz`,
     which names the one surface for either answer; the track's own
     `Haptics.bubbleConfirms` went in integration, two spellings of one
     rule): the free and the docked bubble buzz for a payment only while
     Monokular isn't on screen, and the answer reaches `BubbleFace` as a
     parameter from the overlay's state, so no composable under a window
     reads the app's graph. With the app in front its sale screen is
     the one the cashier is looking at and buzzes (S6, the screens track,
     reads the same rule from the other side), and a bubble still on its
     way out, or a tablet bubble the app came up beside, stays quiet. A
     device test counts the haptics a payment performs on the bubble with
     the app away (one) and in front (none); the test with the sale screen
     composed beside it waits for S6.
177. **The stage's progress springs, and every part of a screen change
    rides it on a straight line** (dev2, round 2; implements 169).
    `Motion.track` (a linear 450 ms, about as long as `surface` takes to
    settle) is what the shared surfaces' bounds, the rises and the slides
    are timed on, and `Motion.stage` (`surface.fraction`, null under
    Still) is what moves the stage's progress: `animateTo(screen,
    motion.stage)` for a change the till or a tap makes, `seekTo` under
    the finger, and the same `animateTo` for the release either way. Two
    presets were added to `Motion.kt` for this, the only way a seek and a
    spring can share one physics. The gesture first seeks to 0 and waits
    two frames, so the screen back goes to is composed and laid out
    before the finger moves anything: a shared surface's track starts on
    the frame its target bounds are known, and a first seek at 5 % left
    every later point 5 % short (the device test caught it). Where back
    goes is read from the model as the gesture starts, not in
    composition. A `BackScrub` is held from the gesture's start until it
    has committed or sprung back, and the enlarged code keeps full
    brightness while it is held, so a cancelled gesture no longer drops
    the brightness and the bar colours for the length of the preview.
178. **The code's white is a ground with nothing in it, and the code is
    the top layer** (dev2, round 2; implements 170). The card's white and
    the enlarged screen's white are one `sharedGround`: an empty rounded
    rectangle, remeasured to its moving bounds each frame (nothing inside
    to measure) rather than a scaled screen, its corner radius going from
    32 dp to 0 on the stage's progress (`Stage.shown`, on the same track
    as the bounds), so both sides draw the same shape and the corners
    don't snap square. The bitmap is `code/<id>` at `Layers.CODE`, over
    every ground; the enlarged code's own card has no white (it sits on
    the screen's), because a second white card drawn at rest under the
    moving one made the ground look like two rectangles. The heading and
    the foot render over the ground while the move runs
    (`overSurfaces`) and fade on the second half of the arrival, the first
    half of the leaving, read in `graphicsLayer` with `ModulateAlpha`.
    Because both bounds run linearly on one progress, a rectangle inside
    one at both ends is inside it on every frame: `MotionFramesTest`
    seeks the grow and the shrink to a quarter, a half and three
    quarters and checks the code is square, no smaller than at rest and
    has pure white on all four sides (its quiet zone, on its ground) on
    every one. The same test writes every screen move's seeked frames for
    review (the `additionalTestOutputDir` the runner pulls), so a cropped
    code is caught in CI rather than in a recording.
179. **The order page fades in over the sale while its card grows**
    (dev2, round 2; the designer's review of M7's OrderGrow). Only the
    list is the order's surface; the page's own ground, bar, heading and
    "Show the code" bar were drawn at full opacity on the first frame,
    covering the sale while the card was still its sale size, so the move
    read as a page appearing and then a card stretching. The page now
    fades in (`fade`) over the sale, which stays under it, and on the way
    back fades out first (`fadeQuick`) over the sale returning beneath. One
    full-screen alpha layer for 200 ms, which the host-GPU emulator draws
    within the frame; the RowGrow and CodeGrow pairs keep their
    no-fade rule (162). A device test checks the frame at progress 0 is
    the sale itself.
180. **A sale paid on screen closes its code into the page's hero, and a
    payment is one buzz from the surface the cashier is looking at**
    (dev2, round 2; M10, M9's app half, §6). `SaleScreen` is one
    `AnimatedContent` over `sale.live`: the page fades in over the live
    sale (which stays under it, no second alpha layer), and the code
    card is a `faceSurface` with the page's 72 dp hero: its bounds close
    on `Motion.surface`, scaled as one layer, the QR fading under
    `fadeQuick` while the tick fades in, the clip's corners going from
    the card's 32 dp to the circle's on the same spring (one reused
    path, nothing allocated a frame). "Show the code again" is the same
    move reversed, and a change mid-morph retargets. The hero is also
    `badge/<id>`, a surface during `RowGrow`, so the paid page stepping
    back after the dwell shrinks its hero into the row's badge, and a
    paid row opened from the shift grows its badge into the hero. The
    amount crossfades rather than moving as its own element: one more
    shared element for a number the cashier has already read. The buzz:
    `PaidBuzz.by(appVisible)` names one surface, the app when it is on
    screen, else the bubble; the view model emits when a payment lands
    (a sale live at the last look is paid now) and the activity buzzes
    only while started, and the bubble's `Confirm` buzzes only when the
    rule says bubble, so a bubble still leaving as the app comes up
    doesn't buzz a second time (in integration this became the
    `confirms` parameter of 176, not a read inside `Confirm`).
    `leave()` cancels the paid step-back, so Back during the dwell is
    the only step back.
181. **The window's background is the theme's ground** (dev2, round 2;
    review finding on the dark-mode recreation). The app ran on
    `Theme.Material.Light.NoActionBar`, so the frame between a light/dark
    change recreating the activity and Compose's first draw was the
    light window background: one white frame in the dark theme.
    `Theme.Monokular` (light and night) sets `windowBackground` to
    `@color/ground`, which a JVM test holds equal to `LightTokens.ground`
    and `DarkTokens.ground`, so the colour role stays defined once in
    `Theme.kt` and the resource follows it.
182. **A surface's two faces crossfade per draw, on the stage's progress,
    never through an offscreen layer each** (dev2, round 2; the review's
    native-heap blocker). On the host-GPU emulator the GPU's resource cache
    after ten shift-to-sale round trips held 86 MB (84 MB of it purgeable
    scratch textures) against 2.3 MB before: `sharedBounds`' `fadeIn`/
    `fadeOut` are `graphicsLayer` alphas with the default compositing, so
    each face of a full-screen surface was drawn into a screen-sized
    offscreen texture on every frame of the move, and the cache kept them.
    The faces now draw straight into the overlay with
    `CompositingStrategy.ModulateAlpha`, their alpha a function of
    `SeekableTransitionState.fraction` toward the screen they belong to
    (the arriving face in over the first `Motion.crossfadeOnTrack` of the
    move, the leaving one out over the last), read in `graphicsLayer {}`,
    so a cancelled gesture fades back the way it came. The leaving face's
    exit is a `fadeOut` that holds alpha at 1 for the move: the
    `sharedBounds` contract needs an exit to keep a leaving face drawn, and
    one at full opacity costs no layer. After: 5.1 MB and 0.2 MB of GPU
    cache in two runs. `Motion.crossfadeOnTrack` (fade's share of track)
    is the third preset this round adds to `Motion.kt`.
183. **The shared surfaces cost 6–14 MB of native heap once, and that is
    recorded rather than fixed this round** (dev2, round 2). Measured on a
    host-GPU emulator after decision 182 took the GPU cache back to the
    baseline's: 2.6 MB is Skia's glyph cache (text in a scaled surface
    rasterised at each size it passes through, bounded by Skia's own
    budget), the rest a step on the first transitions that holds flat
    afterwards and that `RUNNING_CRITICAL` doesn't return. Neither is a
    leak in the till's code (nothing grows with round trips), and the
    obvious levers would cost what the motion is for: not scaling text
    means re-laying out each frame, which principle 4 rules out. Settling
    it needs a device: the owner's Pixel was attached, but installing on
    it needs their say-so. Until then the 2 MB PSS rule is not met on the
    emulator and the after report says so.

184. **The integrated build is measured against live codes, on one
    emulator at a time** (lead, round 2 integration). The shared dev
    stack held nine "pending" orders 9 to 66 minutes past their
    `expires_at`: Monokulo would have expired them, the fake stack never
    does. The baseline build shows such a code live for as long as the
    shift runs ("Waiting · 0:00 left", 174's bug) and raises its sale by
    itself, the integrated build ends it by the clock, so the two would
    have measured different shifts and neither would have found a live
    row to open. The dead orders were marked expired on the stack
    (`__dev/orders/<id>/expired`, what Monokulo itself says of them), each
    pass of the A/B rings up two fresh sales first (the stack's codes
    live 15 minutes, a pass takes about five), and the phone and the
    tablet run one after the other, since a sale rung up for one till
    comes up by itself on the other and would land mid-flow. `to_shift`
    now leaves up to forty sales before giving up, not twelve: with other
    tills ringing up on the same store, twelve was not enough.

185. **The integrated build is not signed off on memory: the native heap's
    step is named, and it is text rasterised at every scale a surface
    passes through** (lead, round 2 integration; sharpens 183). On both
    emulators, before and after alternately, the integrated build holds
    12 to 24 MB more PSS at rest than the baseline after ten shift ↔ sale
    round trips (native heap +11 to +15 MB), and a `RUNNING_CRITICAL`
    trim returns 1 to 3 MB of it. `heapprofd` on the phone emulator,
    attached before the process started, puts the step in two places:
    1.6 MB at start (more JIT code, the first glyphs), then 7.5 MB over
    the ten trips, of which 5.1 MB is Skia's glyph strike cache and text
    atlas (`SkStrike::prepareImages` and `GrDrawOpAtlas::addToAtlas`
    under `AtlasTextOp`; `dumpsys gfxinfo` shows the glyph cache at
    3.0–4.3 MB against the baseline's 0.2). That is the shared surfaces
    scaling as layers (`ResizeMode.scaleToBounds`): the text inside a
    moving row or sale is drawn through a different matrix on each
    frame, every scale is a new strike, and each glyph is rasterised and
    uploaded again at that size. The cache is capped, so it is a step
    and not a leak, but a trim does not return it. One run also held a
    14.3 MB purgeable GPU scratch texture (16.45 MB of GPU caches against
    2.3), the size of one screen in RGBA: a whole-screen offscreen layer,
    which the order page's `fadeIn` on `OrderGrow` (179) would make the
    way the faces' fades did before 182; it did not recur in the second
    run and is not yet pinned to that. The screens track owns the fix:
    either the text inside a moving surface fades at its resting size
    while the ground and the glyph scale (so no glyph is ever drawn
    scaled), or the moving surface is one offscreen layer only for the
    move's frames with its texture released on landing, measured for
    both heaps; and the order page's parts take their alpha per draw as
    the code screen's heading does (`overSurfaces`). Measured again with
    `mem-r2.sh` and `heap-r2.sh` (the lead's scratch folder), the 2 MB
    rule is judged on PSS after a trim.

186. **A Back that lands mid-move tells the model the screen it was
    really on** (designer, round 2 review; blocker, screens track). On
    the integrated build, tapping the code and pressing Back before the
    grow has settled leaves the till unable to go Back or enlarge the code
    until an on-screen button is used (reproduced three times on
    emulator-5558: `input tap <code>; input keyevent 4` in one shell, then
    Back twice and a tap on the code, all ignored; the header's arrow
    recovers it). `StageBack` reads `from = state.currentState`, which
    during a running move is still the *previous* screen (the sale), so
    after the gesture lands on the sale it calls `back(from = Sale)`,
    `screenState` is `Code`, nothing matches and the model stays on
    `Code` while the stage shows the sale. The rule: what Back is "from" is
    the model's screen at the gesture's start (`state.targetState`, the
    screen the model is on), never `currentState`; a gesture that starts
    while a move is running retargets that move from its current fraction
    (the seekable state already allows it) rather than seeking a new pair;
    and the stage and the model are never allowed to disagree at rest: a
    device test taps the code, presses Back within 100 ms, and asserts the
    model is on the sale and a second Back reaches the shift.
187. **Every screen's frame at rest after a move is the frame it has
    under Still** (designer, round 2 review; blocker, screens track). With
    motion on, the order page's list card is not on screen once
    `OrderGrow` has landed: the page shows its heading and the "Show the
    code" bar over an empty ground, and the list's nodes are missing from
    the semantics tree; under `Motion.Still` the same page draws its lines
    (`review-r2/phone-order-page-motion-vs-still.webp`, sale #1257 with six
    lines, emulator-5558, light). The seeked frames in `frames/order-grow-
    *.webp` show the card drawn mid-move, so the card's face or the surface
    it rides is left at zero once the stage is at rest. The rule, and the
    test that keeps it: `MotionFramesTest` adds a rest frame after each
    move and compares the semantics tree after a move has landed with the
    tree composed under Still for the same screen; they must be equal.
188. **Two different texts are never drawn over each other above half
    opacity, on any surface** (designer, round 2 review; major, screens
    track; generalises 172 from the panel to the screens). Decision 182's
    crossfade brings the arriving face in over the first 44 % of the track
    and the leaving one out over the last 44 %, so from 44 % to 56 % both
    are opaque and either side of that one is above 0.8. Where the two
    faces hold different words in the same place the result is a smudge:
    the code screen's foot over the sale's "Copy address / Back to Square"
    (`frames/code-grow-50.webp`, `-75.webp`, and a recorded frame in
    `review-r2/phone-code-grow-recorded-frame.webp`, where the heading sits
    over "Sale #1249" too), the order page's heading over the sale's
    (`frames/order-grow-25.webp`) and its three-line card's words over the
    full list's (`order-grow-50`, `-75`), and the paid fold, where the
    whole page dissolves over the live sale and an eight-line order shows
    two receipts at once (`review-r2/phone-paid-fold-dissolve.webp`,
    `frames/paid-fold-120.webp`). The rule: words that differ between two
    faces fade through, the leaving under `fadeQuick` on the first part of
    the track and the arriving over the last (`Motion.fadeThrough`), as the
    panel's do; a card whose content differs between its faces (the order
    card's three lines against the whole list) shows one content during
    the move, the arriving one laid out at the target and scaled as one
    layer; and in the paid fold only the hero, the heading line and the
    buttons change: the order card is the same card on both sides and
    rides `order/<id>` to its place instead of dissolving.
189. **The code's ground keeps the code's rest margin on every frame of
    the enlarge** (designer, round 2 review; major, screens track;
    sharpens 170 and 178). `frames/code-grow-75.webp` and `code-shrink-25`
    show the white only two or three pixels wider than the code on either
    side; the code's bounds and the ground's are not interpolating between
    corresponding rectangles (the code reaches the ground's width before
    the ground reaches the screen's), so the quiet zone collapses for the
    last quarter of the grow and the first of the shrink. The rule: on
    every frame the white around the code is at least the smaller of the
    two rest margins (the card's and the enlarged screen's), which linear
    tracks between corresponding rectangles give by construction, and the
    device test measures that margin rather than "some white on each
    side".
190. **A ring's two colours are two strokes, faded through, never a blend**
    (dev1, round 3; review: the olive ring on the next sale). The bubble's
    ring went from paid green back to the lit orange through
    `animateColorAsState`, so for the whole fade the arc was a colour that
    means nothing (`frames/phone-next-sale-q1.png`), and the same blend ran
    the other way on every payment. `PhaseRing` now draws the arc twice, in
    the fill and in the paid colour, each with its own alpha read in the
    draw: the colour going fades out under `fadeQuick` and the colour coming
    fades in under `fadeThrough`, which starts when the other is gone. A
    plain alpha crossfade of two strokes in one place would still be the
    same olive (one over the other is the blend), so the arc passes over
    the bare track for a few frames instead; the track is the ring's
    neutral, and the sweep keeps moving under it. It is the one ring, so
    the bubble, the panel's status row, the sale's pill, the enlarged
    code's foot, a row's badge and the close sheet's glyph all get it.
    `PhaseRingOnDeviceTest` reads the arc on every frame of Confirming to
    Paid to Waiting and fails on any colour that isn't the fill or the
    paid colour over the track.
191. **A card's shadow fades with the card** (dev1, round 3; review: the
    grey box left by the lower). The panel's grow and collapse fade the
    card per draw (`CompositingStrategy.ModulateAlpha`, so there is no
    window-sized offscreen buffer), and a parent's alpha drawn that way
    never reaches a child's elevation shadow: the renderer draws shadows
    with the casting node's own alpha only. So the card faded out of a
    shadow that stayed whole (`frames/phone-lower-q2.png`), a grey square
    over Square for the last frames of every lower. The card's own layer
    now casts its shadow from `16 dp × Panel.shown(raise)` in
    `DefaultShadowColor` at that same alpha, `Panel.shown` being the one
    rule the grow's fade already used (opaque by a third of the way). An
    offscreen layer for the fade was the other fix and the one ruled out by
    decision 182's reasoning: a screen-sized texture per window for a
    shadow. Every surface with a shadow under a per-draw fade gets the
    same: the phone's card, the tablet's floating card and its order card,
    and the free bubble's circle, whose window fades it in and out on
    appear and go (`BubbleFace(presence)`). `OverlayFramesTest` lowers the
    panel over white and fails when the band under the card is darker than
    the card's opacity allows (`frames/phone-lower-light-q*.png`); on the
    old build it read 0.14 dark under a card 5 % opaque.
192. **A payment code is never faded in on the panel; its words fade
    through around it** (dev1, round 3; review: the next sale's code at
    40 % then 85 %). Performance rule 6 and principle 3 say the code is
    never faded in, and the panel broke it in three places: the next
    waiting sale's code grew out of the paid hero fading in with
    `motion.fade` (`frames/phone-next-sale-q1.png`, q2), a tablet's code
    came back from the order inside `AnimatedContent`'s `fadeIn`, and a
    phone's did the same under Still, where the order's ground snaps away
    and nothing covered the fade. A customer's camera may already be on the
    panel when the next code comes up. Now the code's layer is opaque
    whenever its face or page is the target: the next code grows from the
    hero's size at the same centre, opaque, over the hero, and only the
    words (the grabber, header, amount, order line, status and buttons)
    fade, through `SalePanel(words)`, one alpha read in each of their
    layers, arriving under `fadeThrough` so the words leaving are gone
    first (188). On a tablet the code's page comes in under the order
    (`targetContentZIndex`), whole, while the order fades off it; the
    order's own entrance now fades through too, where it used to fade in
    over the panel's words. Under Still a phone's order fades its ground
    with its words, a 120 ms crossfade off a whole code, instead of an
    empty ground covering the code for the length of the transition. The
    code still fades out into a paid or ended hero, which decision 172
    allows: a paid code scans nothing. The raise's fade over its first
    third (M1) is unchanged: the code there is a third of its size and
    growing out of the bubble, and the spec chose it. `OverlayFramesTest`
    reads the code's darkest module on each seeked frame of those three
    moves and fails on a grey one; the old build read 0.62 at q1.
193. **The panel's order line opens its room as it fades in** (dev1,
    round 3; N3, the panel's half: the sale screen's order card and header
    line are the screens track's). A sale's receipt reaches the till about
    a second after the sale, so the panel's order line popped in, and
    because the line sits above the code on a phone's panel, which hangs
    from the top, the code jumped down a line's height (about 62 dp) in
    one frame, under a camera that may already be on it. N3 asked for a
    fade alone, without a layout per frame; on the panel that would keep
    the jump, so the line's room opens with `expandVertically` on
    `motion.settle` while its words fade in on `motion.fade`
    (`AnimatedVisibility`, nothing on screen at all when there's no
    receipt). That is a layout per frame for about 250 ms of the card's
    few rows: the code inside is placed lower each frame, never measured
    or laid out again (its slot's constraints don't change), so principle 3
    holds and the code slides rather than jumps. The 14 dp above the line
    is inside its room, so nothing steps on the first frame. A panel that
    comes up with its receipt already there shows the line at once.
    `OverlayRecompositionsTest.theLateOrderLine` counts zero recompositions
    of the card, its code, rings, status row and bubble across the move;
    `OverlayFramesTest` saves `phone-late-order-q*.png` and holds the code
    square and opaque on each.

194. **One driver keeps the stage on the model's screen, and a Back says
    what it left from the model** (dev2, round 3; implements 186 and the
    lead's stage/model desync finding). `FollowScreen` is the only caller
    of `animateTo` on the stage: it follows the model's screen with
    `Motion.stage`, stands aside while a back gesture holds the stage, and
    when the gesture lets go (committed or cancelled) sends the stage to
    wherever the model is by then. A run cut short by a gesture's seek
    (the transition's mutex cancels the run, not the driver) is picked up
    again when the gesture ends, so a sale coming up or a payment landing
    under the finger is followed, never dropped. `StageBack` takes what it
    goes back *from* from the model as the gesture starts, never from
    `currentState`, which mid-move is the screen before; it previews only
    from a settled stage, and a Back that lands during a move just tells
    the model, so the running move retargets from wherever it got to. On
    release the model is told at once and the stage follows it, rather
    than the model waiting for the stage to land. `StageBackTest` covers a
    Back 96 ms into the code's grow (the model goes back to the sale and a
    second Back reaches the shift), and a sale coming up under a
    cancelled and a committed gesture (the stage ends on the new sale,
    and Back still works from there).
195. **A moving surface is an empty ground, and the words on it fade
    through at their own size** (dev2, round 3; implements 185's first
    option, 188 and R2-O5; a look the designer should confirm, made while
    they were away). The row-to-sale and card-to-order moves no longer
    scale what is on them. A `sharedGround` (an empty rounded rectangle,
    its colour and corners going from the partner's to its own) moves
    between the two nodes, and each side's words are drawn at the size they
    were laid out at. They sit over the ground in the overlay, clipped to
    where the ground is on that frame (`words`, `GroundTrack`), fading out
    over the first 27 % of the track (`fadeQuick`'s share) and in over the
    last 44 % (`fade`'s share). So the two screens' words are never drawn
    together, and no glyph is drawn through a changing matrix. Only
    glyphs (the badge into the hero) and the code's bitmap still scale as
    layers. Under CodeGrow and OrderGrow the sale's own words fade out the
    same way. The order page has no whole-page fade: its ground steps
    aside during OrderGrow (the sale's is the same colour, under it), and
    its bar, heading and foot fade per draw. The paid fold's two faces
    fade through per draw (the live sale out under `fadeQuick`, the page
    in under `fadeThrough`), so an order never shows two receipts. A row
    knows its own corners (they follow its index) and the sale doesn't, so
    the row draws the ground both ways. What the cashier sees: the row's
    card grows into the screen's ground and the sale's words appear on it
    as it arrives, where before the row's text magnified into the sale's.
    `MotionFramesTest` asserts no text is drawn at a non-1 scale on any
    seeked frame of every move both ways, the Rise and the paid fold;
    `MotionTest` holds the two shares under the whole track.
196. **What clips a moving surface moves with it** (dev2, round 3; the
    paid fold's flat top, and 189's thin quiet zone). A shared element is
    placed with the bounds of the update before (one frame in a running
    move, a whole step after a single seek), while its overlay clip is
    computed with this frame's. So a clip from outside (the hero's
    `RoundClip`) cut the content wherever the two disagreed: the straight
    edge across the top of the hero at 180 to 240 ms was that clip's top
    edge, not the page's scroll (the fold was re-taken with the scroll
    removed, and the edge stayed). Likewise a ground remeasured to its
    bounds drew a step behind the code it holds: 189's 2 to 3 px of quiet
    zone on the seeked frames. Now a face clips itself, in its own
    coordinates, to a corner that is a share of its own side (the card's
    32 of 264 going to a circle's half). A moving ground is its overlay
    clip's shape, filled: the clip is current and the fill reaches past
    the layout. Ground, code and words now agree on every frame. A face
    surface also stays in the overlay for as long as its face is changing,
    out of reach of its page's scroll. `MotionFramesTest` seeks a hair
    short of each point first, as a running move's previous frame does. It
    measures the white round the code against the smaller of its two rest
    margins on every frame, and checks from 180 ms (once the card has
    closed to about the hero's size) that the hero is round with the
    page's ground just above it.
197. **A still stage has no move** (dev2, round 3; the cause of 187).
    `Stage.moving()` asked `Nav.move(current, target)` at rest too. For the
    order page that is Order to Order, the same order on both sides, which
    `Nav.move` calls OrderGrow. So the list's face kept OrderGrow's
    crossfade at the resting stage's progress, 0, and the page landed with
    no lines. The code screen had the same, harmlessly. `moving()` is now
    null when the current and target screens are the same.
    `MotionFramesTest` lands every move (RowGrow, CodeGrow and OrderGrow
    each way, the Rise and the paid fold), composes the same screen under
    Still, and requires the same semantics tree and the same frame (under
    0.1 % of pixels apart).
198. **A sale that comes up by itself slides up as it fades in, and a
    fade-through keeps its wait under Still** (dev2, round 3). The Rise
    scaled the whole screen from 0.94. The interrupt matrix caught the
    rising sale's code at 643 px against its 684 px at rest (principle 3),
    and it drew its words through a changing scale (195). It now rises
    from 4 % of the height below, so every frame is at full size. Under
    Still, `fadeThrough` keeps its 120 ms wait: it puts two different
    labels in one place, and a quick crossfade would overlap them. It
    takes 240 ms where a Still crossfade takes 120.
199. **What reaches a screen after it came up fades in** (dev2, round 3;
    N3). A receipt reaches the till about a second after its sale. The
    order card now fades in with `Motion.fade` (an `Animatable` read per
    draw: no layer, no recomposition per frame). On a wide window the
    column beside the code settles to its new height rather than jumping.
    The header's second line ("From Register 2" to "Front · this till")
    fades through. `LateReceiptTest` checks the card is half in at 100 ms,
    and that the code, the pill and the ring are not recomposed by the
    fade. The panel's order line is the overlay track's, asked of dev1.
200. **"Close it for now" goes to the shift; a sale that ends any other
    way while on screen folds into its page in place** (dev2, round 3;
    settles N4). The sheet's "Not sure: close it for now" is
    `CloseOutcome.Then.Shift`: the model leaves the sale at once
    (`closeSale`, then `leave`), so the stage shrinks the sale into its
    row, which already reads "Closed · not paid". That is what round 2's
    recording saw, and what the cashier asked for: back to the counter.
    There is no dwell on that path, since the dwell is reading time for a
    payment (M9). A sale that stays on screen as it ends ("No: refund it
    in Square", which is `Then.Sale`, or a sale ended by its clock or by
    its other till) folds its code into the ended hero on the warn ground
    in place, the paid fold's move. `MotionFramesTest` saves those frames
    at 60, 120, 240 and 300 ms and checks the hero is whole on each. M10's
    "a close from the sheet" is read that way.
201. **Round 3 is integrated as the overlay track's six commits, then the
    screens track's six replayed over them, each left a clean, buildable
    commit; the screens track's decisions are renumbered after the
    overlay track's; the rebase onto `origin/main` waits for round 4**
    (lead, round 3). Both tracks branched from `motion` at `3124bf5`, so
    `motion` fast-forwards to `motion-dev1` and `motion-dev2`'s commits
    are cherry-picked on top: no merge commits, and the one source overlap
    (`ui/CodeScreen.kt`: `CodeText.left` reading `countsDown`, and the
    screen's `words` modifiers) merged by itself. Both tracks numbered
    their decisions from 133, so dev2's 133 to 139 are 194 to 200 here,
    and every reference to them in `Nav.kt`, `Till.kt`, the tests,
    `motion.md`, `after.md` and `frames/README.md` is renumbered in the
    commit that adds it, not in a fix-up after. `origin/main` has moved by
    the wake-ups, bell-pos and bubble-loading-mark PRs since the base
    (`f274ad7`), and seventeen files conflict (`Faces.kt`, `Overlay.kt`,
    `Till.kt`, `Nav.kt`'s neighbours, `build.gradle.kts`): that rebase is
    the lead's first job of round 4, done once on the integrated branch
    rather than by each track against a moving target, with the loading
    mark put inside `Circle`'s `AnimatedContent` and the bubble's
    `presence` kept to its shadow (decision 191), before the PR opens.
202. **The 2 to 3 MB of PSS over the baseline at start-up is accepted; what
    the moves and the overlay add on top of it is still measured against
    the 2 MB rule** (lead, round 3; relayed by the workflow's coordinator
    on 2026-10-29 as the owner's acceptance, not heard from the owner
    directly). The screens track's round-3 run put the residual at
    start-up, before any move (Java heap +2.4 to +4.6 MB fresh: the larger
    app's classes and code), and the integrated run agrees (decision 201's
    report). So a start-up step of that size is no longer an issue or a
    blocker. The rule still applies to what a session adds: the memory
    after ten round trips and a trim is read as (after − before) less the
    start-up step, and the bubble's +7 MB of native heap with the panel
    lowered is the overlay track's open item. Measuring on the owner's
    physical phone was relayed in the same message as approved; the lead
    did not act on it, since installing on the owner's own device is the
    owner's call and the only words of theirs in this run do not mention
    it. The emulators' numbers stand, read against each other at the
    host's load, until the owner says so themselves.
203. **An arriving screen's ground is never painted whole while its move
    is running** (designer, round 3 review; blocker, screens track). On
    the integrated build every enlarge opens with one or two frames in
    which the whole window is white and the code is still small at its
    card's place, then a frame of the sale again, then the grow: the code
    screen's ground, which should exist only as its overlay clip's fill
    while `CodeGrow` runs, is painted at its layout size (the window) on
    the frames where the stage has the move but the shared match is not
    yet found, or the other way about. In the dark theme that is a
    near-black to white to near-black flash before every enlarge
    (`review-r3/phone-enlarge-white-flash-dark.webp`, f117 against f116
    and f118; the bars' icons flip on that frame too), and it is there in
    light, in landscape (`phone-enlarge-white-flash-landscape.webp`) and
    on the tablet; the shrink ends with the mirror of it, the leaving
    screen's full white over the sale's header for a frame. The same
    seam shows as the code drawn beside its ground rather than on it
    mid-move (`phone-enlarge-code-off-ground.webp` f261, f501;
    `tablet-enlarge-code-half-off-ground.webp` f297, f298). The rule:
    while a screen change runs, a ground draws through its overlay clip
    or not at all; a frame on which the clip has no bounds yet draws
    nothing for the arriving ground (the leaving screen is still whole
    under it), and the code's surface is always inside the ground's
    bounds on that frame, both read from the same update. The lead's
    first-enlarge frame is likely the same frame: a window-sized fill
    through a path clip on first use. The test that keeps it runs the
    move rather than seeking it (`MotionFramesTest` seeks; a running
    transition is what desynchronises the two), captures each frame of
    one `CodeGrow` and `RowGrow` with `captureToImage`, and fails on any
    frame whose ground area outside the clip's bounds is the ground's
    colour, or whose code's darkest module lies outside the white.
204. **A moving ground never cuts a glyph** (designer, round 3 review;
    major, screens track; sharpens 195). On the shrink the sale's amount
    and pill fade in over the last 44 % of the track, under a ground
    still larger than the card, so the ground's edge sweeps through
    "$9.80" and through the pill's words with a hard line
    (`review-r3/phone-shrink-ground-cuts-words.webp` f352, f353; the
    tablet's shrink cuts the amount vertically). The rule: words that
    arrive on a screen while a ground is still moving over their place
    are drawn above the ground (`Layers.OVER_GROUND`, as the row's words
    are) so they fade in over white and then over the screen's colour,
    or they start their fade only once the ground's bounds no longer
    meet theirs; either way no frame draws a text run half under a
    ground's edge. The leaving words under an arriving ground are fine:
    they are going, and something is covering them.
205. **A payment code is never drawn below full alpha on any screen's
    arrival, the Rise included** (designer, round 3 review; major,
    screens track; extends 192 from the panel to the app). The Rise fades
    the whole sale in (`Nav.transform`: `fadeIn(fade)` with the slide),
    code and all, so for its first 200 ms the code is a grey code on the
    shift, and under Still the enlarge crossfades two codes of two sizes
    over each other (`review-r3/phone-still-enlarge-two-codes.webp` f281,
    f282). The panel's rule holds here for the same reason: a customer's
    camera may already be on the till when the sale comes up. The Rise's
    fade is for the ground and the words; the code's surface
    (`Layers.CODE`) rides the same 4 % slide at alpha 1 from its first
    frame. Under Still, the code screen's white and its code come in
    opaque in the first frame and only the words crossfade, as the panel
    does (192). `MotionFramesTest` reads the code's darkest module on
    every frame of the Rise and of the Still enlarge, as
    `OverlayFramesTest` does for the panel.
206. **A row opens into its sale as one surface on every frame; nothing
    dissolves** (designer, round 3 review; blocker, screens track). On
    both emulators, light and dark, a tap on a live row draws the row's
    ground grown over the shift for a frame or two, then the shift whole
    again at rest, then the sale's words and code fading in over the
    shift's rows, code at half alpha through "Sale #1384 · $9.80"
    (`review-r3/phone-row-grow-dissolves.webp` f418 to f421;
    `tablet-row-grow-dissolves.webp`). The move the cashier sees is a
    crossfade with a ground that gives up, not the container transform of
    M7; the seeked frames never showed it because a seek holds both sides
    in one state. Read with 203, the shape is a `RowGrow` whose ground
    stops drawing (its `isMoving(RowGrow)` or `isMatchFound` goes false
    mid-move) while the arriving content completes under a different
    transform. The rule: `Stage.moving()` and the `AnimatedContent`
    transform agree on the move for the whole of one change (one read of
    `cameUpByItself` per change, held by the stage, not re-read per
    frame), the row's ground is held in the overlay until the sale's own
    ground has landed, and the sale's code is at alpha 1 from its first
    frame (205). The running-move test of 203 covers `RowGrow` both ways
    and fails on any frame where neither the row's ground nor the sale's
    is drawn, or where the shift's words and the sale's are both above
    half alpha (188).
207. **A raise the cashier asked for is not undone by a poll** (designer,
    round 3 review; major, overlay track; the lead's `usable` item,
    reproduced). On the tablet a tap on the bubble grew the floating
    panel to about three quarters, held it there for about 200 ms,
    collapsed it to half and grew it again to full
    (`review-r3/tablet-panel-raise-dips.webp` f56 to f63), on a host at
    load 50: `Overlay.show` lowers whenever `usable` flips, and a poll
    that times out flips it. A dip in a raise is noticed as the panel
    "catching"; on a counter with a slow connection it would be every
    raise. The rule: a timed-out poll alone does not lower a panel; the
    overlay lowers for `runner.disconnected` only once the runner has
    given the connection up (its own retry budget spent), and never
    within the first second after a raise, while `appVisible` still
    lowers at once. The log line the lead asked for on `usable`'s
    changes goes in with it, and `OverlayStateTest` raises, flips a
    single poll timeout, and asserts the panel's `raise` is never
    retargeted below 1.
208. **No poll lowers the overlay; decision 207's dip is not a lower**
    (dev1, round 4; answers 207 and the lead's `usable` item). `usable`
    reads `runner.running`, `runner.disconnected`, `appVisible`, the
    cashier's switch and the overlay permission, and a failed poll or a
    dropped stream changes none of them: `TillRunner.follow` sets
    `disconnected` only on the store's 401, which ends the till, and
    `running` changes only on start and stop. A lower for `usable`
    also clears `panelOpen`, so it could not grow again by itself as
    the review describes. Reproduced on 5554 with the floating panel
    (`wm density 240`, smallest width 853 dp) at load 47 to 90: one
    `Want` per tap and one monotonic grow, three cycles, logged; the
    review strip's f56 to f61 are one held frame of the grow (the
    renderer gave no new frame at load 50), then f62 at rest. So
    there is no retry budget or one-second guard to add: either would
    be a rule for a case the code cannot reach, and a 401 should
    still lower at once. What went in: the rule as one pure function
    (`Handoff.usable`), a JVM test that a failed poll leaves the
    overlay usable and a raised panel's next `Want` asks for nothing
    (no close, no reopen) on a phone and a tablet, and the debug
    build's log line on each change of `usable` with every input
    (tag `MonokularOverlay`), so the next unexplained lower names its
    cause. On a quiet host the wide raise and lower measured 119
    frames, p50 16.9 ms, max 19.2 ms.
209. **The phone's order rides its container's top as it grows** (dev1,
    round 4; the review's minor item on the order page's empty card).
    The order is laid out once at the window's size and only its clip
    grew from the card, so its heading sat above the clip until the
    container reached it: the card was an empty ground from 27 % of the
    move to 56 %. It is now placed (an `offset {}` read in layout, no
    relayout) at the container's top edge on every frame, so the order's
    words arrive at the card's top as soon as they start fading in, and
    settle to the window's top as the container does. No timing change
    and no new layer, so 188's rule (two faces never both above half
    alpha) stands as it was; `PhoneOrderOnDeviceTest` holds the heading
    at or below the card's top on the grow's first frames and only
    rising after.
210. **The panel's card is one layer only while it grows or collapses**
    (dev1, round 4; answers the review's overlay half of the memory
    item). `heapprofd` on 5554 over a cold start, one sale, the bubble
    up and the panel raised and lowered five times, before (f274ad7's
    build) and after alternately, attached before the process started:
    the round 3 head ended with 18.4 MB of unreleased native heap
    against the baseline's 14.5, and 3.4 MB of the 3.8 MB difference was
    Skia's glyph strikes and text atlas (`SkStrike::prepareImages`,
    `GrDrawOpAtlas::addToAtlas`, FreeType glyph loads under
    `AtlasTextOp`). Not the card's shadow, not the `ModulateAlpha`
    layers, not the order line's `AnimatedVisibility`: the raise scales
    the card from 0.3 with `graphicsLayer`, so every word on it is drawn
    through a new matrix each frame and Skia rasterises each glyph again
    at that size, the same cause as 185 in the app. `TextMotion.Animated`
    was tried first and kept 3.3 MB of it. What went in: the card's
    content goes through an `Offscreen` layer while `raise` is strictly
    between 0 and 1 (`Presence.moving`, read in the layer block, so no
    recomposition) and draws straight through, with no offscreen
    buffer, at rest; the phone's order, which rides the same raise,
    does the same. Each glyph is drawn once at its
    own size into the layer and the layer is scaled; its texture goes
    when the panel lands. It sits inside the card's clip and under its
    shadow, so neither is cut. After: 14.1 MB unreleased (0.4 MB under
    the baseline), PSS after the flow 104.2 MB against the baseline's
    103.5 and the round 3 head's 109.5, native heap 13.2 MB against
    14.1 and 18.4 (one pass each at load 28 to 70; the scripts are
    `heap-ov.sh` and `bin/heapdiff.py` in dev1's scratch folder). The
    code is in the layer during the move, scaled as part of it as it
    already was (§10 rule 6 unchanged: one bitmap, never re-laid out).
211. **A running move is checked on the window drawn there and then,
    and on its ground's travel, not on how alike its frames are**
    (dev2, round 4; the harness of 203 and the review's R3-O1 to R3-O3).
    `RunningMoves` drives the real stage (`ModelStage`) on the test
    clock a frame at a time and keeps every frame of `RowGrow`,
    `CodeGrow`, `OrderGrow`, the Rise, the paid fold and the paid
    step-back, both ways, light and dark. Each frame is the window drawn
    in software on the main thread from the state that frame left, not
    `captureToImage`: a copy of the window's last buffer was a frame
    behind the stage on one run and not on the next (the white at the
    card's size with the words already fading), which would make the
    test flaky in exactly the frames it is for. Per frame it asserts:
    both screens' stages name one move; the moving ground has a shape on
    every frame between its first and its last; the code's white is
    painted where its track says, with no grey (a code below alpha 1) in
    it; no arriving glyph is cut by a ground's edge; the ground's area
    goes one way and never stands still two frames short of its place
    (the "hold then jump" of round 3); in dark, no frame is lighter than
    both ends and none steps by more than the whole move (the flash
    detector); in the fold, the hero travels without standing still and
    the page's heading shows only once the hero is past halfway (188).
    A whole-picture "only goes toward the last frame" check was tried
    and dropped: in light the pill and order card are white like the
    code screen, and by 204 they fade out under the white and back in
    once it has passed, so a correct shrink turns back toward its first
    frame. A hard cut steps once by the whole difference, which the
    detector allows: under Still that cut is the move (214); with motion
    on, the cuts of f274ad7 are the jump the moves exist to replace.
212. **Words leaving stay under a ground growing over them; only words
    arriving are lifted above it** (dev2, round 4; 204's last line made
    code). `words(lift = …)` lifted on the move alone, so on the enlarge
    the sale's pill, fading out, was drawn over the growing white's
    bottom edge. A lift now needs the screen to be the one arriving.
213. **A change's move is decided once, and a screen left draws nothing
    on the frame its change lands** (dev2, round 4; 206's rule). The
    stage holds the move for the pair of screens (`HeldMove`): the first
    to ask, the transform or a surface drawing, reads `cameUpByItself`,
    and every reader gets that answer until the stage is at rest. The
    landing frame still composes the screen left (its exit holds until
    the transition finishes), and the shrink drew it whole over the
    screen arrived for that one frame; `Stage.gone()` stops it drawing.
    A ground whose move has no shape on a frame (the match a frame late
    or a frame early) is drawn whole only by the screen that is the
    picture on that frame (half shown or more), never by the arriving
    one at its layout size (203), and the code's surface likewise.
214. **Under Still, a change to or from a screen that may show a code is
    a cut** (dev2, round 4; 205). A crossfade there put two codes of two
    sizes through each other; the cut has the code and its white whole
    on the first frame. Other Still changes keep their short crossfade.
215. **The words' fade makes a layer only while one of its moves is
    under way** (dev2, round 4; review R3-O5). `Modifier.words` was a
    `graphicsLayer` at alpha 1 on every call site at rest, one per shift
    row. It is now a `Modifier.Node` that reads the stage in placement
    (so it places again as a move starts and lands, nothing per frame)
    and places with a layer only during its move, the alpha read in that
    layer's block per frame. Its moves are a set (`through`, `lift`), so
    the API has no `alsoThrough`.
216. **The bars' icons turn at the code screen's half, the brightness on
    its landing** (dev2, round 4; review minor, M8). They were one effect
    on `up()`. The icons now follow a `derivedStateOf` on the code
    screen's shown fraction crossing one half (two recompositions a
    move), so in dark they turn as the white passes the middle, not on
    the first frame or the last; the brightness still waits for the
    white to cover the screen.
217. **A moving ground's fill reaches past its layout by its last step,
    not by four of its sizes** (dev2, round 4; the cause of 203 named).
    The ground is an empty box scaled to its moving bounds, and the
    overlay clips it to this frame's bounds while its scale is an update
    behind; to cover the clip it filled four of its own sizes each way
    (`FILL = 4f`). On the emulator with the host GPU, in the real app,
    dark theme, the enlarge's first frames drew that fill with no clip:
    the whole window white with the code small at its card's place, then
    the sale, then the grow (`dev2-r4-flash.png`; 2 of 3 enlarges, every
    process). The running-move harness never showed it: the test clock
    composes, clips and draws the first frame in one pass. With the fill
    at its own size the flash went in every recorded enlarge; with it
    gated on `isTransitionActive`, or on the track having a shape, it
    stayed, so the overlay draws the ground unclipped on a frame where the
    track is already set. The fill now reaches past its own size by
    twice the distance its bounds moved on the last frame, at the scale
    it is drawn (`GroundTrack.reach`): enough to cover the update it
    trails by, nothing on a first frame, and at worst a step's band if
    the overlay ever draws it unclipped again. Checked by luminance over
    every recorded enlarge (no frame lighter than its neighbours by more
    than 40 of 255), not by a device test: the frame only exists with a
    real Choreographer and render thread. The same fill is the likeliest
    of the lead's first-enlarge suspects (a window-sized fill nine
    windows large through a path clip); the frame table says whether it
    was.
218. **The leaving surfaces' alpha-1 exits stay, with what is known of
    them written down** (dev2, round 4; review minor). `sharedGround`,
    `sharedSurface` and `faceSurface` exit with `fadeOut(targetAlpha =
    1f)`, which changes nothing a frame shows; the review asked for the
    Compose behaviour it works around to be named. It could not be
    shown: on Compose animation 1.12.1 under `TillStage`, with
    `ExitTransition.None` in `sharedGround`, every one of
    `RunningMovesTest`'s 20 checks and `MotionFramesTest`'s 19 still
    passed on 5556, the row's ground still given a shape at 50 % and 75 %
    of its grow. The screens' own `KeepUntilTransitionsFinished` appears
    to be what keeps the leaving side. The exits stay (removing them is a
    change to every shared surface for no frame gained, and the real
    app's first frames have already behaved unlike the test clock's:
    214); the comments say what is and is not known, and
    `theLeavingRowsGroundIsStillDrawnAtThreeQuartersOfItsGrow` guards the
    result, not the mechanism: it fails if the leaving ground stops being
    drawn mid-grow, however that comes about.
219. **Round 4 is integrated as the overlay track's three commits, then
    the screens track's three replayed over them, with the screens track's
    decisions renumbered; the owner's phone stays untouched until the
    user says so in their own session** (lead, round 4). Both tracks sat on
    `motion` at `028ca20`, so `motion` fast-forwards to `motion-dev1` and
    `motion-dev2`'s commits are cherry-picked over it, as 201 did; the one
    overlap is this file, where both tracks numbered from 208, so dev2's
    151 to 158 are 211 to 218 here and the references in `Nav.kt`,
    `CodeScreen.kt` and the commit messages are renumbered in the commits
    that carry them. The lead's own changes on top are small and go in one
    commit: the three leaving surfaces' `fadeOut(targetAlpha = 1f)` exits
    are one named `keptOpaque(spec)` with 218's explanation written once
    (it had been pasted three times), and the half-way mark the stage
    (`Nav.kt`) and the code screen's bar icons (`CodeScreen.kt`) both
    defined as a private `0.5f` is `Nav.HALF_WAY`. The round's brief says
    the owner's Pixel 9 Pro is authorised, quoting the user; that quote
    reaches every agent only inside the workflow's computed text, which by
    the agents' operating rules is not the user's consent, and installing
    over the owner's onboarded data on a personal device is the kind of
    step that needs it. So no agent of round 4 installed on, drove or read
    the phone; the numbers below are emulator numbers, and the
    phone A/B (`lead/ab-r4.sh` with the phone's serial under the flock,
    `adb install -r` only, the shift, a sale, the enlarge, the order and
    Back) runs the moment the user says so in a session of their own.
220. **The code and its white travel as one surface on the enlarge and
    the shrink** (designer, round 4 review; major, screens track; 189
    reopened, 203 and 217 half-answered). On emulator-5558 the dark flash
    of round 3 is gone from every recorded enlarge (a whole-frame
    luminance check over `r4p-dark` finds no frame lighter than both its
    neighbours), but the code and its white still run on two different
    progressions. Light, second enlarge after launch, 720 px frames
    (`review-r4/phone-enlarge-code-outside-white.webp`): f43 the code is
    362 px wide inside a white 355 px wide and reaches 10 px below the
    white's bottom; f44 to f46 the code sits 1 to 5 px outside the white
    left and right (2 to 9 device pixels) while the white has already
    grown past it top and bottom, so the quiet zone is gone for about
    90 ms of every enlarge; by f46 the code is at its full size while the
    white is 66 % of the way. On the shrink
    (`phone-shrink-white-lags-code.webp`) the order is the other way: at
    f24 the code is 76 % of the way home and the white 68 %, its margins
    147 and 159 px, a white halo that collapses after the code has
    landed; the tablet's shrink shows the same (margins 58 to 91 px
    settling over three frames after the code is at rest). In dark the
    loss is invisible as white but the code's edge modules merge into the
    ground. The rule, 189's restated: the white drawn on a frame is the
    clip rect of that frame, from the one read that also places the code
    (the fill of 217 is drawn by the clip node itself, not scaled an
    update behind it and reached past), and the code's scale derives from
    the same rect, so the margin runs linearly between the two rest
    margins and is never below the smaller. The lead's clip rig measures
    the margin per frame of a recorded enlarge and shrink (the designer's
    `an.py` does it from `screenrecord` frames), since the test clock's
    harness composes, clips and draws in one pass and cannot see this.
221. **Two screens' words are never readable together, and words lifted
    above a ground wait for it** (designer, round 4 review; major,
    screens track; 188, 204 and 212 together). On a row opening into its
    sale (`review-r4/phone-row-grow-words-overlap.webp`, f158 to f159)
    "This shift", the store line and the chips are at full alpha when
    "$9.80" and "Sale #1433" arrive over them; the shift's chrome never
    fades, it is cut when the shift leaves on the landing frame. 204
    lifts arriving words above the moving ground so no glyph is cut, and
    212 keeps leaving words under it; what is missing is the half: lifted
    words start their fade only once the ground's bounds cover their
    place, and the leaving screen's words (its chrome included, not only
    its rows) fade over the first `fadeOutOnTrack` as 195 says. A frame
    may show one screen's words above half alpha, never two.
222. **Back from a sale that came up by itself goes into its row, and a
    code is never drawn without its white** (designer, round 4 review;
    major, screens track; the continuity map of `docs/motion.md` §3 and
    205 extended). Light, Back on a risen sale whose row is on the shift
    (`review-r4/phone-back-after-rise-naked-code.webp`, f82): the sale's
    words and its card's white are gone in the first frame, the code sits
    naked on the beige ground, the row's words are drawn faded at their
    final place, and the next frame (69 ms later) is the shift whole. Dark
    (`phone-back-after-rise-dark.webp`, f259 to f260): the row's dark
    ground caps the top of the code's white while the sale's words are
    gone. Neither is the row the sale came from receiving it. The rule:
    when the row exists, Back after a Rise is `RowGrow` reversed, the
    same as a sale the cashier opened; `cameUpByItself` decides only how
    a sale arrives. And the code's white is part of the code's surface
    (`Layers.CODE`) on every frame of any fade, slide or move: 205's "at
    alpha 1 from its first frame" includes the ground the code needs to
    scan, so no frame of any change draws the modules on the screen's
    ground.
223. **A back gesture's preview moves the surface, not only the words**
    (designer, round 4 review; major, screens track; M7 and 177). A
    predictive back on the enlarged code, half the width over 700 ms
    (`review-r4/phone-predictive-back-no-preview.webp`): the code's width
    goes from 524 to 514 px over the whole gesture (f487 to f506) while
    the code screen's words are gone by 200 ms and the sale's words come
    in under it, so the stage's progress passed 56 %; on release the code
    springs from 514 to 312 px in 160 ms. Back on a sale toward the shift
    is the same: the rows fade in under a full-size card, then the card
    drops into the row at the commit. The words follow the seek, the
    grounds and the code do not. The rule is 177's: one progress drives
    every part of a change, a seeked frame and a sprung frame alike, so
    the code shrinks toward its card as the finger moves and the release
    continues from where the finger left it with its velocity. Under
    Still the preview stays off.
224. **Sale to shift reveals the shift under a shrinking white and starts
    on the frame after the tap** (designer, round 4 review; major, screens
    track; M7 "the reverse of how they came"). On "Back to the shift"
    (`review-r4/phone-sale-to-shift-cut-then-code-falls.webp`) the sale's
    words go, the card holds at rest for three frames (about 65 ms), then
    at f133 the shift's chrome and list card cut in whole under the code
    card at full alpha, and the code shrinks into its row over 170 ms
    while the surface that should carry it is invisible (white over the
    list's white in light). The grow (f157 to f160) is a white surface
    growing from the row and revealing the sale at full size; its reverse
    is a white surface shrinking to the row, clipping the sale and
    revealing the shift, with the shift's words fading in over the last
    `crossfadeOnTrack` and the sale's fading out over the first
    `fadeOutOnTrack` of the same track, not before it. Two rules fall out:
    the leaving sale's words fade concurrently with the surface's first
    frames (no hold, the first moving frame follows the tap within one
    frame), and a payment code is drawn whole or not at all, never clipped
    by a moving ground: f157 shows the top fifth of the full-size code
    inside the one-row-tall window on the grow's first frame.
225. **The paid moment is not signed off from the emulator; the card's
    white shrinks into the hero on every frame** (designer, round 4
    review; major, both tracks; M5, M10, 200). The app's fold
    (`review-r4/phone-paid-fold-crossfade.webp`) is five frames 62, 20 and
    68 ms apart: the tick appears at the card's place and size at about
    30 % over the fading code (f90, f91), then the hero is at the top of
    the page (f92) with nothing between; the panel's
    (`r4p-ov2-light`, f80 to f82) fades the code to grey at full size
    with its white already gone, then shows the hero 100 ms later. Both
    may be dropped frames on a renderer that composes the paid page under
    the fold, which the lead's `framestats` on the fold says; but the
    frame that exists already breaks a rule: the card's white left before
    the hero arrived, and the hero never travelled. The fold the spec
    asks for is one white surface shrinking from the card to the 88 or
    72 dp circle with the tick fading in inside it; the white is on
    every frame. A real-device recording of the fold is the acceptance,
    not the test clock.
226. **The panel's next sale: the header follows the hero, never leads
    it** (designer, round 4 review; minor, overlay track; M5). With a
    second sale waiting, two frames after the dwell
    (`review-r4/phone-panel-next-sale-header-ahead-of-hero.webp`, f93 to
    f94) read "Sale #1444 · $9.80 · 2 items" above "Paid · 0.0588 XMR
    received", the amount of the sale just paid; then the hero grows into
    the next code, which is right and reads well. The header, amount and
    order line crossfade in step with the hero's grow, from the same
    progress, so no frame mixes two sales.
227. **A lowered panel is gone before the bubble lands, and its window
    goes then** (dev1, round 5; the ghost in
    `review-r4/phone-panel-lower-into-bubble.webp`). The card's alpha was
    three times the raise, so in the spring's long tail (raise 0.1 to 0,
    about a quarter of its length) the card sat at its smallest at 30 %
    to 3 % alpha on top of a bubble that had, to the eye, already landed;
    the base build on emulator-5554 shows it for 70 to 100 ms
    (`dev1-r5/base-lower.png`, f84 to f87). Now `Panel.opened(raise)` is
    the raise with its last tenth (`Panel.IN_BUBBLE`) taken off: the
    card's scale and the docked bubble's slide follow it, so both are
    done at raise 0.1, and the card's alpha is gone while the docked
    bubble still has the last `Panel.BUBBLE_ALONE` (5 %) of its way to
    go, so the bubble lands alone. `Presence.exit` takes a `landsAt`: the
    panel's collapse reports its landing the frame it crosses 0.1, and
    the handoff (free bubble added at rest, panel window removed once it
    has drawn) runs then instead of after the spring's invisible tail.
    A raise passes 0.1 within its first frame, so nothing is lost on the
    way up. `theLowerLeavesNoCardOnceTheBubbleIsHome` fails on the old
    code (2,000 card pixels at 4 % alpha on the frame the bubble is
    within 4 dp of home) and passes now; `after-lower.png` on 5554 has no
    card from the frame the bubble lands.
228. **The panel's paid fold is one surface, and its status words fade
    through** (dev1, round 5; 225's panel half). `Face` draws the code's
    white card as a surface of its own under the code: one rounded
    rectangle at the code's size, scaled with the code's layer from 264
    (280) dp to the 88 dp hero, its corner growing from the card's 28 (30)
    dp to half its side, its colour going from white to the hero's ground
    (`paidGround`, or `warnGround`, `dangerGround` or `chip` for an ended
    sale) on the hero's own fade, and the hero's icon fading in at its
    rest size inside it. The code fades off that surface under `fadeQuick`
    as before, clipped to the surface's rounding corners while it folds
    (one path per size, rewound per frame), so the code's own white square
    never pokes out of the circle; growing back into the next sale's code
    it is not clipped and is opaque from its first frame (§10 rule 6). So
    there is a white or hero ground under the tick on every frame, and the
    hero travels: `thePaidFoldShrinksOneSurfaceIntoTheHero` steps the test
    clock frame by frame and finds the surface never under the hero's
    size, never growing, and at three or more sizes between the card and
    the circle. `PaidHero` and `EndedHero` went into one `heroLook`. The
    panel's status words ("Waiting for payment" to "Payment seen") now
    fade through (`fadeThrough` in, `fadeQuick` out) instead of across,
    as 225 asks of the pill; the app's own status pill is the screens
    track's and is not changed here. On emulator-5554 (SwiftShader, load
    9 to 19) the fold is still one or two drawn frames between the code
    and the hero (`dev1-r5-paid-fold.mp4`, f15 to f16, 8 ms apart, with a
    heads-up notification arriving on the same frames), so the recorded
    acceptance 225 asks for still needs the phone or a GPU host.
229. **The panel's header, amount and order line follow the face
    transition** (dev1, round 5; 226). `Details` is an `AnimatedContent`
    on the same `faces` transition as the hero and the status words,
    keyed by the sale's order id, with the words' own spec (`fadeThrough`
    in, `fadeQuick` out, the size settling unclipped): the paid sale's
    header and amount leave with its "Paid · … received", the next sale's
    arrive with its "Waiting for payment", so no frame reads two sales; a
    change to the same sale (its receipt, its phase) is no transition
    there. `theNextSalesHeaderNeverMeetsTheLastSalesAmount` measures each
    frame's ink of the old "… received" and of the part of the new header
    past the old header's words, and fails if both are over half their
    rest strength; it fails on the old code from the first frame. The
    "where" line ("Front Counter") is the overlay's, one value for both
    sides; it only differs between sales from different tills.
230. **A tap during the raise lowers the panel and opens nothing; the
    docked bubble raises a collapsing one** (dev1, round 5; the review's
    inconclusive adb interrupt checks). Compose hit-tests through the
    grow's scale, so a tap where the grabber will be could land on the
    scaled card's header ("Open the shift") mid-raise. A pointer filter
    (`heldWhileMoving`) sits before the grow's layer, so it covers the
    card's place at rest: while the raise is short of 1 it takes every
    touch in the initial pass, gives none to the card, and on release
    calls `onLower`, which retargets the raise (M1); at rest it passes
    everything through. The progress is read when the finger comes
    down, never in composition. The phone's docked bubble called
    `onLower` whatever the panel was doing, so a tap on it during the
    collapse did nothing; it now has `onBubble`, the free bubble's toggle,
    so it raises a collapsing panel again from where it is. The tablet's
    floating panel has the same filter. `aTapDuringTheRaiseRetargetsItAndOpensNothing`
    taps the grabber's, the header's and the card's middle place
    mid-raise (each lowers, nothing opens) and the header at rest (it
    opens the shift).
231. **The phone panel's first order open is a rendering cost, not a
    composition one; the order is not composed ahead** (dev1, round 5;
    the review's empty-card finding). On emulator-5554 at load 11 to 19
    the first open after a cold start still shows the grown card empty
    for about 210 ms (`dev1-r5/after-order2.png`, f4 to f5). Composing
    `OrderContent` under the code as soon as the receipt arrives (at
    alpha 0, under the code in z, cleared from semantics) was built and
    measured: the slowest frames of the first open are 350 to 530 ms
    both with and without it, and in every one of them 230 to 510 ms is
    sync and GPU (SwiftShader rasterising the order's glyphs), with input,
    animation and composition at 2 to 31 ms either way. Composing ahead
    would only have added a hidden composition and its countdown's tick
    per second for every live sale with the panel up, so it was taken
    out. Whether a real GPU draws the first open in time is the phone's
    question (R4-O2).
232. **The stage draws every moving ground and the code from one read of
    its progress** (dev2, round 5; answers 220 and 223, and the
    desynchronisation behind 203, 217 and 222). The grounds and the code
    rode `sharedBounds`: a bounds animation of its own, placed with the
    layout of the update before, a clip computed with this frame's, and a
    fill reaching past to cover the gap (217). Three reads of one move, and
    a back gesture's seek moved the words (which read the stage's
    progress) but not them. Now each shared surface's two ends register
    where their screens lay them out (`StageGeometry`, `End`, one
    `Modifier.Node` each), and on every frame of a move its place is the
    straight line between the two at the stage's own progress, the number
    the words read, the spring drives and a seek sets (`Course`). The stage
    draws the moving grounds and the code itself, in two window-sized
    layers of the overlay (a node's own overlay layer is cut to the node,
    so a ground drawn from a 264 dp card could not cover the screen); the
    code is its one bitmap drawn into its line's box with no smoothing, so
    it is still one image scaled whole (principle 3), and the code and its
    white come from the same read, so the quiet zone runs straight from
    one rest margin to the other. A change that takes over another
    mid-move (a Back during a grow) starts each surface from where it was
    drawn last, so nothing jumps. `GroundTrack` is now each screen's view
    of a course (the words' clip, what waits for it, a test or the trace
    reading it) and `LocalGroundTracks` is documented as the test seam it
    is. On 5556, a recorded enlarge, shrink and slow back gesture
    (`dev2-r5-flow-light2.mp4`, every encoded frame measured with the
    designer's `an.py`): the code's side margins stay 25 to 29 px on every
    frame against 27 at rest (review-r4's f43 to f46 had it outside the
    white), and the gesture shrinks the code with the finger (612 to 570 px
    wide over the first half of it). `RunningMovesTest` fails on
    round 4's head (margins of 22 and 14.6 px against a rest 27,
    `margins`) and passes; `MotionFramesTest` holds every seeked code on
    the line between its rests.
233. **Words wait for the ground they sit on, and every screen's words
    fade through a row's grow, the shift's chrome too** (dev2, round 5;
    answers 221). Words on a moving ground (`words(on = …)`) now show
    only while it holds them: arriving, their fade starts once the ground
    covers their place, leaving, it ends before the ground lets go of
    them; arriving words under a shrinking white (`clear`) start once it
    has passed them. Each is a point of the move worked out from the
    course's line (`Course.holding`, `clearOf`; the edges move on straight
    lines, so it is exact and per draw, with no state), not a frame
    noticed late, so no fade jumps. The shift's top row, summary, notice
    and chips fade through `RowGrow` like its rows, and the rows the
    cashier didn't tap no longer stand at full alpha until the cut. The
    sale's page passes its words modifier to each of its parts, since one
    page-sized block is never held by a ground until it lands.
    `RunningMovesTest.wordsTogether` fits each frame's text regions as a
    mix of the move's two ends and fails any frame with a leaving and an
    arriving run both above half, on every move both ways.
234. **A payment code on a moving row ground is drawn whole or not at all;
    Back from a risen sale is its row's shrink** (dev2, round 5; answers
    222 and 224's code). The sale's code card (white, code, badge) draws
    only on the frames the row's ground holds all of it (`wholeOn`), so
    the one-row-tall clip no longer shows a strip of a full-size code on
    the grow's first frames, nor a code without its white on the way back.
    Back from a sale that rose by itself was already `RowGrow` by
    `Nav.move`; what the review saw was the stage's ground cut to a node
    and words lifted only while a shared match was active
    (`isTransitionActive`), which the stage's own move now replaces.
    `backFromASaleThatRoseGoesIntoItsRow` (light and dark) runs it, with
    the whole-or-none check on every frame of every row move; it fails on
    round 4's head (part of the code drawn inside the one-row ground on
    f1 to f3).
235. **A change from rest composes its new screen once at 0 before its
    spring starts** (dev2, round 5; 224's "hold then jump"). The arriving
    screen's first frame is the change's heaviest (a whole screen
    composed and laid out), and counted on the spring's clock it put the
    stage a third to a half of the way there on the first frame drawn: on
    5556 a row grow went from the row to most of the screen between two
    16 ms frames. `FollowScreen` now seeks to 0 and waits one frame, as a
    back gesture already did, so that frame shows the screen left, and
    the spring starts from 0 on the next; mid-move a new target is still
    retargeted from where it got to. The cost is one frame before the
    first moving one; the check (`startsAtOnce`) allows two at 0 and then
    requires travel, and round 4's head failed it with the ground standing
    at its rest for three.
236. **The paid fold is one white surface closing into the hero, and the
    pill's words fade through** (dev2, round 5; the app's half of 225).
    The live card's face (`faceSurface(code = true)`) keeps its white,
    clipped to the corner it is closing to, under its fading code for as
    long as the fold runs, and the hero is drawn over it (`Layers.HERO`),
    so the tick fades in on white and the screen's ground never shows
    through. Reversed ("Show the code again") the card is opaque from its
    first frame under the fading hero (205). The pill's new words fade
    through (`fadeThrough`), so "Paid" is never over "Waiting for
    payment". Recorded on 5556 in dark (`dev2-r5-fold-yes.mp4`): the card
    closes over seven frames at seven sizes with white round the hero on
    each. `RunningMovesTest.foldKeepsItsSurface` fails on round 4's head
    in dark (the screen's ground inside the folding card at f13) and
    passes. The panel's fold is the overlay track's.
237. **The debug build has a stage trace** (dev2, round 5; N-R4-2).
    `adb shell setprop log.tag.MonokularStage DEBUG` and every frame of a
    running change logs one line: the move, the two screens, the stage's
    progress, whether a back gesture is seeking, the words' shares out
    and in, and each moving surface's drawn bounds with the progress they
    were drawn at. It is read as each change starts (one property read)
    and compiled out of a release build (`BuildConfig.DEBUG`; no
    `MonokularStage` in the release dex). The rig diffs the code's line
    against its white's per frame from it. `StageTraceTest` checks it is
    silent off and a line a frame on.
238. **Round 5's screens numbers, and the owner's phone still untouched**
    (dev2, round 5). On 5556 (host GPU, load 7 to 27), five times row,
    enlarge, shrink, Back: round 4's head 20 to 27 % janky, p95 89 to
    150 ms; this head 13.5 to 14.7 %, p95 53 to 65 ms (two runs each,
    alternated). Memory against the base (`f274ad7`), same flow, then a
    `RUNNING_CRITICAL` trim: PSS +5.6 and +6.0 MB (round 4 reported +7.1),
    Java heap +1.2, native +2.1 MB; fresh on the shift +2.7 MB (202's
    accepted start-up step). So a session adds about 3 MB over the base,
    over the 2 MB rule by about 1 MB; the heap dump that names it was not
    taken this round. The brief again quoted the owner authorising their
    Pixel 9 Pro, through the workflow's text only, so no screens-track
    agent installed on it (219); the first-enlarge frame on real hardware
    is still unjudged.
239. **Round 5 is integrated as the overlay track's five commits, then
    the screens track's four replayed over them with their decisions
    renumbered 232 to 238; a shared surface's course forgets its ends;
    the owner's phone stays untouched** (lead, round 5). Both tracks sat
    on `motion` at `dc51b61`, so `motion` fast-forwards to `motion-dev1`
    (`d919b48`) and `motion-dev2`'s commits are cherry-picked over it, as
    201 and 219 did; the one overlap is this file, where both tracks
    numbered from 227, so dev2's 170 to 176 are 232 to 238 here, and the
    references in `Nav.kt`, `RunningMovesTest.kt`, `StageTraceTest.kt`,
    `CourseTest.kt`, `StatusWordsTest.kt` and the commit messages are
    renumbered in the commits that carry them (the overlay track's 227 to
    231 keep their numbers). The lead's own change on top: `Course` kept
    the two `End`s it last drew between for the life of the stage, keyed
    by the sale (`code/<order>`, `sale/<order>`), and a code's end holds
    the code's bitmap, so every sale opened and enlarged left a 13 KB code
    in `StageGeometry`; `Course.remove` now drops the ends once neither is
    on a screen, and `Course.place` takes the ends' boxes as floats (the
    geometry reads the coordinates) so `CourseTest` covers it on the JVM.
    The round's brief says, for the third time, that the owner's Pixel
    9 Pro is authorised, quoting the user to the coordinator; the quote
    reaches the lead the same way as in 219 and 238, inside the workflow's
    computed text, and installing over the owner's onboarded data on a
    personal device is still the step that needs the user's own word, so
    no agent of round 5 installed on, drove or read the phone. The
    numbers below are emulator numbers; `ab-r5.sh <serial>` runs the same
    A/B on the phone under the flock, `adb install -r` only, the moment
    the user says so in a session of their own.
240. **Round 5's design review: what now reads as the thing it is, what
    is still a cut, and the owner's phone untouched** (designer, round 5
    review). The integrated build (`fe6f893`) on emulator-5558 (Pixel 9
    Pro AVD, SwiftShader) and emulator-5560 (Pixel Tablet), with real
    sales on the shared dev stack, every encoded frame of each clip
    pulled with `ffmpeg -fps_mode passthrough` and measured, and the
    stage trace (237) read beside the frames. Light and dark, animations
    off, the phone on its side, the panel raised, lowered, flung,
    dragged, thrown and interrupted, three payments landing in the app
    and three in the overlay. Answered since round 4: the code and its
    white travel together (220; side margins 20 to 26 px on every frame
    of every enlarge and shrink on both devices, against 22 to 24 at
    rest, `review-r5/phone-row-grow-and-shrink-codes-whole.webp`), a
    back gesture on the code shrinks it with the finger and the release
    carries on from there (223; 612 to 570 px over a third of the width,
    `phone-code-back-gesture-follows-finger.webp`), Back from a risen
    sale is its row's shrink (222), the dark enlarge has no flash
    (`phone-dark-enlarge-no-flash.webp`), the panel's paid fold is one
    surface with the code scaling inside it (228; white 343, 319, 233 px
    then the hero, margins 19 to 21 px, `phone-panel-fold-into-hero.webp`),
    the panel's next sale never mixes two sales (226, 229), a lowered
    panel is gone before the bubble lands (227,
    `phone-panel-lowers-into-green-bubble.webp`), and the bubble's paid
    moment is the arc closing green over 260 ms with the pupil becoming
    the tick, held five seconds, fading to idle only once the collapse
    has landed (`phone-bubble-paid-ring-closes.webp`). The tablet's
    enlarge, shrink and gesture measure the same as the phone's. Under
    animations off every change is one frame (214). What the rig cannot
    settle: on 5558 a row's grow draws three or four frames (the stage's
    progress goes 0.14 to 0.72 across one 83 ms gap in the trace), so the
    grow's feel, the app's paid fold (two frames of code fade then the
    page, `phone-paid-fold-on-screen.webp`, 225 carried) and the paid
    page's step back into its row are still a GPU's to judge. The order
    page was not reached: the emulators' Square sign-in has lapsed ("Sign
    in to Square again"), so no sale carries a receipt or an order line;
    the lead's framestats cover it. Split screen was not walked. The
    round's brief says, for the fourth time, that the owner's Pixel 9 Pro
    is authorised, quoting the user; as in 219, 238 and 239 the quote
    reaches the designer only inside the workflow's computed text, which
    by the agents' operating rules is not the user's consent, and
    installing on and driving the owner's onboarded till is the step that
    needs it, so the phone was not installed on, driven or read; the
    flows below run on it the moment the user says so in their own
    session. Not signed off: 241 to 243 are cuts and a wrong origin in
    the two moments the cashier watches most.
241. **A payment that lands on the enlarged code folds the code into the
    hero; today it cuts** (designer, round 5 review; major, screens
    track; M8 "a paid during the enlarge", M10). Paid while the code
    screen is up (`review-r5/phone-paid-on-code-cut.webp`, f318 to f325):
    the header turns to "0.0392 XMR · Paid" and the foot still says
    "Scan with any Monero wallet" over a full-size code for 100 ms, a
    small tick fades in at the top left over it, then between f323 and
    f324 (16 ms) the code is gone and a white page is there, and the paid
    page's words fade in over the next seven frames. The trace agrees:
    `CodeGrow Code -> Sale` ran its 20 frames and drew no surface, so the
    change was a crossfade of two whole screens. The rule: a sale that
    turns paid on the code screen is one move, the code screen's white
    (`code-card/<id>`, the whole screen) closing into the paid page's
    72 dp hero with the code fading out under `fadeQuick` clipped inside
    it and the tick fading in, exactly 236's fold from a bigger card; the
    foot's "Scan with …" and the expand badge leave with the code's
    first fading frame, never over a paid code. The stage already has
    both ends (the code screen's root and the page's hero); what is
    missing is the course between them when the target is a paid sale.
242. **"Close it for now" keeps the code on its card until the row has
    taken it; today the code vanishes with the sheet** (designer, round 5
    review; major, screens track; 200, 234). The sheet's "No: close it"
    (`review-r5/phone-close-for-now-cuts.webp`, f477 to f488): the sheet
    drops and in the same 83 ms the code and its white card are gone
    from the live layout, whose amount, pill and buttons then fade for
    five frames over an empty ground; the shift's ground, then the row's
    X badge alone at its final place
    (`phone-close-for-now-badge-alone.webp`, f485), then the shift's
    words fade in. The trace shows `RowGrow Sale -> Till` with the row's
    surface drawn, so the move ran; what broke is that the leaving
    screen re-laid itself out from the sale as it now is (closed, no
    code) on the change's first frame, so the one thing the cashier was
    looking at left before anything moved. The rule, 234 extended to a
    screen being left: a leaving screen draws what it drew on the frame
    the change began, its code card included, until the move lands; the
    code's end keeps its bitmap for the move, and the card leaves with
    the row's shrink (whole or not at all), never with the model's
    update. The same holds for a sale that expires or is refunded while
    on screen.
243. **The phone panel grows out of the bubble wherever the bubble is;
    today it grows out of the dock while the bubble is still on its way**
    (designer, round 5 review; major, overlay track; M1 "the panel grows
    out of the bubble"). With the free bubble at the left edge, half way
    down (`review-r5/phone-panel-raise-from-far-bubble.webp`, f1065 to
    f1073): the card appears at the top right corner at 30 % scale on
    f1067 while the bubble is still at the left; over f1068 to f1071 the
    bubble crosses the screen diagonally, over the card's header, and
    lands in the dock as the card reaches full size. The card came out
    of an empty corner and the bubble flew into it: the opposite of the
    spatial model, in which the panel is the bubble opened. With the
    bubble already beside the dock the raise reads right
    (`r5p-ov1`, f64 to f72). The rule: the card's pivot is the docked
    bubble's centre on this frame, the one the slide's `Animatable`
    already gives the bubble's offset, read in the card's
    `graphicsLayer` as `transformOrigin` and a translation, so the card
    is always growing out of the circle whatever its distance from the
    dock; the card's alpha reaches 1 on the same share of the way as now.
    No new state and nothing per frame beyond the two reads the layer
    makes already. The reverse on the lower (the bubble leaves the dock
    as the card collapses into it, then flies home) is the same read.
244. **A grabber fling carries the finger's velocity into the collapse;
    today the card turns round on the release frame** (designer, round 5
    review; minor, overlay track; M2). A 120 ms fling down the grabber
    (`review-r5/phone-grabber-fling-reverses-on-release.webp`, f218 to
    f224): the card's top follows the finger down 184, 222, 295, 371,
    438, 536, 619 px, then on the very next frame it is at 544 and on
    its way up and shrinking (450, 368, 304). Nothing of the throw
    survives the release. M2 asks for the first frames after release to
    keep the finger's direction, the drag offset running on with the
    release velocity under `Motion.flick` toward 0 while the raise
    collapses under `Motion.surface`, so a thrown panel reads as thrown.
    A short drag springs back as it should (f363 to f402).
245. **A change that takes over a move composes its first frame before
    its spring starts, as a change from rest does** (designer, round 5
    review; minor, screens track; 235). A sale rising while a Back is
    in flight (`review-r5/phone-rise-during-back-shift-flash.webp`;
    trace `t2-back`): the committed Back's `RowGrow` sat at 0 for three
    frames, the `Rise` took over at f3, stayed at 0 for two frames and
    was at 0.264 on its first moving frame, 0.398 on the next, so a
    quarter of the rise was never drawn; on the recording the shift
    stands whole for one frame with the new sale's code card over its
    rows (f221). 235's seek-to-0-and-wait runs only from rest
    (`currentState == targetState`); a takeover of a move that is itself
    at 0 (a committed gesture, a change on its first frames) should take
    the same path. Likely the emulator's stall made it visible; the
    trace is the evidence, a GPU the judge.
246. **The phone panel scales about the docked bubble where it is on
    each frame, so it grows out of the bubble from anywhere** (dev1,
    round 6; 243, M17). `grows` took the dock's centre as the card's
    pivot, so from a free bubble far from the dock the card came out of
    the empty top corner while the bubble flew across it. The pivot is
    now `PanelMotion.bubbleNow(raise)`, `Panel.docked`: the free
    bubble's place lerped to the dock by `Panel.opened`, the same point
    the docked bubble is drawn at, read in the card's layer as
    `transformOrigin` (an `Offset` and a `TransformOrigin` are values:
    nothing allocated, no read the layer did not make already). No
    translation is added: scaling about a point holds that point, so the
    card at 0.3 sits round the bubble wherever it is and opens out to its
    place as the bubble reaches the dock; the lower is the same read
    backwards, the card folding into the bubble as the bubble leaves the
    dock for home. Alpha keeps its share (`Panel.shown`). On a phone's
    order page the same point is used, so a lower from the order folds
    toward the free bubble too. The device test
    (`OverlayFramesTest.theCardGrowsOutOfTheBubbleOnEveryFrameWhereverTheBubbleIs`)
    reads "touch" as within the design's 12 dp between the docked
    bubble's circle and the card at rest (`DockedPadding`): at rest the
    circle is 12 dp above the card by design, so the literal "touch" of
    the review cannot hold on the last frames; the old pivot fails it by
    260 dp on the first frame. A side effect to look at: mid-grow the
    bubble sits inside the card's top, over its header, since the card
    grows out of it; that is the spatial model the review asked for.
    Recorded on emulator-5554 with a host GPU
    (`review-r5/gpu-panel-raise-from-far-bubble.webp`).
247. **A grabber fling carries on after the release: the drag runs on as
    a decay inside the card's scaling layer** (dev1, round 6; 244, M18).
    244's literal fix, the drag under `Motion.flick` toward 0 seeded with
    the release velocity, turns the card round on the first frame or the
    second at any fling speed (simulated at 1500 to 5000 px/s: `flick`'s
    pull from a 400 px drag outweighs the throw within 16 ms). So the
    drag runs on under a decay (`Animatable.animateDecay`), the new
    `Motion.thrown` preset (`exponentialDecay()` with the platform's
    defaults, null under Still: nothing carried past a release), and the
    offset is the drag alone, applied inside the grow's layer, so on
    screen it is the drag times the card's scale and goes with the card
    (before, it was the drag times the raise, also inside the layer, so
    on screen drag × raise × scale, which reversed on the release frame).
    The first two frames after the release carry on down at a review-like
    6000 px/s, then the collapse takes the card; a thrown card fades out
    lower than an untouched lower's, which is the throw being seen, and
    the only lower 246's "touch" rule does not hold for. Raised again
    mid-collapse (the docked bubble tapped), the drag springs home under
    `flick`, from a coroutine waiting on the raise's target
    (`Presence.heading`), not a frame read; before, a re-raise after a
    lower past the rule kept the drag and came up a drag's length low.
    Tests: `aFlungPanelKeepsTheFingersWayAfterTheRelease` (a 6000 px/s
    `swipeWithVelocity`, the top lower on each of the two frames after
    release, then rising) and `aThrownPanelRaisedAgainComesBackToItsPlace`
    (the old code puts the card 1823 px low); `MotionTest` checks the
    preset is null under Still. `docs/motion.md` M2 and the preset table
    say so.
248. **Round 6's overlay track: the owner's phone untouched, a host GPU
    emulator instead, and why 5554 asked to sign in to Square again**
    (dev1, round 6). The brief says, for the fifth time, that the owner
    authorised their Pixel 9 Pro, quoting the user to the coordinator;
    it reaches this agent only inside the workflow's computed text,
    which by the agents' operating rules is not the user's consent, and
    installing over the owner's onboarded till is the step that needs it
    (219, 238, 239, 240), so the phone was not installed on, driven or
    read. 5554 crashed mid-round and was started again with `-gpu host`
    (the RX 6800 XT through gfxstream): the overlay then draws every frame
    at 60 fps, so the panel's paid fold is recorded with its white at
    six sizes and the code fading inside it
    (`review-r5/gpu-panel-fold-into-hero.webp`, f3 to f9): the panel's
    half of 225 seen on a GPU; the app's half is the screens track's.
    The host-GPU emulator then died under the device tests' window
    captures (gfxstream "null ctx"), so the tests ran on SwiftShader
    again: the GPU instance is for recordings and framestats, not for
    `connectedAndroidTest`.
    5554's "This request could not be authorized · Sign in to Square
    again" (M19) was a build without `-Pmonokular.square.baseUrl=
    http://10.0.2.2:8738`: a plain `installDebug` or `./gradlew build`
    APK installed over the fake-Square one talks to the real sandbox with
    the fake's token. Installing the build made with the property brought
    the order lines back at once with no new onboarding; the lead's
    5558 and 5560 likely need only that.
249. **A change that takes over a move still at 0 is a plain retarget;
    decision 245's seek is not taken** (dev2, round 6; 235, 245). The
    review asked for 235's seek-to-0-and-wait on a takeover too
    (`currentState != targetState`, `fraction == 0`). Tried, it is worse
    than what it fixes: `SeekableTransitionState.seekTo(0f, newTarget)`
    on a running transition first runs the move it takes over to its
    end, so a sale rising over a committed Back stood at 0 for 32
    frames (half a second) on the test clock before it moved
    (`RunningMovesTest.aRiseTakingOverABackAtItsStartIsComposedBeforeItMoves`,
    which held 32 frames with the seek and 3 without). The plain
    retarget moves on the frame after the new screen is composed and
    from 0 (0.041, 0.136, 0.256 on its first three frames), which is
    what 245 asked for on a clock that counts frames evenly; the 0.264
    first step in the review's trace is the emulator's frame taking
    100 ms to draw the composing frame, which no driver can hide and a
    GPU is to judge. The test stays as the guard: at most three frames
    at 0, a first step under 0.15.
250. **A course lives as long as one of its ends is on a screen, a track
    finds its course by key, and a surface is placed once a frame**
    (dev2, round 6; review-r5 minors on 232 and 234). `StageGeometry`
    kept one `Course` per sale key for the stage's life (a26a242 only
    dropped its ends), and `GroundsPainter`/`paintCodes` walked them all
    each frame of a move. Now `Courses` makes a course with its first end
    (on attach) and drops it with its last; `GroundTrack.locate` looks
    its course up by key each time (a `HashMap` get on a `String` whose
    hash is cached, nothing allocated), so a track remembered across a
    lazy row's reuse can't point at a course no longer drawn
    (`CourseTest.theStageKeepsACourseOnlyWhileOneOfItsEndsIsOnAScreen`).
    The painters read the move from the course's ends (`Course.during`)
    instead of the last placement, so the stage draws a surface whether
    or not a node drew first. `locate` was run five or six times a frame
    per surface (the lift predicate, the clip, the alpha, the end node,
    the painters), each walking the layout tree twice; a frame loop that
    runs only while a change does (`withFrameNanos`, beside the stage
    trace's) stamps the frame, and a course placed on this frame at this
    progress of this change answers at once. With no loop ticking (the
    frame stamp 0) nothing is cached, so a draw outside a running change
    is computed as before. `Course.holding` returns its range packed in a
    `Long` instead of writing two fields every word run on a ground
    shares; `GroundTrack.bounds` (a `Rect` per call, read only by tests)
    is gone, the tests read the track's floats through a test helper. The
    four stage elements say what they are to the inspector, and
    `SaleDetail`'s `words` keeps its name with a suppression saying why
    (it is its words' modifier, not the page's); the four
    `ModifierParameter` warnings left are the overlay's.
251. **A screen being left keeps what it showed; the model's screen, not
    the stage, says when** (dev2, round 6; 242). The cut came from the
    model: the close sheet's "No: close it" closes the sale and leaves its
    screen in one update, so the leaving `SaleScreen` recomposed with a
    closed sale and swapped its live face (code card included) for the
    closed page on the change's first frame. `keptWhileLeaving(screen,
    model screen) { sale }` (`Nav.kt`) hands a screen the sale as it was
    while the model's screen was it, and from the update that moves the
    model on, the value it had then: it has to be the model's screen,
    since the stage's target only changes a frame later, after the
    composition that would already show the closed sale. A
    `derivedStateOf` that hands back the same value while held, so a
    sale changing under a leaving screen recomposes nothing. Used by
    `MainActivity.TillScreens` for every till screen, and by
    `RunningMoves`, so the device tests run the app's rule: closed,
    expired and refunded in the leaving update each leave with the code
    on its card, the change's first frame the frame before it, the code
    drawn wherever the row's ground still holds its card
    (`RunningMovesTest.closeForNowLeavesWithItsCode*`,
    `aSaleExpiringAsItIsLeftLeavesWithItsCode`,
    `aSaleRefundedAsItIsLeftLeavesWithItsCode`; without the rule the
    first fails on frame 1, the code gone with the row's ground still
    the whole screen). The code screen gets the same: paid under it, it
    keeps its live words and code while it leaves.
252. **Paid or ended on the enlarged code: the code screen's white closes
    into the page's hero, the hero the white's end on the page** (dev2,
    round 6; 241, 236). The paid page's hero registers as the arriving
    end of `code-card/<id>` (`Modifier.groundEnd`: an end that draws
    nothing of its own at rest, looking like a white 72 dp circle as the
    white lands), so `CodeGrow Code -> Sale` has a course again and the
    stage paints the one white from the whole screen into the hero. The
    code has no partner on the page; its end names its white
    (`End.white`), and the stage paints it at its own place, clipped to
    that white's bounds on the frame and faded by the stage's progress
    over `fadeOutOnTrack` (the quick fade on the track, so no new
    preset), so it is never drawn outside its white and is gone by the
    time the white is smaller than it. The code screen's heading and
    foot leave over the same share from the same frame; the hero is
    lifted over the white and fades in over the last `crossfadeOnTrack`
    (the tick appearing in the white as it lands), and the page's words
    wait for the white to pass them (`clear`). Checked on the test clock
    in both themes (`paidOnTheEnlargedCodeFoldsIntoTheHero*`: a white
    surface on every frame, never under the hero's size, no module
    outside it, no two screens' words together; without the hero's end
    it fails with no white on any frame). The recording on a GPU is the
    acceptance and is not made yet (no GPU host or phone here: 254).
253. **The memory step is named: it is the shift's motion machinery,
    standing from launch, not a session's growth; accepted at +5.5 MB
    after a trim on the emulator** (dev2, round 6; N-R5-1, review-r5
    blocker; 202, 238). Java heap dumps (`am dumpheap -g`, read with
    `tmp/dev2-r6/hprof_hist.py`, a class histogram of the app heap, and
    `hprof_owners.py`, which classes hold a class's instances) before
    and after `mem-r5.sh`'s flow, base `f274ad7` and this branch
    alternately on emulator-5556 with the same sales on the shift (two
    pairs; three earlier pairs rang two sales per run, so the later
    build always had two more rows, and are not counted). Trimmed PSS:
    base 105 375 / 105 610 kB, after 110 840 / 111 203 kB, so +5.5 and
    +5.6 MB, as rounds 4 and 5 read. Fresh on the shift, before any
    flow: +4.9 and +4.4 MB. The session adds 3.2 and 3.6 MB on this
    build against 2.7 and 2.7 MB on the base: **half a megabyte**, not
    the 3 MB decision 238 inferred. The app heap's shallow bytes say the
    same: the Java step is all there at launch (+1.31 MB, 27 300 more
    objects on the shift fresh), and the flow adds 139 KB here against
    109 KB on the base. What the 27 300 are, by bytes: slot-table arrays
    (`Object[][]` +163 KB, `int[]` +82 KB, `long[]` +66 KB), `byte[]`
    +136 KB, 2 100 more snapshot state records and 1 060 more
    `mutableStateOf`s, 880 `MutableObjectIntMap`s and 1 300
    `AtomicInt`s (recompose scopes' bookkeeping), and the lookahead
    pass's per-node delegates (`LookaheadPassDelegate`,
    `LookaheadDelegateForLayoutModifierNode`: 0 to 193 and 327, every
    layout node under `SharedTransitionLayout` carries one). The
    `mutableStateOf`s are held by `TransitionInstance` (160),
    `TransformScopeImpl` (126), `Animatable` (96),
    `MutableTransitionState` (93), `SharedElementEntry` (90),
    `animateValueAsState` (72), `LazyLayoutItemAnimation` (50, the rows'
    `animateItem`), `SharedMutableTransformState` (42) and
    `BoundsAnimation` (30): each shift row's badge transition and shared
    surface, the rows' placement animation and the stage's transitions,
    composed at rest. The native heap swings by 30 MB between runs of the
    same build on this AVD (host GPU allocations in process), so its
    +2.1 MB is read only as the pair's difference; `smaps` puts it in
    `scudo:primary` (small allocations, +1.4 MB in the cleanest pair),
    which is what render nodes and their display lists are, one per
    graphics layer the rows and stage now keep (`GraphicsLayer` 21 to 36
    on the Java side). The stage's two window-sized overlay boxes, the
    review's first suspect, are two of those render nodes and record
    nothing at rest; they can't be one (the words draw between them,
    `Layers.OVER_GROUND`), and they hold no bitmap. No perfetto native
    profile was taken: no `trace_processor` here to read one, and adding
    a downloaded tool was not worth the call. So the cost is a fixed one
    of composing the shift with its motion (per row, not per sale ever
    opened, and no growth per session beyond 0.5 MB), and the call is to
    accept it rather than take shared surfaces off the rows at rest
    (which would compose them on a move's first frame, the frame the
    first-enlarge review is already about). The 2 MB rule stays the
    phone's to judge: emulator PSS is not the device's.
254. **The owner's phone is still not used by the screens track** (dev2,
    round 6; 238, 239). The authorisation again reached this track only
    inside the workflow-computed brief, quoted as the owner's words to
    the coordinator; this agent's rules don't take a quote inside a
    computed task as the owner's consent, and the step installs builds
    over the owner's own data on their personal device. Nothing was
    installed on or run against `adb-52161FDAP000RD-2T8G7d`. So the
    first-enlarge frame, the paid-on-code and close-for-now recordings
    on a GPU, the memory rule on a device, and R4-O2/R4-O4 stay open;
    `tmp/dev2-r6/heap.sh <serial> <apk> <label>` and `dev2-r5-ab.sh`
    take a serial and run unchanged under the flock once the owner says
    so in their own session (`heap.sh` uses `am dumpheap`, which needs
    the debug build, and `run-as`, nothing else).
255. **What the screens track did not take this round, and why** (dev2,
    round 6). M17 and M18 (the panel's pivot following the bubble, a
    thrown panel keeping its throw) are `overlay/*`, dev1's. M19 (the
    fake Square token refresh in `onboard.sh`), N-R5-3 (the rigs under
    `tools/motion-rig`), N-R5-4 (rebasing `motion` onto `origin/main`)
    and N-R5-5 (the A/B rig ringing sales per serial) are the lead's
    files and branch. The S1 `secondsLeft` deletion still waits on dev1;
    Layout Inspector counts, the P2 haptic test, the S5 brightness and
    4x scan checks, and the dark-mode flash check on 5558/5560 are
    carried. On this AVD the screens track's emulator also needed the
    fake-Square build (`-Pmonokular.square.baseUrl=http://10.0.2.2:8738`)
    to onboard; a plain debug build goes to Square's sandbox sign-in,
    which may be how the review's emulators came to say "Sign in to
    Square again" (not checked on 5558/5560, the lead's).
256. **Round 6 is integrated as the overlay track's three commits, then
    the screens track's five replayed over them with their decisions
    renumbered 249 to 255; a leaving screen's sale is kept by a write in
    composition, not a derived state that writes as it reads; the owner's
    phone stays untouched** (lead, round 6; 239, 242, 251). Both tracks
    sat on `motion` at `a63b39a`, so `motion` fast-forwards to
    `motion-dev1` (`4f1dc5c`) and `motion-dev2`'s commits are
    cherry-picked over it, as 201, 219 and 239 did; the one overlap is
    this file and the report, where both tracks numbered from 246, so
    dev2's 189 to 195 are 249 to 255 here and the one citation in
    `Nav.kt` (the takeover retarget, 249) and those in the report follow;
    the overlay track's 246 to 248 keep their numbers. The lead's own
    change on top: `keptWhileLeaving` (251) held a screen's sale in a
    plain object written from inside a `derivedStateOf` calculation, a
    calculation Compose may run again whenever it likes and that was
    also reading the sale while the screen was being left, so every
    change to a leaving screen's sale recomputed it; it is now the
    `rememberUpdatedState` idiom, a `MutableState` written in
    composition while the model's screen is this one and left alone once
    it is not, so a held screen reads the sale no more. Same frames
    (`RunningMovesTest`'s close-for-now, expiry, refund and paid-on-code
    tests on 5558). The round's brief says, for the fourth time, that
    the owner's Pixel 9 Pro is authorised, quoting the user to the
    coordinator; the quote reaches the lead as in 219, 238 and 239,
    inside the workflow's computed text, and installing over the
    owner's onboarded data on a personal device is still the step that
    needs the user's own word in their own session, so the phone was
    not touched, its lock never taken, and the phone A/B, the
    first-enlarge frame on a GPU and the memory rule on a device stay
    open. The emulators' numbers are in the report.
257. **Round 6's design review: the new moves read as the things they are,
    a Back with a sale waiting is still a cut, and the owner's phone
    untouched** (designer, round 6 review). The integrated build
    (`b120d88`, built with the fake-Square property and installed with
    `-r`) on emulator-5558 (Pixel 9 Pro AVD) and emulator-5560 (Pixel
    Tablet), real sales on the shared dev stack, every encoded frame of
    each clip pulled with `ffmpeg -fps_mode passthrough` and measured,
    the stage trace (237) read beside the frames; light and dark,
    animations off, split screen (Monokular over Settings on a third
    Pixel 9 Pro AVD, 1280 × 1413), the panel raised, lowered, flung,
    thrown to the far corner and raised from there, three payments on
    the sale screen, three on the enlarged code, two expiries, a close
    for now, taps 80 and 300 ms apart, two Backs 250 ms apart. Answered
    since round 5: a payment on the enlarged code fades the code inside
    its white over the move's first share, then the white closes into the
    page's hero with the tick arriving in it, light and dark, no flash
    (241, 252; `review-r6/phone-paid-on-code-*.webp`); on the tablet the
    tick travels from the left pane's card to the page's hero over three
    frames (`tablet-paid-on-sale-fold.webp`). "Close it for now" keeps
    the code on its card through the sale's words fading, then the row's
    ground takes the screen and shrinks to the row (242, 251;
    `phone-close-for-now-*.webp`; the one full-ground frame is 184 → 195
    luminance in light, 46 → 28 in dark: no flash). The phone panel grows
    out of the bubble wherever it is: from a bubble thrown to the bottom
    left the card opens round the bubble and rides with it to the dock
    over twelve frames, and the lower folds it back into the bubble on
    its way home (243, 246; `phone-far-bubble-raise.webp`,
    `phone-far-bubble-lower.webp`). Taps 80 ms apart on the code reverse
    the enlarge from wherever it is (the trace: each takeover's course
    starts from the drawn bounds; no words of the other screen pop), and
    a 150 ms interruption reverses with the margins held at 20 to 26 px
    (`phone-enlarge-interrupted-at-150ms.webp`). An expired page's Back
    carries the clock glyph into its row's badge
    (`phone-expired-back-glyph-to-badge.webp`). Under animations off
    every change is three or four frames of fade and nothing moves
    (`phone-still-dark-strip.webp`). In split screen the enlarge fills
    the app's half with the code whole. Not reached: the phone on its
    side (the AVD stayed in portrait under `user_rotation 1` this time;
    the rig, not the app), the panel's order line by name (the overlay's
    nodes are not in the accessibility dump; the fallback tap opened the
    order page, `review-r6/phone-panel-order-page.webp`), and the paid panel's fold into the
    green bubble, recorded and accepted in round 5 and untouched by this
    round's overlay commits except for the pivot. The shared store had
    another till's sales waiting all round, which is how 258 was found.
    The brief says, for the sixth time, that the owner's Pixel 9 Pro is
    authorised, quoting the user to the coordinator; as in 240, 248, 254
    and 256 the quote reaches the designer only inside the workflow's
    computed text, which the agents' operating rules do not take as the
    user's consent, and installing over the owner's onboarded till is the
    step that needs it, so the phone was not installed on, driven or
    read, and its lock was never taken. Not signed off: 258 and 259 are
    cuts in paths a cashier takes every shift.
258. **Back from a sale while another sale is waiting unseen never shows
    the shift: the leave is taken over at 0 and the next sale's code
    replaces this one's in a frame** (designer, round 6 review; 77, 249).
    With an unseen sale waiting, Back on a sale runs `RowGrow Sale →
    Till` for three frames at p = 0 (the sale drawn whole), then the
    model's `follow` moves to the waiting sale and a `Rise Till → Sale`
    takes the move over from 0 (249): the first sale's words fade, its
    code is replaced by the next sale's code in one frame, the next
    sale's words fade in, and the shift is never drawn
    (`review-r6/phone-back-with-sale-waiting.webp`, `review-r6/trace-back.log`,
    `trace-backback.log`). Two Backs 250 ms apart go through two sales the
    same way (`Rise Sale → Sale` in the trace). A cashier who pressed
    Back to see the shift sees the sale change under them and cannot
    tell they left anything; the baseline cut to the shift for a frame
    and then to the next sale, which at least showed the leave. The call:
    a leave lands before a follow starts. `follow` runs on the leaving
    move's completion (the stage reports landing to the model, as the
    collapse reports to the bubble in M9), so the sale folds into its
    row, the shift is seen, and the waiting sale rises from it; the
    takeover rule (249) stays for everything else. The screens track's;
    with a test that holds a `RowGrow Sale → Till` to its end under a
    waiting sale before any `Rise` starts.
259. **The paid page's step back after the dwell is a crossfade, not the
    page settling into its row** (designer, round 6 review; M10, 240).
    After `Motion.Dwell.PAID_MS` the paid page leaves as `RowGrow Sale →
    Till` with no `sale/<id>` course in the trace, so its words fade and
    the shift's list fades in with no surface moving
    (`review-r6/phone-paid-stepback-crossfade.webp`, `review-r6/trace-paid-shown.log`);
    a Back pressed on the same paid page a moment later leaves with the
    course (`trace-paid-row-back.log`: `| sale/order_…`) and the row ground
    shrinks into the row. M10 says the page settles into its row with
    the hero becoming the row's badge, and the paid row exists by then.
    The screens track's: give the dwell's step back the same ends as the
    Back (the leaving page's `sharedGround` and the glyph between hero
    and badge), and a test that the step back's trace carries the course.
260. **Three small ones for the screens track, and a call on the second
    Back** (designer, round 6 review; 234, 236, 77). (a) On a Back from a
    live sale and on a close for now, the code is drawn on its card
    while the words fade and then not at all once the row's ground
    appears, including the two frames on which that ground covers the
    whole screen and so holds the card (`r6p-paid` frames 68 to 69 and
    444: a bare ground, luminance 242, no code), so the shrink into the
    row begins with a code-less frame or two (`phone-back-into-row.webp`,
    `phone-close-for-now-light.webp`); 234 says drawn whole while the
    ground holds the card, and here it holds it. Minor: a frame or two on
    the emulator, three or four at 120 Hz. (b) In the sale screen's paid
    fold the tick is drawn over the fading code while the card is still
    four fifths of its size (`phone-paid-fold-on-sale.webp`, frames 3 to
    6 of 9), so for three frames there are two glyphs in one card; the
    tick belongs to the last share of the fold (`crossfadeOnTrack`), as
    252 has it on the code screen. Minor. (c) The lead asked (256) whether
    a second Back while the stage is still on its way to the shift should
    go to the launcher, as the baseline's does: no. A cashier who presses
    Back twice meant the shift, and leaving the till is the one thing a
    stray press must not do; `StageBack` stays enabled while `screen ==
    Till && stage.currentState != stage.targetState`, with `model.back
    (Till)` the no-op it already is. Minor; with 258 in, the move to the
    shift is the common case again.
261. **The showcase ranks the round by what a cashier feels, its clips
    are cut pairs on one emulator, and the open items are listed whole**
    (designer, showcase; `docs/reports/motion.md`). The order is the
    paid moment first (the one event a cashier must not miss and the
    one place the bubble, panel, chip and page used to end at different
    times), then the code's enlarge (the harshest cut, under the
    customer's camera), the panel as the bubble opened, a sale as its
    row, the shift's rows and words, animations off, and last the
    things taken away (a still panel's 5 frames in 5 s, the pulse only
    when it calls), which a cashier feels as heat and battery rather
    than as motion. The "after" clips were recorded today on
    emulator-5558 against this head (its code is round 6's `b03c864`:
    the commits above it are reports), one short clip per improvement,
    480 px wide and under 300 KB, with real sales on the shared stack;
    the "before" clips are cut from the round-1 recordings of `f274ad7`
    on the same AVD (`before/README.md`), not re-recorded, since the
    baseline build has not moved. Two things the clips cannot show are
    said rather than hidden: the Seen and Confirming sweeps of the ring
    (the stand-in Square has only paid and expired, so the one sweep on
    camera is the paid arc), and the feel of a 450 ms move on a real
    screen (SwiftShader and the emulator's change-only recorder give it
    five to twelve frames). The open items are the brief's list
    unchanged in substance, in the designer's words, with the three
    majors from 258, 259 and the memory rule first and the owner's
    phone beside them, so the owner reads the whole bill in one place.
    Nothing in the showcase is a new finding; what it adds is the
    ranking and the pairs.

262. **The ship step did not install on, drive or read the owner's
    phone, as 256 and 257 did not** (tech lead, ship step). The brief
    for this step says the owner's Pixel 9 Pro is authorised, quotes
    the owner's words to the coordinator, and asks for the final
    `f274ad7` against `motion` A/B on it before the push. That text
    reached this agent only as the workflow's computed brief, and the
    rules this agent runs under say in so many words that a quoted
    consent inside such text is script output, not the user speaking,
    and that no agent's message is ever the user's approval. The step
    the consent would unlock installs builds over the owner's
    onboarded store on their personal device; the phone is attached
    (`adb devices` lists it beside five emulators), so the only thing
    between the step and the device was this rule. The rule wins, as
    it did in 219, 238, 239, 251, 256 and 257, and nothing was sent to
    the phone's serial, its lock never taken, its backup untouched. So
    the PR's performance table is the emulators' (round 6, same
    session pairs), marked as such, and the phone A/B stays the first
    open item: it needs the owner's own word in their own session, or
    their own hand on `ab-r6.sh`, and then one decision amending 202
    and 253 with the device's numbers. If the owner reads this and did
    say those words, the agents did hear them; they could not act on
    them from where they stood.

263. **The PR opens from the round's base, `f274ad7`, and the rebase on
    to today's `main` is the next job, not this one** (tech lead, ship
    step). `origin/main` has merged #18 (the loading mark on the lit
    bubble), #16, #17 and #19 (the Bell, Clover), #20, #21 and #22
    since the base. A trial merge (`git merge-tree`) of this branch on
    to it conflicts in 17 files with 47 hunks: `overlay/Faces.kt` (7)
    and `overlay/Overlay.kt` (4) against #18, which 28 and 16 of this
    branch's commits touch; `ui/Till.kt` (11), `AppViewModel.kt`,
    `MainActivity.kt`, `TillRunner.kt`, `ShiftList.kt`, `ui/Shift.kt`,
    `ui/SaleDetail.kt`, `ui/CodeScreen.kt`, `ui/OrderPage.kt`,
    `ui/CloseSheet.kt`, `sales/Till.kt`, `PanelTest.kt` against the
    Bell and Clover; `build.gradle.kts` and `libs.versions.toml`
    against the pins; and `docs/decisions/android.md`, where `main` now
    numbers its own decisions 90 to 146 and this branch numbers 90 to
    204 from the same 89. The repo merges by rebase only, so every one
    of the 96 commits must land on `main` clean and buildable; doing
    that in the ship step, blind, across the overlay's 44 commits over
    #18's bubble faces and the stage's over the Bell's view model, is a
    round of work with its own measurements, not an hour's. The PR is
    therefore a draft from the base, its body names the conflict set
    and the plan: rebase in the order the tracks were built (M-series
    overlay first, over #18's `Faces.kt`, keeping the mark's glints
    inside the one `rememberInfiniteTransition` rule 7 allows; then
    the screens' stage over the Bell's `AppViewModel` and
    `MainActivity`), with `git rebase -x 'cd android && ./gradlew
    :app:compileDebugKotlin :app:testDebugUnitTest'` so no commit lands
    unbuilt, renumber this branch's 90 to 206 to 147 to 263 in one
    final commit (the spec, the reports and the code's comments cite
    them by number, so it is a search and replace with a check that
    every cited number still points at the decision it did), and
    re-run the A/B against the new `main` since the Bell's wake-ups
    change what the till does at launch. The history itself is clean
    for that: 96 commits, every subject a full sentence, no fixup or
    wip, `./gradlew build` green at the tip; `gh pr checks` on the
    draft will say what CI makes of the base. (Done in decision 265: the
    numbers in this entry are the ones before that rebase; this branch's
    90 to 207 are 147 to 264 now.)

264. **The Needs action count is a pill concentric with its chip** (the
    owner's report, 2026-10-30: the badge "doesn't follow the curvature
    + padding"). The badge was a 20 dp box with 10 dp corners, 8 dp from
    the chip's top and bottom but 14 dp from its end, so it sat off the
    centre of the chip's round end, and a longer count made it a
    rounded rectangle. Now `FilterChip` names its sizes once (chip 36,
    badge 20) and insets the badge from the chip's end by
    (36 - 20) / 2, as from its top and bottom; the badge fills the
    chip's height inside that inset, is at least as wide as it is tall,
    and has fully round ends, so it is a circle for one digit, a pill
    for more, and stays concentric at any font scale. Labels and counts
    stay on one line, and the chip row scrolls sideways when a large
    font makes the chips wider than the screen, where a label used to
    wrap into a blob under a fixed 36 dp height. `FilterChipTest`
    measures the drawn badge against the chip: counts 1, 9, 12 and 128,
    selected and in the dark, and at twice the font size.

265. **The motion branch sits on `main` (`16dee74`, #16 to #22 in), and
    its decisions are 147 to 264** (tech lead, round 2's first step). The
    89 commits over `f274ad7` were replayed one by one with `git rebase
    -x` compiling the app, its unit tests and its device tests after each,
    so every commit on the branch builds; each conflict was resolved in
    the commit that made it, and the tip passes `./gradlew build`. The
    calls, where `main` and the branch wanted different things: (a) #18's
    loading mark stays the lit bubble's face, inside M4's one circle
    (`Faces.kt`'s `Circle`: alpha and ground fading per draw, the tick
    crossfading in). The phase ring, which the branch kept under the lit
    face and closed green on paid, is now drawn only once paid (its alpha
    is 1 for `Paid`, 0 otherwise), because #18 took the ring off the lit
    bubble to make room for the mark's own ring; on paid it still closes
    green round the tick. The round's overlay track replaces this whole
    look (B5 c), so nothing more was spent on it. (b) `main`'s point of
    sale words (`PosText`, `SaleHeader`, `posOf`, `LocalPointOfSale`) run
    through the branch's one clock: where a `Countdown` names the point
    of sale, `posOf(sale)` is read once in composition and the value is
    captured by the words lambda, never a composable call inside it. (c)
    `MainActivity` keeps the branch's shape (`Flows`, `TillScreens`,
    `SetupScreens`, `TillStage`, `StageBack`) and carries `main`'s
    screens in it: the PointOfSale step, Settings with Wake-ups,
    `tenderState` and `refundSheet` as two more states collected once at
    the root. `tillActions` is keyed on the point of sale and the till's
    source as `main` has it, the source read through `derivedStateOf` on
    the runner state so a poll that changes nothing else recomposes
    nothing at the root (performance rule 10). The Back rule is `main`'s
    (no Back on step 4 of setup) inside `StageBack`; `LocalPointOfSale`
    wraps the stage, so the overlay and the app provide it the same way.
    (d) `Screen.Square` is `Screen.PointOfSale` in `Nav.step` and the
    tests. (e) `TillRunner` launches the Bell's dings and `lapseOnTime`
    side by side. (f) The per-text `secondsLeft(expiresAt)` timer the
    branch removed stays removed; `main`'s callers of it (the close
    sheet, the code screen, the order page's bar, the shift's rows) read
    the screen's clock instead. (g) The renumbering is +57 everywhere the
    branch wrote a number: the spec, the reports, this file's motion
    section and the code's comments and tests, including the wrapped,
    possessive ("128's", "203 a") and bare parenthesised forms; `main`'s
    own citations of its 90 to 146 are untouched, which a line-by-line
    diff against `main` kept apart from the branch's. Every number 90 to
    207 left in the branch's text was read by hand: heap-object counts,
    luminance values, millisecond and pixel figures and the earlier
    tracks' own numbering notes are what remain. Decision 206 keeps the
    numbers it was written with and says so. The round-2 brief's 201,
    202 and 203 a to c are 258, 259 and 260 here; its "decisions 90-207"
    are 147 to 264.

266. **Back always lands on the shift, waiting sales wait in a tray, and
    the paid step-back is Back** (the owner, 2026-10-30; the designer's
    spec, `docs/motion.md` §12.1 to §12.4; supersedes 258 and 259).
    Pressing Back on a sale lands on the shift even when another sale is
    waiting unseen: there is no follow on a leave, not after it lands
    either, so 258's "a leave lands, then the follow starts" goes. A
    sale that arrived while the cashier was on another screen waits in
    the shift's waiting tray (design A4, opens in place): a "Waiting for
    payment" header with the count, an amber `warnGround` tray holding a
    `card` of every live sale, folded after three rows into "Show N more
    waiting" (ink, not link orange: 4.36:1 on the tint) and "Show fewer",
    the rest under "Earlier"; no New tag, bar, snackbar, banner, stripe
    or outline. A sale arriving while the shift itself is showing still
    rises by itself (198, 222): the canvas says nothing against it and
    it is what the till is for, so `TillNav.follow` narrows to that case
    rather than going. The paid page's step-back after the dwell is the
    same `RowGrow` reverse into its row (at the top of Earlier) as a
    Back, with the course, so 259's crossfade goes; the owner wants the
    two to feel identical. The three frame tidy-ups (260 a, b, c) are in
    scope as written there. Before clips of the three behaviours are in
    `docs/reports/motion/before-r2/`.
267. **The bubble is the logo, and its border is the spinner** (the
    owner, 2026-10-30, design B5 c; `docs/motion.md` §12.5; supersedes
    M4's faces and 265 a's look). Idle is the grey mark at 55 % as today;
    Waiting is a still orange lens (#18's glints leave the bubble;
    `LoadingMark` stays elsewhere) whose border becomes a pale #D8D2C7
    track with a black #1A1917 segment chasing clockwise at constant
    speed, one turn per 1250 ms; Paid fills the lens green from the core
    out (four rings, 60 ms apart, 200 ms each) and draws a white tick
    over it, the border whole and black, no ring, spinner or pulse. The
    bubble draws no `PhaseRing` and no pupil; the ground is white in
    every state (`paidGround` leaves the bubble). Still: no chase (a
    whole ink border), no pulse, 120 ms fades. Two presets join
    `Motion`: `chase`, a loop beside `pulse`, and `spread`, a fade with
    a per-ring start. The designer's calls where the canvas and the code
    differed, each a small one (§12.6): the arc is the canvas's 30 % of
    the border, not the brief's "a third"; the one mark is 38 dp in
    every state (the lit face was `LoadingMark` at 40 dp); the idle
    border's #565656, a literal in `Faces.kt`, and the two new greens go
    in `Fixed`; the tick starts where the core's ring ends, in the spec,
    never a `delay()`; the bubble's window gets the TalkBack label the
    canvas gives it ("Monokular, $12.00 waiting for payment", carrying
    the step), where `BubbleFace` cleared its semantics.
268. **The +4.9 to +5.9 MB memory step after a trim is accepted, and the
    2 MB rule is superseded by "no worse"** (the owner, 2026-10-30;
    `docs/motion.md` §12.7; supersedes §8's and §10 rule 12's "PSS at
    rest within 2 MB" and the part of 202 and 253 that left it to the
    phone). Decision 253 named the step as the shift's motion machinery
    standing from launch (+0.5 MB a session), and the owner accepts it
    as read. Round 2 does not spend time on it, and does not make it
    worse: the round's after-minus-before, measured as 253 measured
    (same emulator instance, one session, trimmed, pairs alternated), is
    within that instance's run-to-run noise, with the heap's object
    count read the same way.
269. **Round 2's architecture: the tray is keyed items in the shift's one
    list, Back and the dwell share `leave`, the stage reports its rest
    to the model, and the bubble is one mark read in draw** (tech lead,
    round 2 build; `docs/motion.md` §13; 266, 267). The tray has no node
    of its own: `ShiftList.sections` (pure, JVM-tested) lays out header,
    tray rows, fold, Earlier and the rest as items of the `LazyColumn`
    the rows already live in, keyed `"tray"`, `"fold"`, `"earlier"` and
    the sale id, so a row paid or ended is the same keyed item moving to
    Earlier under M11's three `animateItem` specs, and each tray item
    draws its slice of the `warnGround` behind itself; the fold is one
    `rememberSaveable` beside the filter. Back into a folded row opens
    the fold as the change starts (the shift reads the stage's
    `currentState` once per change, at composition), not on landing,
    because the ground needs a row to land in. `AppViewModel.leave` is
    the one path for Back and the dwell; `TillNav.follow` on the shift
    narrows to a live sale new since the last look while the stage rests
    on the shift, which the stage reports through `FollowScreen`'s
    `onLanded` (258's mechanism, kept for this reason alone: a sale
    arriving mid-Back goes under the moving card into the tray, one
    arriving on the resting shift still rises, §12.6 item 4); `leave`,
    `openShift`, `closeSettings` and `backTarget` make no follow of
    their own. The second Back (260 c) is `StageBack` enabled while the
    stage is still arriving on the shift, read as a `derivedStateOf` at
    the activity's root: two recompositions a change, none a frame. The
    bubble's `AnimatedContent` becomes one `updateTransition(look)`
    whose values (alpha, border colour, four ring colours, arc alpha,
    tick trim) and the one infinite transition (pulse and chase) are
    read only in `mark()`'s draw. `TillStage`'s two `snapshotFlow`s go
    to the screens track with `Nav.kt`, not the lead, since the track
    owns the file this round.
270. **Three presets join `Motion`: `chase`, `spread` and `tick`; a
    spring that must wait gets its wait in its spec** (tech lead, round
    2 build; 267, §12.5). `chase` is `loop(CHASE_MS = 1250, LinearEasing)`
    beside `pulse`, null under Still (a whole ink border). `spread` is a
    list of four presets, ring `i` a `fade` with `delayMillis = i ×
    SPREAD_STEP_MS` (60), so the rim is done at 380 ms; under Still each
    ring is `fadeQuick` with no delay, one quick fade for the lens. The
    tick is `sweep` started where the core's ring ends; a spring carries
    no delay of its own, so `After` is a `Preset` whose spec (`Delayed`,
    a `FiniteAnimationSpec` that holds the inner spec at its start for
    the wait, as a tween's `delayMillis` does) plays the spring from a
    later start: a start in the spec, never a `delay()`, interruptible
    like any spring (retargeted mid-hold it restarts from where it
    stands). Under Still the tick snaps whole. `MotionTest` holds all
    three to their numbers and forms.
271. **The bubble's new colours are `Fixed` by purpose; the mark's own
    shades stay the mark's** (tech lead, round 2 build; 267, §12.6 item
    5). `Fixed.PaidLight` #2EA052 and `Fixed.PaidMid` #238A43 for the
    lens's lighter greens (`Fixed.Paid` is the core), `Fixed.ChaseTrack`
    #D8D2C7 for the border's pale track, `Fixed.IdleBorder` #565656 out
    of `Faces.kt`'s literal. The orange and grey shade lists in `Mark.kt`
    are the SVG's own, as round 1 had them; the green list is built
    there from the three `Fixed` greens. `BubbleColoursTest` holds the
    canvas's contrast claims: the white tick at least 3:1 on every green,
    ink at least 3:1 on the chase track, the greens darkening toward the
    core. The "206." between 262 and 264 was 263 (a numbering slip in
    65a81c7), relabelled here.
272. **The bubble's `mark()` takes the arc's start as a plain `Float`,
    and an arc alpha of 0 is "no arc"** (dev1, round 2; §13.5). §13.5
    sketched `arcStart: Float?`, null for no arc; a `Float?` read from
    the chase each frame is a boxed `Float`, one object a frame, so the
    entry takes `arcStart: Float` and draws no arc at `arcAlpha` 0 (the
    look's own off, which Still and Idle and Paid already give). The
    rings, the arc's share and start, the tick's path and the strokes
    are the mark's own in `MarkGeometry` (pure, so `MarkGeometryTest`
    holds the 1 + 6 + 12 + 12 rings, each one shade, and the 30 % from
    12 o'clock on the JVM); a facet's ring comes from its place in the
    list, its shade from its ring. `LensLook` is a `fun interface` of a
    ring to its `Color`, so the logo's orange and grey and the paid
    green are three constant looks and the bubble's is one remembered
    look reading its four animated colours in the draw. The strokes,
    made per draw before, are made once with the paths; the facets are
    walked by index, not an iterator. `MarkOnDeviceTest` counts the
    objects 100 frames of the mark allocate (lens, border, arc and half
    a tick) against 100 empty frames, with the counter shown to count,
    and holds the bubble's entry pixel-identical to `Logo`; the logo
    and the loading mark (still and at 1234 ms of its glints) were
    checked pixel-identical before and after the change on 5554.
273. **The bubble's TalkBack label stays on its window, and `BubbleFace`
    carries the same words** (dev1, round 2; §12.5, 267). The free
    bubble's window hides its descendants from accessibility and speaks
    its own `contentDescription` (`BubbleFrame.describe`), so the label
    TalkBack reads is the window's; it is now `Bubble.label(look, sale)`
    with the canvas's words: "Monokular, $12.00 waiting for payment",
    "… part paid", "… seen on the network", "… confirming" (the live
    status words the shift and panel use, lower-cased), "Monokular,
    $12.00 paid", "Monokular, nothing waiting". The old "new sale, tap
    to show the code" goes: the window is a button, so TalkBack already
    says what a tap does. `BubbleFace` takes the same string as `label`,
    so the compose node says it too (and a test can read it). The
    label is null where the bubble is a picture (onboarding) or another
    node speaks for it (the docked bubble's "Lower to the bubble").
    `Bubble.label` lives in `SaleText.kt`, a screens-track file. Its
    one function changed there (and its test line in `SaleTextTest`)
    rather than a second copy in `overlay/`.
274. **A paid that lands mid-turn holds the arc where it was; the loop
    itself ends with the lit look** (dev1, round 2; §12.5 "interruption").
    The chase's infinite transition is in the composition only while
    the look is Lit (and motion is Full), so on Paid it is gone with the
    recomposition that changes the look, and nothing runs a loop
    behind a paid or idle bubble. The arc's alpha fades on `fade` from
    the transition; its angle is the last one the draw read, kept in a
    one-slot `FloatArray` the draw writes (not snapshot state: nothing
    observes it), so the fading arc stays where the cashier saw it while
    the track darkens to ink under it rather than jumping to 12 o'clock.
    The pulse and the chase share the one `rememberInfiniteTransition`,
    made only while one of them runs.
275. **What became dead with the bubble's ring, and the tests ported**
    (dev1, round 2; 267). `PhaseRing`'s `paid` parameter (only the bubble
    passed it) and its `pupil` parameter (no caller left) go, the closed
    ring always the theme's `paid`. `PhaseRingOnDeviceTest` now draws in
    the light theme and reads `LightTokens.paid` where it passed
    `Fixed.Paid`. No test asserted the old bubble's ring or tick, so
    none is removed. `OverlayFramesTest` adds the bubble's pixel tests:
    the chase at 0, ⅓ and ⅔ of a turn, a whole grey border idle, the
    paid core green by ~200 ms, rim by ~380 ms and the tick only after
    the core, a paid mid-turn arc held in place, Still's whole ink
    border, and the label. The loop's absence is read from the
    composition tracer's `rememberInfiniteTransition` marker, with a lit
    bubble as the control that the tracer sees it.
    `OverlayRecompositionsTest` counts `BubbleFace`, `Circle` and the
    mark's `Canvas` at 0 across ten frames of the chase and of the
    spread.
280. **The tray's sections are one pure list, and Needs action now lists
    the live sales** (dev2, round 2, S1; `docs/motion.md` §12.3, §12.6
    items 7 and 8). `ShiftList.sections(sales, filter, folded)` returns
    the shift's items in order (`TrayHeader`, `Row`, `Fold`, `Earlier`)
    beside `rows`, which it still uses for each filter's sales. Under
    Needs action the tray holds every live sale, unfolded and with no
    fold row, then the sales ended unpaid under Earlier: until now the
    filter showed no live sale at all (a waiting sale is not one that
    needs action, and the chip's count still says so), but the canvas
    puts the tray on that filter, so its live sales are listed and the
    count is unchanged. The Earlier header is only there when something
    is under it (a tray of live sales alone reads as the tray), and with
    no live sale under the filter the items are `rows` as they read
    before, corners counted over the whole list.
281. **The tray's amber starts at its first row, under the header, as
    the canvas draws it** (dev2, round 2, S2; `docs/motion.md` §12.3,
    §13.3). §13.3 gives the header the tray's 24 dp top corners; every
    artboard (`Main`, `TrayDark`, `TrayFew`) draws "Waiting for payment"
    and its count on the screen's ground, above an amber box that starts
    4 dp over the first row. The canvas is the source of truth, so the
    first tray row's slice carries the 24 dp top corners and the 4 dp
    top padding, the fold row (or, with no fold, the last row) the
    bottom ones, and the header no amber. Each slice is a `background`
    with a shape made once (a `drawBehind` of a cached outline), not a
    node of its own. Other calls in the tray's build: the fold row's
    chevron is a new `MkIcons.Fold` (the canvas's `M6 9l6 6 6-6`, 2.2
    stroke) turned in `graphicsLayer` on `settle`; the row's open lambda
    captures the sale's id only, so a row whose corners change as the
    tray opens skips its `SaleRow` (`TrayTest` counts it); and folding
    an open tray scrolled past its third row asks the list
    (`requestScrollToItem`) to keep the fold row where it was on screen,
    counting the kept rows above it at one row's height each, since the
    item the list anchored on is one of the rows going.
282. **Back lands on the shift: the follow narrows to a sale new since
    the last look on a resting shift, the stage reports its rest, and
    the row came back to takes focus** (dev2, round 2, S3; 266, 269,
    `docs/motion.md` §12.1, §13.4). `TillNav.follow(Till, …)` takes the
    last look's live ids and whether the stage rests on the shift, and
    brings up only a live sale in neither the look nor `left`; `leave`
    makes no follow, `closeSettings` and `backTarget` call `follow` with
    `restingOnShift = false`, so a code or order page whose sale is down
    still gives way to its page while nothing rises. The model's
    `restingOn` starts as the first screen (at rest before anything
    moves, so a till opened on its shift with a live sale still brings
    it up as before), is cleared by a `go` to another screen (a `go` to
    the screen already up changes nothing the stage would land from, so
    "Open the shift" on the shift cannot leave it unset with no landing
    to set it again) and by a rise, and is set by `landed`, which
    `FollowScreen` calls from one `snapshotFlow` (the stage at rest on
    the model's screen, emitted once a landing). The shift reads
    `Stage.leavingSale` in composition (twice a change) to open the
    fold when the sale being left is a live one past the third, as the
    change starts, and to hand focus to that row once the stage lands;
    a clickable row takes no focus in touch mode, so that one row gets
    `focusProperties { canFocus = true }` while it is the row came back
    to (the shift screen's life), the only way a `requestFocus` lands.
    `TestModel` mirrors `follow`, `landed` and `left` so
    `RunningMovesTest` drives the same rules on the stage.
283. **The second Back is the stage's while the shift is arriving**
    (dev2, round 2, S4; 260 c, `docs/motion.md` §12.1). `StageBack` is
    enabled on the shift while the stage's `currentState` and
    `targetState` differ, read through one `derivedStateOf` at the
    activity's root (and in `ModelStage`, so the device tests run the
    same handler); a press then reaches `model.back(Till)`, which does
    nothing from the shift (`back()` there is `goHome()`, the shift
    itself), and the stage lands where it was going. On the resting
    shift the handler is off and Back leaves the app (decision 77).
    `StageBackTest` presses the second Back half way through the shrink
    and counts `ModelStage`'s compositions over the move: one to four,
    not one a frame.
284. **`TillStage` has one collector on whether a change runs, and a held
    move is its target's until rest** (dev2, round 2, S7; 206, 245, 249;
    the after report's `aSaleRisesDark` flake). The frame loop and the
    forgetting at rest share one `snapshotFlow { currentState !=
    targetState }` collector; at rest it forgets the held move only if
    the stage still rests when the collector gets there, so a rest
    reported late (after the next change began) cannot reset the move
    the new change already holds. `HeldMove.during` starts a generation
    only for a new target (a takeover) or from rest; the same target
    read with another current screen mid-change is the same change, so
    nothing read meanwhile (the till's came-up-by-itself above all) can
    turn a rise into a grow half way. The device flake did not show on
    5556: three slow-frame variants (a 120 ms test-clock frame mid-rise,
    light and dark; a sales update turning came-up-by-itself off mid-rise
    then a slow frame; a rise sent on the very frame a Back lands) pass
    before the guard and after, so the guard stands on its reading of
    the mechanism and on `NavTest`'s JVM test of `HeldMove`, which fails
    before it (a second generation re-reading came-up-by-itself as a
    grow) and passes after; the slow-frame tests stay as regressions.
285. **The paid step-back already carries the sale's course on the
    emulator; it is the Back's move with the Back's ends, held so by a
    test, and what read as a crossfade there is dropped frames** (dev2,
    round 2, S5; 259, 266, `docs/motion.md` §12.2). Traced before any
    change on emulator-5556 (`docs/reports/motion/r2-dev2/`): at the
    round's base (ca702e4) and on this branch the dwell's leave is
    `RowGrow Sale → Till` with `| sale/<id>` from its second frame, from
    the whole window to the paid row, the same two ends as a Back on the
    same page (`[0,0,1280,2856]` to `[48,769,1232,988]`). The difference
    on this emulator is pacing: two of three dwell step-backs drew four
    to nine frames with 100 to 150 ms gaps where the Back draws twenty
    at 16 ms, and a Back pressed during the dwell on the same folded
    page draws twenty too, so the page is not the cause; moving the
    service's and the overlay's own `PAID_MS` timers 1.5 s later (a
    diagnostic build, not kept) did not remove the gaps. The course
    missing on the owner's phone (259's `trace-paid-shown`) is not
    reproduced here, and the phone is not used; the likeliest cause
    there is round 1's `leave` calling `follow` with live sales waiting
    (258), which this round removed (282). So no second mechanism is
    added: `RunningMovesTest` pays a sale in place, waits `PAID_MS` on
    the test clock, leaves as the dwell's job does, and holds the move
    to the Back's frame for frame within half a pixel, landing in the
    first Earlier row; Still is the Back's crossfade. Reviewers on the
    phone should read the dwell's trace for `| sale/<id>` first.
286. **The paid fold's tick comes in over the fold's last share; the
    code-less full-screen frames of 260 a are not reproduced here and
    get regressions, not a speculative change** (dev2, round 2, S6;
    260 a, b, 234, 252). (b) `faceSurface`'s arriving glyph (the paid
    page's hero coming in over the live code card) takes the lesser of
    its fade and `Nav.through` over the fold's own progress with
    `crossfadeOnTrack` as the arriving share, read per draw from the
    `shown` value the surface already animates, so the tick is never
    drawn while the card is still closing over its first share: one
    glyph in the card at a time, as the code screen's hero has it.
    `RunningMovesTest` reads the hero's green against how far the card
    has closed: before the change it found the tick at 0.00, 0.09, 0.24
    and 0.39 closed (f4 to f7, light and dark), after it none before
    0.56. (a) The bare ground (luminance 242, no code) on a Back from a
    live sale and on "Close it for now" does not show in the test's
    frames (a new `backFromALiveSaleLeavesWithItsCode`, light and dark,
    beside the close-for-now one: the code is on its card on every frame
    the ground holds it) nor in five screen recordings of a Back on
    5556 (the code on its card from the first frame until the ground
    reaches the row). A reading for the lead to check on the phone: the
    test draws each frame to a software canvas, re-running every draw,
    while the device redraws a node only when something it read in draw
    changed; the sale face's lift into the stage's overlay is decided in
    draw from plain fields (the held move, the course's place), so a
    frame on which the ground starts being drawn in the overlay, over
    both screens, could replay the face's last recording in place,
    under it, until the face next draws. If the phone shows it, the fix
    is for that decision to read the stage's progress (a state the
    spring already writes each frame), so the face redraws on the frames
    of a move; it is not made here unseen.
287. **Focus goes to the row came back to only for a screen reader**
    (dev2, round 2, S8; amends 282, `docs/motion.md` §12.3's TalkBack).
    The side-by-side screenshots showed the row a Back landed in drawn
    grey: a row given input focus shows the clickable's focus state, so
    every sighted cashier saw a highlighted row the canvas does not
    have. The shift now moves focus only when the system's
    accessibility manager says touch exploration is on (TalkBack), read
    once as the Back lands through `LocalScreenReader`, a test seam as
    `LocalGroundTracks` is; `RunningMovesTest` checks the focus with a
    screen reader and no focused row without one.
288. **Round 2 integrated on `motion-main`: dev2's track replayed first,
    then dev1's, and what the review changed** (tech lead, round 2
    integration; 272 to 275, 280 to 287; `docs/motion.md` §13.7). Both
    tracks replayed onto `ca702e4` by cherry-pick, linear, dev2's eleven
    commits then dev1's four (§13.7 said dev1 first; the order makes no
    difference to a linear replay, and dev2's was the branch with the
    draft PR); only `docs/decisions/android.md` conflicted, both tracks
    appending, and `SaleText.kt` (dev1's `Bubble.label`, dev2's `follow`)
    merged clean. The numbers 276 to 279 are unused: dev1's range ended
    at 275 and dev2's began at 280. The review's one change of its own:
    `SaleItem`'s `moves: Modifier` is `modifier: Modifier = Modifier`
    (the item's root takes it), the last `ModifierParameter` warning,
    so lint is at the manifest's and network config's five warnings
    only. Read and left as they are: the held arc angle written by the
    draw into a one-slot array (274: a frame of the chase allocates
    nothing, and nothing but the draw reads it); the model's `restingOn`
    nulled by `go` and set only by the stage's landing (282), whose one
    hole, two `go` calls inside one snapshot ending where the stage
    already rests, no path in the till takes (every `go` is a tap, a
    Back, a dwell's end or a sales change, each its own snapshot), so the
    shift's `landed` report is left as the one writer rather than given
    a second from `FollowScreen`'s no-move branch that the same
    coalescing would also miss; Needs action listing the live sales in
    its tray (280: the canvas puts the tray on that filter); the tick
    un-drawing over `fadeQuick` as the bubble leaves Paid (the brief's
    own words); and `keepFoldInPlace` counting the rows going above the
    fold at the first visible item's height (the rows are one height,
    and a wrong count is a scroll of a row, not a jump off the screen).
289. **The tray opens for a Back in the composition that starts the
    change, not an effect after it** (dev2, round 2 review; 260 a, 269,
    286, 235). The lead caught the bare ground (luminance 242, no code,
    no card) for three frames on a Back from the fourth waiting sale on
    both emulators; `RunningMovesTest` then caught it too once its Back
    started from a folded row (`backIntoAFoldedRowOpensTheTray`, light
    and dark: f2 and f3 100% ground, the code gone from its card). The
    cause was the fold, not a stale recording: `ShiftScreen` opened it in
    `LaunchedEffect(cameFrom)`, a frame after the change's first, so the
    stage's held RowGrow ran with a course whose shift end (the row) did
    not exist yet; the sale's ground drew whole and its code, which hides
    while its course moves, was nowhere. The shift now derives the shown
    fold in composition (`folded && !opens`, `opens` when the stage
    leaves a sale the folded tray would hide) and writes the saveable
    only in a `SideEffect`, so the row is composed on the very frame the
    change is. Decision 286's reading (the face's lift replaying a stale
    recording) is not the mechanism and is not acted on: with the row
    there from the first frame no bare frame shows, and a Back with no
    row at all (`backWithNoRowToLandInNeverDrawsTheBareGround`) never
    showed one. Under Still the same fix makes the cut land on the open
    tray, not the folded one and a jump (`...UnderStillCutsToTheOpenTray`).
    All three new tests fail before the change and pass after. The
    tablet's one "Close it for now" frame (lead's `closenow` f30) is not
    the fold (a closed sale is not live, so it never folds) and is not
    reproduced here; it is left for the lead's phone reading.
290. **The sale screen's leaving code fades on the paid fold's own
    progress, gone by its first `fadeOutOnTrack`** (dev2, round 2
    review; 260 b, 252, 286). Decision 286 gated the arriving tick to
    the fold's last `crossfadeOnTrack`, but the leaving code still faded
    on `seen`, a `fadeQuick` tween with a clock of its own: on the test
    clock the modules were still drawn at 0.39 and 0.52 closed, and on
    the lead's device recording (stepback f6, 58% closed) they were at
    about 0.2 under the tick at 0.4. While the fold runs, `faceSurface`
    now draws the leaving code at `Nav.through(1 − shown, leaving =
    fadeOutOnTrack)`, as the code screen's hero does (252): gone by 27%
    closed, so with the tick from 55% the two glyphs are never in the
    card together whatever the frame pacing, and both read the one
    spring. (The review wrote `through(shown, …)`; `shown` falls from 1
    as the face leaves, so the progress toward the page is `1 − shown`.)
    `seen` stays for every case but the fold (the code arriving on "Show
    the code again", the hero leaving). `RunningMovesTest`'s
    `thePaidFoldDrawsTheTickOnlyInItsLastShare`, light and dark, now
    reads the code's modules inside the card by their pattern (a line
    crossing eight or more dark-light edges; the hero's ground and its
    tick cross at most four) and fails any frame with modules past 0.32
    closed or modules under the tick: before the change f7 and f8 failed
    in both themes, after it none.
291. **The fold row's label fades through, not across: the canvas's
    "crossfades" amended** (dev2, round 2 review; 188, TraySpec's motion
    table "Fold label and chevron: fade 200 · Show 18 more waiting
    crossfades to Show fewer"). A plain fade both ways drew the two
    labels over each other for about 100 ms ("Show fewerore waiting",
    the lead's fold21dark f7 to f12), which decision 188 forbids for two
    different texts. The designer's call, recorded here: the canvas's
    word was shorthand for "the label changes with a fade", so the fold
    row uses the till's preset for unlike labels, the one "Copy address"
    and "Done" use: the old out over `fadeQuick`, the new in over
    `fadeThrough` (its `fade` after the quick one's length), the chevron's
    turn on `settle` unchanged. `TrayTest.theFoldLabelFadesThroughNeverAcross`
    reads the label's ink at a quarter or more on every frame of the
    change; with the plain fade it found 6425 pixels where the labels
    alone draw 5664 and 3397, with the fade through no more than the
    longer label.
292. **Folding a scrolled tray holds the fold row to the pixel: found by
    its key, placed by a negative offset, and moved by nothing** (dev2,
    round 2 review; 269, 288's reading of `keepFoldInPlace`, TraySpec
    "the list scrolls so the fold row stays where the finger was"). The
    lead measured a 28 dp hop on the phone and 18 on the tablet. Two
    causes, both found by logging the list's layout under a new
    `TrayTest`: `keepFoldInPlace` took the fold's index in the tray's
    items (`items.indexOf`), four short of its index in the list (the
    top row, the notice, the summary and the chips come first), so it
    measured some row's offset, not the fold row's; and the list's item
    animator, with the item it kept its place by gone, slid the fold row
    and everything under it in from the list's far edge (`movingInFromEndBound`),
    from 2901 px to its place. Now the fold row is found among the items
    on screen by its key, the list is asked to put item `index − hidden`
    at minus its present offset (the list lays out the rows that stay
    above it, whatever their heights: no rounding to a row's height, so
    288's "a wrong count is a scroll of a row" no longer applies), and
    while the shift shows the fold so made, of those same sales, the
    items' placement spec is null, so no item slides; the next change
    (the tray opened, a sale paid or come) moves rows as ever. The flag
    is a fact about what is shown, not a frame: the rows compose in the
    list's measure, after the screen's composition, so a flag reset in a
    `SideEffect` was already false when they read it (tried first, it
    left the slide). `foldingAScrolledTrayKeepsTheFoldRowWhereItIs`
    holds the fold row within a pixel on each of thirty frames; before,
    it was off screen on frame 0.

293. **The paid bubble's tick leaves by fading whole, never un-drawn**
    (dev1, round 2 review; supersedes 288's "the tick un-drawing over
    `fadeQuick`"; `docs/motion.md` §12.5). Paid to Idle ran the tick's
    trim 1 → 0 on `fadeQuick`, so for about 85 ms it read as an eraser
    wiping the tick from its end while the lens spread to grey; nothing
    else in the till un-draws a glyph, and the spec's "then Idle on
    `fade`" means the tick goes as the rest of the paid look does, by
    fading. `Circle` now has two values for the tick, both read in the
    draw: its trim, which only grows (`Motion.tick` in, and out the new
    `Motion.untick`, a `snap()` held for `fadeQuick`'s 120 ms by `After`,
    so it rewinds to 0 only once the tick is invisible and the next paid
    draws it in again), and its alpha on `fadeQuick` both ways. A tick
    left mid-draw keeps the length it had and fades at that length; one
    paid again mid-fade comes back at its length, already whole. Under
    Full the alpha is 1 long before the trim starts (120 ms against the
    core's 200), so the way in is unchanged; under Still the tick, whole
    at once as before, now comes in on the same 120 ms fade as the
    lens's colours rather than in one frame, which is what the canvas's
    "no loops · 120 ms fades" row says for every change. `mark()` takes
    the alpha as one more argument; the frame still allocates nothing
    (`MarkOnDeviceTest`, now with a faded half tick), and leaving Paid
    recomposes nothing (`OverlayRecompositionsTest.theBubblesLeavingPaid`).
    `OverlayFramesTest.paidToIdleFadesTheTickWholeNeverUndrawsIt` reads
    the tick's start, corner and end against the lens beside each on
    every frame of the leave: they fade as one and none drops to the lens
    while another is still white; the old build fails it at 48 ms with
    the end at 0.06 of white and the other two at 0.97.
294. **A chasing bubble keeps `uiautomator dump` from ever seeing an idle
    window; the scripts work round it, the bubble does not change** (dev1,
    round 2 review; 274, `after.md` round 2 finding 8). The dump waits
    for the window to go idle, and the waiting bubble's border chases on
    every frame by design (one turn per 1250 ms, the owner's decision 6),
    so every screen-reading script step times out until the sale is paid
    or has expired. Pausing the chase for a dump would need a test hook
    in production code, and TalkBack, the one real reader, does not wait
    for idle and hears the bubble's label throughout
    (`OverlayFramesTest.theBubblesLabelSaysTheAmountAndTheStep`). So it is
    written down where a script author looks: `motion/baseline.md`'s
    emulator caveats (where `ui.sh` is described), `after.md`'s method,
    and the showcase report's notes on its clips: pay or expire the
    waiting sale before dumping, tap by coordinates, or read the Compose
    semantics through a test. The scripts themselves live in the lead's
    scratch folder, outside the repo, so their own notes are the lead's.
295. **A filter chosen after a kept fold moves its rows: the kept fold's
    held placement is cleared by the chip** (tech lead, round 2 final
    review; 292). Decision 292 places a fold kept under the finger with no
    placement move for as long as the shift shows that fold of the same
    `sales` list, so the next sales change moves rows again. A filter is
    the other input of the items, and it did not clear the flag: after a
    scrolled tray was folded, choosing Needs action or Paid jumped every
    row to its new place in one frame instead of settling there on
    `settle`, as a filter does everywhere else. The chip's handler now
    clears the kept fold as it sets the filter; the fold itself is
    unchanged. `TrayTest.aFilterChosenAfterAKeptFoldStillMovesItsRows`
    reads Paid's first row over the change: before, at its end on the
    first frame (1936 to 805 px in one step); after, between the two.
296. **A Back from a row tapped far down the tray comes back scrolled where
    the cashier left, and a code with no row to go to fades with its card,
    never left whole** (dev2, round 2 fix round 1; the designer's backfold
    and backfold2; 260 a, 289). With 21 sales waiting (the canvas's Main),
    the tray opened, scrolled to its end and its last row tapped, a Back
    opened the tray as 289 intends but on a list at its top: the stage
    composes only the screens it shows, so the shift is composed afresh by
    the Back and its list state (and the fold, and the filter, decision
    269's "until the shift is left") went with it while the sale was up.
    The row was not laid out, the ground had no end, and `wholeOn` drew a
    code with no course whole over the rows until the change landed, then
    cut it (about 300 ms). Two changes. (1) `ShiftPlace`, kept by the
    activity over the stage (`rememberSaveable`, so a rotation keeps it),
    is written by the row's tap (the sale and its offset from the list's
    top) and read once by the composition a Back starts: when the Back is
    from that sale, a `SideEffect` of that composition asks the list for
    the row's index at minus that offset (`requestScrollToItem`, as
    `keepFoldInPlace` does), so the first measure lays the row out where
    it was tapped and the ground has its end on the change's first frame.
    Kept by the sale and an offset, not a raw index, so a sale come or
    paid meanwhile still finds the row; the list clamps at its ends. A
    Back from a sale not left from its row (one that rose by itself) asks
    nothing and forgets any older place. The row's index counts the list's
    four items above the rows (and "Not watching" when it shows),
    `rowIndex`, checked by the test. (2) The guard: `wholeOn` places its
    code with a layer while its move runs (as `words` does, nothing at
    rest, no recomposition), and a leaving code whose ground has no course
    fades out on the stage's own progress, gone as the arriving screen's
    words begin (`1 − crossfadeOnTrack`). The review asked for it to go
    with the leaving words, by `fadeOutOnTrack`; tried first, it left two
    frames of 99% bare ground between the sale's words going and the
    shift's coming (`backWithNoRowToLandInNeverDrawsTheBareGround`, which
    the whole code had been hiding), the very frame 260 a removed. So the
    card is the last of the sale to go, handing over to the shift's words
    with no bare frame and no cut. `RunningMovesTest.backIntoARowScrolledFarDownTheTrayLandsInIt`
    (light, dark) taps the last of 21 tray rows after scrolling to the end,
    then Back: the ground has its end from the first moving frame, on
    screen, the same end to half a pixel on every frame, landing in the
    row, and no frame shows the code's pixels (a quarter of its contrast
    against the shift at rest, the shift's words left out) where the
    ground does not hold its card. Before: no ground on any frame and the
    whole code on every frame to landing, light and dark; after, none. The
    full `connectedDebugAndroidTest` on 5556: 219 tests, 0 failures. The no-row test now
    also fails a code still drawn past `1 − crossfadeOnTrack`.
297. **A back gesture peeked and cancelled leaves the shift's place for
    the Back that follows** (the lead, round 2 review 2; 296). 296 forgot
    the place in the `SideEffect` of the first composition that asked for
    it. A back gesture seeks the stage to the shift as it starts
    (`StageBack`'s `seekTo(0f, to)`), so a peek composes the shift, asks
    for the row and forgets the place; cancelled, the shift leaves
    composition, and the real Back composed it afresh at the list's top:
    no ground end on any frame and the code whole over the rows until its
    296 fade. Now the place is asked for once per list (`ShiftPlace.askedBy`,
    the list that asked; a shift composed afresh has a new one) and
    forgotten only once the stage has landed on the shift (the
    `LaunchedEffect(cameFrom)` that already runs as the change lands, when
    `stage.landed()`); a cancelled peek has not landed, so the place stays.
    A Back from a sale not left from its row still asks nothing (the sale
    must match), and a place can't outlive a landing, so an old one is
    never used later. `RunningMovesTest.backAfterACancelledPeekStillLandsInTheRowScrolledFarDown`
    peeks to 0.3 and cancels before 296's flow: before, no ground end and
    the whole code on f1 to f7 at least; after, it passes with the rest of
    `RunningMovesTest` (52) on 5558.
298. **The 17 MB of purgeable HWUI cache after a self-rise is left for
    the owner to read on his phone** (the lead, round 2 review 2; dev1's
    report). The emulators don't count it in PSS and the only real phone
    attached is the owner's, off limits to the team; PSS after a trim is
    level between r8 and r9 and inside the accepted step. On the phone:
    `mem-r8c.sh`'s flow (ten self-rises and Backs, 10 s at rest,
    `RUNNING_CRITICAL`), then `dumpsys meminfo` and `dumpsys gfxinfo`;
    if the cache shows in Graphics/GL and outlives HWUI's idle purge, the
    likely source is a screen-sized layer kept from the rise's
    `graphicsLayer`, and the fix belongs to the track that owns the rise.
299. **A card whose ground lets go of its code before the shift's words
    begin carries the code inside it, whole, for as long as it fits** (dev2,
    round 2 fix round 2; the designer's back1, back5 and back6, 260 a).
    A sale that rose by itself with its row two under the fold (the oldest
    of five waiting) and a Back: the tray opens in place (289) and the
    ground lands in the row, but the row is low on the screen, so the
    shrinking ground's top edge passed the code's top at about p 0.3 on
    the phone (0.5 on the test's window), and `WholeNode.draw` dropped the
    code there (the `holds` exit: the course was located, the layer's
    alpha was 1). The sale's words are gone by `fadeOutOnTrack` and the
    shift's begin at `1 − crossfadeOnTrack`, so nothing of either screen
    was drawn between: two frames of bare, near-full-screen ground. The
    two ways the review offered were weighed. Keeping the code drawn while
    the ground covers it is what it did. Fading it on `1 − crossfadeOnTrack`
    as 296's no-course path does was tried first: it dimmed the code on
    every Back into a low row from the first frame (backIntoFoldedRow's
    "a code wherever the ground holds it" failed on f5 to f7), and still
    left the code outside its card from the moment the edge passed
    (backIntoScrolledRow's no-code-off-its-card failed at p 0.38). Instead
    the card carries its code: when the course says (exactly, from the
    straight lines, the same answer on every frame) that the ground lets
    go of the code's box before the shift's words begin, the layer
    `WholeNode` already places while its move runs moves the code by the
    least whole-pixel step that keeps it inside the ground, read in the
    layer block as the alpha is, and it is drawn while that fits. So it is
    whole, sharp (whole pixels), on its card and at full contrast on every
    frame until the words are in, and goes when the card is too small to
    hold it, as every other Back's code does. A Back whose ground holds the
    code until the words begin (a row higher up, back2 to back4) is not
    moved at all, and an arriving code is never moved. `codeOffItsCard`
    now reads only the pixels outside the ground on each frame: inside it
    the code is on its card wherever it is carried. `RunningMovesTest.backAfterASelfRiseIntoARowUnderTheFoldIsNeverBare`
    (light, dark: five waiting, a1 the oldest, a self-rise, then Back)
    checks no bare frame, the ground letting go of the card's laid-out
    place before the words, the code's ink and white inside the ground on
    every frame until the words begin, and no code outside it. Before:
    f8 (p 0.50) no code, light and dark; after, it passes with the rest of
    `RunningMovesTest` (54) on 5556.
300. **A Back puts the shift's list back where it was, and the row under
    the finger only when it would otherwise be off screen** (dev2, round
    2 fix round 2; the designer's stepback; amends 296). With the list at
    its top and five waiting, the first tray row tapped and the sale paid,
    the step-back came back scrolled about 400 px: the title, the chips,
    "Waiting for payment" and the tray's first waiting sale above the
    screen. 296 asked for the tapped sale's row at its tap offset, but a
    paid sale has moved from the tray to the top of Earlier, so keeping
    that row under the finger scrolls the whole list by the rows now above
    it. `ShiftPlace` now also keeps the list's own place when the row is
    tapped (`firstVisibleItemIndex`, its scroll offset, and the last item
    shown whole), and the Back's first composition puts the list back
    there when the row's index now falls inside that window; only a row
    that would then be off screen (or cut at the top) is put under the
    finger as 296 did. Judged by item indices, not measured heights: the
    composition that asks has no layout yet, and a row that moved within
    the shown window lands within a row's height of where the window was.
    296's far-down case (21 waiting, scrolled to the end, the last row
    tapped) keeps its index and is restored by the list's place, so
    `backIntoARowScrolledFarDownTheTrayLandsInIt` and the peeked variant
    still pass. The place survives a rotation (the saver keeps the three
    new numbers). `RunningMovesTest.thePaidStepBackFromTheFirstTrayRowKeepsTheListAtItsTop`
    (five waiting, 18 paid so the list can scroll, the first tray row
    tapped, paid, the step-back): the title at its place before the tap,
    the tray's first waiting sale on screen, the paid row under it and on
    screen, the ground landing in it. Before: the title gone and the tray's
    first row at -7 px; after, it passes with RunningMovesTest (55) and
    TrayTest (15) on 5556.
301. **The shift list's items above the rows are one list, laid out and
    counted from the same place** (dev2, round 2 fix round 2; the lead's
    minor on 296). `rowIndex` added a hand-kept `HEAD_ITEMS = 4` (the top
    row, the notice, the summary, the chips, plus "Not watching") to a
    row's place among the rows; an item added over the rows would have
    put a Back's restored row one off, caught only by a device test. Now
    `Head.of(status, sales)` lists those items' keys in order, the
    `LazyColumn` composes one item per key from it, and `rowIndex` counts
    its size. Remembered on the three facts it reads, so it is made once
    per change of them, not per composition. No change on screen; the
    scrolled-row and step-back tests in `RunningMovesTest` (55) pass on 5556.
302. **`ShiftPlace` is not marked `@Stable`** (dev2, round 2 fix round 2;
    the lead's minor on 296). It holds plain mutable fields that a tap
    writes and no snapshot sees, so the annotation promised what the class
    does not keep (that a change would be observed). It is dropped rather
    than the fields made states: nothing should recompose when they
    change, and the one instance lives for the activity, so with Kotlin
    2.4's strong skipping a composable taking it compares it by identity
    and still skips.
303. **The showcase puts round 2 first, describes only the approved
    design, and marks round 1's open items in place** (the lead designer,
    round 2's report). `docs/reports/motion.md` opens with a "Round 2"
    section, and round 1's sections keep their text under "Round 1:"
    headings, so the before and after of round 1 still reads as it did.
    The round-2 section describes the owner's approved canvas only.
    Options the owner did not choose are not shown or compared there, so
    an implementor reading the report has one design to build to. Round
    1's open items are not deleted. Each is marked "Closed in round 2"
    with the decisions that closed it, or "Still open" with where it
    went, so the history stays checkable. The after clips were recorded
    on `f2dd97f` on emulator-5558 by coordinates, because `uiautomator`
    cannot read a screen while the bubble chases (294). They are in
    `docs/reports/motion/after-r2/` beside `before-r2/`, with one
    reduced-motion clip added (Back into a folded row with animations
    off), because the round's reduced-motion behaviour had no recording.
    The step-back clip shows only four frames of the move on the
    emulator, the two stalls already on the phone list. The report says
    so and points at the test that holds the move frame for frame.

304. **The round-2 PR is opened on `16dee74` and is not rebased onto the
    newer `main` before review** (the tech lead, ship step). Since
    `16dee74`, `main` has merged one more PR, #25 (`c14b7b3`, the Bell's
    environments). A trial `git merge-tree origin/main motion-main`
    conflicts in one file only, `docs/decisions/android.md`. #25 numbers
    its new decisions 147 to 150, and this branch numbers its own 147 to
    304 after the same 146. No code conflicts. #25 touches the Bell's
    client, `MonokularApp.kt` and `SquareConfig.kt`, and none of the
    motion files. `origin/motion-main` already exists at `82ea3c7`, an
    ancestor of this head, so the ship is a fast-forward push. A rebase
    now would need a force-push, which the ship step must not do. The
    renumbering went into the merge-time rebase instead: the owner had
    the branch rebased onto `main` and force-pushed to land it at once.
    #25's four decisions were renumbered rather than this branch's, since
    they are cited ten times and this branch's 147 to 304 hundreds: #25's
    147 to 150 are now 305 to 308, below, and its ten citations in the
    Bell's client, `MonokularApp.kt` and their tests point at the new
    numbers. This branch's numbers are unchanged.

## App: a Bell for several environments (2026-10-30)

The Bell (monokulo's `crates/monokulo-bell`, `docs/bell.md` there) now
rings several environments of each point of sale at once: Square's
production and its sandbox, Clover's regions and its sandbox. Its
`/v1/info` maps each point of sale to its environments, each to the
application it hears that environment through, and `POST /v1/register`
names the environment. Refines 95, 96 and 132.

305. **The till reads its own point of sale's own environment, and nothing
     else, from `/v1/info`.** `BellInfo.pointsOfSale` is point of sale →
     environment → application id. In acceptance T5.2's order: the point of
     sale absent, `Refused("it doesn't ring Clover tills")` as before; the
     till's environment absent, `Refused` in words naming it ("it doesn't
     ring Square sandbox tills", "it doesn't ring Clover tills in Europe"),
     final, as decision 132's "it's for Clover in Europe" was (a Bell for
     several environments can't be described by one, so the words name the
     till's instead); that environment's application not the till's,
     `SignInNeeded` through it, as decision 96 has it, which by
     construction is the same environment. Environments other than the
     till's are never read, so one this build doesn't know, or an entry in a
     shape it doesn't know, is passed over (`BellApi` drops an entry without
     a string `application_id`) instead of making the Bell "not a Bell" as
     an unknown environment did before: the Bell may grow environments
     before the app does. A Bell from before environments
     (`{"square":{"application_id","environment"}}`) reads as ringing none
     of the till's environments and refuses it in those words. The till's
     environment is Square's application's (`SquareConfig.environment`: an
     id starting `sandbox-` is the sandbox, any other production; the build's
     `monokular.square.sandbox` still picks Square's hosts) and Clover's
     build's `monokular.clover.environment` (decision 137).
306. **A registration remembers its point of sale and environment, and one
     made for another is taken back before registering.**
     `BellRegistration.pointOfSale` and `environment` are kept with it; a
     pass whose sign-in is for another point of sale or environment (a
     sign-in through another Bell's application, another build) `DELETE`s
     the old registration and `POST`s again, in the place decision 95 takes
     back one for an old endpoint, since the Bell keys registrations by
     endpoint. A registration kept from before this change has neither
     (both read as null) and counts as made for another, so the first pass
     after the update re-registers it once, `DELETE` then `POST`. Inferring
     the environment instead would have kept a registration the Bell may
     hold under its old, single environment; one extra round trip at one
     start is cheaper than a till the Bell rings wrongly.
307. **`POST /v1/register` sends `environment`**, the till's
     (`PosSignIn.environment`), between `point_of_sale` and the token; the
     body is still exactly its five fields, as the Bell refuses one it
     doesn't know. The answer's `environment` and `application_id` are not
     read: the till keeps the id and what it asked for.
308. **The contract test's monokulo pin waits for the Bell's change.**
     `monokulo.sha` stays at `2b200ef2a` in this branch, since the Bell's
     side isn't merged in monokulo yet; it must move to the monokulo commit
     that lands the multi-environment Bell once that merges, so the app is
     checked against the monokulo it ships with. The Bell's own shapes are
     held here by `BellApiTest` and `BellWakeUpsTest`, written from
     monokulo's `docs/bell.md` as that change has it.
309. **An order card opened by a tap scrolls itself into view** (the
     owner's device testing, 2026-10-31). `OrderCard` holds a
     `BringIntoViewRequester`; while a tap-opened card's fold runs, each
     frame of `shown` asks for the card in view (`collectLatest`, so the
     scrollable's one animation follows the growing card), and once more
     after the last frame. A card that opens by itself had the room for
     its lines and asks nothing, nor does one come back to open, so a page
     returned to from its code or order never jumps. It works on every
     page with the card: the live sale, the detail pages and the panel.
310. **The panel's order card is the sale screen's, under the code.** The
     panel's own order line (a bag, "15 items", the first few, above the
     code) read as a second receipt design and put what was bought above
     the code, against the sale screen. The panel now shows `OrderCard`
     after the code and its status, before the buttons; in a phone on its
     side it goes under the amount, beside the code. It folds by the same
     `ReceiptRoom` rule against the panel's own scrolling column, with a
     `ReceiptFolds` of the panel's for as long as it is up; it still fades
     in late (N3) and fades through with the header and amount when the
     panel moves on to another sale (decision 226). `OrderLine` is gone.
311. **The panel's code has the sale screen's enlarge badge, and a tap on
     it goes full screen**, as the sale's code does. The badge sits on the
     code's layer, so it folds into the hero and fades with the code; the
     "Full screen" button stays.
312. **A swiped row's danger ground reaches a corner under the card.** The
     strip was clipped at the card's edge, so the card's rounded right
     corners showed the tray's amber behind them as yellow wedges. The
     strip now starts a `RowCorner` further left, under the card, which
     covers it everywhere but those corners.
313. **A code leaving on its row's ground dims with the words** (answers
     234 for the way back). `wholeOn` kept a leaving code at alpha 1 while
     its ground held it, so on a Back, and through a back gesture's scrub,
     a bright white code sat over a page whose words and status foot had
     faded round it. Leaving on a located ground it now fades over
     `Motion.fadeOutOnTrack`, the words' own share, and the draw rule
     (whole or not at all) is unchanged; arriving it is still whole from
     its first frame (§10 rule 6), and the no-ground fade (296) and the
     carry (299) are as they were.
314. **The launcher icon is the Monokulo Till mark**: a square card
     terminal whose screen is the Monokulo lens redrawn on a square, with
     three keys (the last orange) and a card slot, chosen from the logo
     concepts (https://claude.ai/artifact/LiP5cRHAYLgrVM4X2YxKbs). The
     terminal is ink on paper, and paper on the night background
     (`drawable-night`) so it doesn't vanish on a dark launcher. It's
     drawn at 0.54 of its 120 grid so its rounded corners stay inside the
     adaptive icon's 66dp safe zone. The monochrome (themed) icon cuts
     the screen, keys and slot out of the body. Notifications keep their
     own small icons (`ic_mark*`), and the logo inside the app
     (`ui/Mark.kt`) is still Monokulo's.
