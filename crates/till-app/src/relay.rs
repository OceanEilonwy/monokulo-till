//! APNs through the push relay (design.md §5, "The relay API"): iOS's push
//! side for the Bell, carried by the engine, so the app only hands over its
//! APNs device token ([`Action::PushToken`]) and has no HTTP or crypto of
//! its own.
//!
//! Subscribing for a Bell registers the token with the relay under the
//! Bell's VAPID key (`POST /v1/register`, platform `apns`, app `till`) and
//! answers with the relay's endpoint and this device's Web Push keys
//! ([`PushKeys`]), as [`Action::PushEndpoint`] through the engine's own
//! queue, the way a platform's [`PushPlatform`] answers. Every token iOS
//! hands over registers again: the same token and key give the same
//! endpoint, its expiry pushed back, and a new token a new endpoint, which
//! the Bell is then given. There is no unregister (the relay has none):
//! handles expire.

use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use serde::Deserialize;
use till_types::Secret;
use tokio::runtime::Handle;
use tokio::sync::mpsc::UnboundedSender;

use crate::engine::Msg;
use crate::store::Store;
use crate::web_push::PushKeys;
use crate::{Action, Distributors, Platform, PushFailure, PushPlatform, PushProblem};

/// The name the relay serves Till's bundle ID under (its `handles.apps`).
pub(crate) const APP: &str = "till";

/// How long a registration may take before it counts as the network's
/// failure.
const TIMEOUT: Duration = Duration::from_secs(30);

/// What iOS said of the device's APNs token.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum DeviceToken {
    /// Not yet heard.
    #[default]
    NotYet,
    /// iOS couldn't register for remote notifications.
    Failed,
    /// The token's bytes in hex: as good as an address for pushes to this
    /// device, so never logged.
    Have(Secret),
}

till_types::text_id!(
    /// A Bell's VAPID public key (base64url), as the relay registers a
    /// token under it: never a token's place.
    VapidKey
);

/// The relay's answer to a registration.
#[derive(Deserialize)]
struct Registered {
    endpoint: String,
}

pub(crate) struct RelayPush {
    /// The relay's origin, with no trailing slash.
    relay: String,
    platform: Arc<dyn Platform>,
    http: reqwest::Client,
    tx: UnboundedSender<Msg>,
    runtime: Handle,
    me: Weak<RelayPush>,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    token: DeviceToken,
    /// The Bell's VAPID key while subscribed for it.
    vapid: Option<VapidKey>,
    /// Bumped by every subscription, unsubscription and token, so a
    /// registration's answer that something later replaced is dropped.
    asked: u64,
}

impl RelayPush {
    pub fn new(
        relay: &str,
        platform: Arc<dyn Platform>,
        http: reqwest::Client,
        tx: UnboundedSender<Msg>,
        runtime: Handle,
        token: DeviceToken,
    ) -> Arc<RelayPush> {
        Arc::new_cyclic(|me| RelayPush {
            relay: relay.trim_end_matches('/').to_owned(),
            platform,
            http,
            tx,
            runtime,
            me: me.clone(),
            inner: Mutex::new(Inner {
                token,
                ..Inner::default()
            }),
        })
    }

    /// iOS's word on the APNs token: registered again while subscribed.
    pub fn token(&self, token: DeviceToken) {
        let mut inner = self.inner.lock().unwrap();
        inner.token = token;
        self.register(&mut inner);
    }

    /// The plaintext of a push the relay carried, opened with this device's
    /// keys.
    pub fn open(&self, body: &[u8]) -> Option<Vec<u8>> {
        crate::web_push::open(self.platform.clone(), body)
    }

    fn answer(&self, action: Action) {
        let _ = self.tx.send(Msg::Action(action));
    }

