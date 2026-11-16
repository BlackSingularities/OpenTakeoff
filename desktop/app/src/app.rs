//! Logika okna: stan programu po stronie interfejsu, odświeżanie kart i obsługa poleceń (App.cmd).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use chrono::{Datelike, Local, TimeZone, Timelike};
use opentakeoff_core::aero::airports::{self, Airport};
use opentakeoff_core::aero::weather::{flight_category, Metar, Taf};
use opentakeoff_core::aero::{calc, metar as metar_dec};
use opentakeoff_core::lang::tr;
use opentakeoff_core::logbook::{achievements, export, landing, stats, FlightLog};
use opentakeoff_core::navdata::{math, AirportProcedures, LegInfo};
use opentakeoff_core::source::unix_ms;
use opentakeoff_core::{log, trf};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::fmt::{self, Units};
use crate::host::{Host, HostEvent};
use crate::map::{self, Overlay, Point, Style, Track, View};
use crate::settings::{Settings, ACCENTS};
use crate::*;

pub struct State {
    pub host: Arc<Host>,
    pub s: Settings,
    pub views: [View; 4],
    pub scale: f32,
    pub list: Vec<String>,
    pub selected: HashSet<String>,
    pub anchor: Option<String>,
    pub detail: Option<Arc<FlightLog>>,
    pub replay: f64,
    pub playing: bool,
    /// Odtwarzanie lotu w 3D (zamiast mapy 2D), mnożnik prędkości i kamera.
    pub replay_3d: bool,
    pub replay_speed: f64,
    pub replay_cam: crate::scene3d::Camera,
    pub logbook_rev: u64,
    pub airport: Option<String>,
    pub weather: HashMap<String, (Vec<Metar>, Option<Taf>, String)>,
    pub watch: HashMap<String, Metar>,
    pub procs: Option<(String, Arc<AirportProcedures>)>,
    pub legs: Vec<LegInfo>,
    pub confirm: Option<(String, Vec<String>)>,
    pub log_serial: u64,
    pub update: Option<opentakeoff_core::update::Release>,
    pub tiles_serial: u64,
    pub live_render: Instant,
    pub stats_rev: u64,
    pub db_loaded: bool,
    pub toast_serial: i32,
    pub watch_fetched: Option<Instant>,
}

thread_local! {
    static ST: RefCell<Option<Rc<RefCell<State>>>> = const { RefCell::new(None) };
    static UI: RefCell<Option<slint::Weak<AppWindow>>> = const { RefCell::new(None) };
}

pub fn install(state: State, ui: &AppWindow) {
    ST.with(|s| *s.borrow_mut() = Some(Rc::new(RefCell::new(state))));
    UI.with(|u| *u.borrow_mut() = Some(ui.as_weak()));
}

pub fn state() -> Rc<RefCell<State>> {
    ST.with(|s| s.borrow().clone().expect("stan"))
}

pub fn ui() -> Option<AppWindow> {
    UI.with(|u| u.borrow().as_ref().and_then(|w| w.upgrade()))
}

fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

fn units(s: &Settings) -> Units {
    Units { meters: s.units_alt == 1, kmh: s.units_speed == 1 }
}

pub fn toast(text: impl Into<String>) {
    let Some(ui) = ui() else { return };
    let st = state();
    let mut st = st.borrow_mut();
    st.toast_serial += 1;
    let app = ui.global::<App>();
    app.set_toast(ss(text.into()));
    app.set_toast_serial(st.toast_serial);
}

/// Wynik pracy w tle → wątek okna.
pub fn later(f: impl FnOnce() + Send + 'static) {
    let _ = slint::invoke_from_event_loop(f);
}

fn style(st: &State, dark: bool) -> Style {
    match st.s.map_style {
        1 => Style::Dark,
        2 => Style::Light,
        3 => Style::Osm,
        4 => Style::Satellite,
        _ => {
            if dark {
                Style::Dark
            } else {
                Style::Light
            }
        }
    }
}

pub fn map_style(st: &State, dark: bool) -> Style {
    style(st, dark)
}

pub fn labels_model(l: Vec<(f32, f32, String, i32)>) -> ModelRc<LabelV> {
    labels(l)
}

fn labels(l: Vec<(f32, f32, String, i32)>) -> ModelRc<LabelV> {
    model(l.into_iter().map(|(x, y, t, k)| LabelV { x, y, text: ss(t), kind: k }).collect())
}

// ---------- pasek stanu, Pulpit, Telefony ----------

pub fn refresh_status() {
    let Some(ui) = ui() else { return };
    let st = state();
    let mut st = st.borrow_mut();
    let app = ui.global::<App>();
    let host = st.host.clone();
    let src = host.source();
    app.set_sim_connected(src.is_connected());
    app.set_sim_status(ss(src.status_text()));
    app.set_sim_name(ss(src.sim_name()));
    app.set_simulation(host.simulation.load(std::sync::atomic::Ordering::SeqCst));
    app.set_link_ok(host.link_ok.load(std::sync::atomic::Ordering::SeqCst));
    let clients = host.server.clients();
    let paired: Vec<_> = clients.iter().filter(|c| c.paired).collect();
    app.set_phones(paired.len() as i32);
    app.set_phones_text(ss(match paired.len() {
        0 => tr("Brak połączonych telefonów").to_string(),
        1 => paired[0].name.clone(),
        n => trf!("{n} telefony połączone"),
    }));
    app.set_pairing_code(ss(host.pairing.code()));
    let addrs = host.addresses();
    app.set_addresses(ss(if addrs.is_empty() { tr("brak sieci").to_string() } else { addrs.join(", ") }));
    app.set_port(ss(host.server.port().to_string()));
    let (rec, since, held, fuel, last) = host.recording();
    app.set_recording(rec);
    app.set_recording_text(ss(if rec {
        let started = since.map(|s| fmt::duration_min((unix_ms() - s) / 60_000)).unwrap_or_default();
        let mut t = trf!("Od {start} · {dur}", start = since.map(fmt::time).unwrap_or_default(), dur = started);
        if let (Some(f0), Some(snap)) = (fuel, host.snapshot()) {
            let used = f0 - snap.fuel.total_kg;
            if used > 1.0 {
                t.push_str(&trf!(" · zużyte paliwo {kg} kg", kg = used.round()));
            }
        }
        t
    } else if held {
        tr("Wstrzymany do lądowania – „Wznów”, żeby nagrywać od razu").to_string()
    } else if let Some(l) = last {
        trf!("Ostatnie lądowanie: {score} · {vs} ft/min", score = l.score, vs = l.vs_fpm)
    } else {
        tr("Zacznie się samo po starcie").to_string()
    }));
    // Komunikaty z tła
    let mut msgs = Vec::new();
    for e in host.take_events() {
        match e {
            HostEvent::FlightSaved(_, route) => msgs.push(trf!("Lot {route} zapisany w dzienniku")),
            HostEvent::FlightDiscarded => msgs.push(tr("Zapis zakończony – loty krótsze niż 2 min nie trafiają do dziennika").to_string()),
        }
    }
    let mut synced: HashMap<String, (usize, usize, usize)> = HashMap::new();
    for e in host.server.take_events() {
        let opentakeoff_core::server::ServerEvent::Synced { device, received, sent, removed } = e;
        let x = synced.entry(device).or_default();
        x.0 += received;
        x.1 += sent;
        x.2 += removed;
    }
    for (d, (r, s, rm)) in synced {
        if r + s + rm > 0 {
            msgs.push(trf!("Synchronizacja z {d}: odebrano {r}, wysłano {s}", r = fmt::flights(r), s = fmt::flights(s)));
        }
    }
    // Aktywność
    if opentakeoff_core::log::serial() != st.log_serial {
        st.log_serial = opentakeoff_core::log::serial();
        let mut lines: Vec<LogV> = opentakeoff_core::log::recent().into_iter().map(|(t, text)| LogV { time: ss(fmt::clock(t)), text: ss(text.lines().next().unwrap_or("")) }).collect();
        lines.reverse();
        lines.truncate(300);
        app.set_activity(model(lines));
    }
    // Urządzenia
    let owner = clients.iter().find(|c| c.controlling).map(|c| c.name.clone());
    let devices: Vec<DevV> = host
        .pairing
        .devices()
        .into_iter()
        .map(|d| {
            let session = clients.iter().find(|c| c.paired && (c.name == d.name || c.ip == d.last_ip) && !d.name.is_empty());
            let connected = session.is_some();
            let controlling = session.is_some_and(|c| c.controlling);
            DevV {
                token: ss(&d.token),
                name: ss(if d.name.is_empty() { tr("Telefon (sparowany wcześniej)").to_string() } else { d.name.clone() }),
                ip: ss(&d.last_ip),
                status: ss(if controlling {
                    tr("Połączony · steruje samolotem").to_string()
                } else if let Some(c) = session {
                    trf!("Połączony · {ip} · {hz} Hz", ip = c.ip, hz = c.rate_hz)
                } else {
                    tr("Niepołączony").to_string()
                }),
                last_seen: ss(trf!("Ostatnio: {t}", t = fmt::ago(d.last_seen))),
                last_sync: ss(trf!("Synchronizacja: {t}", t = fmt::ago(d.last_sync))),
                connected,
                controlling,
                session: ss(session.map(|c| c.id.clone()).unwrap_or_default()),
            }
        })
        .collect();
    app.set_devices(model(devices));
    let live = app.get_live();
    if live.controlled_by.as_str() != owner.clone().unwrap_or_default() {
        let mut l = live;
        l.controlled_by = ss(owner.map(|n| trf!("{n} – przechylanie telefonu albo drążek")).unwrap_or_default());
        app.set_live(l);
    }
    // Baza lotnisk
    if !st.db_loaded {
        if let Some(db) = airports::db() {
            st.db_loaded = true;
            app.set_db_status(ss(trf!("Baza lotnisk: {n} · {d}", n = db.len(), d = fmt::date(db.fetched_at))));
        }
    }
    let size: u64 = ["flights.oab", "trash.oab", "meta.json"].iter().filter_map(|f| std::fs::metadata(host.logbook.dir().join(f)).ok()).map(|m| m.len()).sum();
    app.set_logbook_size(ss(format!("{} · {}", fmt::flights(host.logbook.len()), fmt::bytes(size))));
    drop(st);
    for m in msgs {
        toast(m);
    }
}

// ---------- Na żywo ----------

fn phase(s: &opentakeoff_core::proto::FlightState) -> String {
    if s.aircraft.on_ground {
        if s.speed.gs_kt > 30.0 {
            tr("rozbieg / dobieg").into()
        } else if s.speed.gs_kt > 2.0 {
            tr("kołowanie").into()
        } else {
            tr("na ziemi").into()
        }
    } else if s.speed.vs_fpm > 400.0 {
        tr("wznoszenie").into()
    } else if s.speed.vs_fpm < -400.0 {
        if s.position.alt_agl_ft < 3000.0 {
            tr("podejście").into()
        } else {
            tr("zniżanie").into()
        }
    } else {
        tr("przelot").into()
    }
}

/// Etap lotu na osi karty Na żywo: 0 kołowanie, 1 start, 2 wznoszenie, 3 przelot, 4 zniżanie,
/// 5 podejście, 6 po lądowaniu. Nagrywanie zaczyna się dopiero w powietrzu, więc rozbieg z trwającym
/// nagraniem to dobieg po lądowaniu.
fn phase_index(s: &opentakeoff_core::proto::FlightState, recording: bool) -> i32 {
    if s.aircraft.on_ground {
        match (recording, s.speed.gs_kt > 30.0) {
            (false, false) => 0,
            (false, true) => 1,
            (true, _) => 6,
        }
    } else if s.speed.vs_fpm > 400.0 {
        2
    } else if s.speed.vs_fpm < -400.0 {
        if s.position.alt_agl_ft < 3000.0 {
            5
        } else {
            4
        }
    } else if s.position.alt_agl_ft < 1500.0 && s.speed.vs_fpm < -100.0 {
        5
    } else {
        3
    }
}

fn parse_sim_utc(t: Option<&str>) -> Option<chrono::DateTime<chrono::Utc>> {
    let t = t?;
    chrono::DateTime::parse_from_rfc3339(t).ok().map(|d| d.with_timezone(&chrono::Utc)).or_else(|| chrono::NaiveDateTime::parse_from_str(t.trim_end_matches('Z'), "%Y-%m-%dT%H:%M:%S").ok().map(|n| n.and_utc()))
}

/// Godzina symulatora „HH:MM:SSZ”.
fn utc_clock(t: Option<&str>) -> String {
    parse_sim_utc(t).map(|d| d.format("%H:%MZ").to_string()).unwrap_or_default()
}

/// Przewidywana godzina przylotu (czas symulatora + pozostały czas).
fn utc_plus(t: Option<&str>, sec: f64) -> String {
    parse_sim_utc(t).map(|d| (d + chrono::Duration::seconds(sec as i64)).format("%H:%MZ").to_string()).unwrap_or_default()
}

fn lamp(k: &str, v: &str, on: bool, tone: i32) -> LampV {
    LampV { k: ss(k), v: ss(v), on, tone }
}

// Zegar silnika jak na E/WD: łuk 210° od lewego dołu (0 %) do prawej (100 %), środek (50, 56), promień 38,
// w polu 100×100. Kąt liczony od góry zgodnie z ruchem wskazówek.
const GAUGE_START: f64 = -120.0;
const GAUGE_SWEEP: f64 = 210.0;

fn gauge_pt(frac: f64, r: f64) -> (f64, f64) {
    let a = (GAUGE_START + GAUGE_SWEEP * frac).to_radians();
    (50.0 + r * a.sin(), 56.0 - r * a.cos())
}

fn gauge_arc(frac: f64) -> String {
    let f = frac.clamp(0.0, 1.1);
    if f < 0.005 {
        return String::new();
    }
    let (x0, y0) = gauge_pt(0.0, 38.0);
    let (x1, y1) = gauge_pt(f, 38.0);
    let large = if GAUGE_SWEEP * f > 180.0 { 1 } else { 0 };
    format!("M {x0:.2} {y0:.2} A 38 38 0 {large} 1 {x1:.2} {y1:.2}")
}

fn gauge_needle(frac: f64) -> String {
    let f = frac.clamp(0.0, 1.1);
    let (x1, y1) = gauge_pt(f, 42.0);
    let (x0, y0) = gauge_pt(f, 8.0);
    format!("M {x0:.2} {y0:.2} L {x1:.2} {y1:.2}")
}

/// Znacznik położenia dźwigni na zewnątrz łuku.
fn gauge_tick(frac: f64) -> String {
    let (x0, y0) = gauge_pt(frac, 43.0);
    let (x1, y1) = gauge_pt(frac, 49.0);
    format!("M {x0:.2} {y0:.2} L {x1:.2} {y1:.2}")
}

/// Profil wysokości z bieżącego śladu: linia i wypełnienie w polu 200×60 (do 160 próbek).
fn alt_profile(track: &[crate::host::LivePoint]) -> (String, String, i32) {
    if track.len() < 2 {
        return (String::new(), String::new(), 0);
    }
    let n = track.len().min(160);
    let pts: Vec<i32> = (0..n).map(|i| track[i * (track.len() - 1) / (n - 1).max(1)].alt).collect();
    let top = pts.iter().copied().max().unwrap_or(0);
    let lo = pts.iter().copied().min().unwrap_or(0).min(0);
    let span = ((top - lo) as f64).max(500.0);
    let mut line = String::new();
    for (i, a) in pts.iter().enumerate() {
        let x = i as f64 * 200.0 / (n - 1) as f64;
        let y = 58.0 - (*a - lo) as f64 / span * 54.0;
        line.push_str(&format!("{} {x:.1} {y:.1} ", if i == 0 { "M" } else { "L" }));
    }
    let area = format!("{line}L 200 60 L 0 60 Z");
    (line, area, top)
}

