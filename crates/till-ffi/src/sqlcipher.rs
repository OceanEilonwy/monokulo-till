//! That the library carries SQLCipher, and that it works as the ledger will
//! use it (docs/ledger.md §4, §11), on every platform the library is built
//! for: the apps' tests call [`sqlcipher_check`] on the device or simulator,
//! Rust CI on Linux (docs/decisions/core.md 143).
//!
//! It opens a database in `dir` with a raw key and a salt the caller keeps
//! (the ledger's are kept in the platform's secret store), with its first 32
//! bytes left in plain text so iOS recognises a WAL database in a shared
//! container (§11, "0xdead10cc"), in WAL mode; writes a row; opens it again
//! and reads the row back. The ledger crate does the real thing; this only
//! proves the build.

use rusqlite::Connection;
use std::path::Path;

/// What went wrong in [`sqlcipher_check`].
#[derive(Debug, uniffi::Error)]
pub enum SqlcipherCheckError {
    Failed { reason: String },
}

impl std::error::Error for SqlcipherCheckError {}

impl std::fmt::Display for SqlcipherCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SqlcipherCheckError::Failed { reason } => f.write_str(reason),
        }
    }
}

impl From<rusqlite::Error> for SqlcipherCheckError {
    fn from(error: rusqlite::Error) -> Self {
        SqlcipherCheckError::Failed {
            reason: error.to_string(),
        }
    }
}

fn failed(reason: impl Into<String>) -> SqlcipherCheckError {
    SqlcipherCheckError::Failed {
        reason: reason.into(),
    }
}

/// The header SQLite writes at the start of every database file.
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// Opens `path` with `key` (32 bytes) and `salt` (16), as the ledger does.
fn open(path: &Path, key: &[u8; 32], salt: &[u8; 16]) -> Result<Connection, SqlcipherCheckError> {
    let db = Connection::open(path)?;
    // Raw key and salt as one blob literal: no key derivation, and the salt
    // isn't read from the file, whose header is plain text.
    db.execute_batch(&format!(
        "PRAGMA key = \"x'{}{}'\"; PRAGMA cipher_plaintext_header_size = 32;",
        hex(key),
        hex(salt)
    ))?;
    db.pragma_update(None, "journal_mode", "WAL")?;
    Ok(db)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Checks SQLCipher in a scratch database under `dir`, which it removes,
/// and returns `PRAGMA cipher_version` ("4.x.y community").
#[uniffi::export]
pub fn sqlcipher_check(dir: String) -> Result<String, SqlcipherCheckError> {
    let dir = Path::new(&dir);
    let path = dir.join("sqlcipher-check.db");
    let remove = || {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    };
    remove();
    let result = check(&path);
    remove();
    result
}

fn check(path: &Path) -> Result<String, SqlcipherCheckError> {
    let key = [7u8; 32];
    let salt = [9u8; 16];
    let version = {
        let db = open(path, &key, &salt)?;
        let version: String = db
            .query_row("PRAGMA cipher_version", [], |row| row.get(0))
            .map_err(|_| failed("no cipher_version: SQLite without SQLCipher got linked"))?;
        if version.is_empty() {
            return Err(failed(
                "no cipher_version: SQLite without SQLCipher got linked",
            ));
        }
        db.execute_batch("CREATE TABLE fact (id INTEGER PRIMARY KEY, body TEXT NOT NULL)")?;
        db.execute("INSERT INTO fact (body) VALUES (?1)", ["paid"])?;
        version
    };
    let db = open(path, &key, &salt)?;
    let body: String = db.query_row("SELECT body FROM fact", [], |row| row.get(0))?;
    if body != "paid" {
        return Err(failed(format!("read back {body:?}")));
    }
    drop(db);
    // The plain-text header iOS looks for; the rest is ciphertext.
    let bytes = std::fs::read(path).map_err(|e| failed(e.to_string()))?;
    if bytes.get(..16) != Some(&SQLITE_HEADER[..]) {
        return Err(failed("the first 32 bytes aren't plain text"));
    }
    if bytes.windows(4).any(|w| w == b"paid") {
        return Err(failed("the row is in plain text"));
    }
    // Another key opens nothing.
    let read = open(path, &[8u8; 32], &salt).and_then(|db| {
        Ok(db.query_row("SELECT count(*) FROM fact", [], |row| row.get::<_, i64>(0))?)
    });
    if read.is_ok() {
        return Err(failed("another key read the database"));
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    #[test]
    fn sqlcipher_is_linked_and_encrypts() {
        let dir = tempfile::tempdir().unwrap();
        let version = super::sqlcipher_check(dir.path().display().to_string()).unwrap();
        assert!(version.starts_with('4'), "cipher_version {version}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
