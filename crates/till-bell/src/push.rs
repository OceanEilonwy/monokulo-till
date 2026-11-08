//! The device's push subscription for its Bell (Android:
//! `push/PushSubscriptions.kt`, and the platform-neutral part of
//! `push/UnifiedPushSubscriptions.kt`).
//!
//! The platform makes the subscription: UnifiedPush's connector on Android,
//! APNs through the relay on iOS. It implements [`PushSubscriptions`],
//! usually over a [`PushFeed`] it hands what its callbacks say.

use monokulo_client::PointOfSale;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, watch};

/// A Web Push subscription as the Bell takes it (monokulo's `docs/bell.md`,
/// `POST /v1/register`): the endpoint the Bell pushes to and the public half
/// of this device's keys, base64url. The private keys stay on the device
/// (decision 92); nothing here can decrypt a ding.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PushSubscription {
    pub endpoint: String,
    /// The device's P-256 public key, uncompressed, base64url.
    pub p256dh: String,
    /// The 16-byte auth secret, base64url.
    pub auth: String,
}

/// Why a push subscription couldn't be made, or ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PushFailure {
    /// Nothing on the device can carry dings: no UnifiedPush distributor
    /// (ntfy, NextPush, …) is installed.
    NoDistributor,
    /// The distributor couldn't reach its server: worth trying again when
    /// the network is back.
    Network,
    /// The distributor needs the cashier to do something in it first (sign
    /// in again, say).
    ActionRequired,
    /// The distributor gave an endpoint without Web Push keys, so the Bell
    /// couldn't encrypt for it.
    NoKeys,
    /// The distributor ended the subscription: no more dings come to it.
    Lost,
    /// Anything else the platform reported.
    Other,
}

/// Where the device's push subscription stands.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum PushState {
    /// Not subscribed.
    #[default]
    None,
    /// Asked for a subscription; waiting for its endpoint.
    Subscribing,
    Subscribed(PushSubscription),
    Failed(PushFailure),
}

/// The device's Web Push subscription for the Bell (docs/design.md §5).
/// One subscription at a time: for the Bell in use, made for its VAPID key,
/// so only that Bell can push to it.
///
/// On Android this is UnifiedPush's connector, through whichever distributor
/// is installed; on iOS it is APNs through the relay, the one "distributor"
/// there is. Calls come from the core's tasks and must not block.
pub trait PushSubscriptions: Send + Sync {
    /// Where the subscription stands, now and as it changes.
    fn state(&self) -> watch::Receiver<PushState>;

    /// Each ding the Bell sent, once the platform has decrypted it and it
    /// reads as `{"wake":"<the till's point of sale>"}` ([`is_ding`]).
    /// Nothing else about it: a ding carries no sale. A ding nothing is
    /// listening for is dropped (decision 93), and a listener that falls
    /// behind skips to the newest.
    fn dings(&self) -> broadcast::Receiver<()>;

    /// Subscribe for a sender whose VAPID public key is `vapid_public_key`
    /// (base64url, the Bell's `/v1/info`). The outcome comes through
    /// [`PushSubscriptions::state`]: [`PushState::Subscribing`] at once, or
    /// a failure. Subscribing again with another key replaces the
    /// subscription.
    fn subscribe(&self, vapid_public_key: &str);

    /// End the subscription; the state goes to [`PushState::None`] at once.
    fn unsubscribe(&self);
}

/// An endpoint the platform gave, with the subscription's public keys when
/// it gave them: a subscription, or, without both keys, one the Bell can't
/// encrypt for.
pub fn endpoint(url: &str, p256dh: Option<&str>, auth: Option<&str>) -> PushState {
    match (p256dh, auth) {
        (Some(p256dh), Some(auth)) if !p256dh.is_empty() && !auth.is_empty() => {
            PushState::Subscribed(PushSubscription {
                endpoint: url.to_owned(),
                p256dh: p256dh.to_owned(),
                auth: auth.to_owned(),
            })
        }
        _ => PushState::Failed(PushFailure::NoKeys),
    }
}

/// Whether a push is a ding: decrypted, and exactly the JSON object
/// `{"wake":"<point_of_sale>"}` for the till's point of sale (decisions 92,
/// 133). A Bell rings a till only for the point of sale it registered, so a
/// ding for another is stale or forged; a till with none takes no ding.
pub fn is_ding(content: &[u8], decrypted: bool, point_of_sale: Option<PointOfSale>) -> bool {
    let Some(point_of_sale) = point_of_sale.filter(|_| decrypted) else {
        return false;
    };
    let Ok(serde_json::Value::Object(object)) = serde_json::from_slice(content) else {
        return false;
    };
    object.len() == 1
        && matches!(object.get("wake"), Some(serde_json::Value::String(wake)) if wake == point_of_sale.wire())
}

/// The state and dings of a [`PushSubscriptions`], fed by the platform's
/// callbacks: what they mean apart from any platform. A platform's
/// implementation holds one, returns its receivers, and hands it each
/// endpoint, push, failure and end it hears of.
#[derive(Debug)]
pub struct PushFeed {
    state: watch::Sender<PushState>,
    dings: broadcast::Sender<()>,
}

impl Default for PushFeed {
    fn default() -> PushFeed {
        PushFeed::new()
    }
}

impl PushFeed {
    /// A feed with no subscription.
    pub fn new() -> PushFeed {
        // One ding held is enough: a ding means "look now", and two mean no more.
        PushFeed {
            state: watch::Sender::new(PushState::None),
            dings: broadcast::Sender::new(1),
        }
    }

