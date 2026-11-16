//! Lot w dzienniku – te same pola i nazwy JSON co w aplikacji na telefon (FlightLog, TrackPoint, LandingReport),
//! więc archiwum .oab z telefonu i z komputera to jeden format.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TrackPoint {
    pub t: i64,
    pub lat: f64,
    pub lon: f64,
    pub alt_ft: i32,
    pub gs_kt: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vs_fpm: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ias_kt: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hdg: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bank_deg: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch_deg: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agl_ft: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub g_load: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_ground: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct FlightMarker {
    pub t: i64,
    pub label: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct LandingReport {
    pub score: i32,
    pub grade: String,
    pub vs_fpm: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub g_force: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bank_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ias_kt: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runway: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub along_ft: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_ft: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crosswind_kt: Option<i32>,
    pub bounces: i32,
    pub notes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub airport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runway_length_ft: Option<i32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct FlightLog {
    pub id: String,
    pub start: i64,
    pub end: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aircraft: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    pub max_alt_ft: i32,
    pub distance_nm: i32,
    /// Zawsze zapisywane (także puste) – telefon wymaga tego pola.
    pub track: Vec<TrackPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub landing: Option<LandingReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub landings: Vec<LandingReport>,
    pub markers: Vec<FlightMarker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
}

impl FlightLog {
    pub fn duration_min(&self) -> i64 {
        (self.end - self.start) / 60_000
    }

    /// Lądowania do statystyk: lista, a dla starszych wpisów pojedyncze `landing`.
    pub fn all_landings(&self) -> Vec<LandingReport> {
        if self.landings.is_empty() {
            self.landing.iter().cloned().collect()
        } else {
            self.landings.clone()
        }
    }

    /// Ocena ostatniego lądowania.
    pub fn last_landing(&self) -> Option<LandingReport> {
        self.landings.last().cloned().or_else(|| self.landing.clone())
    }

    /// Ten sam lot nagrany dwa razy (telefon i komputer jednocześnie): czasy pokrywają się w ponad połowie
    /// krótszego z lotów, a samolot się zgadza (albo jednego z nich nie znamy).
    pub fn same_flight(&self, other_start: i64, other_end: i64, other_aircraft: Option<&str>) -> bool {
        let overlap = self.end.min(other_end) - self.start.max(other_start);
        let shorter = (self.end - self.start).min(other_end - other_start).max(1);
        let aircraft_ok = match (self.aircraft.as_deref(), other_aircraft) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            _ => true,
        };
        overlap > 0 && overlap * 2 > shorter && aircraft_ok
    }
}

/// Nowy identyfikator lotu (UUID v4 jak w aplikacji na telefon).
pub fn new_id() -> String {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).expect("losowanie");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_detection() {
        let f = FlightLog { start: 0, end: 3_600_000, aircraft: Some("A320".into()), ..Default::default() };
        assert!(f.same_flight(60_000, 3_660_000, Some("a320")));
        assert!(f.same_flight(60_000, 3_660_000, None));
        assert!(!f.same_flight(60_000, 3_660_000, Some("C172")));
        assert!(!f.same_flight(3_000_000, 7_000_000, None));
        assert!(!f.same_flight(4_000_000, 5_000_000, None));
    }

    #[test]
    fn uuid_shape() {
        let id = new_id();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
    }
}