    /// Registers the token for the Bell subscribed for, if there are both.
    fn register(&self, inner: &mut Inner) {
        inner.asked += 1;
        let Some(vapid) = inner.vapid.clone() else {
            return;
        };
        let token = match &inner.token {
            // iOS hands it over as the app starts: the subscription waits.
            DeviceToken::NotYet => return,
            DeviceToken::Failed => {
                return self.answer(Action::PushFailed {
                    failure: PushFailure::Network,
                });
            }
            DeviceToken::Have(token) => token.clone(),
        };
        let asked = inner.asked;
        let me = self.me.clone();
        let (relay, platform, http) =
            (self.relay.clone(), self.platform.clone(), self.http.clone());
        self.runtime.spawn(async move {
            let answer = registration(&relay, platform, &http, &token, &vapid).await;
            // Dropped once the engine let this push side go, or once a later
            // subscription, unsubscription or token replaced it.
            let Some(me) = me.upgrade() else { return };
            if me.inner.lock().unwrap().asked == asked {
                me.answer(answer);
            }
        });
    }
}

/// The token registered with the relay for the sender `vapid`, as the
/// action that answers it.
async fn registration(
    relay: &str,
    platform: Arc<dyn Platform>,
    http: &reqwest::Client,
    token: &Secret,
    vapid: &VapidKey,
) -> Action {
    let failed = |failure| Action::PushFailed { failure };
    let keys = match PushKeys::load_or_make(&Store::new(platform)) {
        Ok(Some(keys)) => keys,
        _ => return failed(PushFailure::NoKeys),
    };
    let Some(p256dh) = keys.p256dh() else {
        return failed(PushFailure::NoKeys);
    };
    let body = serde_json::json!({
        "platform": "apns",
        "token": token.expose_str(),
        "app": APP,
        "vapid_public_key": vapid,
    });
    let sent = http
        .post(format!("{relay}/v1/register"))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .timeout(TIMEOUT)
        .send()
        .await;
    let response = match sent {
        Ok(response) => response,
        Err(_) => return failed(PushFailure::Network),
    };
    let status = response.status();
    if status.as_u16() == 429 || status.is_server_error() {
        return failed(PushFailure::Network);
    }
    if !status.is_success() {
        // The relay refused: it doesn't serve Till, or took the token or
        // key for a bad one.
        return failed(PushFailure::Other);
    }
    match response.bytes().await {
        Ok(bytes) => match serde_json::from_slice::<Registered>(&bytes) {
            Ok(registered) => Action::PushEndpoint {
                url: registered.endpoint,
                p256dh: Some(p256dh),
                auth: Some(keys.auth()),
            },
            Err(_) => failed(PushFailure::Other),
        },
        Err(_) => failed(PushFailure::Network),
    }
}

impl PushPlatform for RelayPush {
    fn subscribe(&self, vapid_public_key: String) {
        let mut inner = self.inner.lock().unwrap();
        inner.vapid = Some(VapidKey(vapid_public_key));
        self.register(&mut inner);
    }

    fn unsubscribe(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.vapid = None;
        inner.asked += 1;
    }

    /// APNs, always: alerts that are off show as Wake-ups' "Alerts are off"
    /// (decision 25), not as a missing distributor.
    fn distributors(&self) -> Result<Distributors, PushProblem> {
        Ok(Distributors::Found {
            name: "apns".into(),
        })
    }

    fn save_distributor(&self, _name: String) {}
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::testing::FakePlatform;

    const TOKEN: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";
    const VAPID: &str = "BKey";

    fn relay(server: &MockServer) -> (Arc<RelayPush>, Arc<FakePlatform>, UnboundedReceiver<Msg>) {
        let (tx, rx) = unbounded_channel();
        let platform = FakePlatform::with(&[]);
        let push = RelayPush::new(
            &format!("{}/", server.uri()),
            platform.clone(),
            reqwest::Client::builder()
                .pool_max_idle_per_host(0)
                .build()
                .unwrap(),
            tx,
            Handle::current(),
            DeviceToken::NotYet,
        );
        (push, platform, rx)
    }

