//! Pogoda lotnicza z darmowego Data API NOAA (aviationweather.gov) – bez klucza. Odpowiedzi trzymamy
//! w pamięci 5 minut, żeby przełączanie między lotniskami nie przekraczało limitu zapytań.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::Value;

const BASE: &str = "https://aviationweather.gov/api/data";
const TTL: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cloud {
    pub cover: String,
    pub base_ft: Option<i32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Metar {
    pub station: String,
    pub raw: String,
    pub obs_time: i64,
    pub temp_c: Option<f64>,
    pub dewpoint_c: Option<f64>,
    pub wind_dir: Option<i32>,
    pub wind_variable: bool,
    pub wind_kt: Option<i32>,
    pub gust_kt: Option<i32>,
    pub visibility: Option<String>,
    pub altim_hpa: Option<f64>,
    pub wx: Option<String>,
    pub clouds: Vec<Cloud>,
    pub flight_category: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Taf {
    pub station: String,
    pub raw: String,
    pub issue_time: Option<String>,
}

fn get(url: &str) -> Result<String, String> {
    ureq::get(url)
        .set("User-Agent", "OpenTakeoff-Windows")
        .timeout(Duration::from_secs(20))
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())
}

fn num(o: &Value, k: &str) -> Option<f64> {
    match o.get(k)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().trim_end_matches('+').parse().ok(),
        _ => None,
    }
}

fn text(o: &Value, k: &str) -> Option<String> {
    match o.get(k)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub fn parse_metar(o: &Value) -> Metar {
    let wdir = text(o, "wdir");
    Metar {
        station: text(o, "icaoId").unwrap_or_default(),
        raw: text(o, "rawOb").unwrap_or_default(),
        obs_time: o.get("obsTime").and_then(|v| v.as_i64()).unwrap_or(0),
        temp_c: num(o, "temp"),
        dewpoint_c: num(o, "dewp"),
        wind_dir: wdir.as_deref().and_then(|w| w.parse().ok()),
        wind_variable: wdir.as_deref() == Some("VRB"),
        wind_kt: num(o, "wspd").map(|v| v as i32),
        gust_kt: num(o, "wgst").map(|v| v as i32),
        visibility: text(o, "visib"),
        altim_hpa: num(o, "altim"),
        wx: text(o, "wxString"),
        clouds: o
            .get("clouds")
            .and_then(|c| c.as_array())
            .map(|a| a.iter().filter_map(|c| Some(Cloud { cover: text(c, "cover")?, base_ft: num(c, "base").map(|b| b as i32) })).collect())
            .unwrap_or_default(),
        flight_category: text(o, "fltCat").or_else(|| text(o, "fltcat")),
        name: text(o, "name"),
    }
}

#[derive(Default)]
pub struct WeatherClient {
    metars: Mutex<HashMap<String, (Instant, Vec<Metar>)>>,
    tafs: Mutex<HashMap<String, (Instant, Option<Taf>)>>,
}

impl WeatherClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Depesze z ostatnich `hours` godzin, najnowsza pierwsza.
    pub fn metars(&self, icao: &str, hours: u32) -> Result<Vec<Metar>, String> {
        let key = format!("{icao}|{hours}");
        if let Some((t, v)) = self.metars.lock().get(&key) {
            if t.elapsed() < TTL {
                return Ok(v.clone());
            }
        }
        let body = get(&format!("{BASE}/metar?ids={icao}&format=json&hours={hours}"))?;
        let list: Vec<Metar> = if body.trim().is_empty() {
            Vec::new()
        } else {
            let v: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
            v.as_array().map(|a| a.iter().map(parse_metar).collect()).unwrap_or_default()
        };
        let mut list = list;
        list.sort_by(|a, b| b.obs_time.cmp(&a.obs_time));
        self.metars.lock().insert(key, (Instant::now(), list.clone()));
        Ok(list)
    }

    /// Bieżące METAR-y wielu lotnisk naraz (lista obserwowanych).
    pub fn latest(&self, ids: &[String]) -> Result<Vec<Metar>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let body = get(&format!("{BASE}/metar?ids={}&format=json", ids.join(",")))?;
        if body.trim().is_empty() {
            return Ok(Vec::new());
        }
        let v: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        let mut out: Vec<Metar> = v.as_array().map(|a| a.iter().map(parse_metar).collect()).unwrap_or_default();
        out.sort_by(|a, b| b.obs_time.cmp(&a.obs_time));
        let mut seen = std::collections::HashSet::new();
        out.retain(|m| seen.insert(m.station.clone()));
        Ok(out)
    }

    pub fn taf(&self, icao: &str) -> Result<Option<Taf>, String> {
        if let Some((t, v)) = self.tafs.lock().get(icao) {
            if t.elapsed() < TTL {
                return Ok(v.clone());
            }
        }
        let body = get(&format!("{BASE}/taf?ids={icao}&format=json"))?;
        let taf = if body.trim().is_empty() {
            None
        } else {
            let v: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
            v.as_array().and_then(|a| a.first()).and_then(|o| {
                Some(Taf { station: text(o, "icaoId").unwrap_or_else(|| icao.to_string()), raw: text(o, "rawTAF")?, issue_time: text(o, "issueTime") })
            })
        };
        self.tafs.lock().insert(icao.to_string(), (Instant::now(), taf.clone()));
        Ok(taf)
    }
}

/// Kategoria lotu z widzialności (SM) i podstawy chmur (ft), gdy API jej nie poda.
pub fn flight_category(m: &Metar) -> String {
    if let Some(c) = &m.flight_category {
        return c.clone();
    }
    let vis: f64 = m.visibility.as_deref().and_then(super::metar::parse_sm).unwrap_or(10.0);
    let ceiling = m.clouds.iter().filter(|c| matches!(c.cover.as_str(), "BKN" | "OVC" | "OVX" | "VV")).filter_map(|c| c.base_ft).min().unwrap_or(99_999);
    if vis < 1.0 || ceiling < 500 {
        "LIFR".into()
    } else if vis < 3.0 || ceiling < 1000 {
        "IFR".into()
    } else if vis <= 5.0 || ceiling <= 3000 {
        "MVFR".into()
    } else {
        "VFR".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_api_json() {
        let v: Value = serde_json::from_str(
            r#"{"icaoId":"EPWA","rawOb":"METAR EPWA 151200Z 27015G25KT 9999 BKN012 08/05 Q1013","obsTime":1792231200,"temp":8,"dewp":5,"wdir":270,"wspd":15,"wgst":25,"visib":"6+","altim":1013,"clouds":[{"cover":"BKN","base":1200}]}"#,
        )
        .unwrap();
        let m = parse_metar(&v);
        assert_eq!(m.wind_dir, Some(270));
        assert_eq!(m.gust_kt, Some(25));
        assert_eq!(m.clouds[0].base_ft, Some(1200));
        assert_eq!(flight_category(&m), "MVFR");
        let vrb: Value = serde_json::from_str(r#"{"icaoId":"X","wdir":"VRB","wspd":3,"visib":"1/2"}"#).unwrap();
        let m = parse_metar(&vrb);
        assert!(m.wind_variable);
        assert_eq!(m.wind_dir, None);
    }
}
