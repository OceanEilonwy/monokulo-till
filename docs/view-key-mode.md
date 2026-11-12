# View-key mode: Till without a Monokulo server

Status: proposal, 2026-11-12. The direction decisions in §2 were taken by
the owner on 2026-11-12; everything else is open for review. Nothing here is
built yet.

Today Till needs a Monokulo server: it makes the order, picks the
subaddress, converts the price, watches the chain and tells the till what
happened. This proposal lets a shop use Till with only its own wallet:
the till holds the wallet's **private view key and public spend key**
(entered, or read from a wallet QR or seed phrase), picks a **Monero node**,
and watches for its own payments. A third mode, through a **light wallet
server** the shop runs (monokulo-lws or monero-lws), sits behind the same
interface.

## 1. Where Till depends on Monokulo today

There is no trait in front of Monokulo. `MonokuloApi`
(`crates/monokulo-client/src/api.rs`) is a concrete struct, and the tests
stand Monokulo up as a wiremock server. The calls:

| Call | Used by |
|---|---|
| `connect(&ConnectCode)` | onboarding (`engine.rs` `connect()`) |
| `store()`, `app()`, `onion_api()` | `LiveStore::read`, Settings |
| `create_order(amount, currency, merchant_order_id, idempotency_key)` | `Till::take` (`till-sales/src/till.rs`) |
| `order(url)` | `resync` on `Ready`/`Lagged`, `follow_confirmations` (every 20 s), `background_check` |
| `events()` | `Inner::follow` (`till-sales/src/runner.rs`): `Ready`, `Lagged`, `Sale(SaleEvent)` |
| `disconnect()` | Settings |

It's constructed in `Till::new`, `TillRunner::new`, `LiveStore::new` and
`background.rs`. The connection is saved under the `"connection"` secret
(Keychain or Keystore).

What Till needs from any payment backend is already source-neutral in the
ledger: `Fact::OrderCreated` holds the address, payment URI, XMR amount,
expiry and confirmations required, and `Fact::OrderObserved` holds the
phase, confirmations, `is_final`, received and overpaid
(`till-ledger/src/facts.rs`). Only `url` is Monokulo's, and it's only used to
read the order again.

Monokulo also does two jobs Till never sees: it converts the fiat amount to
XMR (Till sends `decimal_amount` + currency, gets `xmr_amount_piconero`), and
its server decides the payment's status (`derive_status` in mokulo's
`engine/src/status.rs`). Both move onto the device in view-key mode.

## 2. Decisions taken (2026-11-12)

1. **One trait over three backends**: Monokulo, the device's own wallet,
   and a light wallet server (§4).
2. **Reuse Fob's crates by depending on them**, not by copying files. Fob's
   main (0.1.19) is already split: `fob-chain`, `fob-scanner`, `fob-monero`,
   `fob-price` and `fob-codes` are libraries with no UI or storage.
3. **Where dependencies conflict, both repos move to the latest version**
   (§3.5).
4. **A new `fob-keys` crate** takes `Keys` out of `fob-vault`, so a key can be
   used without the vault's database (§3.2).
5. **A new repository, `monokulo-orf`**, takes `orf-codec` and
   `orf-codec-ffi` out of this repo, and both Fob and Till depend on it
   (§3.1).
6. **A new stateless `fob-sync` crate**, made from Fob's sync, that both apps
   use to follow the chain and the pool (§3.3).
7. **monokulo-8ar gets GPU scanning on iOS** (§3.4).
8. **A `LightBackend`** for a shop's own light wallet server (§6).
9. **Till copies Fob's price feed selection** (§7).

## 3. Cross-repo groundwork

These come first: Till's backends depend on them.

### 3.1 `monokulo-orf` (new repository)

- Move in `crates/orf-codec` and `crates/orf-codec-ffi`, plus
  `tools/receipt-qr` (an ORF tool). Keep their history (`git filter-repo
  --path …`).
- Same licence (GPL-3.0-only), same CI for its tests and its Kotlin and Swift
  bindings.
- Till depends on it by git and rev in `[workspace.dependencies]`, replacing
  today's path dependencies. `till-ffi` keeps re-exporting `orf-codec` as it
  does now.
- Fob replaces its current dependency
  (`git = "https://github.com/OceanEilonwy/monokular", rev = "3823b28…"`) with
  the new repository.
