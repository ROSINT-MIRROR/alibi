use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::Error;

pub fn epoch_now() -> Result<u64, Error> {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| Error::Clock(e.to_string()))?;
    Ok(since.as_secs())
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 {
        year / 400
    } else {
        (year - 399) / 400
    };
    let year_of_era = year - era * 400;
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146097 + day_of_era - 719468
}

pub fn ts14_to_epoch(ts: &str) -> Result<u64, Error> {
    if ts.len() < 4 || !ts.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::Parse(format!("not a ts14: {ts:?}")));
    }
    let year = digits(ts, 0..4, 0)?;
    let month = digits(ts, 4..6, 1)?;
    let day = digits(ts, 6..8, 1)?;
    let hour = digits(ts, 8..10, 0)?;
    let minute = digits(ts, 10..12, 0)?;
    let second = digits(ts, 12..14, 0)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(Error::Parse(format!("ts14 out of range: {ts:?}")));
    }
    if hour > 23 || minute > 59 || second > 60 {
        return Err(Error::Parse(format!("ts14 out of range: {ts:?}")));
    }
    let secs = days_from_civil(year, month, day) * 86400 + hour * 3600 + minute * 60 + second;
    u64::try_from(secs).map_err(|e| Error::Parse(e.to_string()))
}

fn digits(ts: &str, range: std::ops::Range<usize>, absent: i64) -> Result<i64, Error> {
    if ts.len() < range.end {
        return Ok(absent);
    }
    ts[range]
        .parse::<i64>()
        .map_err(|e| Error::Parse(e.to_string()))
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719468;
    let era = if shifted >= 0 {
        shifted / 146097
    } else {
        (shifted - 146096) / 146097
    };
    let day_of_era = shifted - era * 146097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

pub fn epoch_to_ts14(epoch: u64) -> Result<String, Error> {
    let secs = i64::try_from(epoch).map_err(|e| Error::Parse(e.to_string()))?;
    let days = secs.div_euclid(86400);
    let rest = secs.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    if !(0..=9999).contains(&year) {
        return Err(Error::Parse(format!("epoch out of ts14 range: {epoch}")));
    }
    let hour = rest / 3600;
    let minute = (rest % 3600) / 60;
    let second = rest % 60;
    Ok(format!(
        "{year:04}{month:02}{day:02}{hour:02}{minute:02}{second:02}"
    ))
}

pub fn rfc1123_to_epoch(stamp: &str) -> Result<u64, Error> {
    let body = after_weekday(stamp.trim());
    let mut fields = body.split_whitespace();
    let day: i64 = next_num(&mut fields, "day")?;
    let month = month_num(next_field(&mut fields, "month")?)?;
    let year: i64 = next_num(&mut fields, "year")?;
    let clock = next_field(&mut fields, "time")?;
    let mut hms = clock.split(':');
    let hour: i64 = next_num(&mut hms, "hour")?;
    let minute: i64 = next_num(&mut hms, "minute")?;
    let second: i64 = next_num(&mut hms, "second")?;
    let secs = days_from_civil(year, month, day) * 86400 + hour * 3600 + minute * 60 + second;
    u64::try_from(secs).map_err(|e| Error::Parse(e.to_string()))
}

fn after_weekday(stamp: &str) -> &str {
    let Some((_weekday, rest)) = stamp.split_once(',') else {
        return stamp;
    };
    rest.trim()
}

fn next_field<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
    what: &str,
) -> Result<&'a str, Error> {
    fields
        .next()
        .ok_or(Error::Parse(format!("rfc1123: missing {what}")))
}

fn next_num<'a>(fields: &mut impl Iterator<Item = &'a str>, what: &str) -> Result<i64, Error> {
    next_field(fields, what)?
        .parse::<i64>()
        .map_err(|e| Error::Parse(format!("rfc1123 {what}: {e}")))
}

fn month_num(token: &str) -> Result<i64, Error> {
    match token {
        "Jan" => Ok(1),
        "Feb" => Ok(2),
        "Mar" => Ok(3),
        "Apr" => Ok(4),
        "May" => Ok(5),
        "Jun" => Ok(6),
        "Jul" => Ok(7),
        "Aug" => Ok(8),
        "Sep" => Ok(9),
        "Oct" => Ok(10),
        "Nov" => Ok(11),
        "Dec" => Ok(12),
        other => Err(Error::Parse(format!("rfc1123: bad month {other:?}"))),
    }
}
