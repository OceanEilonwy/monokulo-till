//! The store as last read from its payment backend (Android:
//! `StoreNow.kt`).

use std::sync::Arc;

use monokulo_client::{AppConnection, Store};
use till_payments::{BackendInfo, PaymentBackend};
use tokio::runtime::Handle;
use tokio::sync::watch;

/// The store and this device as last read from the payment backend, this
/// session. Held in memory and never saved (decision 50 in
/// docs/decisions/android.md): shown as "last seen" while the next read is
/// under way.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreNow {
    /// What the backend said of itself ([`PaymentBackend::describe`]).
    pub backend: Option<BackendInfo>,
    /// A Monokulo store, as it said: `None` from any other backend.
    pub store: Option<Store>,
    pub device: Option<AppConnection>,
    /// The entry on the store's onion address (`mk:onion-api`), while it
    /// has one.
    pub onion_api: Option<String>,
    /// A read is under way.
    pub reading: bool,
    /// Why the last read failed, if it did.
    pub problem: Option<String>,
}

/// Told the store answered 401: it disconnected this device.
pub type OnDisconnected = Arc<dyn Fn() + Send + Sync>;

/// Told where the store's onion entry is now, or `None` for none: kept
/// with the connection.
pub type OnOnionApi = Arc<dyn Fn(Option<&str>) + Send + Sync>;

/// Reads the store live: on app start, after connecting, and each time
/// Settings opens.
pub struct LiveStore {
    on_disconnected: OnDisconnected,
    on_onion_api: OnOnionApi,
    runtime: Handle,
    state: watch::Sender<StoreNow>,
}

impl LiveStore {
    /// A live store, its reads run on the tokio runtime it is made in.
    ///
    /// # Panics
    ///
    /// When made outside a tokio runtime.
    pub fn new(on_disconnected: OnDisconnected, on_onion_api: OnOnionApi) -> LiveStore {
        LiveStore {
            on_disconnected,
            on_onion_api,
            runtime: Handle::current(),
            state: watch::Sender::new(StoreNow::default()),
        }
    }

    /// What was last seen, as it changes.
    pub fn subscribe(&self) -> watch::Receiver<StoreNow> {
        self.state.subscribe()
    }

    /// What was last seen now.
    pub fn now(&self) -> StoreNow {
        self.state.borrow().clone()
    }

    /// Asks `backend` to describe itself again, in the background: for
    /// Monokulo, the store, this device's connection and the store's onion
    /// entry. What was last seen stays meanwhile. Nothing without a
    /// backend.
    pub fn read(&self, backend: Option<&Arc<dyn PaymentBackend>>) {
        let Some(backend) = backend.cloned() else {
            return;
        };
        self.state.send_modify(|now| {
            now.reading = true;
            now.problem = None;
        });
        let on_disconnected = self.on_disconnected.clone();
        let on_onion_api = self.on_onion_api.clone();
        let state = self.state.clone();
        self.runtime.spawn(async move {
            match backend.describe().await {
                Ok(info) => {
                    let monokulo = info.monokulo.clone();
                    let onion_api = monokulo.as_ref().and_then(|m| m.onion_api.clone());
                    if monokulo.is_some() {
                        // The connection keeps the onion entry before the read shows as done,
                        // so whoever sees `reading` drop also sees the connection that came
                        // with it.
                        on_onion_api(onion_api.as_deref());
                    }
                    let (store, device) = monokulo.map(|m| (m.store, m.device)).unzip();
                    state.send_replace(StoreNow {
                        backend: Some(info),
                        store,
                        device,
                        onion_api,
                        reading: false,
                        problem: None,
                    });
                }
                Err(e) => {
                    let problem = if e.answered() {
                        if e.disconnected() {
                            on_disconnected();
                        }
                        e.say()
                    } else {
                        format!("Couldn't reach the store: {e}")
                    };
                    state.send_modify(|now| {
                        now.reading = false;
                        now.problem = Some(problem);
                    });
                }
            }
        });
    }

    /// Forgets what was seen (the till was disconnected).
    pub fn forget(&self) {
        self.state.send_replace(StoreNow::default());
    }
}

#[cfg(test)]
mod tests {
    //! Ported from the Android app's `LiveStoreTest`: the store read live,
    //! never saved: what a read gives, and what a failed one leaves.

    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::Duration;

