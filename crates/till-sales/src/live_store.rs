//! The store as last read from Monokulo (Android: `StoreNow.kt`).

use std::sync::Arc;

use monokulo_client::{AppConnection, Connection, MonokuloApi, Store, problems};
use tokio::runtime::Handle;
use tokio::sync::watch;

/// The store and this device as last read from Monokulo, this session.
/// Held in memory and never saved (decision 50 in
/// docs/decisions/android.md): shown as "last seen" while the next read is
/// under way.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreNow {
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
    monokulo: MonokuloApi,
    on_disconnected: OnDisconnected,
    on_onion_api: OnOnionApi,
    runtime: Handle,
    state: watch::Sender<StoreNow>,
}

impl LiveStore {
    /// A live store reading through `monokulo`, its reads run on the tokio
    /// runtime it is made in.
    ///
    /// # Panics
    ///
    /// When made outside a tokio runtime.
    pub fn new(
        monokulo: MonokuloApi,
        on_disconnected: OnDisconnected,
        on_onion_api: OnOnionApi,
    ) -> LiveStore {
        LiveStore {
            monokulo,
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

    /// Reads the store, this device's connection and the store's onion
    /// entry again, in the background; what was last seen stays meanwhile.
    /// Nothing without a connection.
    pub fn read(&self, connection: Option<&Connection>) {
        let Some(connection) = connection.cloned() else {
            return;
        };
        self.state.send_modify(|now| {
            now.reading = true;
            now.problem = None;
        });
        let monokulo = self.monokulo.clone();
        let on_disconnected = self.on_disconnected.clone();
        let on_onion_api = self.on_onion_api.clone();
        let state = self.state.clone();
        self.runtime.spawn(async move {
            let read = tokio::try_join!(
                monokulo.store(&connection),
                monokulo.app(&connection),
                monokulo.onion_api(&connection),
            );
            match read {
                Ok((store, device, onion_api)) => {
                    // The connection keeps the onion entry before the read shows as done, so
                    // whoever sees `reading` drop also sees the connection that came with it.
                    on_onion_api(onion_api.as_deref());
                    state.send_replace(StoreNow {
                        store: Some(store),
                        device: Some(device),
                        onion_api,
                        reading: false,
                        problem: None,
                    });
                }
                Err(e) => {
                    let problem = if e.answered() {
                        if e.status() == 401 {
                            on_disconnected();
                        }
                        problems::say(&e)
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

    use monokulo_client::{AppIdentity, SystemClock};
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
            api,
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
            live,
            disconnected,
            onion,
        }
    }

    impl Fixture {
        fn connection(&self) -> Connection {
            Connection::new("ak_1", format!("{}/api/v1", self.server.uri()))
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
        f.live.read(Some(&f.connection()));
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
        f.live.read(Some(&f.connection()));
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
        f.live.read(Some(&f.connection()));
        let now = f.settled().await;
        assert_eq!(now.problem.as_deref(), Some("This key was not accepted."));
        assert_eq!(now.store, None);
        assert!(*f.disconnected.lock().unwrap() > 0);
    }
}
