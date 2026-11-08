//! Square as a sale source (Android: `SquareSource.kt`).

use std::sync::Arc;

use async_trait::async_trait;
use monokulo_client::{Clock, PointOfSale};

use super::{Money, SquareApi, SquareConfig, SquareLocation, SquarePayment, receipt};
use crate::source::{OVERLAP_SECONDS, mark_handled};
use crate::{CursorStore, FoundSale, PosError, RefundByTill, Refunds, SaleOrder, SaleSource};

/// Square as a [`SaleSource`]: the till's location's payments since it
/// started watching, kept to Other tenders named
/// [`SquareConfig::tender_name`] (docs/design.md §1). A Point of Sale
/// "Other" tender is read as an `EXTERNAL` payment whose
/// `external_details.source` is the tender's name (decision 13).
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

#[async_trait]
impl SaleSource for SquareSource {
    fn point_of_sale(&self) -> PointOfSale {
        PointOfSale::Square
    }

    /// The location's Monero payments from two minutes before the cursor
    /// (Square can list a payment late), less those handled. The first look
    /// starts the cursor now.
    async fn new_sales(&self) -> Result<Vec<FoundSale>, PosError> {
        let since = match self.cursor.since() {
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
        let handled = self.cursor.handled();
        let payments = self.api.payments(&self.location.id, &from).await?;
        Ok(payments
            .into_iter()
            .filter(|p| self.is_monero(p) && !handled.contains(&p.id))
            .map(found_sale)
            .collect())
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
    /// idempotency key is `monokular-refund-<payment id>` (decision 21).
    async fn refund(&self, sale: &FoundSale) -> Result<(), PosError> {
        let key: String = format!("monokular-refund-{}", sale.id)
            .chars()
            .take(45)
            .collect();
        let amount = Money {
            amount: sale.amount_minor,
            currency: sale.currency.clone(),
        };
        self.api
            .refund(&sale.id, &amount, &key, "Not paid in Monero")
            .await
    }
}

fn found_sale(payment: SquarePayment) -> FoundSale {
    let money = payment.total_money.unwrap_or(payment.amount_money);
    FoundSale {
        id: payment.id,
        amount_minor: money.amount,
        currency: money.currency,
        reference: payment.receipt_number,
        order_id: payment.order_id,
        created_at: payment.created_at,
        device_id: payment.device_details.and_then(|d| d.device_id),
        title: None,
        point_of_sale: PointOfSale::Square,
    }
}
