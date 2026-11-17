//! Karta Checklisty: listy kontrolne dopasowane do samolotu z symulatora, z pozycjami odhaczanymi
//! ręcznie albo samoczynnie, gdy stan samolotu się zgadza (hamulec postojowy, klapy, podwozie, światła…).
//!
//! Zestawy to pliki JSON (opis formatu: docs/checklists.md):
//! - wbudowane – `app/checklists/*.json`, wkompilowane w program,
//! - własne – folder `checklists` obok ustawień (`paths::config()`); zestaw o tym samym `id` co wbudowany
//!   zastępuje go, każdy inny dochodzi do listy. Wczytywane przy starcie i przyciskiem „Wczytaj ponownie”.
//!
//! Zestaw wybiera się sam: pierwszy, którego wzorzec `match` pasuje do nazwy samolotu, a gdy żaden –
//! pierwszy ze stylem kabiny (`styles`) rozpoznanym jak na karcie Przyrządy. Można też wybrać ręcznie.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;

use opentakeoff_core::lang::{self, tr};
use opentakeoff_core::proto::FlightState;
use opentakeoff_core::{log, paths, trf};
use regex::Regex;
use serde::Deserialize;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::app::{state, toast, ui};
use crate::avionics::data::DisplayStyle;
use crate::*;

// Zestawy wbudowane – wszystkie pliki z app/checklists (lista składana przez build.rs).
include!(concat!(env!("OUT_DIR"), "/checklists.rs"));

/// Zmienne warunków `auto` (opis w docs/checklists.md).
const VARS: &[&str] = &[
    "on_ground",
    "parking_brake",
    "gear_down",
    "flaps",
    "flaps_pct",
    "spoilers_armed",
    "spoilers_pct",
    "throttle",
    "mixture",
    "prop",
    "trim",
    "beacon",
    "strobe",
    "nav_lights",
    "landing_lights",
    "taxi_lights",
    "logo_lights",
    "wing_lights",
    "panel_lights",
    "cabin_lights",
    "engines",
    "engines_running",
    "ap",
    "fd",
    "athr",
    "ap_alt",
    "ap_hdg",
    "ap_spd",
    "xpdr_on",
    "xpdr_alt",
    "xpdr_code",
    "ias",
    "gs",
    "vs",
    "alt",
    "agl",
    "fuel_kg",
    "baro_std",
    "baro_hpa",
    "altimeter_set",
];

/// Tekst jednojęzyczny albo w kilku językach: `"tekst"` lub `{"pl": "…", "en": "…"}`.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum Txt {
    One(String),
    Many(BTreeMap<String, String>),
}

impl Default for Txt {
    fn default() -> Self {
        Txt::One(String::new())
    }
}

