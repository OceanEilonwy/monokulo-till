# Monokular: companion app, POS integration and push

Monokular is the iPhone, iPad and Android app that sits next to a point of
sale (Square first) and shows the customer a Monero payment code when the
cashier takes a Monero sale. The code can carry the whole receipt, so the
customer's wallet keeps it with the payment. This document covers what the
app needs from [monokulo](https://github.com/OceanEilonwy/monokulo), the
Monokular Bell that wakes tills up, and the design of the app itself. Paths
under `crates/` in §5 and §8 refer to the monokulo repository; §9 is the
layout of this one.

1. [The sale flow](#1-the-sale-flow): the till talks to Square; monokulo
   never does.
2. [Connected apps](#2-connected-apps): how a device connects to one store on
   a self-hosted monokulo, and the order API it shares with shop plugins.
3. [Wake-ups](#3-wake-ups): what makes a till check Square, from a timer or
   the Monokular Bell.
4. [The receipt](#4-the-receipt): built on the till, carried in the code.
5. [Push](#5-push): a reusable `push` crate, Web Push everywhere,
   UnifiedPush, and a `push-relay` crate for APNs and FCM.
6. [The Live Activity](#6-the-live-activity).
7. [Battery and latency](#7-battery-and-latency).
8. [Monokulo code layout](#8-monokulo-code-layout), settings and migrations.
9. [The Monokular repository](#9-the-monokular-repository).
10. [Delivery plan](#10-delivery-plan) and [open questions](#11-open-questions).

Design mockups (iPhone, iPad, Android, web connect pages, onboarding and
settings): <https://claude.ai/artifact/Kb9AtJFBF3ockyddomLc6t>.

---

## 1. The sale flow

### Principles

- **The till holds the Square connection, not monokulo.** Monokulo has no
  Square permission, no Square settings and no Square code. It learns an
  amount and an opaque reference, nothing else, on the public instance and on
  every self-hosted one.
- **What someone bought never leaves the till except in the QR code** (§4).
- **Self-hosters set up nothing for Square.** Tills sign in to Square through
  the project's Square application; any monokulo works with it.
- **Monokulo stays POS-agnostic, and app-agnostic.** Another POS is app work
  only, and monokulo keeps no list of apps, as it keeps none of shop plugins.

### Why the cashier tenders "Other: Monero"

Square doesn't expose a sale rung up in Square Point of Sale until it has been
tendered, and a third party can't add a payment to a POS-created order or
complete it (`UpdateOrder` can't touch POS orders; the "open orders" alpha is
closed to new apps). So the cashier tenders with a custom payment type and
the till reconciles.

### Flow

```text
cashier: rings up in Square POS ─▶ taps "Other: Monero" ─▶ Square closes the sale
till:    woken by a timer or the Monokular Bell (§3)
         GET Square payments since its cursor, at its location
         ─▶ a new payment whose Other tender is "Monero"
         GET /v2/orders/{order_id} (line items, for the receipt)
         POST monokulo /api/v1/orders   (§2: the create-order form, with the app's key)
              { amount, currency, merchant_order_id, idempotency_key }
         ◀─ 201 { order_id, status, address, xmr_amount_piconero, expires_at, _links.self }
         payment URI = monero:{address}?tx_amount={XMR}
         orf_codec::query::append(payment URI, receipt) (§4)
         shows amount + code (Slide Over / bubble / app; Live Activity on iPhone)
customer pays ─▶ engine: order.unconfirmed ─▶ order.confirming ─▶ order.paid
monokulo ─▶ SSE / push "sale.seen" / "sale.confirming" / "sale.paid" (§5)
never paid ─▶ order.expired ─▶ "sale.expired" ─▶ the till offers
         "Refund the Square sale" (RefundPayment with its own token,
         or the cashier refunds in POS)
```

Nothing is written back to Square when the payment arrives: the Square sale
was recorded as "Monero" when the cashier tapped it. The only write is the
refund of a sale that never pays.

### Sales from Square, on the till

| Item | Value |
| --- | --- |
| Square application | One, owned by the project. Tills use it unless they're connected to a self-hosted Bell, which brings its own (§3) |
| Sign-in | OAuth with PKCE on the device: no secret anywhere. Redirect to `https://monokular.monokulo.com/square`, an App Link / universal link, so Square hands the code straight back to the app |
| Scopes | `PAYMENTS_READ` (find sales), `ORDERS_READ` (items for the receipt), `MERCHANT_PROFILE_READ` (pick the till's location), `PAYMENTS_WRITE` (refund a sale that never pays) |
| Tokens | Kept in the Keystore / Keychain, never sent to monokulo. PKCE refresh tokens are single-use and expire (90 days at the time of writing; the spike confirms): the app refreshes on every use and asks for a new sign-in when one lapses |
| Finding sales | `ListPayments` for the till's location since a saved cursor (`begin_time`, plus payment ids already handled) |
| Filter | Other tender whose name is the configured one (default "Monero"); status `COMPLETED` |
| Which till | The till's location, and its own `device_details` when Square gives them; otherwise every Monokular at the location shows the sale |
| Two tills, one sale | `idempotency_key` is `sq:` plus a hash of the Square payment id, so every till that finds it gets the same order |
| Sandbox | Square's sandbox can't run Square POS: verify with $0 Other tenders on a real account |

Say on the Square sign-in screen what it allows: `ORDERS_READ` lets the till
read every order of the business, not just Monero sales. That's the right
place for it, a device the merchant controls, but it should be said.

### POS sources

A POS is a **sale source** in the app: "the new Monero sales since my
cursor", plus "refund this one". Square is the first and Clover the second.
Another POS adds a source and, if it has webhooks, a Bell adapter (§3);
monokulo, the receipt, wake-ups and the screens don't change.

| | Square | Clover |
| --- | --- | --- |
| Monero tender | Other, with the note "Monero" | a tender labelled "Monero" (`labelKey com.clover.tender.external_payment`), known by its id |
| Webhook the Bell reads | `payment.created`/`payment.updated` of a `COMPLETED` payment, signed (HMAC over URL and body) | a batch of `P:` `CREATE` updates, ids only, with a static `X-Clover-Auth` header: every payment dings, as the body carries no tender |
| Location | a location of the merchant | the merchant itself |
| A sale never paid | refunded by the till over Square's API | refunded by the cashier in Clover's Orders app (no API refunds an external-tender payment); the till confirms it from the payment's refunds |

---

## 2. Connected apps

Monokulo already connects **shop plugins** (`http::connect`, the Shop plugins
page on the Pages site): any plugin names itself with a slug and a name,
monokulo keeps no list of them, and the plugin then makes orders with
Monokulo's HTTP API (`/api/v1`). Connected apps follow the same rules and use
the same API, but they connect differently, because a till isn't a shop.
Monokulo's docs (`/api/` and `/apps/`) and its OpenAPI document are the
source of truth; the app's contract test runs against that document,
generated from a pinned Monokulo commit (§9).

| | Shop plugin | Connected app |
| --- | --- | --- |
| Names itself | slug + name (`plugins::PluginSlug`, `PluginName`) | The same types and rules: `monokular`, "Monokular" |
| Per store | One: connecting a plugin closes the active one | Many, one per device |
| Starts from | The shop: the browser goes to `/connect/{slug}?site_url=…`, and the shop's host finds the store | The store's dashboard: a single-use connect code |
| Credentials | The store's secret key (`sk_…`) | The device's own `app_key` (`ak_…`), revocable on its own |
| Makes orders | The `create-order` form, `POST /api/v1/orders` | The same |
| Hears about them | Signed webhooks to the shop | SSE and Web Push (§5): a till can't take webhooks |

The plugin flow can't be reused as it is: a till has no shop host, and
connecting it as the store's plugin would disconnect the shop's WooCommerce
plugin.

### Web

- **Store settings › Connected apps** (a card beside Connections, where the
  store's plugin stays): every connected device, as the app named itself
  plus the device's name ("Monokular · Front Counter iPad"), last seen, and
  Disconnect.
- **Connect an app**: a device name, then a single-use connect code (QR) and
  the same as a link with a Copy button, for when the dashboard is open on
  the till itself. The page names no app. Without JavaScript the "waiting"
  line is a Refresh button.

### The connect code

```text
https://pay.yourshop.com/apps/connect#token=ct_…
```

- It is on the store's own host, so any app can use it and monokulo needs
  no app's domain. The app reads it with its scanner or from a paste; the
  fragment never reaches a server.
- Opened in a browser, the page says to scan it or paste it into the app.
- `token` is single-use and short-lived (10 minutes, like the plugins'
  connect tokens; `Db::consume_connect_token`).

### Redeeming

```http
POST https://pay.yourshop.com/apps/connect
Monokulo-Client: monokular/1.0.0
{ "token": "ct_…", "slug": "monokular", "name": "Monokular",
  "device_name": "Front Counter iPad", "platform": "android" }
→ 200 { "app_key": "ak_…" (secret, shown once), "api": "/api/v1",
        "onion_api": "http://….onion/api/v1" (only while the Monokulo has an onion address) }
```

The app sends it to the link's own address, the code's link without its
`#` part, so a Monokulo served under a path
(`https://example.com/monokulo/apps/connect#token=…`) is reached under it,
and its `api` (`/monokulo/api/v1`) carries the path too.

`slug` and `name` are parsed with the plugins' types, so the same rules hold
(a lowercase letter then 1 to 31 lowercase letters, digits or hyphens, not
one of monokulo's own sources; a name of at most 40 characters with no
invisible formatting). The path is `/apps/connect`, outside
`/connect/{slug}`, so no plugin slug can collide with it. Every error is a
problem (`application/problem+json`): `invalid-connect-code` (401: used,
expired or unknown, ask for a new one), `no-public-address` (503: the code
is still good), `invalid-request`, `unsupported-media-type`, `too-large`,
`rate-limited` (429, wait `retry_after` seconds), `challenge-required` (429:
the app solves the proof of work and sends it again with `Monokulo-Proof`)
and `internal`. The app tells them apart by `type` and shows `detail`.

The answer is the key and the API's entry, nothing else. `api` is a path:
the app resolves it against the address it redeemed the code at, so a till
that connected over an onion address stays on it, and keeps `app_key`
(Keychain / Android Keystore), that absolute entry, the onion entry when
there is one, and whether it goes over Tor (decision 55).
Every later request carries `Authorization: Bearer ak_…` and
`Monokulo-Client: {slug}/{version}`.

The app then reads the entry (`GET /api/v1`) and follows its links:
`mk:store` (`GET /api/v1/store`: name, base currency, and the wallet as
name, network and whole address), `mk:app` (`GET /api/v1/app`: this
device's name as the merchant set it), `mk:events`, `mk:onion-api`, and the
forms `create-order` and `disconnect`. Every link is relative (but the
onion one) and resolved against the URL the app actually called; the app
builds no path. A link or form the key may not use is left out; the app
doesn't try it. The entry is kept per key as its `Cache-Control` says
(`private, max-age=3600`), asked again with `If-None-Match` once it
expires, and read again at once when anything followed from it answers
`401`, `404` or `405` (decision 54).

**The store's details are read live, never saved** (decision 50): after
connecting (the confirm screen, "Connected to …", is drawn from that read),
each time Settings opens, and at app start (the base currency, the store's
name for the till and its notification). They're held in memory for the
run, so a screen shows the last read while the next is under way. The
wallet address is shortened on the device (first 8, "…", last 8); Settings
copies the whole of it.

Errors are `application/problem+json`; the app shows `detail`. A `401` at
the entry means the store disconnected this device (an unknown key is a
`401` on every route, the entry included). Disconnecting (either side)
deletes the connection, its key and its push targets: the app uses
the `disconnect` form (`DELETE /api/v1/app`). Square sign-in is a separate
step in the app (§1) and has nothing to do with monokulo.

**Upgrading.** A till that connected before Monokulo gave `onion_api`
kept only `app_key` and `api`. It carries on as it is, off Tor, and
learns the onion entry from the entry's `mk:onion-api` the next time it
reads the store; Settings then offers Tor. The first app's saved form
(`endpoint`, `public_key`, …), from before the API, was never released
and isn't read: such a till connects again.

### Making an order

The `create-order` form, with the app's key:

```http
POST https://pay.yourshop.com/api/v1/orders
Authorization: Bearer ak_…
Monokulo-Client: monokular/1.0.0
{ "amount": "24.50", "currency": "USD",
  "merchant_order_id": "#1042",         (Square's receipt number, or omitted)
  "idempotency_key": "sq:9f2c…" }       (a retry or a second till gets the same order)
→ 201 Location: /api/v1/orders/order_…
      { "order_id", "status", "is_final", "address", "xmr_amount_piconero",
        "amount", "currency", "merchant_order_id", "expires_at", …,
        "_links": { "self", "mk:order-events", "mk:checkout", … } }
```

- **The app never sends `description`.** It is the API's only free-text
  field, and anyone with the payment link can read it, so it must never
  carry items (§4).
- The order is recorded as made with the app's key, so the dashboard shows
  it as the app's, with the device's name.
- The order carries no payment URI. The app builds the plain one,
  `monero:{address}?tx_amount={XMR}`, and appends the receipt to it (§4).
- The app keeps the order's `self` link, and reads the order there again
  when its event stream reopens. A sale another till made reaches it only
  as a sale event, which links its order (`mk:order`): that one is read
  there.

### What a connected app can do

Make orders, read its store, its orders and its own connection, follow the
store's sale events, disconnect itself, and subscribe to pushes. Never
wallet changes, refunds or spending, and never the store's settings. The list
is shown on both the web connect page and the app's confirm screen.

### Live data while the app is open

```http
GET https://pay.yourshop.com/api/v1/events    (mk:events: SSE, Authorization: Bearer ak_…)
```

The store-wide stream of the same "sale" events as the pushes (§5): `ready`
first, `lagged` when some were missed, and a comment every 25 s. After
`ready` or `lagged` the app reads the orders on screen again, each at its
link. One order's
own stream (`mk:order-events`) is there for a page showing one order; the
app doesn't need it. Used whenever the app is in the foreground, so
on-screen updates never wait for a push. Because every till at a store gets
`sale.waiting` when any of them makes an order, a device without a Square
sign-in (a second iPhone, say) still shows the store's sales, without
receipts.

---

## 3. Wake-ups

The till finds sales by asking Square (§1). What makes it ask is separate,
so the sale source never cares why it was woken.

| Trigger | Where | When |
| --- | --- | --- |
| **Timer** | Every platform | Always, during a shift: every 2 s without the Bell, every 30 s with it (as a backstop for a lost ding) |
| **Ding** from the Monokular Bell | Every platform that registered with one | Within a second or two of the POS's webhook |
| **App opened / brought forward** | Every platform | Immediately |

Every trigger does the same thing: fetch since the cursor, idempotently. A
ding carries no sale data, so a lost, late, repeated or forged ding costs at
most one extra request to Square.

A "shift" is the time the app is set to watch for sales (on by default
while the app runs; the cashier can stop it in Settings). Outside a shift
nothing runs.

### Per platform

| Platform | Without the Bell | With the Bell |
| --- | --- | --- |
| Android | Foreground service, 2 s timer during a shift | Ding through UnifiedPush or FCM wakes the service; timer slows to 30 s |
| iPad | Slide Over keeps the app in the foreground: 2 s timer | Ding as an alert push (below); timer slows to 30 s |
| iPhone | Only while the app is open: the cashier switches to it after tendering | Ding as an alert push; the Notification Service Extension fetches the sale and rewrites the alert |

**iPhone and iPad dings are alert pushes, never silent ones.** iOS rations
and delays background (silent) pushes, so they can't be trusted to wake a
till. The Bell sends an alert with `mutable-content`. The Notification Service
Extension, which runs for up to about 30 s with network access, reads the
Square token from a shared Keychain group, fetches the new payment, creates
the order and rewrites the alert as "Monero sale · $24.50 · Tap to show
code". Tapping opens the app, which builds the receipt and shows the code,
and starts or updates the Live Activity (§6). If the extension runs out of
time the alert stays "New sale · Tap to show code" and the app does the rest.

### The Monokular Bell

A small service that turns a POS's webhooks into dings for the right tills.
It lives in the monokulo repository as the crate and binary `monokulo-bell`
(`crates/monokulo-bell`), beside the gateway and never inside it: the
monokulo binary depends on nothing of it. It was this repository's
`crates/monokular-bell` until 2026-10-29, when it moved there with its
history, with the stand-in Square and Clover (`tools/fake-square`,
`tools/fake-clover`) and with the part of this section that designed it.
Its design note is monokulo's
[docs/bell.md](https://github.com/OceanEilonwy/monokulo/blob/main/docs/bell.md),
its API and how to run one are at <https://docs.monokulo.com/bell/>, and the
choices behind it are monokulo's
[docs/decisions/bell.md](https://github.com/OceanEilonwy/monokulo/blob/main/docs/decisions/bell.md).
In short: a POS's payment webhook comes to the Bell, which verifies it
(Square's signature, Clover's Auth Code), reads the merchant, location and
payment id and drops the body, then sends each till registered at that
location an encrypted, VAPID-signed Web Push ding that carries no sale data.

In the app: **Settings › Wake-ups**: "Timer only" (default), "Monokular Bell"
(the build's `-Pmonokular.bell.url`, `bell.monokulo.com` by default), or "My
own Bell" with its address. Built on Android (decisions 90–99 and on): picking
a Bell reads its `/v1/info` and registers for the till's point of sale and
environment (one Bell may ring Square's production and sandbox at once): a
Bell without that point of sale refuses the till ("it doesn't ring Clover
tills"), as does one without the till's environment ("it doesn't ring Square
sandbox tills"), and if the Bell's application for it there differs, the app
asks for a new sign-in
through that application (the working sign-in stands until the new one
finishes). It subscribes through the UnifiedPush app installed (ntfy, say;
none installed says so, and the till stays on the timer) for the Bell's VAPID
key, and registers. A status card under the choices says how often the till
checks the point of sale, where the Bell stands and when the last wake-up
came; the timer runs at 30 s only while the Bell is connected, and drops back
to 2 s within a tick when it isn't, or when the timer finds a sale the Bell
didn't ring for. A ding wakes the running shift's check; outside a shift it
does nothing. Signing out of the point of sale, choosing Timer only and
disconnecting take the registration back first.

---

## 4. The receipt

The payment code can carry the whole receipt (`orf-codec`, in this
repository: an Open Receipt Format document compacted into one
`orf=<digits>` query parameter). What someone bought must not reach
monokulo, the Bell, the push relay, or anything else between the till and
the customer.

1. **Monokulo never sees the items.** It has no Square access at all, and the
   app never sends an order `description`, the order API's only free-text
   field (§2).
2. **The till fetches the order from Square itself** with its own
   `ORDERS_READ` token. Square POS has no on-device API for other apps, so
   this is a Square API call; the items travel Square ─▶ till ─▶ QR code, and
   Square has them anyway.
3. **`orf-codec` runs in the apps** through UniFFI bindings to the Rust crate
   (Kotlin and Swift), so there is one implementation of the format and its
   dictionary (§9).
4. **The app appends the receipt** to the plain payment URI it built from
   the order's address and amount, with `orf_codec::query::append(uri,
   &receipt)`, which adds `?orf=` or `&orf=` correctly and refuses a URI that
   already has one. The app keeps the plain URI and appends to it fresh each
   time. `orf-codec` knows nothing about Monero.
5. **Without the order, the code still works.** If the order can't be read
   or the receipt doesn't encode, the app shows the plain payment URI.
   Paying never depends on the receipt.

Only the app's own screens show the receipt code. Anything drawn from a push
(the Live Activity's "Show code in island", §6) uses the plain payment URI,
because that content comes through the relay.

---

## 5. Push

Monokulo pushes the state of each order (seen, confirming, paid, expired)
to the store's tills. The Bell's dings (§3) use the same Web Push transport.

### Goals

- A **reusable** piece of monokulo: any connected app can ask for pushes;
  Monokular is only the first user.
- **One transport**: Web Push (RFC 8030), encrypted (RFC 8291, `aes128gcm`)
  and signed (VAPID, RFC 8292), from monokulo and from the Bell.
- UnifiedPush works with no relay at all; APNs and FCM go through a relay
  (`push-relay`), embedded in monokulo or standalone.
- The relay learns as little as possible: no store, ciphertext for every
  ordinary notification, and a handles file that can't link a device's
  handles or name the senders it takes pushes from (each row sealed under a
  key only its endpoint gives). The relay itself sees a token and its
  sender together at each registration and push, as it must to hand them
  on.

### Why Web Push for everything

A Web Push subscription is `(endpoint URL, p256dh key, auth secret)`. Three
kinds of endpoint all accept the same request:

| Endpoint | Who runs it | Delivers to |
| --- | --- | --- |
| UnifiedPush distributor (ntfy, NextPush, …) | The user, or a public instance | Android app via its distributor |
| `https://relay.monokulo.com/v1/push/{handle}` | The project (standalone `monokulo-push-relay`) | APNs (iOS) or FCM (Android with Google services) |
| A browser's push service | Browser vendor | A future web app or admin page |

So a sender only ever does "encrypt, sign, POST to endpoint". Whether that
lands at a UnifiedPush distributor or the relay is the endpoint's business.

### VAPID ties a subscription to one sender

Each monokulo, and each Bell, has one VAPID key pair (P-256), generated on
first start and kept secret. The app reads the public key from
`GET /api/v1/push-targets` (`vapid_public_key`, fetched live each time it
subscribes, so a rotated key is never stale; the connect response is
unchanged) or from the Bell's `/v1/info`. The app passes it to whoever
issues the endpoint:

- UnifiedPush: the app gives it to the distributor at registration (the
  AND_3 spec's VAPID field), which then refuses pushes not signed by it.
- The relay: the app registers its APNs/FCM token **with the VAPID key**, and
  the relay accepts pushes to that handle only when the VAPID JWT verifies
  against it.

This needs no per-sender relay account: a sender is identified by its VAPID
key, rate limits are per key, and a stolen endpoint URL is useless without
the sender's private key. A till holds one subscription per sender (its
store's monokulo, and the Bell if it uses one).

### Two kinds of subscription

| Kind | Payload | Used for |
| --- | --- | --- |
| `encrypted` | RFC 8291 ciphertext | Every notification and ding, and every Android message |
| `live-activity` | Plain JSON (iOS applies Live Activity pushes itself, with no extension to decrypt) | Live Activity updates only, about the till's own sales (never another till's: the island would show the wrong sale): `phase`, `sale`, `amount`, `currency`, `xmr`, `expiresAt`, `waitingCount`, and `paymentURI` only when the device turned on "Show code in island" (§6) and only while the sale waits for its first payment (not after a part payment, when the full amount is no longer owed). Never the order's id. |

For `encrypted`, the iOS Notification Service Extension and the Android
receiver decrypt with the subscription's private key, which never leaves the
device.

### Two crates: `push` and `push-relay`

Push is split like the engine: a library every sender uses, and a relay that
runs embedded in monokulo or on its own, the way the engine runs inside
monokulo by default and as `monokulo-engine` when separate.

| Crate | Kind | Used by |
| --- | --- | --- |
| `push` | library | Every monokulo and the Bell (sending), `push-relay` (VAPID checks), tests |
| `push-relay` | library + binary `monokulo-push-relay` | Monokulo (embedded, cargo feature `relay`, on by default), or standalone |

Keeping them apart means a monokulo built with `--no-default-features`
drops the relay's server code, APNs/FCM clients and OAuth entirely, and
another Rust sender (the Bell) only takes the small crate. The Bell is
`monokulo-bell` in monokulo's workspace and takes `push` (and
`live-settings`, `sqlite`, `rate-limit`, `abuse`, `service-runtime`,
`outbound`) as path dependencies.

### The `push` crate (`crates/push`)

A library with no monokulo types and no server code.

```text
crates/push/src/
  lib.rs            re-exports; what a sender does: encrypt, sign, POST
  subscription.rs   Endpoint (https, or http to this machine; no onion, no userinfo,
                    ≤ 2048 chars), Origin, P256dh, AuthSecret, SubscriptionKeys, Subscription
  message.rs        Urgency, Ttl, Topic (1–32 URL-safe chars), PlainJson (≤ 4096 bytes),
                    Message { Encrypted | PlainJson }, Options; MAX_PLAINTEXT = 3993
  encrypt.rs        RFC 8291 aes128gcm: encrypt (fresh salt and key per message);
                    decrypt and ReceiverKeys behind the test-support feature
  jwt.rs            Es256Key: compact ES256 JWTs, shared by VAPID and the relay's APNs auth
  vapid.rs          VapidKey, VapidPublicKey, Contact, Claims, Authorization, verify
                    (aud = endpoint origin, exp 12 h ahead, at most 24 h)
  wire.rs           the RFC 8030 headers; PushRequest::parse, what a relay reads
  send.rs           Sender over the caller's reqwest::Client: one token per origin
                    for 11 h; answers sorted into Sent or SendError (Gone, TooLarge,
                    RateLimited / Server with Retry-After, Refused, Request, Vapid)
```

Written on `p256` 0.14, `hkdf`, `aes-gcm` and `sha2` (the RustCrypto
generation already in monokulo's lock), tested against RFC 8291 Appendix A
and RFC 8292's example token; no `web-push-native` or `jwt-simple`
(monokulo `docs/decisions/push.md` 3 and 4). Builds on stable.

### The `push-relay` crate (`crates/push-relay`)

```text
crates/push-relay/src/
  lib.rs            Relay::start(&RelayConfig), Relay::router() -> axum::Router,
                    Relay::serve(listener, shutdown); the hourly sweep of expired handles
  settings.rs       the settings, declared once (live-settings): RelayConfig, load(), Host
  cli.rs            `monokulo-push-relay`'s command line, from the declarations
  routes.rs         POST /v1/register, POST /v1/push/{handle}  (no unregister: nothing
                    would call it; handles expire and die when APNs/FCM say the token is gone)
  handles.rs        Handle, HandleId (ph_…), Platform, DeviceToken; HandleStore on sqlite
                    (its own file, migrations/0001_handles.sql); no trait, no in-memory store
  apns.rs           APNs: HTTP/2 (prior knowledge), ES256 provider token renewed at 50 min;
                    alert, liveactivity (always an update)
  fcm.rs            FCM HTTP v1: RS256 service-account assertion (rsa) exchanged for an
                    access token, kept until 5 min before it expires; data messages
  limits.rs         per sender, per handle and per address, on the workspace's rate-limit crate
  upstream.rs       what APNs's and FCM's answers mean for the sender (gone, too large,
                    unavailable with Retry-After, refused)
  problem.rs        the problem+json answers
  main.rs           `monokulo-push-relay`: --options, --init, serve until SIGTERM
```

The library interface is small on purpose: settings in, a `Relay` out,
served on a listener. Monokulo (`crates/monokulo/src/relay.rs`) and the
standalone binary build it the same way and differ only in who owns the
listener and where the settings come from (`[relay.*]` of monokulo's
options file, or `relay.toml`).

### Crates

Chosen to fit the workspace: rustls only (there's no OpenSSL in `Cargo.lock`),
RustCrypto, reqwest, axum, tokio.

| Need | Crate | Why |
| --- | --- | --- |
| RFC 8291, VAPID | `p256` 0.14 (`ecdh`, `ecdsa`), `hkdf`, `aes-gcm`, `sha2`, written in `push` | The RustCrypto generation already in the lock; `web-push-native` 0.5 pins the one before it and would duplicate the crates for about 150 lines of well-specified code, tested here against the RFCs' own vectors (`push.md` 3). |
| JWT (VAPID ES256, APNs ES256) | `push::jwt::Es256Key` on `p256` | One signature over two base64url segments; `jwt-simple` would bring `k256`, `p384`, `ed25519-compact` and more (`push.md` 4). |
| FCM OAuth (RS256) | `rsa` 0.9 (already in the tree) | A thin client: one service-account assertion exchanged at `token_uri`. `gcp_auth` adds `ring` and discovers credentials by itself, against the settings rule (`push.md` 5). |
| HTTP | `reqwest` (already used), `http2` feature | APNs requires HTTP/2. The feature reaches every reqwest client in a monokulo built with the relay (`push.md` 118). |
| Server | `axum`, `tokio` (already used) | Relay and Bell routes. |
| Rate limits | the workspace's `rate-limit` crate | Fixed windows with `Retry-After`, already serving the engine and monokulo; keyed by VAPID key, handle and address (`push.md` 6). |
| Storage | `rusqlite` through the workspace's `sqlite` crate | Monokulo's tables, the relay's handles, the Bell's registrations. |

For APNs, `a2` was considered and not used: it hasn't been released since
2024 and has no Live Activity push type. APNs is a single HTTP/2 POST
(`/3/device/{token}`, headers `apns-push-type`, `apns-topic`,
`apns-priority`, `apns-expiration`, `apns-collapse-id`, bearer JWT refreshed
every 50 minutes), so a thin client on reqwest is smaller and easier to keep
current.

### The relay API

Registration, from the app:

```http
POST /v1/register
{ "platform": "apns" | "apns-liveactivity" | "fcm",
  "token": "<APNs/FCM device or activity token>",
  "app": "monokular",                   // picks the bundle ID / FCM project
  "vapid_public_key": "BJ…" }           // the sender: a monokulo or a Bell
→ 201 { "endpoint": "https://relay.monokulo.com/v1/push/ph_7f3k…",
        "expires_at": … }
```

- One registration per (platform, token, app, VAPID key): a device connected
  to two stores gets two handles, which nothing in the relay's file links
  (the handle id is an HMAC of the four under the relay's handle key,
  which is never in the file; the row is keyed by the id's hash, and the
  token and key are sealed under a key derived from the id). A copy of
  the file alone links nothing; with a sender's endpoints it opens that
  sender's handles, so their tokens, but not those devices' handles for
  other senders, which need the key too; with the key as well it links
  each such device to every sender whose public key is known, so the key
  is kept apart from the file and its backups. Registering the
  same four again gives the same endpoint, its expiry pushed back. A handle
  no push has reached lasts `handles.unused_ttl_days` (7) after its
  registration, since registering needs no credential; once APNs or FCM
  has taken a push for it, `handles.ttl_days` (90) after its last
  registration or delivered push.
- There is no unregister route: it would be unauthenticated (the app holds
  no secret the relay knows) or sender-only, and nothing would call it.
  Handles expire, and die as soon as APNs or FCM says the token is gone.
- Tokens change; the app re-registers whenever iOS or Android hands it a new
  one and sends the new endpoint to monokulo and the Bell.

Delivery, from a sender (plain Web Push):

```http
POST /v1/push/ph_7f3k…
Authorization: vapid t=<JWT>, k=<public key>
Content-Encoding: aes128gcm          (encrypted kind)  |  application/json (live-activity kind)
TTL: 1800
Urgency: high | normal | low
Topic: 3f9c0a7e…                     (32 hex digits: a hash of the target and the
                                      order, which collapses superseded updates,
                                      differs on every device and tells the relay
                                      nothing)
```

The relay checks the JWT against the handle's key, applies limits, and
translates:

| Handle platform | APNs/FCM request |
| --- | --- |
| `apns` | `apns-push-type: alert`, `mutable-content: 1`, the ciphertext in a custom key; the Notification Service Extension decrypts and fills in title and body (for a ding, it fetches the sale first, §3). Fallback text: "New sale". |
| `apns-liveactivity` | `apns-push-type: liveactivity`, topic `<bundle>.push-type.liveactivity`, `aps.content-state` from the plain JSON, always `event: update` (the relay never ends an activity, which the app idles or ends itself), `alert` only for paid ("Paid · 24.50 USD") / expired ("No payment yet"), `stale-date` from `expiresAt` while the sale is open. |
| `fcm` | Data message, priority from `Urgency`, the ciphertext as data; the app decrypts. |

Answers back to the sender, as problem+json: `201`; `404 unknown-handle`
when APNs/FCM say the token is dead (the relay deletes the handle; the
sender deletes the target) or the handle expired; `401` when the VAPID
signature isn't the handle's sender's; `413`; `429` and `503` with
`Retry-After`; `502` when APNs or FCM refuse the relay's own setup. The
full list is the docs page `/push-relay/` (monokulo `xtask/src/pages/docs/push-relay.md`).

### Running the relay

**Embedded** (in every release build of monokulo, off unless turned on):

```toml
[relay]
enabled = true                 # default false: only useful with APNs/FCM keys

[relay.server]
listen = "0.0.0.0:8444"        # its own listener (8443 is the engine's), so its limits
                               # and logs stay apart from the store pages
public_uri = "https://relay.example.org"

[relay.handles.apps.monokular]
apns_topic = "com.example.monokular"
fcm_project_id = "monokular-12345"

[relay.apns]
team_id = "ABCDE12345"
key_id = "FGHIJ67890"
```

**Standalone:**

```sh
monokulo-push-relay --options relay.toml
```

Both read the same settings, declared once in `push-relay` and registered
by both binaries (the same pattern as the engine's `[engine.*]` /
`--engine-…` settings in monokulo). Every key has two parts, so it can sit
under monokulo's `[relay.*]` tables: `server.listen`, `server.public_uri`,
`server.trusted_proxies`, `server.upstream_timeout_ms`, `store.path`,
`handles.ttl_days`, `handles.unused_ttl_days`, `handles.apps` (bundle ID
and FCM project per app), `limits.per_sender_per_minute` (a sender is a
whole monokulo or a whole Bell, so it is sized for the busiest; a push the
handle's own limit refuses doesn't count), `limits.per_handle_per_minute`,
`limits.registrations_per_minute`, `apns.endpoint`, `apns.team_id`,
`apns.key_id`, `fcm.endpoint`, and for the standalone binary alone
`logging.level` and `logging.format`. `relay.enabled` is monokulo's own,
shown read-only on its admin page; the `[relay.*]` settings are read from
the options file and `--relay-…` at start and are not edited on the admin
page (a follow-up, `push.md` 16). They are checked at every start, the
relay on or off, so a mistake stops monokulo then. Invalid values stop the
process, as everywhere else in monokulo. Secrets come only from the
environment, in both cases:

```text
MONOKULO_RELAY_HANDLE_KEY       (required: every handle id is made under it; openssl rand -hex 32, kept)
MONOKULO_RELAY_APNS_KEY         (.p8 contents)
MONOKULO_RELAY_FCM_CREDENTIALS  (service-account JSON)
```

**Storage:** the relay always keeps its own SQLite file (`store.path`, by
default `push-relay.db` beside monokulo's database when embedded), never
tables in monokulo's database. It never touches store data, and moving
from embedded to standalone is a matter of copying that file (and keeping
the same handle key). It holds only `handles(key, platform, app, sealed,
delivered, expires_at)`: the id's hash, the token and sender key sealed,
and an expiry to the hour. No payloads are stored and no request bodies,
tokens or endpoints are logged (a test reads the whole log to check).

**Who runs what:** the project runs the official relay **standalone**, next
to the official Bell, so both can be deployed and scaled on their own
schedule. Embedding suits a small operator shipping their own app with their
own APNs/FCM keys: one process, one upgrade. Either way their app registers
with that relay and hands the endpoint to its senders, with no change to
monokulo.

**Release artifacts:** `monokulo` (relay included), `monokulo-engine` and
`monokulo-push-relay` and `monokulo-bell` (the Bell, since 2026-10-29),
all from the monokulo workspace.

### UnifiedPush

On Android the app uses the UnifiedPush connector. With a distributor
installed (ntfy, NextPush, …) the endpoint is the distributor's and pushes
go **straight from the sender to the distributor**, with no relay. Without
one, the app falls back to FCM through the relay (or the UnifiedPush
embedded FCM distributor pointed at the relay; check that library's gateway
contract before choosing). On a phone without Google services and without a
distributor, Monokular keeps its foreground service on the store's SSE
stream and the Square timer during a shift (§7).

### Push targets in monokulo (`push_targets`)

```text
vapid_keys(id, public_key, private_key_encrypted, created_at_utc)
push_targets(id pt_…, store_id, app_id, client_id, kind encrypted|live-activity,
             endpoint, p256dh, auth, events, include_payment_uri,
             created_at_utc, last_ok_at_utc, failures, registered_seq)
```

- Registered by the app with its key (`ak_` only): `POST` the
  `register-push-target` form on `GET /api/v1/push-targets` (which carries
  `vapid_public_key`), found from the entry's `mk:push-targets` like
  everything else; `GET` and `DELETE /api/v1/push-targets/{id}`. The app
  picks a `client_id` per subscription; registering it again replaces the
  subscription on the same target (the design's `PUT`, as `api-macros`
  has `get`/`post`/`delete` and every link names a `GET`-able resource).
  At most 8 targets per app. Of a store's targets on one endpoint (one
  subscription), only the last registered that wants an event is pushed
  (`registered_seq`, bumped by every registration, in place or new): a
  device re-paired as a new connection gets the same relay handle with
  new keys, and is pushed with those (monokulo `push.md` 145).
- `kind` and keys can't mix: `encrypted` needs `p256dh` and `auth`,
  `live-activity` takes none and may set `include_payment_uri` (the "Show
  code in island" switch, decided at registration). A `CHECK` in the table
  says the same.
- Removed when the connection is disconnected (cascade), when the endpoint
  answers 404/410, or after 20 counted failures in a row
  (`push.target_failure_limit`; `429` doesn't count).
- `events` is a list of the seven `sale.*` names, every one by default; no
  glob syntax.

### Events

Every event is about an order a connected app made (§2); the payload has
the order id, its `merchant_order_id`, the amount, the address,
`xmr_amount_piconero` and `expires_at`. No event carries items.

| Event | From | Urgency | Notification | Live Activity |
| --- | --- | --- | --- | --- |
| `sale.waiting` | An app made an order | normal | none (the till that made it already shows it; the other apps' encrypted targets get it) | phase waiting, only on the till that made it (`order.pending`, waiting again) |
| `sale.seen` | `order.unconfirmed` | low | none | phase seen |
| `sale.confirming` | `order.confirming` | low | none | phase confirming |
| `sale.paid` | `order.paid` / `order.overpaid` | high | "Paid · $24.50" | phase paid, alert; the app idles the activity |
| `sale.partial` | `order.partial` | high | "Part paid" | phase waiting, no `paymentURI` (the engine's event doesn't carry what arrived, so no "amount left", and the full amount is no longer owed: the checkout asks for the rest) |
| `sale.expired` | `order.expired` | high | "No payment yet" | phase expired, alert |
| `sale.double_spend` | `order.double_spend_detected` | high | "Payment reversed" | phase expired |

Payloads use the same body versioning as webhooks (`api_version`) so a
connected app can rely on them. `TTL` is the time left on the code (at
least a minute, at most `push.default_ttl_secs`, 30 minutes) for the
events before a sale is settled, and 30 minutes for `paid`, `expired` and
`double_spend`; `Topic` is the first 32 hex digits of SHA-256(target id,
0x00, order id), the same for every push about one order to one target and
different on every target, so no two of a store's devices carry the same
value (monokulo `push.md` 140).

### Delivery path

Pushes ride the existing webhooks pipeline rather than a parallel one:

```text
engine order_events ──SSE──▶ webhooks::subscriber
                               │ same transaction
                               ├─▶ webhook_deliveries   (unchanged)
                               └─▶ push_deliveries      (new: one row per target)
push::delivery worker ──Web Push──▶ endpoint (distributor or relay)
```

`push_deliveries` reuses the webhook worker's shape (claim, send, back off,
give up), with two differences: a newer delivery with the same `Topic`
supersedes an unsent older one, and deliveries past their TTL are dropped,
not retried (a "Seen" update an hour late is worse than none). The row
keeps the plaintext payload and each attempt encrypts it afresh (RFC 8291
wants a fresh salt and key per message). Retries wait 5, 20, 80 and 320 s
or what `Retry-After` says, never past the TTL, five attempts at most.
Nothing runs until an app registers a target: there is no `push.enabled`.

---

## 6. The Live Activity

A status display for the current sale on iPhone. It creates nothing and
fetches nothing (Live Activity code has no network); the app starts or
updates it when it finds a sale (§3), monokulo's pushes move it through the
later phases, and iOS redraws it.

**Static attributes** (set by the app when it starts the activity, so never
through the relay): location label, store name.

**Content state**:

| Field | Example | Use |
| --- | --- | --- |
| `phase` | idle / waiting / seen / confirming / paid / expired | Layout and colour |
| `sale` | "#1042" | Label |
| `amount` | "24.50" | Large amount, formatted by the device with `currency` |
| `currency` | "USD" | With `amount`: no server formats money for the device's locale |
| `xmr` | "0.1421" | Second line |
| `expiresAt` | 1761312000 | Unix seconds: a countdown iOS draws itself (`Text(timerInterval:)`), and `stale-date` |
| `waitingCount` | 2 | The till's own app orders still waiting for a payment, this one included while it is: "+1 more" at 2 |
| `paymentURI` | `monero:…` | **Only** when the device has "Show code in island" on; the plain URI, never the receipt (§4) |

**Show code in island** is a per-device setting, off by default. Turning it
on explains that the payment address of each sale then passes through the
Monokular push relay, and that the island's code has no receipt. Off: the
expanded island shows amount, status and "Tap to show code"; the QR appears
only in the app.

| Phase | Compact | Expanded / lock screen | Set by |
| --- | --- | --- | --- |
| idle | Logo + green dot | "Watching for sales · Front Counter" | the app |
| waiting | Logo + "$24.50" | Amount, XMR, countdown, QR if enabled | the app, on finding a sale |
| seen | Logo + "Seen" | "Seen on the network" | push, silent, low |
| confirming | Logo + "1/2" | "Confirming" | push, silent, low |
| paid | Tick + "$24.50" | "Paid · 0.1421 XMR received" | push, alert, high; the app idles the activity |
| expired | Clock | "No payment yet · refund in Square" | push, alert, high |

Lifecycle: one activity per shift, started by the app (iOS ends it after 8 h;
the next app launch starts a new one). There is no push-to-start: nothing
would send one, since monokulo's targets carry only the content state, so
the relay has no platform for it (monokulo `push.md` 157); it comes back
as a monokulo target kind and a relay platform in one change, with the
attributes it would send through the relay decided then. Monokulo's pushes
never end it: the relay sends every state as an `update` (`paid` and
`expired` with an alert), and the app takes the activity back to idle or ends it itself
(monokulo `push.md` 113). The island shows only the till's own sales
(monokulo `push.md` 141): monokulo pushes a till's Live Activity nothing of
another till's, and sends a target's pushes in the order they were queued,
so overlapping sales show the newest plus "+N more".
Tapping opens `monokular://sale/{id}`.

The waiting phase needs the app to run, so on iPhone it appears when the
cashier opens the app or taps the Bell's alert (§3), not on its own.

---

## 7. Battery and latency

| State | Finding sales | Order updates | Cost |
| --- | --- | --- | --- |
| App in the foreground | 2 s timer (30 s with the Bell) | SSE to the store's monokulo | Normal app use |
| iPad in Slide Over | Same | Same | Normal app use; tills are usually on chargers |
| iPhone in the background | Bell alert + Notification Service Extension; nothing without the Bell | APNs via the relay | Close to nothing |
| Android with a UnifiedPush distributor | Foreground service: Bell ding, and a 30 s timer while the Bell is connected (2 s otherwise) | Direct Web Push to the distributor | The distributor's one connection, plus the timer during a shift |
| Android with Google services | Same, dings via FCM (not built yet: no FCM keys or deployed relay, so without a UnifiedPush app it is the row below) | FCM via the relay | Close to nothing beyond the timer |
| Android with neither | Foreground service on the 2 s timer | SSE in the same service, heartbeat 25 s | Small; only during a shift |

Rules:

- Timers run only during a shift and slow to 30 s whenever a Bell is set up.
- Push on state change only; at most five pushes per sale.
- `high` urgency only for dings, paid, partial, expired and double spend;
  everything else `low` or `normal`. APNs and FCM throttle apps that overuse
  high priority.
- `Topic` collapses superseded updates; `TTL` equals the time left on the
  code (60 s for dings).
- Monokulo sends one store's apps at most `push.per_store_per_minute`
  pushes a minute (120), and targets sharing an endpoint once per event (as
  the last registered), so
  one store can't spend the relay's per-sender budget, which every store of
  an instance shares (monokulo `push.md` 144, 145).
- iOS: `NSSupportsLiveActivitiesFrequentUpdates` for busy tills.
- Square requests: one `ListPayments` per tick per till, plus one order read
  per Monero sale. The spike measures Square's limits for a busy store with
  several tills on one application.

---

## 8. Monokulo code layout

```text
crates/push/                    new library (§5): Web Push, VAPID, RFC 8291
crates/push-relay/              new library + `monokulo-push-relay` binary (§5)
crates/outbound/                new library: the private-address rule and the checking HTTP client,
                                for monokulo's webhooks and pushes and the Bell's dings
crates/monokulo/                depends on push; on push-relay behind feature "relay" (default)
crates/monokulo/src/
  apps/                         connected apps
    mod.rs                      AppConnection, app keys, connect tokens (the plugins' token model)
    events.rs                   the sale.* event model, shared by SSE and push
  plugins.rs                    PluginSlug and PluginName, now also naming apps
  push/                         monokulo's side of push: Push (the VAPID key), policy.rs (urgency,
                                TTL, topic per event), payload.rs (the Sale JSON and the Live
                                Activity state), delivery.rs (the worker); db/push.rs holds the
                                push_targets and push_deliveries queries, queued from
                                Db::queue_order_event in the event's transaction
  outbound.rs                   the checking HTTP client webhooks and pushes share (no private
                                addresses unless webhooks.allow_private_urls, no redirects, no onion)
  api/v1/push_targets.rs,
  api/v1/push_target.rs         GET/POST /api/v1/push-targets, GET/DELETE /api/v1/push-targets/{id}
  http/apps.rs                  /apps/connect (the page and the redeem call)
  api/v1/                       the HTTP API: the entry, store, orders, order, events, app;
                                an app's key (ak_) is taken wherever the store's secret is
  views/connected_apps.rs       the Connected apps card and Connect an app page
  relay.rs                      (feature "relay") reads and checks [relay.*] at every start and,
                                when relay.enabled, starts a Relay on its own listener; without
                                the feature, relay.enabled and [relay.*] stop monokulo
```

There is no Square code in monokulo.

### Migrations

New tables only (numbered after 0042); nothing existing changes shape, and
no data moves: the store's shop plugin stays where it is.

- `app_connections` (store, slug, name, device name, platform, key hash,
  last seen), `app_connect_tokens`
- `0052_push_targets.sql`: `vapid_keys` (encrypted private key) and `push_targets`
- `0053_push_deliveries.sql`: `push_deliveries`, and a nullable
  `app_orders.last_sale_event` for the Live Activity's `waitingCount`
- none for the relay: its handles live in its own SQLite file (§5)

### Settings (registry)

- `push.delivery_timeout_ms`, `push.max_attempts`, `push.default_ttl_secs`,
  `push.keep_days`, `push.target_failure_limit` (a "Pushes to apps" card on
  the admin Payments tab); no `push.enabled`: nothing runs until an app
  registers a target
- `relay.enabled` (monokulo's own) and `[relay.*]` (declared in push-relay,
  read by monokulo at start when built with the relay, §5)

### Tests

In-process like the rest of monokulo wherever a push needn't land
(queueing, superseding, TTL, give-up, target deletion); where it must, a
stub UnifiedPush endpoint on `127.0.0.1:0` takes it and
`push::encrypt::decrypt` checks what arrived, in tests named `over_http`.
There are no provider traits or in-memory stores for tests: the relay's
APNs and FCM endpoints are settings, so its tests point them at stand-ins
on loopback (HTTP/2 with prior knowledge for APNs). Monokulo's
`push::pipeline_tests::relay_tests` run the whole chain once: a relay
served in-process, a sale pushed to its handles, and what the stand-in
APNs and FCM received read as the device would. App keys on
`/api/v1/orders` are tested for idempotency across two apps, the app
recorded as the source, and a disconnected app's key refused.

---

## 9. The Monokular repository

A separate public repository, `monokular`, holding both apps, the
receipt codec and their shared contract. The apps are native (SwiftUI,
Jetpack Compose) over one Rust core.

> **Changed 2026-11-08.** Everything platform-neutral moves into a Rust core
> shared by both apps, as in Tern and Fob, replacing the native cores
> (`MonokularKit`, the Kotlin `core`) below. The crates, the iOS layout and
> CI are in [ios-plan.md](ios-plan.md); the reasons are in
> [decisions/core.md](decisions/core.md). The layout below is the original
> one, kept for its Android modules and `protocol/`.

GitHub description:

> Monokular: iPhone, iPad and Android companion for Monokulo. Take Monero next to your till: it sees Monero sales in your POS (Square first) and shows the customer the payment code, in the Dynamic Island, Slide Over or a floating bubble.

```text
monokular/
  README.md
  Cargo.toml                    Rust workspace
  crates/
    orf-codec/                  ORF receipts to and from `orf=<digits>` (library, no Monero, no QR)
    orf-codec-ffi/              UniFFI bindings: a Kotlin package and a Swift package
  tools/receipt-qr/             corpus, dictionary training and QR size measurements
  protocol/                     the contract with monokulo and the Bell
                                (Monokulo's own OpenAPI document isn't kept here: it is
                                generated from the commit in android/gradle.properties'
                                `monokulo.sha` by tools/monokulo-openapi/ for the contract
                                test, as Monokulo never commits it either)
    events.schema.json          sale.* payloads, dings and the Live Activity content state
    vectors/                    JSON test vectors generated by monokulo's xtask:
                                connect links, RFC 8291 payloads (keys, ciphertext, plaintext), events
    square/                     recorded Square payments and orders (Other tenders, refunds)
  ios/
    project.yml                 XcodeGen (no .xcodeproj in git)
    Packages/MonokularKit/      SwiftPM package, no UIKit; builds and tests on Linux CI too
      Sources/
        Model/                  Store, Sale, SalePhase, Event decoding
        API/                    connect, orders, SSE client, push-target and Bell registration
        Sales/                  SaleSource (Square: PKCE, payments cursor, orders, refunds),
                                wake triggers (timer, ding, app opened)
        Receipt/                Square order ─▶ ORF, then orf-codec through its Swift bindings
        Crypto/                 RFC 8291 decrypt (swift-crypto, which is CryptoKit's API off Apple platforms)
        Links/                  connect-code parsing
      Tests/                    runs protocol/vectors and protocol/square
    Monokular/                  the app
      App/                      entry point, routing, monokular:// links
      Features/
        Onboarding/  Sale/  Settings/
      Services/                 Keychain (shared group), LiveActivityController, PushRegistration, Scanner
      Resources/                assets, the loading-mark animation, strings
    MonokularWidgets/           Live Activity (compact, minimal, expanded, lock screen)
    MonokularNotifications/     Notification Service Extension: decrypt, handle dings (fetch the
                                sale, make the order), fill in alerts
    MonokularUITests/           snapshot tests of every screen for CI screenshots
  android/
    settings.gradle.kts
    core/                       pure Kotlin (JVM): model, API, SSE, RFC 8291 decrypt, connect codes; runs protocol/vectors
    sales/                      SaleSource (Square: PKCE sign-in, payments cursor, orders, refunds),
                                wake triggers, the shift's foreground service
    receipt/                    Square order ─▶ ORF, orf-codec through its Kotlin bindings
                                (native library per ABI)
    push/                       UnifiedPush connector, FCM fallback, registration with monokulo and the Bell
    overlay/                    the bubble and floating panel (display over other apps)
    notifications/              Live Update / ongoing notifications and their progress
    app/                        Compose UI: onboarding, sale, settings
  docs/
    architecture.md  release.md
  .github/workflows/
    rust.yml                    orf-codec and orf-codec-ffi: tests; clippy over the workspace
    core.yml                    MonokularKit on Linux (swift test) + android core (JVM tests)
    ios.yml                     macos runner: XcodeGen, build, UI snapshots, development .ipa artifact
    android.yml                 ubuntu: build, unit tests, debug APK artifact
    release.yml                 TestFlight / Play internal track / F-Droid metadata
```

Choices:

- **Native per platform** over a shared UI toolkit: Live Activities, Slide
  Over, overlays and Live Updates are each platform's own APIs.
- **The pure core is a package/module with no UI framework.** The parts most
  likely to be wrong (decryption, event decoding, Square reconciliation, link
  parsing) build and test on Linux, so most work doesn't wait on a macOS
  runner.
- **monokulo is the source of truth for its contract:** its xtask writes
  `protocol/vectors/` from the real Rust code, and both apps' tests read
  them. Changing an event shape breaks the app tests, not a till.
- **The receipt codec is shared Rust, through UniFFI.** A bit-exact format
  with a trained zstd dictionary shouldn't be written twice. ~~The rest of
  the core stays native: a shared Rust core for it isn't worth the build
  complexity for this amount of logic.~~ Superseded 2026-11-08: the rest of
  the core is shared Rust too ([decisions/core.md](decisions/core.md) 1).

---

## 10. Delivery plan

1. **Square spike** on a real account with $0 Other tenders: Other tenders in
   `ListPayments` and in `payment.created`; `RefundPayment` on them;
   `device_details` for routing; PKCE sign-in from a phone with an App Link
   redirect, and its refresh token rules; request limits with several tills
   polling every 2 s on one application. Record the responses into
   `protocol/square/`.
2. **Connected apps** in monokulo: tables, the Connected apps card, the
   connect page and code, `/apps/connect`, and app keys on the HTTP API
   (`/api/v1`: orders, the store's events, the app's own connection).
3. **Monokular Android, timer only:** connect, Square sign-in, the payments
   cursor, making orders, the receipt (`orf-codec-ffi` Kotlin bindings,
   `query::append`, the plain-code fallback), SSE, the foreground service.
   **iPad** follows the same way (Slide Over, timer only).
4. **`crates/push`** and push targets in monokulo; UnifiedPush end to end
   with ntfy for order updates.
5. **The Monokular Bell:** Square webhooks (and Clover's, in its second build), registration with the token
   check, dings over Web Push; UnifiedPush dings on Android.
6. **`push-relay`**: APNs (alert, liveactivity) and FCM, the standalone
   binary and the embedded mount; the official relay and Bell deployed.
7. **iPhone:** the Notification Service Extension handling dings, then the
   Live Activity. Android's overlay bubble and Live Updates alongside.

Android and iPad come first because they work with a timer alone: no relay,
no Bell, no Mac for Android.

Status, 2026-10-27: the server side of 4, 5 and 6 is built (monokulo's
`push` crate, push targets and deliveries, `push-relay` standalone and
embedded; the Bell), reviewed and tested; see [reports/bell.md](reports/bell.md)
and monokulo's `docs/reports/bell-and-relay.md`. The app wiring (UnifiedPush
and relay registration, Settings › Wake-ups), the deployments of the official
relay and Bell, and the Square spike (1) are open.

---

## 11. Open questions

- Square: Other tenders in the Payments API and webhooks; refunds of them;
  whether `device_details` is enough to route a sale to one till; request
  limits per application with many tills polling.
- Square PKCE: confirm the refresh token lifetime and single-use rule, that a
  mobile redirect to an App Link / universal link is accepted, and that a
  shared Keychain group lets the Notification Service Extension use and
  refresh the token safely alongside the app.
- Square POS for Android: does it hide other apps' overlays on its checkout
  screens (`HIDE_OVERLAY_WINDOWS`)? If so the bubble disappears there.
- UnifiedPush embedded FCM distributor: can it point at our relay, or does
  Android talk to the relay's FCM path directly?
- The island's code (when on) carries no receipt. Is that clear enough to
  cashiers, or should the island say "Open for receipt code"?
- iPadOS 26.1: the exact steps to put an app in Slide Over, for onboarding
  copy.
- The Bell's and the relay's hosts: `bell.monokulo.com` and
  `relay.monokulo.com` are placeholders. The Square sign-in redirect and the
  App Links are on `monokular.monokulo.com` (this repo's GitHub Pages);
  connect codes are on each store's own host.
- App Store review of a crypto-payments companion; Google Play's "display
  over other apps" declaration and its foreground service type for a shift.
