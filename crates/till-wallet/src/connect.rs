//! Connect to Fob, Till's side (docs/view-key-mode.md §5.9): a request
//! with a one-time key, shown as a QR code beside its identicon (C1) or
//! opened in Fob on the same Android device, and Fob's sealed answer
//! opened with that key.

use fob_codes::connect::{Answer, Asker, ConnectError, Want};
use zeroize::Zeroizing;

use crate::keys::{Network, ViewKeys};

/// The app's name in a request: what Fob names the account after.
pub const APP: &str = "Till";

/// A request to Fob, holding the one-time key its answer opens with. The
/// key is wiped when this goes: keep it only until the answer is opened.
pub struct FobRequest {
    asker: Asker,
}

/// The pattern both screens show: 5×5 cells, `true` drawn in `ink` on
/// `paper` (`#rrggbb`, the same in light and dark).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub cells: [[bool; 5]; 5],
    pub ink: String,
    pub paper: String,
}

/// What Fob handed over, as Till keeps it.
#[derive(Clone, Debug)]
pub struct Granted {
    pub keys: ViewKeys,
    /// The account Fob made for this till: its number.
    pub account: u32,
    /// "Till (Corner Café)".
    pub label: String,
    /// Fob's tip when it answered: where the wallet begins.
    pub restore_height: u64,
    pub wallet_id: String,
    pub wallet_name: String,
    /// Fob's node, when shared.
    pub node: Option<SharedNode>,
    /// Fob's light wallet server, when shared.
    pub light: Option<SharedNode>,
    /// Fob's price provider and currency, when shared.
    pub prices: Option<(String, String)>,
}

/// A node or light server Fob shared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedNode {
    pub url: String,
    pub self_signed: bool,
}

/// Why an answer didn't connect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnswerProblem {
    /// Not Fob's answer at all.
    NotAnAnswer,
    /// Fob's answer to another request, or changed on the way.
    NotForThisRequest,
    /// The person said no in Fob.
    Declined,
    /// It opened but what's inside doesn't hold together.
    BadGrant,
}

impl FobRequest {
    /// A new request for view access, with Fob's node and prices, on
    /// `network`. `name` is the shop's ("Corner Café"), `device` this
    /// device's; `callback` is where Fob answers on the same device, none
    /// for the two-device way.
    pub fn new(
        name: &str,
        device: &str,
        network: Network,
        callback: Option<&str>,
    ) -> Result<FobRequest, String> {
        let want = Want {
            node: true,
            prices: true,
        };
        Asker::new(APP, &clip(name), &clip(device), network, want, callback)
            .map(|asker| FobRequest { asker })
            .map_err(|e| e.to_string())
    }

    /// The request as a link, and what its QR code holds.
    pub fn uri(&self) -> String {
        self.asker.request().to_uri()
    }

    pub fn network(&self) -> Network {
        self.asker.request().network
    }

    pub fn pattern(&self) -> Pattern {
        let identicon = self.asker.request().identicon();
        let swatch = identicon.swatch();
        Pattern {
            cells: identicon.cells,
            ink: swatch.ink.to_owned(),
            paper: swatch.paper.to_owned(),
        }
    }

    /// Opens Fob's answer: a scanned code, a link back, or an activity's
    /// result.
    pub fn open(&self, answer: &str) -> Result<Granted, AnswerProblem> {
        if !fob_codes::connect::looks_like_answer(answer) {
            return Err(AnswerProblem::NotAnAnswer);
        }
        let grant = match self.asker.open(answer) {
            Ok(Answer::Granted(grant)) => grant,
            Ok(Answer::Declined) => return Err(AnswerProblem::Declined),
            Err(ConnectError::WrongState | ConnectError::Sealed) => {
                return Err(AnswerProblem::NotForThisRequest);
            }
            Err(ConnectError::Payload) => return Err(AnswerProblem::BadGrant),
            Err(_) => return Err(AnswerProblem::NotAnAnswer),
        };
        let network = grant.network().map_err(|_| AnswerProblem::BadGrant)?;
        let view_key = Zeroizing::new(grant.view_key.clone());
        // The view key must be the address's: Fob's word isn't taken for it.
        let keys = crate::keys::keys_from_typed(network, &grant.address, &view_key)
            .ok_or(AnswerProblem::BadGrant)?;
        if grant.account == 0 {
            // Account 0 is the merchant's own (§5.3).
            return Err(AnswerProblem::BadGrant);
        }
        let shared = |n: &fob_codes::connect::SharedNode| SharedNode {
            url: n.url.clone(),
            self_signed: n.self_signed,
        };
        Ok(Granted {
            keys,
            account: grant.account,
            label: grant.label.clone(),
            restore_height: grant.restore_height,
            wallet_id: grant.wallet_id.clone(),
            wallet_name: grant.wallet_name.clone(),
            node: grant.node.as_ref().map(shared),
            light: grant.light.as_ref().map(shared),
            prices: grant
                .prices
                .as_ref()
                .map(|p| (p.provider.clone(), p.currency.clone())),
        })
    }
}

