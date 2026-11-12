//! A store's own Monokulo as a [`PaymentBackend`]: everything Till has
//! always done with Monokulo ([`MonokuloApi`]), behind the trait. The
//! client underneath keeps every rule it had: the event stream's contract
//! (core.md decision 8), idempotency keys, a 401 as the store having
//! disconnected the till, proofs of work, and Tor.

use async_trait::async_trait;
use futures_util::stream::BoxStream;
use futures_util::{StreamExt, future};
use monokulo_client::{Connection, MonokuloApi, Order, StreamEvent};
use till_types::UnixSeconds;

use crate::{
    BackendError, BackendEvent, BackendInfo, MonokuloStore, Payment, PaymentBackend, PaymentRef,
    PaymentRequest, Sharing,
};

/// A store's Monokulo, through `api`, as `connection` reaches it.
#[derive(Clone)]
pub struct MonokuloBackend {
    api: MonokuloApi,
    connection: Connection,
    reads_at_once: usize,
}

impl MonokuloBackend {
    /// The store `connection` reaches, asked through `api`. It reads
    /// orders again one at a time until told otherwise
    /// ([`MonokuloBackend::reading_at_once`]).
    pub fn new(api: MonokuloApi, connection: Connection) -> MonokuloBackend {
        MonokuloBackend {
            api,
            connection,
            reads_at_once: 1,
        }
    }

    /// The same backend, reading up to `n` orders again at once: for a
    /// look someone waits on (iOS's extension, core.md decision 204).
    pub fn reading_at_once(self, n: usize) -> MonokuloBackend {
        MonokuloBackend {
            reads_at_once: n.max(1),
            ..self
        }
    }

    /// The connection it asks through.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

#[async_trait]
impl PaymentBackend for MonokuloBackend {
    /// The store, this device as the store has it, and the store's onion
    /// entry, read at once. Every till sees every till's orders.
    async fn describe(&self) -> Result<BackendInfo, BackendError> {
        let (store, device, onion_api) = tokio::try_join!(
            self.api.store(&self.connection),
            self.api.app(&self.connection),
            self.api.onion_api(&self.connection),
        )?;
        Ok(BackendInfo {
            name: format!("Monokulo at {}", self.connection.host()),
            address: store
                .wallet
                .as_ref()
                .map(|wallet| wallet.address.clone())
                .unwrap_or_default(),
            currency: Some(store.base_currency.clone()),
            sharing: Sharing::Shared,
            monokulo: Some(MonokuloStore {
                store,
                device,
                onion_api,
            }),
            warnings: Vec::new(),
        })
    }

    async fn create(&self, req: PaymentRequest) -> Result<Payment, BackendError> {
        let order = self
            .api
            .create_order(
                &self.connection,
                &req.amount,
                req.reference.as_deref(),
                &req.key,
            )
            .await?;
        Ok(payment_of_order(&order))
    }

    /// Each order read again at its `self` link, in the order asked.
    async fn refresh(&self, refs: &[PaymentRef]) -> Result<Vec<Payment>, BackendError> {
        let urls: Vec<String> = refs.iter().map(|r| r.as_str().to_owned()).collect();
        let (api, connection) = (self.api.clone(), self.connection.clone());
        let reads: Vec<_> = futures_util::stream::iter(urls)
            .map(move |url| {
                let (api, connection) = (api.clone(), connection.clone());
                async move { api.order(&connection, &url).await }
            })
            .buffered(self.reads_at_once)
            .collect()
            .await;
        let mut read = Vec::with_capacity(reads.len());
        let mut failed = None;
        for order in reads {
            match order {
                Ok(order) => read.push(payment_of_order(&order)),
                Err(e) => failed = Some(e),
            }
        }
        match failed {
            Some(e) if read.is_empty() => Err(e.into()),
            _ => Ok(read),
        }
    }

    async fn renew(&self, renew: &str, expires_at: UnixSeconds) -> Result<Payment, BackendError> {
        let order = self
            .api
            .renew_order(&self.connection, renew, expires_at.0)
            .await?;
        Ok(payment_of_order(&order))
    }

    /// The store's order stream (`mk:orders-events`): each order as it
    /// changes, a new confirmation included, as a changed payment.
    fn events(&self) -> BoxStream<'static, Result<BackendEvent, BackendError>> {
        self.api
            .events(&self.connection)
            .filter_map(|event| {
                future::ready(match event {
                    Ok(StreamEvent::Ready) => Some(Ok(BackendEvent::Ready)),
                    Ok(StreamEvent::Lagged) => Some(Ok(BackendEvent::Lagged)),
                    Ok(StreamEvent::Order(order)) => {
                        Some(Ok(BackendEvent::Changed(payment_of_order(&order))))
                    }
                    Err(e) => Some(Err(e.into())),
                })
            })
            .boxed()
    }

    fn sharing(&self) -> Sharing {
        Sharing::Shared
    }

    /// Monokulo watches every order itself.
    fn focus(&self, _: Option<&PaymentRef>) {}

    async fn disconnect(&self) -> Result<(), BackendError> {
        Ok(self.api.disconnect(&self.connection).await?)
    }
}