pub fn refresh_live(force_map: bool) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let tab = app.get_tab();
    // Dane lotu trafiają do paska narzędzi i panelu bocznego na każdej karcie; mapa i PFD tylko
    // na Pulpicie i karcie Na żywo.
    let with_map = tab == 0 || tab == 1;
    let st = state();
    let mut st = st.borrow_mut();
    let u = units(&st.s);
    let snap = st.host.snapshot();
    let mut live = app.get_live();
    live.has_data = snap.is_some();
    if let Some(s) = &snap {
        let db = airports::db();
        let near = db.as_ref().and_then(|d| d.within(s.position.lat, s.position.lon, 60.0, false).into_iter().next().map(|(dist, a)| format!("{} {} · {:.0} NM", a.ident, a.name, dist)));
        let av = s.avionics.clone().unwrap_or_default();
        live.title = ss(&s.aircraft.title);
        live.category = ss(&s.aircraft.category);
        live.source = ss(&s.source);
        live.phase = ss(phase(s));
        live.on_ground = s.aircraft.on_ground;
        live.paused = s.aircraft.paused;
        live.alt = ss(u.alt(s.position.alt_ind_ft));
        live.alt_unit = ss(u.alt_unit());
        live.agl = ss(format!("{}", u.alt_full(if av.radio_alt_ft > 0.0 { av.radio_alt_ft } else { s.position.alt_agl_ft })));
        live.ias = ss(format!("{:.0}", s.speed.ias_kt));
        live.tas = ss(format!("{:.0}", s.speed.tas_kt));
        live.gs = ss(format!("{:.0}", s.speed.gs_kt));
        live.mach = ss(if s.speed.mach > 0.3 { format!("{:.2}", s.speed.mach) } else { String::new() });
        live.vs = ss(format!("{:+.0}", s.speed.vs_fpm));
        live.vs_value = s.speed.vs_fpm as f32;
        live.hdg = ss(format!("{:03.0}", s.attitude.hdg_mag_deg.rem_euclid(360.0)));
        live.trk = ss(format!("{:03.0}", s.attitude.track_true_deg.rem_euclid(360.0)));
        live.pitch = s.attitude.pitch_deg as f32;
        live.bank = s.attitude.bank_deg as f32;
        live.position = ss(fmt::latlon(s.position.lat, s.position.lon));
        live.nearest = ss(near.unwrap_or_default());
        live.wind = ss(format!("{:03.0}° / {:.0} kt", s.environment.wind_dir_deg, s.environment.wind_spd_kt));
        live.oat = ss(format!("{:.0} °C", s.environment.oat_c));
        live.qnh = ss(if av.baro_std { "STD".to_string() } else { format!("{:.0} hPa", s.environment.qnh_hpa) });
        live.fuel = ss(format!("{:.0} kg", s.fuel.total_kg));
        let (_, rec_since, _, fuel0, last) = st.host.recording();
        live.fuel_level = fuel0.map(|f| (s.fuel.total_kg / f.max(1.0)) as f32).unwrap_or(1.0);
        let ap = &s.autopilot;
        let mut modes = Vec::new();
        for (on, n) in [(ap.modes.hdg, "HDG"), (ap.modes.nav, "NAV"), (ap.modes.loc, "LOC"), (ap.modes.apr, "APR"), (ap.modes.alt, "ALT"), (ap.modes.vs, "V/S"), (ap.modes.flc, "FLC")] {
            if on {
                modes.push(n);
            }
        }
        if ap.athr {
            modes.push("A/THR");
        }
        live.ap_line = ss(modes.join("  "));
        live.ap_master = ap.master;
        live.ap_alt = ss(format!("{:.0}", ap.alt_ft));
        live.ap_hdg = ss(format!("{:03.0}°", ap.hdg_deg));
        live.ap_spd = ss(if av.ap.mach_hold { format!("M{:.2}", av.ap.mach_var) } else { format!("{:.0} kt", ap.spd_kt) });
        live.ap_vs = ss(format!("{:+.0} ft/min", ap.vs_fpm));
        let r = &s.radios;
        live.com1 = ss(format!("{:.3}  ⇄  {:.3}", r.com1.active_mhz, r.com1.standby_mhz));
        live.com2 = ss(format!("{:.3}  ⇄  {:.3}", r.com2.active_mhz, r.com2.standby_mhz));
        live.nav1 = ss(format!("{:.2}  ⇄  {:.2}{}", r.nav1.active_mhz, r.nav1.standby_mhz, if av.nav1.ident.is_empty() { String::new() } else { format!("  {}", av.nav1.ident) }));
        live.nav2 = ss(format!("{:.2}  ⇄  {:.2}", r.nav2.active_mhz, r.nav2.standby_mhz));
        live.xpdr = ss(format!("{}  {}", r.transponder.code, r.transponder.mode.to_uppercase()));
        let c = &s.controls;
        live.gear = ss(if c.gear_pct >= 99.0 { tr("wypuszczone").to_string() } else if c.gear_pct <= 1.0 { tr("schowane").to_string() } else { format!("{:.0} %", c.gear_pct) });
        live.flaps = ss(format!("{} / {} ({:.0} %)", c.flaps_index, (c.flaps_positions - 1).max(0), c.flaps_pct));
        live.spoilers = ss(if c.spoilers_armed { tr("uzbrojone").to_string() } else { format!("{:.0} %", c.spoilers_pct) });
        live.brake = c.parking_brake;
        live.gear_short = ss(if c.gear_pct >= 99.0 { "DN".to_string() } else if c.gear_pct <= 1.0 { "UP".to_string() } else { format!("{:.0}%", c.gear_pct) });
        live.gear_on = c.gear_pct > 1.0;
        live.flaps_short = ss(if c.flaps_index <= 0 { "UP".to_string() } else { c.flaps_index.to_string() });
        live.spoilers_short = ss(if c.spoilers_armed { "ARM".to_string() } else if c.spoilers_pct > 1.0 { format!("{:.0}%", c.spoilers_pct) } else { "RET".to_string() });
        live.spoilers_on = c.spoilers_armed || c.spoilers_pct > 1.0;
        let l = &s.lights;
        let lights: Vec<&str> = [(l.beacon, "BCN"), (l.strobe, "STRB"), (l.nav, "NAV"), (l.landing, "LDG"), (l.taxi, "TAXI"), (l.logo, "LOGO")].into_iter().filter(|x| x.0).map(|x| x.1).collect();
        live.lights = ss(if lights.is_empty() { tr("wyłączone").to_string() } else { lights.join(" ") });
        let g = &av.gps;
        live.next_wp = ss(if g.next_id.is_empty() { tr("brak planu lotu w symulatorze").to_string() } else { format!("{} ({:03.0}°)", g.next_id, g.next_bearing_mag_deg) });
        live.next_dist = ss(if g.next_id.is_empty() { String::new() } else { u.dist(g.next_dist_nm) });
        live.next_ete = ss(fmt::ete(g.next_ete_sec));
        live.dest_ete = ss(fmt::ete(g.dest_ete_sec));
        live.wp_progress = if g.wp_count > 1 { (g.wp_index as f32 / (g.wp_count - 1) as f32).clamp(0.0, 1.0) } else { 0.0 };
        live.sim_time = ss(s.sim_time_utc.clone().unwrap_or_default().replace('T', " ").replace('Z', " UTC"));
        live.touchdown = ss(last.map(|l| format!("{} · {} · {} ft/min{}\n{}", l.score, landing::grade_of(l.score), l.vs_fpm, l.runway.as_ref().map(|r| format!(" · RWY {r}")).unwrap_or_default(), landing::notes_now(&l).join(" "))).unwrap_or_default());
        // ---- Karta Na żywo ----
        live.phase_index = phase_index(s, rec_since.is_some());
        live.utc = ss(utc_clock(s.sim_time_utc.as_deref()));
        live.flight_time = ss(rec_since.map(|t| fmt::duration_min((unix_ms() - t) / 60_000)).unwrap_or_default());
        live.eta = ss(if g.dest_ete_sec > 0.0 { utc_plus(s.sim_time_utc.as_deref(), g.dest_ete_sec) } else { String::new() });
        // FMA
        let m = &ap.modes;
        live.fma_thr = ss(if ap.athr { if av.ap.mach_hold { "MACH" } else { "SPEED" } } else { "" });
        live.fma_vert = ss(if m.apr && av.nav1.has_gs { "G/S" } else if m.flc { if ap.alt_ft > s.position.alt_ind_ft { "OP CLB" } else { "OP DES" } } else if m.vs { "V/S" } else if m.alt { "ALT" } else { "" });
        live.fma_lat = ss(if m.apr || m.loc { "LOC" } else if m.nav { if g.active_flight_plan { "NAV" } else { "VOR" } } else if m.hdg { "HDG" } else { "" });
        live.ap_fd = ap.fd;
        live.ap_athr = ap.athr;
        // Radia
        live.com1_a = ss(format!("{:.3}", r.com1.active_mhz));
        live.com1_s = ss(format!("{:.3}", r.com1.standby_mhz));
        live.com2_a = ss(format!("{:.3}", r.com2.active_mhz));
        live.com2_s = ss(format!("{:.3}", r.com2.standby_mhz));
        live.nav1_a = ss(format!("{:.2}", r.nav1.active_mhz));
        live.nav1_s = ss(format!("{:.2}", r.nav1.standby_mhz));
        live.nav1_id = ss(&av.nav1.ident);
        live.nav1_dme = ss(if av.nav1.has_dme && av.nav1.dme_nm > 0.0 { format!("{:.1} NM", av.nav1.dme_nm) } else { String::new() });
        live.nav2_a = ss(format!("{:.2}", r.nav2.active_mhz));
        live.nav2_s = ss(format!("{:.2}", r.nav2.standby_mhz));
        live.xpdr_code = ss(&r.transponder.code);
        live.xpdr_mode = ss(r.transponder.mode.to_uppercase());
        // Wiatr względem dziobu: składowa czołowa (+) / tylna (−) i boczna
        let rel_deg = (s.environment.wind_dir_deg - s.attitude.hdg_true_deg).rem_euclid(360.0);
        let ws = s.environment.wind_spd_kt;
        let (head, cross) = (ws * rel_deg.to_radians().cos(), ws * rel_deg.to_radians().sin());
        live.wind_rel = rel_deg as f32;
        live.wind_spd = ss(format!("{:.0}", ws));
        live.wind_head = ss(if ws < 1.0 { String::new() } else if head >= 0.0 { trf!("czołowy {kt} kt", kt = head.round()) } else { trf!("tylny {kt} kt", kt = (-head).round()) });
        live.wind_cross = ss(if cross.abs() < 1.0 { String::new() } else if cross > 0.0 { trf!("z prawej {kt} kt", kt = cross.round()) } else { trf!("z lewej {kt} kt", kt = (-cross).round()) });
        // Paliwo: przepływ, zasięg w czasie, zapas na celu
        let ff: f64 = s.engines.iter().map(|e| e.fuel_flow_kg_h).sum();
        live.fuel_flow = ss(if ff > 1.0 { format!("{:.0} kg/h", ff) } else { String::new() });
        live.fuel_endurance = ss(if ff > 1.0 { fmt::ete(s.fuel.total_kg / ff * 3600.0) } else { String::new() });
        let at_dest = if ff > 1.0 && g.dest_ete_sec > 0.0 { Some(s.fuel.total_kg - ff * g.dest_ete_sec / 3600.0) } else { None };
        live.fuel_dest = ss(at_dest.map(|f| format!("{:.0} kg", f.max(0.0))).unwrap_or_default());
        live.fuel_dest_low = at_dest.is_some_and(|f| f < ff * 0.75);
        let t = &s.fuel.tanks;
        let tmax = t.left_kg.max(t.right_kg).max(t.center_kg).max(1.0);
        live.tank_l = ss(format!("{:.0}", t.left_kg));
        live.tank_c = ss(format!("{:.0}", t.center_kg));
        live.tank_r = ss(format!("{:.0}", t.right_kg));
        live.tank_l_level = (t.left_kg / tmax) as f32;
        live.tank_c_level = (t.center_kg / tmax) as f32;
        live.tank_r_level = (t.right_kg / tmax) as f32;
        live.has_center = t.center_kg > 0.5;
        // Parametry lotu
        live.g_force = ss(if av.g_force != 0.0 { format!("{:.2}", av.g_force) } else { String::new() });
        live.aoa = ss(if av.aoa_deg != 0.0 { format!("{:.1}°", av.aoa_deg) } else { String::new() });
        live.tat = ss(if av.tat_c != 0.0 { format!("{:.0} °C", av.tat_c) } else { String::new() });
        live.trim = ss(format!("{:+.0} %", c.elevator_trim_pct));
        live.stall = av.stall_warning;
        live.overspeed = av.overspeed_warning;
        live.has_route = !g.next_id.is_empty();
        if tab == 1 {
            let (line, area, top) = alt_profile(&st.host.live_track());
            live.alt_line = ss(line);
            live.alt_area = ss(area);
            live.alt_top = ss(if top > 0 { u.alt_full(top as f64) } else { String::new() });
        }
        // Lampki systemów i świateł
        let gear_tone = if c.gear_pct >= 99.0 { 0 } else { 1 };
        let marker = av.marker.to_uppercase();
        let lamps = vec![
            lamp("GEAR", &live.gear_short, c.gear_pct > 1.0, gear_tone),
            lamp("FLAPS", &live.flaps_short, c.flaps_index > 0, 3),
            lamp("SPLR", &live.spoilers_short, live.spoilers_on, if c.spoilers_pct > 1.0 { 1 } else { 0 }),
            lamp("BRAKE", if c.parking_brake { "SET" } else { "OFF" }, c.parking_brake, 1),
            lamp("STALL", if av.stall_warning { "WARN" } else { "" }, av.stall_warning, 2),
            lamp("OVSPD", if av.overspeed_warning { "WARN" } else { "" }, av.overspeed_warning, 2),
            lamp("MKR", &marker, !marker.is_empty() && marker != "NONE", 3),
            lamp("SIM", if s.aircraft.paused { "PAUSE" } else { "RUN" }, s.aircraft.paused, 1),
        ];
        app.set_live_lamps(model(lamps));
        let lt = &s.lights;
        let lights: Vec<LampV> = [(lt.beacon, "BCN"), (lt.strobe, "STRB"), (lt.nav, "NAV"), (lt.landing, "LDG"), (lt.taxi, "TAXI"), (lt.logo, "LOGO"), (lt.wing, "WING")]
            .into_iter()
            .map(|(on, k)| lamp(k, "", on, 0))
            .collect();
        app.set_live_lights(model(lights));
        let engines: Vec<EngineV> = s
            .engines
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let turbine = e.n1_pct > 0.5;
                let frac = if turbine { e.n1_pct / 100.0 } else { e.rpm / 2700.0 };
                let thr = (e.throttle_pct / 100.0).clamp(0.0, 1.0);
                EngineV {
                    label: ss(trf!("Silnik {n}", n = i + 1)),
                    main: ss(if turbine { format!("N1 {:.1} %", e.n1_pct) } else { format!("{:.0} RPM", e.rpm) }),
                    sub: ss(format!("EGT {:.0} °C · {:.0} kg/h · {} {:.0} %", e.egt_c, e.fuel_flow_kg_h, tr("dźwignia"), e.throttle_pct)),
                    level: frac.clamp(0.0, 1.0) as f32,
                    kind: ss(if turbine { "N1" } else { "RPM" }),
                    value: ss(if turbine { format!("{:.1}", e.n1_pct) } else { format!("{:.0}", e.rpm) }),
                    arc: ss(gauge_arc(frac)),
                    needle: ss(gauge_needle(frac)),
                    thr_tick: ss(gauge_tick(thr)),
                    egt: ss(format!("{:.0}", e.egt_c)),
                    egt_level: (e.egt_c / if turbine { 950.0 } else { 900.0 }).clamp(0.0, 1.0) as f32,
                    ff: ss(format!("{:.0}", e.fuel_flow_kg_h)),
                    n2: ss(if turbine && e.n2_pct > 0.5 { format!("{:.1}", e.n2_pct) } else if e.prop_rpm > 1.0 { format!("{:.0}", e.prop_rpm) } else { String::new() }),
                    warn: frac > 1.0,
                }
            })
            .collect();
        app.set_engines(model(engines));
    }
    app.set_live(live);
    if tab == 1 {
        if let Some(img) = crate::instruments::live_pfd(snap.as_ref(), st.scale, &st.s.instr_style) {
            app.set_pfd(img);
        }
    }
    // Mapa: co 0,5 s albo od razu po zmianie widoku
    if with_map && (force_map || st.live_render.elapsed().as_millis() > 500) {
        st.live_render = Instant::now();
        let dark = ui.global::<Theme>().get_dark();
        let track = st.host.live_track();
        let mut ov = Overlay::default();
        if track.len() > 1 {
            ov.tracks.push(Track { pts: track.iter().map(|p| (p.lat, p.lon, p.alt)).collect(), color: None, width: 3.0 });
        }
        if let Some(s) = &snap {
            ov.aircraft = Some((s.position.lat, s.position.lon, s.attitude.hdg_true_deg));
            if let Some(av) = &s.avionics {
                if av.gps.next_lat != 0.0 && av.gps.prev_lat != 0.0 {
                    ov.route = vec![(av.gps.prev_lat, av.gps.prev_lon), (av.gps.next_lat, av.gps.next_lon)];
                }
            }
            if st.views[0].follow {
                st.views[0].lat = s.position.lat;
                st.views[0].lon = s.position.lon;
                if !st.views[0].user_moved && st.views[0].zoom < 8.0 {
                    st.views[0].zoom = 9.0;
                }
            }
            if let Some(db) = airports::db() {
                let v = &st.views[0];
                if v.zoom > 7.0 {
                    let radius = (v.w.max(v.h) * 156_543.0 * v.lat.to_radians().cos() / 2f64.powf(v.zoom) / 1852.0).min(200.0);
                    for (_, a) in db.within(v.lat, v.lon, radius, false).into_iter().filter(|(_, a)| a.kind <= 1 || v.zoom > 9.5).take(80) {
                        ov.points.push(Point { lat: a.lat, lon: a.lon, label: a.ident.clone(), kind: if a.kind <= 1 { 0 } else { 3 } });
                        if v.zoom > 11.0 {
                            for r in a.runways.iter().filter(|r| r.has_coords() && !r.closed) {
                                ov.runways.push((r.le_lat, r.le_lon, r.he_lat, r.he_lon));
                            }
                        }
                    }
                }
            }
        }
        app.set_map_attribution(style(&st, dark).attribution().into());
        let r = map::render(&st.views[0], style(&st, dark), &ov, st.scale, dark);
        app.set_map_live(r.image);
        app.set_map_live_labels(labels(r.labels));
        app.set_map_live_scale(ss(st.views[0].scale_text()));
    }
}

