//! Logika okna: stan programu po stronie interfejsu, odświeżanie kart i obsługa poleceń (App.cmd).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use chrono::{Datelike, Local, TimeZone};
use opentakeoff_core::aero::airports::{self, Airport};
use opentakeoff_core::aero::weather::{flight_category, Metar, Taf};
use opentakeoff_core::aero::{calc, metar as metar_dec};
use opentakeoff_core::lang::tr;
use opentakeoff_core::logbook::{export, landing, stats, FlightLog};
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
        let (_, _, _, fuel0, last) = st.host.recording();
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
        let engines: Vec<EngineV> = s
            .engines
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let turbine = e.n1_pct > 0.5;
                EngineV {
                    label: ss(trf!("Silnik {n}", n = i + 1)),
                    main: ss(if turbine { format!("N1 {:.1} %", e.n1_pct) } else { format!("{:.0} RPM", e.rpm) }),
                    sub: ss(format!("EGT {:.0} °C · {:.0} kg/h · {} {:.0} %", e.egt_c, e.fuel_flow_kg_h, tr("dźwignia"), e.throttle_pct)),
                    level: if turbine { (e.n1_pct / 100.0) as f32 } else { (e.rpm / 2800.0).min(1.0) as f32 },
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
        })
        .collect();
    app.set_detail_landings(model(landings));
    let span = (f.end - f.start).max(1) as f32;
    let markers: Vec<MarkerV> =
        f.markers.iter().map(|m| MarkerV { t: (m.t / 1000) as i32, time: ss(fmt::clock(m.t)), label: ss(&m.label), pos: ((m.t - t.first().map(|p| p.t).unwrap_or(f.start)) as f32 / span).clamp(0.0, 1.0) }).collect();
    app.set_detail_markers(model(markers));
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
    if let Some(p) = replay_point(&f, st.replay) {
        ov.aircraft = Some((p.lat, p.lon, p.hdg.unwrap_or(0) as f64));
    }
    let r = map::render(&st.views[1], style(&st, dark), &ov, st.scale, dark);
    app.set_map_flight(r.image);
    app.set_map_flight_labels(labels(r.labels));
}

fn replay_point(f: &FlightLog, pos: f64) -> Option<opentakeoff_core::logbook::TrackPoint> {
    let t = &f.track;
    if t.len() < 2 {
        return None;
    }
    let (t0, t1) = (t[0].t, t[t.len() - 1].t);
    let target = t0 + ((t1 - t0) as f64 * pos.clamp(0.0, 1.0)) as i64;
    let i = t.partition_point(|p| p.t < target).min(t.len() - 1);
    Some(t[i].clone())
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

pub fn refresh_stats() {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let mut st = st.borrow_mut();
    let u = units(&st.s);
    let store = st.host.logbook.clone();
    st.stats_rev = store.revision();
    let flights: Vec<FlightLog> = store.flights().iter().map(|f| store.effective(f)).collect();
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
    });
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
    for f in store.flights().iter().take(500) {
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
                vec![format!("{hdg:03.0}°"), format!("{wca:+.0}°"), format!("{gs:.0} kt")]
            }
            None => vec![String::new(); 3],
        },
        "density" => match all(4) {
            Some(v) => {
                let pa = calc::pressure_altitude(v[0], v[1]);
                let da = calc::density_altitude(v[0], v[1], v[2]);
                vec![format!("{pa:.0} ft"), format!("{da:.0} ft"), format!("{:.0} kt", calc::tas_from_ias(v[3], da))]
            }
            None => vec![String::new(); 3],
        },
        "descent" => match all(4) {
            Some(v) => {
                let (nm, vs) = calc::descent(v[0], v[1], v[2], v[3]);
                let min = nm / v[2].max(1.0) * 60.0;
                vec![format!("{nm:.0} NM"), format!("{vs:.0} ft/min"), format!("{min:.0} min")]
            }
            None => vec![String::new(); 3],
        },
        "fuel" => match all(4) {
            Some(v) => {
                let (min, trip, total) = calc::fuel(v[0], v[1], v[2], v[3]);
                vec![fmt::duration_min(min.round() as i64), format!("{trip:.0} kg"), format!("{total:.0} kg")]
            }
            None => vec![String::new(); 3],
        },
        "cloudbase" => match all(2) {
            Some(v) => {
                let rh = calc::relative_humidity(v[0], v[1]);
                let spread = v[0] - v[1];
                vec![format!("{:.0} ft AGL", calc::cloud_base_ft(v[0], v[1])), format!("{rh:.0} %"), (if spread <= 2.0 { tr("duże") } else if spread <= 4.0 { tr("umiarkowane") } else { tr("małe") }).to_string()]
            }
            None => vec![String::new(); 3],
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
    let flights: Vec<FlightLog> = if !ids.is_empty() {
        ids.iter().filter_map(|i| store.get(i)).map(|f| store.effective(&f)).collect()
    } else if kind == "oab" || kind == "list" {
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
        "kml" => ("kml", "KML"),
        "csv" | "list" => ("csv", "CSV"),
        _ => ("oab", "OpenTakeoff"),
    };
    let single = flights.len() == 1 && kind != "oab" && kind != "list";
    let name = if kind == "list" {
        "dziennik-lotow.csv".to_string()
    } else if kind == "oab" {
        format!("OpenTakeoff-{}.oab", chrono::Local::now().format("%Y-%m-%d"))
    } else if single {
        let f = &flights[0];
        format!("{}-{}-{}.{ext}", fmt::date(f.start).replace('.', "-"), f.from.as_deref().unwrap_or("X"), f.to.as_deref().unwrap_or("X"))
    } else {
        String::new()
    };
    if single || kind == "oab" || kind == "list" {
        let Some(path) = rfd::FileDialog::new().set_file_name(&name).add_filter(filter, &[ext]).save_file() else { return };
        let bytes: Vec<u8> = match kind {
            "gpx" => export::gpx(&flights[0]).into_bytes(),
            "kml" => export::kml(&flights[0]).into_bytes(),
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
