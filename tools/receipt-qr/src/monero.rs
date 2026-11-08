//! Monero payment URIs, `monero:<address>?tx_amount=<XMR>`, as the app
//! makes them. The receipt goes on the end as `orf-codec`'s query parameter.

pub struct PaymentUri {
    pub address: String,
    /// `tx_amount` as written, in XMR (e.g. "0.0803").
    pub tx_amount: Option<String>,
}

/// `piconero` (1 XMR = 10^12) in XMR, without trailing zeros.
pub fn xmr(piconero: u64) -> String {
    let whole = piconero / 1_000_000_000_000;
    let frac = format!("{:012}", piconero % 1_000_000_000_000);
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{frac}")
    }
}

pub fn build(address: &str, piconero: u64) -> String {
    format!("monero:{address}?tx_amount={}", xmr(piconero))
}

pub fn parse(uri: &str) -> Result<PaymentUri, &'static str> {
    let uri = uri.trim();
    let rest = uri
        .strip_prefix("monero:")
        .or_else(|| uri.strip_prefix("MONERO:"))
        .ok_or("not a monero: URI")?;
    let (address, query) = rest.split_once('?').unwrap_or((rest, ""));
    if address.is_empty() {
        return Err("the URI has no address");
    }
    let tx_amount = query
        .split('&')
        .find_map(|p| p.strip_prefix("tx_amount="))
        .map(String::from);
    Ok(PaymentUri {
        address: address.to_string(),
        tx_amount,
    })
}
