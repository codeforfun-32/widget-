//! Small pure helpers: calendar maths, words for the time, clock hands, weather codes.
//! Nothing in here touches Windows or Slint.

pub const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September",
    "October", "November", "December",
];
pub const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => if is_leap(y) { 29 } else { 28 },
    }
}

/// 0 = Sunday ... 6 = Saturday
pub fn weekday(y: i32, m: u32, d: u32) -> u32 {
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let yy = if m < 3 { y - 1 } else { y };
    ((yy + yy / 4 - yy / 100 + yy / 400 + t[(m - 1) as usize] + d as i32).rem_euclid(7)) as u32
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub text: String,
    pub dim: bool,
    pub today: bool,
    pub weekend: bool,
}

/// The month as a grid. Returns the cells (a whole number of weeks) and the number of rows.
/// `week_start`: 0 = Sunday, 1 = Monday. `today` = (year, month, day) to highlight.
pub fn calendar_cells(y: i32, m: u32, week_start: u32, today: (i32, u32, u32)) -> (Vec<Cell>, i32) {
    let first = weekday(y, m, 1);
    let lead = (first + 7 - week_start) % 7;
    let dim = days_in_month(y, m);
    let (py, pm) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
    let prev_days = days_in_month(py, pm);
    let rows = ((lead + dim + 6) / 7) as i32;
    let total = rows as u32 * 7;
    let mut cells = Vec::new();
    for i in 0..total {
        let (text, other, d, mm, yy) = if i < lead {
            let d = prev_days - (lead - 1 - i);
            (d, true, d, pm, py)
        } else if i < lead + dim {
            let d = i - lead + 1;
            (d, false, d, m, y)
        } else {
            let d = i - lead - dim + 1;
            let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            (d, true, d, nm, ny)
        };
        let col = i % 7;
        let wd = (col + week_start) % 7;
        cells.push(Cell {
            text: text.to_string(),
            dim: other,
            today: !other && (yy, mm, d) == today,
            weekend: wd == 0 || wd == 6,
        });
    }
    (cells, rows)
}

/// Adds months to a (year, month) pair.
pub fn add_months(y: i32, m: u32, delta: i32) -> (i32, u32) {
    let idx = y * 12 + (m as i32 - 1) + delta;
    (idx.div_euclid(12), (idx.rem_euclid(12) + 1) as u32)
}

const NUM: [&str; 13] = [
    "twelve", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    "eleven", "twelve",
];

/// "half past three", "quarter to five" ...
pub fn word_time(h: u32, m: u32) -> String {
    let hour = |x: u32| NUM[(x % 12) as usize];
    let (hh, mm) = (h % 24, m % 60);
    let r = match mm {
        0 => format!("{} o'clock", hour(hh)),
        1..=4 => format!("just after {}", hour(hh)),
        5..=7 => format!("five past {}", hour(hh)),
        8..=12 => format!("ten past {}", hour(hh)),
        13..=17 => format!("quarter past {}", hour(hh)),
        18..=22 => format!("twenty past {}", hour(hh)),
        23..=27 => format!("twenty five past {}", hour(hh)),
        28..=32 => format!("half past {}", hour(hh)),
        33..=37 => format!("twenty five to {}", hour(hh + 1)),
        38..=42 => format!("twenty to {}", hour(hh + 1)),
        43..=47 => format!("quarter to {}", hour(hh + 1)),
        48..=52 => format!("ten to {}", hour(hh + 1)),
        53..=56 => format!("five to {}", hour(hh + 1)),
        _ => format!("almost {}", hour(hh + 1)),
    };
    r
}

/// A line from the middle of a 100x100 box. `deg` is clockwise from 12 o'clock.
pub fn hand(deg: f32, len: f32, back: f32) -> String {
    let a = deg.to_radians();
    let (s, c) = (a.sin(), a.cos());
    let x1 = 50.0 - back * s;
    let y1 = 50.0 + back * c;
    let x2 = 50.0 + len * s;
    let y2 = 50.0 - len * c;
    format!("M{:.2} {:.2} L{:.2} {:.2}", x1, y1, x2, y2)
}

/// Tick marks around a 100x100 face. Big ones every 5 minutes.
pub fn ticks(all_minutes: bool) -> String {
    let mut s = String::new();
    for i in 0..60 {
        let big = i % 5 == 0;
        if !big && !all_minutes {
            continue;
        }
        let a = (i as f32 * 6.0).to_radians();
        let (sn, cs) = (a.sin(), a.cos());
        let r2 = 47.0;
        let r1 = if big { 41.0 } else { 44.5 };
        s.push_str(&format!(
            "M{:.2} {:.2} L{:.2} {:.2} ",
            50.0 + r1 * sn,
            50.0 - r1 * cs,
            50.0 + r2 * sn,
            50.0 - r2 * cs
        ));
    }
    s
}

/// Positions (0..1) for 12 numerals.
pub fn numeral_pos(i: usize) -> (f32, f32) {
    let a = ((i + 1) as f32 * 30.0).to_radians();
    (0.5 + 0.36 * a.sin(), 0.5 - 0.36 * a.cos())
}

pub const ROMAN: [&str; 12] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII"];

// ───────── weather ─────────

/// Faces: 0 sunny, 1 night, 2 partly cloudy, 3 cloudy, 4 fog, 5 rain, 6 snow, 7 storm.
pub fn wmo_face(code: i64, is_day: bool) -> i32 {
    match code {
        0 | 1 => if is_day { 0 } else { 1 },
        2 => if is_day { 2 } else { 3 },
        3 => 3,
        45 | 48 => 4,
        51..=57 | 61..=67 | 80..=82 => 5,
        71..=77 | 85 | 86 => 6,
        95..=99 => 7,
        _ => 3,
    }
}

