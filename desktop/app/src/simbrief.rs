//! Plan lotu z SimBrief: pobranie ostatniego planu użytkownika (API xml.fetcher, JSON), zapis na
//! dysku (plan zostaje po ponownym uruchomieniu), trasa na mapie i postęp lotu względem planu –
//! następny punkt, odległość do celu, paliwo planowane i rzeczywiste.
//!
//! Z OFP czytamy też całość potrzebną do widoku „OFP SimBrief” na karcie Plan lotu: wagi z limitami,
//! rozpisane paliwo, czasy blokowe, średni wiatr i odchyłkę ISA, navlog (etap, wiatr, OAT, GS, czas,
//! paliwo), METAR/TAF lotnisk, plan ATC i – gdy w OFP jest raport TLR – prędkości startu i lądowania.

use std::cell::RefCell;
use std::time::Duration;

use opentakeoff_core::lang::tr;
use opentakeoff_core::navdata::math::distance_nm;
use opentakeoff_core::proto::FlightState;
use opentakeoff_core::{log, trf};
use serde_json::Value;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::app::{self, state, ui};
use crate::*;

#[derive(Clone, Debug, Default)]
pub struct Fix {
    pub ident: String,
    pub via: String,
    pub lat: f64,
    pub lon: f64,
    pub alt_ft: i32,
    /// Odległość od startu po trasie (NM).
    pub cum_nm: f64,
    /// Paliwo planowane na pokładzie (w jednostkach planu).
    pub fuel: Option<f64>,
    pub wind: String,
    /// Navlog OFP: etap (CLB, CRZ, DSC), wiatr, temperatura, odchyłka ISA, prędkość nad ziemią,
    /// czas od startu (s), długość odcinka (NM).
    pub stage: String,
    pub wind_dir: Option<f64>,
    pub wind_spd: Option<f64>,
    pub oat: Option<f64>,
    pub isa_dev: Option<f64>,
    pub gs: Option<f64>,
    pub time_s: Option<f64>,
    pub leg_nm: Option<f64>,
}

/// Lotnisko w OFP (start, cel, zapasowe).
#[derive(Clone, Debug, Default)]
pub struct OfpApt {
    pub icao: String,
    pub name: String,
    pub rwy: String,
    pub elev: f64,
    pub metar: String,
    pub taf: String,
    pub trans_alt: String,
    pub trans_level: String,
    /// Zapasowe: odległość od celu (NM) i paliwo (jednostki planu).
    pub dist: f64,
    pub burn: f64,
}

/// Prędkości z raportu startu / lądowania (TLR) SimBrief.
#[derive(Clone, Debug, Default)]
pub struct Tlr {
    pub rwy: String,
    pub flaps: String,
    pub thrust: String,
    pub v1: f64,
    pub vr: f64,
    pub v2: f64,
    pub vref: f64,
    pub weight: f64,
    pub temp: String,
    pub dist: f64,
}

/// Rozszerzone dane OFP.
#[derive(Clone, Debug, Default)]
pub struct Ofp {
    pub oew: f64,
    pub pax: i32,
    pub payload: f64,
    pub zfw: f64,
    pub max_zfw: f64,
    pub tow: f64,
    pub max_tow: f64,
    pub ldw: f64,
    pub max_ldw: f64,
    pub taxi: f64,
    pub trip: f64,
    pub cont: f64,
    pub altn: f64,
    pub reserve: f64,
    pub etops: f64,
    pub extra: f64,
    pub min_to: f64,
    pub plan_to: f64,
    pub plan_land: f64,
    pub ramp: f64,
    pub avg_ff: f64,
    pub max_tanks: f64,
    pub cruise_mach: String,
    pub cruise_tas: f64,
    pub avg_wind_comp: f64,
    pub avg_wind_dir: f64,
    pub avg_wind_spd: f64,
    pub avg_isa: f64,
    pub air_dist: f64,
    pub step: String,
    pub sched_out: i64,
    pub sched_off: i64,
    pub sched_on: i64,
    pub sched_in: i64,
    pub block_s: f64,
    pub fpl: String,
    pub icao_type: String,
    pub reg: String,
    pub origin: OfpApt,
    pub dest: OfpApt,
    pub alternate: OfpApt,
    pub tlr_to: Option<Tlr>,
    pub tlr_ldg: Option<Tlr>,
}

#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub origin: String,
    pub origin_name: String,
    pub dest: String,
    pub dest_name: String,
    pub altn: String,
    pub callsign: String,
    pub aircraft: String,
    pub route: String,
    pub cruise_ft: i32,
    pub distance_nm: f64,
    pub ete_s: f64,
    pub ramp: f64,
    pub burn: f64,
    pub reserve: f64,
    pub zfw: f64,
    pub tow: f64,
    pub ci: String,
    pub dep_rwy: String,
    pub arr_rwy: String,
    pub lbs: bool,
    pub generated: i64,
    pub fixes: Vec<Fix>,
    pub ofp: Ofp,
}

thread_local! {
    static PLAN: RefCell<Option<Plan>> = const { RefCell::new(None) };
}

fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

fn file() -> std::path::PathBuf {
    opentakeoff_core::paths::config().join("simbrief.json")
}

fn txt(o: &Value, path: &[&str]) -> String {
    let mut v = o;
    for p in path {
        match v.get(p) {
            Some(x) => v = x,
            None => return String::new(),
        }
    }
    match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn num(o: &Value, path: &[&str]) -> f64 {
    txt(o, path).parse::<f64>().unwrap_or(0.0)
}

fn opt_num(o: &Value, path: &[&str]) -> Option<f64> {
    txt(o, path).parse::<f64>().ok()
}

/// Pierwszy element, gdy pole jest tablicą (np. kilka lotnisk zapasowych).
fn first<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v.get(key)? {
        Value::Array(a) => a.first(),
        o => Some(o),
    }
}

fn apt(v: &Value, key: &str) -> OfpApt {
    let Some(a) = first(v, key) else { return OfpApt::default() };
    OfpApt {
        icao: txt(a, &["icao_code"]),
        name: txt(a, &["name"]),
        rwy: txt(a, &["plan_rwy"]),
        elev: num(a, &["elevation"]),
        metar: txt(a, &["metar"]),
        taf: txt(a, &["taf"]),
        trans_alt: txt(a, &["trans_alt"]),
        trans_level: txt(a, &["trans_level"]),
        dist: num(a, &["distance"]),
        burn: num(a, &["burn"]),
    }
}