/// Text a request may carry: at most fob-codes' limit, in characters.
fn clip(text: &str) -> String {
    text.trim()
        .chars()
        .take(fob_codes::connect::MAX_TEXT)
        .collect()
}

/// Runs the protocol's shared vectors, as every app speaking it must:
/// how many passed, or what failed.
pub fn check_vectors() -> Result<usize, String> {
    fob_codes::connect::check_vectors(fob_codes::connect::VECTORS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fob_codes::connect::{Grant, Request, SharedPrices, decline, seal};
    use fob_keys::Keys;

    #[test]
    fn the_protocols_vectors_pass_here_as_in_fob() {
        let passed = check_vectors().unwrap();
        assert!(passed > 0);
    }

    fn grant(keys: &Keys, account: u32) -> Grant {
        Grant {
            address: keys.primary_address(),
            view_key: keys.view_key_hex().to_string(),
            network: "stagenet".into(),
            account,
            label: "Till (Corner Café)".into(),
            restore_height: 2_250_000,
            wallet_id: "w1".into(),
            wallet_name: "Corner Café".into(),
            node: Some(fob_codes::connect::SharedNode {
                url: "https://node.example.com:38089".into(),
                self_signed: false,
                pin: None,
            }),
            light: None,
            prices: Some(SharedPrices {
                provider: "Kraken".into(),
                currency: "AUD".into(),
            }),
        }
    }

    #[test]
    fn fobs_answer_opens_with_the_request_that_asked_and_no_other() {
        let request =
            FobRequest::new("Corner Café", "Front iPad", Network::Stagenet, None).unwrap();
        assert!(
            request
                .uri()
                .starts_with("fob://connect?v=1&app=Till&name=Corner%20Caf")
        );
        let pattern = request.pattern();
        assert!(pattern.ink.starts_with('#') && pattern.paper.starts_with('#'));
        let parsed = Request::parse(&request.uri()).unwrap();
        assert_eq!(
            parsed.identicon().cells,
            pattern.cells,
            "Fob draws the same"
        );

        let keys = Keys::generate(fob_wallet::Network::Stagenet).keys;
        let answer = seal(&parsed, &grant(&keys, 3)).unwrap();
        let granted = request.open(&answer).unwrap();
        assert_eq!(granted.keys.address, keys.primary_address());
        assert_eq!(granted.account, 3);
        assert_eq!(granted.wallet_name, "Corner Café");
        assert_eq!(granted.node.unwrap().url, "https://node.example.com:38089");
        assert_eq!(granted.prices, Some(("Kraken".into(), "AUD".into())));

        let other = FobRequest::new("Corner Café", "Front iPad", Network::Stagenet, None).unwrap();
        assert_eq!(
            other.open(&answer).unwrap_err(),
            AnswerProblem::NotForThisRequest
        );
        assert_eq!(
            request.open(&decline(&parsed)).unwrap_err(),
            AnswerProblem::Declined
        );
        assert_eq!(
            request.open("hello").unwrap_err(),
            AnswerProblem::NotAnAnswer
        );
        let to_zero = seal(&parsed, &grant(&keys, 0)).unwrap();
        assert_eq!(request.open(&to_zero).unwrap_err(), AnswerProblem::BadGrant);
    }
}
