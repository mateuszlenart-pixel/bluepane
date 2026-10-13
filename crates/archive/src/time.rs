//! Conversions between `SystemTime` and the local-time stamps stored in ZIP files.

use std::time::SystemTime;

use chrono::{DateTime, Datelike, Local, LocalResult, NaiveDate, TimeZone, Timelike};

/// ZIP stores local time without a zone, in a two-second resolution from 1980 to 2107.
pub fn to_zip(time: SystemTime) -> Option<zip::DateTime> {
    let local: DateTime<Local> = time.into();
    zip::DateTime::from_date_and_time(
        u16::try_from(local.year()).ok()?,
        local.month() as u8,
        local.day() as u8,
        local.hour() as u8,
        local.minute() as u8,
        local.second().min(59) as u8,
    )
    .ok()
}

pub fn from_zip(stamp: zip::DateTime) -> Option<SystemTime> {
    let naive = NaiveDate::from_ymd_opt(
        i32::from(stamp.year()),
        u32::from(stamp.month()),
        u32::from(stamp.day()),
    )?
    .and_hms_opt(
        u32::from(stamp.hour()),
        u32::from(stamp.minute()),
        u32::from(stamp.second()),
    )?;
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(t) | LocalResult::Ambiguous(t, _) => Some(t.into()),
        LocalResult::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn a_stamp_survives_the_round_trip_to_two_seconds() {
        let original = UNIX_EPOCH + Duration::from_secs(1_760_336_881); // odd second
        let back = from_zip(to_zip(original).unwrap()).unwrap();
        let diff = original
            .duration_since(back)
            .or_else(|_| back.duration_since(original));
        assert!(diff.unwrap() <= Duration::from_secs(2));
    }

    #[test]
    fn times_before_1980_are_rejected() {
        assert!(to_zip(UNIX_EPOCH).is_none());
    }
}
