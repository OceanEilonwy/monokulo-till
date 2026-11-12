# View-key mode: Till without a Monokulo server

Status: proposal, 2026-11-12, revised the same day with the owner's
answers. The decisions in §2 are the owner's. Nothing here is built yet.

Today Till needs a Monokulo server: it makes the order, picks the
subaddress, converts the price, watches the chain and tells the till what
happened. This proposal lets a shop use Till with only its own wallet:
the till holds the wallet's **private view key and public spend key**
(entered, or read from a wallet QR or seed phrase), picks a **Monero node**
or the shop's own **light wallet server**, and watches for its own payments
**while it's open on screen**. Underneath, it uses Fob's wallet,
`fob-wallet`, as a library in Manual Sync.

The same work adds a third point of sale beside Square and Clover:
**Till**, a keypad on the till itself, for a shop with no point of sale.

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

Monokulo also does three jobs Till never sees, and all three move onto the
device:

- It converts the fiat amount to XMR. Till sends `decimal_amount` +
  currency and gets back `xmr_amount_piconero`.
- Its server decides each payment's status (`derive_status` in mokulo's
  `engine/src/status.rs`).
- It is the one place every till's orders meet. Two tills asking for the
  same sale, with the same idempotency key, get the same order
  (core.md decisions 145, 147). §6 covers what replaces this.

## 2. Decisions (owner, 2026-11-12)

1. **One trait over the backends**: Monokulo, the device's own wallet
   through a node, and the shop's own light wallet server (§4).
2. **Reuse Fob by depending on its crates**, not by copying files. Fob's
   main (0.1.19) is already split into libraries.
3. **Use `fob-wallet` as a library, in Manual Sync**, for keys, scanning,
   requests, confirmations, nodes and light servers (§5). Confirmations
   follow Fob's Settled Confirmations.
4. **Where dependencies conflict, both repos move to the latest version**
   (§3.5).
5. **A new `fob-keys` crate** takes `Keys` out of `fob-vault` (§3.2).
6. **A new repository, `monokulo-orf`**, takes the `orf-*` crates out of
   this repo, and both Fob and Till depend on it (§3.1).
7. **A new stateless `fob-sync` crate**, to be reused across apps (§3.3).
8. **monokulo-8ar gets a native Metal backend** for iOS's GPU (§3.4).
9. **A light wallet server is one of the backends** (§5.7).
10. **Till copies Fob's price feed selection** (§8).
11. **In wallet and light modes, Till refreshes and syncs only while it's
    open in the foreground** (§10).
12. **Seed phrases may be entered**, and are wiped as soon as the view key is
    taken from them (§5.2).
13. **No cap on the number of tills.**
14. **Addresses may be reused.** Each till cycles a small pool of addresses in
    its own account, and every payment's amount carries a tag, so two
    payments on one address are told apart (§5.3).
15. **Another till's sales aren't visible** in wallet and light modes (§6).
16. **"Till" is a point of sale**: a keypad on the device, with prices in
    the fiat currency unless the shop chose none (§7).
17. **"Connect to Fob"**: Fob grants Till view access in an OAuth-like
    flow. It makes a named account for the till ("Till (Corner Café)") and
    hands Till everything it needs (§5.9).

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
- **Both apps must pin the same rev.** Till now links `fob-wallet`, which uses
  `orf-codec` too. Two revs would be two crates, and a receipt from one
  wouldn't be accepted by the other.
- For local work on the codec, use a
  `[patch."https://github.com/OceanEilonwy/monokulo-orf"]` entry pointing at a
  local checkout. Don't commit it.

### 3.2 `fob-keys` (new crate in Fob)

- Takes `Keys`, `Restored`, `SeedKind` and seed parsing from
  `fob-vault/src/keys.rs`.
- Takes `view_only`, `address(index)`, `view_pair`, `primary_address`, the
  key-image and output-key helpers, and zeroizing.
- Depends on `monero-wallet`, `monero-seed`, `polyseed`, `curve25519-dalek`,
  `zeroize` and `fob-monero` only.
- `fob-vault` keeps `StoredKeys`, the sealing, the database and mlock, and
  depends on `fob-keys`.
- **Accounts other than 0.** Address derivation takes a full
  `SubaddressIndex` (account and index). Today account 0 is hard-coded in
  `fob-vault/src/keys.rs`, and in `fob-wallet`'s `addresses.rs`, `sync.rs` and
  `light.rs`. Till uses one account per till (§5.3).
- **A view-only key from a seed.** `Restored::view_only_from_words(network,
  words)` derives the view key and public spend key, and zeroizes the spend
  key and the words before it returns (§5.2).

### 3.3 `fob-sync` (new crate in Fob, stateless)

The step that follows the chain is in `fob-wallet/src/wallet/sync.rs`:

- fetching new blocks
- noticing a reorg and how far back it goes
- reading the pool
- the key images a transaction spends

Today that step is tangled with the wallet's state and database. `fob-sync`
is that step with **no state of its own**: the caller passes what it knows
and gets back what changed.