/// Raport TLR: warunki i wiersz planowanego pasa (nazwy pól różnią się między wersjami – kilka wariantów).
fn tlr(v: &Value, kind: &str) -> Option<Tlr> {
    let t = v.get("tlr")?.get(kind)?;
    let cond = t.get("conditions");
    let planned = cond.map(|c| txt(c, &["planned_runway"])).unwrap_or_default();
    let rows: Vec<&Value> = match t.get("runway") {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(o @ Value::Object(_)) => vec![o],
        _ => Vec::new(),
    };
    let row = rows.iter().find(|r| !planned.is_empty() && txt(r, &["identifier"]) == planned).or(rows.first())?;
    let pick = |names: &[&str]| names.iter().map(|n| num(row, &[*n])).find(|x| *x > 0.0).unwrap_or(0.0);
    let out = Tlr {
        rwy: txt(row, &["identifier"]),
        flaps: txt(row, &["flap_setting"]),
        thrust: txt(row, &["thrust_setting"]),
        v1: pick(&["speeds_v1", "v1"]),
        vr: pick(&["speeds_vr", "vr"]),
        v2: pick(&["speeds_v2", "v2"]),
        vref: pick(&["speeds_vref", "vref", "speeds_vapp"]),
        weight: cond.map(|c| [num(c, &["weight"]), num(c, &["planned_weight"])].into_iter().find(|x| *x > 0.0).unwrap_or(0.0)).unwrap_or(0.0),
        temp: cond.map(|c| txt(c, &["temperature"])).unwrap_or_default(),
        dist: pick(&["distance_actual", "actual_distance", "decision_distance", "distance"]),
    };
    (out.v1 > 0.0 || out.vr > 0.0 || out.vref > 0.0).then_some(out)
}

/// METAR: (kierunek wiatru, prędkość, temperatura, QNH hPa).
pub fn metar_parts(m: &str) -> (Option<f64>, Option<f64>, Option<f64>, Option<f64>) {
    let (mut wd, mut ws, mut t, mut q) = (None, None, None, None);
    for tok in m.split_whitespace() {
        if let Some(w) = tok.strip_suffix("KT") {
            let w = w.split('G').next().unwrap_or("");
            if w.len() >= 5 {
                wd = w[..3].parse::<f64>().ok();
                ws = w[3..].parse::<f64>().ok();
            }
        } else if let Some((a, b)) = tok.split_once('/') {
            let temp_like = |x: &str| !x.is_empty() && x.len() <= 3 && x.trim_start_matches('M').chars().all(|c| c.is_ascii_digit());
            if t.is_none() && temp_like(a) && (b.is_empty() || temp_like(b)) {
                t = a.trim_start_matches('M').parse::<f64>().ok().map(|x| if a.starts_with('M') { -x } else { x });
            }
        } else if let Some(x) = tok.strip_prefix('Q') {
            if x.len() == 4 {
                q = x.parse::<f64>().ok();
            }
        } else if let Some(x) = tok.strip_prefix('A') {
            if x.len() == 4 && x.chars().all(|c| c.is_ascii_digit()) {
                q = x.parse::<f64>().ok().map(|v| v / 100.0 * 33.8639);
            }
        }
    }
    (wd, ws, t, q)
}

/// Odczyt odpowiedzi API SimBrief (format JSON).
pub fn parse(text: &str) -> Result<Plan, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let status = txt(&v, &["fetch", "status"]);
    if !status.is_empty() && !status.eq_ignore_ascii_case("success") {
        return Err(status);
    }
    let mut p = Plan {
        origin: txt(&v, &["origin", "icao_code"]),
        origin_name: txt(&v, &["origin", "name"]),
        dest: txt(&v, &["destination", "icao_code"]),
        dest_name: txt(&v, &["destination", "name"]),
        altn: txt(&v, &["alternate", "icao_code"]),
        callsign: {
            let c = txt(&v, &["atc", "callsign"]);
            if c.is_empty() {
                format!("{}{}", txt(&v, &["general", "icao_airline"]), txt(&v, &["general", "flight_number"]))
            } else {
                c
            }
        },
        aircraft: {
            let name = txt(&v, &["aircraft", "name"]);
            let icao = txt(&v, &["aircraft", "icaocode"]);
            if name.is_empty() {
                icao
            } else {
                name
            }
        },
        route: txt(&v, &["general", "route"]),
        cruise_ft: num(&v, &["general", "initial_altitude"]) as i32,
        distance_nm: {
            let d = num(&v, &["general", "route_distance"]);
            if d > 0.0 {
                d
            } else {
                num(&v, &["general", "air_distance"])
            }
        },
        ete_s: num(&v, &["times", "est_time_enroute"]),
        ramp: num(&v, &["fuel", "plan_ramp"]),
        burn: num(&v, &["fuel", "enroute_burn"]),
        reserve: num(&v, &["fuel", "reserve"]),
        zfw: num(&v, &["weights", "est_zfw"]),
        tow: num(&v, &["weights", "est_tow"]),
        ci: txt(&v, &["general", "costindex"]),
        dep_rwy: txt(&v, &["origin", "plan_rwy"]),
        arr_rwy: txt(&v, &["destination", "plan_rwy"]),
        lbs: txt(&v, &["params", "units"]).to_lowercase().starts_with("lb"),
        generated: num(&v, &["params", "time_generated"]) as i64,
        fixes: Vec::new(),
        ofp: Ofp {
            oew: num(&v, &["weights", "oew"]),
            pax: num(&v, &["weights", "pax_count"]) as i32,
            payload: num(&v, &["weights", "payload"]),
            zfw: num(&v, &["weights", "est_zfw"]),
            max_zfw: num(&v, &["weights", "max_zfw"]),
            tow: num(&v, &["weights", "est_tow"]),
            max_tow: num(&v, &["weights", "max_tow"]),
            ldw: num(&v, &["weights", "est_ldw"]),
            max_ldw: num(&v, &["weights", "max_ldw"]),
            taxi: num(&v, &["fuel", "taxi"]),
            trip: num(&v, &["fuel", "enroute_burn"]),
            cont: num(&v, &["fuel", "contingency"]),
            altn: num(&v, &["fuel", "alternate_burn"]),
            reserve: num(&v, &["fuel", "reserve"]),
            etops: num(&v, &["fuel", "etops"]),
            extra: num(&v, &["fuel", "extra"]),
            min_to: num(&v, &["fuel", "min_takeoff"]),
            plan_to: num(&v, &["fuel", "plan_takeoff"]),
            plan_land: num(&v, &["fuel", "plan_landing"]),
            ramp: num(&v, &["fuel", "plan_ramp"]),
            avg_ff: num(&v, &["fuel", "avg_fuel_flow"]),
            max_tanks: num(&v, &["fuel", "max_tanks"]),
            cruise_mach: txt(&v, &["general", "cruise_mach"]),
            cruise_tas: num(&v, &["general", "cruise_tas"]),
            avg_wind_comp: num(&v, &["general", "avg_wind_comp"]),
            avg_wind_dir: num(&v, &["general", "avg_wind_dir"]),
            avg_wind_spd: num(&v, &["general", "avg_wind_spd"]),
            avg_isa: num(&v, &["general", "avg_temp_dev"]),
            air_dist: num(&v, &["general", "air_distance"]),
            step: txt(&v, &["general", "stepclimb_string"]),
            sched_out: num(&v, &["times", "sched_out"]) as i64,
            sched_off: num(&v, &["times", "sched_off"]) as i64,
            sched_on: num(&v, &["times", "sched_on"]) as i64,
            sched_in: num(&v, &["times", "sched_in"]) as i64,
            block_s: num(&v, &["times", "est_block"]),
            fpl: txt(&v, &["atc", "flightplan_text"]),
            icao_type: txt(&v, &["aircraft", "icaocode"]),
            reg: txt(&v, &["aircraft", "reg"]),
            origin: apt(&v, "origin"),
            dest: apt(&v, "destination"),
            alternate: apt(&v, "alternate"),
            tlr_to: tlr(&v, "takeoff"),
            tlr_ldg: tlr(&v, "landing"),
        },
    };
    if p.altn.is_empty() {
        p.altn = p.ofp.alternate.icao.clone();
    }
    if p.origin.is_empty() || p.dest.is_empty() {
        return Err(tr("W odpowiedzi SimBrief nie ma planu lotu").to_string());
    }
    // Start jako pierwszy punkt trasy
    let (olat, olon) = (num(&v, &["origin", "pos_lat"]), num(&v, &["origin", "pos_long"]));
    if olat != 0.0 || olon != 0.0 {
        p.fixes.push(Fix { ident: p.origin.clone(), lat: olat, lon: olon, alt_ft: num(&v, &["origin", "elevation"]) as i32, fuel: Some(p.ramp), ..Default::default() });
    }
    let fixes = match v.get("navlog").and_then(|n| n.get("fix")) {
        Some(Value::Array(a)) => a.clone(),
        Some(o @ Value::Object(_)) => vec![o.clone()],
        _ => Vec::new(),
    };
    let mut cum = 0.0;
    for f in &fixes {
        let lat = num(f, &["pos_lat"]);
        let lon = num(f, &["pos_long"]);
        if lat == 0.0 && lon == 0.0 {
            continue;
        }
        if let Some(prev) = p.fixes.last() {
            cum += distance_nm(prev.lat, prev.lon, lat, lon);
        }
        let wd = txt(f, &["wind_dir"]);
        let wsp = txt(f, &["wind_spd"]);
        let fuel = num(f, &["fuel_plan_onboard"]);
        p.fixes.push(Fix {
            ident: txt(f, &["ident"]),
            via: txt(f, &["via_airway"]),
            lat,
            lon,
            alt_ft: num(f, &["altitude_feet"]) as i32,
            cum_nm: cum,
            fuel: if fuel > 0.0 { Some(fuel) } else { None },
            wind: if wd.is_empty() { String::new() } else { format!("{:0>3}/{}", wd, wsp) },
            stage: txt(f, &["stage"]),
            wind_dir: opt_num(f, &["wind_dir"]),
            wind_spd: opt_num(f, &["wind_spd"]),
            oat: opt_num(f, &["oat"]),
            isa_dev: opt_num(f, &["oat_isa_dev"]),
            gs: opt_num(f, &["groundspeed"]),
            time_s: opt_num(f, &["time_total"]),
            leg_nm: opt_num(f, &["distance"]),
        });
    }
    if p.distance_nm <= 0.0 {
        p.distance_nm = cum;
    }
    Ok(p)
}

