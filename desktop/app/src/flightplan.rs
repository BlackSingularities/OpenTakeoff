//! Karta Plan lotu: edytowalna trasa (punkty, drogi lotnicze), procedury SID / STAR / podejścia z bazy
//! symulatora, profil pionowy, mapa i eksport do MSFS (.pln) i X-Plane (.fms).
//!
//! Źródła danych nawigacyjnych, w kolejności:
//! 1. pliki X-Plane (wskazany folder: punkty, VOR/NDB, drogi lotnicze, CIFP) – działa bez symulatora,
//! 2. MSFS na żywo przez SimConnect (gdy symulator działa) – pobrane procedury i punkty zapisujemy na dysku,
//! 3. zapisane wcześniej dane z MSFS, baza OurAirports (lotniska, VOR, NDB) i punkty z planów SimBrief.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use opentakeoff_core::aero::{airports, navaids};
use opentakeoff_core::lang::tr;
use opentakeoff_core::navdata::{math, AirportProcedures, FixResult, LegInfo, NavDataProvider};
use opentakeoff_core::xpnav::{self, XPlaneNavData};
use opentakeoff_core::{log, paths, trf};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::app::{later, state, toast, ui};
use crate::avionics::gfx::*;
use crate::map::{self, Overlay, Point, Track, View};
use crate::*;