- Both apps must pin the **same rev**. Otherwise Cargo builds two copies of
  the codec, and a receipt type from one won't be accepted by the other.
- For local work on the codec, use a `[patch."https://github.com/OceanEilonwy/monokulo-orf"]`
  entry pointing at a local checkout. Don't commit it.

### 3.2 `fob-keys` (new crate in Fob)

- Takes `Keys`, `Restored`, `SeedKind` and seed parsing from
  `fob-vault/src/keys.rs`.
- Takes `view_only`, `address(index)`, `view_pair`, `primary_address`, the
  key-image and output-key helpers, and zeroizing.
- Depends on `monero-wallet`, `monero-seed`, `polyseed`, `curve25519-dalek`,
  `zeroize` and `fob-monero` only. No rusqlite, argon2 or libc.
- `fob-vault` keeps `StoredKeys`, the sealing, the database and mlock, and
  depends on `fob-keys`. Its public API is unchanged, so `fob-wallet` builds
  as before.
- Address derivation takes a full `SubaddressIndex` (account and index), not
  only an index in account 0. Till uses one account per till (§5.2).

### 3.3 `fob-sync` (new crate in Fob, stateless)

The step that follows the chain is in `fob-wallet/src/wallet/sync.rs`:

- fetching new blocks
- noticing a reorg and how far back it goes
- reading the pool
- the key images a transaction spends

Today that step is tangled with the wallet's state and database. `fob-sync`
is that step with **no state of its own**: the caller passes what it knows
and gets back what changed, then saves it however it likes.

```rust
/// Where the caller has got to: its last scanned height and the hashes of
/// the last blocks, enough to notice a reorg. The caller saves it.
pub struct Cursor { pub height: u64, pub recent: Vec<(u64, [u8; 32])> }

/// Pool transactions the caller has already read, so a step only fetches new ones.
pub struct PoolSeen(pub BTreeSet<[u8; 32]>);

pub struct Watch<'a> {
    pub scanner: &'a Scanner,      // monero-oxide, with the watched subaddresses registered
    pub prefilter: &'a Prefilter,  // fob-scanner / monokulo-8ar
    pub gpu_allowed: bool,
}

pub enum Step {
    /// New blocks read up to `cursor.height`.
    Blocks { cursor: Cursor, received: Vec<Received>, spent: Vec<SpentImage> },
    /// The chain changed under the cursor: forget everything at or above `from`.
    Reorg { from: u64, cursor: Cursor },
    /// Deeper than `recent` reaches: the caller rescans from a height it chooses.
    TooDeep { known_good: Option<u64> },
    /// Nothing new.
    AtTip,
}

pub struct PoolStep { pub received: Vec<Received>, pub spent: Vec<SpentImage>, pub gone: Vec<[u8; 32]>, pub seen: PoolSeen }

pub async fn blocks(daemon: &Daemon, transport: &Transport, watch: &Watch<'_>, cursor: &Cursor, max: usize) -> Result<Step, SyncError>;
pub async fn pool(daemon: &Daemon, watch: &Watch<'_>, seen: &PoolSeen) -> Result<PoolStep, SyncError>;
pub async fn tip(daemon: &Daemon) -> Result<(u64, [u8; 32]), SyncError>;

/// One output paying a watched subaddress: transaction hash, output index,
/// subaddress, amount, height (None in the pool), unlock time.
pub struct Received { /* … */ }
/// A key image a transaction spends, with the transaction that spends it.
pub struct SpentImage { pub image: [u8; 32], pub tx: [u8; 32], pub height: Option<u64> }
```

- Depends on `fob-chain` (fetch, transport), `fob-scanner` and
  `monero-wallet`. No storage, no clock, no tokio tasks of its own: the caller
  runs the loop and decides how often.
- Doesn't decide anything about money. It reports outputs and spent key
  images. Fob matches key images against its own outputs (its spends). Till
  uses them to notice a double spend of a payment it saw in the pool (§5.5).
- **Fob moves first.** `sync.rs`'s node step is rewritten over `fob-sync`, and
  Fob's existing sync and reorg tests must pass unchanged before Till uses it.
  That way the crate's shape is proven by one real user before the second
  arrives.
