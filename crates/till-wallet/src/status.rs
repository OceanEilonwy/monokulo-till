//! Where a payment stands, from the payments made toward it: mokulo's
//! `derive_status` and `covering` (`engine/src/status.rs`), lifted as they
//! are so Monokulo and the device's own wallet agree on every case
//! (docs/view-key-mode.md §5.6). Pure: the caller says what was paid, the
//! terms and the time.
//!
//! Its test cases are mokulo's, row for row.

use monokulo_client::OrderStatus;

/// One payment toward a request, as the status is judged from it: those
/// double spent are left out by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaymentView {
    /// Piconero.
    pub amount: u64,
    pub confirmations: u64,
    /// Still in the pool: no block yet.
    pub is_zero_conf: bool,
}

/// What the payment asked for, and when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusInputs {
    /// Piconero, tag and all.
    pub amount: u64,
    /// Settled Confirmations: Paid at this many. 0 would trust the pool,
    /// and needs no case of its own; Till never asks for fewer than 10.
    pub confirmations_required: u64,
    /// Unix seconds.
    pub now: i64,
    pub expires_at: i64,
}

/// The status the payments toward a request give it.
///
/// Fully covered: Paid (or Overpaid) once the covering payments have the
/// confirmations required, Unconfirmed while they're all in the pool, else
/// Confirming. Short of the amount: Expired past expiry (even partly paid:
/// the money is at the address and a person refunds it), Pending with
/// nothing, else Partial.
pub fn derive_status(payments: &[PaymentView], inputs: StatusInputs) -> OrderStatus {
    // Saturating: a sum that wrapped could turn an overpayment into a
    // partial one.
    let total = total(payments);

    if let Some(covering) = covering(payments, inputs.amount) {
        let min_confirmations = least_confirmed(covering.iter().copied());
        let all_zero_conf = !covering.is_empty() && covering.iter().all(|p| p.is_zero_conf);
        let sufficiently_confirmed = min_confirmations >= inputs.confirmations_required;

        if sufficiently_confirmed {
            if total > inputs.amount {
                OrderStatus::Overpaid
            } else {
                OrderStatus::Paid
            }
        } else if all_zero_conf {
            OrderStatus::Unconfirmed
        } else {
            OrderStatus::Confirming
        }
    } else if inputs.now > inputs.expires_at {
        OrderStatus::Expired
    } else if total == 0 {
        OrderStatus::Pending
    } else {
        OrderStatus::Partial
    }
}

/// The confirmations a payment shows: those of the payments its status is
/// judged by, so a later payment to the same address never lowers the count
/// of one that covers it. Short of the amount, every payment counts.
pub fn confirmations(payments: &[PaymentView], amount: u64) -> u64 {
    match covering(payments, amount) {
        Some(covering) => least_confirmed(covering),
        None => least_confirmed(payments),
    }
}

/// Every piconero received, saturating.
pub fn total(payments: &[PaymentView]) -> u64 {
    payments
        .iter()
        .fold(0u64, |sum, p| sum.saturating_add(p.amount))
}

/// The payments that cover `amount`, deepest first: a later payment to the
/// same address (dust, or a second overpayment still in the pool) never
/// walks a paid payment back to Confirming. `None` while they're short of
/// it.
fn covering(payments: &[PaymentView], amount: u64) -> Option<Vec<&PaymentView>> {
    if total(payments) < amount {
        return None;
    }
    let mut by_depth: Vec<&PaymentView> = payments.iter().collect();
    by_depth.sort_by(|a, b| {
        b.confirmations
            .cmp(&a.confirmations)
            .then(a.is_zero_conf.cmp(&b.is_zero_conf))
    });
    let mut covered = 0u64;
    let mut covering = Vec::with_capacity(by_depth.len());
    for payment in by_depth {
        covering.push(payment);
        covered = covered.saturating_add(payment.amount);
        if covered >= amount {
            break;
        }
    }
    Some(covering)
}

