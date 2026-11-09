//! Square as a sale source (Android: `SquareSource.kt`).

use std::sync::Arc;

use async_trait::async_trait;
use monokulo_client::{Clock, PointOfSale};

use super::{Money, SquareApi, SquareConfig, SquareLocation, SquarePayment, receipt};
use crate::source::{OVERLAP_SECONDS, is_handled, mark_handled};
use crate::{
    CursorStore, FoundSale, ListedRefund, PosError, RefundByTill, Refunds, SaleOrder, SaleSource,
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
        payment.status == "COMPLETED"
            && payment.source_type.as_deref() == Some("EXTERNAL")
            && payment.external_details.as_ref().is_some_and(|e| {
                e.source.trim().to_lowercase() == self.config.tender_name.trim().to_lowercase()
            })
    }
}

/// Whether `payment` is a completed cash tender (core.md decision 116):
/// the till offers it in Monero, or treats it as Monero, as the cashier
/// chose.
fn is_cash(payment: &SquarePayment) -> bool {
    payment.status == "COMPLETED" && payment.source_type.as_deref() == Some("CASH")
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
                let now = crate::time::format(crate::time::from_seconds(self.clock.now()));
                self.cursor.save(&now, &[]);
                now
            }
        };
        let since = crate::time::parse(&since).ok_or_else(|| {
            PosError::Unreadable(format!("The saved Square cursor {since:?} isn't a time."))
        })?;
        let from = crate::time::format(
            since
                .checked_sub(jiff::SignedDuration::from_secs(OVERLAP_SECONDS))
                .unwrap_or(since),
        );
        let handled = self.cursor.handled()?;
        let payments = self.api.payments(&self.location.id, &from).await?;
        Ok(payments
            .into_iter()
            .filter(|p| self.is_monero(p) || is_cash(p))
            .map(found_sale)
            .filter(|sale| !is_handled(&handled, sale))
            .collect())
    }

    /// The location's refunds from `begin_time`, whoever made them.
    async fn refunds_since(&self, begin_time: &str) -> Result<Option<Vec<ListedRefund>>, PosError> {
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
        let order = self.api.order(order_id).await?;
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
        let key: String = format!("till-refund-{}", sale.id)
            .chars()
            .take(45)
            .collect();
        let amount = Money {
            amount: sale.amount_minor,
            currency: sale.currency.clone(),
        };
        Ok(self
            .api
            .refund(&sale.id, &amount, &key, "Not paid in Monero")
            .await?
            .map(listed_refund))
    }
}

fn listed_refund(refund: super::SquareRefund) -> ListedRefund {
    ListedRefund {
        id: refund.id,
        payment_id: refund.payment_id,
        amount_minor: refund.amount_money.amount,
        status: refund.status,
        created_at: refund.created_at,
    }
}

fn found_sale(payment: SquarePayment) -> FoundSale {
    let cash = is_cash(&payment);
    let money = payment.total_money.unwrap_or(payment.amount_money);
    let device = payment.device_details;
    let device_name = device
        .as_ref()
        .and_then(|d| d.device_name.as_deref())
        .map(crate::receipts::one_line)
        .filter(|name| !name.is_empty());
    FoundSale {
        id: payment.id,
        amount_minor: money.amount,
        currency: money.currency,
        reference: payment.receipt_number,
        order_id: payment.order_id,
        created_at: payment.created_at,
        updated_at: payment.updated_at,
        status: Some(payment.status),
        device_id: device.and_then(|d| d.device_id),
        device_name,
        cash,
        title: None,
        point_of_sale: PointOfSale::Square,
    }
}
