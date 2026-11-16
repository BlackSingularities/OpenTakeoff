//! Dane nawigacyjne z bazy symulatora: modele odpowiedzi `navdata`, zapytania telefonu, pamięć podręczna
//! (10 min, wspólne pobranie dla równoległych zapytań) i pomocnicza geometria.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::proto::{codes, Incoming};

#[derive(Clone, Debug, PartialEq)]
pub struct NavError {
    pub code: &'static str,
    pub message: String,
}

impl NavError {
    pub fn not_found(m: impl Into<String>) -> Self {
        Self { code: codes::NOT_FOUND, message: m.into() }
    }
    pub fn unavailable() -> Self {
        Self { code: codes::SIM_UNAVAILABLE, message: "Brak połączenia z MSFS".into() }
    }
}

pub type NavResult<T> = Result<T, NavError>;

// ---------- modele ----------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AirportInfo {
    pub icao: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub elev_ft: f64,
    pub runways: Vec<RunwayInfo>,
    pub frequencies: Vec<FrequencyInfo>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunwayInfo {
    pub ident: String,
    pub heading_true_deg: f64,
    pub length_ft: f64,
    pub width_ft: f64,
    pub threshold_lat: f64,
    pub threshold_lon: f64,
    pub elev_ft: f64,
    pub ils: Option<IlsInfo>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IlsInfo {
    pub ident: String,
    pub freq_mhz: f64,
    pub loc_course_deg: f64,
    pub gs_angle_deg: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrequencyInfo {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub mhz: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LegInfo {
    pub fix: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub leg_type: String,
    pub alt_desc: Option<String>,
    pub alt1_ft: Option<f64>,
    pub alt2_ft: Option<f64>,
    pub speed_kt: Option<f64>,
    pub course_deg: Option<f64>,
    pub dist_nm: Option<f64>,
    pub fly_over: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FixResult {
    pub ident: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
    pub freq_mhz: Option<f64>,
    pub region: Option<String>,
}

pub mod fix_types {
    pub const WAYPOINT: &str = "waypoint";
    pub const VOR: &str = "vor";
    pub const NDB: &str = "ndb";
    pub const AIRPORT: &str = "airport";
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NamedLegs {
    pub name: String,
    pub legs: Vec<LegInfo>,
}

impl NamedLegs {
    pub fn new(name: &str, legs: Vec<LegInfo>) -> Self {
        Self { name: name.to_string(), legs }
    }
}

/// SID albo STAR.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TerminalProcedure {
    pub name: String,
    pub common_legs: Vec<LegInfo>,
    pub runway_transitions: Vec<NamedLegs>,
    pub enroute_transitions: Vec<NamedLegs>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApproachProcedure {
    pub name: String,
    pub kind: String,
    pub runway: Option<String>,
    pub suffix: Option<String>,
    pub transitions: Vec<NamedLegs>,
    pub final_legs: Vec<LegInfo>,
    pub missed_legs: Vec<LegInfo>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AirportProcedures {
    pub icao: String,
    pub departures: Vec<TerminalProcedure>,
    pub arrivals: Vec<TerminalProcedure>,
    pub approaches: Vec<ApproachProcedure>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcedureInfo {
    pub name: String,
    pub runways: Vec<String>,
    pub transitions: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproachInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub runway: Option<String>,
    pub suffix: Option<String>,
    pub transitions: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProceduresInfo {
    pub icao: String,
    pub departures: Vec<ProcedureInfo>,
    pub arrivals: Vec<ProcedureInfo>,
    pub approaches: Vec<ApproachInfo>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcedureLegsInfo {
    pub icao: String,
    pub proc_type: String,
    pub name: String,
    pub runway: Option<String>,
    pub transition: Option<String>,
    pub legs: Vec<LegInfo>,
    pub missed: Vec<LegInfo>,
}

fn distinct(v: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in v {
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Sklejenie części procedury: punkt końcowy jednej części bywa punktem IF następnej – nie dubluj go.
fn append(target: &mut Vec<LegInfo>, legs: &[LegInfo]) {
    for leg in legs {
        if leg.leg_type == "IF" && !leg.fix.is_empty() && target.last().is_some_and(|l| l.fix == leg.fix) {
            continue;
        }
        target.push(leg.clone());
    }
}

impl AirportProcedures {
    pub fn summary(&self) -> ProceduresInfo {
        let sum = |p: &TerminalProcedure| ProcedureInfo {
            name: p.name.clone(),
            runways: distinct(p.runway_transitions.iter().map(|t| t.name.clone())),
            transitions: distinct(p.enroute_transitions.iter().map(|t| t.name.clone())),
        };
        ProceduresInfo {
            icao: self.icao.clone(),
            departures: self.departures.iter().map(sum).collect(),
            arrivals: self.arrivals.iter().map(sum).collect(),
            approaches: self
                .approaches
                .iter()
                .map(|a| ApproachInfo {
                    name: a.name.clone(),
                    kind: a.kind.clone(),
                    runway: a.runway.clone(),
                    suffix: a.suffix.clone(),
                    transitions: a.transitions.iter().map(|t| t.name.clone()).collect(),
                })
                .collect(),
        }
    }

    pub fn legs(&self, proc_type: &str, name: &str, runway: Option<&str>, transition: Option<&str>) -> NavResult<ProcedureLegsInfo> {
        if proc_type == "approach" {
            let a = self.approaches.iter().find(|x| same(&x.name, name)).ok_or_else(|| NavError::not_found(format!("Brak podejścia {name} w {}", self.icao)))?;
            let mut legs = Vec::new();
            if let Some(t) = transition {
                let tr = a.transitions.iter().find(|x| same(&x.name, t)).ok_or_else(|| NavError::not_found(format!("Brak przejścia {t} dla {}", a.name)))?;
                legs.extend(tr.legs.iter().cloned());
            }
            append(&mut legs, &a.final_legs);
            return Ok(ProcedureLegsInfo {
                icao: self.icao.clone(),
                proc_type: proc_type.into(),
                name: a.name.clone(),
                runway: a.runway.clone(),
                transition: transition.map(str::to_string),
                legs,
                missed: a.missed_legs.clone(),
            });
        }
        let list = if proc_type == "departure" { &self.departures } else { &self.arrivals };
        let p = list.iter().find(|x| same(&x.name, name)).ok_or_else(|| NavError::not_found(format!("Brak procedury {name} w {}", self.icao)))?;
        let mut rwy_legs: Vec<LegInfo> = Vec::new();
        let mut enr_legs: Vec<LegInfo> = Vec::new();
        let rwy_name: Option<String>;
        if let (Some(r), false) = (runway, p.runway_transitions.is_empty()) {
            let rt = p.runway_transitions.iter().find(|x| same(&x.name, r)).ok_or_else(|| NavError::not_found(format!("{} nie ma przejścia dla pasa {r}", p.name)))?;
            rwy_legs = rt.legs.clone();
            rwy_name = Some(rt.name.clone());
        } else if runway.is_none() && p.runway_transitions.len() == 1 {
            rwy_legs = p.runway_transitions[0].legs.clone();
            rwy_name = Some(p.runway_transitions[0].name.clone());
        } else {
            rwy_name = runway.map(str::to_string);
        }
        if let Some(t) = transition {
            let et = p.enroute_transitions.iter().find(|x| same(&x.name, t)).ok_or_else(|| NavError::not_found(format!("{} nie ma przejścia {t}", p.name)))?;
            enr_legs = et.legs.clone();
        }
        let mut result = Vec::new();
        if proc_type == "departure" {
            append(&mut result, &rwy_legs);
            append(&mut result, &p.common_legs);
            append(&mut result, &enr_legs);
        } else {
            append(&mut result, &enr_legs);
            append(&mut result, &p.common_legs);
            append(&mut result, &rwy_legs);
        }
        Ok(ProcedureLegsInfo {
            icao: self.icao.clone(),
            proc_type: proc_type.into(),
            name: p.name.clone(),
            runway: rwy_name,
            transition: transition.map(str::to_string),
            legs: result,
            missed: Vec::new(),
        })
    }
}

/// Źródło danych nawigacyjnych: baza MSFS (Facilities API) albo dane przykładowe symulacji.
/// Wywołania są blokujące (do kilku sekund) – woła się je z wątków roboczych.
pub trait NavDataProvider: Send + Sync {
    fn airport(&self, icao: &str) -> NavResult<AirportInfo>;
    fn procedures(&self, icao: &str) -> NavResult<AirportProcedures>;
    fn find_fix(&self, ident: &str) -> NavResult<Vec<FixResult>>;
    fn nearby(&self, _lat: f64, _lon: f64, _radius_nm: f64) -> NavResult<Vec<FixResult>> {
        Ok(Vec::new())
    }
}

// ---------- geometria ----------

pub mod math {
    pub const FT_PER_M: f64 = 3.28083989501;
    pub const M_PER_NM: f64 = 1852.0;

    pub fn norm_deg(deg: f64) -> f64 {
        ((deg % 360.0) + 360.0) % 360.0
    }

    /// Punkt odległy o `dist_m` w kierunku `bearing_deg` (przybliżenie płaskie).
    pub fn offset(lat: f64, lon: f64, bearing_deg: f64, dist_m: f64) -> (f64, f64) {
        let b = bearing_deg.to_radians();
        let d_lat = dist_m * b.cos() / 111_320.0;
        let d_lon = dist_m * b.sin() / (111_320.0 * lat.to_radians().cos());
        (lat + d_lat, lon + d_lon)
    }

    pub fn distance_nm(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
        const R: f64 = 3440.065;
        let d_lat = (lat2 - lat1).to_radians();
        let d_lon = (lon2 - lon1).to_radians();
        let a = (d_lat / 2.0).sin().powi(2) + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);
        2.0 * R * a.sqrt().min(1.0).asin()
    }

    pub fn bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
        let f1 = lat1.to_radians();
        let f2 = lat2.to_radians();
        let dl = (lon2 - lon1).to_radians();
        let y = dl.sin() * f2.cos();
        let x = f1.cos() * f2.sin() - f1.sin() * f2.cos() * dl.cos();
        norm_deg(y.atan2(x).to_degrees())
    }

    /// "RW29", "rwy 9", "29" → "29"/"09"; "11l" → "11L".
    pub fn normalize_runway(s: &str) -> String {
        let mut r = s.trim().to_uppercase();
        if let Some(x) = r.strip_prefix("RWY") {
            r = x.to_string();
        } else if let Some(x) = r.strip_prefix("RW") {
            r = x.to_string();
        }
        let r = r.trim().to_string();
        let c: Vec<char> = r.chars().collect();
        if !c.is_empty() && c[0].is_ascii_digit() && (c.len() == 1 || !c[1].is_ascii_digit()) {
            return format!("0{r}");
        }
        r
    }

    /// Numer i oznaczenie końca pasa w kodowaniu MSFS (1–36, 37+ = kierunki świata; 1 L, 2 R, 3 C, 4 W, 5 A, 6 B).
    pub fn runway_ident(number: i32, designator: i32) -> String {
        const COMPASS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
        let num = match number {
            1..=36 => format!("{number:02}"),
            37..=44 => COMPASS[(number - 37) as usize].to_string(),
            _ => String::new(),
        };
        let des = match designator {
            1 => "L",
            2 => "R",
            3 => "C",
            4 => "W",
            5 => "A",
            6 => "B",
            _ => "",
        };
        num + des
    }
}

// ---------- zapytania telefonu ----------

#[derive(Clone, Debug, PartialEq)]
pub enum Query {
    Airport { icao: String },
    Procedures { icao: String },
    ProcedureLegs { icao: String, proc_type: String, name: String, runway: Option<String>, transition: Option<String> },
    Fix { ident: String, near: Option<(f64, f64)> },
    Nearby { lat: f64, lon: f64, radius_nm: f64 },
}

impl Query {
    pub fn kind(&self) -> &'static str {
        match self {
            Query::Airport { .. } => "airport",
            Query::Procedures { .. } => "procedures",
            Query::ProcedureLegs { .. } => "procedure_legs",
            Query::Fix { .. } => "fix",
            Query::Nearby { .. } => "nearby",
        }
    }

    pub fn parse(m: &Incoming) -> Result<Query, String> {
        let code = |field: &str, min: usize, max: usize| -> Result<String, String> {
            let s = m.str(field).map(|s| s.trim().to_uppercase());
            match s {
                Some(s) if s.len() >= min && s.len() <= max && s.chars().all(|c| c.is_ascii_alphanumeric()) => Ok(s),
                _ => Err(format!("{field}: {min}–{max} liter lub cyfr")),
            }
        };
        let text = |field: &str, required: bool| -> Result<Option<String>, String> {
            if !m.has(field) {
                return if required { Err(format!("Brak pola {field}")) } else { Ok(None) };
            }
            match m.str(field).map(str::trim) {
                Some(s) if !s.is_empty() && s.chars().count() <= 16 && !s.chars().any(char::is_control) => Ok(Some(s.to_string())),
                _ => Err(format!("{field}: tekst 1–16 znaków")),
            }
        };
        let number = |field: &str, min: f64, max: f64| -> Result<f64, String> {
            match m.get(field).and_then(|v| v.as_f64()) {
                Some(d) if d.is_finite() && d >= min && d <= max => Ok(d),
                _ => Err(format!("{field}: liczba {min}…{max}")),
            }
        };
        match m.str("kind") {
            Some("airport") => Ok(Query::Airport { icao: code("icao", 2, 8)? }),
            Some("procedures") => Ok(Query::Procedures { icao: code("icao", 2, 8)? }),
            Some("procedure_legs") => {
                let icao = code("icao", 2, 8)?;
                let proc_type = match m.str("procType") {
                    Some(p @ ("departure" | "arrival" | "approach")) => p.to_string(),
                    _ => return Err("procType musi być departure, arrival albo approach".into()),
                };
                let name = text("name", true)?.unwrap().to_uppercase();
                let runway = text("runway", false)?.map(|r| math::normalize_runway(&r));
                let transition = text("transition", false)?.map(|t| t.to_uppercase());
                Ok(Query::ProcedureLegs { icao, proc_type, name, runway, transition })
            }
            Some("fix") => {
                let ident = code("ident", 1, 8)?;
                let (hl, ho) = (m.has("nearLat"), m.has("nearLon"));
                if hl != ho {
                    return Err("nearLat i nearLon podaje się razem".into());
                }
                let near = if hl { Some((number("nearLat", -90.0, 90.0)?, number("nearLon", -180.0, 180.0)?)) } else { None };
                Ok(Query::Fix { ident, near })
            }
            Some("nearby") => {
                let lat = number("lat", -90.0, 90.0)?;
                let lon = number("lon", -180.0, 180.0)?;
                let radius_nm = if m.has("radiusNm") { number("radiusNm", 1.0, 250.0)? } else { 60.0 };
                Ok(Query::Nearby { lat, lon, radius_nm })
            }
            _ => Err("kind musi być airport, procedures, procedure_legs, fix albo nearby".into()),
        }
    }

    fn cache_key(&self, source: &str) -> String {
        match self {
            Query::Airport { icao } => format!("{source}|airport|{icao}"),
            Query::Procedures { icao } | Query::ProcedureLegs { icao, .. } => format!("{source}|procedures|{icao}"),
            // Pozycja zaokrąglona do 0,2° (~12 NM): zapytania z lecącego samolotu trafiają w tę samą pozycję
            Query::Nearby { lat, lon, radius_nm } => format!("{source}|nearby|{}|{}|{radius_nm}", (lat * 5.0).round(), (lon * 5.0).round()),
            Query::Fix { ident, .. } => format!("{source}|fix|{ident}"),
        }
    }
}

// ---------- pamięć podręczna ----------

#[derive(Clone)]
enum Cached {
    Airport(Arc<AirportInfo>),
    Procedures(Arc<AirportProcedures>),
    Fixes(Arc<Vec<FixResult>>),
}

struct Entry {
    expires: Instant,
    slot: Mutex<Option<NavResult<Cached>>>,
    ready: Condvar,
}

pub const CACHE_TTL: Duration = Duration::from_secs(600);

/// Zapytania z pamięcią podręczną (TTL 10 min). Równoległe zapytania o to samo dzielą jedno żądanie.
/// Błędy nie są zapamiętywane.
#[derive(Default)]
pub struct NavDataService {
    cache: Mutex<HashMap<String, Arc<Entry>>>,
}

impl NavDataService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cached_count(&self) -> usize {
        self.cache.lock().len()
    }

    pub fn clear(&self) {
        self.cache.lock().clear();
    }

    fn get(&self, key: String, load: impl FnOnce() -> NavResult<Cached>) -> NavResult<Cached> {
        let now = Instant::now();
        let (entry, owner) = {
            let mut c = self.cache.lock();
            c.retain(|_, e| e.expires > now);
            match c.get(&key) {
                Some(e) => (e.clone(), false),
                None => {
                    let e = Arc::new(Entry { expires: now + CACHE_TTL, slot: Mutex::new(None), ready: Condvar::new() });
                    c.insert(key.clone(), e.clone());
                    (e, true)
                }
            }
        };
        if owner {
            let result = load();
            if result.is_err() {
                let mut c = self.cache.lock();
                if c.get(&key).is_some_and(|e| Arc::ptr_eq(e, &entry)) {
                    c.remove(&key);
                }
            }
            *entry.slot.lock() = Some(result.clone());
            entry.ready.notify_all();
            return result;
        }
        let mut slot = entry.slot.lock();
        while slot.is_none() {
            entry.ready.wait(&mut slot);
        }
        slot.clone().unwrap()
    }

    pub fn airport(&self, provider: &dyn NavDataProvider, source: &str, icao: &str) -> NavResult<Arc<AirportInfo>> {
        let q = Query::Airport { icao: icao.to_string() };
        match self.get(q.cache_key(source), || provider.airport(icao).map(|a| Cached::Airport(Arc::new(a))))? {
            Cached::Airport(a) => Ok(a),
            _ => Err(NavError::not_found("cache")),
        }
    }

    pub fn procedures(&self, provider: &dyn NavDataProvider, source: &str, icao: &str) -> NavResult<Arc<AirportProcedures>> {
        let q = Query::Procedures { icao: icao.to_string() };
        match self.get(q.cache_key(source), || provider.procedures(icao).map(|p| Cached::Procedures(Arc::new(p))))? {
            Cached::Procedures(p) => Ok(p),
            _ => Err(NavError::not_found("cache")),
        }
    }

    /// Odpowiedź na zapytanie telefonu jako `data` wiadomości `navdata`.
    pub fn execute(&self, q: &Query, provider: &dyn NavDataProvider, source: &str) -> NavResult<Value> {
        let to = |v: &dyn erased::Ser| v.value();
        match q {
            Query::Airport { icao } => Ok(to(&*self.airport(provider, source, icao)?)),
            Query::Procedures { icao } => Ok(to(&self.procedures(provider, source, icao)?.summary())),
            Query::ProcedureLegs { icao, proc_type, name, runway, transition } => {
                let p = self.procedures(provider, source, icao)?;
                Ok(to(&p.legs(proc_type, name, runway.as_deref(), transition.as_deref())?))
            }
            Query::Nearby { lat, lon, radius_nm } => {
                let (lat, lon, radius) = (*lat, *lon, *radius_nm);
                let all = match self.get(q.cache_key(source), || provider.nearby(lat, lon, radius).map(|v| Cached::Fixes(Arc::new(v))))? {
                    Cached::Fixes(f) => f,
                    _ => Arc::new(Vec::new()),
                };
                // Limit na typ, żeby gęste rejony (tysiące punktów trasy) nie zalały telefonu
                let limit = |t: &str| match t {
                    fix_types::AIRPORT => 40,
                    fix_types::VOR | fix_types::NDB => 30,
                    fix_types::WAYPOINT => 120,
                    _ => 30,
                };
                let mut with_d: Vec<(f64, &FixResult)> =
                    all.iter().map(|f| (math::distance_nm(lat, lon, f.lat, f.lon), f)).filter(|(d, _)| *d <= radius).collect();
                with_d.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut counts: HashMap<&str, usize> = HashMap::new();
                let results: Vec<&FixResult> = with_d
                    .into_iter()
                    .filter(|(_, f)| {
                        let c = counts.entry(f.kind.as_str()).or_default();
                        *c += 1;
                        *c <= limit(&f.kind)
                    })
                    .map(|(_, f)| f)
                    .collect();
                Ok(serde_json::json!({ "results": results }))
            }
            Query::Fix { ident, near } => {
                let found = match self.get(q.cache_key(source), || provider.find_fix(ident).map(|v| Cached::Fixes(Arc::new(v))))? {
                    Cached::Fixes(f) => f,
                    _ => Arc::new(Vec::new()),
                };
                if found.is_empty() {
                    return Err(NavError::not_found(format!("Nie znaleziono punktu {ident}")));
                }
                let mut sorted: Vec<FixResult> = found.iter().cloned().collect();
                match near {
                    Some((la, lo)) => sorted.sort_by(|a, b| math::distance_nm(*la, *lo, a.lat, a.lon).total_cmp(&math::distance_nm(*la, *lo, b.lat, b.lon))),
                    None => sorted.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.region.cmp(&b.region))),
                }
                Ok(serde_json::json!({ "results": sorted }))
            }
        }
    }
}

mod erased {
    pub trait Ser {
        fn value(&self) -> serde_json::Value;
    }
    impl<T: serde::Serialize> Ser for T {
        fn value(&self) -> serde_json::Value {
            serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
        }
    }
}

/// Kubełek żetonów: `capacity` żądań od razu, potem `per_second` na sekundę.
pub struct TokenBucket {
    capacity: f64,
    per_second: f64,
    state: Mutex<(f64, Instant)>,
}

impl TokenBucket {
    pub fn new(per_second: f64) -> Self {
        Self { capacity: per_second, per_second, state: Mutex::new((per_second, Instant::now())) }
    }

    pub fn try_take(&self) -> bool {
        let mut s = self.state.lock();
        let now = Instant::now();
        let elapsed = now.duration_since(s.1).as_secs_f64();
        s.0 = (s.0 + elapsed * self.per_second).min(self.capacity);
        s.1 = now;
        if s.0 < 1.0 {
            return false;
        }
        s.0 -= 1.0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simnav::SimulationNavData;

    fn msg(t: &str) -> Incoming {
        Incoming::parse(t).unwrap()
    }

    #[test]
    fn runway_helpers() {
        assert_eq!(math::normalize_runway("RW29"), "29");
        assert_eq!(math::normalize_runway("rwy 9"), "09");
        assert_eq!(math::normalize_runway("11l"), "11L");
        assert_eq!(math::runway_ident(9, 1), "09L");
        assert_eq!(math::runway_ident(38, 0), "NE");
        assert_eq!(math::runway_ident(0, 0), "");
    }

    #[test]
    fn query_parse() {
        let q = Query::parse(&msg(r#"{"v":1,"type":"navdata_request","kind":"airport","icao":"epgd"}"#)).unwrap();
        assert_eq!(q, Query::Airport { icao: "EPGD".into() });
        assert!(Query::parse(&msg(r#"{"v":1,"type":"navdata_request","kind":"airport","icao":"E"}"#)).is_err());
        let q = Query::parse(&msg(r#"{"v":1,"type":"navdata_request","kind":"procedure_legs","icao":"EPWA","procType":"departure","name":"logda1k","runway":"RW15"}"#)).unwrap();
        assert_eq!(q, Query::ProcedureLegs { icao: "EPWA".into(), proc_type: "departure".into(), name: "LOGDA1K".into(), runway: Some("15".into()), transition: None });
        assert!(Query::parse(&msg(r#"{"v":1,"type":"navdata_request","kind":"fix","ident":"WAR","nearLat":52}"#)).is_err());
        assert!(Query::parse(&msg(r#"{"v":1,"type":"navdata_request","kind":"nearby","lat":52,"lon":21,"radiusNm":500}"#)).is_err());
        assert!(Query::parse(&msg(r#"{"v":1,"type":"navdata_request","kind":"x"}"#)).is_err());
    }

    #[test]
    fn service_with_sample_data() {
        let svc = NavDataService::new();
        let p = SimulationNavData;
        let a = svc.execute(&Query::Airport { icao: "EPGD".into() }, &p, "simulation").unwrap();
        assert_eq!(a["runways"][1]["ils"]["ident"], "IGDA");
        assert!(a["runways"][0]["ils"]["gsAngleDeg"].is_number());
        let legs = svc
            .execute(&Query::ProcedureLegs { icao: "EPGD".into(), proc_type: "approach".into(), name: "ILS 29".into(), runway: None, transition: Some("GD401".into()) }, &p, "simulation")
            .unwrap();
        let fixes: Vec<&str> = legs["legs"].as_array().unwrap().iter().map(|l| l["fix"].as_str().unwrap()).collect();
        // Punkt IF powtarzający koniec przejścia jest pomijany
        assert_eq!(fixes, vec!["GD401", "GD291", "FI29", "RW29"]);
        assert_eq!(legs["missed"].as_array().unwrap().len(), 2);
        assert!(legs["legs"][0]["speedKt"].is_null());
        let sid = svc.execute(&Query::ProcedureLegs { icao: "EPGD".into(), proc_type: "departure".into(), name: "OKUPA1A".into(), runway: None, transition: None }, &p, "simulation").unwrap();
        assert_eq!(sid["runway"], "29");
        let fix = svc.execute(&Query::Fix { ident: "SOXER".into(), near: Some((50.0, 8.0)) }, &p, "simulation").unwrap();
        assert_eq!(fix["results"][0]["region"], "ED");
        assert_eq!(svc.execute(&Query::Airport { icao: "KJFK".into() }, &p, "simulation").unwrap_err().code, codes::NOT_FOUND);
        let near = svc.execute(&Query::Nearby { lat: 54.38, lon: 18.47, radius_nm: 30.0 }, &p, "simulation").unwrap();
        let r = near["results"].as_array().unwrap();
        assert!(r.iter().any(|f| f["ident"] == "EPGD"));
        assert!(r.iter().all(|f| f["ident"] != "EPWA"));
        let s = svc.execute(&Query::Procedures { icao: "EPWA".into() }, &p, "simulation").unwrap();
        assert_eq!(s["departures"].as_array().unwrap().len(), 2);
        assert_eq!(s["approaches"][1]["suffix"], "Z");
    }

    #[test]
    fn bucket() {
        let b = TokenBucket::new(3.0);
        assert!(b.try_take() && b.try_take() && b.try_take());
        assert!(!b.try_take());
    }
}