// ---------- Dziennik ----------

fn matches(f: &FlightLog, meta: &opentakeoff_core::logbook::FlightMeta, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    let hay = airports::fold(&format!(
        "{} {} {} {} {} {} {}",
        f.from.as_deref().unwrap_or(""),
        f.to.as_deref().unwrap_or(""),
        f.aircraft.as_deref().unwrap_or(""),
        meta.tags.join(" "),
        meta.notes,
        fmt::date(f.start),
        meta.source
    ));
    airports::fold(q).split_whitespace().all(|w| hay.contains(w))
}

pub fn refresh_logbook() {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let mut st = st.borrow_mut();
    let u = units(&st.s);
    let store = st.host.logbook.clone();
    st.logbook_rev = store.revision();
    let trash = app.get_show_trash();
    let q = app.get_filter_text().to_string();
    let period = app.get_filter_period();
    let sort = app.get_sort_by();
    let metas = store.all_meta();
    let now = Local::now();
    let mut flights: Vec<FlightLog> = (if trash { store.trash() } else { store.flights() })
        .iter()
        .map(|f| store.effective(f))
        .filter(|f| {
            let m = metas.get(&f.id).cloned().unwrap_or_default();
            let in_period = match period {
                1 => unix_ms() - f.start < 30 * 86_400_000,
                2 => Local.timestamp_millis_opt(f.start).single().is_some_and(|d| d.year() == now.year()),
                3 => m.favorite,
                _ => true,
            };
            in_period && matches(f, &m, &q)
        })
        .collect();
    match sort {
        1 => flights.sort_by(|a, b| (b.end - b.start).cmp(&(a.end - a.start))),
        2 => flights.sort_by(|a, b| b.last_landing().map(|l| l.score).unwrap_or(-1).cmp(&a.last_landing().map(|l| l.score).unwrap_or(-1))),
        _ => flights.sort_by(|a, b| b.start.cmp(&a.start)),
    }
    let mut rows = Vec::new();
    let mut last_month = String::new();
    st.list = flights.iter().map(|f| f.id.clone()).collect();
    let selected = st.selected.clone();
    for f in &flights {
        if sort == 0 {
            if let Some(d) = Local.timestamp_millis_opt(f.start).single() {
                let key = format!("{} {}", fmt::month_name(d.month()), d.year()).to_uppercase();
                if key != last_month {
                    last_month = key.clone();
                    rows.push(FlightV { header: true, month: ss(key), score: -1, ..Default::default() });
                }
            }
        }
        let m = metas.get(&f.id).cloned().unwrap_or_default();
        let l = f.last_landing();
        rows.push(FlightV {
            id: ss(&f.id),
            date: ss(fmt::date(f.start)),
            month: ss(""),
            time: ss(fmt::time(f.start)),
            from: ss(f.from.clone().unwrap_or_else(|| "?".into())),
            to: ss(f.to.clone().unwrap_or_else(|| "?".into())),
            aircraft: ss(f.aircraft.clone().unwrap_or_else(|| tr("nieznany samolot").into())),
            category: ss(f.category.clone().unwrap_or_default()),
            duration: ss(fmt::duration_min(f.duration_min())),
            distance: ss(u.dist(f.distance_nm as f64)),
            max_alt: ss(u.alt_full(f.max_alt_ft as f64)),
            score: l.as_ref().map(|l| l.score).unwrap_or(-1),
            grade: ss(l.map(|l| landing::grade_of(l.score).to_string()).unwrap_or_default()),
            landings: f.all_landings().len() as i32,
            source: ss(if m.source != "pc" && !m.source.is_empty() && m.source != "import" { m.source.clone() } else { String::new() }),
            favorite: m.favorite,
            tags: ss(m.tags.join(", ")),
            notes: !m.notes.trim().is_empty(),
            selected: selected.contains(&f.id),
            header: false,
        });
    }
    let total_min: i64 = flights.iter().map(|f| f.duration_min()).sum();
    app.set_flights(model(rows));
    app.set_flights_summary(ss(format!("{} · {}", fmt::flights(flights.len()), fmt::hours(total_min))));
    app.set_trash_count(store.trash().len() as i32);
    app.set_selected_count(st.selected.len() as i32);
}

fn flight_title(f: &FlightLog) -> String {
    format!("{}  →  {}", f.from.as_deref().unwrap_or("?"), f.to.as_deref().unwrap_or("?"))
}

pub fn open_flight(id: &str) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let mut st = st.borrow_mut();
    let store = st.host.logbook.clone();
    let Some(raw) = store.get(id) else {
        app.set_has_detail(false);
        return;
    };
    let in_trash = store.trash().iter().any(|f| f.id == id);
    let f = Arc::new(store.effective(&raw));
    let meta = store.meta(id);
    let u = units(&st.s);
    st.detail = Some(f.clone());
    st.replay = 0.0;
    st.playing = false;
    st.anchor = Some(id.to_string());
    let t = &f.track;
    let gs: Vec<f64> = t.iter().filter(|p| !p.on_ground.unwrap_or(false) && p.gs_kt > 30).map(|p| p.gs_kt as f64).collect();
    let avg = if gs.is_empty() { 0.0 } else { gs.iter().sum::<f64>() / gs.len() as f64 };
    let maxgs = gs.iter().cloned().fold(0.0, f64::max);
    let maxvs = t.iter().filter_map(|p| p.vs_fpm).max().unwrap_or(0);
    let minvs = t.iter().filter_map(|p| p.vs_fpm).min().unwrap_or(0);
    let l = f.last_landing();
    let source = match meta.source.as_str() {
        "pc" => tr("Nagrany na tym komputerze").to_string(),
        "import" | "" => tr("Zaimportowany").to_string(),
        s => trf!("Z telefonu: {s}"),
    };
    let detail = DetailV {
        id: ss(id),
        title: ss(flight_title(&f)),
        route: ss(flight_title(&f)),
        subtitle: ss(format!(
            "{} · {} – {} · {}",
            fmt::date(f.start),
            fmt::time(f.start),
            fmt::time(f.end),
            f.aircraft.clone().unwrap_or_else(|| tr("nieznany samolot").into())
        )),
        aircraft: ss(f.aircraft.clone().unwrap_or_default()),
        category: ss(f.category.clone().unwrap_or_default()),
        from: ss(f.from.clone().unwrap_or_default()),
        to: ss(f.to.clone().unwrap_or_default()),
        duration: ss(fmt::duration_min(f.duration_min())),
        distance: ss(u.dist(f.distance_nm as f64)),
        max_alt: ss(u.alt_full(f.max_alt_ft as f64)),
        avg_gs: ss(u.spd(avg)),
        max_gs: ss(u.spd(maxgs)),
        max_vs: ss(format!("+{maxvs} ft/min")),
        min_vs: ss(format!("{minvs} ft/min")),
        points: ss(t.len().to_string()),
        source: ss(format!("{source} · {} {}", t.len(), tr("punktów ścieżki"))),
        notes: ss(&meta.notes),
        tags: ss(meta.tags.join(", ")),
        favorite: meta.favorite,
        in_trash,
        score: l.as_ref().map(|l| l.score).unwrap_or(-1),
        grade: ss(l.map(|l| landing::grade_of(l.score).to_string()).unwrap_or_default()),
        cursor_info: ss(""),
        cursor_pos: 0.0,
    };
    app.set_detail(detail);
    let landings: Vec<LandingV> = f
        .all_landings()
        .iter()
        .enumerate()
        .map(|(i, l)| LandingV {
            title: ss(format!("{} {}{}", tr("Lądowanie"), i + 1, l.airport.as_ref().map(|a| format!(" · {a}")).unwrap_or_default())),
            score: l.score,
            grade: ss(landing::grade_of(l.score)),
            vs: ss(format!("{} ft/min", l.vs_fpm)),
            g: ss(l.g_force.map(|g| format!("{g:.2} g")).unwrap_or_default()),
            runway: ss(l.runway.as_ref().map(|r| format!("RWY {r}")).unwrap_or_default()),
            along: ss(l.along_ft.map(|a| trf!("{a} ft za progiem")).unwrap_or_default()),
            notes: ss(landing::notes_now(l).join(" ")),
            at: ss(l.at.map(fmt::time).unwrap_or_default()),
            has_profile: l.profile.len() >= 10,
        })
        .collect();
    app.set_detail_landings(model(landings));
    // Analiza: ostatnie lądowanie z zapisanym przebiegiem
    let all = f.all_landings();
    let pick = all.iter().rposition(|l| l.profile.len() >= 10).or(if all.is_empty() { None } else { Some(all.len() - 1) });
    show_analysis(&app, &f, pick);
    let span = (f.end - f.start).max(1) as f32;
    let markers: Vec<MarkerV> =
        f.markers.iter().map(|m| MarkerV { t: (m.t / 1000) as i32, time: ss(fmt::clock(m.t)), label: ss(&m.label), pos: ((m.t - t.first().map(|p| p.t).unwrap_or(f.start)) as f32 / span).clamp(0.0, 1.0) }).collect();
    app.set_detail_markers(model(markers));
    let shots: Vec<ShotV> = crate::shots::for_flight(&f)
        .iter()
        .map(|s| ShotV {
            id: ss(&s.id),
            thumb: crate::shots::thumb_image(s),
            time: ss(fmt::clock(s.time)),
            date: ss(fmt::date(s.time)),
            place: ss(&s.nearest),
            alt: ss(if s.has_pos { u.alt_full(s.alt_ft) } else { String::new() }),
            has_pos: s.has_pos,
            ..Default::default()
        })
        .collect();
    app.set_detail_shots(model(shots));
    set_chart(&app, &f, &st.s);
    app.set_has_detail(true);
    app.set_replay_playing(false);
    // Mapa dopasowana do toru
    if let Some(b) = map::bounds(t.iter().map(|p| (p.lat, p.lon))) {
        st.views[1].fit(b);
    }
    drop(st);
    render_flight_map();
    update_cursor();
}

fn show_analysis(app: &App, f: &FlightLog, idx: Option<usize>) {
    let all = f.all_landings();
    match idx.and_then(|i| all.get(i).map(|l| (i, l))) {
        Some((i, l)) => {
            let title = format!("{} {}{}", tr("Lądowanie"), i + 1, l.at.map(|t| format!(" · {}", fmt::time(t))).unwrap_or_default());
            app.set_analysis(crate::analysis::build(l, title));
            app.set_analysis_index(i as i32);
        }
        None => {
            app.set_analysis(AnalysisV::default());
            app.set_analysis_index(-1);
        }
    }
}

/// Analiza wybranego lądowania bieżącego lotu.
pub fn select_landing(i: i32) {
    let Some(ui) = ui() else { return };
    let f = state().borrow().detail.clone();
    if let Some(f) = f {
        show_analysis(&ui.global::<App>(), &f, usize::try_from(i).ok());
    }
}

fn set_chart(app: &App, f: &FlightLog, s: &Settings) {
    let c = crate::chart::build(f, s.units_alt == 1, s.units_speed == 1);
    let lv = |v: Vec<crate::chart::Label>| model(v.into_iter().map(|l| LabelV { x: l.x, y: l.y, text: ss(l.text), kind: l.kind }).collect());
    let y2 = if app.get_chart_mode() == 2 { crate::chart::vs_labels(f) } else { c.y2_labels };
    let y1 = if app.get_chart_mode() == 0 { c.y_labels } else { Vec::new() };
    app.set_chart(ChartV {
        alt: ss(c.alt),
        spd: ss(c.spd),
        gs: ss(c.gs),
        vs: ss(c.vs),
        ground: ss(c.ground),
        grid: ss(c.grid),
        y_labels: lv(y1),
        x_labels: lv(c.x_labels),
        y2_labels: lv(y2),
        markers: lv(c.markers),
        has_ias: c.has_ias,
        has_vs: c.has_vs,
    });
}

pub fn render_flight_map() {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let st = st.borrow();
    let Some(f) = st.detail.clone() else { return };
    let dark = ui.global::<Theme>().get_dark();
    let mut ov = Overlay::default();
    ov.tracks.push(Track { pts: f.track.iter().map(|p| (p.lat, p.lon, p.alt_ft)).collect(), color: None, width: 3.0 });
    let db = airports::db();
    for code in [f.from.as_deref(), f.to.as_deref()].into_iter().flatten() {
        if let Some(a) = db.as_ref().and_then(|d| d.get(code)) {
            ov.points.push(Point { lat: a.lat, lon: a.lon, label: a.ident.clone(), kind: 0 });
            for r in a.runways.iter().filter(|r| r.has_coords() && !r.closed) {
                ov.runways.push((r.le_lat, r.le_lon, r.he_lat, r.he_lon));
            }
        }
    }
    for (i, l) in f.all_landings().iter().enumerate() {
        if let Some(p) = l.at.and_then(|at| f.track.iter().min_by_key(|p| (p.t - at).abs())) {
            ov.points.push(Point { lat: p.lat, lon: p.lon, label: format!("{} {} · {}", tr("Lądowanie"), i + 1, l.score), kind: 2 });
        }
    }
    for m in &f.markers {
        if let Some(p) = f.track.iter().min_by_key(|p| (p.t - m.t).abs()) {
            ov.points.push(Point { lat: p.lat, lon: p.lon, label: m.label.clone(), kind: 2 });
        }
    }
    for s in crate::shots::for_flight(&f).iter().filter(|s| s.has_pos) {
        ov.points.push(Point { lat: s.lat, lon: s.lon, label: format!("{} {}", tr("Zrzut"), fmt::time(s.time)), kind: 4 });
    }
    if let Some(p) = replay_point(&f, st.replay) {
        ov.aircraft = Some((p.lat, p.lon, p.hdg.unwrap_or(0) as f64));
    }
    if st.replay_3d {
        let img = render_flight_3d(&f, &st, dark);
        app.set_map_flight(img);
        app.set_map_flight_labels(model(Vec::new()));
        return;
    }
    let r = map::render(&st.views[1], style(&st, dark), &ov, st.scale, dark);
    app.set_map_flight(r.image);
    app.set_map_flight_labels(labels(r.labels));
}

