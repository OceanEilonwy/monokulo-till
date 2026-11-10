//! The shift (docs/ledger.md §9): a window over the sales, from the shift's
//! start (or the trading day's, if later) until it's stopped. Live sales
//! always show, whatever the window.

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::Time;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};

use crate::model::PaymentPhase;
use crate::view::SaleView;
use crate::{UnixMillis, UnixSeconds};

/// Minutes after local midnight when the trading day starts: 04:00.
pub const DEFAULT_DAY_START_MINUTES: u32 = 4 * 60;

/// The shift's window: `until` is `None` while it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShiftWindow {
    pub from: UnixMillis,
    pub until: Option<UnixMillis>,
}

impl ShiftWindow {
    /// The window from the shift's marks: from `started` (or the trading
    /// day's start, if later) until `stopped`, when it was stopped after it
    /// started.
    pub fn of(
        started: Option<UnixMillis>,
        stopped: Option<UnixMillis>,
        day_start: UnixMillis,
    ) -> ShiftWindow {
        let from = started.map_or(day_start, |s| s.max(day_start));
        let until = match (started, stopped) {
            (Some(s), Some(t)) if t >= s && t >= from => Some(t),
            (None, Some(t)) if t >= from => Some(t),
            _ => None,
        };
        ShiftWindow { from, until }
    }

    pub fn contains(&self, at: UnixMillis) -> bool {
        at >= self.from && self.until.is_none_or(|u| at < u)
    }
}

/// When the trading day that `now_ms` falls in started: the last
/// `day_start_minutes` past local midnight at or before `now_ms`.
pub fn trading_day_start(now_ms: UnixMillis, tz: &TimeZone, day_start_minutes: u32) -> UnixMillis {
    let Ok(now) = Timestamp::from_millisecond(now_ms.0) else {
        return now_ms;
    };
    let zoned = now.to_zoned(tz.clone());
    let minutes = day_start_minutes.min(24 * 60 - 1);
    let start_time =
        Time::new((minutes / 60) as i8, (minutes % 60) as i8, 0, 0).unwrap_or(Time::midnight());
    let today = zoned.date().to_datetime(start_time);
    let start = match today.to_zoned(tz.clone()) {
        Ok(start) if start.timestamp() <= now => start,
        _ => match zoned
            .date()
            .yesterday()
            .ok()
            .and_then(|d| d.to_datetime(start_time).to_zoned(tz.clone()).ok())
        {
            Some(start) => start,
            None => return now_ms,
        },
    };
    UnixMillis(start.timestamp().as_millisecond())
}

/// The shift as a screen shows it: its sales (live ones always, ended ones
/// rung up in the window), newest first, and its totals over the window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShiftSummary {
    pub window: ShiftWindow,
    pub sales: Vec<SaleView>,
    /// Paid sales rung up in the window, by currency, in minor units.
    pub paid_total: BTreeMap<String, i64>,
    pub paid_count: u32,
    /// Cash sales paid in Monero in the window (core.md decision 117).
    pub cash_in_monero_count: u32,
    pub cash_in_monero_total: BTreeMap<String, i64>,
}

fn minor(view: &SaleView) -> Option<i64> {
    if let Some(found) = &view.found {
        return Some(found.amount.minor);
    }
    let exponent = monokulo_client::amounts::exponent(&view.currency).unwrap_or(2);
    monokulo_client::amounts::minor_units(&view.amount, exponent)
}

/// The shift over `views` (any order), within `window`.
pub fn summarise(mut views: Vec<SaleView>, window: ShiftWindow) -> ShiftSummary {
    views.retain(|v| v.live || window.contains(UnixSeconds(v.created_at).to_millis()));
    views.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.sale.cmp(&b.sale)));
    let mut summary = ShiftSummary {
        window,
        sales: Vec::new(),
        paid_total: BTreeMap::new(),
        paid_count: 0,
        cash_in_monero_count: 0,
        cash_in_monero_total: BTreeMap::new(),
    };
    for v in &views {
        let paid = v.payment.is_some_and(PaymentPhase::is_paid);
        if paid && window.contains(UnixSeconds(v.created_at).to_millis()) {
            let amount = minor(v).unwrap_or(0);
            summary.paid_count += 1;
            *summary.paid_total.entry(v.currency.clone()).or_default() += amount;
            if v.cash {
                summary.cash_in_monero_count += 1;
                *summary
                    .cash_in_monero_total
                    .entry(v.currency.clone())
                    .or_default() += amount;
            }
        }
    }
    summary.sales = views;
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trading_day_starts_at_four_local() {
        let tz = TimeZone::get("America/Los_Angeles").unwrap();
        // 2026-11-09 10:00 in Los Angeles (UTC-8) is 18:00 UTC.
        let now = "2026-11-09T18:00:00Z".parse::<Timestamp>().unwrap();
        let start = trading_day_start(
            UnixMillis(now.as_millisecond()),
            &tz,
            DEFAULT_DAY_START_MINUTES,
        );
        assert_eq!(
            Timestamp::from_millisecond(start.0).unwrap().to_string(),
            "2026-11-09T12:00:00Z"
        );
        // 02:00 local belongs to the day before.
        let early = "2026-11-09T10:00:00Z".parse::<Timestamp>().unwrap();
        let start = trading_day_start(
            UnixMillis(early.as_millisecond()),
            &tz,
            DEFAULT_DAY_START_MINUTES,
        );
        assert_eq!(
            Timestamp::from_millisecond(start.0).unwrap().to_string(),
            "2026-11-08T12:00:00Z"
        );
    }

    #[test]
    fn the_window_starts_at_the_later_of_the_shift_and_the_day() {
        let ms = UnixMillis;
        assert_eq!(ShiftWindow::of(None, None, ms(100)).from, ms(100));
        assert_eq!(ShiftWindow::of(Some(ms(50)), None, ms(100)).from, ms(100));
        assert_eq!(ShiftWindow::of(Some(ms(150)), None, ms(100)).from, ms(150));
        assert_eq!(
            ShiftWindow::of(Some(ms(150)), Some(ms(200)), ms(100)),
            ShiftWindow {
                from: ms(150),
                until: Some(ms(200))
            }
        );
        // Stopped before it was started again: running.
        assert_eq!(
            ShiftWindow::of(Some(ms(250)), Some(ms(200)), ms(100)).until,
            None
        );
    }
}
