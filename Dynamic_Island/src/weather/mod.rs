use std::thread;
use std::time::Duration;

use serde_json::Value;
use slint::Weak;
use windows::Devices::Geolocation::Geolocator;
use windows::Foundation::TimeSpan;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use crate::app_state::WEATHER_CITY;
use crate::links::{http_get_string, url_encode};
use crate::MainWindow;

struct Report {
    city: String,
    temp: i32,
    hi: i32,
    lo: i32,
    humidity: i32,
    wind: i32,
    kind: i32,
    desc: &'static str,
}

#[derive(Clone)]
struct Place {
    lat: f64,
    lon: f64,
    name: String,
}

fn describe(code: i64) -> (i32, &'static str) {
    match code {
        0 => (0, "Clear sky"),
        1 => (0, "Mostly clear"),
        2 => (1, "Partly cloudy"),
        3 => (2, "Overcast"),
        45 | 48 => (6, "Fog"),
        51 | 53 | 55 | 56 | 57 => (3, "Drizzle"),
        61 | 63 | 65 | 66 | 67 => (3, "Rain"),
        80 | 81 | 82 => (3, "Showers"),
        71 | 73 | 75 | 77 | 85 | 86 => (4, "Snow"),
        95 | 96 | 99 => (5, "Thunderstorm"),
        _ => (2, "Cloudy"),
    }
}

fn place_name(lat: f64, lon: f64) -> String {
    let url = format!(
        "https://api.bigdatacloud.net/data/reverse-geocode-client?latitude={lat}&longitude={lon}&localityLanguage=en"
    );
    let parsed: Option<Value> = http_get_string(&url).and_then(|t| serde_json::from_str(&t).ok());
    if let Some(v) = parsed {
        for key in ["city", "locality", "principalSubdivision"] {
            if let Some(s) = v[key].as_str() {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
    }
    "Your location".to_string()
}

fn windows_place() -> Option<Place> {
    let geo = Geolocator::new().ok()?;
    let max_age = TimeSpan { Duration: 10 * 60 * 10_000_000 };
    let timeout = TimeSpan { Duration: 8 * 10_000_000 };
    let pos = geo
        .GetGeopositionAsyncWithAgeAndTimeout(max_age, timeout)
        .ok()?
        .join()
        .ok()?;
    let point = pos.Coordinate().ok()?.Point().ok()?.Position().ok()?;
    let lat = point.Latitude;
    let lon = point.Longitude;
    Some(Place { lat, lon, name: place_name(lat, lon) })
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
    let geo_url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1",
        url_encode(city)
    );
    let geo: Value = serde_json::from_str(&http_get_string(&geo_url)?).ok()?;
    let place = geo["results"].get(0)?;
    Some(Place {
        lat: place["latitude"].as_f64()?,
        lon: place["longitude"].as_f64()?,
        name: place["name"].as_str().unwrap_or(city).to_string(),
    })
}

fn forecast(place: &Place) -> Option<Report> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}\
         &current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m\
         &daily=temperature_2m_max,temperature_2m_min&timezone=auto",
        place.lat, place.lon
    );
    let data: Value = serde_json::from_str(&http_get_string(&url)?).ok()?;
    let cur = &data["current"];
    let (kind, desc) = describe(cur["weather_code"].as_i64()?);

    Some(Report {
        city: place.name.clone(),
        temp: cur["temperature_2m"].as_f64()?.round() as i32,
        humidity: cur["relative_humidity_2m"].as_f64()?.round() as i32,
        wind: cur["wind_speed_10m"].as_f64()?.round() as i32,
        hi: data["daily"]["temperature_2m_max"].get(0)?.as_f64()?.round() as i32,
        lo: data["daily"]["temperature_2m_min"].get(0)?.as_f64()?.round() as i32,
        kind,
        desc,
    })
}

pub fn spawn(ui: Weak<MainWindow>) {
    thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut last_place: Option<Place> = None;

        loop {
            let place = windows_place()
                .or_else(ip_place)
                .or_else(|| last_place.clone())
                .or_else(|| city_place(WEATHER_CITY));

            let wait_secs = match place.as_ref().and_then(forecast) {
                Some(r) => {
                    last_place = place;
                    let sent = ui.upgrade_in_event_loop(move |ui| {
                        ui.set_weather_city(r.city.into());
                        ui.set_weather_temp(format!("{}°", r.temp).into());
                        ui.set_weather_desc(r.desc.into());
                        ui.set_weather_hilo(format!("{}° / {}°", r.hi, r.lo).into());
                        ui.set_weather_humidity(format!("{}%", r.humidity).into());
                        ui.set_weather_wind(format!("{} km/h", r.wind).into());
                        ui.set_weather_kind(r.kind);
                        ui.set_weather_ready(true);
                    });
                    if sent.is_err() {
                        return;
                    }
                    900
                }
                None => 60,
            };
            thread::sleep(Duration::from_secs(wait_secs));
        }
    });
}