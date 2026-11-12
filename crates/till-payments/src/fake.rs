//! A payment backend in memory, for tests: it makes payments on request,
//! answers refreshes with what the test last set, and sends the events
//! the test emits.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use futures_util::stream::{self, BoxStream};
use monokulo_client::SalePhase;
use tokio::sync::mpsc;

use crate::{
    BackendError, BackendEvent, BackendInfo, Payment, PaymentBackend, PaymentRef, PaymentRequest,
    Sharing,
};

type Event = Result<BackendEvent, BackendError>;

#[derive(Default)]
struct State {
    /// Every payment made or set, by its reference.
    payments: BTreeMap<PaymentRef, Payment>,
    /// The payment each request key made.
    keys: BTreeMap<String, PaymentRef>,
    /// The open event streams.
    streams: Vec<mpsc::UnboundedSender<Event>>,
    focused: Option<PaymentRef>,
    refreshes: usize,
    disconnected: bool,
    /// The next call's failure, set by the test.
    failing: Option<String>,
}

/// A payment backend in memory. Its payments are worked out at a rate of
/// 100 USD (or whatever currency) per XMR, start waiting, and run out at
/// 0; a test moves them on with [`FakeBackend::set`] and
/// [`FakeBackend::emit`].
pub struct FakeBackend {
    sharing: Sharing,
    state: Mutex<State>,
}

