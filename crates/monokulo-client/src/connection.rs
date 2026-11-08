//! This device's connection to one store (Android: `core/Connection.kt`).

use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::tor;

/// This device's connection to one store: its key and where the store's API
/// is. The app keeps it (encrypted) until it is disconnected, and nothing
/// else: what the store is called and where its payments go are read from
/// the store each time ([`MonokuloApi::store`](crate::MonokuloApi::store)),
/// never kept (docs/decisions/android.md 50).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    /// The app's own key: `Authorization: Bearer ak_…`.
    pub app_key: String,
    /// The API's entry, absolute (`https://pay.example/api/v1`, or
    /// `https://example.com/monokulo/api/v1` for a Monokulo under a path):
    /// the `api` path `POST /apps/connect` answered, resolved against the
    /// link the code was redeemed at, so a till that connected over an onion
    /// address stays on it.
    pub api: String,
    /// The same entry on the store's onion address (`http://….onion/api/v1`),
    /// while it has one: the handshake's `onion_api`, or the entry's
    /// `mk:onion-api` for a till that connected before Monokulo gave it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onion_api: Option<String>,
    /// Reach the store over Tor, at [`Connection::onion_api`] (decision 55).
    #[serde(default, skip_serializing_if = "is_false")]
    pub tor: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

impl Connection {
    /// A connection off Tor, with no onion address.
    pub fn new(app_key: impl Into<String>, api: impl Into<String>) -> Connection {
        Connection {
            app_key: app_key.into(),
            api: api.into(),
            onion_api: None,
            tor: false,
        }
    }

    /// Where the app reads the entry: on the onion address when it goes over
    /// Tor.
    pub fn entry(&self) -> &str {
        match &self.onion_api {
            Some(onion) if self.tor => onion,
            _ => &self.api,
        }
    }

    /// Whether the app reaches the store over Tor, by choice or because it
    /// connected at an onion address.
    pub fn over_tor(&self) -> bool {
        tor::is_onion(self.entry())
    }

    /// The store's host, as the app shows it (`pay.example`), with its port
    /// when that isn't the scheme's own.
    pub fn host(&self) -> String {
        Url::parse(&self.api)
            .ok()
            .and_then(|url| {
                let host = url.host_str()?.to_owned();
                Some(match url.port() {
                    Some(port) => format!("{host}:{port}"),
                    None => host,
                })
            })
            .unwrap_or_else(|| self.api.clone())
    }

    /// Whether the connection is encrypted on the way: https, or Tor to an
    /// onion address.
    pub fn encrypted(&self) -> bool {
        self.over_tor() || self.api.starts_with("https://")
    }

    /// What the app writes to keep `connection`: JSON, leaving out an onion
    /// entry it hasn't got and a Tor it doesn't use, as the Android app does.
    pub fn save(connection: &Connection) -> String {
        serde_json::to_string(connection).expect("a connection is always JSON")
    }

    /// The connection `saved` holds, or `None` for none.
    pub fn restore(saved: &str) -> Option<Connection> {
        serde_json::from_str(saved).ok()
    }
}