```rust
/// Where the caller has got to: its last scanned height and the hashes of
/// the last blocks, enough to notice a reorg. The caller saves it.
pub struct Cursor { pub height: u64, pub recent: Vec<(u64, [u8; 32])> }

/// Pool transactions the caller has already read.
pub struct PoolSeen(pub BTreeSet<[u8; 32]>);

pub struct Watch<'a> {
    pub scanner: &'a Scanner,      // monero-oxide, with the watched subaddresses registered
    pub prefilter: &'a Prefilter,  // fob-scanner / monokulo-8ar
    pub gpu_allowed: bool,
}

pub enum Step {
    Blocks { cursor: Cursor, received: Vec<Received>, spent: Vec<SpentImage> },
    Reorg { from: u64, cursor: Cursor },
    TooDeep { known_good: Option<u64> },
    AtTip,
}

pub struct PoolStep { pub received: Vec<Received>, pub spent: Vec<SpentImage>, pub gone: Vec<[u8; 32]>, pub seen: PoolSeen }

pub async fn blocks(daemon: &Daemon, transport: &Transport, watch: &Watch<'_>, cursor: &Cursor, max: usize) -> Result<Step, SyncError>;
pub async fn pool(daemon: &Daemon, watch: &Watch<'_>, seen: &PoolSeen) -> Result<PoolStep, SyncError>;
pub async fn tip(daemon: &Daemon) -> Result<(u64, [u8; 32]), SyncError>;
```

- Depends on `fob-chain`, `fob-scanner` and `monero-wallet`. No storage, no
  clock, no tasks of its own: the caller runs the loop.
- Decides nothing about money. It reports outputs, and the key images
  transactions spend.
- `fob-wallet`'s node sync is rewritten over it. Fob's sync, reorg and
  Manual Sync tests must pass unchanged.
- **Till reaches it through `fob-wallet`** (decision 3), not directly. It's
  there for the next app that wants to follow the chain without a whole
  wallet: monokulo-lws, or a Bell that watches a view key.

### 3.4 monokulo-8ar: a native Metal backend

8aR today is Vulkan through `ash`, with its shader in
`crates/monokulo-8ar/shaders/scan.comp` (compiled to the checked-in
`scan.spv`). iOS has no Vulkan driver, so 8aR gets a second backend:

- **A `metal` module beside `gpu.rs`**, through `objc2-metal`, behind a
  `metal` feature, built on Apple targets. `Engine` picks Metal on Apple and
  Vulkan elsewhere. Callers (`Prefilter::flag`) don't change.
- **One shader source.** `scan.comp` stays the source. `shaders/build.sh`
  also writes MSL from the SPIR-V (SPIRV-Cross, or naga), checked in as
  `scan.metal` beside `scan.spv`. CI checks that both match the source.
- **The trust model doesn't change**:
  - test vectors once per process, and again with each wallet's own key
  - every returned point checked
  - a GPU that fails is retired until the process restarts
- **The background**: iOS doesn't let an app use the GPU in the background,
  and command buffers submitted then fail. This matters to Fob more than to
  Till, since Till only scans in the foreground (§10).
  - 8aR notices this failure and hands the batch to the CPU `Scanner`, but
    doesn't *retire* the GPU for it.
  - Otherwise an app coming back to the foreground would stay on the CPU.
- **Proven on the iPad**: `examples/gpu_check.rs` and `scan_speed.rs` run on
  a device against `prototypes/fixtures/mainnet-3797000-2000.bin`, with the
  speed recorded beside Vulkan's in the README.
- Till's scans are small (§5.4). Till must work well on the CPU alone, and
  the GPU only makes it cheaper.

### 3.5 Dependency alignment

Where the two workspaces pin different versions of a crate, both move to the
latest release:

| Crate | Till | Fob | Why it matters |
|---|---|---|---|
| `rusqlite` / `libsqlite3-sys` | 0.41.1 `bundled-sqlcipher` | 0.40 `bundled-sqlcipher-vendored-openssl` | **Won't build together**: `libsqlite3-sys` links `sqlite3` and only one version may be in a build. Till now links `fob-wallet`, so this one is required. |
| `reqwest` | 0.13.5 | 0.12 | Builds, but ships two HTTP stacks |
| `monero-wallet`, `monero-daemon-rpc`, `monero-epee` | — | 0.2 | Till takes Fob's |
| `uniffi` | =0.32.2 | =0.32.2 | Already the same |

Also:

- **OpenSSL for SQLCipher.** On Android, Till vendors OpenSSL in `till-ffi`
  (core.md decision 150), and Fob uses rusqlite's `vendored-openssl`. One
  build can only have one, so pick one way and write it down as a decision
  in both repositories.
- **`proptest`** is in `fob-wallet`'s `[dependencies]`, not only its
  dev-dependencies. Move it, so Till doesn't ship it.
- **`fob-wallet`'s `usb` feature** (Trezor over libusb) stays off in Till.

## 4. The trait

New crate **`till-payments`**: the trait, its types, `MonokuloBackend`, and
a `FakeBackend` for tests. It depends on `monokulo-client` for the types the
core shares (`SalePhase`, `PointOfSale`, `Clock`; decision 4). `till-sales`
and `till-app` depend on `till-payments`, never on a backend's internals.

