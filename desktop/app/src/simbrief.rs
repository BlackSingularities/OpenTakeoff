//! Plan lotu z SimBrief: pobranie ostatniego planu użytkownika (API xml.fetcher, JSON), zapis na
//! dysku (plan zostaje po ponownym uruchomieniu), trasa na mapie i postęp lotu względem planu –
//! następny punkt, odległość do celu, paliwo planowane i rzeczywiste.

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
    };
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
    crate::navmap::render(true);
}

/// Trasa planu do mapy: (szer., dł., wysokość ft, nazwa punktu).
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
    app.set_plan(v);
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