/// Wczytuje plan zapisany przy poprzednim pobraniu.
pub fn load_saved() {
    let Ok(t) = std::fs::read_to_string(file()) else { return };
    if let Ok(p) = parse(&t) {
        PLAN.with(|x| *x.borrow_mut() = Some(p));
    }
    refresh();
    publish_ofp();
}

/// Pobiera najnowszy plan użytkownika z SimBrief (w tle).
pub fn fetch(user: &str) {
    let user = user.trim().to_string();
    if user.is_empty() {
        app::toast(tr("Wpisz nazwę użytkownika albo Pilot ID z SimBrief"));
        return;
    }
    {
        let st = state();
        let mut b = st.borrow_mut();
        if b.s.simbrief_user != user {
            b.s.simbrief_user = user.clone();
            b.s.save();
        }
    }
    if let Some(u) = ui() {
        u.global::<App>().set_plan_busy(true);
    }
    std::thread::spawn(move || {
        let key = if user.chars().all(|c| c.is_ascii_digit()) { "userid" } else { "username" };
        let url = format!("https://www.simbrief.com/api/xml.fetcher.php?{key}={}&json=1", urlenc(&user));
        let res = ureq::get(&url)
            .set("User-Agent", "OpenTakeoff-Windows")
            .timeout(Duration::from_secs(25))
            .call()
            .map_err(|e| match e {
                ureq::Error::Status(400, r) => r.into_string().ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).map(|v| txt(&v, &["fetch", "status"])).filter(|s| !s.is_empty()).unwrap_or_else(|| tr("Nie znaleziono planu dla tego użytkownika").to_string()),
                e => e.to_string(),
            })
            .and_then(|r| r.into_string().map_err(|e| e.to_string()));
        app::later(move || {
            if let Some(u) = ui() {
                u.global::<App>().set_plan_busy(false);
            }
            match res.and_then(|t| parse(&t).map(|p| (t, p))) {
                Ok((t, p)) => {
                    let _ = opentakeoff_core::paths::write_atomic(&file(), t.as_bytes());
                    let msg = trf!("Plan {a} → {b} pobrany z SimBrief", a = p.origin, b = p.dest);
                    PLAN.with(|x| *x.borrow_mut() = Some(p));
                    refresh();
                    publish_ofp();
                    crate::navmap::render(true);
                    app::toast(msg);
                }
                Err(e) => {
                    log::warn(&format!("SimBrief: {e}"));
                    app::toast(trf!("SimBrief: {e}"));
                }
            }
        });
    });
}

fn urlenc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn clear() {
    PLAN.with(|x| *x.borrow_mut() = None);
    let _ = std::fs::remove_file(file());
    refresh();
    publish_ofp();
    crate::navmap::render(true);
}

/// Trasa planu do mapy: (szer., dł., wysokość ft, nazwa punktu).
pub fn plan() -> Option<Plan> {
    PLAN.with(|x| x.borrow().clone())
}

pub fn route() -> Vec<(f64, f64, i32, String)> {
    PLAN.with(|x| x.borrow().as_ref().map(|p| p.fixes.iter().map(|f| (f.lat, f.lon, f.alt_ft, f.ident.clone())).collect()).unwrap_or_default())
}

