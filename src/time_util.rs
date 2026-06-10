//! UTC math without a chrono dep: wayback ts14 strings ("20260610050921")
//! and RFC1123 memento datetimes ("Wed, 10 Jun 2026 05:09:21 GMT").

use std::time::{SystemTime, UNIX_EPOCH};

pub fn epoch_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// Hinnant civil-date algorithms; exact for all Gregorian dates.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z / 146097 } else { (z - 146096) / 146097 };
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn epoch_to_ts14(epoch: u64) -> String {
    let (y, m, d) = civil_from_days((epoch / 86400) as i64);
    let rem = epoch % 86400;
    format!(
        "{y:04}{m:02}{d:02}{:02}{:02}{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Wayback emits 4–14 digits; missing parts default to the period start.
pub fn ts14_to_epoch(ts: &str) -> Option<u64> {
    if ts.len() < 4 || !ts.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let part = |r: std::ops::Range<usize>, def: i64| -> Option<i64> {
        if ts.len() >= r.end { ts[r].parse().ok() } else { Some(def) }
    };
    let y = part(0..4, 0)?;
    let m = part(4..6, 1)?;
    let d = part(6..8, 1)?;
    let h = part(8..10, 0)?;
    let mi = part(10..12, 0)?;
    let s = part(12..14, 0)?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    let secs = days_from_civil(y, m, d) * 86400 + h * 3600 + mi * 60 + s;
    u64::try_from(secs).ok()
}

pub fn rfc1123_to_epoch(s: &str) -> Option<u64> {
    let s = s.trim();
    let s = s.split_once(',').map(|(_, r)| r.trim()).unwrap_or(s);
    let mut it = s.split_whitespace();
    let d: i64 = it.next()?.parse().ok()?;
    let m = match it.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let y: i64 = it.next()?.parse().ok()?;
    let mut hms = it.next()?.split(':');
    let h: i64 = hms.next()?.parse().ok()?;
    let mi: i64 = hms.next()?.parse().ok()?;
    let sec: i64 = hms.next()?.parse().ok()?;
    let secs = days_from_civil(y, m, d) * 86400 + h * 3600 + mi * 60 + sec;
    u64::try_from(secs).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts14_epoch_zero() {
        assert_eq!(epoch_to_ts14(0), "19700101000000");
        assert_eq!(ts14_to_epoch("19700101000000"), Some(0));
    }

    #[test]
    fn ts14_known_date() {
        // 2026-06-10 00:00:00 UTC
        assert_eq!(epoch_to_ts14(1_781_049_600), "20260610000000");
        assert_eq!(ts14_to_epoch("20260610000000"), Some(1_781_049_600));
    }

    #[test]
    fn ts14_round_trip() {
        for e in [0u64, 1_000_000_000, 1_779_824_055, 1_781_068_161] {
            assert_eq!(ts14_to_epoch(&epoch_to_ts14(e)), Some(e));
        }
    }

    #[test]
    fn ts14_partial() {
        assert_eq!(ts14_to_epoch("2026"), ts14_to_epoch("20260101000000"));
        assert_eq!(ts14_to_epoch("20260526"), ts14_to_epoch("20260526000000"));
    }

    #[test]
    fn ts14_garbage() {
        assert_eq!(ts14_to_epoch(""), None);
        assert_eq!(ts14_to_epoch("not-a-ts"), None);
        assert_eq!(ts14_to_epoch("20269999"), None);
    }

    #[test]
    fn rfc1123_known() {
        assert_eq!(rfc1123_to_epoch("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(
            rfc1123_to_epoch("Wed, 10 Jun 2026 00:00:00 GMT"),
            Some(1_781_049_600)
        );
        // weekday-less variant some servers emit
        assert_eq!(rfc1123_to_epoch("10 Jun 2026 00:00:00 GMT"), Some(1_781_049_600));
        assert_eq!(rfc1123_to_epoch("garbage"), None);
    }
}