/// Odtwarzanie lotu w 3D: kamera za samolotem w interpolowanej pozycji, cały ślad i pasy lotnisk.
fn render_flight_3d(f: &FlightLog, st: &State, dark: bool) -> slint::Image {
    use crate::scene3d::{self, Label3};
    let v = &st.views[1];
    let w = ((v.w * st.scale as f64) as u32).max(16);
    let h = ((v.h * st.scale as f64) as u32).max(16);
    let Some((p, alt)) = replay_interp(f, st.replay) else {
        return slint::Image::default();
    };
    // Teren pod samolotem: z AGL, a bez niego najniższy punkt na ziemi (albo całego śladu)
    let ground = match p.agl_ft {
        Some(agl) => alt - agl as f64,
        None => f.track.iter().filter(|t| t.on_ground == Some(true)).map(|t| t.alt_ft).min().or_else(|| f.track.iter().map(|t| t.alt_ft).min()).unwrap_or(0) as f64,
    };
    let mut runways = Vec::new();
    let mut points = Vec::new();
    let db = airports::db();
    for code in [f.from.as_deref(), f.to.as_deref()].into_iter().flatten() {
        if let Some(a) = db.as_ref().and_then(|d| d.get(code)) {
            points.push(Label3 { lat: a.lat, lon: a.lon, text: a.ident.clone(), kind: 0 });
            for r in a.runways.iter().filter(|r| r.has_coords() && !r.closed) {
                runways.push((r.le_lat, r.le_lon, r.he_lat, r.he_lon, r.width_ft.max(60) as f64));
            }
        }
    }
    let track: Vec<(f64, f64, i32)> = f.track.iter().map(|t| (t.lat, t.lon, t.alt_ft)).collect();
    let u = units(&st.s);
    let el = (p.t - f.track[0].t).max(0) / 1000;
    let hud = vec![
        format!("{} · ×{}", f.aircraft.clone().filter(|a| !a.is_empty()).unwrap_or_else(|| "OpenTakeoff".into()), st.replay_speed as i64),
        format!("{}:{:02}:{:02} · ALT {} · GS {}", el / 3600, el % 3600 / 60, el % 60, u.alt_full(alt), u.spd(p.gs_kt as f64)),
        format!("HDG {:03}° · V/S {:+} ft/min", p.hdg.unwrap_or(0).rem_euclid(360), p.vs_fpm.unwrap_or(0)),
    ];
    let sc = scene3d::Scene {
        style: style(st, dark),
        dark,
        w,
        h,
        scale: st.scale,
        lat: p.lat,
        lon: p.lon,
        alt_ft: alt,
        hdg: p.hdg.unwrap_or(0) as f64,
        pitch: p.pitch_deg.unwrap_or(0.0) as f64,
        bank: p.bank_deg.unwrap_or(0.0) as f64,
        ground_ft: ground,
        mag_var: 0.0,
        show_aircraft: true,
        track: &track,
        runways: &runways,
        points: &points,
        route: &[],
        plan: &[],
        hud,
        compass: false,
        curtain: true,
    };
    scene3d::render(&sc, &st.replay_cam)
}

fn replay_point(f: &FlightLog, pos: f64) -> Option<opentakeoff_core::logbook::TrackPoint> {
    replay_interp(f, pos).map(|(p, _)| p)
}

/// Punkt śladu w chwili `pos` (0–1) interpolowany między próbkami – płynny ruch przy odtwarzaniu.
/// Zwraca też wysokość bez zaokrąglenia do pełnych stóp.
fn replay_interp(f: &FlightLog, pos: f64) -> Option<(opentakeoff_core::logbook::TrackPoint, f64)> {
    let t = &f.track;
    if t.len() < 2 {
        return None;
    }
    let (t0, t1) = (t[0].t, t[t.len() - 1].t);
    let target = t0 + ((t1 - t0) as f64 * pos.clamp(0.0, 1.0)) as i64;
    let i = t.partition_point(|p| p.t < target);
    if i == 0 {
        return Some((t[0].clone(), t[0].alt_ft as f64));
    }
    if i >= t.len() {
        let l = t[t.len() - 1].clone();
        let a = l.alt_ft as f64;
        return Some((l, a));
    }
    let (a, b) = (&t[i - 1], &t[i]);
    let k = ((target - a.t) as f64 / (b.t - a.t).max(1) as f64).clamp(0.0, 1.0);
    let lerp = |x: f64, y: f64| x + (y - x) * k;
    let lerp_o = |x: Option<f64>, y: Option<f64>| match (x, y) {
        (Some(x), Some(y)) => Some(lerp(x, y)),
        (x, y) => y.or(x),
    };
    let mut p = b.clone();
    p.t = target;
    p.lat = lerp(a.lat, b.lat);
    p.lon = lerp(a.lon, b.lon);
    let alt = lerp(a.alt_ft as f64, b.alt_ft as f64);
    p.alt_ft = alt.round() as i32;
    p.gs_kt = lerp(a.gs_kt as f64, b.gs_kt as f64).round() as i32;
    p.ias_kt = lerp_o(a.ias_kt.map(f64::from), b.ias_kt.map(f64::from)).map(|v| v.round() as i32);
    p.vs_fpm = lerp_o(a.vs_fpm.map(f64::from), b.vs_fpm.map(f64::from)).map(|v| v.round() as i32);
    p.agl_ft = lerp_o(a.agl_ft.map(f64::from), b.agl_ft.map(f64::from)).map(|v| v.round() as i32);
    p.bank_deg = lerp_o(a.bank_deg.map(f64::from), b.bank_deg.map(f64::from)).map(|v| v as f32);
    p.pitch_deg = lerp_o(a.pitch_deg.map(f64::from), b.pitch_deg.map(f64::from)).map(|v| v as f32);
    // kurs po krótszym łuku (359° → 1°)
    p.hdg = match (a.hdg, b.hdg) {
        (Some(x), Some(y)) => {
            let d = ((y - x) as f64 + 540.0).rem_euclid(360.0) - 180.0;
            Some(((x as f64 + d * k).round() as i32).rem_euclid(360))
        }
        (x, y) => y.or(x),
    };
    Some((p, alt))
}

pub fn update_cursor() {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let st = st.borrow();
    let Some(f) = st.detail.clone() else { return };
    let u = units(&st.s);
    let mut d = app.get_detail();
    d.cursor_pos = st.replay as f32;
    if let Some(p) = replay_point(&f, st.replay) {
        let el = (p.t - f.track[0].t) / 1000;
        d.cursor_info = ss(format!(
            "{}:{:02}:{:02}  {}  {}  {} ft/min  {}",
            el / 3600,
            el % 3600 / 60,
            el % 60,
            u.alt_full(p.alt_ft as f64),
            u.spd(p.ias_kt.unwrap_or(p.gs_kt) as f64),
            p.vs_fpm.unwrap_or(0),
            fmt::clock(p.t)
        ));
    }
    app.set_detail(d);
}

// ---------- Statystyki ----------

/// Początek okresu statystyk (ms): 0 – całość, 1 – 12 miesięcy, 2 – 30 dni, 3 – bieżący rok.
fn period_start(period: i32) -> i64 {
    let now = unix_ms();
    match period {
        1 => now - 365 * 86_400_000,
        2 => now - 30 * 86_400_000,
        3 => Local.with_ymd_and_hms(Local::now().year(), 1, 1, 0, 0, 0).single().map(|d| d.timestamp_millis()).unwrap_or(0),
        _ => i64::MIN,
    }
}

