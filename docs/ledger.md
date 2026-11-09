# The ledger: Till's sales as facts

Status: accepted 2026-11-09 (revised after review the same night), for
implementation on `ipad-redesign` before it merges to main. It replaces the
till's in-memory sale list, the kept shift JSON and the alert record with one
encrypted journal of facts and projections derived from it, shared by the app
and the iOS Notification Service Extension.

## 1. Why

The till keeps its own copy of the shift's sales and patches it from several
places: the foreground timer (`Till::check`), Monokulo's event stream
(`on_event`), the resync and confirmation reads, the Bell's wake-ups, and the
iOS extension's `background_check`, which runs a second `Till` over the same
kept shift and saves its whole list over the file
(`crates/till-app/src/background.rs`, `store.rs`).

On the owner's iPad (2026-11-09) that produced:

- **A second sale shown Paid** at the moment another was paid. The extension
  saved a stale list over the app's; the app reloaded it, a paid sale went
  back to Waiting, and the resync made it Paid again with a fresh time and a
  second alert. Monokulo showed one paid order: the money was right, the
  till's state was not.
- **"From iPad"**: a sale this iPad made, shown as another till's, without its
  receipt, because the found record was lost between the extension and the
  app.
- **A missed alert and a double alert**, from the same split.

The fix is structural: no process writes a whole list; every write is one
fact, merged into one sale's row under SQLite's write lock by a pure function
that can't move a sale backwards.

## 2. Principles (the invariants the tests enforce)

1. **Square (or Clover) and Monokulo are the truth about sales.** The POS
   says what was rung up; Monokulo says what happened to the payment.
2. **Every till on the same POS account sees the same sales.** A sale's state
   is a function of shared facts only. Settings (which device this till alerts
   for, cash mode, how sales show up) never enter the state machine: they feed
   the views and the effects only.
3. **Writes are facts, merged monotonically.** A stale or duplicate fact can't
   undo a newer one. Money that arrives is never hidden: a Paid on any attempt
   always shows.
4. **One sale identity, enforced by types.** Newtypes make it a compile error
   to apply one sale's order to another.
5. **Local facts are kept apart from shared ones.** What this device did
   (alerts shown, actions, closes, settings) is recorded in the same journal,
   but as local marks layered over the shared state, never mixed into it.
6. **Effects come from state changes and are claimed in the database**, so
   the app and the extension can't both post one, and a claimed effect that
   wasn't delivered is retried.
7. **Projections are a cache of the journal.** Replaying the journal rebuilds
   them exactly. (Refetching from the sources rebuilds what the sources still
   offer: Square's recent payments and Monokulo's open orders; the journal is
   what keeps the rest.)

## 3. Identities

```rust
pub struct PaymentId(String);          // the POS payment id
pub enum SaleKey {
    /// A sale this till can see in its POS: "sq:<payment>" / "cl:<payment>".
    Pos { pos: PointOfSale, payment: PaymentId },
    /// A sale known only from Monokulo, on a till with no POS sign-in.
    /// Never merged with a Pos key (a till without a POS never sees one).
    Order(OrderId),
}
pub struct OrderKey { payment: PaymentId, attempt: u8 }  // idempotency key
pub struct OrderId(String);
pub struct DeviceId(String);
```

- `OrderKey` attempt 1 is today's key, `"sq:" + 40 hex of SHA-256(payment)`
  (`Till::idempotency_key`), so orders already made keep matching. Attempt n
  appends `#n` (New code, §7).
- The link from an order to its sale is the `order_id` that create-order
  returns, recorded in `orders(order_id → sale_key, attempt)`. Monokulo events
  and reads carry no idempotency key, so nothing else links them.
- On a till signed in to a POS, a Monokulo event for an order not (yet) in
  `orders` is kept in `pending_events` and applied when its `OrderCreated`
  arrives (any till may create the order, §7, and gets the same `order_id`).
  It never makes an order-keyed sale there.

## 4. The database