impl Txt {
    /// Tekst w języku interfejsu; brak – angielski, polski albo pierwszy z brzegu.
    fn get(&self) -> String {
        match self {
            Txt::One(s) => s.clone(),
            Txt::Many(m) => m.get(lang::current()).or_else(|| m.get("en")).or_else(|| m.get("pl")).or_else(|| m.values().next()).cloned().unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct Item {
    item: Txt,
    resp: Txt,
    note: Txt,
    /// Warunek z danych symulatora, np. `parking_brake`, `flaps>0 & !on_ground`.
    auto: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct List {
    name: Txt,
    items: Vec<Item>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct Set {
    id: String,
    name: String,
    /// Wyrażenia regularne (bez rozróżniania wielkości liter) dla nazwy samolotu z symulatora.
    #[serde(rename = "match")]
    patterns: Vec<String>,
    /// Style kabiny z karty Przyrządy (A320, B737, B787, ATR72, E190, G1000, C172…) – gdy nazwa nie pasuje.
    styles: Vec<String>,
    source: Txt,
    note: Txt,
    lists: Vec<List>,
    #[serde(skip)]
    re: Option<Regex>,
    #[serde(skip)]
    user: bool,
    #[serde(skip)]
    raw: String,
}

/// Stan pozycji: odhaczona, przez symulator, zablokowana (odznaczona ręcznie po odhaczeniu przez symulator).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Mark {
    done: bool,
    auto: bool,
    blocked: bool,
}

struct St {
    sets: Vec<Set>,
    errors: Vec<String>,
    cur: Option<usize>,
    /// Zestaw wybrany samoczynnie (nie ręcznie).
    detected: bool,
    /// Nazwa samolotu, dla której wybrano zestaw.
    aircraft: String,
    list: usize,
    marks: Vec<Vec<Mark>>,
    /// Warunki bieżącej listy: -1 brak warunku albo danych, 0 niespełniony, 1 spełniony.
    live: Vec<i32>,
    connected: bool,
    /// Podpis ostatnio wysłanego stanu – bez zmian nie odświeżamy modeli.
    shown: String,
    /// Prędkości z kalkulatorów osiągów (karta Narzędzia): start i lądowanie.
    perf_to: String,
    perf_ldg: String,
}

thread_local! {
    static ST: RefCell<Option<St>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R {
    ST.with(|s| {
        let mut s = s.borrow_mut();
        let st = s.get_or_insert_with(|| {
            let (sets, errors) = load_sets();
            St { sets, errors, cur: None, detected: false, aircraft: String::new(), list: 0, marks: Vec::new(), live: Vec::new(), connected: false, shown: String::new(), perf_to: String::new(), perf_ldg: String::new() }
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

/// Folder własnych checklist.
pub fn user_dir() -> PathBuf {
    paths::config().join("checklists")
}

// ---------- wczytywanie ----------

fn parse(text: &str, file: &str, user: bool, errors: &mut Vec<String>) -> Option<Set> {
    let mut set: Set = match serde_json::from_str(text) {
        Ok(s) => s,
        Err(e) => {
            errors.push(format!("{file}: {e}"));
            return None;
        }
    };
    if set.id.trim().is_empty() {
        set.id = file.trim_end_matches(".json").to_lowercase();
    }
    if set.name.trim().is_empty() {
        set.name = set.id.clone();
    }
    if set.lists.is_empty() {
        errors.push(trf!("{file}: brak list (pole „lists”)"));
        return None;
    }
    let pats: Vec<String> = set.patterns.iter().filter(|p| !p.trim().is_empty()).map(|p| format!("(?:{p})")).collect();
    if !pats.is_empty() {
        match Regex::new(&format!("(?i){}", pats.join("|"))) {
            Ok(r) => set.re = Some(r),
            Err(e) => errors.push(format!("{file}: match – {e}")),
        }
    }
    // Nieznane zmienne w warunkach – pozycja zostaje ręczna, ale autor pliku dostaje podpowiedź.
    for l in &set.lists {
        for it in &l.items {
            for name in vars_of(&it.auto) {
                if !VARS.contains(&name.as_str()) {
                    errors.push(trf!("{file}: nieznana zmienna „{name}” w warunku „{c}”", c = it.auto));
                }
            }
        }
    }
    set.user = user;
    set.raw = text.to_string();
    Some(set)
}

/// Własne zestawy (najpierw, w kolejności nazw plików), potem wbudowane bez zastąpionych.
fn load_sets() -> (Vec<Set>, Vec<String>) {
    let mut errors = Vec::new();
    let mut sets: Vec<Set> = Vec::new();
    let dir = user_dir();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json"))).collect();
        files.sort();
        for p in files {
            let file = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            match std::fs::read_to_string(&p) {
                Ok(t) => {
                    // Notatnik zapisuje UTF-8 z BOM
                    if let Some(s) = parse(t.trim_start_matches('\u{feff}'), &file, true, &mut errors) {
                        if sets.iter().any(|x| x.id == s.id) {
                            errors.push(trf!("{file}: zestaw „{id}” już jest – pominięty", id = s.id));
                        } else {
                            sets.push(s);
                        }
                    }
                }
                Err(e) => errors.push(format!("{file}: {e}")),
            }
        }
    }
    for &(file, text) in BUILTIN {
        if let Some(s) = parse(text, file, false, &mut errors) {
            if !sets.iter().any(|x| x.id == s.id) {
                sets.push(s);
            }
        }
    }
    for e in &errors {
        log::warn(&format!("Checklisty: {e}"));
    }
    (sets, errors)
}

// ---------- warunki ----------

/// Nazwy zmiennych użytych w warunku.
fn vars_of(expr: &str) -> Vec<String> {
    expr.split(['|', '&'])
        .map(|t| t.trim().trim_start_matches('!').trim())
        .filter(|t| !t.is_empty())
        .map(|t| t.split(['<', '>', '=', '!']).next().unwrap_or("").trim().to_string())
        .filter(|n| !n.is_empty())
        .collect()
}

/// Silnik pracuje: obroty sprężarki, obroty silnika tłokowego albo śmigła.
fn engines_running(s: &FlightState) -> usize {
    s.engines.iter().filter(|e| e.n2_pct > 40.0 || e.rpm > 400.0 || e.prop_rpm > 400.0).count()
}

fn var(s: &FlightState, name: &str) -> Option<f64> {
    let b = |v: bool| if v { 1.0 } else { 0.0 };
    let c = &s.controls;
    let l = &s.lights;
    let av = s.avionics.as_ref();
    Some(match name {
        "on_ground" => b(s.aircraft.on_ground),
        "parking_brake" => b(c.parking_brake),
        "gear_down" => b(c.gear_down),
        "flaps" => c.flaps_index as f64,
        "flaps_pct" => c.flaps_pct,
        "spoilers_armed" => b(c.spoilers_armed),
        "spoilers_pct" => c.spoilers_pct,
        "throttle" => c.throttle_pct,
        "mixture" => c.mixture_pct,
        "prop" => c.prop_pct,
        "trim" => c.elevator_trim_pct,
        "beacon" => b(l.beacon),
        "strobe" => b(l.strobe),
        "nav_lights" => b(l.nav),
        "landing_lights" => b(l.landing),
        "taxi_lights" => b(l.taxi),
        "logo_lights" => b(l.logo),
        "wing_lights" => b(l.wing),
        "panel_lights" => b(l.panel),
        "cabin_lights" => b(l.cabin),
        "engines" => s.engines.len() as f64,
        "engines_running" => engines_running(s) as f64,
        "ap" => b(s.autopilot.master),
        "fd" => b(s.autopilot.fd),
        "athr" => b(s.autopilot.athr),
        "ap_alt" => s.autopilot.alt_ft,
        "ap_hdg" => s.autopilot.hdg_deg,
        "ap_spd" => s.autopilot.spd_kt,
        "xpdr_on" => b(matches!(s.radios.transponder.mode.as_str(), "on" | "alt" | "ground")),
        "xpdr_alt" => b(s.radios.transponder.mode == "alt"),
        "xpdr_code" => s.radios.transponder.code.trim().parse().unwrap_or(0.0),
        "ias" => s.speed.ias_kt,
        "gs" => s.speed.gs_kt,
        "vs" => s.speed.vs_fpm,
        "alt" => s.position.alt_ind_ft,
        "agl" => s.position.alt_agl_ft,
        "fuel_kg" => s.fuel.total_kg,
        "baro_std" => b(av?.baro_std),
        "baro_hpa" => av?.baro_hpa,
        // Wysokościomierz na bieżące QNH (±1 hPa) i nie na STD
        "altimeter_set" => {
            let a = av?;
            let qnh = s.environment.qnh_hpa;
            if qnh <= 0.0 || a.baro_hpa <= 0.0 {
                return None;
            }
            b(!a.baro_std && (a.baro_hpa - qnh).abs() <= 1.0)
        }
        _ => return None,
    })
}

fn term(s: &FlightState, t: &str) -> Option<bool> {
    if let Some(n) = t.strip_prefix('!') {
        return var(s, n.trim()).map(|v| v == 0.0);
    }
    for op in [">=", "<=", "==", "!=", ">", "<", "="] {
        if let Some(i) = t.find(op) {
            let v = var(s, t[..i].trim())?;
            let rhs = t[i + op.len()..].trim();
            // Po prawej liczba albo inna zmienna (np. engines_running>=engines)
            let k = rhs.parse::<f64>().ok().or_else(|| var(s, rhs))?;
            return Some(match op {
                ">=" => v >= k,
                "<=" => v <= k,
                "==" | "=" => (v - k).abs() < 1e-6,
                "!=" => (v - k).abs() >= 1e-6,
                ">" => v > k,
                _ => v < k,
            });
        }
    }
    var(s, t).map(|v| v != 0.0)
}

/// Warunek: alternatywy `|` z koniunkcji `&`; `None` – brak danych do oceny.
fn eval(s: &FlightState, expr: &str) -> Option<bool> {
    let mut unknown = false;
    for alt in expr.split('|').map(str::trim).filter(|a| !a.is_empty()) {
        let mut res = Some(true);
        for t in alt.split('&').map(str::trim).filter(|t| !t.is_empty()) {
            match term(s, t) {
                Some(false) => {
                    res = Some(false);
                    break;
                }
                None => res = None,
                Some(true) => {}
            }
        }
        match res {
            Some(true) => return Some(true),
            None => unknown = true,
            Some(false) => {}
        }
    }
    if unknown {
        None
    } else {
        Some(false)
    }
}

// ---------- wybór zestawu ----------

fn reset_marks(st: &mut St) {
    st.marks = match st.cur.and_then(|i| st.sets.get(i)) {
        Some(set) => set.lists.iter().map(|l| vec![Mark::default(); l.items.len()]).collect(),
        None => Vec::new(),
    };
    st.list = 0;
    st.live.clear();
    st.shown.clear();
}

/// Zestaw dla samolotu: ręczny wybór, nazwa samolotu, styl kabiny.
fn pick(st: &mut St, snap: Option<&FlightState>, manual: &str) {
    let before = st.cur.map(|i| st.sets[i].id.clone());
    if !manual.is_empty() {
        if let Some(i) = st.sets.iter().position(|s| s.id == manual) {
            st.cur = Some(i);
            st.detected = false;
        }
    } else if let Some(s) = snap {
        let a = &s.aircraft;
        let text = format!("{} {} {}", a.title, a.atc_model, a.atc_type);
        let style = DisplayStyle::detect(s).key();
        let found = st
            .sets
            .iter()
            .position(|x| x.re.as_ref().is_some_and(|r| r.is_match(&text)))
            .or_else(|| st.sets.iter().position(|x| x.styles.iter().any(|k| k.eq_ignore_ascii_case(style))));
        st.cur = found;
        st.detected = found.is_some();
    } else {
        // Auto bez symulatora – zostaje ostatnio rozpoznany zestaw
        st.detected = st.cur.is_some();
    }
    let after = st.cur.map(|i| st.sets[i].id.clone());
    if before != after || st.marks.is_empty() {
        reset_marks(st);
    }
}

fn manual_choice() -> String {
    state().borrow().s.checklist_set.clone()
}

// ---------- odświeżanie ----------

/// Co 250 ms: zmiana samolotu, warunki bieżącej listy i samoczynne odhaczanie.
/// `visible` – karta na ekranie (wtedy też modele interfejsu).
pub fn tick(visible: bool) {
    let snap = state().borrow().host.snapshot();
    let manual = manual_choice();
    let changed = with(|st| {
        let mut changed = st.connected != snap.is_some();
        st.connected = snap.is_some();
        if let Some(s) = &snap {
            let title = s.aircraft.title.trim().to_string();
            if !title.is_empty() && title != st.aircraft {
                st.aircraft = title;
                // Inny samolot – nowy lot: wszystko od nowa
                let keep = st.cur;
                pick(st, Some(s), &manual);
                if st.cur == keep {
                    reset_marks(st);
                }
                changed = true;
            }
        }
        let Some(set) = st.cur.and_then(|i| st.sets.get(i)) else {
            return changed;
        };
        let Some(list) = set.lists.get(st.list) else {
            return changed;
        };
        let live: Vec<i32> = list
            .items
            .iter()
            .map(|it| match (&snap, it.auto.trim()) {
                (Some(s), a) if !a.is_empty() => match eval(s, a) {
                    Some(true) => 1,
                    Some(false) => 0,
                    None => -1,
                },
                _ => -1,
            })
            .collect();
        if let Some(marks) = st.marks.get_mut(st.list) {
            for (m, l) in marks.iter_mut().zip(&live) {
                if *l == 1 && !m.done && !m.blocked {
                    m.done = true;
                    m.auto = true;
                    changed = true;
                }
            }
        }
        if live != st.live {
            st.live = live;
            changed = true;
        }
        changed
    });
    if visible || changed {
        publish(false);
    }
}

/// Karta otwarta: wybór zestawu (gdy jeszcze żadnego) i modele.
pub fn refresh() {
    let snap = state().borrow().host.snapshot();
    let manual = manual_choice();
    with(|st| {
        if st.cur.is_none() || !manual.is_empty() {
            pick(st, snap.as_ref(), &manual);
        }
    });
    publish(true);
}

fn publish(force: bool) {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    with(|st| {
        let key = format!("{:?}|{}|{:?}|{:?}|{}|{}", st.cur, st.list, st.marks, st.live, st.connected, st.detected);
        if !force && key == st.shown {
            return;
        }
        st.shown = key;
        let sets: Vec<ClSetV> = st
            .sets
            .iter()
            .enumerate()
            .map(|(i, s)| ClSetV { id: ss(&s.id), name: ss(&s.name), user: s.user, active: st.cur == Some(i) })
            .collect();
        app.set_cl_sets(model(sets));
        app.set_cl_auto(manual_choice().is_empty());
        app.set_cl_errors(ss(st.errors.join("\n")));
        app.set_cl_folder(ss(user_dir().display().to_string()));
        app.set_cl_connected(st.connected);
        let Some(set) = st.cur.and_then(|i| st.sets.get(i)) else {
            app.set_cl_set_id(ss(""));
            app.set_cl_set_name(ss(""));
            app.set_cl_status(ss(if st.connected { tr("Nie rozpoznano samolotu – wybierz zestaw checklist z listy.") } else { tr("Połącz symulator albo wybierz zestaw checklist z listy.") }));
            app.set_cl_lists(model(Vec::new()));
            app.set_cl_items(model(Vec::new()));
            return;
        };
        app.set_cl_set_id(ss(&set.id));
        app.set_cl_set_name(ss(&set.name));
        app.set_cl_status(ss(if st.detected && !st.aircraft.is_empty() {
            trf!("Rozpoznany samolot: {a}", a = st.aircraft)
        } else if st.detected {
            tr("Wybrany samoczynnie").to_string()
        } else {
            tr("Wybrany ręcznie").to_string()
        }));
        let note = set.note.get();
        let source = set.source.get();
        app.set_cl_source(ss(if note.is_empty() { source } else { format!("{source}\n{note}") }));
        let lists: Vec<ClListV> = set
            .lists
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let marks = st.marks.get(i).map(|m| m.as_slice()).unwrap_or(&[]);
                let done = marks.iter().filter(|m| m.done).count() as i32;
                let total = l.items.len() as i32;
                ClListV { name: ss(l.name.get()), done, total, complete: total > 0 && done == total }
            })
            .collect();
        let (all_done, all_total) = lists.iter().fold((0, 0), |a, l| (a.0 + l.done, a.1 + l.total));
        app.set_cl_progress(ss(format!("{all_done}/{all_total}")));
        app.set_cl_lists(model(lists));
        app.set_cl_current(st.list as i32);
        let list = set.lists.get(st.list);
        let marks = st.marks.get(st.list).map(|m| m.as_slice()).unwrap_or(&[]);
        let next = marks.iter().position(|m| !m.done);
        let items: Vec<ClItemV> = list
            .map(|l| {
                l.items
                    .iter()
                    .enumerate()
                    .map(|(i, it)| {
                        let m = marks.get(i).copied().unwrap_or_default();
                        ClItemV {
                            item: ss(it.item.get()),
                            resp: ss(it.resp.get()),
                            note: ss(perf_note(&it.item.get(), &it.resp.get(), it.note.get(), &st.perf_to, &st.perf_ldg)),
                            auto: !it.auto.trim().is_empty(),
                            live: st.live.get(i).copied().unwrap_or(-1),
                            done: m.done,
                            by_sim: m.done && m.auto,
                            next: next == Some(i),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        app.set_cl_items(model(items));
        app.set_cl_has_next(st.list + 1 < set.lists.len());
    });
}

/// Dopisek z kalkulatora osiągów przy pozycjach z prędkościami startu albo lądowania.
fn perf_note(item: &str, resp: &str, note: String, to: &str, ldg: &str) -> String {
    let t = format!("{item} {resp}").to_uppercase();
    let extra = if !to.is_empty() && (t.contains("V1") || t.contains("SPEEDS") || t.contains("VR ")) {
        to
    } else if !ldg.is_empty() && (t.contains("VREF") || t.contains("LANDING DATA") || t.contains("LDG DATA")) {
        ldg
    } else {
        return note;
    };
    let extra = trf!("Z kalkulatora: {v}", v = extra);
    if note.is_empty() {
        extra
    } else {
        format!("{note} · {extra}")
    }
}

// ---------- polecenia ----------

fn select_list(st: &mut St, i: usize) {
    let n = st.cur.and_then(|c| st.sets.get(c)).map(|s| s.lists.len()).unwrap_or(0);
    if i < n {
        st.list = i;
        st.live.clear();
    }
}

pub fn cmd(id: &str, arg: &str) -> bool {
    match id {
        "cl-list" => {
            let i: usize = arg.parse().unwrap_or(0);
            with(|st| select_list(st, i));
        }
        "cl-toggle" => {
            let i: usize = arg.parse().unwrap_or(usize::MAX);
            with(|st| {
                let li = st.list;
                if let Some(m) = st.marks.get_mut(li).and_then(|m| m.get_mut(i)) {
                    if m.done {
                        // Odznaczone ręcznie – symulator nie odhaczy ponownie aż do „Od nowa”
                        m.blocked = m.auto || m.blocked;
                        m.done = false;
                        m.auto = false;
                    } else {
                        m.done = true;
                        m.auto = false;
                    }
                }
            });
        }
        "cl-check-next" => {
            let advance = with(|st| {
                let li = st.list;
                let Some(marks) = st.marks.get_mut(li) else { return false };
                if let Some(m) = marks.iter_mut().find(|m| !m.done) {
                    m.done = true;
                    m.auto = false;
                    false
                } else {
                    true
                }
            });
            // Lista już ukończona – przejdź do następnej
            if advance {
                with(|st| {
                    let i = st.list + 1;
                    select_list(st, i);
                });
            }
        }
        "cl-next" => with(|st| {
            let i = st.list + 1;
            select_list(st, i);
        }),
        "cl-prev" => with(|st| {
            let i = st.list.saturating_sub(1);
            select_list(st, i);
        }),
        "cl-reset-list" => with(|st| {
            let li = st.list;
            if let Some(m) = st.marks.get_mut(li) {
                m.iter_mut().for_each(|x| *x = Mark::default());
            }
        }),
        "cl-reset" => {
            with(reset_marks);
            toast(tr("Wszystkie checklisty od nowa"));
        }
        "cl-set" => {
            {
                let st = state();
                let mut b = st.borrow_mut();
                b.s.checklist_set = arg.to_string();
                b.s.save();
            }
            let snap = state().borrow().host.snapshot();
            with(|st| {
                pick(st, snap.as_ref(), arg);
            });
        }
        "cl-folder" => {
            let dir = user_dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = open::that(&dir);
        }
        "cl-reload" => {
            let snap = state().borrow().host.snapshot();
            let manual = manual_choice();
            let (n, errs) = with(|st| {
                let keep_id = st.cur.map(|i| st.sets[i].id.clone());
                let (sets, errors) = load_sets();
                st.sets = sets;
                st.errors = errors;
                st.cur = keep_id.and_then(|id| st.sets.iter().position(|s| s.id == id));
                st.marks.clear();
                pick(st, snap.as_ref(), &manual);
                if st.marks.is_empty() {
                    reset_marks(st);
                }
                (st.sets.iter().filter(|s| s.user).count(), st.errors.len())
            });
            if errs > 0 {
                toast(trf!("Wczytano własne zestawy: {n}, błędy: {errs} – szczegóły pod listą checklist"));
            } else {
                toast(trf!("Wczytano własne zestawy: {n}"));
            }
        }
        "cl-tlr" => {
            // Prędkości z raportu TLR w OFP SimBrief
            let list = crate::simbrief::tlr_for_checklist();
            if list.is_empty() {
                toast(tr("W OFP nie ma raportu startu i lądowania (TLR)"));
                return true;
            }
            with(|st| {
                for x in &list {
                    if let Some(v) = x.strip_prefix("to|") {
                        st.perf_to = format!("SimBrief: {v}");
                    } else if let Some(v) = x.strip_prefix("ldg|") {
                        st.perf_ldg = format!("SimBrief: {v}");
                    }
                }
                st.shown.clear();
            });
            toast(tr("Prędkości z OFP wpisane do checklisty"));
        }
        "cl-perf" => {
            // „to|V1 143 kt · VR 147 kt · V2 151 kt” albo „ldg|VREF 133 kt · VAPP 138 kt”
            let (kind, v) = arg.split_once('|').unwrap_or(("", ""));
            with(|st| {
                if kind == "to" {
                    st.perf_to = v.to_string();
                } else {
                    st.perf_ldg = v.to_string();
                }
                st.shown.clear();
            });
            toast(trf!("W checkliście: {v}"));
        }
        "cl-copy" => {
            // Bieżący zestaw do folderu własnych – wzór do poprawek albo nowego samolotu
            let item = with(|st| st.cur.and_then(|i| st.sets.get(i)).map(|s| (s.id.clone(), s.raw.clone())));
            if let Some((id, raw)) = item {
                let dir = user_dir();
                let _ = std::fs::create_dir_all(&dir);
                let mut path = dir.join(format!("{id}.json"));
                let mut n = 2;
                while path.exists() {
                    path = dir.join(format!("{id}-{n}.json"));
                    n += 1;
                }
                match std::fs::write(&path, raw) {
                    Ok(()) => {
                        toast(trf!("Zapisano {f} – zmień plik i kliknij „Wczytaj”", f = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()));
                        let _ = open::that(&dir);
                    }
                    Err(e) => toast(trf!("Nie udało się zapisać pliku: {e}")),
                }
            }
        }
        _ => return false,
    }
    publish(true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(f: impl FnOnce(&mut FlightState)) -> FlightState {
        let mut s = FlightState::default();
        f(&mut s);
        s
    }

    #[test]
    fn builtin_sets_parse() {
        let mut errors = Vec::new();
        for &(file, text) in BUILTIN {
            let s = parse(text, file, false, &mut errors).expect(file);
            assert!(!s.lists.is_empty(), "{file}");
            assert!(s.re.is_some(), "{file}");
        }
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn conditions() {
        let s = state_with(|s| {
            s.controls.parking_brake = true;
            s.controls.flaps_index = 2;
            s.lights.beacon = true;
        });
        assert_eq!(eval(&s, "parking_brake"), Some(true));
        assert_eq!(eval(&s, "!parking_brake"), Some(false));
        assert_eq!(eval(&s, "flaps>0 & beacon"), Some(true));
        assert_eq!(eval(&s, "flaps==0 | beacon"), Some(true));
        assert_eq!(eval(&s, "flaps>=3"), Some(false));
        assert_eq!(eval(&s, "flaps!=2"), Some(false));
        // Brak danych awioniki – nie wiadomo
        assert_eq!(eval(&s, "baro_std"), None);
        assert_eq!(eval(&s, "flaps>5 & baro_std"), Some(false));
        assert_eq!(eval(&s, "engines_running>=engines"), Some(true));
    }

    #[test]
    fn unknown_variable_reported() {
        let mut errors = Vec::new();
        let text = r#"{"id":"x","lists":[{"name":"A","items":[{"item":"X","resp":"Y","auto":"foo>1"}]}]}"#;
        assert!(parse(text, "x.json", true, &mut errors).is_some());
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn text_languages() {
        let t: Txt = serde_json::from_str(r#"{"pl":"Kołowanie","en":"Taxi"}"#).unwrap();
        assert!(!t.get().is_empty());
        let t: Txt = serde_json::from_str(r#""Taxi""#).unwrap();
        assert_eq!(t.get(), "Taxi");
    }
}
