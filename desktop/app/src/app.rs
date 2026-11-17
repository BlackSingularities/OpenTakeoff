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

// Karty w osobnych plikach (app/*.rs); funkcje publiczne dalej jako app::…
mod live_view;
mod logbook_view;
mod stats_view;
mod airports_view;
mod tools_calc;
pub use live_view::refresh_live;
pub use logbook_view::{refresh_logbook, open_flight, select_landing, render_flight_map, update_cursor};
pub use stats_view::{refresh_stats, render_stats_map};
pub use airports_view::{search_airports, open_airport, render_airport_map, load_procedures, open_procedure, refresh_watchlist};
pub use tools_calc::calc;
use live_view::*;
use logbook_view::*;
use stats_view::*;
use airports_view::*;
use tools_calc::*;

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
    /// Mnożnik prędkości odtwarzania lotu.
    pub replay_speed: f64,
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
            // podpowiedzi z nazwą lotniska przy kodach ICAO – przelicz po wczytaniu bazy
            app.set_abbr_rev(app.get_abbr_rev() + 1);
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
    if id == crate::simbrief::OFP_PROFILE {
        if kind == "size" {
            crate::simbrief::size_ofp(x, y);
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
            "tilt" => {
                if !v.tilt_by(dy as f64) {
                    return;
                }
            }
            "orbit" => {
                let turned = v.rotate_by(dx as f64);
                let tilted = v.tilt_by(dy as f64);
                if !turned && !tilted {
                    return;
                }
                if turned {
                    publish_bearing(id, v.bearing);
                }
            }
            "north" => {
                v.reset_bearing();
                publish_bearing(id, 0.0);
            }
            "tilt-toggle" => v.toggle_tilt(),
            "pan" => {
                v.drag(x as f64, y as f64, dx as f64, dy as f64);
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

/// Obrót mapy `id` do kompasu w interfejsie (`App.map-bearings[id]`, stopnie).
pub fn publish_bearing(id: i32, deg: f64) {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    let Ok(idx) = usize::try_from(id) else { return };
    let m = app.get_map_bearings();
    if let Some(vm) = m.as_any().downcast_ref::<VecModel<f32>>() {
        if idx < vm.row_count() {
            vm.set_row_data(idx, deg as f32);
            return;
        }
    }
    let mut v: Vec<f32> = m.iter().collect();
    if v.len() <= idx {
        v.resize(idx + 1, 0.0);
    }
    v[idx] = deg as f32;
    app.set_map_bearings(ModelRc::new(VecModel::from(v)));
}

pub fn model_strings(v: Vec<String>) -> ModelRc<SharedString> {
    model(v.into_iter().map(SharedString::from).collect())
}

pub fn strings_of(m: &ModelRc<SharedString>) -> Vec<String> {
    m.iter().map(|s| s.to_string()).collect()
}