/// Stopnie pilota wg nalotu (godziny, nazwa, belki na pagonach).
fn ranks() -> [(i64, &'static str, i32); 8] {
    [
        (0, tr("Uczeń pilot"), 1),
        (10, tr("Pilot turystyczny"), 1),
        (50, tr("Pilot zawodowy"), 2),
        (150, tr("Drugi pilot"), 2),
        (400, tr("Starszy drugi pilot"), 3),
        (800, tr("Kapitan"), 4),
        (1500, tr("Starszy kapitan"), 4),
        (3000, tr("Szef pilotów"), 4),
    ]
}

/// Czas w fazach lotu z zapisanego śladu (minuty): ziemia, wznoszenie, przelot, zniżanie.
fn phase_minutes(f: &FlightLog) -> [f64; 4] {
    let mut out = [0.0; 4];
    for w in f.track.windows(2) {
        let dt = (w[1].t - w[0].t) as f64 / 60_000.0;
        if !(0.0..=10.0).contains(&dt) {
            continue;
        }
        let p = &w[0];
        let vs = p.vs_fpm.map(|v| v as f64).unwrap_or_else(|| (w[1].alt_ft - w[0].alt_ft) as f64 / dt.max(0.01));
        let i = if p.on_ground.unwrap_or(p.gs_kt < 40 && p.agl_ft.unwrap_or(100) < 20) {
            0
        } else if vs > 300.0 {
            1
        } else if vs < -300.0 {
            3
        } else {
            2
        };
        out[i] += dt;
    }
    out
}

/// Seria dni z lotami: (bieżąca, najdłuższa).
fn streaks(flights: &[FlightLog]) -> (i64, i64) {
    let mut days: Vec<i64> = flights.iter().filter_map(|f| Local.timestamp_millis_opt(f.start).single()).map(|d| d.date_naive().num_days_from_ce() as i64).collect();
    days.sort_unstable();
    days.dedup();
    let (mut best, mut run) = (0i64, 0i64);
    for (i, d) in days.iter().enumerate() {
        run = if i > 0 && days[i - 1] + 1 == *d { run + 1 } else { 1 };
        best = best.max(run);
    }
    let today = Local::now().date_naive().num_days_from_ce() as i64;
    let current = match days.last() {
        Some(&l) if l == today || l + 1 == today => run,
        _ => 0,
    };
    (current, best)
}

fn extra_stats(flights: &[FlightLog], s: &stats::Stats, u: &Units, period: i32) -> StatsV {
    let mut v = StatsV::default();
    // Stopień pilota liczony z całego nalotu w okresie
    let hours = s.minutes / 60;
    let r = ranks();
    let idx = r.iter().rposition(|x| hours >= x.0).unwrap_or(0);
    v.rank = ss(r[idx].1);
    v.rank_stripes = r[idx].2;
    if let Some(next) = r.get(idx + 1) {
        let span = (next.0 - r[idx].0).max(1) as f64 * 60.0;
        v.rank_progress = (((s.minutes - r[idx].0 * 60) as f64) / span).clamp(0.0, 1.0) as f32;
        v.rank_next = ss(next.1);
        v.rank_left = ss(trf!("jeszcze {h} do stopnia: {r}", h = fmt::hours(next.0 * 60 - s.minutes), r = next.1));
    } else {
        v.rank_progress = 1.0;
        v.rank_left = ss(tr("Najwyższy stopień"));
    }
    v.period = ss(match period {
        1 => tr("ostatnie 12 miesięcy"),
        2 => tr("ostatnie 30 dni"),
        3 => tr("bieżący rok"),
        _ => tr("cały dziennik"),
    });
    if s.flights > 0 {
        v.avg_flight = ss(fmt::duration_min(s.minutes / s.flights as i64));
        v.avg_dist = ss(u.dist(s.distance_nm as f64 / s.flights as f64));
    }
    let laps = s.distance_nm as f64 / 21_600.0;
    v.earth_laps = ss(if laps >= 0.01 { trf!("{x}× dookoła Ziemi", x = format!("{laps:.2}").replace('.', ",")) } else { String::new() });
    let (cur, best) = streaks(flights);
    v.streak = ss(if cur > 0 { trf!("{n} dni z rzędu", n = cur) } else { String::new() });
    v.streak_best = ss(if best > 0 { trf!("rekord: {n} dni", n = best) } else { String::new() });
    // Fazy lotu
    let mut ph = [0.0f64; 4];
    for f in flights {
        let m = phase_minutes(f);
        for i in 0..4 {
            ph[i] += m[i];
        }
    }
    let total: f64 = ph.iter().sum::<f64>().max(1.0);
    let t = |m: f64| ss(fmt::duration_min(m.round() as i64));
    v.t_ground = t(ph[0]);
    v.t_climb = t(ph[1]);
    v.t_cruise = t(ph[2]);
    v.t_descent = t(ph[3]);
    v.f_ground = (ph[0] / total) as f32;
    v.f_climb = (ph[1] / total) as f32;
    v.f_cruise = (ph[2] / total) as f32;
    v.f_descent = (ph[3] / total) as f32;
    // Lądowania
    let lands: Vec<LandingReportLite> = flights.iter().flat_map(|f| f.all_landings()).map(|l| LandingReportLite { score: l.score, vs: l.vs_fpm.abs(), g: l.g_force, bounces: l.bounces, cross: l.cross_ft, xwind: l.crosswind_kt }).collect();
    if let Some(avg) = s.avg_score {
        v.score_value = avg.round() as i32;
        v.grade = ss(landing::grade_of(avg.round() as i32));
    } else {
        v.score_value = -1;
    }
    if !lands.is_empty() {
        let n = lands.len() as f64;
        v.butter = ss(lands.iter().filter(|l| l.vs < 60).count().to_string());
        let gs: Vec<f64> = lands.iter().filter_map(|l| l.g).collect();
        v.avg_g = ss(if gs.is_empty() { String::new() } else { format!("{:.2} G", gs.iter().sum::<f64>() / gs.len() as f64) });
        v.bounces = ss(format!("{:.0} %", lands.iter().filter(|l| l.bounces > 0).count() as f64 / n * 100.0));
        let cr: Vec<f64> = lands.iter().filter_map(|l| l.cross.map(|c| c.abs() as f64)).collect();
        v.avg_cross = ss(if cr.is_empty() { String::new() } else { format!("{:.0} ft", cr.iter().sum::<f64>() / cr.len() as f64) });
        v.max_xwind = ss(lands.iter().filter_map(|l| l.xwind).map(|x| x.abs()).max().map(|x| format!("{x} kt")).unwrap_or_default());
    }
    v
}

struct LandingReportLite {
    score: i32,
    vs: i32,
    g: Option<f64>,
    bounces: i32,
    cross: Option<i32>,
    xwind: Option<i32>,
}

fn bars(vals: &[(String, f64, String)], accent_last: bool) -> ModelRc<BarV> {
    let max = vals.iter().map(|x| x.1).fold(0.0, f64::max).max(1e-9);
    let n = vals.len();
    model(vals.iter().enumerate().map(|(i, (l, v, t))| BarV { label: ss(l), value: (v / max) as f32, text: ss(t), accent: accent_last && i + 1 == n }).collect())
}

/// Wykresy karty Statystyki: kalendarz aktywności, godziny, dni tygodnia, poziomy przelotu,
/// opadanie przy przyziemieniu, oceny i ostatnie lądowania.
fn set_stats_charts(app: &App, flights: &[FlightLog], all: &[FlightLog]) {
    // Kalendarz ostatnich 26 tygodni (zawsze z całego dziennika), kolumny = tygodnie od poniedziałku
    let today = Local::now().date_naive();
    let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
    let first = monday - chrono::Duration::weeks(25);
    let mut mins = vec![0i64; 26 * 7];
    let mut count = vec![0usize; 26 * 7];
    for f in all {
        if let Some(d) = Local.timestamp_millis_opt(f.start).single() {
            let idx = (d.date_naive() - first).num_days();
            if (0..26 * 7).contains(&idx) {
                mins[idx as usize] += f.duration_min().max(0);
                count[idx as usize] += 1;
            }
        }
    }
    let hmax = mins.iter().copied().max().unwrap_or(0).max(1) as f32;
    let heat: Vec<BarV> = (0..26 * 7)
        .map(|i| {
            let day = first + chrono::Duration::days(i as i64);
            let future = day > today;
            BarV {
                label: ss(format!("{} {}", day.day(), fmt::month_short(day.month()))),
                value: if future { -1.0 } else if mins[i] == 0 && count[i] == 0 { 0.0 } else { (mins[i] as f32 / hmax).max(0.12) },
                text: ss(if count[i] == 0 { tr("brak lotów").to_string() } else { format!("{} · {}", fmt::flights(count[i]), fmt::duration_min(mins[i])) }),
                accent: day == today,
            }
        })
        .collect();
    app.set_stats_heat(model(heat));
    let mut months = Vec::new();
    let mut last_m = 0;
    for w in 0..26 {
        let d = first + chrono::Duration::weeks(w);
        months.push(if d.month() != last_m { fmt::month_short(d.month()).to_string() } else { String::new() });
        last_m = d.month();
    }
    app.set_stats_heat_months(model_strings(months));
    // Godzina startu i dzień tygodnia
    let mut hours = [0usize; 24];
    let mut wd = [0usize; 7];
    for f in flights {
        if let Some(d) = Local.timestamp_millis_opt(f.start).single() {
            hours[d.hour() as usize] += 1;
            wd[d.weekday().num_days_from_monday() as usize] += 1;
        }
    }
    let cnt = |n: usize| if n == 0 { String::new() } else { n.to_string() };
    app.set_stats_hours(bars(&hours.iter().enumerate().map(|(h, &n)| (if h % 3 == 0 { format!("{h:02}") } else { String::new() }, n as f64, cnt(n))).collect::<Vec<_>>(), false));
    let names = [tr("Pn"), tr("Wt"), tr("Śr"), tr("Cz"), tr("Pt"), tr("Sb"), tr("Nd")];
    app.set_stats_weekdays(bars(&wd.iter().enumerate().map(|(i, &n)| (names[i].to_string(), n as f64, cnt(n))).collect::<Vec<_>>(), false));
    // Najwyższy poziom lotu
    let lv_edges = [(0, "<5k"), (5_000, "5–10k"), (10_000, "FL100"), (20_000, "FL200"), (30_000, "FL300"), (35_000, "FL350"), (40_000, "FL400+")];
    let mut lv = [0usize; 7];
    for f in flights {
        let i = lv_edges.iter().rposition(|e| f.max_alt_ft >= e.0).unwrap_or(0);
        lv[i] += 1;
    }
    app.set_stats_levels(bars(&lv_edges.iter().zip(lv).map(|(e, n)| (e.1.to_string(), n as f64, cnt(n))).collect::<Vec<_>>(), false));
    // Prędkość opadania przy przyziemieniu
    let mut lands: Vec<(i64, i32, i32, String)> = flights.iter().flat_map(|f| f.all_landings().into_iter().map(move |l| (l.at.unwrap_or(f.end), l.score, l.vs_fpm.abs(), l.airport.clone().or(f.to.clone()).unwrap_or_default()))).collect();
    let vs_edges = [(0, "<60"), (60, "60"), (120, "120"), (180, "180"), (240, "240"), (360, "360"), (600, "600+")];
    let mut vb = [0usize; 7];
    for l in &lands {
        vb[vs_edges.iter().rposition(|e| l.2 >= e.0).unwrap_or(0)] += 1;
    }
    app.set_stats_vs(bars(&vs_edges.iter().zip(vb).map(|(e, n)| (e.1.to_string(), n as f64, cnt(n))).collect::<Vec<_>>(), false));
    // Oceny wg stopni
    let grades = [(90, 101), (75, 90), (55, 75), (35, 55), (i32::MIN, 35)];
    let total = lands.len().max(1) as f32;
    let gmax = grades.iter().map(|g| lands.iter().filter(|l| l.1 >= g.0 && l.1 < g.1).count()).max().unwrap_or(1).max(1) as f32;
    app.set_stats_grades(model(
        grades
            .iter()
            .map(|g| {
                let n = lands.iter().filter(|l| l.1 >= g.0 && l.1 < g.1).count();
                RowV { name: ss(landing::grade_of(if g.0 == i32::MIN { 0 } else { g.0 })), a: ss(n.to_string()), b: ss(format!("{:.0} %", n as f32 / total * 100.0)), level: n as f32 / gmax, id: ss("") }
            })
            .collect(),
    ));
    // Ostatnie 30 lądowań
    lands.sort_by_key(|l| l.0);
    let recent: Vec<BarV> = lands.iter().rev().take(30).rev().map(|l| BarV { label: ss(&l.3), value: (l.1.max(0) as f32 / 100.0).max(0.04), text: ss(format!("{} · {} ft/min", l.1, l.2)), accent: false }).collect();
    app.set_stats_recent(model(recent));
}

pub fn refresh_stats() {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let mut st = st.borrow_mut();
    let u = units(&st.s);
    let store = st.host.logbook.clone();
    st.stats_rev = store.revision();
    let all_flights: Vec<FlightLog> = store.flights().iter().map(|f| store.effective(f)).collect();
    let period = app.get_stats_period();
    let from = period_start(period);
    let flights: Vec<FlightLog> = all_flights.iter().filter(|f| f.start >= from).cloned().collect();
    let s = stats::compute(&flights);
    let rec = |r: &Option<stats::Record>, f: &dyn Fn(f64) -> String| (r.as_ref().map(|r| f(r.value)).unwrap_or_default(), r.as_ref().map(|r| r.flight_id.clone()).unwrap_or_default());
    let (longest, longest_id) = rec(&s.longest, &|v| fmt::duration_min(v as i64));
    let (farthest, farthest_id) = rec(&s.farthest, &|v| u.dist(v));
    let (highest, highest_id) = rec(&s.highest, &|v| u.alt_full(v));
    let (softest, softest_id) = rec(&s.softest, &|v| format!("{v:.0} ft/min"));
    let (best, best_id) = rec(&s.best_landing, &|v| format!("{v:.0}"));
    app.set_stats(StatsV {
        flights: ss(s.flights.to_string()),
        hours: ss(fmt::hours(s.minutes)),
        distance: ss(u.dist(s.distance_nm as f64)),
        landings: ss(s.landings.to_string()),
        avg_score: ss(s.avg_score.map(|v| format!("{v:.0}")).unwrap_or_default()),
        avg_vs: ss(s.avg_vs_fpm.map(|v| trf!("średnio {v} ft/min", v = format!("{v:.0}"))).unwrap_or_default()),
        airports: ss(s.airports.to_string()),
        aircraft: ss(s.aircraft_types.to_string()),
        since: ss(s.first_flight.map(|t| trf!("od {d}", d = fmt::date(t))).unwrap_or_default()),
        longest: ss(longest),
        farthest: ss(farthest),
        highest: ss(highest),
        softest: ss(softest),
        best: ss(best),
        longest_id: ss(longest_id),
        farthest_id: ss(farthest_id),
        highest_id: ss(highest_id),
        softest_id: ss(softest_id),
        best_id: ss(best_id),
        ..extra_stats(&flights, &s, &u, period)
    });
    set_stats_charts(&app, &flights, &all_flights);
    // Osiągnięcia i wyzwania tygodnia
    let ach = achievements::compute(&all_flights, unix_ms());
    let icon = |i: achievements::Icon| i as i32;
    app.set_ach_badges(model(
        ach.badges
            .iter()
            .map(|b| BadgeV { id: ss(b.id), title: ss(&b.title), desc: ss(&b.desc), icon: icon(b.icon), level: b.level as i32, levels: b.levels as i32, progress: b.progress, value: ss(&b.value) })
            .collect(),
    ));
    app.set_ach_week(model(
        ach.week
            .iter()
            .map(|c| ChallengeV {
                title: ss(&c.title),
                desc: ss(&c.desc),
                icon: icon(c.icon),
                airport: ss(c.airport.clone().unwrap_or_default()),
                done: c.done,
                progress: c.progress,
                status: ss(&c.status),
                flight_id: ss(c.flight_id.clone().unwrap_or_default()),
            })
            .collect(),
    ));
    app.set_ach_week_title(ss(&ach.week_title));
    app.set_ach_week_left(ss(&ach.week_left));
    app.set_ach_count(ss(trf!("{a} z {b} poziomów", a = ach.unlocked, b = ach.total_levels)));
    // Powiadomienie o nowym poziomie albo zaliczonym wyzwaniu (pierwsze liczenie – bez powiadomień)
    let keys = ach.keys();
    if !all_flights.is_empty() || st.s.ach_init {
        let fresh: Vec<String> = if st.s.ach_init { keys.iter().filter(|k| !st.s.ach_seen.contains(k)).cloned().collect() } else { Vec::new() };
        if !st.s.ach_init || !fresh.is_empty() {
            st.s.ach_init = true;
            st.s.ach_seen = keys.clone();
            st.s.save();
        }
        if let Some(k) = fresh.last() {
            let text = match k.split_once(':') {
                Some((id, lvl)) => {
                    let title = ach.badges.iter().find(|b| b.id == id).map(|b| b.title.clone()).unwrap_or_default();
                    trf!("Nowe osiągnięcie: {t} – poziom {l}", t = title, l = lvl)
                }
                None => {
                    let title = ach.week.iter().find(|c| &c.key == k).map(|c| c.title.clone()).unwrap_or_default();
                    trf!("Wyzwanie tygodnia zaliczone: {t}", t = title)
                }
            };
            let more = fresh.len().saturating_sub(1);
            let text = if more > 0 { format!("{text} (+{more})") } else { text };
            later(move || toast(text));
        }
    }
    // Ostatnie 12 miesięcy (także puste)
    let metric = app.get_stats_metric();
    let now = Local::now();
    let mut months = Vec::new();
    for i in (0..12).rev() {
        let mut y = now.year();
        let mut m = now.month() as i32 - i;
        while m <= 0 {
            m += 12;
            y -= 1;
        }
        let key = format!("{y:04}-{m:02}");
        let found = s.months.iter().find(|x| x.key == key);
        let v = match metric {
            1 => found.map(|x| x.minutes as f64 / 60.0).unwrap_or(0.0),
            2 => found.map(|x| x.distance_nm as f64).unwrap_or(0.0),
            _ => found.map(|x| x.flights as f64).unwrap_or(0.0),
        };
        months.push((fmt::month_short(m as u32).to_string(), v, i == 0));
    }
    let max = months.iter().map(|m| m.1).fold(0.0, f64::max).max(1.0);
    app.set_stats_months(model(
        months
            .into_iter()
            .map(|(l, v, cur)| BarV {
                label: ss(l),
                value: (v / max) as f32,
                text: ss(if v == 0.0 { String::new() } else if metric == 1 { format!("{v:.1}") } else { format!("{v:.0}") }),
                accent: cur,
            })
            .collect(),
    ));
    let smax = s.score_buckets.iter().copied().max().unwrap_or(1).max(1) as f32;
    app.set_stats_scores(model(
        s.score_buckets
            .iter()
            .enumerate()
            .map(|(i, &n)| BarV { label: ss(format!("{}", i * 10)), value: n as f32 / smax, text: ss(if n == 0 { String::new() } else { n.to_string() }), accent: false })
            .collect(),
    ));
    let rows = |v: &[stats::Counted]| -> ModelRc<RowV> {
        let max = v.iter().map(|c| c.flights).max().unwrap_or(1).max(1) as f32;
        model(v.iter().map(|c| RowV { name: ss(&c.name), a: ss(fmt::flights(c.flights)), b: ss(fmt::hours(c.minutes)), level: c.flights as f32 / max, id: ss("") }).collect())
    };
    app.set_stats_aircraft(rows(&s.top_aircraft));
    app.set_stats_airports(rows(&s.top_airports));
    app.set_stats_routes(rows(&s.top_routes));
    // Mapa wszystkich lotów
    let all: Vec<(f64, f64)> = flights.iter().flat_map(|f| [f.track.first(), f.track.last()]).flatten().map(|p| (p.lat, p.lon)).collect();
    if !st.views[3].user_moved {
        if let Some(b) = map::bounds(all.into_iter()) {
            st.views[3].fit(b);
        }
    }
    drop(st);
    render_stats_map();
}

pub fn render_stats_map() {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let st = st.borrow();
    let dark = ui.global::<Theme>().get_dark();
    let store = st.host.logbook.clone();
    let accent = ACCENTS[st.s.accent.min(ACCENTS.len() - 1)];
    let mut ov = Overlay::default();
    let from = period_start(app.get_stats_period());
    for f in store.flights().iter().filter(|f| f.start >= from).take(500) {
        let step = (f.track.len() / 300).max(1);
        ov.tracks.push(Track {
            pts: f.track.iter().step_by(step).map(|p| (p.lat, p.lon, p.alt_ft)).collect(),
            color: Some([(accent >> 16) as u8, (accent >> 8) as u8, accent as u8, 200]),
            width: 1.8,
        });
    }
    let r = map::render(&st.views[3], style(&st, dark), &ov, st.scale, dark);
    app.set_map_stats(r.image);
}

// ---------- Lotniska ----------

fn apt_v(a: &Airport, extra: String) -> AptV {
    let place = [a.city.as_str(), a.country.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(", ");
    AptV { ident: ss(&a.ident), name: ss(&a.name), place: ss(place), kind: ss(a.kind_name()), extra: ss(extra), cat: ss("") }
}

pub fn search_airports(q: &str) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let Some(db) = airports::db() else {
        app.set_airport_results(model(Vec::new()));
        return;
    };
    let res: Vec<AptV> = db.search(q, 60).into_iter().map(|a| apt_v(a, String::new())).collect();
    app.set_airport_results(model(res));
}

pub fn open_airport(ident: &str, fetch: bool) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let Some(db) = airports::db() else {
        toast(tr("Baza lotnisk jeszcze się wczytuje"));
        return;
    };
    let Some(a) = db.get(ident).cloned() else {
        toast(trf!("Nie znaleziono lotniska {ident}"));
        return;
    };
    let st = state();
    let mut st = st.borrow_mut();
    let u = units(&st.s);
    st.airport = Some(a.ident.clone());
    st.s.last_airport = a.ident.clone();
    if st.procs.as_ref().is_some_and(|(i, _)| i != &a.ident) {
        st.procs = None;
        st.legs.clear();
        app.set_procedures(model(Vec::new()));
        app.set_procedure_legs(model(Vec::new()));
        app.set_procedure_title(ss(""));
        app.set_procedures_status(ss(""));
    }
    app.set_has_airport(true);
    app.set_airport(apt_v(&a, String::new()));
    app.set_airport_watched(st.s.watchlist.iter().any(|w| w == &a.ident));
    let mut info = vec![
        KV { k: ss(tr("Kod ICAO / IATA")), v: ss(format!("{}{}", if a.icao.is_empty() { &a.ident } else { &a.icao }, if a.iata.is_empty() { String::new() } else { format!(" / {}", a.iata) })) },
        KV { k: ss(tr("Elewacja")), v: ss(a.elev_ft.map(|e| u.alt_full(e as f64)).unwrap_or_default()) },
        KV { k: ss(tr("Położenie")), v: ss(fmt::latlon(a.lat, a.lon)) },
        KV { k: ss(tr("Region")), v: ss(&a.region) },
        KV { k: ss(tr("Ruch rozkładowy")), v: ss(if a.scheduled { tr("tak") } else { tr("nie") }) },
        KV { k: ss(tr("Najdłuższy pas")), v: ss(format!("{} ft", a.longest_runway_ft())) },
    ];
    if let Some(snap) = st.host.snapshot() {
        let d = math::distance_nm(snap.position.lat, snap.position.lon, a.lat, a.lon);
        let b = math::bearing_deg(snap.position.lat, snap.position.lon, a.lat, a.lon);
        info.push(KV { k: ss(tr("Od samolotu")), v: ss(format!("{} · {:03.0}°", u.dist(d), b)) });
    }
    let visits = st.host.logbook.flights().iter().filter(|f| f.from.as_deref() == Some(&a.ident) || f.to.as_deref() == Some(&a.ident)).count();
    info.push(KV { k: ss(tr("Twoje loty")), v: ss(fmt::flights(visits)) });
    app.set_airport_info(model(info));
    let mut freqs: Vec<FreqV> = a.freqs.iter().map(|f| FreqV { kind: ss(&f.kind), name: ss(&f.desc), mhz: ss(format!("{:.3}", f.mhz)) }).collect();
    freqs.sort_by(|x, y| x.kind.cmp(&y.kind));
    app.set_airport_freqs(model(freqs));
    let nearby: Vec<AptV> = db.within(a.lat, a.lon, 40.0, false).into_iter().filter(|(_, x)| x.ident != a.ident).take(25).map(|(d, x)| apt_v(x, format!("{:.0} NM", d))).collect();
    app.set_airport_nearby(model(nearby));
    // Mapa lotniska
    let span = 0.08;
    st.views[2].fit((a.lat - span, a.lon - span * 1.6, a.lat + span, a.lon + span * 1.6));
    let cached = st.weather.get(&a.ident).cloned();
    drop(st);
    set_runways(&a, cached.as_ref().and_then(|c| c.0.first()));
    render_airport_map();
    match cached {
        Some(c) if !fetch => show_weather(&a.ident, &c.0, &c.1, &c.2),
        _ => fetch_weather(&a.ident),
    }
}