- `zmq::listen` stays in `fob-chain`. Either app can use its `Events` (`chain()`,
  `pool()`) to start a step early.

### 3.4 monokulo-8ar on iOS's GPU

8aR today is Vulkan through `ash`, with the shader at
`crates/monokulo-8ar/shaders/scan.comp`. Its README says Apple's GPUs are
meant to come later through MoltenVK. iOS has no Vulkan driver of its own.
Two ways:

- **MoltenVK**: bundle the MoltenVK framework and keep the one Vulkan code
  path. Least code. Costs: a framework in the app, SPIR-V translated to Metal
  at run time, and some Vulkan features missing (to check against what
  `scan.comp` and `gpu.rs` use).
- **A native Metal backend** (recommended): a `metal` module beside `gpu.rs`,
  through `objc2-metal`. The shader is translated from `scan.comp` to MSL
  when the crate is built (SPIRV-Cross or naga), and the generated MSL is
  checked in like `scan.spv` is. Nothing extra ships in the app, and the
  shader keeps one source.

Either way:

- The trust model stays as it is: test vectors once per process and again
  with each wallet's own key, every returned point checked, and a GPU that
  fails is retired until the process restarts.
- A one-day spike first: run `examples/gpu_check.rs` and `scan_speed.rs`
  against `prototypes/fixtures/mainnet-3797000-2000.bin` on an iPad with
  MoltenVK. If it passes the vectors, compare the speed. Pick a backend after
  that.
- **iOS doesn't allow GPU work in the background.** Metal command buffers
  submitted while the app is backgrounded fail. The prefilter must notice
  that and hand the batch to the CPU `Scanner`. This is its existing "a GPU
  that fails" path, but it must not *retire* the GPU for that reason, or the
  app returning to the foreground would stay on the CPU.
- Till's scans are small (§5.3), so the GPU matters most to Fob's catch-up.
  Till must work well on the CPU alone, and the GPU only makes it cheaper.

### 3.5 Dependency alignment

Where the two workspaces pin different versions of a crate, both move to the
latest release:

| Crate | Till | Fob | Why it matters |
|---|---|---|---|
| `rusqlite` / `libsqlite3-sys` | 0.41.1 `bundled-sqlcipher` | 0.40 `bundled-sqlcipher-vendored-openssl` | **Won't build together**: `libsqlite3-sys` links `sqlite3` and only one version may be in a build. Till won't depend on `fob-vault` after §3.2, but align anyway. |
| `reqwest` | 0.13.5 | 0.12 | Builds, but ships two HTTP stacks |
| `monero-wallet`, `monero-daemon-rpc` | — | 0.2 | Till takes Fob's version |
| `uniffi` | =0.32.2 | =0.32.2 | Already the same |

On Android, Till vendors OpenSSL for SQLCipher in `till-ffi`
(core.md decision 150), and Fob uses `vendored-openssl`. Keep one way, and
write the choice down as a decision in both repositories.

## 4. The trait

New crate **`till-payments`**: the trait, its types, `MonokuloBackend`, and
a `FakeBackend` for tests. It depends on `monokulo-client` for the types the
core shares (`SalePhase`, `PointOfSale`, `Clock`; decision 4). `till-sales`
and `till-app` depend on `till-payments`, never on a backend's internals.

```rust
#[async_trait]
pub trait PaymentBackend: Send + Sync {
    /// Who is being paid and through what: the shop's name and currency, the
    /// receiving address, and the mode ("Monokulo at shop.example",
    /// "Your wallet, through node.example", "Light wallet server at …").
    async fn describe(&self) -> Result<BackendInfo, BackendError>;

    /// A new payment. Idempotent on `req.key` (today's `OrderKey`, with its
    /// `#n` for New code): asking again with the same key returns the same payment.
    async fn create(&self, req: PaymentRequest) -> Result<Payment, BackendError>;

    /// Where these payments stand now. Monokulo: reads each order.
    /// Wallet: catches up to the tip and the pool, then answers.
    /// Light: asks the server.
    async fn refresh(&self, refs: &[PaymentRef]) -> Result<Vec<Payment>, BackendError>;

    /// Changes as they happen. The same contract as today's `events()`
    /// (core.md decision 8): the stream may end on an error and the caller
    /// reconnects with backoff; `Ready` and `Lagged` mean "call refresh".
    fn events(&self) -> BoxStream<'static, Result<BackendEvent, BackendError>>;

    /// A hint: the cashier is looking at this payment's code (None: at
    /// nothing). The wallet reads the pool more often; Monokulo ignores it.
    fn focus(&self, payment: Option<&PaymentRef>);

    async fn disconnect(&self) -> Result<(), BackendError>;
}