/// Trasa „EPWA → EPGD” albo pusty tekst.
pub fn route_text() -> String {
    PLAN.with(|x| x.borrow().as_ref().map(|p| format!("{} → {}", p.origin, p.dest)).unwrap_or_default())
}

/// Postęp: indeks następnego punktu, odległość do niego i do celu (NM).
pub fn progress(lat: f64, lon: f64) -> Option<(usize, f64, f64)> {
    PLAN.with(|x| {
        let b = x.borrow();
        let p = b.as_ref()?;
        let f = &p.fixes;
        if f.len() < 2 {
            return None;
        }
        let (j, _) = f.iter().enumerate().map(|(i, x)| (i, distance_nm(lat, lon, x.lat, x.lon))).min_by(|a, b| a.1.total_cmp(&b.1))?;
        let mut next = j;
        if j + 1 < f.len() {
            let leg = f[j + 1].cum_nm - f[j].cum_nm;
            if distance_nm(lat, lon, f[j + 1].lat, f[j + 1].lon) < leg {
                next = j + 1;
            }
        }
        if j == 0 && f.len() > 1 {
            next = next.max(1);
        }
        let to_next = distance_nm(lat, lon, f[next].lat, f[next].lon);
        let total = f.last().map(|l| l.cum_nm).unwrap_or(0.0);
        let remaining = to_next + (total - f[next].cum_nm).max(0.0);
        Some((next, to_next, remaining))
    })
}

/// Odległość do celu z planu (do paska transmisji); pusty tekst bez planu.
pub fn remaining_text(s: Option<&FlightState>) -> String {
    let Some(s) = s else { return String::new() };
    progress(s.position.lat, s.position.lon).map(|(_, _, r)| format!("{:.0} NM", r)).unwrap_or_default()
}

fn weight(v: f64, lbs: bool) -> String {
    if v <= 0.0 {
        return String::new();
    }
    format!("{} {}", group(v.round() as i64), if lbs { "lb" } else { "kg" })
}

fn group(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('\u{2009}');
        }
        out.push(c);
    }
    if n < 0 {
        format!("−{out}")
    } else {
        out
    }
}

/// Plan i postęp do interfejsu (Pulpit, panel boczny, pasek transmisji).
pub fn refresh() {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    let snap = state().borrow().host.snapshot();
    let plan = PLAN.with(|x| x.borrow().clone());
    let Some(p) = plan else {
        app.set_plan(PlanV::default());
        app.set_plan_fixes(ModelRc::new(VecModel::from(Vec::<PlanFixV>::new())));
        return;
    };
    let prog = snap.as_ref().and_then(|s| progress(s.position.lat, s.position.lon));
    let next_i = prog.map(|x| x.0);
    let total = p.fixes.last().map(|l| l.cum_nm).unwrap_or(p.distance_nm).max(1.0);
    let mut v = PlanV {
        loaded: true,
        origin: ss(&p.origin),
        origin_name: ss(&p.origin_name),
        dest: ss(&p.dest),
        dest_name: ss(&p.dest_name),
        altn: ss(&p.altn),
        callsign: ss(&p.callsign),
        aircraft: ss(&p.aircraft),
        route: ss(&p.route),
        cruise: ss(if p.cruise_ft >= 18000 { format!("FL{:03}", p.cruise_ft / 100) } else if p.cruise_ft > 0 { format!("{} ft", p.cruise_ft) } else { String::new() }),
        distance: ss(format!("{:.0} NM", p.distance_nm)),
        ete: ss(crate::fmt::ete(p.ete_s)),
        ramp: ss(weight(p.ramp, p.lbs)),
        burn: ss(weight(p.burn, p.lbs)),
        reserve: ss(weight(p.reserve, p.lbs)),
        zfw: ss(weight(p.zfw, p.lbs)),
        tow: ss(weight(p.tow, p.lbs)),
        ci: ss(&p.ci),
        dep_rwy: ss(&p.dep_rwy),
        arr_rwy: ss(&p.arr_rwy),
        fetched: ss(if p.generated > 0 { crate::fmt::date_time(p.generated * 1000) } else { String::new() }),
        ..Default::default()
    };
    if let (Some((ni, to_next, rem)), Some(s)) = (prog, snap.as_ref()) {
        v.live = true;
        v.next = ss(&p.fixes[ni].ident);
        v.next_dist = ss(format!("{:.0} NM", to_next));
        v.remaining = ss(format!("{:.0} NM", rem));
        v.progress = (1.0 - rem / total).clamp(0.0, 1.0) as f32;
        // Paliwo: plan na następnym punkcie (przeliczony na kg) wobec rzeczywistego
        if let Some(plan_fuel) = p.fixes[ni].fuel {
            let plan_kg = if p.lbs { plan_fuel * 0.453_592 } else { plan_fuel };
            let now = s.fuel.total_kg;
            v.fuel_plan = ss(weight(plan_kg, false));
            v.fuel_now = ss(weight(now, false));
            let diff = now - plan_kg;
            v.fuel_diff = ss(format!("{}{} kg", if diff >= 0.0 { "+" } else { "" }, group(diff.round() as i64)));
            v.fuel_ok = diff >= -plan_kg.max(1.0) * 0.03;
        }
    }
    if p.ramp > 0.0 {
        v.burn_frac = (p.burn / p.ramp).clamp(0.0, 1.0) as f32;
        v.reserve_frac = (p.reserve / p.ramp).clamp(0.0, 1.0 - v.burn_frac as f64) as f32;
        if let Some(s) = snap.as_ref() {
            let ramp_kg = if p.lbs { p.ramp * 0.453_592 } else { p.ramp };
            v.fuel_now_frac = (s.fuel.total_kg / ramp_kg.max(1.0)).clamp(0.0, 1.0) as f32;
        }
    }
    app.set_plan(v);
    render_profile();
    let fixes: Vec<PlanFixV> = p
        .fixes
        .iter()
        .enumerate()
        .map(|(i, f)| PlanFixV {
            ident: ss(&f.ident),
            via: ss(&f.via),
            alt: ss(if f.alt_ft >= 18000 { format!("FL{:03}", f.alt_ft / 100) } else { format!("{}", f.alt_ft) }),
            dist: ss(format!("{:.0}", f.cum_nm)),
            fuel: ss(f.fuel.map(|x| weight(x, p.lbs)).unwrap_or_default()),
            wind: ss(&f.wind),
            passed: next_i.is_some_and(|n| i < n),
            next: next_i == Some(i),
        })
        .collect();
    app.set_plan_fixes(ModelRc::new(VecModel::from(fixes)));
}

// ---------- profil trasy na Pulpicie ----------

pub const PROFILE: i32 = 42;

