//! What Wake-ups chooses, reads, keeps and shows (Android: `push/WakeUps.kt`).

use std::collections::BTreeMap;
use std::time::Duration;

use monokulo_client::PointOfSale;
use serde::{Deserialize, Serialize};

use crate::{BellAddress, PushSubscription};

/// Settings › Wake-ups (docs/design.md §3): what makes the till check its
/// point of sale, as the cashier chose it (the acceptance criteria's
/// "mode"). Kept as `{"type":"timer"}`, `{"type":"monokulo"}` or
/// `{"type":"own","bell":{"url":…}}`, as the Android app keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WakeUpChoice {
    /// The default: the till checks its point of sale every 2 seconds
    /// during a shift.
    #[default]
    #[serde(rename = "timer")]
    TimerOnly,
    /// The project's Bell, the build's.
    #[serde(rename = "monokulo")]
    MonokuloBell,
    /// A Bell the merchant runs.
    #[serde(rename = "own")]
    OwnBell { bell: BellAddress },
}

impl WakeUpChoice {
    /// The Bell this choice names, given the project's; `None` for the
    /// timer alone.
    pub fn bell(&self, monokulo_bell: &BellAddress) -> Option<BellAddress> {
        match self {
            WakeUpChoice::TimerOnly => None,
            WakeUpChoice::MonokuloBell => Some(monokulo_bell.clone()),
            WakeUpChoice::OwnBell { bell } => Some(bell.clone()),
        }
    }
}

/// What a Bell says about itself: `GET /v1/info` (monokulo's
/// `docs/bell.md`), as [`BellApi`](crate::BellApi) reads it. One Bell may
/// ring several environments of each point of sale at once (Square's
/// production and its sandbox, say), each through its own application
/// (decision 305).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BellInfo {
    /// The points of sale this Bell rings tills for, by wire name
    /// ([`PointOfSale::wire`]), each mapping its environments to the
    /// application the till must sign in through for this Bell to hear its
    /// sales there. Square's environments are `production` and `sandbox`;
    /// Clover's `north_america`, `europe`, `latin_america` and `sandbox`. A
    /// point of sale the Bell doesn't run is absent; a name this build
    /// doesn't know, of either, is kept and never read.
    pub points_of_sale: BTreeMap<String, BTreeMap<String, String>>,
    /// The key every ding is signed with: what the subscription is made for.
    pub vapid_public_key: String,
}

impl BellInfo {
    /// The Bell's environments for `point_of_sale`, each to its
    /// application; `None` when it doesn't ring that point of sale's tills.
    pub fn get(&self, point_of_sale: PointOfSale) -> Option<&BTreeMap<String, String>> {
        self.points_of_sale.get(point_of_sale.wire())
    }
}

/// The till's sign-in at its point of sale, as registering with a Bell
/// needs it (decision 132): the application and environment it signed in
/// through, and `location_id`, the Square location it watches or the Clover
/// merchant it is at (a Clover merchant is its own location, acceptance D1).
/// `environment` is spelt as a Bell's `/v1/info` spells it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PosSignIn {
    pub point_of_sale: PointOfSale,
    pub application_id: String,
    pub environment: String,
    pub location_id: String,
}

/// A registration the Bell holds for this till: `POST /v1/register`'s
/// answer and what it was made with. Kept in the Android app's JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BellRegistration {
    pub bell: BellAddress,
    /// `br_…`: known only to this till, and all `DELETE /v1/register/{id}`
    /// needs.
    pub registration_id: String,
    /// The Bell's key the subscription was made for: another one means
    /// subscribing again (decision 95).
    pub vapid_public_key: String,
    /// The subscription registered, to tell when the platform has changed
    /// it.
    pub subscription: PushSubscription,
    /// The distributor the subscription was made through: at a start,
    /// before this run has subscribed, the one an uninstall took away
    /// (decision 122).
    pub distributor: String,
    /// The point of sale ([`PointOfSale::wire`]) and environment it was
    /// made for: a till whose sign-in moved to another registers again
    /// (decision 306). `None` in a registration kept from before a Bell
    /// rang several environments, which counts as another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point_of_sale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
}

impl BellRegistration {
    /// Made for `sign_in`'s point of sale and environment.
    pub fn is_for(&self, sign_in: &PosSignIn) -> bool {
        self.point_of_sale.as_deref() == Some(sign_in.point_of_sale.wire())
            && self.environment.as_deref() == Some(sign_in.environment.as_str())
    }
}