```rust
#[async_trait]
pub trait PaymentBackend: Send + Sync {
    /// Who is being paid and through what, and whether other tills share
    /// this till's payments (§6).
    async fn describe(&self) -> Result<BackendInfo, BackendError>;

    /// A new payment. Idempotent on `req.key`: asking again with the same
    /// key returns the same payment.
    async fn create(&self, req: PaymentRequest) -> Result<Payment, BackendError>;

    /// Where these payments stand now. Monokulo: reads each order.
    /// Wallet and light: catch up first (§5.5), then answer.
    async fn refresh(&self, refs: &[PaymentRef]) -> Result<Vec<Payment>, BackendError>;

    /// Changes as they happen, while the stream is polled. The same contract
    /// as today's `events()` (core.md decision 8): it may end on an error and
    /// the caller reconnects with backoff; `Ready` and `Lagged` mean "call refresh".
    fn events(&self) -> BoxStream<'static, Result<BackendEvent, BackendError>>;

    /// A hint: the cashier is looking at this payment's code (None: at
    /// nothing). Wallet and light look more often; Monokulo ignores it.
    fn focus(&self, payment: Option<&PaymentRef>);

    async fn disconnect(&self) -> Result<(), BackendError>;
}

pub struct BackendInfo {
    pub name: String,        // "Monokulo at shop.example", "Your wallet, through node.example"
    pub address: String,     // the wallet's primary address, shown in Settings
    pub currency: Option<String>,
    pub sharing: Sharing,    // §6
}

pub enum Sharing {
    /// Every till's payments meet in one place: a sale's payment can be made
    /// by any till, and every till hears of every payment. Monokulo.
    Shared,
    /// Each till knows only the payments it made. Wallet and light.
    ThisTill,
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
  (decisions 144, 162) don't change.
- **`refresh`** covers pull to refresh, `Action::Opened`, `resync` and
  `background_check`. `follow_confirmations`' 20 s read becomes a `refresh`
  call.
- **`focus`** is a hint, not a call, so the code screen never waits on the
  network. Only pull to refresh waits.
- **`sharing`** is how other tills' sales stay out (§6). It's in what the
  backend describes, not the till's settings, so tests with `FakeBackend`
  can cover both ways.

### 4.1 The saved link

```rust
enum Link {
    Monokulo(Connection),
    Wallet(WalletLink),  // network, till number, wallet file key; node or light server
}
```

- Saved under the existing `"connection"` secret. Serde reads today's bare
  `Connection` as `Link::Monokulo`, so nobody re-onboards.
- The keys themselves are in `fob-wallet`'s encrypted wallet file (§5.1).
  The secret holds only the key that opens that file.
- `RunnerSide::connection()` becomes `backend() -> Option<Arc<dyn
  PaymentBackend>>`. The engine builds the backend from the `Link` when it
  opens, and after onboarding.

## 5. `WalletBackend`: `fob-wallet` in Manual Sync

New crate **`till-wallet`**: a thin adapter from `PaymentBackend` to
`fob_wallet::Wallet`. It depends on `till-payments`, `fob-wallet`,
`fob-keys`, `fob-codes` and `fob-price`. A node and a light wallet server
are two settings of the same wallet (`fob_chain::light::Backend::{Node,
Light}`), so one adapter serves both modes.

### 5.1 The wallet file

- One `fob-wallet` wallet per till: `wallet.db` in the engine's `data_dir`,
  beside the ledger's `till.db`.
- Opened with `Wallet::open(path, file_key)`. The file key is random,
  made at onboarding, and kept in the Keychain or Keystore under the `Link`.
- **Manual Sync is always on** (`set_manual_sync(true)`). The wallet never
  looks for blocks by itself. Till asks it to (§5.5).
- Fob's other features (sending, contacts, profiles, price alerts, Trezor)
  are never used. Being view-only, the wallet can't send anyway.

### 5.2 Keys

Four ways in, all ending in `Wallet::restore_view_only`:

0. **Connect to Fob** (§5.9): the easiest, and the first one offered.
1. **Scan a view-only wallet code**: `monero_wallet:` (Feather, Cake, Fob),
   through `fob_codes::uri::parse_wallet_code` → `WalletCode::ViewOnly`.
2. **Type or paste** the primary address and the private view key.
   `view_only` refuses a subaddress and a view key that doesn't match the
   address.
3. **Seed phrase** (25 words or Polyseed):
   - `fob_keys::Restored::view_only_from_words` derives the view key and
     public spend key (§3.2).
   - The words and the spend key are zeroized in Rust as soon as it returns,
     and never saved.
   - On the platform side the field is a secure entry field, with no
     autocorrect, suggestions or keyboard learning. It's cleared, and its
     string dropped, as soon as the words are handed over.
   - A wallet code holding a seed (`monero_wallet:?seed=…`) is treated the
     same way.

The restore height in a code is ignored. Till never looks back before its
first sale (§5.4).

### 5.3 Subaddresses and amount tags

Two tills on the same wallet must never take each other's payments, even
if both end up handing out the same address at the same time. Three layers
handle that.

**1. One account per till keeps tills apart.**

- Every Monero subaddress is a pair `(account, index)`, computed from the
  view key and public spend key. `(0, 0)` is the primary address.
- Till n uses account n:
  - Connected through Fob (§5.9), Fob picks the next free account and names
    it.
  - Set up by hand, onboarding asks for the till number (1, 2, …; **no
    cap**).
- Account 0 stays the merchant's own. Their wallet shows each till's
  takings as its own account.
- `fob-wallet` gets a **receive account** setting (§5.8).
- Tills never talk to each other. They stay apart only because their
  accounts differ, and a till typed in by hand twice would share one. Layers
  2 and 3 make that safe and visible.

**2. A small pool of addresses per till.**

- Till n cycles `(n, 1) … (n, K)`, with K = 50 by default.
- A new payment takes the address in the pool that has been free the
  longest. An address is free once its last payment is final and its late
  window (24 hours) has passed. If none is free, it takes one more index,
  so the pool grows with the busiest hour.
- The pool lives in the wallet file, so a reinstalled or wiped till simply
  starts its pool again: reusing an address is allowed (decision 14).
- **The merchant's wallet always finds the money.** A wallet only looks
  about 200 indexes past the last used one in an account (wallet2, Feather
  and Cake), and with a pool there's never a gap that long. Only one
  warning is left in Settings: a till whose account is above 50, past the
  default account lookahead.
- **Privacy**: two customers can see the same address. Nobody else can,
  since the chain never shows the address itself.

**3. An amount tag on every payment.**

- `xmr = the converted amount, rounded + tag`, where `tag = hash(till id,
  payment key) mod 10⁶` piconero. That's under 0.000001 XMR, a fraction of
  a cent.
  - The till id is random, made at onboarding, and kept in the wallet file.
  - The payment URI asks for the exact amount, tag included.
- **Matching.** An output to a pool address counts toward a payment:
  - **exactly**, when its amount is that payment's amount, tag and all
  - **as a partial payment**, when its amount matches no payment, the
    output is smaller than the asked amount, and this till has exactly one
    open payment on that address. "New code" for the rest gets its own
    tag.
  - **never otherwise.** Anything else shows as "A payment of an unexpected
    amount: check it", never as Paid.
- **A late payment** matches its old payment's tag, not the new sale that
  took the address since.
- **Two tills sharing an account** each find only their own customers'
  money. Two payments open at once on one address share a tag one time in a
  million.
- **Detecting a shared account.** An output whose amount carries a tag this
  till never made, on one of its addresses, is the sign another till uses
  the same account. Settings warns: "Another till may be using till
  number n."
- **Wallets that round.** A wallet that rounds the URI's amount before
  sending would erase the tag. Before building, test Cake, Feather,
  Monerujo, Fob and the official wallets. If some round, put the tag in
  the digits they keep (10⁻⁶ to 10⁻⁸ XMR, a tag of 100) and accept more
  collisions; the matching rules don't change.

**`OrderId`** is `w:<account>:<index>:<tag>:<n>`, with `n` counting that
address's payments, so it stays unique when an address is reused.
`PaymentRef` is the same string.

### 5.4 What's scanned

- Manual Sync's requests, from `fob_screens::manual::ScanRequest`:
  - **`FromLast`** (last scan to the newest block) when Till comes to the
    foreground or is pulled, so blocks mined while it was closed are read
    once.
  - The **pool and newest blocks** while it's open (§5.5).
  - **`Height(h)`** for a payment made at height `h` that a scan hasn't
    reached.
- The wallet's restore height is the tip at onboarding, so there's no
  history to scan.
- A scan only finds money to registered subaddresses: the till's pool
  (§5.3), plus a margin of 10. In `fob-wallet` that's the receive account
  (§5.8).

### 5.5 When it syncs (foreground only)

The stream `events()` returns is the loop. It runs only while the runner
polls it, and the runner only polls it while Till is in the foreground
(§10). Each turn asks the wallet for a scan (§5.4), reads the requests it
watches, and emits `Changed` for each `Payment` that changed.

| State (foreground) | Pool and newest blocks | Catch up (`FromLast`) |
|---|---|---|
| Opened, or pulled to refresh | at once | at once |
| A code on screen (`focus`) | every 2 s | on a new tip |
| Payments open, no code on screen | every 10 s | on a new tip |
| Nothing open | — | every 60 s, on the tip only |

ZMQ (`set_zmq`) can be turned on for a node on the shop's own host. It
starts a turn early and lengthens the intervals, as in Fob.

### 5.6 Status and confirmations

- **Confirmations are Fob's Settled Confirmations**
  (`set_settled_confirmations`; 10 to 720, default 10;
  `fob_screens::manual::SETTLED_MIN`/`SETTLED_MAX`). A payment is Paid at
  that many. Settings shows and sets it as Fob's Manual Sync screen does.
- Lift mokulo's `derive_status` and `covering` (`engine/src/status.rs`) into
  `till-wallet` as one pure function over the outputs matched to a payment
  (§5.3, layer 3), with its test cases as fixtures, so both products
  agree:

| Phase | When |
|---|---|
| Waiting | nothing received |
| Seen | received, all still in the pool |
| Confirming | mined, fewer than Settled Confirmations |
| Paid | the full amount at Settled Confirmations |
| Overpaid | more than asked, once confirmed |
| Partial | some received before expiry |
| Expired | past expiry without the full amount, even if partly paid (refunded by hand) |
| DoubleSpend | a pool payment whose inputs were spent by another transaction, or which left the pool unmined and didn't come back within 10 blocks |

- `covering` takes the deepest-confirmed payments first, so a later bit of
  dust never sends a paid payment back to Confirming.
- **Expiry**: 30 minutes by default, in Settings.
- **`is_final`**: Paid at Settled Confirmations, Expired after its late
  window (24 hours, watched only while Till is open), or Cancelled.

### 5.7 Light wallet server

The same adapter, with the wallet's backend set to `Light { url,
self_signed }` (`fob-wallet/src/wallet/light.rs`). The server holds the view
key and scans, and Fob checks its proofs that a transaction is in a block,
against hashes from a node.

- The server sees every payment. The shop chooses that by running it, and
  the screen says so.
- The till tells the server about its account and indexes as it gives them
  out (`upsert_subaddrs`), so the server watches them.
- It syncs only in the foreground, at the same pace (§5.5), asking the
  server instead of a node.

### 5.8 What `fob-wallet` needs, to be Till's library

1. **A receive account.** `set_receive_account(n)`: requests take addresses
   in account `n`, scans register its subaddresses, and the light server is
   told about them. Fob itself stays on 0 unless it has accounts (§5.9).
2. **Requests made with a key.** `make_request` with an idempotency key
   returns the existing request for a key it has seen, so Till's New code
   (decision 145) and a retried Charge (§7) don't make a second payment. It
   also takes an expiry, kept with the request.
3. **Pooled addresses and amount tags** (§5.3):
   - a request takes the longest-free address in the account's pool, and
     grows the pool when none is free
   - its amount gets the tag
   - outputs are matched to requests by subaddress **and** amount, with the
     partial and unexpected-amount rules
   - outputs carrying a tag this wallet never made are reported, for the
     shared-account warning
   - Fob's own requests keep a fresh address each and no tag
4. **A request's incoming payments, in full.** `request_payments(id) ->
   Vec<Incoming { tx, atomic, height: Option<u64>, confirmations, double_spent }>`.
   `requests::Received` has only the total today, and `derive_status` needs
   each payment.
5. **Telling what changed.** `SyncListener` has only `on_status`. Add
   `on_requests(ids)` after a scan that touched requests, which the adapter
   turns into `Changed`.
6. **A library build.** No Trezor or USB, no price alerts, and its words
   (`fob-words`) only where Till shows them. Keep it to features rather than
   new crates.
7. **View-only from a seed** through `fob-keys` (§3.2).

Each change is Fob's own, in Fob's repository, with Fob's tests.

### 5.9 Connect to Fob

The easiest way in. The shop already has Fob, and it should feel like
"Sign in with …": Till asks, Fob opens and asks one question, and Till
comes back with everything set up.

**What the person sees**

1. In Till's onboarding: **Connect to Fob** (above "Scan a wallet code" and
   the rest).
2. Fob opens and asks which wallet, if it has more than one, and unlocks it
   as usual.
3. One consent screen:

   > **Give Till access to view payments?**
   > Till on "Corner Café iPad" will get its own account in **My wallet**:
   > **Till (Corner Café)**.
   > Till can see money this wallet receives. It can't spend anything.
   > [Give access] [Not now]

   - The name comes from Till ("Corner Café", the store or POS location
     name), and can be edited here.
   - The screen must say plainly that the **view key is wallet-wide**:
     Till can see payments to every account in this wallet, not only its
     own. The same screen offers **Make a separate wallet for Till
     instead**: a fresh wallet in Fob for the shop, so Till sees nothing
     else. That's the better choice for a personal wallet.
4. Fob returns to Till, which shows "Connected to Fob · Till (Corner Café)"
   and moves on to the point-of-sale step. The node and prices come from
   Fob too, unless changed (below).

**What Fob does**

- **Makes the account.** The next free account, labelled "Till (Corner
  Café)". Fob knows every account it has handed out, so tills connected
  through the same Fob never share an account: the shared-account case in
  §5.3 only happens with tills set up by hand.
- **Remembers the connection** under Settings → Connected apps: "Till
  (Corner Café) · account 2 · connected 12 Nov".
  - **Forget** removes the entry and the label, not the access. A view key
    can't be taken back, and the screen says so: the only real revocation
    is moving the money to a new wallet.
- **Reconnecting** (a reinstalled till, or a second phone for the same
  counter): Fob offers the existing "Till (Corner Café)" account, or a new
  one.

**What's handed over**

All in one sealed message:
- the primary address and private view key
- the network
- the account number and its label
- the restore height (Fob's tip now)
- a Fob wallet id, so Till can say "Connected to Fob" and Fob can recognise
  a reconnect
- **optionally**, Fob's node (or light server, and pinned certificate),
  and its price provider and currency. Shown as switches on the consent
  screen, on by default, so Till needs no node or price setup at all.

**The protocol** (`fob-codes/src/connect.rs`, a shared module both apps
use; versioned, `v=1`):

1. **Request.** Till makes an ephemeral X25519 key pair and a random
   `state`, then opens:
   `fob://connect?v=1&app=Till&name=Corner%20Caf%C3%A9&device=Corner%20Caf%C3%A9%20iPad&net=mainnet&pk=<x25519>&state=<16 bytes>&want=view,node,prices&cb=<callback>`
