//! The SQLCipher database (docs/ledger.md §4, §11): its key, how a
//! connection is opened, and the schema.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::{LedgerError, UnixMillis};

/// The schema this build writes. A database with a larger one is refused.
pub const SCHEMA_VERSION: i64 = 1;

/// The projections' shape and rules. When `meta.projection_version`
/// differs, every projection is rebuilt by replaying the journal (§13).
/// 2: a sale kept from Monokulo alone is folded into the point-of-sale
/// sale whose order it is. 3: one order per sale, renewed by New code,
/// with endings per quote (core.md decisions 167 to 173). 4: a sale keeps
/// every order it had, New code on a wallet payment making its next
/// (decision 241).
pub const PROJECTION_VERSION: i64 = 4;

/// The database's file name in the engine's data directory.
pub const FILE_NAME: &str = "till.db";

/// The secret the key and salt are kept under, through the platform's
/// secret store (Keychain in the shared access group, Android's
/// Keystore-wrapped store).
pub const KEY_SECRET: &str = "ledger.key";

/// The database's key and the salt kept beside it (§11): with the first 32
/// bytes of the file left in plain text (so iOS recognises a WAL database
/// and doesn't kill a suspended app for its lock, 0xdead10cc), SQLCipher
/// can't keep the salt in the header, so the caller supplies it.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyMaterial {
    pub key: [u8; 32],
    pub salt: [u8; 16],
}

impl std::fmt::Debug for KeyMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeyMaterial(..)")
    }
}

impl KeyMaterial {
    /// A new random key and salt, from the system's generator.
    pub fn generate() -> Result<KeyMaterial, LedgerError> {
        let mut bytes = [0u8; 48];
        getrandom::fill(&mut bytes).map_err(|e| LedgerError::Random(e.to_string()))?;
        Ok(KeyMaterial::from_secret(&bytes).expect("48 bytes"))
    }

    /// The 48 bytes kept as the secret: key, then salt.
    pub fn to_secret(&self) -> Vec<u8> {
        let mut out = self.key.to_vec();
        out.extend_from_slice(&self.salt);
        out
    }

    /// The key and salt a secret holds, or `None` for one of another size.
    pub fn from_secret(bytes: &[u8]) -> Option<KeyMaterial> {
        if bytes.len() != 48 {
            return None;
        }
        let mut key = [0u8; 32];
        let mut salt = [0u8; 16];
        key.copy_from_slice(&bytes[..32]);
        salt.copy_from_slice(&bytes[32..]);
        Some(KeyMaterial { key, salt })
    }

    /// SQLCipher's raw key with its salt: `x'<key><salt>'`, so no key
    /// derivation runs at open.
    fn pragma(&self) -> String {
        let hex: String = self
            .to_secret()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("\"x'{hex}'\"")
    }
}

/// The platform's secret store, as the ledger needs it: `Platform::secret_get`
/// and `secret_put` in till-app. `Ok(None)` only when there is no such
/// secret; an error when the store can't tell (before first unlock).
pub trait SecretStore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, String>;
    fn put(&self, name: &str, value: Vec<u8>) -> Result<(), String>;
}

