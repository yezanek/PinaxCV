//! Time source and RFC 3339 timestamps for stored rows.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cv_core::Timestamp;

/// The store's source of the current time. Injected so tests can control
/// soft-delete purging and snapshot throttling.
pub trait Clock: Send + Sync {
    fn now(&self) -> SystemTime;
}

/// The system clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Formats `t` as an RFC 3339 UTC timestamp with milliseconds, for example
/// `2026-10-13T15:03:33.120Z`. The fixed width makes text order match time
/// order, which the store's SQL relies on. Times before 1970 clamp to the epoch.
pub fn timestamp(t: SystemTime) -> Timestamp {
    let since = t.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let secs = since.as_secs();
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX);
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    Timestamp(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        since.subsec_millis()
    ))
}

/// Converts days since 1970-01-01 to a proleptic Gregorian date
/// (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    // Both values are in range by construction.
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64, millis: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs) + Duration::from_millis(millis)
    }

    #[test]
    fn formats_known_instants() {
        assert_eq!(timestamp(at(0, 0)).0, "1970-01-01T00:00:00.000Z");
        assert_eq!(
            timestamp(at(1_791_903_813, 120)).0,
            "2026-10-13T15:03:33.120Z"
        );
        // Leap day.
        assert_eq!(timestamp(at(951_782_400, 0)).0, "2000-02-29T00:00:00.000Z");
        assert_eq!(
            timestamp(at(1_735_689_599, 999)).0,
            "2024-12-31T23:59:59.999Z"
        );
    }

    #[test]
    fn text_order_matches_time_order() {
        let a = timestamp(at(1_700_000_000, 5));
        let b = timestamp(at(1_700_000_000, 40));
        let c = timestamp(at(1_700_000_009, 0));
        assert!(a < b && b < c);
    }

    #[test]
    fn times_before_the_epoch_clamp() {
        let before = UNIX_EPOCH - Duration::from_secs(10);
        assert_eq!(timestamp(before).0, "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn system_clock_is_after_2026() {
        assert!(timestamp(SystemClock.now()).0.as_str() > "2026");
    }
}