pub const MAP: i32 = 40;
pub const PROFILE: i32 = 41;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Wpt {
    pub ident: String,
    /// apt, wpt, vor, ndb, rwy, latlon
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
    /// DCT albo nazwa drogi lotniczej, którą dochodzimy do punktu.
    pub via: String,
    /// Wysokość wymagana / planowana w punkcie (ft) – tylko trasa.
    pub alt: Option<i32>,
    pub region: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct FPlan {
    pub origin: String,
    pub dest: String,
    pub altn: String,
    pub dep_rwy: String,
    pub sid: String,
    pub sid_trans: String,
    pub star: String,
    pub star_trans: String,
    pub arr_rwy: String,
    pub app: String,
    pub app_trans: String,
    pub cruise_ft: i32,
    pub tas_kt: i32,
    pub burn_kgh: f64,
    /// Paliwo: rezerwa końcowa (min), kołowanie (kg), dodatkowe (kg), rezerwa trasy (% paliwa trasy).
    pub reserve_min: i32,
    pub taxi_kg: f64,
    pub extra_kg: f64,
    pub cont_pct: f64,
    pub enroute: Vec<Wpt>,
}

impl Default for FPlan {
    fn default() -> Self {
        Self {
            origin: String::new(),
            dest: String::new(),
            altn: String::new(),
            dep_rwy: String::new(),
            sid: String::new(),
            sid_trans: String::new(),
            star: String::new(),
            star_trans: String::new(),
            arr_rwy: String::new(),
            app: String::new(),
            app_trans: String::new(),
            cruise_ft: 24_000,
            tas_kt: 420,
            burn_kgh: 2400.0,
            reserve_min: 30,
            taxi_kg: 200.0,
            extra_kg: 0.0,
            cont_pct: 5.0,
            enroute: Vec::new(),
        }
    }
}

/// Punkt trasy wyliczonej (z procedurami) do listy, mapy i profilu.
#[derive(Clone, Debug, Default)]
pub struct Leg {
    pub ident: String,
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
    pub via: String,
    /// ORIG, SID, ENR, STAR, APP, DEST
    pub seg: &'static str,
    pub constraint: String,
    pub c_min: Option<f64>,
    pub c_max: Option<f64>,
    pub speed: Option<f64>,
    /// Indeks w `enroute` (tylko punkty trasy są edytowalne).
    pub enroute_idx: Option<usize>,
    pub dist: f64,
    pub cum: f64,
    pub crs: f64,
    pub alt_plan: f64,
}

struct St {
    plan: FPlan,
    procs: HashMap<String, Arc<AirportProcedures>>,
    loading: Vec<String>,
    xp: Option<Arc<XPlaneNavData>>,
    xp_for: String,
    view: View,
    fitted: bool,
    sel: i32,
    legs: Vec<Leg>,
    sizes: HashMap<i32, (f32, f32)>,
}

thread_local! {
    static ST: RefCell<Option<St>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R {
    ST.with(|s| {
        let mut s = s.borrow_mut();
        let st = s.get_or_insert_with(|| St {
            plan: load_plan(),
            procs: HashMap::new(),
            loading: Vec::new(),
            xp: None,
            xp_for: String::new(),
            view: View::default(),
            fitted: false,
            sel: -1,
            legs: Vec::new(),
            sizes: HashMap::new(),
        });
        f(st)
    })
}

fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

// ---------- pliki ----------

fn plan_path() -> PathBuf {
    paths::config().join("flightplan.json")
}

fn cache_dir() -> PathBuf {
    let d = paths::config().join("navcache");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn load_plan() -> FPlan {
    std::fs::read_to_string(plan_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_plan(p: &FPlan) {
    if let Ok(t) = serde_json::to_string_pretty(p) {
        let _ = std::fs::write(plan_path(), t);
    }
}

/// Punkty zapamiętane z MSFS i SimBrief: ident → (lat, lon, rodzaj, region).
fn fix_cache() -> HashMap<String, Vec<(f64, f64, String, String)>> {
    std::fs::read_to_string(cache_dir().join("fixes.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn remember_fixes(list: &[(String, f64, f64, String, String)]) {
    if list.is_empty() {
        return;
    }
    let mut c = fix_cache();
    for (id, la, lo, k, r) in list {
        let v = c.entry(id.clone()).or_default();
        if !v.iter().any(|x| math::distance_nm(x.0, x.1, *la, *lo) < 1.0) {
            v.push((*la, *lo, k.clone(), r.clone()));
        }
    }
    if let Ok(t) = serde_json::to_string(&c) {
        let _ = std::fs::write(cache_dir().join("fixes.json"), t);
    }
}

fn cached_procs(icao: &str) -> Option<AirportProcedures> {
    std::fs::read_to_string(cache_dir().join(format!("{icao}.json"))).ok().and_then(|t| serde_json::from_str(&t).ok())
}

// ---------- źródła ----------

/// Dane X-Plane z folderu w ustawieniach (albo wykrytego automatycznie).
fn xp() -> Option<Arc<XPlaneNavData>> {
    let path = state().borrow().s.xplane_path.clone();
    let root = if path.is_empty() { xpnav::detect().into_iter().next() } else { Some(PathBuf::from(&path)).filter(|p| xpnav::is_root(p)) };
    let key = root.as_ref().map(|r| r.display().to_string()).unwrap_or_default();
    with(|s| {
        if s.xp_for != key {
            s.xp_for = key.clone();
            s.xp = root.map(|r| Arc::new(XPlaneNavData::new(r)));
        }
        s.xp.clone()
    })
}

fn live() -> Option<(Arc<dyn NavDataProvider>, &'static str, Arc<opentakeoff_core::navdata::NavDataService>)> {
    let st = state();
    let host = st.borrow().host.clone();
    let src = host.source();
    let p = src.navdata().filter(|_| src.is_connected())?;
    Some((p, src.kind(), host.nav.clone()))
}

fn source_text() -> String {
    if let Some(x) = xp() {
        let c = x.cycle();
        return if c.is_empty() { trf!("Dane: X-Plane ({p})", p = x.root().display()) } else { trf!("Dane: X-Plane, AIRAC {c}") };
    }
    match live() {
        Some((_, k, _)) if k == "msfs" => tr("Dane: MSFS na żywo (zapisywane na później)").to_string(),
        Some(_) => tr("Dane: tryb symulacji (przykładowe procedury EPWA, EPGD)").to_string(),
        None => tr("Dane: zapisane z MSFS i OurAirports – uruchom MSFS albo wskaż folder X-Plane, by mieć procedury i drogi lotnicze").to_string(),
    }
}

/// Procedury lotniska w tle: X-Plane → MSFS na żywo (zapis na dysk) → zapis z dysku.
fn fetch_procs(icao: &str) {
    if icao.len() < 3 || with(|s| s.procs.contains_key(icao) || s.loading.iter().any(|l| l == icao)) {
        return;
    }
    with(|s| s.loading.push(icao.to_string()));
    let xp = xp();
    let live = live();
    let icao = icao.to_string();
    std::thread::spawn(move || {
        let res: Option<AirportProcedures> = if let Some(x) = xp {
            x.procedures(&icao).ok()
        } else if let Some((p, kind, nav)) = live {
            match nav.procedures(p.as_ref(), kind, &icao) {
                Ok(a) => {
                    if kind == "msfs" {
                        if let Ok(t) = serde_json::to_string(a.as_ref()) {
                            let _ = std::fs::write(cache_dir().join(format!("{icao}.json")), t);
                        }
                        // punkty z procedur też na później
                        let pts: Vec<_> = a
                            .departures
                            .iter()
                            .chain(&a.arrivals)
                            .flat_map(|d| d.common_legs.iter().chain(d.runway_transitions.iter().flat_map(|t| &t.legs)).chain(d.enroute_transitions.iter().flat_map(|t| &t.legs)))
                            .filter_map(|l| Some((l.fix.clone(), l.lat?, l.lon?, "wpt".to_string(), String::new())))
                            .filter(|p| !p.0.is_empty() && !p.0.starts_with("RW"))
                            .collect();
                        remember_fixes(&pts);
                    }
                    Some((*a).clone())
                }
                Err(e) => {
                    log::warn(&format!("Procedury {icao}: {}", e.message));
                    cached_procs(&icao)
                }
            }
        } else {
            cached_procs(&icao)
        };
        later(move || {
            with(|s| {
                s.loading.retain(|l| l != &icao);
                if let Some(p) = res {
                    s.procs.insert(icao.clone(), Arc::new(p));
                }
            });
            publish();
        });
    });
}

/// Kandydaci na punkt o danym identyfikatorze ze wszystkich źródeł offline (+ X-Plane).
fn candidates(ident: &str, xp: Option<&XPlaneNavData>) -> Vec<FixResult> {
    let mut out = Vec::new();
    if let Some(x) = xp {
        out.extend(x.find_fix(ident).unwrap_or_default());
    }
    if let Some(db) = navaids::db() {
        for n in db.find(ident) {
            out.push(FixResult { ident: n.ident.clone(), kind: if n.kind.is_ndb() { "ndb".into() } else { "vor".into() }, lat: n.lat, lon: n.lon, freq_mhz: Some(n.freq), region: None });
        }
    }
    if ident.len() == 4 {
        if let Some(a) = airports::db().and_then(|d| d.get(ident).cloned()) {
            out.push(FixResult { ident: a.ident.clone(), kind: "airport".into(), lat: a.lat, lon: a.lon, freq_mhz: None, region: None });
        }
    }
    for (la, lo, k, r) in fix_cache().remove(ident).unwrap_or_default() {
        out.push(FixResult { ident: ident.to_string(), kind: k, lat: la, lon: lo, freq_mhz: None, region: Some(r) });
    }
    out
}

/// Współrzędne „5230N02000E” / „52N020E”.
fn parse_latlon(t: &str) -> Option<(f64, f64)> {
    let re = regex::Regex::new(r"^(\d{2})(\d{2})?([NS])(\d{3})(\d{2})?([EW])$").ok()?;
    let c = re.captures(t)?;
    let lat = c[1].parse::<f64>().ok()? + c.get(2).map(|m| m.as_str().parse::<f64>().unwrap_or(0.0) / 60.0).unwrap_or(0.0);
    let lon = c[4].parse::<f64>().ok()? + c.get(5).map(|m| m.as_str().parse::<f64>().unwrap_or(0.0) / 60.0).unwrap_or(0.0);
    Some((if &c[3] == "S" { -lat } else { lat }, if &c[6] == "W" { -lon } else { lon }))
}

fn kind_of(k: &str) -> String {
    match k {
        "vor" => "vor",
        "ndb" => "ndb",
        "airport" => "apt",
        _ => "wpt",
    }
    .to_string()
}

/// Rozwiązanie identyfikatora na punkt najbliższy poprzedniemu (wątek roboczy; MSFS na żywo, gdy działa).
fn resolve(ident: &str, near: (f64, f64), xp: Option<&XPlaneNavData>, live: Option<&Arc<dyn NavDataProvider>>) -> Option<Wpt> {
    let id = ident.trim().to_uppercase();
    if let Some((la, lo)) = parse_latlon(&id) {
        return Some(Wpt { ident: id, kind: "latlon".into(), lat: la, lon: lo, via: "DCT".into(), ..Default::default() });
    }
    let mut c = candidates(&id, xp);
    if c.is_empty() {
        if let Some(p) = live {
            let found = p.find_fix(&id).unwrap_or_default();
            remember_fixes(&found.iter().map(|f| (f.ident.clone(), f.lat, f.lon, kind_of(&f.kind), f.region.clone().unwrap_or_default())).collect::<Vec<_>>());
            c = found;
        }
    }
    c.into_iter()
        .min_by(|a, b| math::distance_nm(near.0, near.1, a.lat, a.lon).total_cmp(&math::distance_nm(near.0, near.1, b.lat, b.lon)))
        .map(|f| Wpt { ident: id.clone(), kind: kind_of(&f.kind), lat: f.lat, lon: f.lon, via: "DCT".into(), alt: None, region: f.region.unwrap_or_default() })
}

/// Paliwo planu (kg) – bez SimBriefa, z TAS i spalania godzinowego.
pub struct Fuel {
    pub taxi: f64,
    pub trip: f64,
    pub cont: f64,
    pub altn: f64,
    pub altn_nm: f64,
    pub reserve: f64,
    pub extra: f64,
}

impl Fuel {
    /// Minimum do startu (bez kołowania).
    pub fn takeoff(&self) -> f64 {
        self.trip + self.cont + self.altn + self.reserve + self.extra
    }
    pub fn block(&self) -> f64 {
        self.takeoff() + self.taxi
    }
}

pub fn fuel_plan(p: &FPlan, total_nm: f64) -> Fuel {
    let tas = p.tas_kt.max(60) as f64;
    let trip = total_nm / tas * p.burn_kgh;
    let altn_nm = match (apt_pos(&p.dest), apt_pos(&p.altn)) {
        (Some(a), Some(b)) if !p.altn.is_empty() => math::distance_nm(a.0, a.1, b.0, b.1),
        _ => 0.0,
    };
    // Lot na zapasowe niżej i wolniej (×0,8 TAS) plus nieudane podejście (12 min)
    let altn = if altn_nm > 0.0 { (altn_nm / (tas * 0.8) + 0.2) * p.burn_kgh } else { 0.0 };
    // Rezerwa końcowa w oczekiwaniu – spalanie ok. 85% przelotowego
    let reserve = p.reserve_min.max(0) as f64 / 60.0 * p.burn_kgh * 0.85;
    Fuel { taxi: p.taxi_kg.max(0.0), trip, cont: trip * p.cont_pct.max(0.0) / 100.0, altn, altn_nm, reserve, extra: p.extra_kg.max(0.0) }
}

/// Propozycje lotnisk zapasowych: duże i średnie lotniska do 200 NM od celu z pasem nie krótszym
/// niż ¾ najdłuższego pasa celu (najwyżej 6000 ft), najbliższe najpierw.
pub fn alternates(p: &FPlan) -> Vec<(String, f64, i32)> {
    let Some(db) = airports::db() else { return Vec::new() };
    let Some(d) = db.get(&p.dest) else { return Vec::new() };
    let need = (d.longest_runway_ft() as f64 * 0.75).min(6000.0);
    let mut v: Vec<(String, f64, i32)> = db
        .within(d.lat, d.lon, 200.0, false)
        .into_iter()
        .filter(|(_, a)| a.ident != d.ident && a.ident != p.origin && a.kind <= 1 && a.longest_runway_ft() as f64 >= need)
        .map(|(dist, a)| (a.ident.clone(), dist, a.longest_runway_ft()))
        .collect();
    v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    v.truncate(5);
    v
}

fn apt_pos(icao: &str) -> Option<(f64, f64, f64)> {
    airports::db().and_then(|d| d.get(icao).map(|a| (a.lat, a.lon, a.elev_ft.unwrap_or(0) as f64)))
}

// ---------- trasa z tekstu ----------

/// „EPWA BAMS1G BAMSO UL29 ABCDE DCT XYZ EPKK” → plan (w wątku; lotniska i procedury rozpoznane po nazwie).
fn parse_route(text: &str, mut plan: FPlan, procs: HashMap<String, Arc<AirportProcedures>>) -> (FPlan, Vec<String>) {
    let xp = xp_cached();
    let live = live_cached();
    let mut toks: Vec<String> = text.split_whitespace().map(|t| t.to_uppercase()).filter(|t| t != "DCT").collect();
    let mut missing = Vec::new();
    let is_apt = |t: &str| t.len() == 4 && airports::db().is_some_and(|d| d.get(t).is_some());
    if toks.first().is_some_and(|t| is_apt(t)) {
        plan.origin = toks.remove(0);
    }
    if toks.last().is_some_and(|t| is_apt(t)) {
        plan.dest = toks.pop().unwrap_or_default();
    }
    // SID / STAR z nazwy (np. BAMS1G) – tylko gdy znamy procedury lotniska
    if let Some(p) = procs.get(&plan.origin) {
        if let Some(t) = toks.first().cloned() {
            let base = t.split('.').next().unwrap_or(&t).to_string();
            if p.departures.iter().any(|d| d.name == base) {
                plan.sid = base;
                plan.sid_trans.clear();
                toks.remove(0);
            }
        }
    }
    if let Some(p) = procs.get(&plan.dest) {
        if let Some(t) = toks.last().cloned() {
            let base = t.split('.').next().unwrap_or(&t).to_string();
            if p.arrivals.iter().any(|d| d.name == base) {
                plan.star = base;
                plan.star_trans.clear();
                toks.pop();
            }
        }
    }
    let mut route: Vec<Wpt> = Vec::new();
    let mut near = apt_pos(&plan.origin).map(|p| (p.0, p.1)).unwrap_or((52.0, 20.0));
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        let airway = xp.as_ref().is_some_and(|x| x.has_airway(t)) && !route.is_empty() && i + 1 < toks.len();
        if airway {
            let from = route.last().map(|w| w.ident.clone()).unwrap_or_default();
            let to = toks[i + 1].clone();
            if let Some(path) = xp.as_ref().and_then(|x| x.airway_path(t, &from, &to)) {
                for (id, reg) in path {
                    if let Some(f) = xp.as_ref().and_then(|x| x.fix_at(&id, &reg)) {
                        near = (f.lat, f.lon);
                        route.push(Wpt { ident: id, kind: kind_of(&f.kind), lat: f.lat, lon: f.lon, via: t.clone(), alt: None, region: reg });
                    }
                }
                i += 2;
                continue;
            }
        }
        match resolve(t, near, xp.as_deref(), live.as_ref()) {
            Some(w) => {
                near = (w.lat, w.lon);
                route.push(w);
            }
            None => missing.push(t.clone()),
        }
        i += 1;
    }
    plan.enroute = route;
    (plan, missing)
}

// Wątki robocze nie mają dostępu do stanu UI – przekazujemy źródła przez zmienne globalne.
static XP_SHARED: parking_lot::Mutex<Option<Arc<XPlaneNavData>>> = parking_lot::Mutex::new(None);
static LIVE_SHARED: parking_lot::Mutex<Option<Arc<dyn NavDataProvider>>> = parking_lot::Mutex::new(None);

fn xp_cached() -> Option<Arc<XPlaneNavData>> {
    XP_SHARED.lock().clone()
}

fn live_cached() -> Option<Arc<dyn NavDataProvider>> {
    LIVE_SHARED.lock().clone()
}

fn share_sources() {
    *XP_SHARED.lock() = xp();
    *LIVE_SHARED.lock() = live().map(|l| l.0);
}

// ---------- wyliczenie trasy ----------

fn constraint(l: &LegInfo) -> (String, Option<f64>, Option<f64>) {
    let fmt = |v: f64| if v >= 18_000.0 { format!("FL{:03.0}", v / 100.0) } else { format!("{v:.0}") };
    // MSFS / tryb symulacji: AT, ABOVE, BELOW, BETWEEN; X-Plane CIFP: + - B
    let d = match l.alt_desc.as_deref().unwrap_or("").trim() {
        "ABOVE" | "+" => "+",
        "BELOW" | "-" => "-",
        "BETWEEN" | "B" => "B",
        _ => "",
    };
    match (d, l.alt1_ft, l.alt2_ft) {
        ("+", Some(a), _) => (format!("+{}", fmt(a)), Some(a), None),
        ("-", Some(a), _) => (format!("-{}", fmt(a)), None, Some(a)),
        ("B", Some(a), Some(b)) => (format!("{}/{}", fmt(a.max(b)), fmt(a.min(b))), Some(a.min(b)), Some(a.max(b))),
        (_, Some(a), _) => (fmt(a), Some(a), Some(a)),
        _ => (String::new(), None, None),
    }
}

fn proc_legs(procs: Option<&Arc<AirportProcedures>>, kind: &str, name: &str, rwy: &str, trans: &str) -> Vec<LegInfo> {
    if name.is_empty() {
        return Vec::new();
    }
    let Some(p) = procs else { return Vec::new() };
    let rw = (!rwy.is_empty()).then_some(rwy);
    let tr = (!trans.is_empty()).then_some(trans);
    p.legs(kind, name, if kind == "approach" { None } else { rw }, tr).map(|l| l.legs).unwrap_or_default()
}

fn compute(s: &St) -> Vec<Leg> {
    let p = &s.plan;
    let mut legs: Vec<Leg> = Vec::new();
    // ten sam punkt na styku części (trasa → STAR, STAR → podejście): zostaw jeden, z danymi procedury
    let push = |l: Leg, legs: &mut Vec<Leg>| {
        if let Some(x) = legs.last_mut().filter(|x| x.ident == l.ident && math::distance_nm(x.lat, x.lon, l.lat, l.lon) < 30.0) {
            if l.seg != "ENR" {
                let (via, seg, eidx) = (x.via.clone(), x.seg, x.enroute_idx);
                *x = l;
                if seg == "ENR" {
                    x.via = via;
                    x.seg = seg;
                    x.enroute_idx = eidx;
                }
            }
            return;
        }
        legs.push(l);
    };
    let o = apt_pos(&p.origin);
    let d = apt_pos(&p.dest);
    if let Some((la, lo, _)) = o {
        push(Leg { ident: p.origin.clone(), kind: "apt".into(), lat: la, lon: lo, seg: "ORIG", ..Default::default() }, &mut legs);
    }
    let from_proc = |l: &LegInfo, seg: &'static str, via: &str| -> Option<Leg> {
        let (c, mn, mx) = constraint(l);
        Some(Leg { ident: if l.fix.is_empty() { l.leg_type.clone() } else { l.fix.clone() }, kind: if l.fix.starts_with("RW") { "rwy".into() } else { "wpt".into() }, lat: l.lat?, lon: l.lon?, via: via.to_string(), seg, constraint: c, c_min: mn, c_max: mx, speed: l.speed_kt, ..Default::default() })
    };
    for l in proc_legs(s.procs.get(&p.origin), "departure", &p.sid, &p.dep_rwy, &p.sid_trans) {
        if let Some(x) = from_proc(&l, "SID", &p.sid) {
            push(x, &mut legs);
        }
    }
    for (i, w) in p.enroute.iter().enumerate() {
        let (c, mn, mx) = match w.alt {
            Some(a) => (if a >= 18_000 { format!("FL{:03}", a / 100) } else { a.to_string() }, Some(a as f64), Some(a as f64)),
            None => (String::new(), None, None),
        };
        push(Leg { ident: w.ident.clone(), kind: w.kind.clone(), lat: w.lat, lon: w.lon, via: w.via.clone(), seg: "ENR", constraint: c, c_min: mn, c_max: mx, enroute_idx: Some(i), ..Default::default() }, &mut legs);
    }
    for l in proc_legs(s.procs.get(&p.dest), "arrival", &p.star, &p.arr_rwy, &p.star_trans) {
        if let Some(x) = from_proc(&l, "STAR", &p.star) {
            push(x, &mut legs);
        }
    }
    // podejście: gdy jego pierwszy punkt jest już na trasie (koniec STAR), utnij trasę do niego – bez zawracania
    let app_legs: Vec<Leg> = proc_legs(s.procs.get(&p.dest), "approach", &p.app, &p.arr_rwy, &p.app_trans).iter().filter_map(|l| from_proc(l, "APP", &p.app)).collect();
    if let Some(first) = app_legs.first() {
        if let Some(pos) = legs.iter().position(|x| x.ident == first.ident && x.seg != "ORIG") {
            legs.truncate(pos);
        }
    }
    for x in app_legs {
        push(x, &mut legs);
    }
    if let Some((la, lo, _)) = d {
        push(Leg { ident: p.dest.clone(), kind: "apt".into(), lat: la, lon: lo, seg: "DEST", ..Default::default() }, &mut legs);
    }
    // odległości i kursy
    let mut cum = 0.0;
    for i in 0..legs.len() {
        if i > 0 {
            let (a, b) = (&legs[i - 1], &legs[i]);
            let dist = math::distance_nm(a.lat, a.lon, b.lat, b.lon);
            let crs = math::bearing_deg(a.lat, a.lon, b.lat, b.lon);
            cum += dist;
            legs[i].dist = dist;
            legs[i].crs = crs;
        }
        legs[i].cum = cum;
    }
    // profil: wznoszenie ~300 ft/NM, zniżanie 3° (318 ft/NM), ograniczenia procedur
    let (e0, e1) = (o.map(|x| x.2).unwrap_or(0.0), d.map(|x| x.2).unwrap_or(0.0));
    let total = cum.max(1.0);
    let cruise = p.cruise_ft as f64;
    for l in legs.iter_mut() {
        let climb = e0 + l.cum * 300.0;
        let desc = e1 + (total - l.cum) * 318.0;
        let mut a = climb.min(desc).min(cruise);
        if let Some(mn) = l.c_min {
            a = a.max(mn);
        }
        if let Some(mx) = l.c_max {
            a = a.min(mx);
        }
        l.alt_plan = a;
    }
    if let Some(f) = legs.first_mut() {
        f.alt_plan = e0;
    }
    if let Some(l) = legs.last_mut() {
        l.alt_plan = e1;
    }
    legs
}

/// Trasa planu do mapy nawigacyjnej (2D i 3D): (lat, lon, planowana wysokość, nazwa).
pub fn route_points() -> Vec<(f64, f64, i32, String)> {
    with(|s| {
        if s.legs.is_empty() {
            s.legs = compute(s);
        }
        s.legs.iter().map(|l| (l.lat, l.lon, l.alt_plan as i32, l.ident.clone())).collect()
    })
}

/// Trasa do mapy nawigacyjnej: edytowany plan (gdy ma co najmniej dwa punkty), inaczej plan SimBrief.
pub fn active_route() -> Vec<(f64, f64, i32, String)> {
    let r = route_points();
    if r.len() > 1 {
        r
    } else {
        crate::simbrief::route()
    }
}

// ---------- interfejs ----------

pub fn refresh() {
    share_sources();
    let (o, d) = with(|s| (s.plan.origin.clone(), s.plan.dest.clone()));
    fetch_procs(&o);
    fetch_procs(&d);
    publish();
}

pub(crate) fn fmt_alt(a: f64) -> String {
    if a >= 18_000.0 {
        format!("FL{:03.0}", a / 100.0)
    } else {
        format!("{:.0}", (a / 10.0).round() * 10.0)
    }
}

pub(crate) fn hm(h: f64) -> String {
    let m = (h * 60.0).round() as i64;
    format!("{}:{:02}", m / 60, m % 60)
}

pub fn publish() {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    let source = source_text();
    with(|s| {
        s.legs = compute(s);
        let p = s.plan.clone();
        app.set_plan_origin(ss(&p.origin));
        app.set_plan_dest(ss(&p.dest));
        app.set_plan_cruise(ss(format!("FL{:03}", p.cruise_ft / 100)));
        app.set_plan_tas(ss(p.tas_kt.to_string()));
        app.set_plan_source(ss(source));
        let tas = (p.tas_kt.max(60)) as f64;
        let rows: Vec<PlanRowV> = s
            .legs
            .iter()
            .enumerate()
            .map(|(i, l)| PlanRowV {
                idx: i as i32,
                ident: ss(&l.ident),
                via: ss(match l.seg {
                    "ORIG" | "DEST" => l.seg.to_string(),
                    "ENR" => l.via.clone(),
                    _ => l.via.clone(),
                }),
                kind: ss(&l.kind),
                seg: ss(l.seg),
                constraint: ss(&l.constraint),
                alt: ss(fmt_alt(l.alt_plan)),
                crs: ss(if i == 0 { String::new() } else { format!("{:03.0}°", l.crs) }),
                dist: ss(if i == 0 { String::new() } else { format!("{:.0}", l.dist) }),
                cum: ss(format!("{:.0}", l.cum)),
                ete: ss(hm(l.cum / tas)),
                editable: l.enroute_idx.is_some(),
                selected: s.sel == i as i32,
            })
            .collect();
        app.set_plan_rows(model(rows));
        let total = s.legs.last().map(|l| l.cum).unwrap_or(0.0);
        let hours = total / tas;
        let (e0, e1) = (apt_pos(&p.origin).map(|x| x.2).unwrap_or(0.0), apt_pos(&p.dest).map(|x| x.2).unwrap_or(0.0));
        let toc = ((p.cruise_ft as f64 - e0) / 300.0).min(total);
        let tod = (total - (p.cruise_ft as f64 - e1) / 318.0).max(0.0);
        let kv = |k: &str, v: String| KV { k: ss(k), v: ss(v) };
        let fuel = fuel_plan(&p, total);
        app.set_plan_summary(model(vec![
            kv(tr("Dystans"), format!("{total:.0} NM")),
            kv(tr("Czas lotu"), hm(hours)),
            kv(tr("Paliwo blok"), format!("{:.0} kg", fuel.block())),
            kv("TOC", format!("{toc:.0} NM · {}", hm(toc / tas))),
            kv("TOD", trf!("{d} NM przed celem", d = format!("{:.0}", (total - tod).max(0.0)))),
        ]));
        let kg = |v: f64| format!("{v:.0} kg");
        app.set_plan_fuel(model(vec![
            kv(tr("Kołowanie"), kg(fuel.taxi)),
            kv(tr("Trasa"), kg(fuel.trip)),
            kv(&trf!("Rezerwa trasy {p}%", p = format!("{:.0}", p.cont_pct)), kg(fuel.cont)),
            kv(&if p.altn.is_empty() { tr("Zapasowe").to_string() } else { format!("{} {} · {:.0} NM", tr("Zapasowe"), p.altn, fuel.altn_nm) }, kg(fuel.altn)),
            kv(&trf!("Rezerwa końcowa {m} min", m = p.reserve_min), kg(fuel.reserve)),
            kv(tr("Dodatkowe"), kg(fuel.extra)),
            kv(tr("Minimum do startu"), kg(fuel.takeoff())),
            kv(tr("Blok"), kg(fuel.block())),
        ]));
        app.set_plan_altn(ss(&p.altn));
        app.set_plan_burn(ss(format!("{:.0}", p.burn_kgh)));
        app.set_plan_reserve(ss(p.reserve_min.to_string()));
        app.set_plan_taxi(ss(format!("{:.0}", p.taxi_kg)));
        app.set_plan_extra(ss(format!("{:.0}", p.extra_kg)));
        app.set_plan_altn_opts(model(
            alternates(&p).into_iter().map(|(id, d, len)| PlanOptV { selected: id == p.altn, sub: ss(format!("{d:.0} NM · {:.0} m", len as f64 * 0.3048)), name: ss(id) }).collect(),
        ));
        // procedury do wyboru
        let opt = |name: &str, sub: String, sel: bool| PlanOptV { name: ss(name), sub: ss(sub), selected: sel };
        let po = s.procs.get(&p.origin).cloned();
        let pd = s.procs.get(&p.dest).cloned();
        let rwys = |icao: &str| -> Vec<String> { airports::db().and_then(|d| d.get(icao).map(|a| a.runways.iter().filter(|r| !r.closed).flat_map(|r| [r.le.clone(), r.he.clone()]).filter(|x| !x.is_empty()).collect())).unwrap_or_default() };
        let tab = app.get_plan_proc_tab();
        let (a, b, c): (Vec<PlanOptV>, Vec<PlanOptV>, Vec<PlanOptV>) = match tab {
            0 => {
                let a = rwys(&p.origin).into_iter().map(|r| opt(&r, String::new(), r == p.dep_rwy)).collect();
                let b = po
                    .as_ref()
                    .map(|x| {
                        x.departures
                            .iter()
                            .filter(|d| p.dep_rwy.is_empty() || d.runway_transitions.is_empty() || d.runway_transitions.iter().any(|t| t.name == p.dep_rwy))
                            .map(|d| opt(&d.name, d.runway_transitions.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(" "), d.name == p.sid))
                            .collect()
                    })
                    .unwrap_or_default();
                let c = po.as_ref().and_then(|x| x.departures.iter().find(|d| d.name == p.sid)).map(|d| d.enroute_transitions.iter().map(|t| opt(&t.name, String::new(), t.name == p.sid_trans)).collect()).unwrap_or_default();
                (a, b, c)
            }
            1 => {
                let a = pd.as_ref().map(|x| x.arrivals.iter().map(|d| opt(&d.name, d.runway_transitions.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(" "), d.name == p.star)).collect()).unwrap_or_default();
                let b = pd.as_ref().and_then(|x| x.arrivals.iter().find(|d| d.name == p.star)).map(|d| d.enroute_transitions.iter().map(|t| opt(&t.name, String::new(), t.name == p.star_trans)).collect()).unwrap_or_default();
                let c = rwys(&p.dest).into_iter().map(|r| opt(&r, String::new(), r == p.arr_rwy)).collect();
                (a, b, c)
            }
            _ => {
                let a = pd
                    .as_ref()
                    .map(|x| {
                        x.approaches
                            .iter()
                            .filter(|ap| p.arr_rwy.is_empty() || ap.runway.as_deref() == Some(p.arr_rwy.as_str()))
                            .map(|ap| opt(&ap.name, format!("{} {}", ap.kind, ap.runway.clone().unwrap_or_default()).trim().to_string(), ap.name == p.app))
                            .collect()
                    })
                    .unwrap_or_default();
                let b = pd.as_ref().and_then(|x| x.approaches.iter().find(|ap| ap.name == p.app)).map(|ap| ap.transitions.iter().map(|t| opt(&t.name, String::new(), t.name == p.app_trans)).collect()).unwrap_or_default();
                (a, b, Vec::new())
            }
        };
        app.set_plan_opts_a(model(a));
        app.set_plan_opts_b(model(b));
        app.set_plan_opts_c(model(c));
        let loading = !s.loading.is_empty();
        let no_procs = (!p.origin.is_empty() && po.is_none()) || (!p.dest.is_empty() && pd.is_none());
        app.set_plan_status(ss(if loading {
            tr("Wczytywanie procedur…").to_string()
        } else if no_procs {
            tr("Brak procedur dla tego lotniska w dostępnych danych").to_string()
        } else {
            String::new()
        }));
        app.set_plan_route_text(ss(route_string(&p)));
        if !s.fitted && s.legs.len() > 1 {
            if let Some(b) = map::bounds(s.legs.iter().map(|l| (l.lat, l.lon))) {
                s.view.fit(b);
                s.fitted = true;
            }
        }
    });
    render_map();
    render_profile();
}

pub(crate) fn route_string(p: &FPlan) -> String {
    let mut out = vec![p.origin.clone()];
    if !p.sid.is_empty() {
        out.push(p.sid.clone());
    }
    let mut last_via = String::new();
    for (i, w) in p.enroute.iter().enumerate() {
        let via = if w.via.is_empty() { "DCT".to_string() } else { w.via.clone() };
        // kolejne punkty tej samej drogi: zostaw tylko ostatni
        let next_same = p.enroute.get(i + 1).is_some_and(|n| n.via == via && via != "DCT");
        if via != "DCT" {
            if via != last_via {
                out.push(via.clone());
            }
            if !next_same {
                out.push(w.ident.clone());
            }
        } else {
            if i > 0 {
                out.push("DCT".into());
            }
            out.push(w.ident.clone());
        }
        last_via = via;
    }
    if !p.star.is_empty() {
        out.push(p.star.clone());
    }
    out.push(p.dest.clone());
    out.retain(|x| !x.is_empty());
    out.join(" ")
}

fn changed() {
    with(|s| save_plan(&s.plan));
    publish();
}

pub fn cmd(id: &str, arg: &str) -> bool {
    let a = arg.trim().to_uppercase();
    match id {
        "plan-origin" => {
            with(|s| {
                s.plan.origin = a.clone();
                s.plan.dep_rwy.clear();
                s.plan.sid.clear();
                s.plan.sid_trans.clear();
                s.fitted = false;
            });
            fetch_procs(&a);
            changed();
        }
        "plan-dest" => {
            with(|s| {
                s.plan.dest = a.clone();
                s.plan.star.clear();
                s.plan.star_trans.clear();
                s.plan.app.clear();
                s.plan.app_trans.clear();
                s.plan.arr_rwy.clear();
                s.fitted = false;
            });
            fetch_procs(&a);
            changed();
        }
        "plan-cruise" => {
            let v: i32 = a.trim_start_matches("FL").parse().unwrap_or(0);
            let ft = if v < 1000 { v * 100 } else { v };
            if ft > 0 {
                with(|s| s.plan.cruise_ft = ft.clamp(1000, 60_000));
            }
            changed();
        }
        "plan-altn" => {
            with(|s| s.plan.altn = a.clone());
            changed();
        }
        "plan-burn" | "plan-taxi" | "plan-extra" | "plan-reserve" => {
            if let Ok(v) = a.replace(',', ".").parse::<f64>() {
                with(|s| match id {
                    "plan-burn" => s.plan.burn_kgh = v.clamp(1.0, 50_000.0),
                    "plan-taxi" => s.plan.taxi_kg = v.clamp(0.0, 10_000.0),
                    "plan-extra" => s.plan.extra_kg = v.clamp(0.0, 200_000.0),
                    _ => s.plan.reserve_min = (v as i32).clamp(0, 180),
                });
            }
            changed();
        }
        "plan-notam" => {
            // Darmowe źródła NOTAM bez klucza nie odpowiadają programom – wyszukiwarka FAA w przeglądarce,
            // kody lotnisk planu w schowku do wklejenia
            let ids = with(|s| [s.plan.origin.clone(), s.plan.dest.clone(), s.plan.altn.clone()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" "));
            if !ids.is_empty() {
                if let Ok(mut c) = arboard::Clipboard::new() {
                    let _ = c.set_text(ids.clone());
                }
                toast(trf!("Skopiowano {ids} – wklej w wyszukiwarce NOTAM"));
            }
            let _ = open::that("https://notams.aim.faa.gov/notamSearch/nsapp.html#/");
        }
        "plan-briefing" => {
            // Z widoku OFP: najpierw plan z SimBrief do edycji, żeby briefing i OFP się zgadzały
            if arg == "ofp" {
                if let Some(sb) = crate::simbrief::plan() {
                    let same = with(|s| s.plan.origin == sb.origin && s.plan.dest == sb.dest);
                    if !same {
                        cmd("plan-simbrief", "");
                    }
                }
            }
            let (plan, legs) = with(|s| (s.plan.clone(), s.legs.clone()));
            crate::planbrief::make(plan, legs);
        }
        "plan-tas" => {
            if let Ok(v) = a.parse::<i32>() {
                with(|s| s.plan.tas_kt = v.clamp(40, 700));
            }
            changed();
        }
        "plan-route" => {
            share_sources();
            let (plan, procs) = with(|s| (s.plan.clone(), s.procs.clone()));
            let text = a.clone();
            std::thread::spawn(move || {
                let (p, missing) = parse_route(&text, plan, procs);
                later(move || {
                    let (o, d) = (p.origin.clone(), p.dest.clone());
                    with(|s| {
                        s.plan = p;
                        s.fitted = false;
                        s.sel = -1;
                    });
                    fetch_procs(&o);
                    fetch_procs(&d);
                    if !missing.is_empty() {
                        toast(trf!("Nie znaleziono: {m}", m = missing.join(", ")));
                    }
                    changed();
                });
            });
        }
        "plan-sel" => {
            let i: i32 = arg.parse().unwrap_or(-1);
            with(|s| {
                s.sel = if s.sel == i { -1 } else { i };
                if let Some(l) = s.legs.get(i.max(0) as usize).filter(|_| s.sel >= 0) {
                    s.view.lat = l.lat;
                    s.view.lon = l.lon;
                }
            });
            publish();
        }
        "plan-insert" => {
            // „IDENT” – za zaznaczonym punktem trasy (albo na końcu trasy)
            share_sources();
            let (near, pos) = with(|s| {
                let sel = s.legs.get(s.sel.max(0) as usize).filter(|_| s.sel >= 0);
                let near = sel.map(|l| (l.lat, l.lon)).or_else(|| s.plan.enroute.last().map(|w| (w.lat, w.lon))).or_else(|| apt_pos(&s.plan.origin).map(|p| (p.0, p.1))).unwrap_or((52.0, 20.0));
                let pos = match sel {
                    Some(l) if l.enroute_idx.is_some() => l.enroute_idx.unwrap() + 1,
                    Some(l) if l.seg == "ORIG" || l.seg == "SID" => 0,
                    _ => s.plan.enroute.len(),
                };
                (near, pos)
            });
            let ident = a.clone();
            std::thread::spawn(move || {
                let w = resolve(&ident, near, xp_cached().as_deref(), live_cached().as_ref());
                later(move || {
                    match w {
                        Some(w) => with(|s| {
                            let pos = pos.min(s.plan.enroute.len());
                            s.plan.enroute.insert(pos, w);
                            s.sel = -1;
                        }),
                        None => toast(trf!("Nie znaleziono punktu {ident}")),
                    }
                    changed();
                });
            });
        }
        "plan-del" | "plan-up" | "plan-down" => {
            let i: usize = arg.parse().unwrap_or(usize::MAX);
            with(|s| {
                let Some(e) = s.legs.get(i).and_then(|l| l.enroute_idx) else { return };
                let n = s.plan.enroute.len();
                match id {
                    "plan-del" => {
                        s.plan.enroute.remove(e);
                        s.sel = -1;
                    }
                    "plan-up" if e > 0 => {
                        s.plan.enroute.swap(e, e - 1);
                        s.plan.enroute[e].via = "DCT".into();
                        s.plan.enroute[e - 1].via = "DCT".into();
                        s.sel -= 1;
                    }
                    "plan-down" if e + 1 < n => {
                        s.plan.enroute.swap(e, e + 1);
                        s.plan.enroute[e].via = "DCT".into();
                        s.plan.enroute[e + 1].via = "DCT".into();
                        s.sel += 1;
                    }
                    _ => {}
                }
            });
            changed();
        }
        "plan-alt" => {
            // „indeks|wysokość” – FL240, 24000, puste = bez ograniczenia
            let (i, v) = arg.split_once('|').unwrap_or((arg, ""));
            let i: usize = i.parse().unwrap_or(usize::MAX);
            let v = v.trim().to_uppercase();
            let alt = if let Some(fl) = v.strip_prefix("FL") { fl.parse::<i32>().ok().map(|x| x * 100) } else { v.parse::<i32>().ok() };
            with(|s| {
                if let Some(e) = s.legs.get(i).and_then(|l| l.enroute_idx) {
                    s.plan.enroute[e].alt = alt;
                }
            });
            changed();
        }
        "plan-proc-tab" => publish(),
        "plan-pick" => {
            // „kolumna|nazwa”
            let (col, name) = arg.split_once('|').unwrap_or(("", ""));
            let tab = ui().map(|u| u.global::<App>().get_plan_proc_tab()).unwrap_or(0);
            with(|s| {
                let p = &mut s.plan;
                let toggle = |f: &mut String, v: &str| {
                    if f == v {
                        f.clear();
                    } else {
                        *f = v.to_string();
                    }
                };
                match (tab, col) {
                    (0, "a") => {
                        toggle(&mut p.dep_rwy, name);
                    }
                    (0, "b") => {
                        toggle(&mut p.sid, name);
                        p.sid_trans.clear();
                    }
                    (0, _) => toggle(&mut p.sid_trans, name),
                    (1, "a") => {
                        toggle(&mut p.star, name);
                        p.star_trans.clear();
                    }
                    (1, "b") => toggle(&mut p.star_trans, name),
                    (1, _) => toggle(&mut p.arr_rwy, name),
                    (_, "a") => {
                        toggle(&mut p.app, name);
                        p.app_trans.clear();
                    }
                    _ => toggle(&mut p.app_trans, name),
                }
                s.fitted = false;
            });
            changed();
        }
        "plan-new" => {
            with(|s| {
                s.plan = FPlan::default();
                s.sel = -1;
                s.fitted = false;
            });
            changed();
        }
        "plan-simbrief" => {
            let Some(sb) = crate::simbrief::plan() else {
                toast(tr("Najpierw pobierz plan z SimBrief (Pulpit → SimBrief)"));
                return true;
            };
            let fixes: Vec<_> = sb.fixes.iter().filter(|f| f.ident != sb.origin && f.ident != sb.dest && !f.ident.starts_with("TOC") && !f.ident.starts_with("TOD")).collect();
            remember_fixes(&fixes.iter().map(|f| (f.ident.clone(), f.lat, f.lon, "wpt".to_string(), String::new())).collect::<Vec<_>>());
            with(|s| {
                let mut p = FPlan { origin: sb.origin.clone(), dest: sb.dest.clone(), altn: sb.altn.clone(), dep_rwy: sb.dep_rwy.clone(), arr_rwy: sb.arr_rwy.clone(), cruise_ft: sb.cruise_ft.max(1000), ..Default::default() };
                if sb.ete_s > 0.0 && sb.distance_nm > 0.0 {
                    p.tas_kt = (sb.distance_nm / (sb.ete_s / 3600.0)).round().clamp(60.0, 700.0) as i32;
                    p.burn_kgh = sb.burn / (sb.ete_s / 3600.0) * if sb.lbs { 0.4536 } else { 1.0 };
                }
                // Paliwo z OFP: spalanie średnie, kołowanie, rezerwa trasy, rezerwa końcowa, dodatkowe
                let o = &sb.ofp;
                let kgf = if sb.lbs { 0.453_592 } else { 1.0 };
                if o.avg_ff > 0.0 {
                    p.burn_kgh = o.avg_ff * kgf;
                }
                if o.taxi > 0.0 {
                    p.taxi_kg = (o.taxi * kgf).round();
                }
                if o.trip > 0.0 && o.cont > 0.0 {
                    p.cont_pct = (o.cont / o.trip * 100.0).clamp(0.0, 20.0);
                }
                if o.extra > 0.0 {
                    p.extra_kg = (o.extra * kgf).round();
                }
                if o.reserve > 0.0 && p.burn_kgh > 0.0 {
                    p.reserve_min = (o.reserve * kgf / (p.burn_kgh * 0.85) * 60.0).round().clamp(0.0, 180.0) as i32;
                }
                p.enroute = fixes
                    .iter()
                    .map(|f| Wpt { ident: f.ident.clone(), kind: "wpt".into(), lat: f.lat, lon: f.lon, via: if f.via.is_empty() || f.via == "DCT" { "DCT".into() } else { f.via.clone() }, alt: None, region: String::new() })
                    .collect();
                // SID / STAR z SimBrief zostają jako punkty trasy (nazwy procedur w polu „via”)
                s.plan = p;
                s.fitted = false;
                s.sel = -1;
            });
            refresh();
            changed();
        }
        "plan-export-pln" => export(false),
        "plan-export-fms" => export(true),
        "plan-xp-folder" => {
            if let Some(dir) = rfd::FileDialog::new().set_title(tr("Folder X-Plane")).pick_folder() {
                if xpnav::is_root(&dir) {
                    state().borrow_mut().s.xplane_path = dir.display().to_string();
                    state().borrow().s.save();
                    with(|s| s.procs.clear());
                    refresh();
                    toast(source_text());
                } else {
                    toast(tr("To nie jest folder X-Plane (brak Resources/default data)"));
                }
            }
        }
        "plan-to-map" => {
            state().borrow_mut().s.nav_plan = true;
            state().borrow().s.save();
            if let Some(u) = ui() {
                u.global::<App>().set_tab(9);
            }
            crate::navmap::render(true);
        }
        _ => return false,
    }
    true
}

pub fn map_event(id: i32, kind: &str, x: f32, y: f32, dx: f32, dy: f32) {
    let rerender_map = with(|s| {
        if kind == "size" {
            let old = s.sizes.insert(id, (x, y));
            if id == MAP {
                s.view.w = (x as f64).max(16.0);
                s.view.h = (y as f64).max(16.0);
            }
            return old.is_none_or(|(a, b)| (a - x).abs() > 0.5 || (b - y).abs() > 0.5);
        }
        if id != MAP {
            return false;
        }
        match kind {
            "pan" => s.view.drag(x as f64, y as f64, dx as f64, dy as f64),
            "tilt" => return s.view.tilt_by(dy as f64),
            "orbit" => {
                let turned = s.view.rotate_by(dx as f64);
                let tilted = s.view.tilt_by(dy as f64);
                if turned {
                    crate::app::publish_bearing(MAP, s.view.bearing);
                }
                return turned || tilted;
            }
            "north" => {
                s.view.reset_bearing();
                crate::app::publish_bearing(MAP, 0.0);
            }
            "tilt-toggle" => s.view.toggle_tilt(),
            "zoom" => s.view.zoom_at(dy as f64, x as f64, y as f64),
            "fit" => {
                if let Some(b) = map::bounds(s.legs.iter().map(|l| (l.lat, l.lon))) {
                    s.view.fit(b);
                }
            }
            _ => return false,
        }
        true
    });
    if rerender_map {
        if id == PROFILE {
            render_profile();
        } else {
            render_map();
        }
    }
}

pub fn render_map() {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    let dark = u.global::<Theme>().get_dark();
    let (style, scale) = {
        let st = state();
        let b = st.borrow();
        (crate::app::map_style(&b, dark), b.scale)
    };
    let r = with(|s| {
        let mut ov = Overlay::default();
        let seg_col = |seg: &str| -> [u8; 4] {
            match seg {
                "SID" | "STAR" => [255, 51, 230, 230],
                "APP" => [255, 181, 71, 230],
                _ => [79, 216, 235, 235],
            }
        };
        for w in s.legs.windows(2) {
            ov.tracks.push(Track { pts: vec![(w[0].lat, w[0].lon, w[0].alt_plan as i32), (w[1].lat, w[1].lon, w[1].alt_plan as i32)], color: Some(seg_col(w[1].seg)), width: 2.6, wall: true });
        }
        for (i, l) in s.legs.iter().enumerate() {
            ov.points.push(Point { lat: l.lat, lon: l.lon, label: l.ident.clone(), kind: if s.sel == i as i32 { 2 } else if l.kind == "apt" { 0 } else { 1 }, alt: Some(l.alt_plan as i32) });
        }
        if let Some(sn) = state().borrow().host.snapshot() {
            ov.aircraft = Some((sn.position.lat, sn.position.lon, sn.attitude.hdg_true_deg));
        }
        map::render(&s.view, style, &ov, scale, dark)
    });
    app.set_plan_map(r.image);
    app.set_plan_map_labels(crate::app::labels_model(r.labels));
}

fn render_profile() {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    let scale = state().borrow().scale.max(1.0);
    let img = with(|s| {
        let (w, h) = s.sizes.get(&PROFILE).copied()?;
        let (w, h) = ((w * scale) as u32, (h * scale) as u32);
        if w < 16 || h < 16 {
            return None;
        }
        let mut c = Canvas::new(w, h);
        draw_profile(&mut c, &s.legs, s.plan.cruise_ft as f64, s.sel, w as f32, h as f32, scale);
        Some(c.into_image())
    });
    if let Some(i) = img {
        app.set_plan_profile(i);
    }
}

fn draw_profile(c: &mut Canvas, legs: &[Leg], cruise: f64, sel: i32, w: f32, h: f32, k: f32) {
    const BG: Argb = 0xFF0B1016;
    const CY: Argb = 0xFF4FD8EB;
    const MG: Argb = 0xFFFF33E6;
    const DIM: Argb = 0xFF6F7C8A;
    c.rect(0.0, 0.0, w, h, BG);
    if legs.len() < 2 {
        c.text_f(tr("Dodaj lotniska i punkty, aby zobaczyć profil"), w / 2.0, h / 2.0, 12.0 * k, DIM, Align::Center, Face::Cond);
        return;
    }
    let (l, r, t, b) = (54.0 * k, w - 14.0 * k, 16.0 * k, h - 26.0 * k);
    let total = legs.last().map(|x| x.cum).unwrap_or(1.0).max(1.0);
    let top = ((legs.iter().map(|x| x.alt_plan).fold(cruise, f64::max) / 5000.0).ceil() * 5000.0 + 2000.0) as f32;
    let x = |d: f64| l + (d / total) as f32 * (r - l);
    let y = |a: f64| b - (a as f32 / top).clamp(0.0, 1.0) * (b - t);
    let step = if top > 30_000.0 { 10_000 } else { 5000 };
    let mut a = 0;
    while a as f32 <= top {
        c.dash_line(l, y(a as f64), r, y(a as f64), alpha(WHITE, 0x14), 1.0 * k, 3.0 * k, 5.0 * k);
        c.text_f(&if a >= 18_000 { format!("FL{}", a / 100) } else { a.to_string() }, l - 6.0 * k, y(a as f64), 9.5 * k, DIM, Align::Right, Face::Mono);
        a += step;
    }
    // teren pod trasą: tylko linia odniesienia
    c.line(l, b, r, b, 0xFF3C5A44, 1.4 * k);
    // T/C i T/D jako punkty załamania profilu, żeby przelot był poziomy aż do T/D,
    // a znaczniki leżały na linii (a nie nad skosem między punktami trasy)
    let (e0, e1) = (legs[0].alt_plan, legs[legs.len() - 1].alt_plan);
    let toc = ((cruise - e0) / 300.0).clamp(0.0, total);
    let tod = (total - (cruise - e1) / 318.0).clamp(0.0, total);
    let marks: Vec<(f64, &str)> = if toc < tod {
        vec![(toc, "T/C"), (tod, "T/D")]
    } else {
        Vec::new()
    };
    let mut pts: Vec<(f64, f64)> = legs.iter().map(|g| (g.cum, g.alt_plan)).collect();
    let mut mark_alt = Vec::with_capacity(marks.len());
    for &(d, _) in &marks {
        // między sąsiednimi punktami: poziom przelotowy, ale nie wyżej niż wyższy z sąsiadów
        // (gdy oba są niżej przez ograniczenia procedur, nie robimy szpica)
        let i = pts.iter().position(|p| p.0 >= d).unwrap_or(pts.len() - 1);
        if i > 0 && d > pts[i - 1].0 && d < pts[i].0 {
            let a = cruise.min(pts[i - 1].1.max(pts[i].1));
            pts.insert(i, (d, a));
            mark_alt.push(a);
        } else {
            mark_alt.push(pts[i].1);
        }
    }
    // wypełnienie pod profilem
    let mut poly = vec![x(0.0), b];
    for &(d, a) in &pts {
        poly.push(x(d));
        poly.push(y(a));
    }
    poly.push(x(total));
    poly.push(b);
    c.poly(alpha(CY, 0x22), 0, 0.0, &poly);
    let line: Vec<f32> = pts.iter().flat_map(|&(d, a)| [x(d), y(a)]).collect();
    c.polyline(CY, 2.2 * k, &line);
    // ograniczenia i punkty
    for (i, g) in legs.iter().enumerate() {
        let (px, py) = (x(g.cum), y(g.alt_plan));
        let col = if g.seg == "ENR" || g.seg == "ORIG" || g.seg == "DEST" { CY } else { MG };
        c.line(px, py, px, b, alpha(col, 0x30), 1.0 * k);
        c.circle(px, py, (if sel == i as i32 { 5.0 } else { 3.0 }) * k, if sel == i as i32 { WHITE } else { col }, 0.0);
        if let Some(mn) = g.c_min {
            let yy = y(mn);
            c.poly(AMBER, 0, 0.0, &[px - 5.0 * k, yy + 6.0 * k, px + 5.0 * k, yy + 6.0 * k, px, yy]);
        }
        if let Some(mx) = g.c_max {
            let yy = y(mx);
            c.poly(AMBER, 0, 0.0, &[px - 5.0 * k, yy - 6.0 * k, px + 5.0 * k, yy - 6.0 * k, px, yy]);
        }
        // co któryś podpis, żeby się nie zlewały
        let every = ((legs.len() as f32 * 46.0 * k) / (r - l)).ceil().max(1.0) as usize;
        if i % every == 0 || i == legs.len() - 1 || sel == i as i32 {
            c.text_f(&g.ident, px, b + 12.0 * k, 9.5 * k, if sel == i as i32 { WHITE } else { DIM }, Align::Center, Face::Mono);
        }
    }
    // TOC / TOD — na linii profilu
    for (&(d, lbl), &a) in marks.iter().zip(&mark_alt) {
        let (px, py) = (x(d), y(a));
        c.circle(px, py, 4.0 * k, 0xFF3CCF7A, 1.6 * k);
        c.text_f(lbl, px, py - 11.0 * k, 10.0 * k, 0xFF3CCF7A, Align::Center, Face::CondBold);
    }
}

// ---------- eksport ----------

fn dms(v: f64, pos: char, neg: char) -> String {
    let h = if v >= 0.0 { pos } else { neg };
    let a = v.abs();
    let d = a.floor();
    let m = ((a - d) * 60.0).floor();
    let s = ((a - d) * 60.0 - m) * 60.0;
    format!("{h}{d:.0}° {m:.0}' {s:.2}\"")
}

fn export(fms: bool) {
    let (p, legs) = with(|s| (s.plan.clone(), compute(s)));
    if p.origin.is_empty() || p.dest.is_empty() {
        toast(tr("Plan potrzebuje lotniska startu i celu"));
        return;
    }
    let text = if fms { to_fms(&p, &legs) } else { to_pln(&p, &legs) };
    let name = format!("{}{}.{}", p.origin, p.dest, if fms { "fms" } else { "pln" });
    let mut dlg = rfd::FileDialog::new().set_file_name(&name).add_filter(if fms { "X-Plane FMS" } else { "MSFS PLN" }, &[if fms { "fms" } else { "pln" }]);
    if fms {
        if let Some(x) = xp() {
            let d = x.root().join("Output").join("FMS plans");
            if d.is_dir() {
                dlg = dlg.set_directory(d);
            }
        }
    }
    let Some(path) = dlg.save_file() else { return };
    match std::fs::write(&path, text) {
        Ok(_) => toast(trf!("Zapisano plan {f}", f = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())),
        Err(e) => toast(trf!("Nie zapisano pliku: {e}")),
    }
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Plan MSFS (2020 i 2024): punkty trasy z procedurami oznaczonymi DepartureFP / ArrivalFP.
pub fn to_pln(p: &FPlan, legs: &[Leg]) -> String {
    let lla = |la: f64, lo: f64, alt: f64| format!("{},{},{:+011.2}", dms(la, 'N', 'S'), dms(lo, 'E', 'W'), alt);
    let name = |icao: &str| airports::db().and_then(|d| d.get(icao).map(|a| a.name.clone())).unwrap_or_default();
    let o = legs.first();
    let d = legs.last();
    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<SimBase.Document Type=\"AceXML\" version=\"1,0\">\n    <Descr>AceXML Document</Descr>\n    <FlightPlan.FlightPlan>\n");
    x.push_str(&format!("        <Title>{} to {}</Title>\n        <FPType>IFR</FPType>\n        <RouteType>{}</RouteType>\n        <CruisingAlt>{}</CruisingAlt>\n", p.origin, p.dest, if p.cruise_ft >= 18_000 { "HighAlt" } else { "LowAlt" }, p.cruise_ft));
    x.push_str(&format!("        <DepartureID>{}</DepartureID>\n        <DepartureLLA>{}</DepartureLLA>\n", p.origin, o.map(|l| lla(l.lat, l.lon, l.alt_plan)).unwrap_or_default()));
    x.push_str(&format!("        <DestinationID>{}</DestinationID>\n        <DestinationLLA>{}</DestinationLLA>\n", p.dest, d.map(|l| lla(l.lat, l.lon, l.alt_plan)).unwrap_or_default()));
    x.push_str(&format!("        <Descr>{}, {}</Descr>\n", p.origin, p.dest));
    if !p.dep_rwy.is_empty() {
        x.push_str(&format!("        <DeparturePosition>{}</DeparturePosition>\n", xml(&p.dep_rwy)));
    }
    x.push_str(&format!("        <DepartureName>{}</DepartureName>\n        <DestinationName>{}</DestinationName>\n", xml(&name(&p.origin)), xml(&name(&p.dest))));
    x.push_str("        <AppVersion>\n            <AppVersionMajor>11</AppVersionMajor>\n            <AppVersionBuild>282174</AppVersionBuild>\n        </AppVersion>\n");
    let rwy_tag = |r: &str| -> String {
        if r.is_empty() {
            return String::new();
        }
        let num: String = r.chars().take_while(|c| c.is_ascii_digit()).collect();
        let des = match r.chars().last() {
            Some('L') => "LEFT",
            Some('R') => "RIGHT",
            Some('C') => "CENTER",
            _ => "",
        };
        let mut t = format!("            <RunwayNumberFP>{num}</RunwayNumberFP>\n");
        if !des.is_empty() {
            t.push_str(&format!("            <RunwayDesignatorFP>{des}</RunwayDesignatorFP>\n"));
        }
        t
    };
    for l in legs {
        if l.kind == "rwy" {
            continue;
        }
        let typ = match l.kind.as_str() {
            "apt" => "Airport",
            "vor" => "VOR",
            "ndb" => "NDB",
            "latlon" => "User",
            _ => "Intersection",
        };
        x.push_str(&format!("        <ATCWaypoint id=\"{}\">\n            <ATCWaypointType>{typ}</ATCWaypointType>\n            <WorldPosition>{}</WorldPosition>\n", xml(&l.ident), lla(l.lat, l.lon, l.alt_plan)));
        match l.seg {
            "SID" => {
                x.push_str(&format!("            <DepartureFP>{}</DepartureFP>\n", xml(&p.sid)));
                x.push_str(&rwy_tag(&p.dep_rwy));
            }
            "STAR" => {
                x.push_str(&format!("            <ArrivalFP>{}</ArrivalFP>\n", xml(&p.star)));
                x.push_str(&rwy_tag(&p.arr_rwy));
            }
            "APP" => {
                let kind = p.app.chars().next().map(|c| match c {
                    'I' => "ILS",
                    'L' => "LOC",
                    'R' => "RNAV",
                    'D' => "VORDME",
                    'V' => "VOR",
                    'N' => "NDB",
                    _ => "ILS",
                });
                x.push_str(&format!("            <ApproachTypeFP>{}</ApproachTypeFP>\n", kind.unwrap_or("ILS")));
                x.push_str(&rwy_tag(&p.arr_rwy));
            }
            "ENR" if l.via != "DCT" && !l.via.is_empty() => x.push_str(&format!("            <ATCAirway>{}</ATCAirway>\n", xml(&l.via))),
            _ => {}
        }
        if typ != "User" {
            x.push_str("            <ICAO>\n");
            let region = with(|s| s.plan.enroute.iter().find(|w| w.ident == l.ident).map(|w| w.region.clone())).unwrap_or_default();
            if !region.is_empty() {
                x.push_str(&format!("                <ICAORegion>{}</ICAORegion>\n", xml(&region)));
            }
            x.push_str(&format!("                <ICAOIdent>{}</ICAOIdent>\n            </ICAO>\n", xml(&l.ident)));
        }
        x.push_str("        </ATCWaypoint>\n");
    }
    x.push_str("    </FlightPlan.FlightPlan>\n</SimBase.Document>\n");
    x
}

/// Plan X-Plane 11/12 (FMS v1100): procedury jako nazwy (FMC rozwinie je z własnej bazy), trasa jako punkty.
pub fn to_fms(p: &FPlan, legs: &[Leg]) -> String {
    let mut x = String::from("I\n1100 Version\n");
    let cyc = xp_cached().map(|x| x.cycle()).filter(|c| !c.is_empty()).unwrap_or_else(|| "2511".into());
    x.push_str(&format!("CYCLE {cyc}\nADEP {}\n", p.origin));
    if !p.dep_rwy.is_empty() {
        x.push_str(&format!("DEPRWY RW{}\n", p.dep_rwy));
    }
    if !p.sid.is_empty() {
        x.push_str(&format!("SID {}\n", p.sid));
        if !p.sid_trans.is_empty() {
            x.push_str(&format!("SIDTRANS {}\n", p.sid_trans));
        }
    }
    x.push_str(&format!("ADES {}\n", p.dest));
    if !p.arr_rwy.is_empty() {
        x.push_str(&format!("DESRWY RW{}\n", p.arr_rwy));
    }
    if !p.star.is_empty() {
        x.push_str(&format!("STAR {}\n", p.star));
        if !p.star_trans.is_empty() {
            x.push_str(&format!("STARTRANS {}\n", p.star_trans));
        }
    }
    if !p.app.is_empty() {
        x.push_str(&format!("APP {}\n", p.app));
        if !p.app_trans.is_empty() {
            x.push_str(&format!("APPTRANS {}\n", p.app_trans));
        }
    }
    let enr: Vec<&Leg> = legs.iter().filter(|l| matches!(l.seg, "ORIG" | "ENR" | "DEST")).collect();
    x.push_str(&format!("NUMENR {}\n", enr.len()));
    for l in enr {
        let (t, via) = match l.seg {
            "ORIG" => ("1", "ADEP".to_string()),
            "DEST" => ("1", "ADES".to_string()),
            _ => (
                match l.kind.as_str() {
                    "vor" => "3",
                    "ndb" => "2",
                    "latlon" => "28",
                    "apt" => "1",
                    _ => "11",
                },
                if l.via.is_empty() || l.via == "DCT" { "DRCT".to_string() } else { l.via.clone() },
            ),
        };
        x.push_str(&format!("{t} {} {via} {:.6} {:.6} {:.6}\n", l.ident, l.alt_plan, l.lat, l.lon));
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legs() -> (FPlan, Vec<Leg>) {
        let p = FPlan { origin: "EPWA".into(), dest: "EPKK".into(), dep_rwy: "29".into(), sid: "BAMS1G".into(), cruise_ft: 23_000, ..Default::default() };
        let l = vec![
            Leg { ident: "EPWA".into(), kind: "apt".into(), lat: 52.16, lon: 20.96, seg: "ORIG", ..Default::default() },
            Leg { ident: "BAMSO".into(), kind: "wpt".into(), lat: 51.6, lon: 20.4, seg: "SID", ..Default::default() },
            Leg { ident: "ABCDE".into(), kind: "wpt".into(), lat: 51.0, lon: 20.0, via: "UL29".into(), seg: "ENR", alt_plan: 23_000.0, ..Default::default() },
            Leg { ident: "EPKK".into(), kind: "apt".into(), lat: 50.07, lon: 19.78, seg: "DEST", ..Default::default() },
        ];
        (p, l)
    }

    #[test]
    fn fms_and_pln() {
        let (p, l) = legs();
        let f = to_fms(&p, &l);
        assert!(f.contains("ADEP EPWA") && f.contains("SID BAMS1G") && f.contains("NUMENR 3"));
        assert!(f.contains("11 ABCDE UL29 23000.000000 51.000000 20.000000"));
        let x = to_pln(&p, &l);
        assert!(x.contains("<DepartureFP>BAMS1G</DepartureFP>") && x.contains("<RunwayNumberFP>29</RunwayNumberFP>"));
        assert!(x.contains("<ATCAirway>UL29</ATCAirway>"));
        assert!(x.contains("<ATCWaypoint id=\"EPKK\">"));
    }

    #[test]
    fn latlon_tokens() {
        assert_eq!(parse_latlon("5230N02000E"), Some((52.5, 20.0)));
        assert_eq!(parse_latlon("52N020W"), Some((52.0, -20.0)));
        assert!(parse_latlon("BAMSO").is_none());
    }

    #[test]
    fn route_text() {
        let mut p = FPlan { origin: "EPWA".into(), dest: "EPKK".into(), sid: "BAMS1G".into(), ..Default::default() };
        p.enroute = vec![
            Wpt { ident: "BAMSO".into(), via: "DCT".into(), ..Default::default() },
            Wpt { ident: "ABCDE".into(), via: "UL29".into(), ..Default::default() },
            Wpt { ident: "FGHIJ".into(), via: "UL29".into(), ..Default::default() },
            Wpt { ident: "XYZ".into(), via: "DCT".into(), ..Default::default() },
        ];
        assert_eq!(route_string(&p), "EPWA BAMS1G BAMSO UL29 FGHIJ DCT XYZ EPKK");
    }
}