fn least_confirmed<'a>(payments: impl IntoIterator<Item = &'a PaymentView>) -> u64 {
    payments
        .into_iter()
        .map(|p| p.confirmations)
        .min()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(expected: u64, conf_required: u64, now: i64, expires_at: i64) -> StatusInputs {
        StatusInputs {
            amount: expected,
            confirmations_required: conf_required,
            now,
            expires_at,
        }
    }

    fn confirmed(amount: u64, confirmations: u64) -> PaymentView {
        PaymentView {
            amount,
            confirmations,
            is_zero_conf: false,
        }
    }

    fn zero_conf(amount: u64) -> PaymentView {
        PaymentView {
            amount,
            confirmations: 0,
            is_zero_conf: true,
        }
    }

    /// mokulo's cases (`derive_status_decides_each_case`), as they are:
    /// what is paid, the terms, and the status that follows. Times are
    /// `now` against `expires_at` 1000 unless a row says.
    #[test]
    fn derive_status_decides_each_case() {
        let before = |amount, required| inputs(amount, required, 500, 1000);
        let after = |amount, required| inputs(amount, required, 1500, 1000);
        let long_past = |amount, required| inputs(amount, required, 9_999_999, 1000);
        let nothing = PaymentView {
            amount: 0,
            confirmations: 0,
            is_zero_conf: true,
        };
        let cases: Vec<(&str, Vec<PaymentView>, StatusInputs, OrderStatus)> = vec![
            (
                "no payments before expiry",
                vec![],
                before(100, 10),
                OrderStatus::Pending,
            ),
            (
                "no payments after expiry",
                vec![],
                after(100, 10),
                OrderStatus::Expired,
            ),
            (
                "a fully confirmed half payment is partial",
                vec![confirmed(50, 100)],
                before(100, 10),
                OrderStatus::Partial,
            ),
            (
                "a partial payment after expiry is expired",
                vec![confirmed(50, 100)],
                after(100, 10),
                OrderStatus::Expired,
            ),
            (
                "the full amount, all in the pool, short of a threshold",
                vec![zero_conf(100)],
                before(100, 10),
                OrderStatus::Unconfirmed,
            ),
            (
                "the full amount in the pool at threshold zero",
                vec![zero_conf(100)],
                before(100, 0),
                OrderStatus::Paid,
            ),
            (
                "mined and pooled, below the threshold",
                vec![confirmed(60, 2), zero_conf(40)],
                before(100, 10),
                OrderStatus::Confirming,
            ),
            (
                "mined and pooled at threshold zero",
                vec![confirmed(60, 2), zero_conf(40)],
                before(100, 0),
                OrderStatus::Paid,
            ),
            (
                "exactly at the threshold",
                vec![confirmed(100, 10)],
                before(100, 10),
                OrderStatus::Paid,
            ),
            (
                "one below the threshold",
                vec![confirmed(100, 9)],
                before(100, 10),
                OrderStatus::Confirming,
            ),
            (
                "overpaid and confirmed",
                vec![confirmed(150, 10)],
                before(100, 10),
                OrderStatus::Overpaid,
            ),
            (
                "overpaid in the pool at threshold zero",
                vec![zero_conf(150)],
                before(100, 0),
                OrderStatus::Overpaid,
            ),
            (
                "two payments covering the order",
                vec![confirmed(60, 10), confirmed(40, 10)],
                before(100, 10),
                OrderStatus::Paid,
            ),
            (
                "the one left after the other was voided",
                vec![confirmed(60, 10)],
                before(100, 10),
                OrderStatus::Partial,
            ),
            (
                "survivors that still cover the order",
                vec![confirmed(60, 10), confirmed(60, 10)],
                before(100, 10),
                OrderStatus::Overpaid,
            ),
            (
                "a zero-threshold payment once mined",
                vec![confirmed(100, 1)],
                inputs(100, 0, 600, 100_000),
                OrderStatus::Paid,
            ),
            (
                "a zero-threshold payment 12 deep",
                vec![confirmed(100, 12)],
                inputs(100, 0, 700, 100_000),
                OrderStatus::Paid,
            ),
            (
                "mined and pooled at threshold zero, later",
                vec![confirmed(60, 2), zero_conf(40)],
                inputs(100, 0, 800, 100_000),
                OrderStatus::Paid,
            ),
            (
                "settled on confirmations, long past expiry",
                vec![confirmed(100, 10)],
                long_past(100, 0),
                OrderStatus::Paid,
            ),
            (
                "settled on a zero threshold, long past expiry",
                vec![zero_conf(100)],
                long_past(100, 0),
                OrderStatus::Paid,
            ),
            (
                "overpaid, long past expiry",
                vec![confirmed(150, 10)],
                long_past(100, 0),
                OrderStatus::Overpaid,
            ),
            (
                "a piconero short at threshold zero, long past expiry",
                vec![zero_conf(99)],
                long_past(100, 0),
                OrderStatus::Expired,
            ),
            (
                "paid, then dust in the pool",
                vec![confirmed(100, 10), zero_conf(1)],
                before(100, 10),
                OrderStatus::Overpaid,
            ),
            (
                "paid, then nothing in the pool",
                vec![confirmed(100, 10), nothing],
                before(100, 10),
                OrderStatus::Paid,
            ),
            (
                "paid, then a second payment a block deep",
                vec![confirmed(100, 10), confirmed(100, 1)],
                before(100, 10),
                OrderStatus::Overpaid,
            ),
            (
                "two halves, one shallow",
                vec![confirmed(50, 10), confirmed(50, 3)],
                before(100, 10),
                OrderStatus::Confirming,
            ),
            (
                "a deep full payment and a shallow half",
                vec![confirmed(50, 3), confirmed(100, 10)],
                before(100, 10),
                OrderStatus::Overpaid,
            ),
            (
                "covered by a pooled and a mined payment",
                vec![zero_conf(60), confirmed(40, 1)],
                before(100, 10),
                OrderStatus::Confirming,
            ),
            (
                "amounts that would overflow saturate",
                vec![confirmed(u64::MAX, 10), confirmed(10, 10)],
                before(100, 10),
                OrderStatus::Overpaid,
            ),
        ];
        for (case, payments, terms, expected) in cases {
            assert_eq!(derive_status(&payments, terms), expected, "{case}");
        }
    }

    #[test]
    fn the_count_shown_is_the_covering_payments_never_a_later_one() {
        let paid_then_dust = [confirmed(100, 10), zero_conf(1)];
        assert_eq!(confirmations(&paid_then_dust, 100), 10);
        let halves = [confirmed(50, 10), confirmed(50, 3)];
        assert_eq!(confirmations(&halves, 100), 3);
        let short = [confirmed(50, 7)];
        assert_eq!(confirmations(&short, 100), 7);
        assert_eq!(confirmations(&[], 100), 0);
    }
}