thread_local! {
    static PSIZE: RefCell<(f32, f32)> = const { RefCell::new((0.0, 0.0)) };
}

pub fn size(w: f32, h: f32) {
    let changed = PSIZE.with(|s| {
        let old = *s.borrow();
        *s.borrow_mut() = (w, h);
        (old.0 - w).abs() > 0.5 || (old.1 - h).abs() > 0.5
    });
    if changed {
        render_profile();
    }
}

/// Profil pionowy planu: wysokość po trasie, planowane paliwo (oś po prawej), T/C, T/D i samolot na żywo.
fn render_profile() {
    use crate::avionics::gfx::*;
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    let Some(p) = PLAN.with(|x| x.borrow().clone()) else { return };
    let (w0, h0) = PSIZE.with(|s| *s.borrow());
    let k = state().borrow().scale.max(1.0);
    let (w, h) = ((w0 * k) as u32, (h0 * k) as u32);
    if w < 32 || h < 32 || p.fixes.len() < 2 {
        return;
    }
    let mut c = Canvas::new(w, h);
    let (wf, hf) = (w as f32, h as f32);
    const BG: Argb = 0xFF0B1016;
    const CY: Argb = 0xFF4FD8EB;
    const DIM: Argb = 0xFF6F7C8A;
    const FUEL: Argb = 0xFFFFB547;
    c.rect(0.0, 0.0, wf, hf, BG);
    let (l, r, t, b) = (52.0 * k, wf - 60.0 * k, 22.0 * k, hf - 22.0 * k);
    let total = p.fixes.last().map(|f| f.cum_nm).unwrap_or(1.0).max(1.0);
    let max_alt = p.fixes.iter().map(|f| f.alt_ft).max().unwrap_or(10_000).max(p.cruise_ft) as f32;
    let top = (max_alt / 5000.0).ceil() * 5000.0 + 2000.0;
    let x = |d: f64| l + (d / total) as f32 * (r - l);
    let y = |a: f32| b - (a / top).clamp(0.0, 1.0) * (b - t);
    let step = if top > 30_000.0 { 10_000 } else { 5000 };
    let mut a = 0;
    while a as f32 <= top {
        c.dash_line(l, y(a as f32), r, y(a as f32), alpha(WHITE, 0x14), 1.0 * k, 3.0 * k, 5.0 * k);
        c.text_f(&if a >= 10_000 { format!("FL{}", a / 100) } else { a.to_string() }, l - 6.0 * k, y(a as f32), 9.5 * k, DIM, Align::Right, Face::Mono);
        a += step;
    }
    // paliwo planowane (prawa oś: 0 – paliwo na starcie)
    let ramp = p.ramp.max(1.0);
    let fy = |f: f64| b - (f / ramp).clamp(0.0, 1.0) as f32 * (b - t);
    let fuel: Vec<f32> = p.fixes.iter().filter_map(|f| Some([x(f.cum_nm), fy(f.fuel?)])).flatten().collect();
    if fuel.len() >= 4 {
        c.polyline(alpha(FUEL, 0xB0), 1.6 * k, &fuel);
        // czcionka wyświetlacza nie ma wąskiej spacji z formatu wag
        let plain = |v: f64| weight(v, p.lbs).replace(['\u{202F}', '\u{00A0}', '\u{2009}'], " ").replace('\u{2212}', "-");
        c.text_f(&plain(ramp), r + 6.0 * k, fy(ramp), 9.0 * k, FUEL, Align::Left, Face::Mono);
        c.text_f(&plain(p.reserve), r + 6.0 * k, fy(p.reserve).min(b - 6.0 * k), 9.0 * k, alpha(FUEL, 0xB0), Align::Left, Face::Mono);
    }
    // wysokość
    let mut poly = vec![x(0.0), b];
    for f in &p.fixes {
        poly.push(x(f.cum_nm));
        poly.push(y(f.alt_ft as f32));
    }
    poly.push(x(total));
    poly.push(b);
    c.poly(alpha(CY, 0x22), 0, 0.0, &poly);
    let line: Vec<f32> = p.fixes.iter().flat_map(|f| [x(f.cum_nm), y(f.alt_ft as f32)]).collect();
    c.polyline(CY, 2.2 * k, &line);
    let every = ((p.fixes.len() as f32 * 48.0 * k) / (r - l)).ceil().max(1.0) as usize;
    for (i, f) in p.fixes.iter().enumerate() {
        let (px, py) = (x(f.cum_nm), y(f.alt_ft as f32));
        if f.ident == "TOC" || f.ident == "TOD" {
            c.circle(px, py, 4.0 * k, 0xFF3CCF7A, 1.6 * k);
            c.text_f(if f.ident == "TOC" { "T/C" } else { "T/D" }, px, py - 11.0 * k, 10.0 * k, 0xFF3CCF7A, Align::Center, Face::CondBold);
            continue;
        }
        c.circle(px, py, 2.4 * k, CY, 0.0);
        if i % every == 0 || i + 1 == p.fixes.len() {
            c.text_f(&f.ident, px, b + 11.0 * k, 9.0 * k, DIM, Align::Center, Face::Mono);
        }
    }
    // samolot na żywo
    if let Some(s) = state().borrow().host.snapshot() {
        if let Some((_, _, rem)) = progress(s.position.lat, s.position.lon) {
            let d = (total - rem).clamp(0.0, total);
            let (px, py) = (x(d), y(s.position.alt_ind_ft as f32));
            c.line(px, t, px, b, alpha(WHITE, 0x50), 1.0 * k);
            c.poly(WHITE, 0, 0.0, &[px + 8.0 * k, py, px - 6.0 * k, py - 5.0 * k, px - 6.0 * k, py + 5.0 * k]);
            if let Some(f) = Some(s.fuel.total_kg).filter(|f| *f > 0.0) {
                let fk = if p.lbs { f / 0.453_592 } else { f };
                c.circle(px, fy(fk), 3.5 * k, FUEL, 0.0);
            }
        }
    }
    c.text_f(tr("PALIWO"), wf - 6.0 * k, 9.0 * k, 8.5 * k, alpha(FUEL, 0xC0), Align::Right, Face::CondBold);
    app.set_plan_profile_sb(c.into_image());
}

// ---------- widok OFP na karcie Plan lotu ----------

pub const OFP_PROFILE: i32 = 43;

thread_local! {
    static OSIZE: RefCell<(f32, f32)> = const { RefCell::new((0.0, 0.0)) };
}

pub fn size_ofp(w: f32, h: f32) {
    let changed = OSIZE.with(|s| {
        let old = *s.borrow();
        *s.borrow_mut() = (w, h);
        (old.0 - w).abs() > 0.5 || (old.1 - h).abs() > 0.5
    });
    if changed {
        render_ofp_profile();
    }
}

