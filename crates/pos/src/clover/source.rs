//! Clover as a sale source (Android: `CloverSource.kt`).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use monokulo_client::{Clock, PointOfSale};

use super::{CloverApi, CloverConfig, CloverMerchant, CloverPayment, CloverTender, receipt};
use crate::source::{OVERLAP_SECONDS, mark_handled};
use crate::{CursorStore, FoundSale, PosError, RefundInPos, Refunds, SaleOrder, SaleSource};

/// Where the Monero tender's id is remembered, per merchant: Clover gives
/// it when it's made. The app implements it.
pub trait TenderStore: Send + Sync {
    /// The tender's id at `merchant_id`, if one is remembered.
    fn tender_id(&self, merchant_id: &str) -> Option<String>;
    /// Remembers `tender_id` at `merchant_id`; `None` forgets it.
    fn save(&self, merchant_id: &str, tender_id: Option<&str>);
}

/// What the Monero tender is at the merchant, as the sign-in card shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TenderState {
    /// The cashier can take Monero in Clover Register with this tender.
    Ready(CloverTender),
    /// Clover didn't let the till add it (no Merchant write, risk R9): the
    /// cashier adds a payment type called Monero in Clover, then asks the
    /// till to look again. The reason is what Clover said.
    AddInClover(String),
}

/// Clover filters payments by time at most 90 days back: 89 to be safe.
const FURTHEST_BACK_SECONDS: i64 = 89 * 24 * 60 * 60;

/// Clover as a [`SaleSource`] (docs/design.md §1): the merchant's payments
/// since the till started watching, kept to the Monero tender's, that
/// succeeded and weren't voided or refunded.
///
/// A tender made over REST shares its `labelKey` with every external
/// payment, so the tender's id, which the till found or made, is what tells
/// Monero apart (decision 129 in docs/decisions/android.md). Clover has no
/// API that refunds such a payment: the cashier does, in Clover's Orders
/// app, and [`Refunds::InPos`] looks for it (decision 130).
pub struct CloverSource {
    api: CloverApi,
    config: CloverConfig,
    merchant: CloverMerchant,
    cursor: Arc<dyn CursorStore>,
    tenders: Arc<dyn TenderStore>,
    clock: Arc<dyn Clock>,
    /// Clover asked the till to slow down: no looks before this (Unix
    /// seconds).
    hold_until: Mutex<Option<i64>>,
}

impl CloverSource {
    pub fn new(
        api: CloverApi,
        config: CloverConfig,
        merchant: CloverMerchant,
        cursor: Arc<dyn CursorStore>,
        tenders: Arc<dyn TenderStore>,
        clock: Arc<dyn Clock>,
    ) -> CloverSource {
        CloverSource {
            api,
            config,
            merchant,
            cursor,
            tenders,
            clock,
            hold_until: Mutex::new(None),
        }
    }

    /// The Monero tender: the one remembered, else the merchant's tender
    /// labelled [`CloverConfig::tender_label`] (trimmed, any case), else one
    /// made now. Called after sign-in and from the card's "Look again",
    /// never on a timer: a refused `POST` is [`TenderState::AddInClover`],
    /// tried again only when the cashier asks.
    pub async fn ensure_tender(&self) -> Result<TenderState, PosError> {
        let merchant = &self.merchant.id;
        let all = self.api.tenders(merchant).await?;
        if let Some(id) = self.tenders.tender_id(merchant) {
            if let Some(tender) = all.iter().find(|t| t.id == id) {
                return Ok(TenderState::Ready(tender.clone()));
            }
            // Deleted in Clover since: find it again, or make it.
            self.tenders.save(merchant, None);
        }
        if let Some(tender) = self.labelled(&all) {
            self.tenders.save(merchant, Some(&tender.id));
            return Ok(TenderState::Ready(tender.clone()));
        }
        let made = match self
            .api
            .create_tender(merchant, &self.config.tender_label)
            .await
        {
            Ok(made) => made,
            Err(e) if e.is_refusal() => return Ok(TenderState::AddInClover(e.to_string())),
            Err(e) => return Err(e),
        };
        self.tenders.save(merchant, Some(&made.id));
        Ok(TenderState::Ready(made))
    }

