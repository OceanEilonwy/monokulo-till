//! How Till is paid (docs/view-key-mode.md §4, core.md decision 238): one
//! trait, [`PaymentBackend`], in front of whatever makes and watches the
//! till's payments: a store's own Monokulo ([`MonokuloBackend`]), or the
//! device's own wallet, through a node or the shop's light wallet server
//! (till-wallet's `WalletBackend`). [`FakeBackend`] stands in for any of
//! them in tests.
//!
//! The till and the engine (till-sales, till-app) hold an
//! `Arc<dyn PaymentBackend>` and never a backend's internals. What a
//! backend says about a payment is a [`Payment`], which the till turns into
//! the same ledger facts whichever backend said it.
//!
//! Which backend a till uses is saved as its [`Link`], under the
//! `"connection"` secret it has always used.

use async_trait::async_trait;
use futures_util::stream::BoxStream;
use monokulo_client::{AppConnection, SalePhase, Store};
use till_types::{Money, PriceUsed, UnixSeconds};

mod error;
pub mod fake;
mod link;
pub mod monokulo;

pub use error::BackendError;
pub use fake::FakeBackend;
pub use link::{DEFAULT_CONFIRMATIONS, DEFAULT_EXPIRY_MINUTES, FobWallet, Link, WalletLink};
pub use monokulo::MonokuloBackend;
pub use monokulo_client::OrderId;

till_types::text_id!(
    /// Where a backend reads a payment again: opaque to the till, which
    /// keeps it in the ledger as the order's `url`. Monokulo's is the
    /// order's `self` link; empty when the backend gave none.
    PaymentRef
);

/// Makes the till's payments and says where each stands.
///
/// Every method but [`PaymentBackend::events`] and
/// [`PaymentBackend::focus`] may wait on the network. A backend is shared
/// by the till's tasks, so it is `Send + Sync`.
#[async_trait]
pub trait PaymentBackend: Send + Sync {
    /// Who is being paid and through what, and whether other tills share
    /// this till's payments.
    async fn describe(&self) -> Result<BackendInfo, BackendError>;

    /// A new payment. Idempotent on `req.key`: asking again with the same
    /// key, from this till or (with [`Sharing::Shared`]) another, returns
    /// the same payment.
    async fn create(&self, req: PaymentRequest) -> Result<Payment, BackendError>;

    /// Where these payments stand now. Monokulo reads each order again; a
    /// payment that couldn't be read is left out, and only when none could
    /// be is it an error.
    async fn refresh(&self, refs: &[PaymentRef]) -> Result<Vec<Payment>, BackendError>;

    /// A new code for a payment whose code ran out unpaid (New code,
    /// core.md decisions 167 to 172): the same payment and address with a
    /// new price and expiry. `renew` is the payment's [`Payment::renew`],
    /// `expires_at` the expiry of the code being replaced, so two tills
    /// asking at once get one renewal. A backend that gives no `renew`
    /// never has this asked of it; by default it refuses.
    async fn renew(&self, renew: &str, expires_at: UnixSeconds) -> Result<Payment, BackendError> {
        let _ = (renew, expires_at);
        Err(BackendError::Unsupported("make a new code for a sale"))
    }

    /// Changes as they happen, for as long as the stream is polled. The
    /// contract of Monokulo's event stream (core.md decision 8): nothing is
    /// asked until it is first polled; it may end, on an error or not, and
    /// the caller opens it again with a growing wait; [`BackendEvent::Ready`]
    /// and [`BackendEvent::Lagged`] mean "call refresh". An error whose
    /// [`BackendError::disconnected`] is true means stop asking.
    fn events(&self) -> BoxStream<'static, Result<BackendEvent, BackendError>>;

    /// Whether other tills share this till's payments, as
    /// [`PaymentBackend::describe`] says, without waiting: the till drops
    /// other devices' sales by it (docs/view-key-mode.md §6).
    fn sharing(&self) -> Sharing;

