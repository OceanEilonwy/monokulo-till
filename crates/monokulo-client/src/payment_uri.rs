//! Monero payment URIs: `monero:<address>?tx_amount=<XMR>` (Android:
//! `PaymentUri` in `core/Order.kt`).

const PICONERO_PER_XMR: u64 = 1_000_000_000_000;

/// `piconero` as XMR, without trailing zeros: 80370584829 is
/// "0.080370584829", and 2000000000000 is "2".
pub fn xmr(piconero: u64) -> String {
    let whole = piconero / PICONERO_PER_XMR;
    let frac = format!("{:012}", piconero % PICONERO_PER_XMR);
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{frac}")
    }
}

/// The plain payment URI for `piconero` to `address`, which a receipt is
/// appended to.
pub fn build(address: &str, piconero: u64) -> String {
    format!("monero:{address}?tx_amount={}", xmr(piconero))
}
