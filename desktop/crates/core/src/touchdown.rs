//! Wykrywanie przyziemień (zatrzask) i heurystyka kategorii samolotu dla profili paneli w telefonie.

use crate::proto::{r, TouchdownData};

/// Jedna klatka danych. `td_*` – wartości z SimVarów PLANE TOUCHDOWN * (None, gdy źródło ich nie ma).
#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    pub time_ms: i64,
    pub unix_ms: i64,
    pub on_ground: bool,
    pub vs_fpm: f64,
    pub g_force: f64,
    pub pitch_deg: f64,
    pub bank_deg: f64,
    pub heading_true_deg: f64,
    pub lat: f64,
    pub lon: f64,
    pub ias_kt: f64,
    pub gs_kt: f64,
    pub td_vs_fpm: Option<f64>,
    pub td_pitch_deg: Option<f64>,
    pub td_bank_deg: Option<f64>,
    pub td_lat: Option<f64>,
    pub td_lon: Option<f64>,
}

pub const MIN_AIRBORNE_MS: i64 = 250;
pub const PEAK_WINDOW_MS: i64 = 600;

/// Zatrzaskuje dane każdego przyziemienia (przejście powietrze → ziemia po ≥ 250 ms w powietrzu, więc
/// odbicia też się liczą). Przez 600 ms po kontakcie zbiera szczyt przeciążenia i odświeża PLANE TOUCHDOWN *.
#[derive(Default)]
pub struct TouchdownLatch {
    airborne_since: Option<i64>,
    last_on_ground: Option<bool>,
    last_airborne: Sample,
    window_end: i64,
    seq: i64,
    pub current: Option<TouchdownData>,
}

fn fresh(on_ground: Option<f64>, airborne: Option<f64>) -> Option<f64> {
    match on_ground {
        Some(v) if v.is_finite() && airborne != Some(v) => Some(v),
        _ => None,
    }
}

/// PLANE TOUCHDOWN NORMAL VELOCITY bywa dodatnia przy opadaniu – w protokole opadanie jest ujemne.
fn vs(v: Option<f64>) -> Option<f64> {
    v.filter(|x| *x != 0.0 && x.is_finite()).map(|x| -x.abs())
}

impl TouchdownLatch {
    pub fn new() -> Self {
        Self { window_end: i64::MIN, ..Default::default() }
    }

    pub fn update(&mut self, s: &Sample) -> Option<TouchdownData> {
        if !s.on_ground {
            if self.airborne_since.is_none() {
                self.airborne_since = Some(s.time_ms);
            }
            self.last_airborne = *s;
        } else if self.last_on_ground == Some(false) && self.airborne_since.is_some_and(|since| s.time_ms - since >= MIN_AIRBORNE_MS) {
            let air = self.last_airborne;
            self.seq += 1;
            self.current = Some(TouchdownData {
                seq: self.seq,
                t: s.unix_ms,
                vs_fpm: r(vs(fresh(s.td_vs_fpm, air.td_vs_fpm)).unwrap_or(air.vs_fpm), 0),
                g_force: r(air.g_force.max(s.g_force), 2),
                pitch_deg: r(fresh(s.td_pitch_deg, air.td_pitch_deg).unwrap_or(air.pitch_deg), 1),
                bank_deg: r(fresh(s.td_bank_deg, air.td_bank_deg).unwrap_or(air.bank_deg), 1),
                heading_true_deg: r(s.heading_true_deg, 1),
                lat: s.lat,
                lon: s.lon,
                ias_kt: r(s.ias_kt, 1),
                gs_kt: r(s.gs_kt, 1),
            });
            self.refresh_position(s);
            self.window_end = s.time_ms + PEAK_WINDOW_MS;
        } else if s.on_ground && self.current.is_some() && s.time_ms <= self.window_end {
            let air = self.last_airborne;
            let c = self.current.as_mut().unwrap();
            c.g_force = r(c.g_force.max(s.g_force), 2);
            if let Some(v) = vs(fresh(s.td_vs_fpm, air.td_vs_fpm)) {
                c.vs_fpm = r(v, 0);
            }
            if let Some(p) = fresh(s.td_pitch_deg, air.td_pitch_deg) {
                c.pitch_deg = r(p, 1);
            }
            if let Some(b) = fresh(s.td_bank_deg, air.td_bank_deg) {
                c.bank_deg = r(b, 1);
            }
            self.refresh_position(s);
        }
        if s.on_ground {
            self.airborne_since = None;
        }
        self.last_on_ground = Some(s.on_ground);
        self.current.clone()
    }

    fn refresh_position(&mut self, s: &Sample) {
        let air = self.last_airborne;
        if let (Some(la), Some(lo)) = (fresh(s.td_lat, air.td_lat), fresh(s.td_lon, air.td_lon)) {
            if la != 0.0 && lo != 0.0 {
                let c = self.current.as_mut().unwrap();
                c.lat = la;
                c.lon = lo;
            }
        }
    }
}

const BIZJET_WORDS: &[&str] = &[
    "CITATION", "LEARJET", "PHENOM", "GULFSTREAM", "CHALLENGER", "FALCON", "GLOBAL", "HONDAJET", "HONDA JET", "ECLIPSE", "VISION", "PRAETOR", "LEGACY", "PILATUS PC-24", "PC-24",
    "SF50", "CJ4",
];

const AIRLINER_WORDS: &[&str] = &[
    "A300", "A310", "A318", "A319", "A320", "A321", "A330", "A340", "A350", "A380", "BOEING 7", "B7", "737", "747", "757", "767", "777", "787", "CRJ", "E170", "E175", "E190",
    "E195", "ERJ", "EMBRAER E", "MD-11", "MD-80", "DC-10", "ATR", "DASH 8", "DHC-8", "Q400", "SAAB 340", "CONCORDE", "BAE 146", "AVRO",
];