/// What the app knows about the Bell it chose (the acceptance criteria's
/// "Bell status"): one value at a time, for the shift's pace and the
/// Settings card. Times are Unix seconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WakeUpStatus {
    /// Timer only: no Bell, no status card.
    Off,
    /// A Bell is chosen, but the till isn't signed in to a point of sale,
    /// which registering needs.
    NoPos,
    /// A Bell is chosen, but nothing on the device can carry its dings: no
    /// UnifiedPush distributor is installed.
    NoDistributor,
    /// Several distributors are installed and none is the default: the
    /// cashier picks one (`packages`: their package names).
    ChooseDistributor { packages: Vec<String> },
    /// Reading the Bell, subscribing, or registering.
    Connecting,
    /// Registered: dings reach the till, so the timer runs at 30 s.
    /// `last_ding_at` is the last ding's time, `None` for none yet.
    Connected { last_ding_at: Option<i64> },
    /// The timer found a sale the Bell hadn't rung for (decision 94): the
    /// timer is back at 2 s and the till has registered again; the next
    /// ding makes it [`WakeUpStatus::Connected`].
    MissingWakeups { last_ding_at: Option<i64> },
    /// The Bell couldn't be reached, or isn't one: `reason` is short, in
    /// the cashier's words ("no connection", "it didn't answer", "it
    /// answered 503", "it isn't a Bell", "the UnifiedPush app refused").
    /// Retried by itself, with a growing wait.
    Unreachable { reason: String },
    /// The Bell answered with a problem: `reason` in the cashier's words
    /// (monokulo's `docs/bell.md` types, decision 95). `retry_at` is when it
    /// tries again by itself (`rate-limited`); `None` when only a re-pick, a
    /// sign-in, an app start or a shift start will.
    Refused {
        reason: String,
        retry_at: Option<i64>,
    },
    /// The Bell hears `point_of_sale` through another application,
    /// `application_id`: the till must sign in to it through that one
    /// (decisions 96, 132).
    SignInNeeded {
        point_of_sale: PointOfSale,
        application_id: String,
    },
}

impl WakeUpStatus {
    /// How often the shift's timer checks the point of sale: 30 s only
    /// while [`WakeUpStatus::Connected`], else 2 s (docs/design.md §3, §7).
    pub fn interval(&self) -> Duration {
        match self {
            WakeUpStatus::Connected { .. } => Duration::from_secs(30),
            _ => Duration::from_secs(2),
        }
    }

    pub(crate) fn unreachable(reason: impl Into<String>) -> WakeUpStatus {
        WakeUpStatus::Unreachable {
            reason: reason.into(),
        }
    }

    pub(crate) fn refused(reason: impl Into<String>) -> WakeUpStatus {
        WakeUpStatus::Refused {
            reason: reason.into(),
            retry_at: None,
        }
    }
}

/// What Wake-ups keeps between runs, with the rest of the till's state. The
/// engine holds it in memory and saves it as it changes; reading and writing
/// here never fail.
pub trait WakeUpStore: Send + Sync {
    fn choice(&self) -> WakeUpChoice;
    fn set_choice(&self, choice: WakeUpChoice);

    /// The last "My own Bell" address entered, kept through Timer only so
    /// the row still shows it (decision 97).
    fn own_bell(&self) -> Option<BellAddress>;
    fn set_own_bell(&self, bell: Option<BellAddress>);

    /// The registration the Bell holds, while it does.
    fn registration(&self) -> Option<BellRegistration>;
    fn set_registration(&self, registration: Option<BellRegistration>);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registration_reads_and_writes_the_android_apps_json() {
        let kept = r#"{"bell":{"url":"https://bell.monokulo.com"},"registration_id":"br_1","vapid_public_key":"BOS-key","subscription":{"endpoint":"https://ntfy.sh/up1","p256dh":"BKey","auth":"auth"},"distributor":"io.heckel.ntfy"}"#;
        let registration: BellRegistration = serde_json::from_str(kept).unwrap();
        assert_eq!(registration.point_of_sale, None);
        assert_eq!(
            serde_json::to_string(&registration).unwrap(),
            kept,
            "no nulls written"
        );
        assert_eq!(
            serde_json::to_string(&WakeUpChoice::MonokuloBell).unwrap(),
            r#"{"type":"monokulo"}"#
        );
    }
}