    use monokulo_client::{AppIdentity, Connection, MonokuloApi, SystemClock};
    use till_payments::{FakeBackend, MonokuloBackend, Sharing};
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate, matchers::any};

    use super::*;

    struct Answers(HashMap<&'static str, ResponseTemplate>);

    impl Respond for Answers {
        fn respond(&self, request: &Request) -> ResponseTemplate {
            self.0
                .get(request.url.path())
                .cloned()
                .unwrap_or_else(|| ResponseTemplate::new(404))
        }
    }

    struct Fixture {
        server: MockServer,
        api: MonokuloApi,
        live: LiveStore,
        disconnected: Arc<Mutex<u32>>,
        onion: Arc<Mutex<Vec<Option<String>>>>,
    }

    async fn fixture(answers: impl Respond + 'static) -> Fixture {
        let server = MockServer::builder().start().await;
        Mock::given(any())
            .respond_with(answers)
            .mount(&server)
            .await;
        let api = MonokuloApi::new(
            reqwest::Client::new(),
            AppIdentity {
                slug: "till".into(),
                name: "Till".into(),
                version: "0.1.0".into(),
                platform: "android".into(),
                device_name: "Pixel 8".into(),
            },
            Arc::new(SystemClock),
        );
        let disconnected = Arc::new(Mutex::new(0));
        let onion = Arc::new(Mutex::new(Vec::new()));
        let live = LiveStore::new(
            {
                let disconnected = disconnected.clone();
                Arc::new(move || *disconnected.lock().unwrap() += 1)
            },
            {
                let onion = onion.clone();
                Arc::new(move |at: Option<&str>| onion.lock().unwrap().push(at.map(String::from)))
            },
        );
        Fixture {
            server,
            api,
            live,
            disconnected,
            onion,
        }
    }

    impl Fixture {
        /// The store's Monokulo, as the engine makes it.
        fn backend(&self) -> Arc<dyn PaymentBackend> {
            Arc::new(MonokuloBackend::new(
                self.api.clone(),
                Connection::new("ak_1", format!("{}/api/v1", self.server.uri())),
            ))
        }

        async fn settled(&self) -> StoreNow {
            let mut rx = self.live.subscribe();
            tokio::time::timeout(Duration::from_secs(5), rx.wait_for(|now| !now.reading))
                .await
                .expect("the read settles within 5 s")
                .expect("the live store is still there")
                .clone()
        }
    }

    fn json(body: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_string(body)
    }

    #[tokio::test]
    async fn a_read_gives_the_store_its_wallet_and_this_device() {
        let f = fixture(Answers(HashMap::from([
            ("/api/v1", json(r#"{"_links":{"mk:store":{"href":"/api/v1/store"},"mk:app":{"href":"/api/v1/app"}}}"#)),
            (
                "/api/v1/store",
                json(r#"{"name":"Bakery","base_currency":"EUR","wallet":{"name":"Shop","network":"mainnet","address":"4Ad"}}"#),
            ),
            ("/api/v1/app", json(r#"{"app_id":"app_1","device_name":"Front counter"}"#)),
        ])))
        .await;
        f.live.read(Some(&f.backend()));
        let now = f.settled().await;
        assert_eq!(now.store.as_ref().map(|s| s.name.as_str()), Some("Bakery"));
        assert_eq!(
            now.store
                .as_ref()
                .and_then(|s| s.wallet.as_ref())
                .map(|w| w.address.as_str()),
            Some("4Ad")
        );
        assert_eq!(
            now.device.as_ref().map(|d| d.device_name.as_str()),
            Some("Front counter")
        );
        assert_eq!(now.problem, None);
        assert_eq!(now.onion_api, None);
        assert_eq!(*f.onion.lock().unwrap(), [None], "no onion address");
    }

    #[tokio::test]
    async fn a_read_finds_the_stores_onion_address_for_the_connection_to_keep() {
        let at = "http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/api/v1";
        let entry = format!(
            r#"{{"_links":{{"mk:store":{{"href":"/api/v1/store"}},"mk:app":{{"href":"/api/v1/app"}},"mk:onion-api":{{"href":"{at}"}}}}}}"#
        );
        let f = fixture(Answers(HashMap::from([
            ("/api/v1", json(&entry)),
            (
                "/api/v1/store",
                json(r#"{"name":"Bakery","base_currency":"EUR"}"#),
            ),
            (
                "/api/v1/app",
                json(r#"{"app_id":"app_1","device_name":"Front counter"}"#),
            ),
        ])))
        .await;
        f.live.read(Some(&f.backend()));
        assert_eq!(f.settled().await.onion_api.as_deref(), Some(at));
        assert_eq!(*f.onion.lock().unwrap(), [Some(at.to_owned())]);
    }

    #[tokio::test]
    async fn a_disconnected_till_hears_so_and_the_app_doesnt_fall_over() {
        struct Refuse;
        impl Respond for Refuse {
            fn respond(&self, _: &Request) -> ResponseTemplate {
                ResponseTemplate::new(401).set_body_raw(
                    r#"{"title":"No valid key","status":401,"detail":"This key was not accepted."}"#,
                    "application/problem+json",
                )
            }
        }
        let f = fixture(Refuse).await;
        f.live.read(Some(&f.backend()));
        let now = f.settled().await;
        assert_eq!(now.problem.as_deref(), Some("This key was not accepted."));
        assert_eq!(now.store, None);
        assert!(*f.disconnected.lock().unwrap() > 0);
    }

    /// A backend that isn't Monokulo describes itself, with no store and
    /// no onion entry for the connection to keep (core.md decision 238).
    #[tokio::test]
    async fn another_backend_describes_itself_with_no_monokulo_store() {
        let f = fixture(Answers(HashMap::new())).await;
        let backend: Arc<dyn PaymentBackend> = Arc::new(FakeBackend::new(Sharing::ThisTill));
        f.live.read(Some(&backend));
        let now = f.settled().await;
        let info = now.backend.expect("described");
        assert_eq!(info.sharing, Sharing::ThisTill);
        assert_eq!((now.store, now.device, now.onion_api), (None, None, None));
        assert!(f.onion.lock().unwrap().is_empty(), "nothing to keep");
    }
}
