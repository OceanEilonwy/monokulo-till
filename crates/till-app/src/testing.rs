//! What the engine's tests share: a platform with secrets in memory.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::{Platform, SecretError};

/// Secrets in memory, as UTF-8 strings for reading in tests; told to fail,
/// every call fails.
#[derive(Default)]
pub struct FakePlatform {
    pub secrets: Mutex<BTreeMap<String, Vec<u8>>>,
    pub failing: AtomicBool,
}

impl FakePlatform {
    pub fn with(pairs: &[(&str, &str)]) -> Arc<FakePlatform> {
        let platform = FakePlatform::default();
        for (key, value) in pairs {
            platform.set(key, value);
        }
        Arc::new(platform)
    }

    pub fn set(&self, key: &str, value: &str) {
        self.secrets
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.as_bytes().to_vec());
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.secrets
            .lock()
            .unwrap()
            .get(key)
            .map(|v| String::from_utf8(v.clone()).unwrap())
    }

    /// Every key and its string.
    pub fn all(&self) -> BTreeMap<String, String> {
        self.secrets
            .lock()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), String::from_utf8(v.clone()).unwrap()))
            .collect()
    }

    pub fn fail(&self, failing: bool) {
        self.failing.store(failing, Ordering::SeqCst);
    }

    fn check(&self) -> Result<(), SecretError> {
        if self.failing.load(Ordering::SeqCst) {
            Err(SecretError::Unavailable {
                reason: "locked".into(),
            })
        } else {
            Ok(())
        }
    }
}

impl Platform for FakePlatform {
    fn secret_get(&self, key: String) -> Result<Option<Vec<u8>>, SecretError> {
        self.check()?;
        Ok(self.secrets.lock().unwrap().get(&key).cloned())
    }

    fn secret_put(&self, key: String, value: Vec<u8>) -> Result<(), SecretError> {
        self.check()?;
        self.secrets.lock().unwrap().insert(key, value);
        Ok(())
    }

    fn secret_remove(&self, key: String) -> Result<(), SecretError> {
        self.check()?;
        self.secrets.lock().unwrap().remove(&key);
        Ok(())
    }

    /// "$24.50" for USD, "24.50 EUR" for anything else.
    fn format_money(&self, minor: i64, currency: String) -> String {
        let amount = format!("{}.{:02}", minor / 100, (minor % 100).abs());
        if currency == "USD" {
            format!("${amount}")
        } else {
            format!("{amount} {currency}")
        }
    }
}