2. **Consent** in Fob, as above.
3. **Response.** Fob makes its own ephemeral key and seals the payload
   (JSON, zeroized after) with ChaCha20-Poly1305, under a key from the
   X25519 shared secret (HKDF-SHA256, with `state` as the salt). It opens
   `<callback>#v=1&state=…&epk=…&box=…`. The **fragment** keeps it out of
   any server's logs, if the callback is a web link.
4. **Till** checks `state`, opens the box, drops the ephemeral key, and
   restores the view-only wallet (§5.2). If Fob says no, it gets
   `#v=1&state=…&error=declined`.

The payload is sealed because a link can be caught by another app that
registered the same scheme, or end up in a log. Only the till that made
the request can open it.

**Per platform**

- **Android, same device**: an explicit intent to Fob's package, with
  Fob's signing certificate checked first. The answer comes back as an
  activity result, not a link. Fob checks the calling package
  (`getCallingActivity`) and shows the caller's verified app name on the
  consent screen.
- **iOS, same device**: universal links both ways (a `connect` path on
  Fob's domain, and a callback path on Till's), with the custom schemes
  only as a fallback. iOS can't tell Fob who called, which is why the
  answer is sealed. **Fob has no iOS app yet**, so on iOS this waits for
  one. Until then the iPad uses the two-device way.