fn kg_of(v: f64, lbs: bool) -> f64 {
    if lbs {
        v * 0.453_592
    } else {
        v
    }
}

fn hhmm_utc(t: i64) -> String {
    if t <= 0 {
        return String::new();
    }
    chrono::DateTime::from_timestamp(t, 0).map(|d| d.format("%H:%M").to_string()).unwrap_or_default()
}

fn fl(ft: f64) -> String {
    if ft >= 18_000.0 {
        format!("FL{:03.0}", ft / 100.0)
    } else {
        format!("{ft:.0}")
    }
}

/// Prędkości TLR jednym wierszem (do widoku i checklisty).
pub fn tlr_text(t: &Tlr, takeoff: bool) -> String {
    let kt = |v: f64| format!("{v:.0} kt");
    let mut parts = Vec::new();
    if takeoff {
        if t.v1 > 0.0 {
            parts.push(format!("V1 {}", kt(t.v1)));
        }
        if t.vr > 0.0 {
            parts.push(format!("VR {}", kt(t.vr)));
        }
        if t.v2 > 0.0 {
            parts.push(format!("V2 {}", kt(t.v2)));
        }
    } else if t.vref > 0.0 {
        parts.push(format!("VREF {}", kt(t.vref)));
    }
    parts.join(" · ")
}

/// Dane OFP do widoku (po pobraniu, wczytaniu i usunięciu planu – nie co sekundę).
pub fn publish_ofp() {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    let empty = || -> ModelRc<OfpBarV> { ModelRc::new(VecModel::from(Vec::<OfpBarV>::new())) };
    let Some(p) = PLAN.with(|x| x.borrow().clone()) else {
        app.set_ofp(OfpV::default());
        app.set_ofp_weights(empty());
        app.set_ofp_fuel(ModelRc::new(VecModel::from(Vec::<OfpSegV>::new())));
        app.set_ofp_navlog(ModelRc::new(VecModel::from(Vec::<OfpRowV>::new())));
        app.set_ofp_apts(ModelRc::new(VecModel::from(Vec::<OfpAptV>::new())));
        return;
    };
    let o = &p.ofp;
    let w = |v: f64| weight(v, p.lbs);
    let unit = if p.lbs { "lb" } else { "kg" };
    let comp = o.avg_wind_comp;
    let tlr_to = o.tlr_to.as_ref().map(|t| format!("{} {} · {}{}", tr("Start"), t.rwy, tlr_text(t, true), if t.flaps.is_empty() { String::new() } else { format!(" · FLAPS {}", t.flaps) })).unwrap_or_default();
    let tlr_ldg = o.tlr_ldg.as_ref().map(|t| format!("{} {} · {}{}", tr("Lądowanie"), t.rwy, tlr_text(t, false), if t.flaps.is_empty() { String::new() } else { format!(" · FLAPS {}", t.flaps) })).unwrap_or_default();
    app.set_ofp(OfpV {
        loaded: true,
        callsign: ss(&p.callsign),
        title: ss(format!("{} → {}", p.origin, p.dest)),
        origin_name: ss(&p.origin_name),
        dest_name: ss(&p.dest_name),
        altn: ss(&p.altn),
        aircraft: ss([p.aircraft.as_str(), o.reg.as_str()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(" · ")),
        ci: ss(&p.ci),
        cruise: ss(fl(p.cruise_ft as f64)),
        speed: ss([if o.cruise_mach.is_empty() { String::new() } else { format!("M{}", o.cruise_mach.trim_start_matches('0')) }, if o.cruise_tas > 0.0 { format!("TAS {:.0} kt", o.cruise_tas) } else { String::new() }].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" · ")),
        wind: ss(format!("{} {:.0} kt · {:03.0}/{:.0}", if comp >= 0.0 { "TW" } else { "HW" }, comp.abs(), o.avg_wind_dir, o.avg_wind_spd)),
        wind_head: comp < 0.0,
        isa: ss(format!("ISA {:+.0} °C", o.avg_isa)),
        dist: ss(format!("{:.0} NM · {} {:.0} NM", p.distance_nm, tr("w powietrzu"), o.air_dist)),
        t_out: ss(hhmm_utc(o.sched_out)),
        t_off: ss(hhmm_utc(o.sched_off)),
        t_on: ss(hhmm_utc(o.sched_on)),
        t_in: ss(hhmm_utc(o.sched_in)),
        block: ss(crate::fmt::ete(o.block_s)),
        ete: ss(crate::fmt::ete(p.ete_s)),
        units: ss(unit),
        pax: o.pax,
        payload: ss(w(o.payload)),
        oew: ss(w(o.oew)),
        avg_ff: ss(if o.avg_ff > 0.0 { format!("{}/h", w(o.avg_ff)) } else { String::new() }),
        plan_land: ss(w(o.plan_land)),
        min_to: ss(w(o.min_to)),
        ramp: ss(w(o.ramp)),
        step: ss(&o.step),
        route: ss(&p.route),
        fpl: ss(&o.fpl),
        generated: ss(if p.generated > 0 { crate::fmt::date_time(p.generated * 1000) } else { String::new() }),
        has_tlr: o.tlr_to.is_some() || o.tlr_ldg.is_some(),
        tlr_to: ss(tlr_to),
        tlr_ldg: ss(tlr_ldg),
    });
    // Wagi wobec limitów
    let bar = |k: &str, est: f64, max: f64| OfpBarV {
        k: ss(k),
        est: ss(w(est)),
        max: ss(if max > 0.0 { format!("max {}", w(max)) } else { String::new() }),
        frac: if max > 0.0 { (est / max).clamp(0.0, 1.2) as f32 } else { 0.0 },
        over: max > 0.0 && est > max,
        margin: ss(if max > 0.0 { format!("{}{}", if max - est >= 0.0 { "+" } else { "−" }, w((max - est).abs())) } else { String::new() }),
    };
    let weights = vec![bar("ZFW", o.zfw, o.max_zfw), bar("TOW", o.tow, o.max_tow), bar("LDW", o.ldw, o.max_ldw)];
    app.set_ofp_weights(ModelRc::new(VecModel::from(weights)));
    // Paliwo: części bloku
    let parts = [
        (tr("Kołowanie"), o.taxi),
        (tr("Trasa"), o.trip),
        (tr("Rezerwa trasy"), o.cont),
        (tr("Zapasowe"), o.altn),
        (tr("Rezerwa końcowa"), o.reserve),
        ("ETOPS", o.etops),
        (tr("Dodatkowe"), o.extra),
    ];
    let sum: f64 = parts.iter().map(|x| x.1).sum::<f64>().max(o.ramp).max(1.0);
    let fuel: Vec<OfpSegV> = parts.iter().enumerate().filter(|(_, x)| x.1 > 0.0).map(|(i, x)| OfpSegV { k: ss(x.0), v: ss(w(x.1)), frac: (x.1 / sum) as f32, tone: i as i32 }).collect();
    app.set_ofp_fuel(ModelRc::new(VecModel::from(fuel)));
    // Navlog
    let rows: Vec<OfpRowV> = p
        .fixes
        .iter()
        .map(|f| OfpRowV {
            ident: ss(&f.ident),
            via: ss(&f.via),
            stage: ss(&f.stage),
            alt: ss(fl(f.alt_ft as f64)),
            wind: ss(match (f.wind_dir, f.wind_spd) {
                (Some(d), Some(s)) => format!("{d:03.0}/{s:.0}"),
                _ => String::new(),
            }),
            oat: ss(f.oat.map(|t| format!("{t:.0}°")).unwrap_or_default()),
            isa: ss(f.isa_dev.map(|t| format!("{t:+.0}")).unwrap_or_default()),
            gs: ss(f.gs.map(|g| format!("{g:.0}")).unwrap_or_default()),
            dist: ss(format!("{:.0}", f.cum_nm)),
            time: ss(f.time_s.map(|t| format!("{}:{:02}", (t as i64) / 3600, (t as i64) % 3600 / 60)).unwrap_or_default()),
            fuel: ss(f.fuel.map(|x| format!("{:.1}", x / 1000.0)).unwrap_or_default()),
            mark: f.ident == "TOC" || f.ident == "TOD",
        })
        .collect();
    app.set_ofp_navlog(ModelRc::new(VecModel::from(rows)));
    // Lotniska
    let apt_v = |role: &str, a: &OfpApt, extra: String| OfpAptV {
        role: ss(role),
        icao: ss(&a.icao),
        name: ss(&a.name),
        rwy: ss(if a.rwy.is_empty() { String::new() } else { format!("RWY {}", a.rwy) }),
        metar: ss(&a.metar),
        taf: ss(&a.taf),
        info: ss(extra),
    };
    let ta = |a: &OfpApt| if a.trans_alt.is_empty() { String::new() } else { format!("TA {} · TL {}", a.trans_alt, a.trans_level) };
    let mut apts = vec![apt_v(tr("start"), &o.origin, ta(&o.origin)), apt_v(tr("cel"), &o.dest, ta(&o.dest))];
    if !o.alternate.icao.is_empty() {
        apts.push(apt_v(tr("zapasowe"), &o.alternate, format!("{:.0} NM · {}", o.alternate.dist, w(o.alternate.burn))));
    }
    app.set_ofp_apts(ModelRc::new(VecModel::from(apts)));
    render_ofp_profile();
}