    async fn answered(rx: &mut UnboundedReceiver<Msg>) -> Action {
        match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
            Ok(Some(Msg::Action(action))) => action,
            _ => panic!("no answer"),
        }
    }

    async fn registers(server: &MockServer, answer: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path("/v1/register"))
            .and(header("content-type", "application/json"))
            .and(body_json(serde_json::json!({
                "platform": "apns",
                "token": TOKEN,
                "app": "till",
                "vapid_public_key": VAPID,
            })))
            .respond_with(answer)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn the_token_is_registered_for_the_bell_and_answered_with_the_endpoint_and_keys() {
        let server = MockServer::builder().start().await;
        registers(
            &server,
            ResponseTemplate::new(201).set_body_raw(
                r#"{"endpoint":"https://relay.example/v1/push/ph_1","expires_at":1893456000}"#,
                "application/json",
            ),
        )
        .await;
        let (push, platform, mut rx) = relay(&server);
        // Subscribed before iOS gave a token: it waits for one.
        push.subscribe(VAPID.into());
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(rx.try_recv().is_err());
        assert!(server.received_requests().await.unwrap().is_empty());

        push.token(DeviceToken::Have(TOKEN.into()));
        let Action::PushEndpoint { url, p256dh, auth } = answered(&mut rx).await else {
            panic!("not an endpoint");
        };
        assert_eq!(url, "https://relay.example/v1/push/ph_1");
        let keys = PushKeys::load(&Store::new(platform.clone()))
            .unwrap()
            .unwrap();
        assert_eq!(p256dh, keys.p256dh());
        assert_eq!(auth, Some(keys.auth()));

        // A token handed over again registers again, with the same keys.
        push.token(DeviceToken::Have(TOKEN.into()));
        let Action::PushEndpoint { p256dh: again, .. } = answered(&mut rx).await else {
            panic!("not an endpoint");
        };
        assert_eq!(again, keys.p256dh());
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_relay_that_refuses_or_cant_be_reached_fails_the_subscription() {
        for (status, failure) in [
            (400, PushFailure::Other),
            (429, PushFailure::Network),
            (503, PushFailure::Network),
        ] {
            let server = MockServer::builder().start().await;
            registers(&server, ResponseTemplate::new(status)).await;
            let (push, _, mut rx) = relay(&server);
            push.token(DeviceToken::Have(TOKEN.into()));
            push.subscribe(VAPID.into());
            assert_eq!(answered(&mut rx).await, Action::PushFailed { failure });
        }
        // An endpoint the relay didn't give.
        let server = MockServer::builder().start().await;
        registers(
            &server,
            ResponseTemplate::new(201).set_body_raw("{}", "application/json"),
        )
        .await;
        let (push, _, mut rx) = relay(&server);
        push.token(DeviceToken::Have(TOKEN.into()));
        push.subscribe(VAPID.into());
        assert_eq!(
            answered(&mut rx).await,
            Action::PushFailed {
                failure: PushFailure::Other
            }
        );
    }

    #[tokio::test]
    async fn no_token_from_ios_fails_the_subscription_and_keys_that_cant_be_kept_too() {
        let server = MockServer::builder().start().await;
        let (push, platform, mut rx) = relay(&server);
        push.token(DeviceToken::Failed);
        push.subscribe(VAPID.into());
        assert_eq!(
            answered(&mut rx).await,
            Action::PushFailed {
                failure: PushFailure::Network
            }
        );
        platform.fail(true);
        push.token(DeviceToken::Have(TOKEN.into()));
        assert_eq!(
            answered(&mut rx).await,
            Action::PushFailed {
                failure: PushFailure::NoKeys
            }
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_answer_after_unsubscribing_is_dropped() {
        let server = MockServer::builder().start().await;
        registers(
            &server,
            ResponseTemplate::new(201)
                .set_body_raw(
                    r#"{"endpoint":"https://relay.example/v1/push/ph_1"}"#,
                    "application/json",
                )
                .set_delay(Duration::from_millis(200)),
        )
        .await;
        let (push, _, mut rx) = relay(&server);
        push.token(DeviceToken::Have(TOKEN.into()));
        push.subscribe(VAPID.into());
        push.unsubscribe();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(rx.try_recv().is_err());
        assert_eq!(
            push.distributors().unwrap(),
            Distributors::Found {
                name: "apns".into()
            }
        );
    }
}