    /// The merchant's tender labelled as the config says, an enabled and
    /// visible one first.
    fn labelled<'a>(&self, all: &'a [CloverTender]) -> Option<&'a CloverTender> {
        let label = self.config.tender_label.trim().to_lowercase();
        let matches: Vec<&CloverTender> = all
            .iter()
            .filter(|t| {
                t.label
                    .as_deref()
                    .is_some_and(|l| l.trim().to_lowercase() == label)
            })
            .collect();
        matches
            .iter()
            .find(|t| t.enabled && t.visible)
            .or(matches.first())
            .copied()
    }

    async fn look(&self, at: i64) -> Result<Vec<FoundSale>, PosError> {
        let merchant = &self.merchant.id;
        // Not made yet (the card asks the cashier to): found by its label,
        // never made from here.
        let tender_id = match self.tenders.tender_id(merchant) {
            Some(id) => id,
            None => match self.labelled(&self.api.tenders(merchant).await?) {
                Some(tender) => {
                    self.tenders.save(merchant, Some(&tender.id));
                    tender.id.clone()
                }
                None => return Ok(Vec::new()),
            },
        };
        let since = match self.cursor.since() {
            Some(since) => since,
            None => {
                let now = crate::time::format(crate::time::from_seconds(at));
                self.cursor.save(&now, &[]);
                now
            }
        };
        let since = crate::time::parse(&since).ok_or_else(|| {
            PosError::Unreadable(format!("The saved Clover cursor {since:?} isn't a time."))
        })?;
        let from_ms = (since.as_millisecond() - OVERLAP_SECONDS * 1000).max(
            at.saturating_sub(FURTHEST_BACK_SECONDS)
                .saturating_mul(1000),
        );
        let handled = self.cursor.handled();
        let mut payments: Vec<CloverPayment> = self
            .api
            .payments(merchant, &tender_id, from_ms)
            .await?
            .into_iter()
            .filter(|p| is_monero(p, &tender_id) && !handled.contains(&p.id))
            .collect();
        payments.sort_by_key(|p| p.created_time);
        Ok(payments.into_iter().map(|p| self.found_sale(p)).collect())
    }

    fn found_sale(&self, payment: CloverPayment) -> FoundSale {
        let (order_id, title) = match payment.order {
            Some(order) => (Some(order.id), order.title.filter(|t| !t.trim().is_empty())),
            None => (None, None),
        };
        FoundSale {
            id: payment.id,
            amount_minor: payment.amount,
            currency: self.merchant.currency.clone(),
            // Clover prints no receipt number on a payment (acceptance D7).
            reference: None,
            order_id,
            created_at: crate::time::format(crate::time::from_millis(payment.created_time)),
            device_id: payment.device.map(|d| d.id),
            title,
            point_of_sale: PointOfSale::Clover,
        }
    }
}

/// Whether `payment` is a Monero sale: it succeeded with the Monero tender,
/// and wasn't voided, nor refunded by a refund that wasn't voided.
fn is_monero(payment: &CloverPayment, tender_id: &str) -> bool {
    payment.result == "SUCCESS"
        && !payment.voided
        && payment.tender.as_ref().is_some_and(|t| t.id == tender_id)
        && payment.refunds.elements.iter().all(|r| r.voided)
}

#[async_trait]
impl SaleSource for CloverSource {
    fn point_of_sale(&self) -> PointOfSale {
        PointOfSale::Clover
    }

    /// The Monero tender's payments from two minutes before the cursor (no
    /// further back than Clover allows), less those handled, oldest first.
    /// Nothing is asked while Clover's last `429` holds the till back.
    async fn new_sales(&self) -> Result<Vec<FoundSale>, PosError> {
        let at = self.clock.now();
        if self
            .hold_until
            .lock()
            .unwrap()
            .is_some_and(|until| until > at)
        {
            return Ok(Vec::new());
        }
        let looked = self.look(at).await;
        if let Err(e) = &looked
            && e.status() == Some(429)
        {
            let wait = e.retry_after_seconds().unwrap_or(1);
            *self.hold_until.lock().unwrap() =
                Some(at.saturating_add(i64::try_from(wait).unwrap_or(i64::MAX)));
        }
        looked
    }

    /// The receipt alone: the till reads no customer from Clover's order.
    /// No receipt, and nothing asked, when the merchant's country isn't
    /// known.
    async fn order(&self, sale: &FoundSale) -> Result<SaleOrder, PosError> {
        let Some(order_id) = &sale.order_id else {
            return Ok(SaleOrder::default());
        };
        if self.merchant.country.is_none() {
            return Ok(SaleOrder::default());
        }
        let order = self.api.order(&self.merchant.id, order_id).await?;
        Ok(SaleOrder {
            receipt: receipt(&order, &self.merchant, sale),
            customer_name: None,
        })
    }

    fn refunds(&self) -> Refunds<'_> {
        Refunds::InPos(self)
    }

    fn handled(&self, sale: &FoundSale) {
        mark_handled(self.cursor.as_ref(), sale);
    }
}

#[async_trait]
impl RefundInPos for CloverSource {
    /// The payment's refunds that weren't voided reach its amount, or the
    /// payment was voided, or its order reads `REFUNDED`. A part refund
    /// isn't enough.
    async fn refunded(&self, sale: &FoundSale) -> Result<bool, PosError> {
        let payment = self.api.payment(&self.merchant.id, &sale.id).await?;
        if payment.voided || payment.result == "VOIDED" {
            return Ok(true);
        }
        let refunded: i64 = payment
            .refunds
            .elements
            .iter()
            .filter(|r| !r.voided)
            .map(|r| r.amount)
            .sum();
        if refunded >= sale.amount_minor {
            return Ok(true);
        }
        let Some(order_id) = sale
            .order_id
            .clone()
            .or_else(|| payment.order.map(|o| o.id))
        else {
            return Ok(false);
        };
        let order = self.api.order(&self.merchant.id, &order_id).await?;
        Ok(order.payment_state.as_deref() == Some("REFUNDED"))
    }
}
