//! Weather from Open-Meteo (free, no key). Everything runs on background threads.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Report {
    pub place: String,
    pub temp: f64,
    pub feels: f64,
    pub code: i64,
    pub is_day: bool,
    pub humidity: f64,
    pub wind: f64,
    /// (hour label, temp, code, is_day)
    pub hours: Vec<(String, f64, i64, bool)>,
    /// (label like 29.07, high, low, code)
    pub days: Vec<(String, f64, f64, i64)>,
}

#[derive(Default)]
struct Entry {
    report: Option<Arc<Report>>,
    error: bool,
    busy: bool,
    last: Option<Instant>,
}

#[derive(Clone, Default)]
pub struct Hub {
    map: Arc<Mutex<HashMap<String, Entry>>>,
}

pub struct Poll {
    pub report: Option<Arc<Report>>,
    pub error: bool,
}

fn http_get_string(url: &str) -> Option<String> {
    ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 DynamicIsland")
        .timeout(Duration::from_secs(10))
        .call()
        .ok()?
        .into_string()
        .ok()
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

struct Place {
    lat: f64,
    lon: f64,
    name: String,
}

fn ip_place() -> Option<Place> {
    let text = http_get_string("https://ipwho.is/")?;
    let v: Value = serde_json::from_str(&text).ok()?;
    if v["success"].as_bool() == Some(false) {
        return None;
    }
    Some(Place {
        lat: v["latitude"].as_f64()?,
        lon: v["longitude"].as_f64()?,
        name: v["city"].as_str().unwrap_or("Your location").to_string(),
    })
}

fn city_place(city: &str) -> Option<Place> {
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1",
        url_encode(city)
    );
    let v: Value = serde_json::from_str(&http_get_string(&url)?).ok()?;
    let p = v["results"].get(0)?;
    Some(Place {
        lat: p["latitude"].as_f64()?,
        lon: p["longitude"].as_f64()?,
        name: p["name"].as_str().unwrap_or(city).to_string(),
    })
}

fn forecast(place: &Place) -> Option<Report> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,weather_code,is_day\
         &hourly=temperature_2m,weather_code\
         &daily=weather_code,temperature_2m_max,temperature_2m_min\
         &timezone=auto&forecast_days=5&wind_speed_unit=ms",
        place.lat, place.lon
    );
    let v: Value = serde_json::from_str(&http_get_string(&url)?).ok()?;
    let cur = &v["current"];
    let now_text = cur["time"].as_str().unwrap_or("");
    let prefix: String = now_text.chars().take(13).collect();

    let mut hours = Vec::new();
    if let (Some(times), Some(temps), Some(codes)) = (
        v["hourly"]["time"].as_array(),
        v["hourly"]["temperature_2m"].as_array(),
        v["hourly"]["weather_code"].as_array(),
    ) {
        let start = times
            .iter()
            .position(|t| t.as_str().map(|s| s.starts_with(&prefix)).unwrap_or(false))
            .unwrap_or(0);
        for step in [3usize, 6, 9] {
            let i = start + step;
            if i >= times.len() {
                break;
            }
            let t = times[i].as_str().unwrap_or("");
            let label: String = t.chars().skip(11).take(5).collect();
            let hour: u32 = label.chars().take(2).collect::<String>().parse().unwrap_or(12);
            hours.push((
                label,
                temps[i].as_f64().unwrap_or(0.0),
                codes[i].as_i64().unwrap_or(3),
                (6..20).contains(&hour),
            ));
        }
    }

    let mut days = Vec::new();
    if let (Some(times), Some(his), Some(los), Some(codes)) = (
        v["daily"]["time"].as_array(),
        v["daily"]["temperature_2m_max"].as_array(),
        v["daily"]["temperature_2m_min"].as_array(),
        v["daily"]["weather_code"].as_array(),
    ) {
        for i in 0..times.len() {
            let t = times[i].as_str().unwrap_or("");
            // 2026-07-29 -> 29.07
            let parts: Vec<&str> = t.split('-').collect();
            let label = if parts.len() == 3 { format!("{}.{}", parts[2], parts[1]) } else { t.to_string() };
            days.push((
                label,
                his.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0),
                los.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0),
                codes.get(i).and_then(|x| x.as_i64()).unwrap_or(3),
            ));
        }
    }

    Some(Report {
        place: place.name.clone(),
        temp: cur["temperature_2m"].as_f64()?,
        feels: cur["apparent_temperature"].as_f64().unwrap_or(0.0),
        code: cur["weather_code"].as_i64().unwrap_or(3),
        is_day: cur["is_day"].as_i64().unwrap_or(1) == 1,
        humidity: cur["relative_humidity_2m"].as_f64().unwrap_or(0.0),
        wind: cur["wind_speed_10m"].as_f64().unwrap_or(0.0),
        hours,
        days,
    })
}

fn fetch(key: &str) -> Option<Report> {
    let place = if key == "auto" {
        ip_place()?
    } else {
        city_place(key.trim_start_matches("city:"))?
    };
    forecast(&place)
}

impl Hub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Latest data for a place ("auto" or "city:London"). Starts a download when it is missing or old.
    pub fn poll(&self, key: &str) -> Poll {
        let mut start = false;
        let result;
        {
            let mut map = match self.map.lock() {
                Ok(m) => m,
                Err(_) => return Poll { report: None, error: true },
            };
            let e = map.entry(key.to_string()).or_default();
            let age = e.last.map(|t| t.elapsed());
            let stale = match (e.report.is_some(), age) {
                (_, None) => true,
                (true, Some(a)) => a > Duration::from_secs(20 * 60),
                (false, Some(a)) => a > Duration::from_secs(60),
            };
            if stale && !e.busy {
                e.busy = true;
                start = true;
            }
            result = Poll { report: e.report.clone(), error: e.error && e.report.is_none() };
        }
        if start {
            let map = self.map.clone();
            let key = key.to_string();
            std::thread::spawn(move || {
                let r = fetch(&key);
                if let Ok(mut m) = map.lock() {
                    let e = m.entry(key).or_default();
                    e.busy = false;
                    e.last = Some(Instant::now());
                    match r {
                        Some(rep) => {
                            e.report = Some(Arc::new(rep));
                            e.error = false;
                        }
                        None => e.error = true,
                    }
                }
            });
        }
        result
    }
}
