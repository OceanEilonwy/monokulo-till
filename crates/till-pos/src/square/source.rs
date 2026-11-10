//! Square as a sale source (Android: `SquareSource.kt`).

use std::sync::Arc;

use async_trait::async_trait;
use monokulo_client::{Clock, PointOfSale};

use super::{
    Money, SourceType, SquareApi, SquareConfig, SquareLocation, SquareOrderId, SquarePayment,
    SquarePaymentId, receipt,
};
use crate::source::{OVERLAP_SECONDS, is_handled, mark_handled};
use crate::time::Rfc3339Instant;
use crate::{
    CursorStore, FoundSale, ListedRefund, PaymentStatus, PosDeviceId, PosError, RefundByTill,
    RefundKey, Refunds, SaleOrder, SaleSource,
};

/// Square as a [`SaleSource`]: the till's location's payments since it
/// started watching, kept to Other tenders named
/// [`SquareConfig::tender_name`] (docs/design.md §1) and to cash
/// ([`FoundSale::cash`], core.md decision 116), which the till offers or
/// treats as Monero. A Point of Sale "Other" tender is read as an
/// `EXTERNAL` payment whose `external_details.source` is the tender's name
/// (decision 13).
///
/// The till refunds a sale never paid through Square's API
/// ([`Refunds::ByTill`]).
pub struct SquareSource {
    api: SquareApi,
    config: SquareConfig,
    location: SquareLocation,
    cursor: Arc<dyn CursorStore>,
    clock: Arc<dyn Clock>,
}

impl SquareSource {
    pub fn new(
        api: SquareApi,
        config: SquareConfig,
        location: SquareLocation,
        cursor: Arc<dyn CursorStore>,
        clock: Arc<dyn Clock>,
    ) -> SquareSource {
        SquareSource {
            api,
            config,
            location,
            cursor,
            clock,
        }
    }

    /// Whether `payment` is a completed Monero tender: `EXTERNAL`, its
    /// source the tender's name (trimmed, any case).
    fn is_monero(&self, payment: &SquarePayment) -> bool {
        payment.status == PaymentStatus::Completed
            && payment.source_type == Some(SourceType::External)
            && payment.external_details.as_ref().is_some_and(|e| {
                e.source.trim().to_lowercase() == self.config.tender_name.trim().to_lowercase()
            })
    }
}

/// Whether `payment` is a completed cash tender (core.md decision 116):
/// the till offers it in Monero, or treats it as Monero, as the cashier
/// chose.
fn is_cash(payment: &SquarePayment) -> bool {
    payment.status == PaymentStatus::Completed && payment.source_type == Some(SourceType::Cash)
}

#[async_trait]
impl SaleSource for SquareSource {
    fn point_of_sale(&self) -> PointOfSale {
        PointOfSale::Square
    }

    /// The location's Monero payments from two minutes before the cursor
    /// (Square can list a payment late), less those handled as they are now:
    /// one Square changed since it was handled (its `updated_at`) is given
    /// again. The first look starts the cursor now.
    async fn new_sales(&self) -> Result<Vec<FoundSale>, PosError> {
        let since = match self.cursor.since()? {
            Some(since) => since,
            None => {
                let now = Rfc3339Instant::from_second(self.clock.now().0);
                self.cursor.save(&now, &[]);
                now
            }
        };
        let from = Rfc3339Instant::from(since.timestamp()).earlier_by(OVERLAP_SECONDS);
        let handled = self.cursor.handled()?;
        let payments = self.api.payments(&self.location.id, &from).await?;
        Ok(payments
            .into_iter()
            .filter(|p| self.is_monero(p) || is_cash(p))
            .filter_map(found_sale)
            .filter(|sale| !is_handled(&handled, sale))
            .collect())
    }

    /// The location's refunds from `begin_time`, whoever made them.
    async fn refunds_since(
        &self,
        begin_time: &Rfc3339Instant,
    ) -> Result<Option<Vec<ListedRefund>>, PosError> {
        let refunds = self.api.refunds(&self.location.id, begin_time).await?;
        Ok(Some(refunds.into_iter().map(listed_refund).collect()))
    }

    /// The receipt and the order's recipient
    /// ([`SquareOrder::customer_name`](super::SquareOrder::customer_name)),
    /// from the one read.
    async fn order(&self, sale: &FoundSale) -> Result<SaleOrder, PosError> {
        let Some(order_id) = &sale.order_id else {
            return Ok(SaleOrder::default());
        };
        let order = self
            .api
            .order(&SquareOrderId::new(order_id.as_str()))
            .await?;
        Ok(SaleOrder {
            receipt: receipt(&order, &self.location, sale),
            customer_name: order.customer_name(),
        })
    }

    fn refunds(&self) -> Refunds<'_> {
        Refunds::ByTill(self)
    }

    fn handled(&self, sale: &FoundSale) {
        mark_handled(self.cursor.as_ref(), sale);
    }
}

#[async_trait]
impl RefundByTill for SquareSource {
    /// A full refund through Square's API, once whatever the retries: the
    /// idempotency key is `till-refund-<payment id>` (decision 21).
    async fn refund(&self, sale: &FoundSale) -> Result<Option<ListedRefund>, PosError> {
        let key = RefundKey::for_payment(&sale.id);
        let amount = Money::of(sale.amount);
        Ok(self
            .api
            .refund(
                &SquarePaymentId::new(sale.id.as_str()),
                &amount,
                &key,
                "Not paid in Monero",
            )
            .await?
            .map(listed_refund))
    }
}

fn listed_refund(refund: super::SquareRefund) -> ListedRefund {
    ListedRefund {
        id: refund.id.0,
        payment_id: refund.payment_id.0,
        amount_minor: refund.amount_money.amount,
        status: refund.status,
        created_at: refund.created_at,
    }
}

/// The sale `payment` is; `None` for one whose currency isn't a currency
/// code, which no order could be priced in.
fn found_sale(payment: SquarePayment) -> Option<FoundSale> {
    let cash = is_cash(&payment);
    let money = payment.total_money.unwrap_or(payment.amount_money);
    let amount = till_types::Money::new(money.amount, money.currency.parse().ok()?);
    let device = payment.device_details;
    let device_name = device
        .as_ref()
        .and_then(|d| d.device_name.as_deref())
        .map(crate::receipts::one_line)
        .filter(|name| !name.is_empty());
    Some(FoundSale {
        id: payment.id.0,
        amount,
        reference: payment.receipt_number,
        order_id: payment.order_id.map(|id| id.0),
        created_at: payment.created_at,
        updated_at: payment.updated_at,
        status: Some(payment.status),
        device_id: device.and_then(|d| d.device_id).map(PosDeviceId::from),
        device_name,
        cash,
        title: None,
        point_of_sale: PointOfSale::Square,
    })
}
