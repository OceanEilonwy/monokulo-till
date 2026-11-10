//! The ledger (docs/ledger.md §6): every write is one fact, merged into one
//! sale's row under SQLite's write lock, journalled only when it changed
//! something, with the effects of the change claimed in the same
//! transaction.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jiff::tz::TimeZone;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::db::{self, KeyMaterial, SecretStore, meta_delete, meta_get, meta_set};
use crate::effects::{DiffContext, Effect, diff, notification_id};
use crate::facts::{About, AlertKind, AlertState, Fact, Process, SnapshotAlert, SnapshotState};
use crate::ids::{OrderId, SaleKey, Scope};
use crate::model::{LocalMarks, SharedSale};
use crate::shift::{self, ShiftSummary, ShiftWindow};
use crate::view::{SaleView, ViewSettings, setting, view};
use crate::{Clock, LedgerError, UnixMillis};

/// A claim stuck in `pending` this long (its process died) is delivered
/// again by whichever process looks next.
pub const REDELIVER_AFTER_MS: i64 = 30_000;

/// Compaction runs at most this often, unless forced (§12).
pub const COMPACT_EVERY_MS: i64 = 60 * 60 * 1000;

/// Days of sales kept when the setting doesn't say (§12).
pub const DEFAULT_RETENTION_DAYS: i64 = 30;

/// What one [`Ledger::record`] did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recorded {
    /// A projection changed, so the fact was journalled.
    pub changed: bool,
    /// Every effect the change produced that wasn't already claimed or
    /// posted, whoever delivers it.
    pub effects: Vec<Effect>,
    /// The effects this process claimed and must now deliver, then report
    /// with [`Ledger::posted`].
    pub effects_claimed: Vec<Claim>,
}

/// An alert this process claimed: deliver it, then call
/// [`Ledger::posted`].
#[derive(Clone, Debug, PartialEq)]
pub struct Claim {
    pub sale: SaleKey,
    pub kind: AlertKind,
    /// `sale_key + kind`: a repeat replaces rather than adds.
    pub notification_id: String,
    pub view: SaleView,
    /// Another process claimed it more than 30 s ago and never posted it.
    pub redelivery: bool,
}

/// Which waiting alerts [`Ledger::claim_due`] claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimPolicy {
    /// Every one (the app).
    All,
    /// Only the newest new-sale alert (the extension, which can present
    /// one notification per push).
    NewestNewSale,
}

/// One sale's row in `sales`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SaleRow {
    pub sale: SaleKey,
    pub shared: SharedSale,
    pub marks: LocalMarks,
    pub rung_at: UnixMillis,
    pub applied_seq: i64,
}

impl SaleRow {
    fn new(sale: SaleKey, at: UnixMillis) -> SaleRow {
        SaleRow {
            sale,
            shared: SharedSale::default(),
            marks: LocalMarks::default(),
            rung_at: at,
            applied_seq: 0,
        }
    }

    fn same(&self, other: &SaleRow) -> bool {
        self.shared == other.shared && self.marks == other.marks && self.rung_at == other.rung_at
    }

    pub fn view(&self, settings: &ViewSettings) -> SaleView {
        view(
            &self.sale,
            &self.shared,
            &self.marks,
            settings,
            self.rung_at,
        )
    }
}

/// How a ledger was opened by [`Ledger::open_with_secrets`].
pub struct Opened {
    pub ledger: Ledger,
    /// No database was there: a new one, with a new key.
    pub created: bool,
    /// The key was missing or wrong for the database there: it was moved
    /// aside and a new one started, learning without alerting.
    pub recovered: bool,
}

/// What [`Ledger::import_shift`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Imported {
    /// The shift's sales, and the alerts marked posted.
    Done { sales: u32, alerts: u32 },
    /// Another opener (or an earlier run) already imported.
    AlreadyImported,
}

/// How [`apply`] treats a fact.
enum Mode<'a> {
    /// A new fact: journal it, and work out effects.
    Live(&'a Live<'a>),
    /// A migrated fact: journal it, no effects.
    Import,
    /// A journal row being replayed: never journalled again, no effects.
    Replay { seq: i64 },
}

struct Live<'a> {
    process: Process,
    learning: bool,
    cx: DiffContext<'a>,
}

#[derive(Default)]
struct Applied {
    changed: bool,
    effects: Vec<Effect>,
    claimed: Vec<Claim>,
}

/// The ledger, over one connection to the database. Each process (the
/// app, the extension) opens its own; they share the file.
pub struct Ledger {
    conn: Connection,
    path: PathBuf,
    scope: Scope,
    process: Process,
    clock: Arc<dyn Clock>,
    open_sale: Option<SaleKey>,
    time_zone: TimeZone,
}

impl Ledger {
    /// Opens the database at `path` with `key`, creating it if need be,
    /// and rebuilds the projections when their version changed (§13).
    pub fn open(
        path: &Path,
        key: &KeyMaterial,
        scope: Scope,
        process: Process,
        clock: Arc<dyn Clock>,
    ) -> Result<Ledger, LedgerError> {
        let conn = db::connect(path, key)?;
        let mut ledger = Ledger {
            conn,
            path: path.to_path_buf(),
            scope,
            process,
            clock,
            open_sale: None,
            time_zone: TimeZone::system(),
        };
        let version: Option<i64> =
            meta_get(&ledger.conn, "projection_version")?.and_then(|v| v.parse().ok());
        if version != Some(db::PROJECTION_VERSION) {
            ledger.rebuild()?;
        }
        Ok(ledger)
    }