impl FakeBackend {
    /// A backend whose payments are shared with other tills, or only this
    /// till's.
    pub fn new(sharing: Sharing) -> FakeBackend {
        FakeBackend {
            sharing,
            state: Mutex::default(),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Replaces the payment with `payment`'s reference (or adds it): what
    /// the next refresh answers.
    pub fn set(&self, payment: Payment) {
        self.state()
            .payments
            .insert(payment.reference.clone(), payment);
    }

    /// The payment with `reference`, as it stands.
    pub fn payment(&self, reference: &PaymentRef) -> Option<Payment> {
        self.state().payments.get(reference).cloned()
    }

    /// Sends `event` down every open event stream.
    pub fn emit(&self, event: BackendEvent) {
        self.state()
            .streams
            .retain(|stream| stream.send(Ok(event.clone())).is_ok());
    }

    /// Ends every open event stream with `error`.
    pub fn fail_streams(&self, error: impl Fn() -> BackendError) {
        for stream in std::mem::take(&mut self.state().streams) {
            let _ = stream.send(Err(error()));
        }
    }

    /// The next call (describe, create, refresh, disconnect) fails, saying
    /// `said`.
    pub fn fail_next(&self, said: impl Into<String>) {
        self.state().failing = Some(said.into());
    }

    /// What the cashier was last looking at ([`PaymentBackend::focus`]).
    pub fn focused(&self) -> Option<PaymentRef> {
        self.state().focused.clone()
    }

    /// How many refreshes were asked.
    pub fn refreshes(&self) -> usize {
        self.state().refreshes
    }

    /// How many payments were made.
    pub fn made(&self) -> usize {
        self.state().keys.len()
    }

    /// Whether the till disconnected.
    pub fn disconnected(&self) -> bool {
        self.state().disconnected
    }

    fn failure(&self) -> Result<(), BackendError> {
        match self.state().failing.take() {
            Some(said) => Err(BackendError::Other(said)),
            None => Ok(()),
        }
    }
}

#[async_trait]
impl PaymentBackend for FakeBackend {
    fn sharing(&self) -> Sharing {
        self.sharing
    }

    async fn describe(&self) -> Result<BackendInfo, BackendError> {
        self.failure()?;
        Ok(BackendInfo {
            name: "A fake wallet".into(),
            address: "4Fake".into(),
            currency: Some("USD".into()),
            sharing: self.sharing,
            monokulo: None,
            warnings: Vec::new(),
        })
    }

    async fn create(&self, req: PaymentRequest) -> Result<Payment, BackendError> {
        self.failure()?;
        let mut state = self.state();
        if let Some(made) = state.keys.get(&req.key).and_then(|r| state.payments.get(r)) {
            return Ok(made.clone());
        }
        let n = state.keys.len() + 1;
        // 100 of the currency's units to the XMR: a minor unit is 10^8
        // piconero in a two-digit currency.
        let piconero = u64::try_from(req.amount.minor).unwrap_or(0) * 100_000_000;
        let address = format!("8Fake{n}");
        let payment = Payment {
            reference: PaymentRef::new(format!("fake:{n}")),
            order_id: format!("fake_{n}").into(),
            address,
            xmr_piconero: piconero,
            expires_at: till_types::UnixSeconds(0),
            phase: SalePhase::Waiting,
            confirmations: None,
            required: Some(10),
            is_final: false,
            received: None,
            overpaid: false,
            price: Some(till_types::PriceUsed {
                provider: "Fake".into(),
                rate: till_types::Price::new("100.00", req.amount.currency.to_string()),
                at: till_types::UnixSeconds(0),
            }),
            amount: Some(req.amount.decimal()),
            currency: Some(req.amount.currency.to_string()),
            receipt: req.reference,
            device_name: None,
            renew: None,
        };
        state.keys.insert(req.key, payment.reference.clone());
        state
            .payments
            .insert(payment.reference.clone(), payment.clone());
        Ok(payment)
    }

    async fn refresh(&self, refs: &[PaymentRef]) -> Result<Vec<Payment>, BackendError> {
        self.failure()?;
        let mut state = self.state();
        state.refreshes += 1;
        Ok(refs
            .iter()
            .filter_map(|r| state.payments.get(r).cloned())
            .collect())
    }

    /// Opens a stream that says it's ready at once, then carries what the
    /// test emits.
    fn events(&self) -> BoxStream<'static, Result<BackendEvent, BackendError>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let _ = tx.send(Ok(BackendEvent::Ready));
        self.state().streams.push(tx);
        Box::pin(stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|event| (event, rx))
        }))
    }

    fn focus(&self, payment: Option<&PaymentRef>) {
        self.state().focused = payment.cloned();
    }

    async fn disconnect(&self) -> Result<(), BackendError> {
        self.failure()?;
        self.state().disconnected = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use till_types::Money;

    use super::*;

    fn request(key: &str) -> PaymentRequest {
        PaymentRequest {
            key: key.into(),
            amount: Money::new(500, "USD".parse().unwrap()),
            reference: Some("bP9m".into()),
        }
    }

    #[tokio::test]
    async fn the_same_key_makes_the_same_payment() {
        let backend = FakeBackend::new(Sharing::ThisTill);
        let first = backend.create(request("sq:1")).await.unwrap();
        assert_eq!(
            first.xmr_piconero, 50_000_000_000,
            "5 USD at 100 USD the XMR"
        );
        assert_eq!(backend.create(request("sq:1")).await.unwrap(), first);
        assert_ne!(backend.create(request("sq:2")).await.unwrap(), first);
        assert_eq!(backend.made(), 2);
        assert_eq!(backend.describe().await.unwrap().sharing, Sharing::ThisTill);
    }

    #[tokio::test]
    async fn refresh_answers_what_was_set_and_events_carry_what_was_emitted() {
        let backend = FakeBackend::new(Sharing::Shared);
        let made = backend.create(request("sq:1")).await.unwrap();
        let mut events = backend.events();
        assert!(matches!(events.next().await, Some(Ok(BackendEvent::Ready))));
        let paid = Payment {
            phase: SalePhase::Paid,
            is_final: true,
            ..made.clone()
        };
        backend.set(paid.clone());
        assert_eq!(
            backend
                .refresh(&[made.reference.clone(), PaymentRef::new("fake:9")])
                .await
                .unwrap(),
            std::slice::from_ref(&paid)
        );
        backend.emit(BackendEvent::Changed(paid.clone()));
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            BackendEvent::Changed(paid)
        );
        backend.fail_next("No node answered.");
        assert_eq!(
            backend.refresh(&[]).await.unwrap_err().to_string(),
            "No node answered."
        );
        backend.focus(Some(&made.reference));
        assert_eq!(backend.focused(), Some(made.reference));
        backend.disconnect().await.unwrap();
        assert!(backend.disconnected());
    }
}