pub fn wmo_text(code: i64) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 | 63 | 65 => "Rain",
        66 | 67 => "Freezing rain",
        71 | 73 | 75 | 77 => "Snow",
        80..=82 => "Rain showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm, hail",
        _ => "Cloudy",
    }
}

/// Face name from the settings to face number (None = automatic).
pub fn face_from_name(name: &str) -> Option<i32> {
    match name {
        "Sunny" => Some(0),
        "Night" => Some(1),
        "Partly cloudy" => Some(2),
        "Cloudy" => Some(3),
        "Foggy" => Some(4),
        "Rain" => Some(5),
        "Snow" => Some(6),
        "Storm" => Some(7),
        _ => None,
    }
}

/// Background colours (top, bottom) of the big weather card for a face, as 0xRRGGBB.
pub fn face_colors(face: i32) -> (u32, u32) {
    match face {
        0 => (0x4a90e2, 0x2f6fc4),
        1 => (0x2a2f5c, 0x141735),
        2 => (0x5b86b5, 0x3b5f8a),
        3 => (0x6c7684, 0x4a5260),
        4 => (0x7d8791, 0x5d6670),
        5 => (0x3f5873, 0x273a50),
        6 => (0x7f9cbd, 0x5a7799),
        _ => (0x4a3a6b, 0x241a3d),
    }
}

pub fn fmt_temp(c: f64, fahrenheit: bool) -> String {
    let v = if fahrenheit { c * 9.0 / 5.0 + 32.0 } else { c };
    let unit = if fahrenheit { "°F" } else { "°C" };
    if v >= 0.0 { format!("+{:.1}{}", v, unit) } else { format!("{:.1}{}", v, unit) }
}

pub fn fmt_temp_short(c: f64, fahrenheit: bool) -> String {
    let v = if fahrenheit { c * 9.0 / 5.0 + 32.0 } else { c };
    let r = v.round() as i32;
    if r > 0 { format!("+{}°", r) } else { format!("{}°", r) }
}

pub fn fmt_wind(ms: f64, unit: &str) -> String {
    match unit {
        "km/h" => format!("{:.0} km/h", ms * 3.6),
        "mph" => format!("{:.0} mph", ms * 2.23694),
        _ => format!("{:.2} m/s", ms),
    }
}

// ───────── time zones ─────────

/// "Asia/Kolkata" -> "Kolkata", "America/New_York" -> "New York"
pub fn zone_label(z: &str) -> String {
    if z == "Local" { return "Local time".to_string(); }
    z.rsplit('/').next().unwrap_or(z).replace('_', " ")
}

/// Minutes to "+5:30" / "-8:00"
pub fn fmt_offset(mins: i32) -> String {
    let sign = if mins < 0 { '-' } else { '+' };
    let a = mins.abs();
    format!("{}{}:{:02}", sign, a / 60, a % 60)
}

/// Days since 1970-01-01 for a calendar date (handy to compare two dates).
pub fn day_number(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn pad2(n: u32) -> String {
    format!("{:02}", n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekdays() {
        assert_eq!(weekday(2023, 1, 21), 6); // Saturday
        assert_eq!(weekday(2025, 10, 24), 5); // Friday
        assert_eq!(weekday(2000, 2, 29), 2); // Tuesday
    }

    #[test]
    fn october_2025_grid() {
        let (cells, rows) = calendar_cells(2025, 10, 1, (2025, 10, 24));
        assert_eq!(rows, 5);
        assert_eq!(cells.len(), 35);
        assert_eq!(cells[0].text, "29"); // Mon 29 Sep
        assert!(cells[0].dim);
        assert_eq!(cells[2].text, "1");
        assert!(cells[2 + 23].today);
        assert_eq!(cells[34].text, "2");
    }

    #[test]
    fn sunday_start() {
        let (cells, rows) = calendar_cells(2023, 1, 0, (2000, 1, 1));
        assert_eq!(rows, 5);
        assert_eq!(cells[0].text, "1");
        assert!(!cells[0].dim);
        assert!(cells[0].weekend);
    }

    #[test]
    fn day_numbers() {
        assert_eq!(day_number(1970, 1, 1), 0);
        assert_eq!(day_number(2000, 3, 1) - day_number(2000, 2, 28), 2);
        assert_eq!(day_number(2024, 1, 1) - day_number(2023, 1, 1), 365);
    }

    #[test]
    fn months() {
        assert_eq!(add_months(2025, 12, 1), (2026, 1));
        assert_eq!(add_months(2025, 1, -1), (2024, 12));
        assert_eq!(add_months(2025, 6, -18), (2023, 12));
    }

    #[test]
    fn words() {
        assert_eq!(word_time(3, 30), "half past three");
        assert_eq!(word_time(14, 45), "quarter to three");
        assert_eq!(word_time(23, 58), "almost twelve");
        assert_eq!(word_time(0, 0), "twelve o'clock");
    }

    #[test]
    fn faces() {
        assert_eq!(wmo_face(0, true), 0);
        assert_eq!(wmo_face(0, false), 1);
        assert_eq!(wmo_face(63, true), 5);
        assert_eq!(wmo_face(95, true), 7);
        assert_eq!(fmt_offset(330), "+5:30");
        assert_eq!(fmt_offset(-480), "-8:00");
        assert_eq!(zone_label("America/New_York"), "New York");
        assert_eq!(fmt_temp(19.55, false), "+19.6°C");
        assert_eq!(fmt_temp_short(-3.4, false), "-3°");
    }
}
