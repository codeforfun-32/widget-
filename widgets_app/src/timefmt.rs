//! Time in any time zone.
use chrono::{DateTime, Datelike, Duration, Local, Offset, TimeZone, Timelike, Utc};

#[derive(Clone, Debug)]
pub struct Parts {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// 0 = Sunday
    pub weekday: u32,
    /// minutes east of UTC
    pub offset_min: i32,
}

fn parts_from<T: TimeZone>(t: &DateTime<T>) -> Parts {
    Parts {
        year: t.year(),
        month: t.month(),
        day: t.day(),
        hour: t.hour(),
        minute: t.minute(),
        second: t.second(),
        weekday: t.weekday().num_days_from_sunday(),
        offset_min: t.offset().fix().local_minus_utc() / 60,
    }
}

/// The time now in a zone like "Asia/Tokyo" ("Local" or anything unknown = this PC), moved by `shift_min` minutes.
pub fn now_in(zone: &str, shift_min: i64) -> Parts {
    let utc = Utc::now() + Duration::minutes(shift_min);
    if zone != "Local" && !zone.is_empty() {
        if let Ok(tz) = zone.parse::<chrono_tz::Tz>() {
            return parts_from(&utc.with_timezone(&tz));
        }
    }
    parts_from(&utc.with_timezone(&Local))
}