- **Two devices** (Fob on a phone, Till on the counter iPad: the usual
  case):
  1. Till shows the request as a QR code.
  2. Fob scans it ("Scan" already reads codes), asks the same consent, and
     shows the sealed answer as a QR code.
  3. Till scans that.
  4. NFC can carry both ways instead, through Fob's existing NFC framing
     (`fob-codes` `nfc.rs`, `ndef.rs`).
  5. Both screens show the same **four words** derived from the request's
     key. A request QR swapped by someone nearby then shows different words.
- **Fob not installed**: "Connect to Fob" says where to get it, and the
  other three ways stay one tap away.

**What Fob needs** (in Fob's repository):

1. **Accounts.** Today Fob uses account 0 only (fob decision 19). It needs:
   - labelled accounts
   - each account's balance and activity
   - a way to move a till's takings to the main account: **Sweep to main**,
     or a send from any account
   
   Built on the receive-account work in point 1 of the list above.
2. **The connect screen and Connected apps** in Settings.
3. **`fob://connect`**: the Android intent and universal links, plus
   reading a connect request from its QR scanner and over NFC.
4. **`connect.rs`** in `fob-codes`, with test vectors both apps run.

## 6. Another till's sales: `Sharing::ThisTill`

With Monokulo, every till hears every order, and any till may make the
order for a Monero sale. Monokulo's idempotency makes two tills asking for
the same sale get one order (decisions 145, 147). Without a shared server,
two tills would each make their own payment, at two addresses, for one sale.
The customer pays one, and the other till waits forever.

So when the backend describes `Sharing::ThisTill`:

1. **A POS sale belongs to the till set to its device.** With Square or
   Clover, onboarding asks which POS device this till is. "Every device" is
   not offered, so each till has exactly one device.
2. **Sales on other devices are dropped as they're found.** `Till::check`
   ignores a `FoundSale` whose device isn't this till's, before any fact is
   written. They're never listed, never alerted and never offered.
3. **Only this till makes payments.** `may_make_order` allows a sale only
   on this till's device, Monero or cash. "Take it in Monero" for another
   counter's sale isn't offered.
4. **No sales from events.** There's no `event_order` path, since a wallet
   backend only ever describes payments this till made.
5. **Screens.** `Standing::OtherTill`, "waiting for its counter"
   (`SaleView::for_its_counter`) and the "only show other tills' sales"
   option don't appear. `till-screens` hides them when sharing is
   `ThisTill`.
6. **The ledger** is unchanged: it simply never receives another till's
   sale. Its principle 2 ("every till on the same POS account sees the same
   sales") is scoped to `Sharing::Shared` by a new decision.

Keypad sales (§7) are always this till's own.

## 7. "Till": a keypad point of sale

A new choice on the step where the shop picks its point of sale:
**Square**, **Clover** or **Till**. Till is a keypad on the device, for a
shop with no point of sale. It works with every backend: with Monokulo it
does what Monokulo's web POS does, without a browser.

**The keypad**, modelled on Monokulo's web POS (`apps/pos` in mokulo):

- Keys 1–9, C, 0 and ⌫, plus typing on a hardware keyboard: digits,
  Backspace, Escape to clear, Return to charge.
- **In fiat** (the shop's currency): digits fill from the right in the
  currency's minor units. Typing 1, 2, 5 shows 1.25 €. Zero-decimal
  currencies such as yen get whole units.
- **With no fiat currency** (XMR only): a decimal point key, up to 12
  decimals, amounts in XMR, and no price lookups at all.
- An optional **Reference** ("customer name or note"). It becomes the
  sale's reference and the payment's `merchant_order_id`.
- **Charge** is disabled at zero. It makes the sale and its payment, and
  shows the code.
- A Charge retried with the same amount and reference reuses its
  idempotency key, so a failed request tried again never makes a second
  payment.
- Big keys, sized for a counter iPad and a phone held in one hand, following
  `docs/design.md`'s controls.

**The sale**:

- `PointOfSale::Till`, with wire name `"till"`. The Bell never sees it,
  since there's no POS webhook to ring.
- A keypad sale is a `SaleSource` of its own, `till-pos/src/keypad.rs`. Charge
  records a found sale (amount, currency, reference, key) in the ledger, and
  `Till::take` makes its payment as for a Square sale. The sale list, code
  screen, Close, New code and alerts all work as they do now.
- **Receipt**: a one-line ORF receipt (the amount, and the reference as its
  note), appended to the payment URI as now.
- **Refunds**: there's no POS to refund in. An overpaid or expired sale
  shows the amount to return by hand, as Monokulo's web POS does.
- **Background order**: the code can be put aside to serve the next
  customer, and stays on the sale list as a live sale.

**The currency**:

- With Monokulo, the store's own base currency.
- In wallet and light modes, chosen at onboarding: one of `fob-price`'s
  currencies, or **None (XMR only)**.

## 8. Prices

In wallet and light modes the till converts fiat to XMR, with `fob-price`:

- **Providers**: Kraken and CoinGecko, through `fob_price::Source`. The
  currency is the point of sale's own (Square location, Clover merchant), or
  the keypad's (§7). With XMR only, no price is ever asked for.
- **Copy Fob's price feed selection**: the provider choice limited to
  providers that price the currency (`providers_for`), the checks (`Check`),
  and the wording from `fob-words/src/prices.rs`.
  - Port it to `till-screens` (the model) and to the Compose and SwiftUI
    settings screens.
  - Fob's Android screen is in `android/.../ui/screens/Settings.kt`. Fob has
    no SwiftUI yet, so Till's is new.
- **Freshness, stricter than Fob's**:
  - The price is refreshed every 5 minutes while Till is in the foreground,
    and on pull to refresh.
  - A code is never made from a price older than `AGED` (10 minutes): Till
    fetches first, and if that fails it says so and makes no code.
  - The code screen shows the provider and the time of the price.
- **Recording**: `Payment.price` (provider, rate, time) goes into
  `Fact::OrderCreated`, so a receipt can show the rate used.
- **Privacy** is Fob's: a request names only the currency pair, never an
  amount or a payment.

## 9. Nodes, Tor and trust

- **Choosing a node**: through `fob-wallet`'s connection calls (`set_node`,
  `check_node`, `check_nodes`, `add_node`): Fob's default nodes per network,
  a custom URL, and Fob's checks:
  - certificate
  - answers within 5 s
  - right network
  - up to date
  - blocks a second
  
  Self-signed certificates can be pinned.
- **Tor**: Till has Tor today (Orbot on Android; android.md decision 55). Fob
  refuses onion nodes on purpose (`node::ONION_REFUSED`), and fob-price's Tor
  transport isn't built.
  - `fob-chain` and `fob-price` get a SOCKS transport a caller can turn on.
  - Onion URLs are allowed only through it.
  - Fob's defaults don't change.
- **Trust**: Till doesn't check proof of work, so it trusts the node.
  - A dishonest node could answer with blocks it made up, confirmations
    and all, since Till checks no proof of work. So the node must be one
    the shop trusts. Settings suggests running one, and the light server
    mode checks proofs against a second node (§5.7).
  - The node doesn't learn which outputs are the shop's: whole blocks and
    the whole pool are read. It does see the shop's IP address and when it
    asks.

## 10. Foreground only

In wallet and light modes, Till looks for payments only while it's open in
the foreground:

- **The runner stops the backend's stream when the app leaves the
  foreground**, and starts it again when the app comes back. The platforms
  already pass the process's life to the core, and the core decides
  (decision 151).
- Coming back starts with `FromLast` and the pool, so nothing mined
  meanwhile is missed. Expiry is worked out from the clock, so a code that
  ran out while Till was closed shows as Expired when it opens. A payment
  found then is judged by `derive_status` like any other, and shows as a
  late payment (open question 2).
- **No background work** in these modes:
  - no Bell registration
  - no Notification Service Extension work (`background_check` isn't called)
  - no `BGAppRefreshTask`
  - no Android foreground service
- So there are no payment alerts or Live Activity updates while Till is
  closed. Onboarding says so plainly: "Till hears payments while it's
  open. Keep it on screen at the counter."
- **Monokulo mode is unchanged**: its Bell wake-ups and the extension keep
  working.

## 11. Changes inside Till

- **`till-payments`** (new): §4.
- **`till-wallet`** (new): §5, the status function, prices (§8).
- **`till-pos`**: `keypad.rs`, the keypad's `SaleSource` (§7).
- **`monokulo-client`**: `PointOfSale::Till`.
- **`till-sales`**:
  - `Till::new` and `TillRunner::new` take `Arc<dyn PaymentBackend>` instead
    of `MonokuloApi` + `Connection`.
  - `follow_confirmations` and `resync` call `refresh`.
  - `Sharing::ThisTill` filtering in `Till::check` and `may_make_order` (§6).
  - The runner stops and starts the stream with the foreground (§10).
- **`till-app`**:
  - `Link` (§4.1)
  - onboarding (§11.1)
  - `LiveStore` reads `describe()`
  - in wallet mode: no Bell and no background check
  - `Disconnect` in wallet mode deletes the wallet file and forgets its key
- **`till-ledger`**:
  - `OrderCreated.url` keeps its name on the wire and means "the backend's
    reference".
  - `OrderCreated` gains an optional `price`.
  - Keypad sales get `SaleKey::Till(<key>)`, beside `sq:` and `cl:`.
- **`till-screens`**:
  - the keypad
  - Settings: the mode, node or server, till number and account, lookahead
    warning, Settled Confirmations, expiry, prices
  - the code screen shows the price's provider and time
  - other-till views hidden with `ThisTill`
- **Tests**:
  - `FakeBackend` replaces wiremock in the till-sales and till-app tests that
    aren't about Monokulo's wire.
  - `till-wallet` is tested against a regtest monerod in CI, and against
    monokulo-lws for the light server.

### 11.1 Onboarding

1. **How will Till see payments?** Monokulo, Your own wallet, or Your light
   wallet server.
2. **Monokulo**: today's connect code, unchanged.
   **Your own wallet** or **light wallet server**:
   1. The wallet: **Connect to Fob** (§5.9), or scan its view-only code,
      type the address and view key, or enter the seed phrase (wiped
      after, §5.2).
   2. The till number. Skipped when connected through Fob, which chose the
      account.
   3. The node, with its checks. For a light server: its URL and check,
      and a node for proofs. Skipped when Fob handed over its own.
   4. Settled Confirmations and expiry, with the defaults shown.
3. **Point of sale**: Square, Clover or Till.
   - Square or Clover: sign in, then **this till's device**. In wallet
     modes there's no "every device" (§6).
   - Till: the currency, or None (XMR only).
4. **Prices** (wallet modes with a fiat currency): the provider, with Fob's
   checks. Skipped when Fob handed over its own.
5. **Foreground only** (wallet modes): the one screen that says Till hears
   payments while open.

## 12. Order of work

| # | Where | What | Done when |
|---|---|---|---|
| 1 | new `monokulo-orf` | Move `orf-codec`, `orf-codec-ffi`, `receipt-qr` with history | Till and Fob build from it at one rev, CI green |
| 2 | Fob, Till | Dependency alignment (§3.5) | Both on the latest `rusqlite` and `reqwest`, one OpenSSL way, `proptest` dev-only |
| 3 | Fob | `fob-keys` (accounts, view-only from words) | `fob-vault` depends on it, Fob's tests pass |
| 4 | Fob | `fob-sync`, and `fob-wallet`'s node sync over it | Fob's sync, reorg and Manual Sync tests pass unchanged |
| 5 | Fob | `fob-wallet` as a library (§5.8) | Receive account, keyed requests with expiry, address pool and amount tags, `request_payments`, `on_requests`, a build without USB |
| 5a | Fob | Accounts, Connected apps, `fob://connect`, `connect.rs` (§5.9) | Labelled accounts with balances and Sweep to main; the connect protocol's vectors pass in both repos; a QR connect on Android |
| 5b | Till, wallets | Amount tag survey | The tag's digits picked from what Cake, Feather, Monerujo, Fob and the official wallets actually send |
| 6 | monokulo-8ar | Native Metal backend | Vectors pass on an iPad; MSL checked against `scan.comp` in CI; the background falls back to the CPU without retiring the GPU |
| 7 | Till | `till-payments` + `MonokuloBackend`, threaded through | No behaviour change; every existing test passes |
| 8 | Till | Keypad point of sale (§7) | Charge → code → paid against Monokulo, on Android and iOS |
| 9 | Till | `till-wallet` over `fob-wallet`, `Sharing::ThisTill`, foreground only | Regtest: Waiting → Seen → Confirming → Paid; Partial, Overpaid, Expired, DoubleSpend, reorg; a backgrounded till catches up on return |
| 10 | Till | Onboarding (with Connect to Fob), Settings, prices and nodes screens | A keypad sale and a Square sale paid end to end on stagenet, on both apps; an iPad connected from Fob on a phone by QR |
| 11 | Till | The light wallet server setting | The same tests against monokulo-lws |

Steps 1, 3, 5b and 6 can start at once. Step 5a needs 5. Step 7 needs only
step 1, and step 8 only step 7. Step 9 needs 2, 3, 5, 5b and 7. Step 10
needs 5a for Connect to Fob. Step 4 runs beside the others:
Till doesn't wait for it, since it reaches `fob-sync` through `fob-wallet`.

## 13. Open questions

1. **Connect to Fob's default**: a named account in the existing wallet
   (as written), or a separate wallet for Till? The separate wallet keeps
   the shop's other money private from the till, and costs a second wallet
   to look after.
2. **Expiry and late payments while closed**: a payment that arrives after
   expiry, while Till was closed, is found on the next open and shown as a
   late payment. Is that the right standing, or should it be flagged for a
   refund straight away?
3. **One wallet, two modes**: should a shop be able to move between node
   and light server without onboarding again? `fob-wallet` allows it
   (`light_server` / `set_node`).
4. **Keypad and Monokulo**: should the keypad's sales in Monokulo mode also
   show in Monokulo's own web POS list, or stay the till's? They're
   ordinary orders either way.