    /// For [`PushSubscriptions::state`].
    pub fn state(&self) -> watch::Receiver<PushState> {
        self.state.subscribe()
    }

    /// For [`PushSubscriptions::dings`].
    pub fn dings(&self) -> broadcast::Receiver<()> {
        self.dings.subscribe()
    }

    /// The state as it is now.
    pub fn current(&self) -> PushState {
        self.state.borrow().clone()
    }

    /// Set the state: [`PushState::Subscribing`] when the platform is asked
    /// to subscribe, [`PushState::None`] when it is asked to unsubscribe, or
    /// a failure it reports.
    pub fn set(&self, state: PushState) {
        self.state.send_if_modified(|held| {
            let changed = *held != state;
            *held = state;
            changed
        });
    }

    /// The platform gave an endpoint ([`endpoint`]).
    pub fn endpoint(&self, url: &str, p256dh: Option<&str>, auth: Option<&str>) {
        self.set(endpoint(url, p256dh, auth));
    }

    /// The platform had a push, decrypted or not, while the till is signed
    /// in to `point_of_sale` (as it is when the push arrives): a ding goes
    /// to [`PushFeed::dings`], anything else nowhere.
    pub fn message(&self, content: &[u8], decrypted: bool, point_of_sale: Option<PointOfSale>) {
        if is_ding(content, decrypted, point_of_sale) {
            // No one listening is no shift running: the ding is dropped.
            let _ = self.dings.send(());
        }
    }

    /// The platform ended the subscription. One the app ended itself is
    /// already [`PushState::None`] and stays so; any other is
    /// [`PushFailure::Lost`].
    pub fn unregistered(&self) {
        if self.current() != PushState::None {
            self.set(PushState::Failed(PushFailure::Lost));
        }
    }
}

/// What the platform's callbacks mean: only a decrypted
/// `{"wake":"<the till's point of sale>"}` is a ding (acceptance A8.35,
/// A8.36; decision 133). Ported from `PushEventsTest`.
#[cfg(test)]
mod tests {
    use super::*;

    fn ding(text: &str) -> bool {
        ding_at(text, true, Some(PointOfSale::Square))
    }

    fn ding_at(text: &str, decrypted: bool, pos: Option<PointOfSale>) -> bool {
        is_ding(text.as_bytes(), decrypted, pos)
    }

    #[test]
    fn a_ding_is_the_decrypted_object_with_wake_square_and_nothing_else() {
        assert!(ding(r#"{"wake":"square"}"#));
        assert!(
            ding(r#" { "wake" : "square" } "#),
            "whitespace doesn't matter"
        );
    }

    #[test]
    fn a_clover_till_takes_wake_clover_and_each_point_of_sale_drops_the_others() {
        let clover = Some(PointOfSale::Clover);
        assert!(ding_at(r#"{"wake":"clover"}"#, true, clover));
        assert!(
            !ding_at(r#"{"wake":"square"}"#, true, clover),
            "Square's ding on a Clover till"
        );
        assert!(
            !ding_at(r#"{"wake":"clover"}"#, true, Some(PointOfSale::Square)),
            "Clover's ding on a Square till"
        );
        assert!(
            !ding_at(r#"{"wake":"Clover"}"#, true, clover),
            "the wire name is lowercase"
        );
        assert!(
            !ding_at(r#"{"wake":"clover"}"#, false, clover),
            "not decrypted"
        );
    }

    #[test]
    fn a_till_with_no_point_of_sale_takes_no_ding() {
        for pos in PointOfSale::ALL {
            assert!(
                !ding_at(&format!(r#"{{"wake":"{}"}}"#, pos.wire()), true, None),
                "{}",
                pos.wire()
            );
        }
    }

    #[test]
    fn anything_else_is_dropped() {
        assert!(
            !ding_at(r#"{"wake":"square"}"#, false, Some(PointOfSale::Square)),
            "not decrypted"
        );
        assert!(!ding(""), "empty");
        assert!(!ding("wake square"), "not JSON");
        assert!(!ding(r#"["wake","square"]"#), "not an object");
        assert!(!ding(r#""square""#), "a string");
        assert!(!ding(r#"{"wake":"square","sale":"4500"}"#), "another key");
        assert!(!ding(r#"{"wake":"clover"}"#), "another POS");
        assert!(!ding(r#"{"wake":["square"]}"#), "not a string");
        assert!(!ding(r#"{"wake":null}"#), "null");
        assert!(!ding("{}"), "nothing");
        assert!(
            !is_ding(&[0xC3, 0x28], true, Some(PointOfSale::Square)),
            "not UTF-8"
        );
    }

    #[test]
    fn an_endpoint_needs_both_keys_to_be_a_subscription() {
        assert_eq!(
            endpoint("https://ntfy.sh/up1", Some("BKey"), Some("auth")),
            PushState::Subscribed(PushSubscription {
                endpoint: "https://ntfy.sh/up1".into(),
                p256dh: "BKey".into(),
                auth: "auth".into(),
            }),
        );
        assert_eq!(
            endpoint("https://ntfy.sh/up1", None, None),
            PushState::Failed(PushFailure::NoKeys)
        );
        assert_eq!(
            endpoint("https://ntfy.sh/up1", Some("BKey"), Some("")),
            PushState::Failed(PushFailure::NoKeys)
        );
    }
}
