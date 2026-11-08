//! The store and this device's connection as the store has them (Android:
//! `core/Store.kt`).

use serde::{Deserialize, Serialize};

/// The store, as `GET /api/v1/store` answers it now. Read each time it is
/// shown, never kept: the merchant can rename the store or move it to
/// another wallet at any time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Store {
    pub name: String,
    pub base_currency: String,
    /// Where its payments go; `None` for a store without a wallet.
    #[serde(default)]
    pub wallet: Option<Wallet>,
}

/// Where a store's payments go.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wallet {
    pub name: String,
    /// `mainnet`, `stagenet` or `testnet`.
    pub network: String,
    /// The primary address, whole.
    pub address: String,
}

impl Wallet {
    /// The address as the app shows it: its first and last 8 characters.
    pub fn address_short(&self) -> String {
        Store::shorten(&self.address)
    }
}

impl Store {
    /// `address`'s first and last 8 characters, or all of it when that's no
    /// shorter.
    pub fn shorten(address: &str) -> String {
        let chars: Vec<char> = address.chars().collect();
        if chars.len() <= 17 {
            return address.to_owned();
        }
        let head: String = chars[..8].iter().collect();
        let tail: String = chars[chars.len() - 8..].iter().collect();
        format!("{head}…{tail}")
    }
}

/// This device's connection as the store has it (`GET /api/v1/app`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConnection {
    pub app_id: String,
    /// What the merchant (or, when they gave none, the app) calls the device.
    pub device_name: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub connected_at: i64,
}