    /// A hint: the cashier is looking at this payment's code (`None`: at
    /// nothing), so a backend that has to look for payments itself looks
    /// more often. It never waits. Monokulo ignores it.
    fn focus(&self, payment: Option<&PaymentRef>);

    /// Ends this till's link: its key stops working at once.
    async fn disconnect(&self) -> Result<(), BackendError>;
}

/// Who a till's payments go to and through what (Settings shows it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendInfo {
    /// "Monokulo at pay.example", "Your wallet, through node.example".
    pub name: String,
    /// The wallet's primary address, whole; empty when the backend doesn't
    /// say (a Monokulo store without a wallet).
    pub address: String,
    /// The currency prices are in, when there is one.
    pub currency: Option<String>,
    pub sharing: Sharing,
    /// What a Monokulo store says of itself and of this device, which
    /// Settings shows as it always has; `None` for any other backend.
    pub monokulo: Option<MonokuloStore>,
    /// What Settings should warn of, in the backend's words: the device's
    /// own wallet's "Another till may be using till number 2."; none from
    /// Monokulo.
    pub warnings: Vec<String>,
}

/// A Monokulo store and this device as the store has it, read now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonokuloStore {
    pub store: Store,
    pub device: AppConnection,
    /// The entry on the store's onion address (`mk:onion-api`), while it
    /// has one.
    pub onion_api: Option<String>,
}

/// Whether other tills share this till's payments (docs/view-key-mode.md
/// §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sharing {
    /// Every till's payments meet in one place: a sale's payment can be
    /// made by any till, and every till hears of every payment. Monokulo.
    Shared,
    /// Each till knows only the payments it made. The device's own wallet.
    ThisTill,
}

/// What a payment is asked for: never what was bought (docs/design.md §4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaymentRequest {
    /// The idempotency key (`sq:…`, core.md decision 167): the same for
    /// every till that finds the same sale.
    pub key: String,
    /// The price, in its currency.
    pub amount: Money,
    /// The point of sale's receipt number, when it gave one: Monokulo's
    /// `merchant_order_id`.
    pub reference: Option<String>,
}

/// A payment, as a backend says it stands: what the till records as the
/// order's facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payment {
    /// Where to read it again; kept in the ledger as the order's `url`.
    pub reference: PaymentRef,
    pub order_id: OrderId,
    /// The address to pay.
    pub address: String,
    /// The XMR to pay, in piconero, for the code it has now.
    pub xmr_piconero: u64,
    /// When the code it has now runs out.
    pub expires_at: UnixSeconds,
    pub phase: SalePhase,
    /// The confirmations its payment has.
    pub confirmations: u32,
    /// The confirmations it needs to count as paid.
    pub required: u32,
    /// The backend says nothing more will happen to it.
    pub is_final: bool,
    /// Piconero received so far.
    pub received: u64,
    /// Paid more than asked.
    pub overpaid: bool,
    /// The rate its XMR was worked out at, when the till's own backend
    /// worked it out; `None` from Monokulo, which converts the price.
    pub price: Option<PriceUsed>,
    /// The price it was asked for, amount and currency as the backend
    /// wrote them, when it said.
    pub amount: Option<String>,
    pub currency: Option<String>,
    /// The receipt number it was made with ([`PaymentRequest::reference`]).
    pub receipt: Option<String>,
    /// The device that made it, as the backend names it (Monokulo: for an
    /// order one of the store's apps made).
    pub device_name: Option<String>,
    /// Handed back to [`PaymentBackend::renew`] for a new code, when the
    /// backend can make one: Monokulo's `renew-order` form.
    pub renew: Option<String>,
}

/// Something a backend's event stream said.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::large_enum_variant,
    reason = "events arrive one at a time, so boxing a payment would save nothing"
)]
pub enum BackendEvent {
    /// It's open: refresh the payments on screen.
    Ready,
    /// Some changes were missed: refresh the payments on screen.
    Lagged,
    /// A payment changed, or (with [`Sharing::Shared`]) another till made
    /// one: it as it stands now, its confirmations included.
    Changed(Payment),
}
