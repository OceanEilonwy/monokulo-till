//! What the engine's tests share: a platform with secrets in memory, a
//! listener that records, and a push platform that records.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{
    Distributors, Effect, EngineListener, Platform, PushPlatform, PushProblem, SecretError, Update,
};

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

/// What the engine sent, in order.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum Heard {
    Update(Update),
    Effect(Effect),
}

pub struct Recorder(Mutex<Sender<Heard>>, pub Mutex<Vec<String>>);

impl Recorder {
    pub fn new() -> (Arc<Recorder>, Receiver<Heard>) {
        let (tx, rx) = channel();
        (Arc::new(Recorder(Mutex::new(tx), Mutex::default())), rx)
    }
}

impl EngineListener for Recorder {
    fn on_update(&self, update: Update) {
        let _ = self.0.lock().unwrap().send(Heard::Update(update));
    }
    fn on_effect(&self, effect: Effect) {
        let _ = self.0.lock().unwrap().send(Heard::Effect(effect));
    }
    fn on_log(&self, message: String) {
        self.1.lock().unwrap().push(message);
    }
}

/// Waits up to 10 s for the first thing heard that `pick` takes.
pub fn wait_for<T>(rx: &Receiver<Heard>, mut pick: impl FnMut(&Heard) -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok(heard) => {
                if let Some(t) = pick(&heard) {
                    return t;
                }
            }
            Err(_) => panic!("nothing heard that was wanted within 10 s"),
        }
    }
}

/// A push platform that records what it was asked.
#[derive(Default)]
pub struct FakePush {
    pub asked: Mutex<Vec<String>>,
}

impl PushPlatform for FakePush {
    fn subscribe(&self, vapid_public_key: String) {
        self.asked
            .lock()
            .unwrap()
            .push(format!("subscribe {vapid_public_key}"));
    }
    fn unsubscribe(&self) {
        self.asked.lock().unwrap().push("unsubscribe".into());
    }
    fn distributors(&self) -> Result<Distributors, PushProblem> {
        Ok(Distributors::Found {
            name: "apns".into(),
        })
    }
    fn save_distributor(&self, name: String) {
        self.asked.lock().unwrap().push(format!("save {name}"));
    }
}
