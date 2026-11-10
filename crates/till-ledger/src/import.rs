//! Migration (docs/ledger.md §10): the kept shift JSON (the Kotlin
//! `TillSale` list `till_sales::shift_json` reads and writes) as facts.

use std::collections::BTreeMap;

use monokulo_client::SalePhase;
use monokulo_client::payment_uri::PaymentUri;
use serde::Deserialize;
use till_pos::FoundSale;

use crate::LedgerError;
use crate::facts::{Fact, MarkKind, OrderCreated, OrderObserved, OrderRead, PaymentSeen, Tender};
use crate::ids::{DeviceId, OrderId, SaleKey};

/// One kept sale's facts, each with the time it's journalled at (Unix ms;
/// `None` for now).
pub(crate) struct ImportedSale {
    pub facts: Vec<(Fact, Option<i64>)>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeptCode {
    uri: PaymentUri,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Kept {
    order_id: String,
    amount: String,
    currency: String,
    #[serde(default)]
    reference: Option<String>,
    code: KeptCode,
    phase: SalePhase,
    expires_at: i64,
    xmr: String,
    address: String,
    #[serde(default)]
    found: Option<FoundSale>,
    #[serde(default)]
    receipt: Option<serde_json::Value>,
    #[serde(default)]
    customer_name: Option<String>,
    #[serde(default)]
    device_name: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    settled: bool,
    #[serde(default)]
    created_at: i64,
    #[serde(default)]
    seen_at: Option<i64>,
    #[serde(default)]
    paid_at: Option<i64>,
    #[serde(default)]
    ended_at: Option<i64>,
    #[serde(default)]
    closed_at: Option<i64>,
    #[serde(default)]
    refunded_at: Option<i64>,
    #[serde(default)]
    refund_pending_at: Option<i64>,
    #[serde(default)]
    confirmations: Option<u32>,
    #[serde(default)]
    confirmations_required: Option<u32>,
    #[serde(default)]
    offered: bool,
    #[serde(default)]
    kept_as_cash_at: Option<i64>,
    #[serde(default)]
    paid_in_cash_at: Option<i64>,
}

/// The receipt as the Kotlin model wrote it: the ORF document with its
/// version and type left out at their defaults.
fn receipt_of(mut value: serde_json::Value) -> Option<orf_codec::Receipt> {
    let map = value.as_object_mut()?;
    map.entry("orf_version")
        .or_insert_with(|| serde_json::Value::from(orf_codec::ORF_VERSION));
    if let Some(info) = map.get_mut("receipt").and_then(|r| r.as_object_mut()) {
        info.entry("receipt_type")
            .or_insert_with(|| serde_json::Value::from("sale"));
    }
    serde_json::from_value(value).ok()
}

fn ms(seconds: Option<i64>) -> Option<i64> {
    seconds.map(|s| s * 1000)
}

/// The facts of every sale in `json`. Sales with the same order id are one
/// sale, under the POS key when any of them found it.
pub(crate) fn facts_of_shift(json: &str) -> Result<Vec<ImportedSale>, LedgerError> {
    let kept: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|e| LedgerError::Import(e.to_string()))?;
    let kept: Vec<Kept> = kept
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    let mut keys: BTreeMap<String, SaleKey> = BTreeMap::new();
    for sale in &kept {
        if let Some(found) = &sale.found {
            keys.insert(
                sale.order_id.clone(),
                SaleKey::pos(found.point_of_sale, found.id.as_str()),
            );
        }
    }
    let mut out = Vec::new();
    // Oldest first, so later entries win ties as they did in the list.
    for sale in kept.iter().rev() {
        let key = match &sale.found {
            Some(found) => SaleKey::pos(found.point_of_sale, found.id.as_str()),
            None => keys
                .get(&sale.order_id)
                .cloned()
                .unwrap_or_else(|| SaleKey::order(sale.order_id.as_str())),
        };
        let rung = sale
            .found
            .as_ref()
            .map(|f| f.created_at.as_second())
            .or(Some(sale.created_at).filter(|s| *s > 0))
            .map(|s| s * 1000);
        let mut facts: Vec<(Fact, Option<i64>)> = Vec::new();
        if let Some(found) = &sale.found {
            facts.push((
                Fact::PosPaymentSeen(PaymentSeen {
                    pos: found.point_of_sale,
                    payment: found.id.as_str().into(),
                    amount: found.amount,
                    tender: if found.cash {
                        Tender::Cash
                    } else {
                        Tender::Monero
                    },
                    status: till_pos::PaymentStatus::Completed,
                    device_id: found.device_id.as_deref().map(DeviceId::from),
                    device_name: found.device_name.clone(),
                    receipt_number: found.reference.clone(),
                    order_ref: found.order_id.clone(),
                    title: found.title.clone(),
                    created_at: found.created_at.clone(),
                    updated_at: rung.unwrap_or(0),
                }),
                rung,
            ));
            let receipt = sale.receipt.clone().and_then(receipt_of);
            if receipt.is_some() || sale.customer_name.is_some() {
                let read = match &receipt {
                    Some(r) => OrderRead::new(
                        key.clone(),
                        rung.unwrap_or(0),
                        Ok(r),
                        sale.customer_name.clone(),
                    ),
                    None => OrderRead {
                        sale: key.clone(),
                        updated_at: rung.unwrap_or(0),
                        receipt: None,
                        why: None,
                        customer_name: sale.customer_name.clone(),
                    },
                };
                facts.push((Fact::PosOrderRead(read), rung));
            }
        }
        if !sale.offered {
            let order_id = OrderId::new(sale.order_id.as_str());
            facts.push((
                Fact::OrderCreated(OrderCreated {
                    sale: key.clone(),
                    order_id: order_id.clone(),
                    url: sale.url.clone(),
                    payment_uri: sale.code.uri.plain().into_string(),
                    address: sale.address.clone(),
                    xmr: sale.xmr.clone(),
                    amount: sale.amount.clone(),
                    currency: sale.currency.clone(),
                    reference: sale.reference.clone(),
                    expires_at: sale.expires_at,
                    confirmations_required: sale.confirmations_required,
                    device_name: if sale.found.is_none() {
                        sale.device_name.clone()
                    } else {
                        None
                    },
                    renew: None,
                }),
                rung,
            ));
            let observed = |phase, at: Option<i64>| {
                (
                    Fact::OrderObserved(OrderObserved {
                        order_id: order_id.clone(),
                        phase,
                        confirmations: sale.confirmations,
                        is_final: sale.settled && phase == sale.phase,
                        received: (phase == SalePhase::Partial).then_some(0),
                        overpaid: false,
                        // The kept shift knew one quote: the order's first.
                        quote: None,
                        renew: None,
                    }),
                    at.or(rung),
                )
            };
            if sale.seen_at.is_some() && sale.phase != SalePhase::Seen {
                facts.push(observed(SalePhase::Seen, ms(sale.seen_at)));
            }
            let at = match sale.phase {
                SalePhase::Seen | SalePhase::Confirming => ms(sale.seen_at),
                SalePhase::Paid => ms(sale.paid_at),
                SalePhase::Expired | SalePhase::Cancelled | SalePhase::DoubleSpend => {
                    ms(sale.ended_at)
                }
                _ => None,
            };
            if sale.phase != SalePhase::Waiting || sale.settled {
                facts.push(observed(sale.phase, at));
            }
        }
        for (time, mark) in [
            (sale.closed_at, MarkKind::Closed),
            (sale.kept_as_cash_at, MarkKind::KeptAsCash),
            (sale.paid_in_cash_at, MarkKind::PaidInCash),
            (sale.refund_pending_at, MarkKind::RefundPending),
            (sale.refunded_at, MarkKind::RefundConfirmed),
        ] {
            if let Some(at) = ms(time) {
                facts.push((
                    Fact::Mark {
                        sale: key.clone(),
                        mark,
                        at,
                    },
                    Some(at),
                ));
            }
        }
        out.push(ImportedSale { facts });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kept code's receipt is taken out and nothing else: a parameter
    /// after `orf` (`tx_amount`) stays in the order's payment URI.
    #[test]
    fn the_receipt_is_taken_out_of_the_kept_code_and_nothing_else() {
        let shift = r#"[
 {"orderId":"ord_1","amount":"13.88","currency":"USD",
  "code":{"uri":"monero:8Bx?orf=abc&tx_amount=0.08","hasReceipt":true},
  "phase":"Waiting","expiresAt":1794244000,"xmr":"0.08","address":"8Bx","createdAt":1794243600}
]"#;
        let sales = facts_of_shift(shift).unwrap();
        let uri = sales[0]
            .facts
            .iter()
            .find_map(|(fact, _)| match fact {
                Fact::OrderCreated(created) => Some(created.payment_uri.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(uri, "monero:8Bx?tx_amount=0.08");
    }
}
