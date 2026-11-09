//! The ledger in the app and in iOS's extension (docs/ledger.md §6, §10,
//! §11): how each process opens it with the platform's key, under the scope
//! of the store and the point of sale's account, takes the old kept shift
//! into it once, and holds the process awake around its writes.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use monokulo_client::PointOfSale;
use till_ledger::{Claim, Ledger, LedgerError, Opened, Process, Scope, SecretStore};
use till_sales::LedgerHost;

use crate::Platform;
use crate::store::{Kept, Model, keys};

/// Where claimed alerts go: the engine's task.
pub(crate) type Deliver = Arc<dyn Fn(Vec<Claim>) + Send + Sync>;

/// The platform's secret store as the ledger keeps its key in it.
pub(crate) struct PlatformSecrets(pub Arc<dyn Platform>);

impl SecretStore for PlatformSecrets {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        self.0
            .secret_get(name.to_owned())
            .map_err(|e| e.to_string())
    }

    fn put(&self, name: &str, value: Vec<u8>) -> Result<(), String> {
        self.0
            .secret_put(name.to_owned(), value)
            .map_err(|e| e.to_string())
    }
}

/// The scope of the till's rows (docs/ledger.md §4): the store it's
/// connected to, by its API's address, and the point of sale's account
/// (the Square location, the Clover merchant) when it has one.
pub(crate) fn scope_of(model: &Model) -> Scope {
    let store = model
        .connection
        .as_ref()
        .map(|c| c.api.as_str())
        .unwrap_or("");
    let account = match model.point_of_sale {
        Some(PointOfSale::Square) => model
            .location
            .as_ref()
            .map(|l| (PointOfSale::Square, l.id.as_str())),
        Some(PointOfSale::Clover) => model
            .clover_merchant
            .as_ref()
            .map(|m| (PointOfSale::Clover, m.id.as_str())),
        None => None,
    };
    Scope::of(store, account)
}

/// The ledger as this process opens it.
pub(crate) struct AppLedger {
    dir: PathBuf,
    platform: Arc<dyn Platform>,
    kept: Arc<Kept>,
    process: Process,
    deliver: OnceLock<Deliver>,
}

impl AppLedger {
    pub fn new(
        data_dir: &str,
        platform: Arc<dyn Platform>,
        kept: Arc<Kept>,
        process: Process,
    ) -> AppLedger {
        AppLedger {
            dir: PathBuf::from(data_dir),
            platform,
            kept,
            process,
            deliver: OnceLock::new(),
        }
    }

    /// Where this process's claimed alerts go (the app's engine).
    pub fn deliver_to(&self, deliver: Deliver) {
        let _ = self.deliver.set(deliver);
    }

    /// The old kept shift and alert record become the ledger's, once
    /// (docs/ledger.md §10), and are then deleted. Either process may get
    /// there first; the ledger lets only one import.
    fn import(&self, ledger: &mut Ledger) {
        let store = self.kept.store();
        let (Ok(shift), Ok(alerted)) = (store.get(keys::SHIFT_SALES), store.get(keys::ALERTED))
        else {
            // The store can't answer (before first unlock): next time.
            return;
        };
        let alerted: Vec<String> = alerted
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        if shift.is_none() && alerted.is_empty() {
            return;
        }
        match ledger.import_shift(shift.as_deref().unwrap_or("[]"), &alerted) {
            Ok(imported) => {
                self.kept.log(format!(
                    "till-core: the kept shift into the ledger: {imported:?}"
                ));
                for key in [keys::SHIFT_SALES, keys::ALERTED] {
                    if let Err(e) = store.put(key, None) {
                        self.kept.log(format!(
                            "till-core: couldn't delete {key} after the import: {e}"
                        ));
                    }
                }
            }
            Err(e) => self
                .kept
                .log(format!("till-core: couldn't import the kept shift: {e}")),
        }
    }
}

impl LedgerHost for AppLedger {
    fn scope(&self) -> Scope {
        self.kept.with(scope_of)
    }

    fn open(&self, clock: Arc<dyn till_ledger::Clock>) -> Result<Opened, LedgerError> {
        Ledger::open_with_secrets(
            &self.dir,
            &PlatformSecrets(self.platform.clone()),
            self.scope(),
            self.process,
            clock,
        )
    }

    /// The app takes the old kept shift in, and deletes the rows of any
    /// scope but the current one: the store or the point of sale's account
    /// changed while it wasn't running (the engine deletes the old scope's
    /// rows itself when it changes while running). The extension leaves
    /// that to the app.
    fn opened(&self, ledger: &mut Ledger) {
        self.import(ledger);
        if self.process == Process::App {
            match ledger.keep_only_scope() {
                Ok(0) => {}
                Ok(n) => self
                    .kept
                    .log(format!("till-core: deleted {n} old scopes' sales")),
                Err(e) => self
                    .kept
                    .log(format!("till-core: couldn't delete old scopes' sales: {e}")),
            }
        }
    }

    fn hold(&self) -> Box<dyn Send> {
        Box::new(crate::lifecycle::hold("ledger write"))
    }

    fn claimed(&self, claims: Vec<Claim>) {
        if let Some(deliver) = self.deliver.get() {
            deliver(claims);
        }
    }

    fn log(&self, line: String) {
        self.kept.log(line);
    }
}

#[cfg(test)]
mod tests;