fn set_runways(a: &Airport, m: Option<&Metar>) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let wind = m.and_then(|m| Some((m.wind_dir? as f64, m.wind_kt? as f64, m.gust_kt)));
    let mut rows: Vec<RwyV> = Vec::new();
    let mut best: Option<(usize, f64)> = None;
    for r in a.runways.iter().filter(|r| !r.closed) {
        for (ident, hdg) in [(r.le.clone(), r.le_heading()), (r.he.clone(), r.he_heading())] {
            if ident.is_empty() {
                continue;
            }
            let (head, cross, wtext) = match (wind, hdg) {
                (Some((d, s, g)), Some(h)) => {
                    let (hw, cw) = landing::wind_components(d, s, h);
                    let gust = g.map(|g| format!(" ({} G{})", tr("porywy"), g)).unwrap_or_default();
                    (
                        hw,
                        cw,
                        format!("{} {:.0} kt · {} {:.0} kt {}{gust}", if hw >= 0.0 { tr("czołowy") } else { tr("tylny") }, hw.abs(), tr("boczny"), cw.abs(), if cw >= 0.0 { tr("z prawej") } else { tr("z lewej") }),
                    )
                }
                _ => (0.0, 0.0, String::new()),
            };
            if wind.is_some() && best.is_none_or(|b| head > b.1) {
                best = Some((rows.len(), head));
            }
            rows.push(RwyV {
                ident: ss(&ident),
                length: ss(format!("{} × {} ft", r.length_ft, r.width_ft)),
                surface: ss(if r.surface.is_empty() { "—".to_string() } else { r.surface.clone() }),
                heading: ss(hdg.map(|h| format!("{:03.0}° {}", h, tr("rzecz."))).unwrap_or_default()),
                wind: ss(wtext),
                head: head as f32,
                cross: cross as f32,
                best: false,
                ils: ss(a.freqs.iter().find(|f| f.desc.to_uppercase().contains(&format!("ILS {ident}")) || f.desc.to_uppercase().contains(&format!("RWY {ident}"))).map(|f| format!("{} {:.2}", f.desc, f.mhz)).unwrap_or_default()),
            });
        }
    }
    if let Some((i, _)) = best {
        rows[i].best = true;
    }
    app.set_airport_runways(model(rows));
}

fn fetch_weather(icao: &str) {
    let st = state();
    let client = st.borrow().host.weather.clone();
    if let Some(ui) = ui() {
        ui.global::<App>().set_weather_status(ss(tr("Pobieranie pogody…")));
    }
    let icao = icao.to_string();
    std::thread::spawn(move || {
        let metars = client.metars(&icao, 4);
        let taf = client.taf(&icao);
        later(move || {
            let (m, t, status) = match (metars, taf) {
                (Ok(m), Ok(t)) => {
                    let s = if m.is_empty() { tr("Lotnisko nie nadaje METAR-u").to_string() } else { trf!("Pobrano {t}", t = fmt::clock(unix_ms())) };
                    (m, t, s)
                }
                (Err(e), _) | (_, Err(e)) => (Vec::new(), None, trf!("Pogoda niedostępna: {e}")),
            };
            state().borrow_mut().weather.insert(icao.clone(), (m.clone(), t.clone(), status.clone()));
            show_weather(&icao, &m, &t, &status);
        });
    });
}

fn show_weather(icao: &str, metars: &[Metar], taf: &Option<Taf>, status: &str) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    if state().borrow().airport.as_deref() != Some(icao) {
        return;
    }
    let groups = |raw: &str| -> ModelRc<GroupV> { model(metar_dec::decode(raw).into_iter().map(|g| GroupV { token: ss(g.token), meaning: ss(g.meaning) }).collect()) };
    let m = metars.first();
    app.set_weather_status(ss(status));
    app.set_metar_raw(ss(m.map(|m| m.raw.clone()).unwrap_or_default()));
    app.set_metar_groups(groups(m.map(|m| m.raw.as_str()).unwrap_or("")));
    app.set_taf_raw(ss(taf.as_ref().map(|t| t.raw.clone()).unwrap_or_default()));
    app.set_taf_groups(groups(taf.as_ref().map(|t| t.raw.as_str()).unwrap_or("")));
    app.set_metar_history(model(metars.iter().skip(1).take(6).map(|m| ss(&m.raw)).collect()));
    let mut apt = app.get_airport();
    apt.cat = ss(m.map(flight_category).unwrap_or_default());
    app.set_airport(apt);
    let summary = match m {
        Some(m) => vec![
            KV {
                k: ss(tr("Wiatr")),
                v: ss(match (m.wind_variable, m.wind_dir, m.wind_kt) {
                    (true, _, Some(s)) => format!("VRB {s} kt"),
                    (_, Some(d), Some(s)) => format!("{d:03}° {s} kt{}", m.gust_kt.map(|g| format!(" G{g}")).unwrap_or_default()),
                    _ => "—".into(),
                }),
            },
            KV { k: ss(tr("Widzialność")), v: ss(m.visibility.clone().map(|v| format!("{v} SM")).unwrap_or_default()) },
            KV { k: ss(tr("Temperatura")), v: ss(m.temp_c.map(|t| format!("{t:.0} °C / {:.0}", m.dewpoint_c.unwrap_or(t))).unwrap_or_default()) },
            KV { k: "QNH".into(), v: ss(m.altim_hpa.map(|q| format!("{q:.0} hPa")).unwrap_or_default()) },
            KV { k: ss(tr("Kategoria")), v: ss(flight_category(m)) },
        ],
        None => Vec::new(),
    };
    app.set_metar_summary(model(summary));
    if let Some(a) = airports::db().and_then(|d| d.get(icao).cloned()) {
        set_runways(&a, m);
        crate::briefing::set(&a, metars, taf);
    }
}

pub fn render_airport_map() {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let st = st.borrow();
    let Some(ident) = st.airport.clone() else { return };
    let Some(db) = airports::db() else { return };
    let Some(a) = db.get(&ident) else { return };
    let dark = ui.global::<Theme>().get_dark();
    let mut ov = Overlay::default();
    for (_, x) in db.within(a.lat, a.lon, 25.0, false).into_iter().take(40) {
        ov.points.push(Point { lat: x.lat, lon: x.lon, label: x.ident.clone(), kind: if x.ident == a.ident { 2 } else if x.kind <= 1 { 0 } else { 3 } });
        for r in x.runways.iter().filter(|r| r.has_coords() && !r.closed) {
            ov.runways.push((r.le_lat, r.le_lon, r.he_lat, r.he_lon));
        }
    }
    if !st.legs.is_empty() {
        let pts: Vec<(f64, f64, i32)> = st.legs.iter().filter_map(|l| Some((l.lat?, l.lon?, l.alt1_ft.unwrap_or(0.0) as i32))).collect();
        for l in &st.legs {
            if let (Some(la), Some(lo)) = (l.lat, l.lon) {
                ov.points.push(Point { lat: la, lon: lo, label: l.fix.clone(), kind: 1 });
            }
        }
        ov.tracks.push(Track { pts, color: Some([255, 0, 220, 230]), width: 2.5 });
    }
    if let Some(s) = st.host.snapshot() {
        ov.aircraft = Some((s.position.lat, s.position.lon, s.attitude.hdg_true_deg));
    }
    let r = map::render(&st.views[2], style(&st, dark), &ov, st.scale, dark);
    app.set_map_airport(r.image);
    app.set_map_airport_labels(labels(r.labels));
}

pub fn load_procedures(icao: &str) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let host = st.borrow().host.clone();
    if st.borrow().procs.as_ref().is_some_and(|(i, _)| i == icao) {
        return;
    }
    let src = host.source();
    let Some(provider) = src.navdata().filter(|_| src.is_connected()) else {
        app.set_procedures_status(ss(tr("Procedury pochodzą z bazy MSFS – uruchom symulator (albo tryb symulacji: EPWA, EPGD).")));
        return;
    };
    app.set_procedures_status(ss(tr("Pobieranie procedur z MSFS…")));
    let icao = icao.to_string();
    let kind = src.kind();
    let nav = host.nav.clone();
    std::thread::spawn(move || {
        let res = nav.procedures(provider.as_ref(), kind, &icao);
        later(move || {
            let Some(ui) = self::ui() else { return };
            let app = ui.global::<App>();
            match res {
                Ok(p) => {
                    let mut rows = Vec::new();
                    let head = |t: &str| ProcV { name: ss(t), info: ss(""), kind: -1 };
                    if !p.departures.is_empty() {
                        rows.push(head(&tr("ODLOTY (SID)").to_string()));
                        rows.extend(p.departures.iter().map(|d| ProcV { name: ss(&d.name), info: ss(d.runway_transitions.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(" ")), kind: 0 }));
                    }
                    if !p.arrivals.is_empty() {
                        rows.push(head(&tr("PRZYLOTY (STAR)").to_string()));
                        rows.extend(p.arrivals.iter().map(|d| ProcV { name: ss(&d.name), info: ss(d.runway_transitions.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(" ")), kind: 1 }));
                    }
                    if !p.approaches.is_empty() {
                        rows.push(head(&tr("PODEJŚCIA").to_string()));
                        rows.extend(p.approaches.iter().map(|a| ProcV { name: ss(&a.name), info: ss(a.transitions.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(" ")), kind: 2 }));
                    }
                    app.set_procedures_status(ss(trf!("{n} procedur z bazy symulatora", n = p.departures.len() + p.arrivals.len() + p.approaches.len())));
                    app.set_procedures(model(rows));
                    state().borrow_mut().procs = Some((icao, p));
                }
                Err(e) => app.set_procedures_status(ss(e.message)),
            }
        });
    });
}

pub fn open_procedure(name: &str) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let Some((icao, p)) = st.borrow().procs.clone() else { return };
    let (kind, runway) = if p.departures.iter().any(|d| d.name == name) {
        ("departure", None)
    } else if p.arrivals.iter().any(|d| d.name == name) {
        ("arrival", None)
    } else {
        ("approach", None::<&str>)
    };
    let pick_rwy = |list: &[opentakeoff_core::navdata::TerminalProcedure]| list.iter().find(|d| d.name == name).and_then(|d| d.runway_transitions.first().map(|t| t.name.clone()));
    let rwy = match kind {
        "departure" => pick_rwy(&p.departures),
        "arrival" => pick_rwy(&p.arrivals),
        _ => runway.map(str::to_string),
    };
    let rwy = rwy.filter(|r| r != "ALL");
    match p.legs(kind, name, rwy.as_deref(), None) {
        Ok(info) => {
            let u = units(&st.borrow().s);
            let mut legs = info.legs.clone();
            legs.extend(info.missed.iter().cloned());
            let rows: Vec<LegV> = legs
                .iter()
                .map(|l| LegV {
                    fix: ss(if l.fix.is_empty() { "(—)".to_string() } else { l.fix.clone() }),
                    kind: ss(&l.leg_type),
                    alt: ss(match (l.alt_desc.as_deref(), l.alt1_ft, l.alt2_ft) {
                        (Some("AT"), Some(a), _) => u.alt_full(a),
                        (Some("ABOVE"), Some(a), _) => format!("≥ {}", u.alt_full(a)),
                        (Some("BELOW"), Some(a), _) => format!("≤ {}", u.alt_full(a)),
                        (Some("BETWEEN"), Some(a), Some(b)) => format!("{} – {}", u.alt(b), u.alt_full(a)),
                        _ => String::new(),
                    }),
                    speed: ss(l.speed_kt.map(|s| format!("≤ {s:.0} kt")).unwrap_or_default()),
                    course: ss(l.course_deg.map(|c| format!("{c:03.0}°")).unwrap_or_default()),
                    dist: ss(l.dist_nm.map(|d| format!("{d:.1} NM")).unwrap_or_default()),
                })
                .collect();
            app.set_procedure_legs(model(rows));
            app.set_procedure_title(ss(format!("{} · {}{}", icao, info.name, info.runway.map(|r| format!(" · RWY {r}")).unwrap_or_default())));
            st.borrow_mut().legs = info.legs;
            // Mapa dopasowana do procedury
            let b = map::bounds(st.borrow().legs.iter().filter_map(|l| Some((l.lat?, l.lon?))));
            if let Some(b) = b {
                st.borrow_mut().views[2].fit(b);
            }
            render_airport_map();
        }
        Err(e) => toast(e.message),
    }
}

pub fn refresh_watchlist(fetch: bool) {
    let st = state();
    let (list, client) = {
        let s = st.borrow();
        (s.s.watchlist.clone(), s.host.weather.clone())
    };
    if fetch {
        st.borrow_mut().watch_fetched = Some(Instant::now());
        let ids = list.clone();
        std::thread::spawn(move || {
            if let Ok(ms) = client.latest(&ids) {
                later(move || {
                    let st = state();
                    for m in ms {
                        st.borrow_mut().watch.insert(m.station.clone(), m);
                    }
                    refresh_watchlist(false);
                });
            }
        });
    }
    let Some(ui) = ui() else { return };
    let db = airports::db();
    let s = st.borrow();
    let rows: Vec<WxV> = list
        .iter()
        .map(|icao| {
            let m = s.watch.get(icao);
            WxV {
                icao: ss(icao),
                name: ss(db.as_ref().and_then(|d| d.get(icao).map(|a| a.name.clone())).unwrap_or_default()),
                cat: ss(m.map(flight_category).unwrap_or_default()),
                raw: ss(m.map(|m| m.raw.clone()).unwrap_or_else(|| tr("Pobieranie…").into())),
                wind: ss(m
                    .map(|m| match (m.wind_variable, m.wind_dir, m.wind_kt) {
                        (true, _, Some(k)) => format!("VRB{k:02}KT"),
                        (_, Some(d), Some(k)) => format!("{d:03}/{k}{}", m.gust_kt.map(|g| format!("G{g}")).unwrap_or_default()) + " kt",
                        _ => String::new(),
                    })
                    .unwrap_or_default()),
                vis: ss(m.and_then(|m| m.visibility.clone()).map(|v| format!("{v} SM")).unwrap_or_default()),
                temp: ss(m.and_then(|m| m.temp_c).map(|t| format!("{t:.0} °C")).unwrap_or_default()),
                qnh: ss(m.and_then(|m| m.altim_hpa).map(|q| format!("Q{q:.0}")).unwrap_or_default()),
                age: ss(m.map(|m| fmt::ago(m.obs_time * 1000)).unwrap_or_default()),
            }
        })
        .collect();
    ui.global::<App>().set_watchlist(model(rows));
}

// ---------- kalkulatory ----------

// ---------- narzędzia: obliczenia pomocnicze ----------