/// Chorągiewka wiatru: drzewce w stronę, z której wieje; 50 kt – trójkąt, 10 kt – pełna kreska, 5 kt – połowa.
fn barb(c: &mut crate::avionics::gfx::Canvas, x: f32, y: f32, dir: f64, spd: f64, k: f32, col: u32) {
    let len = 20.0 * k;
    c.save();
    c.translate(x, y);
    c.rotate(dir as f32, 0.0, 0.0);
    let mut rem = ((spd / 5.0).round() * 5.0) as i32;
    if rem < 3 {
        c.circle(0.0, 0.0, 3.0 * k, col, 1.2 * k);
        c.restore();
        return;
    }
    c.line(0.0, 0.0, 0.0, -len, col, 1.4 * k);
    let mut pos = -len;
    while rem >= 50 {
        c.poly(col, 0, 0.0, &[0.0, pos, 8.0 * k, pos + 3.0 * k, 0.0, pos + 6.0 * k]);
        pos += 7.0 * k;
        rem -= 50;
    }
    while rem >= 10 {
        c.line(0.0, pos, 8.0 * k, pos - 4.0 * k, col, 1.4 * k);
        pos += 4.0 * k;
        rem -= 10;
    }
    if rem >= 5 {
        c.line(0.0, pos, 4.5 * k, pos - 2.2 * k, col, 1.4 * k);
    }
    c.restore();
}

