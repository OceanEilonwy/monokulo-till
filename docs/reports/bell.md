# The Monokular Bell: report

> The app was called Monokular until 2026-11-08, when it became Till
> ("Monokulo Till" in full). This record keeps the names it was written
> with; see [decisions/core.md](../decisions/core.md) 22 for the new ones.

The project report for the push system and the Bell, written 2026-10-27
for the owner's review, is monokulo's
[`docs/reports/bell-and-relay.md`](https://github.com/OceanEilonwy/monokulo/blob/push-relay/docs/reports/bell-and-relay.md)
(on the `push-relay` branch until the stack merges): it covers both
repositories, with every decision made without the owner, the design's
deviations, the tests, the gates, the risks and what is left. This page is
the Bell's part of it.

**Moved 2026-10-29.** The Bell has since moved into the monokulo repository,
history and all, as the crate and binary `monokulo-bell`
(`crates/monokulo-bell`), with `tools/fake-square` and `tools/fake-clover`;
its secrets are `MONOKULO_BELL_*` there. This report is as it was written,
so the paths, names and commands below are monokular's at the time. The
Bell's API and how to run one are at <https://docs.monokulo.com/bell/>, its
design note and decisions in monokulo's `docs/bell.md` and
`docs/decisions/bell.md`.

## What was built

`crates/monokular-bell`, a library and the `monokular-bell` binary (branch
`monokular-bell`, draft PR [#14](https://github.com/OceanEilonwy/monokular/pull/14)):
a small service that turns a Square application's payment webhooks into Web
Push dings for the tills registered at that location. Its API is
[bell.md](https://docs.monokulo.com/bell/); how to run one is the README's "Running a Bell"; the
32 decisions behind it and the 3 from the integration are
[decisions/bell.md](https://github.com/OceanEilonwy/monokulo/blob/main/docs/decisions/bell.md).

- `POST /v1/register`: a till's Web Push subscription for one location,
  with the merchant's Square token, used once for `GET /v2/locations/{id}`
  (its `merchant_id` proves the token and the location), then dropped,
  never stored or logged, wiped from memory.
- `POST /v1/square/webhook`: Square's HMAC-SHA256 signature over
  `bell.public_uri` + `/v1/square/webhook` and the body, compared in
  constant time; only `type`, `merchant_id`, `location_id` and `status` are
  read; `200` at once, then one ding per registration at the location: an
  encrypted `{"wake":"square"}`, `Urgency: high`, `TTL: 60`, `Topic: wake`,
  VAPID-signed. No body is stored or logged (a test reads the database files
  and a trace-level log to check).
- `DELETE /v1/register/{id}`, `GET /v1/info`; registrations limited per
  address behind a trusted proxy; 32 per location; `404`/`410` and 20
  failures in a row delete a registration; dings go to public addresses only
  unless `bell.allow_private_endpoints`.
- Settings through monokulo's `live-settings` (`bell.toml` + CLI, `--init`,
  secrets `MONOKULAR_BELL_SQUARE_WEBHOOK_KEY` and `MONOKULAR_BELL_VAPID_KEY`
  from the environment only, anything invalid stops the process); its own
  SQLite file; graceful shutdown with dings in flight finished.
- It builds on monokulo's `push`, `live-settings`, `sqlite`, `rate-limit`,
  `abuse` and `service-runtime` as git dependencies pinned to one monokulo
  commit (the head of its `push-relay` branch, decision 33); after the
  monokulo push PRs merge the pin moves to `main`.
- `tools/fake-square` answers `GET /v2/locations/{id}` and sends a Bell
  signed `payment.created` webhooks, so a Bell runs end to end on one
  machine; `rust.yml` tests the Bell beside the receipt codec.

## Tests and gates

76 tests (`cargo nextest run -p monokular-bell`, about a second): 17
in-process (since the push audit, one: a proxy adding its own
`X-Forwarded-For` line still has its client counted by that line), 29 over real HTTP (named `over_http`: a completed payment's
webhook dings every till at its location and the ding decrypts to
`{"wake":"square"}` with the right headers; other events ding nothing;
Square refusing, redirecting or timing out; gone and failing subscriptions;
the token left nowhere; private endpoints refused; racing registrations
filling a location exactly; since the push audit, a payment ringing once
and a refund never, another merchant refused a till's endpoint, and the
`main` alias refused as the till's mistake, and one merchant's dings
kept to its share, leaving another merchant's), 4 of the binary (`--init`, missing secrets,
SIGTERM), 26 unit (the signature against Square's documented example,
settings, the command line, the store; the address rule's tests are
monokulo's `outbound` crate's since push audit #2).

Gates, as `rust.yml` runs them, on stable (1.98.1) against the pinned
monokulo commit `aed855ab8`, 2026-10-28: `cargo fmt -p monokular-bell
--check`, `cargo test --locked -p orf-codec -p orf-codec-ffi -p
monokular-bell`, `cargo clippy --locked --workspace --all-targets -- -D
warnings` and `cargo nextest run --locked -p monokular-bell` (76 passed
after the push audit's fixes, decisions 36 to 42), all passed.

## Risks and what is left

- `crypto-bigint` 0.7.5 (under `p256`) is yanked (RUSTSEC-2026-0387); it
  stays until 0.7.10 is two weeks old, 2026-11-09, both repositories moving
  together (decision 30).
- Square's location check and webhook shapes were tested against stand-ins
  built from Square's documentation, not against Square; the spike in
  design §10 is still open.
- The Android app isn't wired to the Bell (decision 28): Settings ›
  Wake-ups, `/v1/info` and `/v1/register` with its UnifiedPush or relay
  endpoint are the next step; `bell.md` is the contract. No deployment of
  the official Bell, no release job yet (decision 6).
- The `--init` header comes from live-settings and mentions an admin page
  the Bell doesn't have (decision 12).

## Audit fixes

The push audit (monokulo's `audit-push` branch, `scripts/audit/REPORT.md`
and `FIXES.md`; the whole stack's fix table is section 10 of
[`bell-and-relay.md`](https://github.com/OceanEilonwy/monokulo/blob/push-relay/docs/reports/bell-and-relay.md))
read the Bell at `99a0c3b`. Its findings on the Bell, and those monokulo
fixes the Bell shares, are fixed on `monokular-bell`, decisions 36 to 42;
each test was run once against the reverted fix and failed.

| # | Finding | Fix | Test | Commit |
| --- | --- | --- | --- | --- |
| 16 | Anyone who knew a till's endpoint could move its registration to their merchant, swap its keys and learn its id | An endpoint already registered moves only for its own merchant, or for a caller with its auth secret; anyone else is answered `409` without the id (37) | `another_merchant_cant_take_a_tills_registration_by_its_endpoint_over_http`, `another_merchant_moves_a_registration_only_with_its_keys` | `d0debba` |
| 3 | Every `payment.updated` of a completed payment dinged again, refunds included | A payment rings once, remembered in memory for ten minutes, and a refunded one never (38) | `a_payment_rings_once_and_a_refund_never_over_http`, `a_payment_rings_once_until_it_is_forgotten`, `at_most_so_many_payments_are_remembered_the_oldest_forgotten_first` | `7dcd133` |
| 4 | `location_id` `main` passed the check and always came back `503` | A location Square names by another id is the till's mistake, `400`, not Square unavailable (36) | `a_location_named_by_squares_main_alias_is_refused_over_http`, `a_square_answer_that_isnt_the_location_asked_for_is_refused_over_http` | `4f98d4b` |
| 8 | The registration limit read only the first `X-Forwarded-For` line | The Bell reads its client's address through monokulo's `abuse::client_address`, every line from the last (39) | `registrations_behind_a_line_adding_proxy_are_limited_by_its_line` | `5516491` |
| 2 | The Bell's private-address guard was a hand copy of monokulo's, already drifting | Dings go out through monokulo's `outbound` crate; the copy and its tests are deleted (41) | `outbound`'s own tests, in monokulo | `c778067` |
| 15 | One merchant could spend the push relay's whole budget for every merchant behind the Bell | Each merchant's tills get a share of the Bell's dings a minute (42) | `one_merchant_cant_spend_another_merchants_dings_over_http` | `1e84895` |
| 10, 18 | (monokulo) one stop token per process; no push-to-start | The Bell stops on one token made before anything waits on it; the design drops push-to-start, as the relay has (40) | monokulo's `service-runtime` test | `ddeda82` |
| 13, 22, 6, 14, 9 | (monokulo) a till's island shows only its sales, a `Topic` per target, no code after a part payment, sealed handles, a week for an unused handle | The design says what monokulo now builds | monokulo's | `c200779`, `cd0a8d8`, `99ce8cf` |

The pin is `2b200ef2a`, monokulo's `main` once the push stack merged
(monokulo #253, the same tree as `push-relay`'s head `aed855ab8`, with every
fix and its report in; decision 33). Against it, on stable, 2026-10-28:
`cargo fmt -p monokular-bell --check`, `cargo test --locked -p orf-codec -p
orf-codec-ffi -p monokular-bell`, `cargo clippy --locked --workspace
--all-targets -- -D warnings` and `cargo nextest run --locked -p
monokular-bell` (76 passed, 1.2 s), all passed.

## Second build: points of sale

Branch `bell-pos` (PR against `main`), 2026-10-28, for the owner's decision
"keep the Bell's name and make it generic for multiple POS; Clover next".
The Bell's API is version 0.2.0 ([docs.monokulo.com/bell/](https://docs.monokulo.com/bell/)); the calls made
without the owner are decisions 43 to 57.

What was built:

- **Points of sale as adapters.** `PointOfSale { Square, Clover }` is the
  wire name everywhere; `square.rs` is the Square adapter, unchanged in
  behaviour (its signature, body-reading and location-proof tests keep
  their assertions, and every Square test from `main` passes with the new
  registration body); `clover.rs` is the Clover adapter. An adapter is on
  when its `[bell.square]` or `[bell.clover]` table has an
  `application_id`, its secret then required and otherwise refused, and
  its webhook route exists only then.
- **One registration body** `{point_of_sale, access_token, location_id,
  subscription}`, answered `{registration_id, point_of_sale,
  application_id}`; `/v1/info` lists `points_of_sale`. A clean break from
  the first Bell's `square_access_token` and `square_application_id`: the
  Bell's PR and the app's say they land together, or the app's first.
- **Clover's webhook:** `X-Clover-Auth` compared in constant time with
  `MONOKULAR_BELL_CLOVER_AUTH_CODE`; the verification handshake answered
  and its code logged once at info, anything else without the header `401`
  and unlogged; `P:` `CREATE` alone dings, at its merchant (its own
  location); another app's batch dropped. A registration is proved with
  `GET /v3/merchants/{id}` and the till's token, used once.
- **Dings** `{"wake":"square"}` or `{"wake":"clover"}`; the rung-payment
  memory and each merchant's ding share keyed by point of sale and id;
  migration 0002 adds `point_of_sale` (the first Bell's rows are Square's)
  and leads the location index with it.
- **`tools/fake-clover`**, a stdlib-only stand-in Clover (PKCE sign-in,
  merchant, tenders, payments, orders, refunds, webhooks into a Bell, QA's
  switches), cherried onto the app branch.

Tests: 112 (`cargo nextest run -p monokular-bell`, about 1.3 s): 25
in-process (the info per adapter, routes off, a registration for an
adapter that is off, Clover's auth header, the handshake and its log, the
1 KiB read without the header), 39 over real HTTP (the 29 from `main`, and
for Clover: the proof and its token nowhere, every Clover answer's problem,
dings and their payload, `CREATE` only and once, a batch per merchant, no
trace of a webhook in the database or a trace-level log, no redirect
followed, Square and Clover as separate rooms and shares, a till moving
between them only with its keys), 4 of the binary (`--init` with
`[bell.clover]`, the three secrets, every on/off rule stopping it, listening
for Square alone and for Clover alone), 44 unit (the adapters, settings,
the store with a database from migration 1, the problems).

Gates on stable (1.98.1) against the same monokulo pin, 2026-10-28:
`cargo fmt -p monokular-bell --check`, `cargo clippy --locked --workspace
--all-targets -- -D warnings`, `cargo nextest run --locked -p
monokular-bell` (112 passed) and `cargo test --locked -p orf-codec -p
orf-codec-ffi -p monokular-bell`, all passed. (`cargo fmt --all --check`
fails on `main` already, in `crates/orf-codec`, which this build doesn't
touch; `rust.yml` runs no fmt step.) A one-machine run, the binary against
fake-clover: `/v1/info`, a registration with fake-clover's test token, a
refusal for the other merchant's, a Square registration refused by a
Clover-only Bell, `/v1/square/webhook` `404`, a Monero and a Cash sale each
dinging, an `O:` webhook not, the handshake's code in the log and a
`<script>` "code" refused unlogged.

Risks that are the Bell's (from the research; nothing here resolves them):

- **Clover's webhook authentication is a static header** (R5): a leaked
  Auth Code lets anyone ding tills, one request to Clover each, within the
  merchant's share; rotating it is manual in Clover's dashboard and the
  Bell reads it at start only.
- **Whether a payment by a REST-created tender emits a `P:` webhook** (R3,
  research assumption A2) is unverified: Clover documents payment events
  with no tender restriction, but nobody here has a sandbox dev kit. If it
  doesn't, the Clover adapter dings nothing and the till relies on its
  timer.
- **Every Clover payment dings**, card or Monero, as the webhook says
  neither: at a busy merchant that spends `bell.dings_per_merchant_per_minute`
  on card sales and wakes each till for one request to Clover.
- Clover's proof (`GET /v3/merchants/{id}` answering only the token's
  merchant, A6) and its shapes were tested against stand-ins built from
  Clover's documentation, not against Clover.