/// Atmosfera wzorcowa: (temperatura °C, ciśnienie hPa, gęstość względna, prędkość dźwięku kt).
fn isa(alt_ft: f64) -> (f64, f64, f64, f64) {
    let h = alt_ft.max(-2000.0);
    let t = isa_temp(h);
    let p = if h <= 36_089.0 { 1013.25 * (1.0 - 6.875_6e-6 * h).powf(5.2559) } else { 226.32 * (-(h - 36_089.0) / 20_806.0).exp() };
    let tk = t + 273.15;
    (t, p, (p / 1013.25) / (tk / 288.15), 661.47 * (tk / 288.15).sqrt())
}

fn isa_temp(alt_ft: f64) -> f64 {
    (15.0 - 1.98 * alt_ft / 1000.0).max(-56.5)
}

/// Ścieżki trójkąta wiatru w polu 100×100 (północ u góry): wektor kursu z TAS, wiatru i drogi.
fn triangle_paths(hdg: f64, tas: f64, wind_dir: f64, wind_kt: f64) -> (String, String, String) {
    let v = |deg: f64, len: f64| (len * deg.to_radians().sin(), len * deg.to_radians().cos());
    let a = v(hdg, tas);
    let w = v(wind_dir + 180.0, wind_kt);
    let b = (a.0 + w.0, a.1 + w.1);
    let pts = [(0.0, 0.0), a, b];
    let (minx, maxx) = pts.iter().fold((f64::MAX, f64::MIN), |m, p| (m.0.min(p.0), m.1.max(p.0)));
    let (miny, maxy) = pts.iter().fold((f64::MAX, f64::MIN), |m, p| (m.0.min(p.1), m.1.max(p.1)));
    let span = (maxx - minx).max(maxy - miny).max(1.0);
    let k = 76.0 / span;
    let (cx, cy) = ((minx + maxx) / 2.0, (miny + maxy) / 2.0);
    let map = |p: (f64, f64)| (50.0 + (p.0 - cx) * k, 50.0 - (p.1 - cy) * k);
    let (o, a, b) = (map((0.0, 0.0)), map(a), map(b));
    (arrow(o, a), arrow(a, b), arrow(o, b))
}