/// Profil OFP: wysokość z etapami, chorągiewki wiatru nad trasą, pasek temperatury (odchyłka ISA) pod
/// trasą, paliwo na pokładzie, T/C i T/D, nazwy punktów.
fn render_ofp_profile() {
    use crate::avionics::gfx::*;
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    let Some(p) = PLAN.with(|x| x.borrow().clone()) else { return };
    let (w0, h0) = OSIZE.with(|s| *s.borrow());
    let k = state().borrow().scale.max(1.0);
    let (w, h) = ((w0 * k) as u32, (h0 * k) as u32);
    if w < 64 || h < 64 || p.fixes.len() < 2 {
        return;
    }
    let mut c = Canvas::new(w, h);
    let (wf, hf) = (w as f32, h as f32);
    const BG: Argb = 0xFF0B1016;
    const CY: Argb = 0xFF4FD8EB;
    const DIM: Argb = 0xFF6F7C8A;
    const FUEL: Argb = 0xFFFFB547;
    const GREEN: Argb = 0xFF3CCF7A;
    c.rect(0.0, 0.0, wf, hf, BG);
    // Pasma: chorągiewki (góra), profil, temperatury, nazwy punktów
    let (l, r) = (54.0 * k, wf - 64.0 * k);
    let barb_y = 26.0 * k;
    let t = 52.0 * k;
    let temp_h = 16.0 * k;
    let b = hf - 24.0 * k - temp_h - 6.0 * k;
    let total = p.fixes.last().map(|f| f.cum_nm).unwrap_or(1.0).max(1.0);
    let max_alt = p.fixes.iter().map(|f| f.alt_ft).max().unwrap_or(10_000).max(p.cruise_ft) as f32;
    let top = (max_alt / 5000.0).ceil() * 5000.0 + 2000.0;
    let x = |d: f64| l + (d / total) as f32 * (r - l);
    let y = |a: f32| b - (a / top).clamp(0.0, 1.0) * (b - t);
    let step = if top > 30_000.0 { 10_000 } else { 5000 };
    let mut a = 0;
    while a as f32 <= top {
        c.dash_line(l, y(a as f32), r, y(a as f32), alpha(WHITE, 0x14), k, 3.0 * k, 5.0 * k);
        c.text_f(&if a >= 10_000 { format!("FL{}", a / 100) } else { a.to_string() }, l - 6.0 * k, y(a as f32), 9.5 * k, DIM, Align::Right, Face::Mono);
        a += step;
    }
    // Paliwo (prawa oś)
    let ramp = p.fixes.iter().filter_map(|f| f.fuel).fold(p.ofp.ramp.max(p.ramp), f64::max).max(1.0);
    let fy = |f: f64| b - (f / ramp).clamp(0.0, 1.0) as f32 * (b - t);
    let fuel: Vec<f32> = p.fixes.iter().filter_map(|f| Some([x(f.cum_nm), fy(f.fuel?)])).flatten().collect();
    if fuel.len() >= 4 {
        c.polyline(alpha(FUEL, 0xC0), 1.8 * k, &fuel);
        let plain = |v: f64| weight(v, p.lbs).replace(['\u{202F}', '\u{00A0}', '\u{2009}'], " ").replace('\u{2212}', "-");
        c.text_f(&plain(ramp), r + 6.0 * k, fy(ramp), 9.0 * k, FUEL, Align::Left, Face::Mono);
        if let Some(last) = p.fixes.iter().rev().find_map(|f| f.fuel) {
            c.text_f(&plain(last), r + 6.0 * k, fy(last).min(b - 6.0 * k), 9.0 * k, alpha(FUEL, 0xC0), Align::Left, Face::Mono);
        }
    }
    // Wysokość: wypełnienie i linia w kolorze etapu
    let mut poly = vec![x(0.0), b];
    for f in &p.fixes {
        poly.push(x(f.cum_nm));
        poly.push(y(f.alt_ft as f32));
    }
    poly.push(x(total));
    poly.push(b);
    c.poly(alpha(CY, 0x20), 0, 0.0, &poly);
    for wv in p.fixes.windows(2) {
        let col = match wv[1].stage.as_str() {
            "CLB" => GREEN,
            "DSC" => 0xFFE07CFF,
            _ => CY,
        };
        c.line(x(wv[0].cum_nm), y(wv[0].alt_ft as f32), x(wv[1].cum_nm), y(wv[1].alt_ft as f32), col, 2.4 * k);
    }
    // Co który punkt – tyle, ile się zmieści
    let every = ((p.fixes.len() as f32 * 34.0 * k) / (r - l)).ceil().max(1.0) as usize;
    let temp_top = b + 6.0 * k;
    for (i, f) in p.fixes.iter().enumerate() {
        let (px, py) = (x(f.cum_nm), y(f.alt_ft as f32));
        // Pasek temperatury: odchyłka ISA od niebieskiego (zimno) do czerwonego (ciepło)
        if let (Some(dev), Some(next)) = (f.isa_dev, p.fixes.get(i + 1)) {
            let tcol = if dev >= 0.0 {
                let a = ((dev / 15.0).clamp(0.0, 1.0) * 200.0) as u32 + 40;
                (a << 24) | 0x00FF_5A4F
            } else {
                let a = ((-dev / 15.0).clamp(0.0, 1.0) * 200.0) as u32 + 40;
                (a << 24) | 0x004F_A8FF
            };
            c.rect(px, temp_top, x(next.cum_nm), temp_top + temp_h, tcol);
        }
        if f.ident == "TOC" || f.ident == "TOD" {
            c.circle(px, py, 4.0 * k, GREEN, 1.6 * k);
            c.text_f(if f.ident == "TOC" { "T/C" } else { "T/D" }, px, py - 11.0 * k, 10.0 * k, GREEN, Align::Center, Face::CondBold);
        } else {
            c.circle(px, py, 2.2 * k, CY, 0.0);
        }
        if i % every == 0 || i + 1 == p.fixes.len() {
            c.text_f(&f.ident, px, hf - 10.0 * k, 9.0 * k, DIM, Align::Center, Face::Mono);
            if let (Some(d), Some(s)) = (f.wind_dir, f.wind_spd) {
                barb(&mut c, px, barb_y + 10.0 * k, d, s, k, 0xFFE8F6F8);
                c.text_f(&format!("{s:.0}"), px, barb_y + 24.0 * k, 8.5 * k, DIM, Align::Center, Face::Mono);
            }
            if let Some(o) = f.oat {
                c.text_f(&format!("{o:.0}"), px, temp_top + temp_h / 2.0, 8.5 * k, 0xFFFFFFFF, Align::Center, Face::Mono);
            }
        }
    }
    c.text_f(tr("WIATR"), 6.0 * k, barb_y + 10.0 * k, 8.5 * k, DIM, Align::Left, Face::CondBold);
    c.text_f("OAT", 6.0 * k, temp_top + temp_h / 2.0, 8.5 * k, DIM, Align::Left, Face::CondBold);
    c.text_f(tr("PALIWO"), wf - 6.0 * k, t - 6.0 * k, 8.5 * k, alpha(FUEL, 0xC0), Align::Right, Face::CondBold);
    app.set_ofp_profile(c.into_image());
}

/// Dane z OFP do kalkulatora osiągów: [typ, masa kg, ICAO, pas, OAT, QNH, kierunek wiatru, prędkość].
pub fn perf_inputs(takeoff: bool) -> Vec<String> {
    let Some(p) = PLAN.with(|x| x.borrow().clone()) else { return Vec::new() };
    let o = &p.ofp;
    let (apt, wgt) = if takeoff { (&o.origin, o.tow) } else { (&o.dest, o.ldw) };
    let (wd, ws, t, q) = metar_parts(&apt.metar);
    let text = format!("{} {}", p.aircraft, o.icao_type);
    let idx = crate::perf::PROFILES.iter().position(|pr| regex::RegexBuilder::new(pr.pattern).case_insensitive(true).build().is_ok_and(|r| r.is_match(&text)));
    let f = |v: Option<f64>, d: &str| v.map(|x| format!("{x:.0}")).unwrap_or_else(|| d.to_string());
    vec![
        idx.map(|i| i.to_string()).unwrap_or_default(),
        if wgt > 0.0 { format!("{:.0}", kg_of(wgt, p.lbs)) } else { String::new() },
        if apt.icao.is_empty() { if takeoff { p.origin.clone() } else { p.dest.clone() } } else { apt.icao.clone() },
        if apt.rwy.is_empty() { if takeoff { p.dep_rwy.clone() } else { p.arr_rwy.clone() } } else { apt.rwy.clone() },
        f(t, "15"),
        f(q, "1013"),
        f(wd, "0"),
        f(ws, "0"),
    ]
}

/// Prędkości TLR z OFP do checklisty („to|…”, „ldg|…”); pusta lista bez TLR.
pub fn tlr_for_checklist() -> Vec<String> {
    let Some(p) = PLAN.with(|x| x.borrow().clone()) else { return Vec::new() };
    let mut out = Vec::new();
    if let Some(t) = &p.ofp.tlr_to {
        out.push(format!("to|{}", tlr_text(t, true)));
    }
    if let Some(t) = &p.ofp.tlr_ldg {
        out.push(format!("ldg|{}", tlr_text(t, false)));
    }
    out
}