    /// Opens `till.db` in `dir` with the key kept in `secrets` (§11):
    /// - a key opens the database;
    /// - no key and no database: a new key and database;
    /// - no key (or one that doesn't open it) with a database: the database
    ///   is moved aside and a new one starts, learning without alerting;
    /// - an error from the store (before first unlock, say) fails, and
    ///   changes nothing.
    pub fn open_with_secrets(
        dir: &Path,
        secrets: &dyn SecretStore,
        scope: Scope,
        process: Process,
        clock: Arc<dyn Clock>,
    ) -> Result<Opened, LedgerError> {
        let path = dir.join(db::FILE_NAME);
        let kept = secrets
            .get(db::KEY_SECRET)
            .map_err(LedgerError::KeyUnavailable)?;
        // The app and the extension may both find no key: one at a time.
        let lock_path = dir.join(format!("{}.lock", db::FILE_NAME));
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)?;
        lock.lock()?;
        let kept = match kept {
            Some(k) => Some(k),
            None => secrets
                .get(db::KEY_SECRET)
                .map_err(LedgerError::KeyUnavailable)?,
        };
        let exists = path.exists();
        let key = kept.as_deref().and_then(KeyMaterial::from_secret);
        if let Some(key) = &key {
            match Ledger::open(&path, key, scope.clone(), process, clock.clone()) {
                Ok(mut ledger) => {
                    if !exists {
                        ledger.set_learning(true)?;
                    }
                    return Ok(Opened {
                        ledger,
                        created: !exists,
                        recovered: false,
                    });
                }
                Err(LedgerError::WrongKey) => {}
                Err(e) => return Err(e),
            }
        }
        let fresh = KeyMaterial::generate()?;
        secrets
            .put(db::KEY_SECRET, fresh.to_secret())
            .map_err(LedgerError::KeyUnavailable)?;
        let recovered = path.exists();
        if recovered {
            db::move_aside(&path, clock.now_ms())?;
        }
        let mut ledger = Ledger::open(&path, &fresh, scope, process, clock)?;
        ledger.set_learning(true)?;
        drop(lock);
        Ok(Opened {
            ledger,
            created: !recovered,
            recovered,
        })
    }

    /// Closes the connection (going to the background, the extension
    /// finishing: §11).
    pub fn close(self) -> Result<(), LedgerError> {
        self.conn.close().map_err(|(_, e)| e.into())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    pub fn process(&self) -> Process {
        self.process
    }

    /// Changes the scope new facts are recorded under and reads come from.
    pub fn set_scope(&mut self, scope: Scope) {
        self.scope = scope;
    }

    /// The time zone the trading day is worked out in (the system's).
    pub fn set_time_zone(&mut self, tz: TimeZone) {
        self.time_zone = tz;
    }

    /// The sale the cashier has open on screen: it gets the paid moment
    /// even when it's another counter's.
    pub fn set_open_sale(&mut self, sale: Option<SaleKey>) {
        self.open_sale = sale;
    }

    /// Records one fact (§6): merged, journalled if it changed something,
    /// with its effects claimed, in one `BEGIN IMMEDIATE`.
    pub fn record(&mut self, fact: Fact) -> Result<Recorded, LedgerError> {
        let now = self.clock.now_ms();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let settings = read_view_settings(&tx)?;
        let foreground = read_foreground(&tx)? > now;
        let learning = meta_get(&tx, "learning")?.is_some();
        let live = Live {
            process: self.process,
            learning,
            cx: DiffContext {
                settings,
                foreground,
                open_sale: self.open_sale.as_ref(),
            },
        };
        let applied = apply(&tx, &self.scope, &fact, now, Mode::Live(&live))?;
        if applied.changed {
            tx.commit()?;
        }
        Ok(Recorded {
            changed: applied.changed,
            effects: applied.effects,
            effects_claimed: applied.claimed,
        })
    }

    /// Claims alerts waiting for a process (left by the extension, or by a
    /// process that died more than 30 s after claiming), as `policy` says.
    /// While learning, they're marked posted and nothing is returned.
    pub fn claim_due(&mut self, policy: ClaimPolicy) -> Result<Vec<Claim>, LedgerError> {
        let now = self.clock.now_ms();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let learning = meta_get(&tx, "learning")?.is_some();
        let settings = read_view_settings(&tx)?;
        let mut found: Vec<(SaleRow, AlertKind, bool)> = Vec::new();
        {
            let mut stmt = tx.prepare(
                "SELECT sale_key, kind, state FROM alerts WHERE scope = ?1
                 AND (state = ?3 OR (state = ?4 AND claimed_at <= ?2))",
            )?;
            let rows = stmt.query_map(
                params![
                    self.scope.0,
                    now.plus(-REDELIVER_AFTER_MS),
                    AlertState::Due,
                    AlertState::Pending
                ],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, AlertState>(2)?,
                    ))
                },
            )?;
            for row in rows {
                let (key, kind, state) = row?;
                let (Some(key), Some(kind)) = (SaleKey::parse(&key), AlertKind::from_wire(&kind))
                else {
                    continue;
                };
                if let Some(sale) = load_row(&tx, &self.scope, &key)? {
                    found.push((sale, kind, state == AlertState::Pending));
                }
            }
        }
        // A new sale no longer live (paid, closed, ended) before anyone
        // showed it is told already: posted, not delivered.
        let (stale, mut due): (Vec<_>, Vec<_>) = found
            .into_iter()
            .partition(|(row, kind, _)| *kind == AlertKind::NewSale && !row.view(&settings).live);
        for (row, kind, _) in stale {
            post_silently(&tx, &self.scope, &row.sale, kind, now)?;
        }
        if policy == ClaimPolicy::NewestNewSale {
            due.retain(|(_, kind, _)| *kind == AlertKind::NewSale);
            due.sort_by(|a, b| b.0.rung_at.cmp(&a.0.rung_at).then(a.0.sale.cmp(&b.0.sale)));
            due.truncate(1);
        }
        let mut claims = Vec::new();
        for (row, kind, redelivery) in due {
            if learning {
                post_silently(&tx, &self.scope, &row.sale, kind, now)?;
                continue;
            }
            let claim = claim(&tx, &self.scope, self.process, &row, kind, now, &settings)?;
            claims.push(Claim {
                redelivery,
                ..claim
            });
        }
        tx.commit()?;
        Ok(claims)
    }

    /// The claim was delivered: `alerts.state = 'posted'`, journalled as
    /// `AlertPosted`.
    pub fn posted(&mut self, claim: &Claim) -> Result<(), LedgerError> {
        self.record(Fact::AlertPosted {
            sale: claim.sale.clone(),
            kind: claim.kind,
        })?;
        Ok(())
    }

    /// Learning without alerting (§6): facts are recorded and their effects
    /// marked posted without delivering. Set on a new or recovered database
    /// and by an import; the first look after that clears it.
    pub fn learning(&self) -> Result<bool, LedgerError> {
        Ok(meta_get(&self.conn, "learning")?.is_some())
    }

    pub fn set_learning(&mut self, on: bool) -> Result<(), LedgerError> {
        if on {
            meta_set(&self.conn, "learning", "1")?;
        } else {
            meta_delete(&self.conn, "learning")?;
        }
        Ok(())
    }

    /// The settings table.
    pub fn settings(&self) -> Result<BTreeMap<String, String>, LedgerError> {
        read_settings(&self.conn)
    }

    pub fn view_settings(&self) -> Result<ViewSettings, LedgerError> {
        read_view_settings(&self.conn)
    }

    /// The app's heartbeat hasn't run out.
    pub fn foreground(&self) -> Result<bool, LedgerError> {
        Ok(read_foreground(&self.conn)? > self.clock.now_ms())
    }

    /// One sale's row.
    pub fn row(&self, sale: &SaleKey) -> Result<Option<SaleRow>, LedgerError> {
        load_row(&self.conn, &self.scope, sale)
    }

    /// One sale's view.
    pub fn sale(&self, sale: &SaleKey) -> Result<Option<SaleView>, LedgerError> {
        let settings = self.view_settings()?;
        Ok(self.row(sale)?.map(|r| r.view(&settings)))
    }

    /// The sale an order belongs to.
    pub fn order(&self, order: &OrderId) -> Result<Option<SaleKey>, LedgerError> {
        order_sale(&self.conn, &self.scope, order)
    }

    /// Every sale in the scope, newest first.
    pub fn rows(&self) -> Result<Vec<SaleRow>, LedgerError> {
        let mut stmt = self.conn.prepare(
            "SELECT sale_key, shared, marks, rung_at, applied_seq FROM sales
             WHERE scope = ?1 ORDER BY rung_at DESC, sale_key",
        )?;
        let rows = stmt.query_map([&self.scope.0], row_of)?;
        let mut out = Vec::new();
        for row in rows {
            if let Some(row) = row? {
                out.push(row);
            }
        }
        Ok(out)
    }

    /// Every sale's view, newest first. A cash sale this till doesn't take
    /// in Monero (cash as Monero is off) is left out: with the setting off,
    /// cash sales are ignored altogether, not offered (core.md decision 221).
    /// [`Ledger::sale`] still answers for one by its key.
    pub fn sales(&self) -> Result<Vec<SaleView>, LedgerError> {
        let settings = self.view_settings()?;
        Ok(self
            .rows()?
            .iter()
            .map(|r| r.view(&settings))
            .filter(|v| !v.ignored(&settings))
            .collect())
    }

    /// The shift's window now (§9).
    pub fn shift_window(&self) -> Result<ShiftWindow, LedgerError> {
        let now = self.clock.now_ms();
        let minutes = read_settings(&self.conn)?
            .get(setting::TRADING_DAY_START)
            .and_then(|m| m.parse().ok())
            .unwrap_or(shift::DEFAULT_DAY_START_MINUTES);
        let day = shift::trading_day_start(now, &self.time_zone, minutes);
        let mark = |name| -> Result<Option<UnixMillis>, LedgerError> {
            Ok(meta_get(&self.conn, name)?
                .and_then(|v| v.parse().ok())
                .map(UnixMillis))
        };
        let (started, stopped) = (mark("shift.started")?, mark("shift.stopped")?);
        Ok(ShiftWindow::of(started, stopped, day))
    }

    /// The shift: live sales and those rung up in its window, with totals.
    pub fn shift(&self) -> Result<ShiftSummary, LedgerError> {
        Ok(shift::summarise(self.sales()?, self.shift_window()?))
    }

    /// A source's cursor: a hint for fetching less, never truth.
    pub fn cursor(&self, source: &str) -> Result<Option<String>, LedgerError> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM cursors WHERE scope = ?1 AND source = ?2",
                [&self.scope.0, source],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn set_cursor(&mut self, source: &str, value: &str) -> Result<(), LedgerError> {
        self.conn.execute(
            "INSERT INTO cursors(scope, source, value) VALUES (?1, ?2, ?3)
             ON CONFLICT(scope, source) DO UPDATE SET value = excluded.value",
            [&self.scope.0, source, value],
        )?;
        Ok(())
    }

    /// Deletes every row of `scope` in one transaction (signing out,
    /// forgetting the store, changing the POS: §4).
    pub fn delete_scope(&mut self, scope: &Scope) -> Result<(), LedgerError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for table in [
            "journal",
            "sales",
            "orders",
            "pending_events",
            "alerts",
            "cursors",
        ] {
            tx.execute(&format!("DELETE FROM {table} WHERE scope = ?1"), [&scope.0])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Deletes the rows of every scope but this ledger's, each as
    /// [`Ledger::delete_scope`] does: the store or the point of sale's
    /// account changed while the app wasn't running. Returns how many
    /// scopes went.
    pub fn keep_only_scope(&mut self) -> Result<usize, LedgerError> {
        let others: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT scope FROM journal WHERE scope != ?1
                 UNION SELECT scope FROM sales WHERE scope != ?1
                 UNION SELECT scope FROM orders WHERE scope != ?1
                 UNION SELECT scope FROM pending_events WHERE scope != ?1
                 UNION SELECT scope FROM alerts WHERE scope != ?1
                 UNION SELECT scope FROM cursors WHERE scope != ?1",
            )?;
            let rows = stmt.query_map([&self.scope.0], |r| r.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        for scope in &others {
            self.delete_scope(&Scope(scope.clone()))?;
        }
        Ok(others.len())
    }

    /// Compaction and retention, when due: what the engine calls at shift
    /// start and on its timer.
    pub fn maintain(&mut self) -> Result<(), LedgerError> {
        self.compact(false)?;
        self.retain()?;
        Ok(())
    }

    /// Compaction (§12): each ended sale rung up before the shift's window
    /// has its journal rows replaced by one `Snapshot`. Runs at most
    /// hourly unless `force`. Old `Foreground` heartbeats are dropped too,
    /// keeping the newest. Returns how many sales were compacted.
    pub fn compact(&mut self, force: bool) -> Result<usize, LedgerError> {
        let now = self.clock.now_ms();
        let last: Option<UnixMillis> = meta_get(&self.conn, "last_compacted")?
            .and_then(|v| v.parse().ok())
            .map(UnixMillis);
        if !force && last.is_some_and(|l| now < l.plus(COMPACT_EVERY_MS) && now >= l) {
            return Ok(0);
        }
        let window = self.shift_window()?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let candidates: Vec<SaleRow> = {
            let mut stmt = tx.prepare(
                "SELECT sale_key, shared, marks, rung_at, applied_seq FROM sales
                 WHERE scope = ?1 AND rung_at < ?2",
            )?;
            let rows = stmt.query_map(params![self.scope.0, window.from], row_of)?;
            let mut out = Vec::new();
            for row in rows {
                if let Some(row) = row? {
                    out.push(row);
                }
            }
            out
        };
        let mut compacted = 0;
        for row in candidates {
            if !(row.shared.ended() || row.marks.ended()) {
                continue;
            }
            let key = row.sale.text();
            let (count, kinds): (i64, String) = tx.query_row(
                "SELECT count(*), coalesce(group_concat(kind), '') FROM journal
                 WHERE scope = ?1 AND sale_key = ?2",
                [&self.scope.0, &key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if count == 1 && kinds == "Snapshot" {
                continue;
            }
            let alerts = read_alerts(&tx, &self.scope, &row.sale)?;
            tx.execute(
                "DELETE FROM journal WHERE scope = ?1 AND sale_key = ?2",
                [&self.scope.0, &key],
            )?;
            let fact = Fact::Snapshot(Box::new(SnapshotState {
                sale: row.sale.clone(),
                shared: row.shared.clone(),
                marks: row.marks.clone(),
                rung_at: row.rung_at,
                applied_seq: row.applied_seq,
                alerts,
            }));
            let seq = next_seq(&tx)?;
            insert_journal(&tx, &self.scope, seq, now, Some(&key), &fact)?;
            tx.execute(
                "UPDATE sales SET applied_seq = ?1 WHERE scope = ?2 AND sale_key = ?3",
                params![seq, self.scope.0, key],
            )?;
            compacted += 1;
        }
        tx.execute(
            "DELETE FROM journal WHERE kind = 'Foreground'
             AND seq < (SELECT max(seq) FROM journal WHERE kind = 'Foreground')",
            [],
        )?;
        meta_set(&tx, "last_compacted", &now.0.to_string())?;
        tx.commit()?;
        Ok(compacted)
    }

    /// Retention (§12): sales rung up more than the retention setting's
    /// days ago (30 by default) are deleted with their journal rows and
    /// projections, in every scope, as are events for orders that never
    /// arrived. Returns how many sales went.
    pub fn retain(&mut self) -> Result<usize, LedgerError> {
        let now = self.clock.now_ms();
        let days = read_settings(&self.conn)?
            .get(setting::RETENTION_DAYS)
            .and_then(|d| d.parse::<i64>().ok())
            .filter(|d| *d > 0)
            .unwrap_or(DEFAULT_RETENTION_DAYS);
        let cutoff = now.plus(-days * 24 * 60 * 60 * 1000);
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: Vec<(String, String)> = {
            let mut stmt = tx.prepare("SELECT scope, sale_key FROM sales WHERE rung_at < ?1")?;
            let rows = stmt.query_map([cutoff], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        for (scope, key) in &old {
            for table in ["journal", "sales", "orders", "alerts"] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE scope = ?1 AND sale_key = ?2"),
                    [scope, key],
                )?;
            }
        }
        tx.execute(
            "DELETE FROM pending_events WHERE seq IN
             (SELECT seq FROM journal WHERE sale_key IS NULL AND kind = 'OrderObserved' AND at < ?1)",
            [cutoff],
        )?;
        tx.execute(
            "DELETE FROM journal WHERE sale_key IS NULL AND kind = 'OrderObserved' AND at < ?1",
            [cutoff],
        )?;
        tx.commit()?;
        Ok(old.len())
    }

    /// Rebuilds every projection, `alerts` included, by replaying the
    /// journal in `seq` order in one transaction (§13). Replay never
    /// delivers effects. Alerts still waiting for a process (`due`) aren't
    /// journalled and are kept.
    pub fn rebuild(&mut self) -> Result<(), LedgerError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        rebuild_in(&tx)?;
        tx.commit()?;
        Ok(())
    }

    /// Imports the kept shift JSON (§10): its sales become shared facts,
    /// their marks local `Mark`s, and `alerts_posted` (the order ids under
    /// `alerts.posted`) `AlertPosted`, in one `BEGIN IMMEDIATE` guarded by
    /// `meta.imported`, so the app and the extension can't both run it.
    /// The caller deletes the JSON and the old record afterwards.
    pub fn import_shift(
        &mut self,
        json: &str,
        alerts_posted: &[String],
    ) -> Result<Imported, LedgerError> {
        let now = self.clock.now_ms();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if meta_get(&tx, "imported")?.is_some() {
            return Ok(Imported::AlreadyImported);
        }
        let sales = crate::import::facts_of_shift(json)?;
        let mut count = 0u32;
        for sale in &sales {
            for (fact, at) in &sale.facts {
                apply(&tx, &self.scope, fact, at.unwrap_or(now), Mode::Import)?;
            }
            count += 1;
        }
        let mut alerts = 0u32;
        for order in alerts_posted {
            if let Some(sale) = order_sale(&tx, &self.scope, &OrderId::new(order.as_str()))? {
                let fact = Fact::AlertPosted {
                    sale,
                    kind: AlertKind::NewSale,
                };
                if apply(&tx, &self.scope, &fact, now, Mode::Import)?.changed {
                    alerts += 1;
                }
            }
        }
        apply(
            &tx,
            &self.scope,
            &Fact::Imported {
                from: "shift".into(),
                count,
            },
            now,
            Mode::Import,
        )?;
        meta_set(&tx, "imported", "1")?;
        meta_set(&tx, "learning", "1")?;
        tx.commit()?;
        Ok(Imported::Done {
            sales: count,
            alerts,
        })
    }

    /// The journal's rows, oldest first: `(seq, scope, at, sale_key, kind)`.
    pub fn journal(&self) -> Result<Vec<JournalRow>, LedgerError> {
        let mut stmt = self
            .conn
            .prepare("SELECT seq, scope, at, sale_key, kind, body FROM journal ORDER BY seq")?;
        let rows = stmt.query_map([], |r| {
            Ok(JournalRow {
                seq: r.get(0)?,
                scope: r.get(1)?,
                at: r.get(2)?,
                sale_key: r.get(3)?,
                kind: r.get(4)?,
                body: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The `alerts` row for one sale and kind: its state (`due`, `pending`,
    /// `posted`), when it was claimed and by whom.
    pub fn alert(&self, sale: &SaleKey, kind: AlertKind) -> Result<Option<AlertRow>, LedgerError> {
        get_alert(&self.conn, &self.scope, sale, kind)
    }

    /// Every projection's rows, as text, for comparing two databases (a
    /// replay against the incremental projections). `due` alerts aren't
    /// journalled and are left out.
    pub fn dump_projections(&self) -> Result<Vec<String>, LedgerError> {
        let mut out = Vec::new();
        let alerts = format!(
            "SELECT scope, sale_key, kind, state, claimed_at, by FROM alerts WHERE state != '{}' ORDER BY scope, sale_key, kind",
            AlertState::Due.as_str()
        );
        for (name, sql) in [
            (
                "sales",
                "SELECT scope, sale_key, shared, marks, device_id, rung_at, applied_seq FROM sales ORDER BY scope, sale_key",
            ),
            (
                "orders",
                "SELECT scope, order_id, sale_key, attempt FROM orders ORDER BY scope, order_id",
            ),
            (
                "pending",
                "SELECT scope, order_id, seq FROM pending_events ORDER BY scope, order_id, seq",
            ),
            ("alerts", alerts.as_str()),
            ("settings", "SELECT name, value FROM settings ORDER BY name"),
            (
                "meta",
                "SELECT name, value FROM meta WHERE name IN ('foreground_until', 'shift.started', 'shift.stopped', 'imported_from') ORDER BY name",
            ),
        ] {
            let mut stmt = self.conn.prepare(sql)?;
            let n = stmt.column_count();
            let rows = stmt.query_map([], |r| {
                let mut cols = Vec::with_capacity(n);
                for i in 0..n {
                    let v: rusqlite::types::Value = r.get(i)?;
                    cols.push(format!("{v:?}"));
                }
                Ok(cols.join("|"))
            })?;
            for row in rows {
                out.push(format!("{name}: {}", row?));
            }
        }
        Ok(out)
    }
}

/// One `alerts` row: its state (`due`, `pending` or `posted`), when it
/// was claimed and by which process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertRow {
    pub state: AlertState,
    pub claimed_at: Option<UnixMillis>,
    pub by: Option<Process>,
}

/// One journal row, as [`Ledger::journal`] lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalRow {
    pub seq: i64,
    pub scope: String,
    pub at: UnixMillis,
    pub sale_key: Option<String>,
    pub kind: String,
    pub body: Vec<u8>,
}

impl JournalRow {
    /// The fact it holds, or `None` for one this build doesn't know.
    pub fn fact(&self) -> Option<Fact> {
        Fact::decode(&self.kind, &self.body)
    }
}

fn rebuild_in(conn: &Connection) -> Result<(), LedgerError> {
    conn.execute_batch(
        "DELETE FROM sales; DELETE FROM orders; DELETE FROM pending_events; DELETE FROM settings;
         DELETE FROM meta WHERE name IN ('foreground_until', 'shift.started', 'shift.stopped', 'imported_from');",
    )?;
    conn.execute(
        "DELETE FROM alerts WHERE state != ?1",
        params![AlertState::Due],
    )?;
    let rows: Vec<(i64, String, UnixMillis, String, Vec<u8>)> = {
        let mut stmt =
            conn.prepare("SELECT seq, scope, at, kind, body FROM journal ORDER BY seq")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?;
        rows.collect::<Result<_, _>>()?
    };
    for (seq, scope, at, kind, body) in rows {
        let Some(fact) = Fact::decode(&kind, &body) else {
            continue;
        };
        apply(conn, &Scope(scope), &fact, at, Mode::Replay { seq })?;
    }
    meta_set(
        conn,
        "projection_version",
        &db::PROJECTION_VERSION.to_string(),
    )?;
    Ok(())
}

fn row_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<Option<SaleRow>> {
    let key: String = r.get(0)?;
    let shared: String = r.get(1)?;
    let marks: String = r.get(2)?;
    let (Some(sale), Ok(shared), Ok(marks)) = (
        SaleKey::parse(&key),
        serde_json::from_str(&shared),
        serde_json::from_str(&marks),
    ) else {
        return Ok(None);
    };
    Ok(Some(SaleRow {
        sale,
        shared,
        marks,
        rung_at: r.get(3)?,
        applied_seq: r.get(4)?,
    }))
}

fn load_row(
    conn: &Connection,
    scope: &Scope,
    sale: &SaleKey,
) -> Result<Option<SaleRow>, LedgerError> {
    Ok(conn
        .query_row(
            "SELECT sale_key, shared, marks, rung_at, applied_seq FROM sales
             WHERE scope = ?1 AND sale_key = ?2",
            [&scope.0, &sale.text()],
            row_of,
        )
        .optional()?
        .flatten())
}

fn save_row(conn: &Connection, scope: &Scope, row: &SaleRow) -> Result<(), LedgerError> {
    let device = row
        .shared
        .pos
        .payment
        .as_ref()
        .and_then(|p| p.device_id.as_ref())
        .map(|d| d.0.clone());
    conn.execute(
        "INSERT INTO sales(scope, sale_key, shared, marks, device_id, rung_at, applied_seq)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(scope, sale_key) DO UPDATE SET shared = excluded.shared,
           marks = excluded.marks, device_id = excluded.device_id,
           rung_at = excluded.rung_at, applied_seq = excluded.applied_seq",
        params![
            scope.0,
            row.sale.text(),
            serde_json::to_string(&row.shared)?,
            serde_json::to_string(&row.marks)?,
            device,
            row.rung_at,
            row.applied_seq,
        ],
    )?;
    Ok(())
}

/// The sale `order` belongs to. (The table's `attempt` column, from
/// before core.md decision 167, is always 1 now.)
fn order_sale(
    conn: &Connection,
    scope: &Scope,
    order: &OrderId,
) -> Result<Option<SaleKey>, LedgerError> {
    let row: Option<String> = conn
        .query_row(
            "SELECT sale_key FROM orders WHERE scope = ?1 AND order_id = ?2",
            [&scope.0, &order.0],
            |r| r.get(0),
        )
        .optional()?;
    Ok(row.and_then(|key| SaleKey::parse(&key)))
}

fn next_seq(conn: &Connection) -> Result<i64, LedgerError> {
    let seq: i64 = conn.query_row(
        "SELECT max(coalesce((SELECT seq FROM sqlite_sequence WHERE name = 'journal'), 0),
                    coalesce((SELECT max(seq) FROM journal), 0))",
        [],
        |r| r.get(0),
    )?;
    Ok(seq + 1)
}

/// Inserts a journal row; `false` when identical content is already there.
fn insert_journal(
    conn: &Connection,
    scope: &Scope,
    seq: i64,
    at: UnixMillis,
    sale_key: Option<&str>,
    fact: &Fact,
) -> Result<bool, LedgerError> {
    let n = conn.execute(
        "INSERT INTO journal(seq, scope, at, sale_key, kind, hash, body)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(scope, hash) DO NOTHING",
        params![
            seq,
            scope.0,
            at,
            sale_key,
            fact.kind(),
            fact.hash(sale_key),
            fact.body()
        ],
    )?;
    Ok(n == 1)
}

fn journal_alert_fact(
    conn: &Connection,
    scope: &Scope,
    at: UnixMillis,
    fact: &Fact,
) -> Result<(), LedgerError> {
    let sale = match fact.about() {
        About::Sale(s) => Some(s.text()),
        _ => None,
    };
    let seq = next_seq(conn)?;
    insert_journal(conn, scope, seq, at, sale.as_deref(), fact)?;
    Ok(())
}

fn read_settings(conn: &Connection) -> Result<BTreeMap<String, String>, LedgerError> {
    let mut stmt = conn.prepare("SELECT name, value FROM settings")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn read_view_settings(conn: &Connection) -> Result<ViewSettings, LedgerError> {
    Ok(ViewSettings::from_table(&read_settings(conn)?))
}

fn read_foreground(conn: &Connection) -> Result<UnixMillis, LedgerError> {
    Ok(UnixMillis(
        meta_get(conn, "foreground_until")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    ))
}

fn get_alert(
    conn: &Connection,
    scope: &Scope,
    sale: &SaleKey,
    kind: AlertKind,
) -> Result<Option<AlertRow>, LedgerError> {
    Ok(conn
        .query_row(
            "SELECT state, claimed_at, by FROM alerts WHERE scope = ?1 AND sale_key = ?2 AND kind = ?3",
            params![scope.0, sale.text(), kind.wire()],
            |r| {
                Ok(AlertRow {
                    state: r.get(0)?,
                    claimed_at: r.get(1)?,
                    by: r.get(2)?,
                })
            },
        )
        .optional()?)
}

fn set_alert(
    conn: &Connection,
    scope: &Scope,
    sale: &SaleKey,
    kind: AlertKind,
    state: AlertState,
    claimed_at: Option<UnixMillis>,
    by: Option<Process>,
) -> Result<(), LedgerError> {
    conn.execute(
        "INSERT INTO alerts(scope, sale_key, kind, state, claimed_at, by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(scope, sale_key, kind) DO UPDATE SET state = excluded.state,
           claimed_at = coalesce(excluded.claimed_at, alerts.claimed_at),
           by = coalesce(excluded.by, alerts.by)",
        params![scope.0, sale.text(), kind.wire(), state, claimed_at, by],
    )?;
    Ok(())
}

fn read_alerts(
    conn: &Connection,
    scope: &Scope,
    sale: &SaleKey,
) -> Result<Vec<SnapshotAlert>, LedgerError> {
    let mut stmt = conn.prepare(
        "SELECT kind, state, claimed_at, by FROM alerts
         WHERE scope = ?1 AND sale_key = ?2 AND state != ?3 ORDER BY kind",
    )?;
    let rows = stmt.query_map(params![scope.0, sale.text(), AlertState::Due], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, AlertState>(1)?,
            r.get::<_, Option<UnixMillis>>(2)?,
            r.get::<_, Option<Process>>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (kind, state, claimed_at, by) = row?;
        if let Some(kind) = AlertKind::from_wire(&kind) {
            out.push(SnapshotAlert {
                kind,
                state,
                claimed_at,
                by,
            });
        }
    }
    Ok(out)
}

/// Marks `kind` posted for `sale` without delivering it, journalled.
fn post_silently(
    conn: &Connection,
    scope: &Scope,
    sale: &SaleKey,
    kind: AlertKind,
    at: UnixMillis,
) -> Result<(), LedgerError> {
    set_alert(conn, scope, sale, kind, AlertState::Posted, None, None)?;
    journal_alert_fact(
        conn,
        scope,
        at,
        &Fact::AlertPosted {
            sale: sale.clone(),
            kind,
        },
    )
}

/// Claims `kind` for `row`: `pending`, by this process, now, journalled.
fn claim(
    conn: &Connection,
    scope: &Scope,
    process: Process,
    row: &SaleRow,
    kind: AlertKind,
    now: UnixMillis,
    settings: &ViewSettings,
) -> Result<Claim, LedgerError> {
    let id = notification_id(&row.sale, kind);
    set_alert(
        conn,
        scope,
        &row.sale,
        kind,
        AlertState::Pending,
        Some(now),
        Some(process),
    )?;
    journal_alert_fact(
        conn,
        scope,
        now,
        &Fact::AlertClaimed {
            sale: row.sale.clone(),
            kind,
            notification_id: id.clone(),
            by: process,
            at: now,
        },
    )?;
    Ok(Claim {
        sale: row.sale.clone(),
        kind,
        notification_id: id,
        view: row.view(settings),
        redelivery: false,
    })
}

/// Folds one fact into the projections (§6). The caller holds the write
/// lock and commits when `changed`.
fn apply(
    conn: &Connection,
    scope: &Scope,
    fact: &Fact,
    at: UnixMillis,
    mode: Mode<'_>,
) -> Result<Applied, LedgerError> {
    let (seq, journal) = match mode {
        Mode::Replay { seq } => (seq, false),
        _ => (next_seq(conn)?, true),
    };
    let unchanged = Ok(Applied::default());
    let sale = match fact.about() {
        About::Nothing => {
            if !apply_global(conn, fact)? {
                return unchanged;
            }
            if journal && !insert_journal(conn, scope, seq, at, None, fact)? {
                return Err(LedgerError::Conflict);
            }
            return Ok(Applied {
                changed: true,
                ..Applied::default()
            });
        }
        About::Order(order) => match order_sale(conn, scope, &order)? {
            Some(sale) => sale,
            None => {
                // Kept until its OrderCreated arrives (§3).
                if journal && !insert_journal(conn, scope, seq, at, None, fact)? {
                    return unchanged;
                }
                conn.execute(
                    "INSERT OR IGNORE INTO pending_events(scope, order_id, seq) VALUES (?1, ?2, ?3)",
                    params![scope.0, order.0, seq],
                )?;
                return Ok(Applied {
                    changed: true,
                    ..Applied::default()
                });
            }
        },
        About::Sale(sale) => sale,
    };

    // Alerts are their own projection.
    match fact {
        Fact::AlertClaimed {
            kind,
            by,
            at: claimed_at,
            ..
        } => {
            let old = get_alert(conn, scope, &sale, *kind)?;
            let new = AlertRow {
                state: AlertState::Pending,
                claimed_at: Some(*claimed_at),
                by: Some(*by),
            };
            if old.as_ref() == Some(&new)
                || old.as_ref().is_some_and(|o| o.state == AlertState::Posted)
            {
                return unchanged;
            }
            if journal && !insert_journal(conn, scope, seq, at, Some(&sale.text()), fact)? {
                return unchanged;
            }
            set_alert(
                conn,
                scope,
                &sale,
                *kind,
                AlertState::Pending,
                Some(*claimed_at),
                Some(*by),
            )?;
            return Ok(Applied {
                changed: true,
                ..Applied::default()
            });
        }
        Fact::AlertPosted { kind, .. } => {
            let old = get_alert(conn, scope, &sale, *kind)?;
            if old.as_ref().is_some_and(|o| o.state == AlertState::Posted) {
                return unchanged;
            }
            if journal && !insert_journal(conn, scope, seq, at, Some(&sale.text()), fact)? {
                return unchanged;
            }
            set_alert(conn, scope, &sale, *kind, AlertState::Posted, None, None)?;
            return Ok(Applied {
                changed: true,
                ..Applied::default()
            });
        }
        _ => {}
    }

    let old = load_row(conn, scope, &sale)?;
    let mut row = old
        .clone()
        .unwrap_or_else(|| SaleRow::new(sale.clone(), at));
    let mut saw = true;
    match fact {
        Fact::PosPaymentSeen(p) => row.shared.see_payment(p),
        Fact::PosOrderRead(r) => row.shared.read_order(r),
        Fact::PosRefundSeen(r) => row.shared.see_refund(r),
        Fact::OrderCreated(o) => row.shared.create_order(o),
        Fact::OrderObserved(o) => row.shared.observe(o),
        Fact::ActionAsked { action, at, .. } => row.marks.action(*action, None, *at),
        Fact::ActionDone {
            action,
            outcome,
            at,
            ..
        } => row.marks.action(*action, Some(outcome), *at),
        Fact::Mark { mark, at, .. } => row.marks.mark(*mark, *at, seq),
        Fact::Snapshot(s) => {
            row.shared.join(&s.shared);
            row.marks = s.marks.clone();
            row.rung_at = s.rung_at;
            saw = false;
        }
        _ => return unchanged,
    }
    if saw {
        row.marks.saw(&row.shared, at);
        if let Some(rung) = row.shared.rung_at() {
            row.rung_at = rung;
        }
    }
    if old.as_ref().is_some_and(|o| o.same(&row)) {
        return unchanged;
    }
    let key = sale.text();
    if journal && !insert_journal(conn, scope, seq, at, Some(&key), fact)? {
        return unchanged;
    }
    row.applied_seq = seq;

    // An OrderCreated links its order to the sale, and applies the events
    // that waited for it.
    let mut created: Vec<OrderId> = Vec::new();
    match fact {
        Fact::OrderCreated(o) => created.push(o.order_id.clone()),
        Fact::Snapshot(s) => {
            if let Some(o) = &s.shared.order {
                created.push(o.order.order_id.clone());
            }
            for alert in &s.alerts {
                set_alert(
                    conn,
                    scope,
                    &sale,
                    alert.kind,
                    alert.state,
                    alert.claimed_at,
                    alert.by,
                )?;
            }
        }
        _ => {}
    }
    // Sales kept from Monokulo alone that this one takes in: what they
    // showed already isn't alerted again.
    let mut folded: Vec<SaleRow> = Vec::new();
    for order in created {
        if let Some(kept) = fold_kept(conn, scope, &sale, &order, &mut row, at)? {
            folded.push(kept);
        }
        conn.execute(
            "INSERT INTO orders(order_id, scope, sale_key, attempt) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(scope, order_id) DO UPDATE SET sale_key = excluded.sale_key, attempt = 1",
            params![order.0, scope.0, key],
        )?;
        let waiting: Vec<i64> = {
            let mut stmt = conn.prepare(
                "SELECT seq FROM pending_events WHERE scope = ?1 AND order_id = ?2 ORDER BY seq",
            )?;
            let rows = stmt.query_map([&scope.0, &order.0], |r| r.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        for pending in waiting {
            let body: Option<(String, Vec<u8>)> = conn
                .query_row(
                    "SELECT kind, body FROM journal WHERE seq = ?1",
                    [pending],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if let Some(Fact::OrderObserved(o)) = body.and_then(|(k, b)| Fact::decode(&k, &b)) {
                row.shared.observe(&o);
                row.marks.saw(&row.shared, at);
            }
            conn.execute(
                "UPDATE journal SET sale_key = ?1 WHERE seq = ?2",
                params![key, pending],
            )?;
        }
        conn.execute(
            "DELETE FROM pending_events WHERE scope = ?1 AND order_id = ?2",
            [&scope.0, &order.0],
        )?;
    }
    save_row(conn, scope, &row)?;

    let mut applied = Applied {
        changed: true,
        ..Applied::default()
    };
    if let Mode::Live(live) = mode {
        let settings = &live.cx.settings;
        let old_view = old.as_ref().map(|o| o.view(settings));
        let new_view = row.view(settings);
        let shown: Vec<SaleView> = folded.iter().map(|k| k.view(settings)).collect();
        for effect in diff(old_view.as_ref(), &new_view, &live.cx) {
            if shown_already(&shown, effect.kind) {
                continue;
            }
            if get_alert(conn, scope, &sale, effect.kind)?.is_some() {
                continue;
            }
            applied.effects.push(effect.clone());
            if live.learning {
                post_silently(conn, scope, &sale, effect.kind, at)?;
            } else if live.process == Process::App {
                applied.claimed.push(claim(
                    conn,
                    scope,
                    live.process,
                    &row,
                    effect.kind,
                    at,
                    settings,
                )?);
            } else {
                // The extension leaves it for `claim_due`: the newest one is
                // what its notification shows; the app's next look takes
                // the rest.
                set_alert(conn, scope, &sale, effect.kind, AlertState::Due, None, None)?;
            }
        }
    }
    Ok(applied)
}

/// Whether a sale kept from Monokulo alone, folded in, already showed what
/// an alert of `kind` would say: paid, or ended. A new sale's alert is
/// left to the alerts it had, which the fold moved over: one kept quietly
/// as another counter's still alerts when this counter's till finds it.
fn shown_already(shown: &[SaleView], kind: AlertKind) -> bool {
    shown.iter().any(|v| match kind {
        AlertKind::Paid | AlertKind::PaidMoment => {
            v.payment.is_some_and(crate::model::PaymentPhase::is_paid)
        }
        AlertKind::Ended => matches!(
            v.payment,
            Some(
                crate::model::PaymentPhase::Expired
                    | crate::model::PaymentPhase::Cancelled
                    | crate::model::PaymentPhase::DoubleSpend
            )
        ),
        _ => false,
    })
}

/// When `order`, which point-of-sale sale `sale` has just taken as its
/// order, belongs to a sale kept from Monokulo alone (an older build's
/// shift, imported before this till found the sale at its point of sale),
/// folds that row into `row`: what Monokulo said about the order, the
/// local marks and the alerts; its journal rows go with `sale` (so
/// compaction and retention take them with it) and its row is deleted.
/// Runs in the caller's transaction, on replay too, so a rebuild folds
/// alike. The row folded in, if any.
fn fold_kept(
    conn: &Connection,
    scope: &Scope,
    sale: &SaleKey,
    order: &OrderId,
    row: &mut SaleRow,
    at: UnixMillis,
) -> Result<Option<SaleRow>, LedgerError> {
    if !matches!(sale, SaleKey::Pos { .. }) {
        return Ok(None);
    }
    let kept_key = match order_sale(conn, scope, order)? {
        Some(key @ SaleKey::Order(_)) if key != *sale => key,
        _ => return Ok(None),
    };
    let kept = load_row(conn, scope, &kept_key)?;
    if let Some(kept) = &kept {
        row.shared.take_observations(&kept.shared, order);
        row.marks.join(&kept.marks);
        row.marks.saw(&row.shared, at);
    }
    let alerts: Vec<(String, AlertState, Option<UnixMillis>, Option<Process>)> = {
        let mut stmt = conn.prepare(
            "SELECT kind, state, claimed_at, by FROM alerts WHERE scope = ?1 AND sale_key = ?2",
        )?;
        let rows = stmt.query_map([&scope.0, &kept_key.text()], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
        rows.collect::<Result<_, _>>()?
    };
    for (kind, state, claimed_at, by) in alerts {
        let Some(kind) = AlertKind::from_wire(&kind) else {
            continue;
        };
        match get_alert(conn, scope, sale, kind)? {
            None => set_alert(conn, scope, sale, kind, state, claimed_at, by)?,
            Some(mine) if mine.state != AlertState::Posted && state == AlertState::Posted => {
                set_alert(conn, scope, sale, kind, AlertState::Posted, None, None)?;
            }
            Some(_) => {}
        }
    }
    let key = sale.text();
    let kept_text = kept_key.text();
    for sql in [
        "DELETE FROM alerts WHERE scope = ?1 AND sale_key = ?2",
        "DELETE FROM sales WHERE scope = ?1 AND sale_key = ?2",
    ] {
        conn.execute(sql, [&scope.0, &kept_text])?;
    }
    conn.execute(
        "UPDATE journal SET sale_key = ?1 WHERE scope = ?2 AND sale_key = ?3",
        params![key, scope.0, kept_text],
    )?;
    Ok(kept)
}

/// Folds a fact about no sale into `settings` or `meta`; `true` when it
/// changed something.
fn apply_global(conn: &Connection, fact: &Fact) -> Result<bool, LedgerError> {
    let set_meta = |name: &str, value: String| -> Result<bool, LedgerError> {
        if meta_get(conn, name)?.as_deref() == Some(value.as_str()) {
            return Ok(false);
        }
        meta_set(conn, name, &value)?;
        Ok(true)
    };
    match fact {
        Fact::SettingChanged { name, value, .. } => {
            let old: Option<String> = conn
                .query_row("SELECT value FROM settings WHERE name = ?1", [name], |r| {
                    r.get(0)
                })
                .optional()?;
            if old == *value {
                return Ok(false);
            }
            match value {
                Some(v) => conn.execute(
                    "INSERT INTO settings(name, value) VALUES (?1, ?2)
                     ON CONFLICT(name) DO UPDATE SET value = excluded.value",
                    [name, v],
                )?,
                None => conn.execute("DELETE FROM settings WHERE name = ?1", [name])?,
            };
            Ok(true)
        }
        Fact::ShiftMarked { mark, at } => set_meta(
            match mark {
                crate::facts::ShiftMark::Started => "shift.started",
                crate::facts::ShiftMark::Stopped => "shift.stopped",
            },
            at.0.to_string(),
        ),
        Fact::Foreground { until } => set_meta("foreground_until", until.0.to_string()),
        Fact::Imported { from, count } => set_meta("imported_from", format!("{from}:{count}")),
        _ => Ok(false),
    }
}