/// Odcinek ze strzałką na końcu (polecenia ścieżki SVG).
fn arrow(p0: (f64, f64), p1: (f64, f64)) -> String {
    let (dx, dy) = (p1.0 - p0.0, p1.1 - p0.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.5 {
        return String::new();
    }
    let (ux, uy) = (dx / len, dy / len);
    let h = 5.0f64.min(len * 0.4);
    let l = (p1.0 - h * ux + h * 0.5 * uy, p1.1 - h * uy - h * 0.5 * ux);
    let r = (p1.0 - h * ux - h * 0.5 * uy, p1.1 - h * uy + h * 0.5 * ux);
    format!("M {:.2} {:.2} L {:.2} {:.2} M {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2}", p0.0, p0.1, p1.0, p1.1, l.0, l.1, p1.0, p1.1, r.0, r.1)
}

/// Wiatr na wszystkich pasach lotniska: wiersze (pas, kurs, czołowy/tylny, boczny, najlepszy).
/// Pierwszy element: nazwa lotniska albo pusty, gdy nie znaleziono.
fn airport_winds(icao: &str, dir: Option<f64>, spd: Option<f64>) -> Vec<String> {
    let Some(db) = airports::db() else { return vec![tr("Baza lotnisk jeszcze się wczytuje").to_string()] };
    let Some(a) = db.get(&icao.trim().to_uppercase()) else { return vec![String::new()] };
    let (dir, spd) = (dir.unwrap_or(0.0), spd.unwrap_or(0.0));
    let mut ends: Vec<(String, f64, f64, f64, i32)> = Vec::new();
    for r in a.runways.iter().filter(|r| !r.closed) {
        for (id, hdg) in [(&r.le, r.le_heading()), (&r.he, r.he_heading())] {
            let hdg = hdg.or_else(|| id.trim_start_matches('0').chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<f64>().ok().map(|x| x * 10.0));
            if let Some(h) = hdg {
                let (hw, xw) = calc::wind_components(dir, spd, h);
                ends.push((id.clone(), h, hw, xw, r.length_ft));
            }
        }
    }
    let best = ends.iter().enumerate().max_by(|x, y| (x.1 .2 - x.1 .3.abs() * 0.5).partial_cmp(&(y.1 .2 - y.1 .3.abs() * 0.5)).unwrap_or(std::cmp::Ordering::Equal)).map(|x| x.0);
    let mut out = vec![format!("{} – {}", a.ident, a.name)];
    for (i, e) in ends.iter().enumerate() {
        out.push(e.0.clone());
        out.push(format!("{:03.0}°", e.1));
        out.push(format!("{} {:.0}", if e.2 >= 0.0 { "▲" } else { "▼" }, e.2.abs()));
        out.push(format!("{:.0} {}", e.3.abs(), if e.3 >= 0.0 { "→" } else { "←" }));
        out.push(if e.4 > 0 { format!("{} ft", e.4) } else { String::new() });
        out.push(if Some(i) == best { "1".into() } else if e.2 < -0.5 { "-1".into() } else { String::new() });
    }
    out
}

/// Trasa po ortodromie między lotniskami: [ok, nazwa A, nazwa B, dystans, kurs początkowy, kurs końcowy,
/// czas przy TAS, elewacja A, elewacja B, różnica stref ±, paliwo] albo [pusty, komunikat].
fn route_calc(a: &str, b: &str, tas: Option<f64>, burn: Option<f64>) -> Vec<String> {
    let Some(db) = airports::db() else { return vec![String::new(), tr("Baza lotnisk jeszcze się wczytuje").to_string()] };
    let (Some(x), Some(y)) = (db.get(&a.trim().to_uppercase()), db.get(&b.trim().to_uppercase())) else {
        return vec![String::new(), tr("Podaj dwa kody ICAO lotnisk, np. EPWA i EGLL").to_string()];
    };
    let d = math::distance_nm(x.lat, x.lon, y.lat, y.lon);
    let c1 = math::bearing_deg(x.lat, x.lon, y.lat, y.lon).rem_euclid(360.0);
    let c2 = (math::bearing_deg(y.lat, y.lon, x.lat, x.lon) + 180.0).rem_euclid(360.0);
    let min = tas.filter(|t| *t > 1.0).map(|t| d / t * 60.0);
    let elev = |e: Option<i32>| e.map(|e| format!("{e} ft")).unwrap_or_default();
    vec![
        "1".into(),
        format!("{} · {}", x.ident, x.name),
        format!("{} · {}", y.ident, y.name),
        format!("{d:.0} NM"),
        format!("{c1:03.0}°"),
        format!("{c2:03.0}°"),
        min.map(|m| fmt::duration_min(m.round() as i64)).unwrap_or_default(),
        elev(x.elev_ft),
        elev(y.elev_ft),
        format!("{:.0} km", d * 1.852),
        match (min, burn.filter(|b| *b > 0.0)) {
            (Some(m), Some(b)) => format!("{:.0} kg", m / 60.0 * b),
            _ => String::new(),
        },
        if c1 < 180.0 { tr("nieparzyste (na wschód)").to_string() } else { tr("parzyste (na zachód)").to_string() },
    ]
}

fn parse_hm(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    match s.split_once(':') {
        Some((h, m)) => Some(h.trim().parse::<f64>().ok()? * 60.0 + m.trim().parse::<f64>().ok()?),
        None => s.replace(',', ".").parse().ok(),
    }
}

/// Czas–prędkość–dystans: liczy brakującą wartość. [dystans, prędkość, czas, która policzona 0/1/2].
fn tsd(d: Option<f64>, s: Option<f64>, t: Option<f64>) -> Vec<String> {
    let hm = |m: f64| format!("{}:{:02}", (m / 60.0).floor() as i64, (m.rem_euclid(60.0)).round() as i64);
    match (d, s, t) {
        (Some(d), Some(s), _) if s > 0.0 => vec![format!("{d:.0} NM"), format!("{s:.0} kt"), hm(d / s * 60.0), "2".into()],
        (Some(d), None, Some(t)) if t > 0.0 => vec![format!("{d:.0} NM"), format!("{:.0} kt", d / (t / 60.0)), hm(t), "1".into()],
        (None, Some(s), Some(t)) => vec![format!("{:.1} NM", s * t / 60.0), format!("{s:.0} kt"), hm(t), "0".into()],
        _ => vec![String::new(), String::new(), String::new(), String::new()],
    }
}

/// Alfabet ICAO: pary (znak, słowo).
fn phonetic(text: &str) -> Vec<String> {
    const W: [&str; 26] = ["ALFA", "BRAVO", "CHARLIE", "DELTA", "ECHO", "FOXTROT", "GOLF", "HOTEL", "INDIA", "JULIETT", "KILO", "LIMA", "MIKE", "NOVEMBER", "OSCAR", "PAPA", "QUEBEC", "ROMEO", "SIERRA", "TANGO", "UNIFORM", "VICTOR", "WHISKEY", "X-RAY", "YANKEE", "ZULU"];
    const D: [&str; 10] = ["ZERO", "ONE", "TWO", "TREE", "FOWER", "FIFE", "SIX", "SEVEN", "AIT", "NINER"];
    let mut out = Vec::new();
    for c in text.to_uppercase().chars().take(40) {
        let w = match c {
            'A'..='Z' => W[(c as u8 - b'A') as usize],
            '0'..='9' => D[(c as u8 - b'0') as usize],
            '.' => "DECIMAL",
            ' ' => continue,
            _ => continue,
        };
        out.push(c.to_string());
        out.push(w.to_string());
    }
    out
}

/// Bieżące wartości z symulatora do narzędzi: [kierunek wiatru, prędkość wiatru, kurs magn., TAS, GS,
/// wysokość, OAT, QNH, droga, najbliższe lotnisko, elewacja najbliższego, punkt rosy (brak)].
fn live_numbers() -> Vec<String> {
    let Some(snap) = state().borrow().host.snapshot() else { return Vec::new() };
    let near = airports::db().and_then(|d| d.nearest(snap.position.lat, snap.position.lon).map(|a| (a.ident.clone(), a.elev_ft)));
    vec![
        format!("{:.0}", snap.environment.wind_dir_deg),
        format!("{:.0}", snap.environment.wind_spd_kt),
        format!("{:.0}", snap.attitude.hdg_mag_deg.rem_euclid(360.0)),
        format!("{:.0}", snap.speed.tas_kt),
        format!("{:.0}", snap.speed.gs_kt),
        format!("{:.0}", snap.position.alt_ind_ft),
        format!("{:.0}", snap.environment.oat_c),
        format!("{:.0}", snap.environment.qnh_hpa),
        format!("{:.0}", snap.attitude.track_true_deg.rem_euclid(360.0)),
        near.as_ref().map(|n| n.0.clone()).unwrap_or_default(),
        near.and_then(|n| n.1).map(|e| e.to_string()).unwrap_or_default(),
        format!("{:.0}", snap.fuel.total_kg),
        format!("{:.0}", snap.engines.iter().map(|e| e.fuel_flow_kg_h).sum::<f64>()),
    ]
}

pub fn calc(kind: &str, input: &[String]) -> Vec<String> {
    let n = |i: usize| input.get(i).and_then(|s| s.trim().replace(',', ".").parse::<f64>().ok());
    let all = |k: usize| (0..k).map(n).collect::<Option<Vec<f64>>>();
    match kind {
        "runway-wind" => match all(3) {
            Some(v) => {
                let (h, c) = calc::wind_components(v[0], v[1], v[2]);
                vec![format!("{} {:.0} kt", if h >= 0.0 { tr("czołowy") } else { tr("tylny") }, h.abs()), format!("{:.0} kt {}", c.abs(), if c >= 0.0 { tr("z prawej") } else { tr("z lewej") })]
            }
            None => vec![String::new(); 2],
        },
        "wind-triangle" => match all(4) {
            Some(v) => {
                let (wca, gs, hdg) = calc::wind_triangle(v[0], v[1], v[2], v[3]);
                let (ph, pw, pg) = triangle_paths(hdg, v[1], v[2], v[3]);
                let drift = (v[0] - hdg + 540.0).rem_euclid(360.0) - 180.0;
                vec![format!("{hdg:03.0}°"), format!("{wca:+.0}°"), format!("{gs:.0} kt"), ph, pw, pg, format!("{:.0} kt", gs - v[1]), format!("{drift:+.0}°")]
            }
            None => vec![String::new(); 8],
        },
        "density" => match all(4) {
            Some(v) => {
                let pa = calc::pressure_altitude(v[0], v[1]);
                let da = calc::density_altitude(v[0], v[1], v[2]);
                let tas = calc::tas_from_ias(v[3], da);
                let isa = isa_temp(pa);
                vec![format!("{pa:.0} ft"), format!("{da:.0} ft"), format!("{tas:.0} kt"), format!("{isa:.0} °C"), format!("ISA {:+.0} °C", v[2] - isa), format!("M {:.2}", calc::mach(tas, v[2]))]
            }
            None => vec![String::new(); 6],
        },
        "descent" => match all(4) {
            Some(v) => {
                let (nm, vs) = calc::descent(v[0], v[1], v[2], v[3]);
                let min = nm / v[2].max(1.0) * 60.0;
                let mut out = vec![format!("{nm:.0} NM"), format!("{vs:.0} ft/min"), format!("{min:.0} min"), format!("{:.0}", (v[0] - v[1]).max(0.0) / 1000.0 * 3.0)];
                // Tabela: odległość do końca zniżania → wysokość na ścieżce
                for k in (0..=8).rev() {
                    let d = nm * k as f64 / 8.0;
                    out.push(format!("{d:.0} NM"));
                    out.push(format!("{:.0} ft", v[1] + d * 6076.12 * v[3].to_radians().tan()));
                }
                out
            }
            None => vec![String::new(); 22],
        },
        "fuel" => match all(4) {
            Some(v) => {
                let (min, trip, total) = calc::fuel(v[0], v[1], v[2], v[3]);
                let mut out = vec![fmt::duration_min(min.round() as i64), format!("{trip:.0} kg"), format!("{total:.0} kg")];
                // Paliwo na pokładzie (opcjonalne): zasięg w czasie i zapas na celu
                match n(4).filter(|f| *f > 0.0) {
                    Some(fob) => {
                        let endur = fob / v[2].max(1.0) * 60.0;
                        out.push(fmt::duration_min(endur.round() as i64));
                        out.push(format!("{:.0} kg", fob - trip));
                        out.push(if fob >= total { "1".into() } else { String::new() });
                        out.push(format!("{:.0} NM", endur / 60.0 * v[1]));
                    }
                    None => out.extend([String::new(), String::new(), String::new(), String::new()]),
                }
                out
            }
            None => vec![String::new(); 7],
        },
        "cloudbase" => match all(2) {
            Some(v) => {
                let rh = calc::relative_humidity(v[0], v[1]);
                let spread = v[0] - v[1];
                let fz = if v[0] > 0.0 { format!("{:.0} ft AGL", v[0] / 1.98 * 1000.0) } else { tr("przy ziemi").to_string() };
                vec![format!("{:.0} ft AGL", calc::cloud_base_ft(v[0], v[1])), format!("{rh:.0} %"), (if spread <= 2.0 { tr("duże") } else if spread <= 4.0 { tr("umiarkowane") } else { tr("małe") }).to_string(), fz, format!("{spread:.0} °C"), format!("{:.0} °F", calc::celsius_to_f(v[0]))]
            }
            None => vec![String::new(); 6],
        },
        "unit-names" => {
            let g = n(0).unwrap_or(0.0) as usize;
            calc::UNITS.get(g).map(|u| u.1.iter().map(|x| x.0.to_string()).collect()).unwrap_or_default()
        }
        "convert" => match all(4) {
            Some(v) => {
                let r = calc::convert(v[0], v[1] as usize, v[2] as usize, v[3] as usize);
                let unit = calc::UNITS.get(v[1] as usize).and_then(|u| u.1.get(v[3] as usize)).map(|u| u.0).unwrap_or("");
                vec![format!("{} {unit}", if r.abs() >= 100.0 { format!("{r:.1}") } else { format!("{r:.4}").trim_end_matches('0').trim_end_matches('.').to_string() })]
            }
            None => vec![String::new()],
        },
        "airport-winds" => airport_winds(input.first().map(|s| s.as_str()).unwrap_or(""), n(1), n(2)),
        "route" => route_calc(input.first().map(|s| s.as_str()).unwrap_or(""), input.get(1).map(|s| s.as_str()).unwrap_or(""), n(2), n(3)),
        "turn" => match all(2) {
            Some(v) => {
                let ms = v[0] * 0.514_444;
                let g = 9.806_65;
                let rate1 = (ms * 3f64.to_radians() / g).atan().to_degrees();
                let bank = v[1].clamp(1.0, 75.0).to_radians();
                let r_m = ms * ms / (g * bank.tan());
                let rate = (g * bank.tan() / ms.max(1.0)).to_degrees();
                let n_load = 1.0 / bank.cos();
                vec![
                    format!("{rate1:.0}°"),
                    format!("{:.2} NM", r_m / 1852.0),
                    format!("{rate:.1}°/s"),
                    format!("{:.0} s", 360.0 / rate.max(0.01)),
                    format!("{n_load:.2} G"),
                    format!("+{:.0} %", (n_load.sqrt() - 1.0) * 100.0),
                    format!("{:.1} NM", r_m / 1852.0),
                    format!("{:.0} m", r_m),
                ]
            }
            None => vec![String::new(); 8],
        },
        "tsd" => tsd(n(0), input.get(1).and_then(|s| s.trim().replace(',', ".").parse::<f64>().ok()), input.get(2).and_then(|s| parse_hm(s))),
        "approach" => match all(3) {
            Some(v) => {
                let t = v[0].to_radians().tan();
                let mut out = vec![format!("{:.0} ft/min", v[1] * 6076.12 / 60.0 * t), format!("{:.0} ft/NM", 6076.12 * t), format!("{:.1} %", t * 100.0)];
                for gs in [90.0, 110.0, 130.0, 140.0, 150.0, 160.0, 180.0] {
                    out.push(format!("{gs:.0} kt"));
                    out.push(format!("{:.0}", gs * 6076.12 / 60.0 * t));
                }
                for d in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0] {
                    out.push(format!("{d:.0} NM"));
                    out.push(format!("{:.0} ft", v[2] + d * 6076.12 * t));
                }
                out
            }
            None => vec![String::new(); 35],
        },
        "climb" => match all(2) {
            Some(v) => {
                let ftnm = 6076.12 * v[1] / 100.0;
                let mut out = vec![format!("{:.0} ft/min", v[0] / 60.0 * ftnm), format!("{ftnm:.0} ft/NM"), format!("{:.1}°", (v[1] / 100.0).atan().to_degrees())];
                for gs in [100.0, 140.0, 180.0, 220.0, 260.0, 300.0] {
                    out.push(format!("{gs:.0} kt"));
                    out.push(format!("{:.0}", gs / 60.0 * ftnm));
                }
                out
            }
            None => vec![String::new(); 15],
        },
        "isa" => {
            let alt = n(0).unwrap_or(0.0);
            let (t, p, sigma, a) = isa(alt);
            let mut out = vec![format!("{t:.1} °C"), format!("{p:.1} hPa"), format!("{sigma:.3}"), format!("{a:.0} kt"), format!("{:.2} inHg", p / 33.8639)];
            for h in (0..=45_000).step_by(5_000) {
                let (t, p, sigma, a) = isa(h as f64);
                out.extend([if h == 0 { "MSL".to_string() } else if h >= 10_000 { format!("FL{:03}", h / 100) } else { format!("{h} ft") }, format!("{t:.1}"), format!("{p:.0}"), format!("{sigma:.3}"), format!("{a:.0}"), if (alt - h as f64).abs() < 2_500.0 { "1".into() } else { String::new() }]);
            }
            out
        }
        "units-all" => match all(3) {
            Some(v) => {
                let g = v[1] as usize;
                calc::UNITS
                    .get(g)
                    .map(|u| {
                        u.1.iter()
                            .enumerate()
                            .flat_map(|(i, x)| {
                                let r = calc::convert(v[0], g, v[2] as usize, i);
                                [x.0.to_string(), if r.abs() >= 100.0 { format!("{r:.1}") } else { format!("{r:.4}").trim_end_matches('0').trim_end_matches('.').to_string() }]
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }
            None => Vec::new(),
        },
        "phonetic" => phonetic(input.first().map(|s| s.as_str()).unwrap_or("")),
        "live" => live_numbers(),
        "decode" => metar_dec::decode(input.first().map(|s| s.as_str()).unwrap_or("")).into_iter().filter(|g| !g.meaning.is_empty()).flat_map(|g| [g.token, g.meaning]).collect(),
        _ => Vec::new(),
    }
}

// ---------- eksport i import ----------

pub fn export(kind: &str) {
    let st = state();
    let (store, ids, detail) = {
        let s = st.borrow();
        let mut ids: Vec<String> = s.selected.iter().cloned().collect();
        ids.sort_by_key(|id| s.list.iter().position(|x| x == id));
        (s.host.logbook.clone(), ids, s.detail.clone())
    };
    // Analiza lądowania: przebieg podejścia do arkusza
    if kind == "landing-csv" {
        let idx = ui().map(|u| u.global::<App>().get_analysis_index()).unwrap_or(-1);
        let Some(l) = detail.as_ref().and_then(|f| f.all_landings().get(idx.max(0) as usize).cloned()).filter(|l| !l.profile.is_empty()) else {
            toast(tr("Ten lot nie ma zapisanego przebiegu podejścia"));
            return;
        };
        let Some(f) = detail.as_ref() else { return };
        let name = format!("ladowanie-{}-{}.csv", fmt::date(l.at.unwrap_or(f.end)).replace('.', "-"), l.airport.as_deref().or(f.to.as_deref()).unwrap_or("X"));
        let Some(path) = rfd::FileDialog::new().set_file_name(&name).add_filter("CSV", &["csv"]).save_file() else { return };
        match std::fs::write(&path, export::landing_csv(&l)) {
            Ok(()) => toast(trf!("Zapisano: {p}", p = path.display())),
            Err(e) => toast(trf!("Nie zapisano pliku: {e}")),
        }
        return;
    }
    let all = kind == "oab" || kind == "list" || kind == "kml-all";
    let flights: Vec<FlightLog> = if !ids.is_empty() && kind != "kml-all" {
        ids.iter().filter_map(|i| store.get(i)).map(|f| store.effective(&f)).collect()
    } else if all {
        store.flights().iter().map(|f| store.effective(f)).collect()
    } else {
        detail.iter().map(|f| (**f).clone()).collect()
    };
    if flights.is_empty() {
        toast(tr("Wybierz lot do eksportu"));
        return;
    }
    let (ext, filter) = match kind {
        "gpx" => ("gpx", "GPX"),
        "kml" | "kml-all" => ("kml", "KML"),
        "csv" | "list" => ("csv", "CSV"),
        _ => ("oab", "OpenTakeoff"),
    };
    let single = flights.len() == 1 && !all;
    // GPX i KML: kilka lotów w jednym pliku; CSV z torem – osobny plik na lot
    let one_file = single || all || kind == "gpx" || kind == "kml";
    let name = if kind == "list" {
        "dziennik-lotow.csv".to_string()
    } else if kind == "kml-all" {
        format!("OpenTakeoff-mapa-lotow-{}.kml", chrono::Local::now().format("%Y-%m-%d"))
    } else if !single && (kind == "gpx" || kind == "kml") {
        format!("OpenTakeoff-{}-lotow.{ext}", flights.len())
    } else if kind == "oab" {
        format!("OpenTakeoff-{}.oab", chrono::Local::now().format("%Y-%m-%d"))
    } else if single {
        let f = &flights[0];
        format!("{}-{}-{}.{ext}", fmt::date(f.start).replace('.', "-"), f.from.as_deref().unwrap_or("X"), f.to.as_deref().unwrap_or("X"))
    } else {
        String::new()
    };
    if one_file {
        let Some(path) = rfd::FileDialog::new().set_file_name(&name).add_filter(filter, &[ext]).save_file() else { return };
        let bytes: Vec<u8> = match kind {
            "gpx" => export::gpx_many(&flights).into_bytes(),
            "kml" | "kml-all" => {
                let extra: Vec<String> = flights.iter().map(shots_kml).collect();
                export::kml_many(&flights, &extra).into_bytes()
            }
            "csv" => export::csv(&flights[0]).into_bytes(),
            "list" => export::logbook_csv(&flights).into_bytes(),
            _ => opentakeoff_core::logbook::archive::encode(&flights),
        };
        match std::fs::write(&path, bytes) {
            Ok(()) => toast(trf!("Zapisano: {p}", p = path.display())),
            Err(e) => toast(trf!("Nie zapisano pliku: {e}")),
        }
        return;
    }
    // Kilka lotów do GPX/KML/CSV – osobne pliki w wybranym folderze
    let Some(dir) = rfd::FileDialog::new().pick_folder() else { return };
    let mut n = 0;
    for f in &flights {
        let body = match kind {
            "gpx" => export::gpx(f),
            "kml" => export::kml(f),
            _ => export::csv(f),
        };
        let file = dir.join(format!("{}-{}-{}-{}.{ext}", fmt::date(f.start).replace('.', "-"), fmt::time(f.start).replace(':', ""), f.from.as_deref().unwrap_or("X"), f.to.as_deref().unwrap_or("X")));
        if std::fs::write(file, body).is_ok() {
            n += 1;
        }
    }
    toast(trf!("Zapisano {n} plików w {d}", d = dir.display()));
}

/// Zrzuty ekranu z lotu jako punkty KML (z podglądem obrazu z dysku w Google Earth).
fn shots_kml(f: &FlightLog) -> String {
    crate::shots::for_flight(f)
        .iter()
        .filter(|s| s.has_pos)
        .map(|s| {
            let img = crate::shots::file_path(s).display().to_string().replace('\\', "/");
            let html = format!("<img src=\"file:///{img}\" width=\"480\"><br>{} · {:.0} ft · HDG {:03.0}°", s.nearest, s.alt_ft, s.hdg);
            export::kml_point(&format!("{} {}", tr("Zrzut"), fmt::time(s.time)), s.time, s.lat, s.lon, s.alt_ft, &html, "shot")
        })
        .collect()
}

pub fn import() {
    let Some(paths) = rfd::FileDialog::new().add_filter("OpenTakeoff / GPX", &["oab", "json", "gpx"]).pick_files() else { return };
    let st = state();
    let store = st.borrow().host.logbook.clone();
    let mut total = opentakeoff_core::logbook::ImportResult::default();
    let mut errors = Vec::new();
    for p in paths {
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let is_gpx = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gpx"));
        let flights = if is_gpx {
            export::from_gpx(&String::from_utf8_lossy(&bytes)).map(|mut f| {
                if let Some(db) = airports::db() {
                    f.from = f.track.first().and_then(|p| db.nearest(p.lat, p.lon)).map(|a| a.ident.clone());
                    f.to = f.track.last().and_then(|p| db.nearest(p.lat, p.lon)).map(|a| a.ident.clone());
                }
                vec![f]
            })
        } else {
            opentakeoff_core::logbook::archive::decode(&bytes)
        };
        match flights {
            Ok(f) => {
                let r = store.import(f, "import");
                total.added += r.added;
                total.skipped += r.skipped;
            }
            Err(e) => errors.push(format!("{}: {e}", p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())),
        }
    }
    if !errors.is_empty() {
        log::warn(&errors.join("; "));
    }
    toast(if errors.is_empty() {
        trf!("Zaimportowano: {a}, pominięte (już są w dzienniku): {s}", a = fmt::flights(total.added), s = total.skipped)
    } else {
        trf!("Zaimportowano {a}; nie udało się odczytać {e} plików", a = fmt::flights(total.added), e = errors.len())
    });
    refresh_logbook();
}

/// Wszystkie widoczne mapy od nowa (zmiana motywu, podkładu, nowe kafelki).
pub fn rerender_maps() {
    let tab = ui().map(|u| u.global::<App>().get_tab()).unwrap_or(0);
    match tab {
        0 | 1 => refresh_live(true),
        2 => render_flight_map(),
        3 => render_stats_map(),
        4 => render_airport_map(),
        8 => crate::instruments::refresh(),
        9 => crate::navmap::render(true),
        _ => {}
    }
}

/// Zdarzenia mapy z interfejsu.
pub fn map_event(id: i32, kind: &str, x: f32, y: f32, dx: f32, dy: f32) {
    // Odtwarzanie 3D w dzienniku: przeciąganie obraca kamerę, kółko zmienia odległość
    if id == 1 && matches!(kind, "pan" | "zoom" | "fit") && state().borrow().replay_3d {
        {
            let st = state();
            let mut s = st.borrow_mut();
            match kind {
                "pan" => s.replay_cam.drag(dx as f64, dy as f64),
                "zoom" => s.replay_cam.zoom(dy as f64),
                _ => s.replay_cam = crate::scene3d::Camera::default(),
            }
        }
        render_flight_map();
        return;
    }
    if id == 4 {
        crate::navmap::event(kind, x, y, dx, dy);
        return;
    }
    if id == crate::simbrief::PROFILE {
        if kind == "size" {
            crate::simbrief::size(x, y);
        }
        return;
    }
    if id == crate::flightplan::MAP || id == crate::flightplan::PROFILE {
        crate::flightplan::map_event(id, kind, x, y, dx, dy);
        return;
    }
    if (30..40).contains(&id) {
        if kind == "size" {
            crate::briefing::size(id, x, y);
        }
        return;
    }
    if id >= 10 {
        if kind == "size" {
            crate::instruments::size(id, x, y);
        }
        return;
    }
    let st = state();
    {
        let mut s = st.borrow_mut();
        let Some(v) = s.views.get_mut(id as usize) else { return };
        match kind {
            "size" => {
                if (v.w - x as f64).abs() < 0.5 && (v.h - y as f64).abs() < 0.5 {
                    return;
                }
                v.w = (x as f64).max(16.0);
                v.h = (y as f64).max(16.0);
            }
            "pan" => {
                v.pan(dx as f64, dy as f64);
                if id == 0 {
                    v.follow = false;
                    if let Some(u) = ui() {
                        u.global::<App>().set_follow(false);
                    }
                }
            }
            "zoom" => v.zoom_at(dy as f64, x as f64, y as f64),
            "follow" => {
                v.follow = x > 0.5;
                v.user_moved = false;
            }
            "fit" => {
                v.user_moved = false;
                match id {
                    1 => {
                        if let Some(b) = s.detail.as_ref().and_then(|f| map::bounds(f.track.iter().map(|p| (p.lat, p.lon)))) {
                            s.views[1].fit(b);
                        }
                    }
                    0 => {
                        let track = s.host.live_track();
                        if let Some(b) = map::bounds(track.iter().map(|p| (p.lat, p.lon))) {
                            s.views[0].fit(b);
                            s.views[0].follow = false;
                        }
                    }
                    2 => {
                        if let Some(a) = s.airport.clone().and_then(|i| airports::db().and_then(|d| d.get(&i).cloned())) {
                            s.views[2].fit((a.lat - 0.08, a.lon - 0.13, a.lat + 0.08, a.lon + 0.13));
                        }
                    }
                    _ => {
                        s.views[3].user_moved = false;
                        drop(s);
                        refresh_stats();
                        return;
                    }
                }
            }
            _ => return,
        }
    }
    match id {
        0 => refresh_live(true),
        1 => render_flight_map(),
        2 => render_airport_map(),
        _ => render_stats_map(),
    }
}

pub fn model_strings(v: Vec<String>) -> ModelRc<SharedString> {
    model(v.into_iter().map(SharedString::from).collect())
}

pub fn strings_of(m: &ModelRc<SharedString>) -> Vec<String> {
    m.iter().map(|s| s.to_string()).collect()
}