pub struct PaymentRequest { pub key: String, pub amount: Decimal, pub currency: String, pub reference: String }

pub struct Payment {
    pub reference: PaymentRef,  // opaque; saved in the ledger where `url` is now
    pub order_id: OrderId,
    pub address: String,
    pub xmr_piconero: u64,
    pub expires_at: Timestamp,
    pub phase: SalePhase,
    pub confirmations: u32,
    pub required: u32,
    pub is_final: bool,
    pub received: u64,
    pub overpaid: u64,
    pub price: Option<PriceUsed>, // wallet and light: provider, rate, when; Monokulo: None
}

pub enum BackendEvent { Ready, Lagged, Changed(Payment) }
```

Why this shape:

- **`events()` copies today's `StreamEvent`**, so `Inner::follow` and
  `resync` keep their logic and only change type.
- **Backends emit `Payment`s, never outputs.** `Till::created` and
  `on_order` keep writing the same facts. The ledger's merge rules
  (decisions 144, 162) don't change: whatever the backend says is read the
  way the ledger reads Monokulo now.
- **`refresh`** covers pull to refresh, `Action::Opened`, `resync` and
  `background_check`. `follow_confirmations`' 20 s read becomes a `refresh`
  call. In wallet and light modes it is nearly free, because the backend
  pushes `Changed` anyway.
- **`focus`** is a hint, not a call, so the code screen never waits on the
  network. Only pull to refresh waits.
- A shop with no point of sale already makes sales from `sale.waiting`
  events (`event_order`, `till.rs`). In wallet and light modes that path
  doesn't exist: every sale starts at the till (`create`), from the POS or
  typed in.

### 4.1 The saved link

```rust
enum Link {
    Monokulo(Connection),
    Wallet(WalletLink),  // address, view key, network, till number, node, confirmations, expiry
    Light(LightLink),    // address, view key, network, till number, server URL, pin, node for proofs
}
```

- Saved under the existing `"connection"` secret. Serde reads today's bare
  `Connection` as `Link::Monokulo`, so nobody re-onboards.
- The view key is in the Keychain or Keystore, like the app key now. On iOS
  the Keychain group is shared with the extension.
- `RunnerSide::connection()` becomes `backend() -> Option<Arc<dyn
  PaymentBackend>>`. The engine builds the backend from the `Link` when it
  opens, and after onboarding.

## 5. `WalletBackend`: the device's own wallet

New crate **`till-wallet`**, depending on `till-payments`, `fob-keys`,
`fob-sync`, `fob-chain`, `fob-scanner`, `fob-monero`, `fob-codes` and
`fob-price`.

### 5.1 Keys

Three ways in, all ending in `fob_keys::Keys::view_only(network, address, view_hex)`:

1. **Scan a view-only wallet code**: `monero_wallet:` (Feather, Cake, Fob)
   through `fob_codes::uri::parse_wallet_code` → `WalletCode::ViewOnly`.
   Recommended.
2. **Type or paste** the primary address and the private view key.
   `view_only` refuses a subaddress and a view key that doesn't match the
   address.
3. **Seed phrase** (25 words or Polyseed). The view key is derived, and the
   spend key is zeroized at once and never saved. The screen says plainly
   that a till is a shared device, often left unattended, and suggests a
   view-only code instead.

The restore height in a wallet code is ignored. Till never looks back
before its first sale (§5.3).

### 5.2 Subaddresses

- **One account per till.** Onboarding asks for the till number (1, 2, …),
  and that till uses `SubaddressIndex(account = n, index = i)`.
  - Two tills with the same keys never give out the same address.
  - Account 0 stays the merchant's own, so the till never gives out an
    address their phone wallet also gives out.
  - The merchant's wallet shows each till's takings as its own account.
- **A counter per till** in the ledger DB (§5.6), allocating `i = 1, 2, …`.
- **The wallet's lookahead.** The merchant's wallet (wallet2, Feather, Cake,
  Fob) only finds money within its subaddress lookahead, by default about
  50 accounts × 200 indexes. Wallets widen it as indexes are used, but
  payments that expire unpaid leave gaps, and a run of 200 unused indexes
  hides everything after it.
  - So the index of a payment that expired with **nothing ever seen** is
    reused after 7 days (settable).
  - Settings shows "In your wallet: account n" and the highest index used,
    for a wallet whose lookahead needs widening.
- **`OrderId` must be unique even when an index is reused**: `w:<account>:<index>:<allocation>`.
  `PaymentRef` is the same string.
- `create` saves `key → (account, index, xmr, price, expires_at,
  start_height)` before answering, so asking again with the same key
  (New code, decision 145) returns the same payment.

### 5.3 What's scanned

- Only blocks from the oldest watched payment's `start_height - 2` to the
  tip, plus the pool. No wallet history.
  - 30 minutes' expiry and 10 confirmations is about 35 blocks.
  - An expired payment is watched for late money for 24 hours more, about
    720 blocks. These are scanned once each, incrementally, never again.
- Only watched subaddresses are registered with the `Scanner` and
  `Prefilter`: the open payments and those in their late window. Both take
  indexes one at a time, so the set grows and shrinks with the shift.
- With no payments open or late, nothing is scanned. The cursor jumps to the
  tip the next time a payment is created.

### 5.4 The loop and its pace

The stream `events()` returns **is** the loop. It runs while the runner
polls it, and stops when the shift stops or the backend is dropped. Each
turn:

1. `fob_sync::pool` reads new pool transactions.
2. If `fob_sync::tip` moved, `fob_sync::blocks` catches up. A `Reorg`
   rewinds the stored outputs at or above `from`. `TooDeep` rescans from
   the oldest watched payment's `start_height`.
3. The status of every payment touched is worked out again (§5.5). A
   `Changed` is emitted only when the `Payment` actually changed.

| State | Pool read | Tip read |
|---|---|---|
| A code on screen (`focus`) | every 1–2 s | every 10 s |
| Payments open, no code on screen | every 10 s | every 20 s |
| Only late payments | — | every 60 s |
| Nothing open | — | none |
| The node's ZMQ heard | on `pool()` | on `chain()` |

ZMQ is optional. It needs the publisher on the node's own host, as in Fob,
so it's mostly for a shop running its own node.

### 5.5 Status

- Lift mokulo's `derive_status` and `covering` (`engine/src/status.rs`) into
  `till-wallet` as one pure function, with its test cases as fixtures, so
  both products agree:
  - **Partial**: some money received before expiry.
  - **Overpaid**: more than asked, once confirmed.
  - **Expired**: past expiry without the full amount, even if partly paid.
    That money is refunded by hand.
  - `covering` takes the deepest-confirmed outputs first, so a later bit of
    dust never sends a paid payment back to confirming.
- Defaults are Monokulo's: **10 confirmations** (0 allowed: paid as soon as
  it's in the pool) and **30 minutes' expiry**. Both are in Settings.
- **`is_final`**: Paid with the confirmations required, Expired once its
  late window ends, or Cancelled.
- **Double spend**: a pool transaction that paid us, whose key image then
  turns up in a different transaction in the pool or a block
  (`fob_sync::SpentImage`), is `SalePhase::DoubleSpend`. So is a pool
  transaction that leaves the pool, isn't mined, and doesn't come back
  within 10 blocks.

### 5.6 What's stored

New tables in the ledger's SQLCipher file, **not** facts:

- the cursor
- pool transactions seen
- outputs per watched subaddress
- allocations and the index counter
- the price used per payment

The facts stay "what the backend said", as with Monokulo (decision 162). A
lost wallet table can be rebuilt by rescanning from the open payments'
`start_height`.

## 6. `LightBackend`: the shop's own light wallet server

`fob-chain::light` already speaks the light wallet server API that
monokulo-lws and monero-lws use: `LightClient`, the wire types, subaddress
provisioning, and proofs that a transaction is in a block, checked against
block hashes from a node. `LightBackend` lives in `till-wallet` beside
`WalletBackend` and shares its key entry (§5.1), till number (§5.2), status
function (§5.5) and price (§7).

- **Login**: address and view key to the server, which scans. The server
  sees every payment, but the shop chooses that by running it. The screen
  says so.
- **Subaddresses**: the till tells the server the range it uses with
  `upsert_subaddrs` (account = till number, indexes `1..=counter + 50`), so
  the server watches them.
- **Status**: `get_address_txs` gives the transactions, their heights (or
  pool) and `blockchain_height`. The till turns those into outputs per
  subaddress and runs the same `derive_status`.
- **Proofs** (optional): with a node set, each paid transaction's inclusion
  proof is checked as Fob does, so a server can't invent a payment.
- **Pace**: the same table as §5.4, asking the server instead of a node.
- **Its big advantage** is that the device scans nothing, and the server
  could wake the till. A later step: the light server tells a Monokulo Bell,
  and the Bell rings the till as Monokulo does now. That gives back iOS
  alerts while Till is in the background (§9). It needs a webhook in
  monokulo-lws and a Bell registration that isn't tied to Monokulo.

## 7. Prices

Monokulo converted fiat to XMR. Now the till does, with `fob-price`:

- **Providers**: Kraken and CoinGecko through `fob_price::Source`. The
  currency is the point of sale's own (Square location or Clover merchant
  currency), or the one chosen in Settings with no POS.
- **Copy Fob's "Prices from" selection**: the provider choice limited to
  providers that price the currency (`providers_for`), the checks
  (`Check`), and the wording from `fob-words/src/prices.rs`. Port it to
  `till-screens` (the model) and to the Compose and SwiftUI settings screens.
  Fob's Android screen is in `android/.../ui/screens/Settings.kt`; Fob has no
  SwiftUI yet, so Till's is new.
- **Freshness, stricter than Fob's**:
  - The price is refreshed every 5 minutes while a shift is open.
  - A code is never made from a price older than `AGED` (10 minutes): the
    till fetches first, and if that fails it says so and makes no code.
  - The code screen shows the provider and the time of the price.
- **Recording**: `Payment.price` (provider, rate, time) goes into
  `Fact::OrderCreated`, so a receipt can show the rate used.
- **Privacy** is Fob's: a request names only the currency pair, never an
  amount or a payment, and `User-Agent` is generic.
- `fob_price::Transport::Tor` isn't built yet. See §8.

## 8. Nodes, Tor and trust

- **Choosing a node**: copy Fob's list of default nodes per network, add a
  custom URL, and run the checks with `fob_chain::node::check_all`:
  certificate, answers within 5 s, right network, up to date, and blocks
  a second through the fetcher. Self-signed certificates can be pinned, as
  in Fob.
- **Tor**: Till has Tor today (Orbot on Android; android.md decision 55). Fob
  refuses onion nodes on purpose (`node::ONION_REFUSED`), and fob-price's Tor
  transport isn't built.
  - `fob-chain` and `fob-price` get a SOCKS transport that a caller can turn
    on (`Transport::socks(proxy)`). Onion URLs are allowed only through it.
  - Fob's defaults don't change.
- **Trust**: Till doesn't check proof of work, so it trusts the node.
  - A dishonest node can show a pool payment that never happened, and with
    0 confirmations that is a sale lost.
  - Settings warns when confirmations are 0 on a node the shop doesn't run.
  - The node doesn't learn which outputs are the shop's: `get_blocks.bin`
    fetches whole blocks, and the pool is read whole. It does see the
    shop's IP address and when it asks.

## 9. Notifications and the background

- **Foreground**: the loop in `events()` drives everything. This is the
  usual case, since a counter iPad stays on the till screen.
- **Android**: a foreground service (`dataSync`, as Fob's `SyncService`)
  while a shift is open, with its notification showing the till is
  watching.
- **iOS in the background**: nothing runs.
  - With Monokulo, the Bell's push wakes the extension. In wallet mode
    nothing sends a push, so there are no alerts and no Live Activity
    updates until Till is opened again.
  - `BGAppRefreshTask` can run `refresh` now and then, but iOS decides when,
    so it's best-effort only.
  - Onboarding says so plainly ("Keep Till open on the counter to hear
    payments"). Light mode with a Bell (§6) is the way back to alerts.
- **The extension** (`background_check`) takes `Arc<dyn PaymentBackend>`
  like the app. It runs only when something pushes, so in practice only in
  Monokulo mode, and in light mode once the Bell can be rung.

## 10. Changes inside Till

- **`till-payments`** (new): §4.
- **`till-wallet`** (new): §5, §6, §7.
- **`till-sales`**:
  - `Till::new` and `TillRunner::new` take `Arc<dyn PaymentBackend>` instead
    of `MonokuloApi` + `Connection`.
  - `follow_confirmations` calls `refresh`.
  - `resync` calls `refresh` for the open sales.
- **`till-app`**:
  - `Link` (§4.1)
  - onboarding's mode choice
  - `LiveStore` reads `describe()`
  - `background.rs` builds the backend from the `Link`
  - `Disconnect` in wallet and light modes forgets the keys and deletes the
    wallet tables
- **`till-ledger`**:
  - `OrderCreated.url` keeps its name on the wire and means "the backend's
    reference".
  - `OrderCreated` gains an optional `price`.
  - `SaleKey::Order(OrderId)` takes the `w:` ids as they are.
- **`till-screens`**:
  - Settings shows the mode, node or server, till number, account, prices,
    confirmations and expiry.
  - The code screen shows the price's provider and time.
- **Tests**:
  - `FakeBackend` replaces wiremock in the till-sales and till-app tests that
    aren't about Monokulo's wire.
  - `till-wallet` is tested on recorded `get_blocks.bin` fixtures and a
    regtest monerod in CI.

### 10.1 Onboarding

A new first question, before connecting: **How will Till see payments?**

1. **Monokulo**: today's connect code, unchanged.
2. **Your own wallet**:
   1. Scan the wallet's view-only code (or type the keys, or the seed
      phrase, with the warning).
   2. Choose the till number.
   3. Choose a node and run its checks.
   4. Choose where prices come from.
   5. Confirmations and expiry, with the defaults shown.
3. **Your light wallet server**: the same, with the server's URL and its
   check in place of the node (and a node for proofs, optionally).

## 11. Order of work

| # | Where | What | Done when |
|---|---|---|---|
| 1 | new `monokulo-orf` | Move `orf-codec`, `orf-codec-ffi`, `receipt-qr` with history | Till and Fob build from it at one rev, CI green |
| 2 | Fob, Till | Dependency alignment (§3.5) | Both on the latest `rusqlite`, `reqwest`, one OpenSSL way |
| 3 | Fob | `fob-keys` | `fob-vault` depends on it, Fob's tests pass |
| 4 | Fob | `fob-sync`, and Fob's sync rewritten over it | Fob's sync and reorg tests pass unchanged |
| 5 | monokulo-8ar | iOS spike, then the Metal (or MoltenVK) backend | Vectors pass on an iPad; background falls back to the CPU without retiring the GPU |
| 6 | Till | `till-payments` + `MonokuloBackend`, threaded through | No behaviour change; every existing test passes |
| 7 | Till | `till-wallet`: `WalletBackend`, status, prices | Regtest: Waiting → Seen → Confirming → Paid; Partial, Overpaid, Expired, DoubleSpend, reorg |
| 8 | Till | Onboarding, Settings, prices and nodes screens (Android and iOS) | A sale paid end to end on stagenet on both apps |
| 9 | Till | `LightBackend` | The same tests against monokulo-lws |
| 10 | Till, lws, Bell | Light server → Bell wake-ups | iOS alert in the background in light mode |

Steps 1–5 are independent of each other and can run side by side. Step 6
needs only step 1. Step 7 needs 2, 3, 4 and 6.

## 12. Open questions

1. **Seed phrase entry**: allow it with the warning (as written), or
   view-only codes and typed keys only?
2. **Confirmations in wallet mode**: Monokulo's 10, or fewer for small
   sales? A rule by amount ("0 under 20 €, 10 above") is possible, since
   `derive_status` takes `required` per payment.
3. **Other tills' sales**: should a wallet till also scan the other tills'
   accounts, to show "Paid at Till 2"? It would know the amounts but not the
   POS receipt. This ties into the iPad feedback on "only show other tills'
   sales".
4. **How many tills**: the lookahead covers 50 accounts. Cap till numbers at
   20?
5. **Reusing indexes**: 7 days after an unpaid expiry, or never, leaving the
   merchant to widen their wallet's lookahead?
6. **Price for no-POS shops**: is the currency chosen in Settings enough, or
   should a sale also be typable in XMR?