/// An order as Monokulo answered it (made, read again or renewed), as a
/// payment.
pub fn payment_of_order(order: &Order) -> Payment {
    Payment {
        reference: PaymentRef::new(order.url.as_str()),
        order_id: order.order_id.clone(),
        address: order.address.clone(),
        xmr_piconero: order.piconero,
        expires_at: UnixSeconds(order.expires_at),
        phase: order.phase(),
        confirmations: order.confirmations,
        required: order.confirmations_required,
        is_final: order.is_final,
        received: order.received,
        overpaid: order.status.overpaid(),
        price: None,
        amount: order.amount.clone(),
        currency: order.currency.clone(),
        receipt: order.merchant_order_id.clone(),
        device_name: order.device_name.clone(),
        renew: order.renew.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use monokulo_client::{AppIdentity, FixedClock, SalePhase};
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn backend(server: &MockServer) -> MonokuloBackend {
        let api = MonokuloApi::new(
            reqwest::Client::new(),
            AppIdentity {
                slug: "till".into(),
                name: "Till".into(),
                version: "0.1.0".into(),
                platform: "android".into(),
                device_name: "Pixel".into(),
            },
            Arc::new(FixedClock::at(1_761_000_000)),
        );
        MonokuloBackend::new(
            api,
            Connection::new("ak_1", format!("{}/api/v1", server.uri())),
        )
    }

    #[tokio::test]
    async fn a_monokulo_store_describes_itself_as_shared_with_what_settings_shows() {
        let server = MockServer::start().await;
        let json = |body: &str| ResponseTemplate::new(200).set_body_string(body.to_owned());
        Mock::given(path("/api/v1"))
            .respond_with(json(
                r#"{"_links":{"mk:store":{"href":"/api/v1/store"},"mk:app":{"href":"/api/v1/app"}}}"#,
            ))
            .mount(&server)
            .await;
        Mock::given(path("/api/v1/store"))
            .respond_with(json(
                r#"{"name":"Bakery","base_currency":"EUR","wallet":{"name":"Shop","network":"mainnet","address":"4Ad"}}"#,
            ))
            .mount(&server)
            .await;
        Mock::given(path("/api/v1/app"))
            .respond_with(json(r#"{"app_id":"app_1","device_name":"Front counter"}"#))
            .mount(&server)
            .await;
        let info = backend(&server).describe().await.unwrap();
        assert_eq!(info.sharing, Sharing::Shared);
        assert_eq!(info.address, "4Ad");
        assert_eq!(info.currency.as_deref(), Some("EUR"));
        assert!(
            info.name.starts_with("Monokulo at 127.0.0.1:"),
            "{}",
            info.name
        );
        let monokulo = info.monokulo.unwrap();
        assert_eq!(monokulo.store.name, "Bakery");
        assert_eq!(monokulo.device.device_name, "Front counter");
        assert_eq!(monokulo.onion_api, None);
    }

    #[tokio::test]
    async fn the_order_stream_says_ready_then_each_order_as_a_changed_payment_counted() {
        let server = MockServer::start().await;
        Mock::given(path("/api/v1"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"_links":{"mk:orders-events":{"href":"/api/v1/orders/events"}}}"#,
            ))
            .mount(&server)
            .await;
        let body = concat!(
            "event: ready\ndata: {}\n\n",
            "event: sale.paid\ndata: {}\n\n",
            "event: order\ndata: {\"order_id\":\"ord_2\",\"status\":\"confirming\",\"is_final\":false,",
            "\"address\":\"8Cy\",\"xmr_amount_piconero\":30000000000,\"amount_received_piconero\":30000000000,",
            "\"amount\":\"5.00\",\"currency\":\"USD\",\"device_name\":\"Front iPad\",",
            "\"confirmations\":4,\"confirmations_required\":10,\"created_at\":1761311000,",
            "\"expires_at\":1761312000,\"_links\":{\"self\":{\"href\":\"/api/v1/orders/ord_2\"}}}\n\n",
        );
        Mock::given(path("/api/v1/orders/events"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
            .mount(&server)
            .await;
        let events: Vec<_> = backend(&server).events().collect().await;
        assert!(matches!(events[0], Ok(BackendEvent::Ready)), "{events:?}");
        let Ok(BackendEvent::Changed(confirming)) = &events[1] else {
            panic!("a changed payment next: {events:?}");
        };
        assert_eq!(confirming.phase, SalePhase::Confirming);
        assert_eq!(
            (
                confirming.confirmations,
                confirming.required,
                confirming.received
            ),
            (4, 10, 30_000_000_000)
        );
        assert_eq!(confirming.device_name.as_deref(), Some("Front iPad"));
        assert_eq!(confirming.price, None, "Monokulo converts the price");
        assert_eq!(
            confirming.reference.as_str(),
            format!("{}/api/v1/orders/ord_2", server.uri())
        );
        assert!(
            matches!(&events[2], Err(e) if e.answered() && !e.disconnected()),
            "then the stream ends, and says so: {events:?}"
        );
    }
}