/// Opens (creating it if need be) the database at `path` with `key`, set
/// up as §4 says, and checks it's SQLCipher and the key is right.
pub(crate) fn connect(path: &Path, key: &KeyMaterial) -> Result<Connection, LedgerError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let cipher: Option<String> = conn
        .query_row("PRAGMA cipher_version", [], |r| r.get(0))
        .optional()?;
    if cipher.is_none_or(|v| v.is_empty()) {
        return Err(LedgerError::NoCipher);
    }
    conn.execute_batch(&format!(
        "PRAGMA key = {};
         PRAGMA cipher_plaintext_header_size = 32;",
        key.pragma()
    ))?;
    // The first read checks the key.
    match conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::NotADatabase =>
        {
            return Err(LedgerError::WrongKey);
        }
        Err(e) => return Err(e.into()),
    }
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = -1024;
         PRAGMA temp_store = MEMORY;",
    )?;
    migrate(&conn)?;
    Ok(conn)
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS journal (
  seq      INTEGER PRIMARY KEY AUTOINCREMENT,
  scope    TEXT NOT NULL,
  at       INTEGER NOT NULL,
  sale_key TEXT,
  kind     TEXT NOT NULL,
  hash     BLOB NOT NULL,
  body     BLOB NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS journal_dedupe ON journal(scope, hash);
CREATE INDEX IF NOT EXISTS journal_sale ON journal(sale_key, seq);
CREATE TABLE IF NOT EXISTS sales (
  scope TEXT NOT NULL, sale_key TEXT NOT NULL,
  shared TEXT NOT NULL,
  marks  TEXT NOT NULL,
  device_id TEXT, rung_at INTEGER NOT NULL, applied_seq INTEGER NOT NULL,
  PRIMARY KEY (scope, sale_key));
CREATE INDEX IF NOT EXISTS sales_rung ON sales(scope, rung_at);
CREATE TABLE IF NOT EXISTS orders (order_id TEXT NOT NULL, scope TEXT NOT NULL,
  sale_key TEXT NOT NULL, attempt INTEGER NOT NULL, PRIMARY KEY (scope, order_id));
CREATE TABLE IF NOT EXISTS pending_events (scope TEXT NOT NULL, order_id TEXT NOT NULL,
  seq INTEGER NOT NULL, PRIMARY KEY (scope, order_id, seq));
CREATE TABLE IF NOT EXISTS alerts (scope TEXT NOT NULL, sale_key TEXT NOT NULL,
  kind TEXT NOT NULL, state TEXT NOT NULL, claimed_at INTEGER, by TEXT,
  PRIMARY KEY (scope, sale_key, kind));
CREATE TABLE IF NOT EXISTS settings (name TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS cursors (scope TEXT NOT NULL, source TEXT NOT NULL,
  value TEXT NOT NULL, PRIMARY KEY (scope, source));
CREATE TABLE IF NOT EXISTS meta (name TEXT PRIMARY KEY, value TEXT NOT NULL);
";

fn migrate(conn: &Connection) -> Result<(), LedgerError> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        conn.execute_batch(SCHEMA)?;
        let version: Option<i64> = meta_get(conn, "schema_version")?.and_then(|v| v.parse().ok());
        match version {
            None => meta_set(conn, "schema_version", &SCHEMA_VERSION.to_string())?,
            Some(v) if v > SCHEMA_VERSION => return Err(LedgerError::NewerSchema(v)),
            Some(_) => {}
        }
        Ok(())
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT")?,
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    }
    Ok(())
}

pub(crate) fn meta_get(conn: &Connection, name: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE name = ?1", [name], |r| {
        r.get(0)
    })
    .optional()
}

pub(crate) fn meta_set(conn: &Connection, name: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO meta(name, value) VALUES (?1, ?2)
         ON CONFLICT(name) DO UPDATE SET value = excluded.value",
        [name, value],
    )?;
    Ok(())
}

pub(crate) fn meta_delete(conn: &Connection, name: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM meta WHERE name = ?1", [name])?;
    Ok(())
}

/// The database's files: the database, its WAL and shared memory.
pub(crate) fn files(path: &Path) -> [PathBuf; 3] {
    let with = |suffix: &str| {
        let mut p = path.as_os_str().to_owned();
        p.push(suffix);
        PathBuf::from(p)
    };
    [path.to_path_buf(), with("-wal"), with("-shm")]
}

/// Moves a database that can't be opened aside (`till.db.lost-<ms>`), so a
/// new one starts in its place and the old one is kept for inspection.
pub(crate) fn move_aside(path: &Path, now_ms: UnixMillis) -> Result<(), LedgerError> {
    for file in files(path) {
        if file.exists() {
            let mut lost = file.as_os_str().to_owned();
            lost.push(format!(".lost-{}", now_ms.0));
            std::fs::rename(&file, PathBuf::from(lost))?;
        }
    }
    Ok(())
}