/// SimVar ENGINE TYPE: 0 piston, 1 jet, 2 none, 3 helo turbine, 4 unsupported, 5 turboprop.
pub fn engine_type_name(engine_type: i64) -> &'static str {
    match engine_type {
        0 => "piston",
        1 => "jet",
        3 => "helicopter",
        5 => "turboprop",
        _ => "none",
    }
}

pub fn classify(engine_type: &str, engine_count: i64, max_gross_weight_lbs: f64, title: &str, sim_category: &str) -> &'static str {
    let text = format!("{title} {sim_category}").to_uppercase();
    if engine_type == "helicopter" || text.contains("HELICOPTER") {
        return "helicopter";
    }
    let airliner = AIRLINER_WORDS.iter().any(|w| text.contains(w));
    let biz = BIZJET_WORDS.iter().any(|w| text.contains(w));
    match engine_type {
        "jet" => {
            if biz && !airliner {
                "bizjet"
            } else if airliner || engine_count >= 3 || max_gross_weight_lbs >= 100_000.0 {
                "airliner"
            } else {
                "bizjet"
            }
        }
        "turboprop" => {
            if airliner || max_gross_weight_lbs >= 35_000.0 {
                "airliner"
            } else {
                "turboprop"
            }
        }
        _ => "ga",
    }
}

/// MSFS zwraca ATC MODEL/ATC TYPE czasem jako klucz lokalizacji, np. "TT:ATCCOM.AC_MODEL_A320.0.text".
pub fn clean_atc_string(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return String::new();
    }
    if !s.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("TT:")) {
        return s.to_string();
    }
    let mut key = &s[3..];
    if key.len() >= 5 && key[key.len() - 5..].eq_ignore_ascii_case(".text") {
        key = &key[..key.len() - 5];
    }
    if let Some(k) = key.strip_suffix(".0") {
        key = k;
    }
    if let Some(dot) = key.rfind('.') {
        key = &key[dot + 1..];
    }
    for prefix in ["AC_MODEL_", "AC_MODEL ", "ATC_NAME_", "ATC_NAME "] {
        if key.len() >= prefix.len() && key[..prefix.len()].eq_ignore_ascii_case(prefix) {
            return key[prefix.len()..].trim().to_string();
        }
    }
    key.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn air(t: i64, vs: f64, g: f64) -> Sample {
        Sample { time_ms: t, unix_ms: 1000 + t, on_ground: false, vs_fpm: vs, g_force: g, pitch_deg: 3.0, lat: 54.0, lon: 18.0, ..Default::default() }
    }

    #[test]
    fn latch_touchdown_and_peak() {
        let mut l = TouchdownLatch::new();
        assert!(l.update(&air(0, -200.0, 1.0)).is_none());
        l.update(&air(300, -180.0, 1.05));
        let mut g = air(320, -180.0, 1.3);
        g.on_ground = true;
        let td = l.update(&g).unwrap();
        assert_eq!(td.seq, 1);
        assert_eq!(td.vs_fpm, -180.0);
        assert_eq!(td.g_force, 1.3);
        let mut later = g;
        later.time_ms = 500;
        later.g_force = 1.6;
        assert_eq!(l.update(&later).unwrap().g_force, 1.6);
        later.time_ms = 2000;
        later.g_force = 2.5;
        assert_eq!(l.update(&later).unwrap().g_force, 1.6);
    }

    #[test]
    fn short_hop_is_not_touchdown() {
        let mut l = TouchdownLatch::new();
        let mut g = air(0, 0.0, 1.0);
        g.on_ground = true;
        l.update(&g);
        l.update(&air(100, 0.0, 1.0));
        g.time_ms = 200;
        assert!(l.update(&g).is_none());
    }

    #[test]
    fn touchdown_simvar_preferred() {
        let mut l = TouchdownLatch::new();
        let mut a = air(0, -300.0, 1.0);
        a.td_vs_fpm = Some(90.0);
        l.update(&a);
        a.time_ms = 400;
        l.update(&a);
        let mut g = a;
        g.time_ms = 420;
        g.on_ground = true;
        g.td_vs_fpm = Some(150.0);
        assert_eq!(l.update(&g).unwrap().vs_fpm, -150.0);
    }

    #[test]
    fn categories() {
        assert_eq!(classify("jet", 2, 170_000.0, "Airbus A320neo", ""), "airliner");
        assert_eq!(classify("jet", 2, 17_000.0, "Cessna Citation CJ4", ""), "bizjet");
        assert_eq!(classify("turboprop", 1, 8_000.0, "TBM 930", ""), "turboprop");
        assert_eq!(classify("turboprop", 2, 50_000.0, "ATR 72-600", ""), "airliner");
        assert_eq!(classify("piston", 1, 2_500.0, "Cessna 172", ""), "ga");
        assert_eq!(classify("none", 1, 2_500.0, "Bell", "Helicopter"), "helicopter");
        assert_eq!(engine_type_name(5), "turboprop");
    }

    #[test]
    fn atc_strings() {
        assert_eq!(clean_atc_string("TT:ATCCOM.AC_MODEL_A320.0.text"), "A320");
        assert_eq!(clean_atc_string("  C172 "), "C172");
        assert_eq!(clean_atc_string("TT:ATCCOM.ATC_NAME CESSNA.0.text"), "CESSNA");
        assert_eq!(clean_atc_string(""), "");
    }
}