One SQLCipher database per device: `till.db` in the engine's `data_dir`
(on iOS the app group container, shared by the app and the extension; on
Android the app's files dir). WAL mode, `busy_timeout` 5 s, small
`cache_size` (the extension's memory limit is about 24 MB). Written only by
the core, through `rusqlite` with SQLCipher bundled. At open the core checks
`PRAGMA cipher_version` and refuses to run without it (a sign system SQLite
got linked).

Rows are scoped to the current Monokulo connection and POS account
(`scope` = a hash of the store id and the POS merchant); signing out,
forgetting the store or changing the POS deletes that scope's rows in one
transaction, and the app deletes every other scope's rows as it opens the
ledger, for a change made while it wasn't running.

### 4.1 The journal

```sql
CREATE TABLE journal (
  seq      INTEGER PRIMARY KEY,
  scope    TEXT NOT NULL,
  at       INTEGER NOT NULL,           -- device clock, Unix ms
  sale_key TEXT,                       -- NULL for facts not about one sale
  kind     TEXT NOT NULL,
  hash     BLOB NOT NULL,              -- SHA-256 of (kind, sale_key, body)
  body     BLOB NOT NULL               -- versioned JSON ("v")
);
CREATE UNIQUE INDEX journal_dedupe ON journal(scope, hash);
CREATE INDEX journal_sale ON journal(sale_key, seq);
```

**Only facts that change a projection are journalled.** The writer merges
first; if the merged row is unchanged, nothing is inserted. Identical content
twice is one row (the hash). So a 20-second confirmations read that says the
same thing writes nothing.

Shared facts (what the sources said):
- `PosPaymentSeen { payment, amount_minor, currency, tender: Monero|Cash,
  status, device_id, device_name, receipt_number, order_ref, created_at,
  updated_at }`
- `PosOrderRead { receipt, customer_name }` (the receipt as the codec
  encodes it, or why it couldn't be built)
- `PosRefundSeen { refund_id, amount_minor, status }` (Square: listed by
  `/v2/refunds`; Clover: not in its API, see §7)
- `OrderCreated { order_id, attempt, url, payment_uri, address, xmr,
  amount, currency, reference, expires_at, confirmations_required }`
- `OrderObserved { order_id, phase, confirmations, is_final, received }`
  (from an event or a read; the merge orders them, §5)

Local facts (what this device did):
- `AlertClaimed { kind, notification_id }`, `AlertPosted { kind }`
- `ActionAsked { action }`, `ActionDone { action, outcome }`
- `Mark { mark }`: Closed / Reopened / KeptAsCash / PaidInCash /
  RefundPending / RefundConfirmed / ClockLapsed (§5)
- `SettingChanged { name, value }`
- `ShiftMarked { Started | Stopped }`
- `Foreground { until }` (the app's heartbeat, §6)
- `Imported { from, count }` (§10)
- `Snapshot { state }` (compaction, §12)

Code that meets a `kind` or `v` it doesn't know keeps the row and skips it
(after a downgrade, say).

### 4.2 Projections

Updated in the same transaction as each journal insert, for the one sale the
fact is about. Screens and effects read only these.

```sql
CREATE TABLE sales (
  scope TEXT NOT NULL, sale_key TEXT NOT NULL,
  shared  TEXT NOT NULL,          -- serde of SharedSale (§5)
  marks   TEXT NOT NULL,          -- serde of LocalMarks (§5)
  device_id TEXT, rung_at INTEGER NOT NULL, applied_seq INTEGER NOT NULL,
  PRIMARY KEY (scope, sale_key));
CREATE INDEX sales_rung ON sales(scope, rung_at);
CREATE TABLE orders (order_id TEXT PRIMARY KEY, scope TEXT NOT NULL,
  sale_key TEXT NOT NULL, attempt INTEGER NOT NULL);
CREATE TABLE pending_events (order_id TEXT NOT NULL, seq INTEGER NOT NULL);
CREATE TABLE alerts (scope TEXT, sale_key TEXT, kind TEXT, state TEXT,
  claimed_at INTEGER, by TEXT, PRIMARY KEY (scope, sale_key, kind));
CREATE TABLE settings (name TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE cursors  (source TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE meta     (name TEXT PRIMARY KEY, value TEXT NOT NULL);
```

`alerts` is derived from `AlertClaimed`/`AlertPosted` like any projection, so
a rebuild restores it. `cursors` are hints for fetching less, never truth.

## 5. The sale model

Three independent parts, combined only when a view is built:

```rust
pub struct SharedSale {
    pub pos: PosFacts,                  // latest payment + order read
    pub attempts: Vec<Attempt>,         // Monokulo orders, attempt 1..n
    pub refund: Option<PosRefund>,      // the POS's refund, if any
}
pub struct Attempt { pub order: OrderFacts, pub payment: PaymentPhase }
pub enum PaymentPhase {
    Waiting, Seen, Confirming { n: u32 }, Partial { received: Amount },
    Paid { overpaid: bool }, Expired, Cancelled, DoubleSpend,
}
pub struct LocalMarks {
    pub closed: Option<CloseMark>, pub kept_as_cash: bool,
    pub paid_in_cash: bool, pub refund: Option<RefundMark>,
    pub clock_lapsed: bool,
}
```

**Merging a payment phase** (pure, commutative over the same observations):
- `Paid` absorbs every other phase, including Expired, Cancelled, Partial and
  DoubleSpend. Monokulo keeps watching an expired or cancelled order and may
  report a late payment (decision 174 in android.md); that must always show.
- `DoubleSpend` stays once seen, unless `Paid` arrives after it with
  `is_final` (Monokulo's own resolution).
- Otherwise the further phase wins: Waiting < Seen < Confirming(n, larger n
  wins) < Partial (larger received wins).
- `Expired` and `Cancelled` win over Waiting/Seen only with `is_final` false
  kept, so a later Seen/Paid still applies.

**Merging the POS**: a later `updated_at` wins for the payment; the order
read replaces the previous one; a refund is recorded beside the attempts,
never instead of them.

**The view** (`fn view(shared, marks, settings, this_device) -> SaleView`) is
where everything combines: a Paid on any attempt shows as Paid; a Paid on a
refunded or closed sale shows as Paid with a flag ("Paid after it was
refunded: refund the Monero" / "Paid after closing"); a Paid on two attempts
shows "Paid twice: refund one". Cash with no attempt is Offered or, with the
setting on, a sale waiting for its order; another counter's cash sale, whose
order its own till makes, is Offered as waiting for its counter, quietly. The device clock's lapse
(`ClockLapsed`) is a local mark: it can show "No payment yet" here before
Monokulo says Expired, and never changes the shared phase.

## 6. Writing and effects

```text
BEGIN IMMEDIATE
  merged = merge(old row, fact)         -- pure
  if merged == old: COMMIT (nothing written)
  INSERT INTO journal ... (deduped by hash)
  UPSERT sales/orders (applied_seq = seq); apply pending_events if this was OrderCreated
  effects = diff(old view, new view, settings, foreground, who = App|Extension)
  for each effect this process will deliver:
      INSERT INTO alerts(state='pending', claimed_at, by) ON CONFLICT DO NOTHING
      + journal AlertClaimed
COMMIT
deliver claimed effects; on success: alerts.state='posted' + AlertPosted
```

- **No network inside a transaction**, and no transaction held open across a
  suspension (§11).
- **Delivery**: a claim stuck in `pending` for 30 s (the process died) is
  re-delivered by whichever process looks next. That makes delivery
  at-least-once with the notification id = `sale_key + kind` making a repeat
  replace, not add (and `deliveredAlerts` replacing the extension's own
  notification, decision 123, stays).
- **The extension claims only what it shows**: it can present one
  notification per push, so it claims the newest qualifying new-sale alert and
  leaves the rest for the app (whose next look claims them) or the next push.
- **Foreground**: the app writes `Foreground { until: now + 10 s }` every few
  seconds while in front. `diff` reads it: in front, Paid is a haptic and the
  paid moment, not a notice; behind, it's a notice (on iPad, on iPhone without
  the Live Activity, and on Android as today).
- **Which sales alert**: sales on the device this till is set to (or every
  device when chosen); entering Waiting, or a cash sale entering its order
  with cash-as-Monero on. Other counters' sales are listed, never alerted,
  except that a sale the cashier has open gets the paid moment.
- **Learning without alerting**: the first look after install, an import, or
  a recovery (§11) records facts and claims their effects as `posted` without
  delivering, like the engine's first look today (decision 123).

## 7. Actions

| Action | Who may | Writes to | Shared fact back |
| --- | --- | --- | --- |
| Make the order (Monero tender) | any till | Monokulo create-order, attempt 1 | `OrderCreated` |
| Make the order (cash, cash-as-Monero on) | only a till set to the sale's device (or to every device) | create-order | `OrderCreated` |
| Take it in Monero (cash offered) | any till, by the cashier | create-order | `OrderCreated` |
| New code (expired, unpaid) | any till, by the cashier | create-order, attempt + 1 | `OrderCreated` |
| Refund (Square) | the till the cashier uses | Square refund, idempotent (decision 21) | `PosRefundSeen` |
| Refund (Clover) | — | done in Clover's app (decision 130) | `Mark RefundPending` → confirmed by the cashier |
| Close / Reopen / Paid in cash / Keep as cash | this till | local only | `Mark` |

- Order creation from any till is safe: the key is deterministic, Monokulo
  returns the same order for the same key and refuses a different amount
  (409). Cash orders are limited to the sale's own counter so a till with
  cash-as-Monero on doesn't make codes for a counter that only offers Monero.
- **New code** reverses android.md 43 by the owner's decision (2026-11-09).
  A Paid on any attempt ends the others (they show as superseded), and the
  view warns when two attempts are paid.
- **Closing stays local.** Monokulo doesn't let an app cancel an order (only
  the store's POS can), so other tills converge when the code expires. A
  shared close needs a Monokulo endpoint (follow-up).
- **Refunds made in the POS** are seen by listing Square refunds since the
  cursor (`/v2/refunds`), not by re-listing old payments.
- **A sale kept from Monokulo alone** (an older build's shift) is folded into
  the POS sale whose `OrderCreated` links its order: its marks, alerts and
  observations move over, its journal rows are re-keyed, and its row goes,
  in the same transaction.

## 8. Reading

- **Monokulo**: the event stream becomes `OrderObserved` facts. A resync of
  open orders runs on reconnect and every few minutes; confirmations are read
  every 20 s only while an attempt is Seen/Confirming (as today). Both write
  only when something changed.
- **Square**: one incremental payments list from the cursor minus a small
  overlap, plus the refunds list, on each look. Each payment's order is read
  once, and again when the payment's `updated_at` changes. The look cadence is
  unchanged (2 s in front, 30 s with a Bell's wake-ups, decision as today);
  the saving comes from writing only changes and reading each order once.
- **Receipts on every till** (owner's decision, 2026-11-09, superseding the
  receipt clause of core.md 113 and ipad-redesign.md item 5): every till
  signed in to the POS reads each listed payment's order and builds its
  receipt, so every till shows the same code, items and customer name. Names
  and items are stored on every till's encrypted database, within retention.
  A till without a POS sign-in shows the plain code and says why.
- **Clover**: the same flow over Clover's payments; refunds as above.
- The extension does one bounded round (Square delta, Monokulo resync of open
  orders). Each fact is its own transaction, so a round cut short leaves only
  complete facts.

## 9. The shift

A shift is a window: from `ShiftMarked(Started)` (or the start of the trading
day, 04:00 local by default, if later) until `ShiftMarked(Stopped)`. iOS gets
Stop/Start watching like Android. Sales still live (waiting, seen,
confirming, offered) always show, whatever the window; the window decides the
shift's total, its "since …" line and the cash-paid-in-Monero count, and which
ended sales are listed.

## 10. Migration

On first open, inside one `BEGIN IMMEDIATE` guarded by `meta.imported`, so
the app and the extension can't both run it: the kept shift JSON's sales
become shared facts and their marks (closed, kept as cash, paid in cash,
refund pending/confirmed) become local `Mark`s; `alerts.posted` becomes
`AlertPosted`; duplicate order ids merge; then the JSON and the old alert
record are deleted. `kotlin_json` stays for previews and fixtures (decisions
28, 79, 108).

## 11. Security at rest and iOS specifics

- **SQLCipher** (owner's choice), AES-256, key = 32 random bytes kept through
  `Platform::secret_get/put` (Keychain, shared access group,
  `AfterFirstUnlockThisDeviceOnly`; Android's Keystore-wrapped store).
- **Key states**: `Ok(Some)` opens; `Ok(None)` with no database makes a new
  key; `Ok(None)` with an existing database moves it aside and recovers (learn
  without alerting, §6); an **error** (e.g. before first unlock) fails the
  round and changes nothing.
- **0xdead10cc**: iOS kills a suspended app holding a lock on a file in a
  shared container unless it recognises an SQLite WAL database by its header,
  which SQLCipher encrypts. So: `cipher_plaintext_header_size = 32` with the
  salt kept beside the key in the Keychain; transactions are short and never
  span a suspension; connections close when the app goes to the background
  and the extension finishes; each write runs inside a background task
  assertion on iOS.
- Excluded from iCloud and Android backups.
- **Export compliance**: the app already ships non-OS cryptography
  (aws-lc-rs, core.md 90), so `ITSAppUsesNonExemptEncryption = NO` needs
  revisiting regardless of SQLCipher; recorded as an open item for the owner
  before the App Store submission, unchanged for TestFlight.

## 12. Compaction and retention

- **Compaction** (at shift start, at most hourly): a sale that is ended
  (Paid, refunded, closed, kept as cash, or expired with `is_final`) and
  older than the current shift has its journal rows replaced by one
  `Snapshot` holding its shared and local parts, in one transaction.
- **Retention**: rows older than 30 days (a setting) are deleted, with their
  projections.
- With change-only journalling, a sale writes about 5–8 facts; a 500-sale day
  is a few thousand rows, a few hundred after compaction.

## 13. Rebuilds

`meta.projection_version` is a constant in the code. When it differs, every
projection, `alerts` included, is rebuilt by replaying the journal in `seq`
order inside one transaction before anything reads them. Replay never
delivers effects.

## 14. What changes in the code

- New crate `crates/till-ledger`: database open/key/migrate/compact, facts,
  identities, `merge`, `view`, `diff`, queries. No network, no platform.
- `crates/till-sales`: `Till` becomes a driver turning POS lists, order
  reads and Monokulo events into facts; `TillState` and `ShiftStore` go.
- `crates/till-app`: the engine and `background.rs` share the ledger;
  `alerted.rs`, the kept shift and `reload_shift` go. The app re-reads
  projections when the extension commits (Darwin notification on iOS).
- `crates/till-screens` and the FFI: shapes kept; new fields for New code,
  Stop/Start watching, the "why no receipt" line, paid-after-refund/close and
  paid-twice warnings, notices on iPad.
- iOS/Android: draw those fields; iOS Stop/Start watching; background task
  assertions around ledger writes; close the ledger on going to the
  background.

## 15. Tests the implementation must pass

1. Any permutation of a set of shared facts, with duplicates, merges to the
   same `SharedSale` (property test).
2. Incremental projections equal a full replay, including after compaction.
3. The §1 case: an extension round writing older observations while the app
   has Paid can't make another sale Paid, lower a phase, or alert twice.
4. Two simulated tills fed the same shared facts in different orders have
   equal `shared` columns; their effects differ only by settings.
5. An `OrderObserved` for one order never changes another sale; one for an
   unknown order waits in `pending_events` and applies on `OrderCreated`.
6. One delivered alert per sale and kind across two connections; a claim
   whose process died is re-delivered once.
7. A late Paid on an expired, cancelled, partly paid, closed or refunded sale
   shows as Paid (flagged where needed) and alerts.
8. New code: attempt 2 has key `…#2`; a Paid on attempt 1 while attempt 2 is
   live shows Paid and ends attempt 2; two Paids warn.
9. Settings never change `shared`; a cash sale gets an order only from a till
   set to its device.
10. Migration imports sales, marks and posted alerts once, with two openers.
11. Key error fails the round without changes; `Ok(None)` with a database
    recovers without alerting.
12. Signing out or changing the store or POS deletes that scope's rows.

## 16. Decisions taken here

Recorded in `docs/decisions/core.md` as implemented:
- Facts in an encrypted journal with per-sale projections (this document).
- Receipts, items and names on every till signed in to the POS (supersedes
  the receipt clause of 113).
- New code (reverses android.md 43); a Paid on any attempt ends the others.
- Closes, paid in cash and keep as cash stay local until Monokulo lets apps
  cancel.
- Cash orders only from a till set to the sale's device.
- The trading day starts at 04:00; live sales always show.
- Retention 30 days; the database isn't backed up.
- Look cadence unchanged.
- Considered and rejected: a plain upserted table without a journal (the
  owner wants the journal for audit and replay) and column encryption instead
  of SQLCipher (the owner chose SQLCipher).
