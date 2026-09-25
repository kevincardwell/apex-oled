use crate::Event;
use serde_json::Value;
use std::{
    fs,
    process::Command,
    sync::mpsc::Sender,
    thread,
    time::{Duration, SystemTime},
};

pub struct Weather {
    pub temp: f32,
    pub code: u64,
    pub day: bool,
    pub high: f32,
    pub low: f32,
    pub hourly: Vec<f32>, // next 24 h temperature
    pub rain: Vec<f32>,   // next 24 h precipitation probability, %
}

/// Fetches every 30 min (5 min after a failure). Checks the wall clock once a minute rather
/// than sleeping 30 min, so a resume from suspend refreshes within a minute.
pub fn spawn(tx: Sender<Event>) {
    thread::spawn(move || {
        let mut due = SystemTime::now();
        loop {
            if SystemTime::now() >= due {
                let got = fetch();
                due = SystemTime::now() + Duration::from_secs(if got.is_some() { 1800 } else { 300 });
                if let Some(w) = got
                    && tx.send(Event::Weather(w)).is_err()
                {
                    return;
                }
            }
            thread::sleep(Duration::from_secs(60));
        }
    });
}

pub fn fetch() -> Option<Weather> {
    let (lat, lon) = location()?;
    let fahrenheit = std::env::var("APEX_OLED_UNITS").is_ok_and(|u| u.eq_ignore_ascii_case("fahrenheit"));
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}\
         &current=temperature_2m,weather_code,is_day&hourly=temperature_2m,precipitation_probability\
         &daily=temperature_2m_max,temperature_2m_min&forecast_days=1&forecast_hours=24&timezone=auto{}",
        if fahrenheit { "&temperature_unit=fahrenheit" } else { "" }
    );
    let out = Command::new("/usr/bin/curl").args(["-fsS", "--max-time", "10", &url]).output().ok()?;
    parse(&serde_json::from_slice(&out.stdout).ok()?)
}

/// APEX_OLED_LOCATION="lat,lon", else the location set in the Omarchy bar's weather widget.
fn location() -> Option<(f64, f64)> {
    if let Ok(v) = std::env::var("APEX_OLED_LOCATION") {
        return parse_location(&v);
    }
    let path = format!("{}/.local/state/omarchy/settings/weather.json", std::env::var("HOME").ok()?);
    let loc: Value = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    Some((loc["latitude"].as_f64()?, loc["longitude"].as_f64()?))
}

fn parse_location(v: &str) -> Option<(f64, f64)> {
    let (lat, lon) = v.split_once(',')?;
    Some((lat.trim().parse().ok()?, lon.trim().parse().ok()?))
}

fn parse(d: &Value) -> Option<Weather> {
    let f = |v: &Value| v.as_f64().map(|x| x as f32);
    let series = |v: &Value| v.as_array().map(|a| a.iter().filter_map(f).collect()).unwrap_or_default();
    Some(Weather {
        temp: f(&d["current"]["temperature_2m"])?,
        code: d["current"]["weather_code"].as_u64()?,
        day: d["current"]["is_day"].as_u64() != Some(0),
        high: f(&d["daily"]["temperature_2m_max"][0])?,
        low: f(&d["daily"]["temperature_2m_min"][0])?,
        hourly: series(&d["hourly"]["temperature_2m"]),
        rain: series(&d["hourly"]["precipitation_probability"]),
    })
}

/// WMO code -> (open_iconic_weather glyph, label), grouped like the bar's widget.
/// Glyphs: '@' cloud, 'A' sun behind cloud, 'B' moon, 'C' rain, 'E' sun.
/// The font has no snow/fog/storm icons, so those borrow the nearest one and rely on the label.
pub fn describe(code: u64, day: bool) -> (char, &'static str) {
    match code {
        0 => (if day { 'E' } else { 'B' }, "Clear"),
        1 | 2 => (if day { 'A' } else { 'B' }, "Partly"),
        45 | 48 => ('@', "Fog"),
        51..=57 | 61 => ('C', "Drizzle"),
        63..=67 | 80..=82 => ('C', "Rain"),
        71..=77 | 85 | 86 => ('@', "Snow"),
        95..=99 => ('C', "Storm"),
        _ => ('@', "Cloudy"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_open_meteo() {
        let d = serde_json::json!({
            "current": {"temperature_2m": 17.0, "weather_code": 2, "is_day": 0},
            "hourly": {"temperature_2m": [17.2, 16.4], "precipitation_probability": [0, 10]},
            "daily": {"temperature_2m_max": [22.1], "temperature_2m_min": [11.6]}
        });
        let w = super::parse(&d).unwrap();
        assert_eq!((w.temp, w.code, w.day, w.high, w.low), (17.0, 2, false, 22.1, 11.6));
        assert_eq!((w.hourly, w.rain), (vec![17.2, 16.4], vec![0.0, 10.0]));
        assert_eq!(super::describe(2, false), ('B', "Partly"));
        assert_eq!(super::parse_location("40.71, -74.01"), Some((40.71, -74.01)));
        assert_eq!(super::parse_location("New York"), None);
    }
}
