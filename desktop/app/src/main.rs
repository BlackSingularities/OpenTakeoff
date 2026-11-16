// OpenTakeoff dla Windows: dziennik lotów z synchronizacją z telefonem, podgląd lotu, statystyki,
// briefing lotnisk, narzędzia i most MSFS ↔ telefon (dawny OpenTakeoff Link).
#![windows_subsystem = "windows"]

slint::include_modules!();

mod analysis;
mod app;
mod avionics;
mod chart;
mod feedback;
mod fmt;
mod host;
mod instruments;
mod map;
mod navmap;
mod overlays;
mod scene3d;
mod screen;
mod settings;
mod shots;
mod simbrief;
mod web;
mod wxmap;
mod win;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use opentakeoff_core::lang::tr;
use opentakeoff_core::{log, trf};
use slint::{ComponentHandle, Model};

use app::{state, toast};
use settings::{Settings, ACCENTS, INTERVALS_MS};

pub use opentakeoff_core::lang;

fn apply_theme(ui: &AppWindow, s: &Settings) {
    let t = ui.global::<Theme>();
    t.set_mode(s.theme);
    let c = ACCENTS[s.accent.min(ACCENTS.len() - 1)];
    t.set_accent(slint::Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8));
    t.set_scale(s.scale);
    t.set_corners(s.corners);
    t.set_sidebar(s.sidebar);
    t.set_sidebar_w(s.sidebar_w.clamp(230, 420) as f32);
    win::dark_title(ui, s.theme != 2 && s.theme != 5);
}

fn apply_settings(ui: &AppWindow, s: &Settings) {
    let app = ui.global::<App>();
    app.set_sim_kind(s.sim_kind);
    app.set_update_auto(s.update_auto);
    app.set_xplane_host(s.xplane_host.clone().into());
    app.set_theme_mode(s.theme);
    app.set_record_flights(s.record_flights);
    app.set_record_interval(s.record_interval);
    app.set_autostart(s.autostart);
    app.set_start_minimized(s.start_minimized);
    app.set_close_to_tray(s.close_to_tray);
    app.set_units_alt(s.units_alt);
    app.set_units_speed(s.units_speed);
    app.set_map_style(s.map_style);
    app.set_auto_sync(s.auto_sync);
    app.set_look_sensitivity(s.look_sensitivity);
    let langs = lang::available();
    let current = if s.language.is_empty() { "" } else { s.language.as_str() };
    let mut opts = vec![OptV { id: "".into(), label: tr("Jak w Windows").into() }];
    opts.extend(langs.iter().map(|(c, n)| OptV { id: c.into(), label: n.into() }));
    app.set_language_index(opts.iter().position(|o| o.id == current).unwrap_or(0) as i32);
    app.set_languages(slint::ModelRc::new(slint::VecModel::from(opts)));
}

fn save() {
    let st = state();
    st.borrow().s.save();
}

fn confirm(text: String, label: &str, action: &str, ids: Vec<String>) {
    let Some(ui) = app::ui() else { return };
    state().borrow_mut().confirm = Some((action.to_string(), ids));
    let a = ui.global::<App>();
    a.set_confirm_label(label.into());
    a.set_confirm_text(text.into());
}

fn targets() -> Vec<String> {
    let st = state();
    let s = st.borrow();
    if !s.selected.is_empty() {
        s.selected.iter().cloned().collect()
    } else {
        s.detail.iter().map(|f| f.id.clone()).collect()
    }
}

fn cmd(ui: &AppWindow, id: &str, arg: &str) {
    let app = ui.global::<App>();
    let host = state().borrow().host.clone();
    match id {
        "tab" => {
            let t: i32 = arg.parse().unwrap_or(0);
            state().borrow_mut().s.tab = t;
            match t {
                0 | 1 => app::refresh_live(true),
                2 => {
                    app::refresh_logbook();
                    app::render_flight_map();
                }
                3 => app::refresh_stats(),
                4 => app::render_airport_map(),
                8 => instruments::refresh(),
                9 => navmap::render(true),
                10 => {
                    overlays::refresh_monitors();
                    overlays::push_settings();
                }
                11 => shots::refresh(),
                _ => {}
            }
            if t == 0 {
                simbrief::refresh();
            }
        }
        "simbrief-fetch" => simbrief::fetch(arg),
        "simbrief-clear" => simbrief::clear(),
        "plan-map" => {
            let pts: Vec<(f64, f64)> = simbrief::route().into_iter().map(|p| (p.0, p.1)).collect();
            if pts.len() > 1 {
                {
                    let st = state();
                    let mut b = st.borrow_mut();
                    b.s.nav_plan = true;
                    b.s.nav_3d = false;
                }
                navmap::fit_points(&pts);
                app.set_tab(9);
                navmap::render(true);
            }
        }
        "shot-delete" => {
            if let Some(id) = shots::selected_id() {
                confirm(tr("Usunąć ten zrzut ekranu? Plik zniknie z dysku.").into(), tr("Usuń"), "shot-delete", vec![id]);
            }
        }
        "pair-new-code" => {
            host.pairing.regenerate_code();
            app::refresh_status();
            toast(trf!("Nowy kod parowania: {c}", c = host.pairing.code()));
        }
        "pair-revoke-all" => confirm(tr("Unieważnić wszystkie sparowane telefony? Każdy będzie musiał połączyć się ponownie z nowym kodem.").into(), tr("Unieważnij"), "revoke-all", Vec::new()),
        "device-revoke" => confirm(tr("Unieważnić ten telefon? Przy następnym połączeniu będzie potrzebny kod parowania.").into(), tr("Unieważnij"), "revoke", vec![arg.into()]),
        "device-disconnect" => host.server.disconnect(arg),
        "copy-address" => {
            let text = format!("{}:{}", host.addresses().first().cloned().unwrap_or_default(), host.server.port());
            if let Ok(mut c) = arboard::Clipboard::new() {
                let _ = c.set_text(text.clone());
            }
            toast(trf!("Skopiowano adres {text}"));
        }
        "source" | "xplane-host" => {
            let (sim, kind, xp_host) = {
                let st = state();
                let mut b = st.borrow_mut();
                if id == "xplane-host" {
                    b.s.xplane_host = arg.trim().to_string();
                } else {
                    match arg {
                        "sim" => b.s.simulation = true,
                        other => {
                            b.s.simulation = false;
                            b.s.sim_kind = match other {
                                "msfs" => 1,
                                "xplane" => 2,
                                _ => 0,
                            };
                        }
                    }
                }
                (b.s.simulation, b.s.sim_kind, b.s.xplane_host.clone())
            };
            save();
            if sim {
                if !host.simulation.load(Ordering::SeqCst) {
                    host.switch_source(true);
                }
            } else {
                host.simulation.store(false, Ordering::SeqCst);
                host.set_sim(kind as i64, &xp_host);
            }
            app.set_sim_kind(kind);
            app.set_xplane_host(xp_host.into());
            app::refresh_status();
        }
        "simulation" => {
            let sim = arg == "1";
            if sim != host.simulation.load(Ordering::SeqCst) {
                host.switch_source(sim);
                state().borrow_mut().s.simulation = sim;
                save();
            }
            app::refresh_status();
        }
        "look-sensitivity" => {
            let v: i32 = arg.parse::<f32>().map(|f| f.round() as i32).unwrap_or(100).clamp(20, 400);
            host.set_look_sensitivity(v);
            state().borrow_mut().s.look_sensitivity = v;
            app.set_look_sensitivity(v);
            save();
        }
        "record-mark" => {
            if host.mark() {
                toast(tr("Dodano znacznik"));
            }
        }
        "record-stop" => host.stop_recording(),
        "record-resume" => {
            host.resume_recording();
            toast(tr("Zapis wznowiony – zacznie się, gdy samolot będzie w powietrzu"));
        }
        "live-clear" => {
            host.clear_live();
            app::refresh_live(true);
        }
        "watch-refresh" => app::refresh_watchlist(true),
        "airport-search" => app::search_airports(arg),
        "airport-search-open" => {
            if let Some(a) = opentakeoff_core::aero::airports::db().and_then(|d| d.search(arg, 1).first().map(|a| a.ident.clone())) {
                app::open_airport(&a, false);
            }
        }
        "airport-open" => {
            app.set_tab(4);
            app.set_airport_view(0);
            app::open_airport(arg, false);
        }
        "airport-refresh" => app::open_airport(arg, true),
        "airport-near-aircraft" => {
            if let (Some(s), Some(db)) = (host.snapshot(), opentakeoff_core::aero::airports::db()) {
                if let Some((_, a)) = db.within(s.position.lat, s.position.lon, 100.0, false).into_iter().find(|(_, a)| a.kind <= 2) {
                    let id = a.ident.clone();
                    app::open_airport(&id, false);
                }
            }
        }
        "airport-watch" => {
            {
                let st = state();
                let mut s = st.borrow_mut();
                let list = &mut s.s.watchlist;
                if let Some(i) = list.iter().position(|w| w == arg) {
                    list.remove(i);
                } else if !arg.is_empty() {
                    list.push(arg.to_string());
                }
                let watched = list.iter().any(|w| w == arg);
                app.set_airport_watched(watched);
            }
            save();
            app::refresh_watchlist(true);
        }
        "procedures-load" => app::load_procedures(arg),
        "procedure-open" => app::open_procedure(arg),
        "filter" => app::refresh_logbook(),
        "flight-open" => {
            if app.get_tab() != 2 {
                app.set_tab(2);
                app::refresh_logbook();
            }
            state().borrow_mut().selected.clear();
            app::open_flight(arg);
            app::refresh_logbook();
        }
        "flight-toggle" => {
            {
                let st = state();
                let mut s = st.borrow_mut();
                if !s.selected.remove(arg) {
                    s.selected.insert(arg.to_string());
                }
                s.anchor = Some(arg.to_string());
            }
            app::refresh_logbook();
        }
        "flight-range" => {
            {
                let st = state();
                let mut s = st.borrow_mut();
                let anchor = s.anchor.clone().unwrap_or_else(|| arg.to_string());
                let a = s.list.iter().position(|x| *x == anchor);
                let b = s.list.iter().position(|x| x == arg);
                if let (Some(a), Some(b)) = (a, b) {
                    let range: Vec<String> = s.list[a.min(b)..=a.max(b)].to_vec();
                    s.selected.extend(range);
                }
            }
            app::refresh_logbook();
        }
        "select-all" => {
            {
                let st = state();
                let mut s = st.borrow_mut();
                let all = s.list.clone();
                s.selected.extend(all);
            }
            app::refresh_logbook();
        }
        "select-none" => {
            state().borrow_mut().selected.clear();
            app::refresh_logbook();
        }
        "flight-favorite" | "flight-from" | "flight-to" | "flight-aircraft" | "flight-tags" | "flight-notes" => {
            let Some(fid) = state().borrow().detail.as_ref().map(|f| f.id.clone()) else { return };
            let v = arg.trim().to_string();
            host.logbook.update_meta(&fid, |m| match id {
                "flight-favorite" => m.favorite = !m.favorite,
                "flight-from" => m.from = (!v.is_empty()).then(|| v.to_uppercase()),
                "flight-to" => m.to = (!v.is_empty()).then(|| v.to_uppercase()),
                "flight-aircraft" => m.aircraft = (!v.is_empty()).then(|| v.clone()),
                "flight-tags" => m.tags = v.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect(),
                _ => m.notes = arg.to_string(),
            });
            if id != "flight-notes" {
                app::open_flight(&fid);
                app::refresh_logbook();
            }
        }
        "marker-delete" => {
            let Some(f) = state().borrow().detail.clone() else { return };
            let t: i64 = arg.parse::<f64>().unwrap_or(0.0) as i64;
            if let Some(raw) = host.logbook.get(&f.id) {
                let mut nf = (*raw).clone();
                nf.markers.retain(|m| m.t / 1000 != t);
                host.logbook.replace(nf);
                app::open_flight(&f.id);
            }
        }
        "delete" => {
            let ids = targets();
            if !ids.is_empty() {
                confirm(trf!("Przenieść {n} do kosza? Usunięte loty nie wrócą przy synchronizacji, a telefon usunie je u siebie przy następnym połączeniu.", n = fmt::flights(ids.len())), tr("Do kosza"), "delete", ids);
            }
        }
        "restore" => {
            let n = host.logbook.restore(&targets());
            state().borrow_mut().selected.clear();
            toast(trf!("Przywrócono {n}", n = fmt::flights(n)));
            app.set_has_detail(false);
            app::refresh_logbook();
        }
        "empty-trash" => {
            let n = host.logbook.trash().len();
            if n > 0 {
                confirm(trf!("Usunąć trwale {n} z kosza? Tego nie da się cofnąć.", n = fmt::flights(n)), tr("Usuń trwale"), "empty-trash", Vec::new());
            }
        }
        "confirm-no" => {
            state().borrow_mut().confirm = None;
            app.set_confirm_text("".into());
        }
        "confirm-yes" => {
            app.set_confirm_text("".into());
            let Some((action, ids)) = state().borrow_mut().confirm.take() else { return };
            match action.as_str() {
                "delete" => {
                    let n = host.logbook.delete(&ids);
                    state().borrow_mut().selected.clear();
                    app.set_has_detail(false);
                    state().borrow_mut().detail = None;
                    toast(trf!("Przeniesiono do kosza: {n}", n = fmt::flights(n)));
                    app::refresh_logbook();
                }
                "empty-trash" => {
                    let n = host.logbook.empty_trash();
                    app.set_has_detail(false);
                    toast(trf!("Usunięto trwale: {n}", n = fmt::flights(n)));
                    app::refresh_logbook();
                }
                "revoke-all" => {
                    host.pairing.revoke_all();
                    toast(tr("Wszystkie telefony unieważnione"));
                }
                "revoke" => {
                    for t in ids {
                        host.pairing.revoke(&t);
                    }
                    toast(tr("Telefon unieważniony"));
                }
                "shot-delete" => {
                    shots::delete(&ids);
                    toast(tr("Usunięto zrzut ekranu"));
                }
                _ => {}
            }
            app::refresh_status();
        }
        "import" => app::import(),
        "export" => app::export(arg),
        "landing-analysis" => app::select_landing(arg.parse().unwrap_or(-1)),
        "export-all" => app::export("oab"),
        "replay-toggle" => {
            let st = state();
            let playing = {
                let mut s = st.borrow_mut();
                s.playing = !s.playing && s.detail.is_some();
                if s.playing && s.replay >= 0.999 {
                    s.replay = 0.0;
                }
                s.playing
            };
            app.set_replay_playing(playing);
        }
        "replay-3d" => {
            state().borrow_mut().replay_3d = arg == "1";
            app.set_replay_3d(arg == "1");
            app::render_flight_map();
        }
        "replay-speed" => {
            let v = arg.parse::<f64>().unwrap_or(60.0).clamp(1.0, 3600.0);
            state().borrow_mut().replay_speed = v;
        }
        "replay-seek" => {
            state().borrow_mut().replay = arg.parse::<f64>().unwrap_or(0.0).clamp(0.0, 1.0);
            app::update_cursor();
            app::render_flight_map();
        }
        "stats-refresh" => app::refresh_stats(),
        "open-logs" => {
            let _ = open::that(opentakeoff_core::log::file().parent().unwrap());
        }
        "open-data" => {
            let _ = open::that(opentakeoff_core::paths::config());
        }
        "open-url" => {
            let _ = open::that(arg);
        }
        "db-refresh" => load_airports(true),
        "theme" | "accent" | "scale" | "corners" | "map-style" => {
            let v: i32 = arg.parse().unwrap_or(0);
            {
                let st = state();
                let mut s = st.borrow_mut();
                match id {
                    "theme" => s.s.theme = v.clamp(0, 5),
                    "accent" => s.s.accent = v.clamp(0, ACCENTS.len() as i32 - 1) as usize,
                    "scale" => s.s.scale = [0.9, 1.0, 1.1, 1.25][v.clamp(0, 3) as usize],
                    "corners" => s.s.corners = v.clamp(0, 2),
                    _ => s.s.map_style = v.clamp(0, 4),
                }
            }
            let s = state().borrow().s.clone();
            apply_theme(ui, &s);
            apply_settings(ui, &s);
            save();
            app::rerender_maps();
        }
        "language" => {
            state().borrow_mut().s.language = arg.to_string();
            save();
            apply_settings(ui, &state().borrow().s.clone());
            toast(tr("Język zmieni się po ponownym uruchomieniu programu"));
        }
        "units-alt" | "units-speed" | "record-interval" => {
            let v: i32 = arg.parse().unwrap_or(0);
            {
                let st = state();
                let mut s = st.borrow_mut();
                match id {
                    "units-alt" => s.s.units_alt = v.clamp(0, 1),
                    "units-speed" => s.s.units_speed = v.clamp(0, 1),
                    _ => {
                        s.s.record_interval = v.clamp(0, 4);
                        host.set_interval(INTERVALS_MS[s.s.record_interval as usize]);
                    }
                }
            }
            save();
            apply_settings(ui, &state().borrow().s.clone());
            app::refresh_logbook();
        }
        "record-flights" | "auto-sync" | "autostart" | "start-minimized" | "close-to-tray" => {
            let on = arg == "1";
            {
                let st = state();
                let mut s = st.borrow_mut();
                match id {
                    "record-flights" => {
                        s.s.record_flights = on;
                        host.record_enabled.store(on, Ordering::SeqCst);
                    }
                    "auto-sync" => {
                        s.s.auto_sync = on;
                        host.set_sync_enabled(on);
                    }
                    "autostart" => {
                        s.s.autostart = on;
                        if let Err(e) = win::set_autostart(on) {
                            toast(trf!("Nie udało się zmienić autostartu: {e}"));
                        }
                    }
                    "start-minimized" => {
                        s.s.start_minimized = on;
                        if s.s.autostart {
                            let _ = win::set_autostart(true);
                        }
                    }
                    _ => s.s.close_to_tray = on,
                }
            }
            save();
            apply_settings(ui, &state().borrow().s.clone());
        }
        "update-check" => check_update(true),
        "update-install" => {
            app.set_update_dialog(false);
            install_update();
        }
        "update-later" => app.set_update_dialog(false),
        "update-skip" => {
            app.set_update_dialog(false);
            let v = app.get_update_version().to_string();
            let st = state();
            let mut b = st.borrow_mut();
            b.s.update_skip = v;
            b.s.save();
        }
        "update-auto" => {
            let on = arg == "1";
            app.set_update_auto(on);
            let st = state();
            let mut b = st.borrow_mut();
            b.s.update_auto = on;
            b.s.save();
        }
        "feedback" => feedback::open_form(),
        _ => {
            if !instruments::cmd(id, arg) && !navmap::cmd(id, arg) && !overlays::cmd(id, arg) && !shots::cmd(id, arg) {
                log::warn(&format!("Nieznane polecenie: {id} {arg}"));
            }
        }
    }
}

fn load_airports(force: bool) {
    if let Some(ui) = app::ui() {
        ui.global::<App>().set_db_status(tr("Wczytywanie bazy lotnisk…").into());
    }
    state().borrow_mut().db_loaded = false;
    std::thread::spawn(move || {
        let progress = |m: String| {
            app::later(move || {
                if let Some(ui) = app::ui() {
                    ui.global::<App>().set_db_status(m.into());
                }
            })
        };
        let res = opentakeoff_core::aero::airports::load(force, &progress);
        if let Err(e) = opentakeoff_core::aero::navaids::load(force) {
            opentakeoff_core::log::warn(&format!("Pomoce nawigacyjne niedostępne: {e}"));
        }
        app::later(move || {
            match res {
                Ok(_) => {
                    let last = state().borrow().s.last_airport.clone();
                    if !last.is_empty() {
                        app::open_airport(&last, false);
                    }
                    app::refresh_watchlist(true);
                }
                Err(e) => {
                    if let Some(ui) = app::ui() {
                        ui.global::<App>().set_db_status(trf!("Baza lotnisk niedostępna: {e}").into());
                    }
                }
            }
            app::refresh_status();
        });
    });
}

fn check_update(manual: bool) {
    if let Some(ui) = app::ui() {
        if manual {
            ui.global::<App>().set_update_text(tr("Sprawdzanie aktualizacji…").into());
        }
    }
    std::thread::spawn(move || {
        let res = opentakeoff_core::update::check();
        app::later(move || {
            let Some(ui) = app::ui() else { return };
            let a = ui.global::<App>();
            match res {
                Ok(Some(r)) => {
                    a.set_update_text(trf!("Dostępna wersja {v} – zainstaluj z tego miejsca.", v = r.version).into());
                    a.set_update_ready(true);
                    let skipped = state().borrow().s.update_skip == r.version;
                    // Okno z opisem zmian: po ręcznym sprawdzeniu zawsze, samoczynnie – raz na wersję,
                    // chyba że użytkownik ją pominął
                    let already = state().borrow().update.as_ref().is_some_and(|u| u.version == r.version);
                    if manual || (!skipped && !already) {
                        a.set_update_version(r.version.clone().into());
                        let notes = r.notes.trim();
                        let notes: String = if notes.chars().count() > 2500 { notes.chars().take(2500).collect::<String>() + "…" } else { notes.to_string() };
                        a.set_update_notes(notes.into());
                        a.set_update_dialog(true);
                    }
                    state().borrow_mut().update = Some(r);
                }
                Ok(None) => a.set_update_text(trf!("Masz najnowszą wersję ({v}).", v = opentakeoff_core::version()).into()),
                Err(e) => {
                    if manual {
                        a.set_update_text(trf!("Nie udało się sprawdzić aktualizacji: {e}").into())
                    }
                }
            }
        });
    });
}

fn install_update() {
    let Some(rel) = state().borrow().update.clone() else { return };
    if !opentakeoff_core::update::installed() {
        let _ = open::that("https://redzikowski.com/portfolio/opentakeoff");
        return;
    }
    if let Some(ui) = app::ui() {
        let a = ui.global::<App>();
        a.set_busy(true);
        a.set_busy_text(tr("Pobieranie aktualizacji…").into());
    }
    std::thread::spawn(move || {
        let res = opentakeoff_core::update::download(&rel, |done, total| {
            let pct = if total > 0 { done * 100 / total } else { 0 };
            app::later(move || {
                if let Some(ui) = app::ui() {
                    ui.global::<App>().set_busy_text(trf!("Pobieranie aktualizacji… {pct} %").into());
                }
            });
        });
        app::later(move || {
            if let Some(ui) = app::ui() {
                ui.global::<App>().set_busy(false);
            }
            match res.and_then(|p| opentakeoff_core::update::apply(&p, true)) {
                Ok(()) => {
                    state().borrow().host.shutdown();
                    std::process::exit(0);
                }
                Err(e) => toast(e),
            }
        });
    });
}

fn main() {
    log::install_panic_hook();
    let args: Vec<String> = std::env::args().collect();
    let minimized_arg = args.iter().any(|a| a == "--minimized");
    let settings = Settings::load();
    let code = lang::init(&settings.language);
    let _ = slint::select_bundled_translation(&code);
    feedback::install("opentakeoff", "OpenTakeoff", opentakeoff_core::version(), opentakeoff_core::paths::cache(), || lang::current() == "pl", true);
    // Jedna kopia programu: druga tylko pokazuje okno pierwszej
    if !win::single_instance() {
        win::signal_show();
        return;
    }
    log::info(&format!("OpenTakeoff {} – start", opentakeoff_core::version()));
    if win::legacy_link_running() {
        log::warn("Działa stary OpenTakeoff Link – zamknij go, żeby telefony łączyły się z nowym programem");
    }

    // Diagnostyka: OPENTAKEOFF_SIMULATION=1 – lot testowy bez zmiany ustawień
    let sim = settings.simulation || std::env::var("OPENTAKEOFF_SIMULATION").is_ok_and(|v| v == "1");
    let host = host::Host::start(sim, settings.sim_kind as i64, &settings.xplane_host, settings.look_sensitivity, settings.record_flights, settings.interval_ms());
    host.set_sync_enabled(settings.auto_sync);
    let ui = AppWindow::new().expect("okno");
    let app = ui.global::<App>();
    app.set_version(opentakeoff_core::version().into());
    app.set_tab(settings.tab.clamp(0, 7));
    app.set_about(
        trf!(
            "OpenTakeoff {v} dla Windows – dziennik lotów, briefing i most do telefonu. Pomysł i wykonanie: Adam Rędzikowski · redzikowski.com. Dane: OurAirports (domena publiczna), aviationweather.gov (NOAA), mapy © OpenStreetMap, © CARTO.",
            v = opentakeoff_core::version()
        )
        .into(),
    );
    app.set_data_path(opentakeoff_core::paths::config().display().to_string().into());
    apply_theme(&ui, &settings);
    apply_settings(&ui, &settings);
    let start_hidden = minimized_arg || settings.start_minimized;
    app::install(
        app::State {
            host: host.clone(),
            s: settings,
            views: Default::default(),
            scale: 1.0,
            list: Vec::new(),
            selected: Default::default(),
            anchor: None,
            detail: None,
            replay: 0.0,
            playing: false,
            replay_3d: false,
            replay_speed: 60.0,
            replay_cam: scene3d::Camera::default(),
            logbook_rev: 0,
            airport: None,
            weather: Default::default(),
            watch: Default::default(),
            procs: None,
            legs: Vec::new(),
            confirm: None,
            log_serial: 0,
            update: None,
            tiles_serial: 0,
            live_render: Instant::now(),
            stats_rev: 0,
            db_loaded: false,
            toast_serial: 0,
            watch_fetched: None,
        },
        &ui,
    );

    // Polecenia i zdarzenia interfejsu
    {
        let w = ui.as_weak();
        app.on_cmd(move |id, arg| {
            if let Some(ui) = w.upgrade() {
                cmd(&ui, id.as_str(), arg.as_str());
            }
        });
    }
    app.on_map_event(|id, kind, x, y, dx, dy| app::map_event(id, kind.as_str(), x, y, dx, dy));
    app.on_calc(|kind, input| app::model_strings(app::calc(kind.as_str(), &app::strings_of(&input))));
    {
        let w = ui.as_weak();
        app.on_win(move |a| {
            if let Some(ui) = w.upgrade() {
                win::action(&ui, a);
            }
        });
    }
    ui.on_key(|_, _, _| false);
    // Nakładki, plan z SimBrief i zrzuty – po pokazaniu okna (lista ekranów pochodzi z okna)
    slint::Timer::single_shot(Duration::from_millis(700), || {
        overlays::refresh_monitors();
        overlays::push_settings();
        simbrief::load_saved();
        shots::refresh();
        overlays::apply();
    });
    {
        let w = ui.as_weak();
        ui.window().on_close_requested(move || {
            let Some(ui) = w.upgrade() else { return slint::CloseRequestResponse::HideWindow };
            if state().borrow().s.close_to_tray {
                let _ = ui.hide();
                if !state().borrow().s.autostart {
                    log::info("Okno schowane do zasobnika – most do telefonu działa dalej");
                }
                slint::CloseRequestResponse::HideWindow
            } else {
                quit();
                slint::CloseRequestResponse::HideWindow
            }
        });
    }

    // Zasobnik systemowy
    let tray = win::Tray::new();

    // Odświeżanie: stan co 250 ms, lot co 200 ms, odtwarzanie co 50 ms
    let timer = slint::Timer::default();
    {
        let w = ui.as_weak();
        let mut tick = 0u64;
        let mut last_frame = Instant::now();
        timer.start(slint::TimerMode::Repeated, Duration::from_millis(50), move || {
            let Some(ui) = w.upgrade() else { return };
            tick += 1;
            let dt = last_frame.elapsed().as_secs_f64();
            last_frame = Instant::now();
            // Skala ekranu (mapy rysowane w pikselach fizycznych)
            let sf = ui.window().scale_factor();
            if (state().borrow().scale - sf).abs() > 0.01 {
                state().borrow_mut().scale = sf;
                app::rerender_maps();
            }
            // Odtwarzanie lotu (×60)
            let playing = {
                let st = state();
                let mut s = st.borrow_mut();
                if s.playing {
                    let len = s.detail.as_ref().map(|f| (f.end - f.start).max(1) as f64 / 1000.0).unwrap_or(1.0);
                    s.replay = (s.replay + dt * s.replay_speed / len).min(1.0);
                    if s.replay >= 1.0 {
                        s.playing = false;
                    }
                    true
                } else {
                    false
                }
            };
            if playing {
                let still = state().borrow().playing;
                ui.global::<App>().set_replay_playing(still);
                app::update_cursor();
                if tick % 2 == 0 {
                    app::render_flight_map();
                }
            }
            if tick % 4 == 0 {
                app::refresh_live(false);
            }
            // Nakładki (karta lądowania, pasek do transmisji, okno), skrót zrzutu ekranu, postęp planu
            overlays::tick();
            shots::poll_key();
            if tick % 20 == 0 {
                simbrief::refresh();
            }
            // Przyrządy płynniej (co 100 ms), mapa nawigacyjna sama ogranicza częstość
            let tab_now = ui.global::<App>().get_tab();
            if tab_now == 8 && tick % 2 == 0 {
                instruments::refresh();
            }
            if tab_now == 9 && tick % 3 == 0 {
                navmap::render(false);
            }
            if tick % 5 == 0 {
                app::refresh_status();
                // Dziennik zmieniony w tle (nowy lot, synchronizacja)
                let st = state();
                let (rev, cur, stats_rev) = {
                    let s = st.borrow();
                    (s.host.logbook.revision(), s.logbook_rev, s.stats_rev)
                };
                let tab = ui.global::<App>().get_tab();
                // Panel boczny pokazuje ostatnie loty na każdej karcie
                let side = ui.global::<Theme>().get_sidebar();
                if rev != cur && (tab == 2 || tab == 0 || side) {
                    app::refresh_logbook();
                }
                // Panel boczny: widoczność i szerokość zapamiętane w ustawieniach
                if tick % 40 == 0 {
                    let w = ui.global::<Theme>().get_sidebar_w().round() as i32;
                    let changed = {
                        let b = st.borrow();
                        b.s.sidebar != side || b.s.sidebar_w != w
                    };
                    if changed {
                        {
                            let mut b = st.borrow_mut();
                            b.s.sidebar = side;
                            b.s.sidebar_w = w;
                        }
                        save();
                    }
                }
                if rev != stats_rev && (tab == 3 || tab == 0) {
                    app::refresh_stats();
                }
                // Nowe kafelki mapy
                let ts = map::TILES_SERIAL.load(Ordering::SeqCst);
                if ts != st.borrow().tiles_serial {
                    st.borrow_mut().tiles_serial = ts;
                    app::rerender_maps();
                }
                // Pogoda obserwowanych lotnisk co 10 min
                let stale = st.borrow().watch_fetched.is_none_or(|t| t.elapsed() > Duration::from_secs(600));
                if stale && opentakeoff_core::aero::airports::db().is_some() {
                    app::refresh_watchlist(true);
                }
            }
            // Druga kopia programu albo zasobnik: pokaż okno
            if win::show_requested() || tray.take_show() {
                let _ = ui.show();
                win::bring_to_front(&ui);
            }
            if tray.take_quit() {
                quit();
            }
        });
    }

    app::refresh_status();
    app::refresh_logbook();
    app::refresh_stats();
    app::refresh_watchlist(false);
    load_airports(false);
    {
        // Sprawdzenie aktualizacji po starcie (w tle, bez komunikatu przy braku)
        let t = slint::Timer::default();
        t.start(slint::TimerMode::SingleShot, Duration::from_secs(8), || {
            if state().borrow().s.update_auto {
                check_update(false)
            }
        });
        std::mem::forget(t);
        // Program działa całymi dniami w zasobniku – kolejne sprawdzenia co 6 godzin
        let t = slint::Timer::default();
        t.start(slint::TimerMode::Repeated, Duration::from_secs(6 * 3600), || {
            if state().borrow().s.update_auto {
                check_update(false)
            }
        });
        std::mem::forget(t);
    }
    if !start_hidden {
        ui.show().expect("okno");
        win::dark_title(&ui, { let t = state().borrow().s.theme; t != 2 && t != 5 });
    }
    feedback::offer_pending_crash();
    // Diagnostyka: OPENTAKEOFF_SNAPSHOT=<folder> – zrzut każdej karty do PNG i koniec (bez ekranu użytkownika)
    if let Ok(dir) = std::env::var("OPENTAKEOFF_SNAPSHOT") {
        snapshots(&ui, std::path::PathBuf::from(dir));
    }
    // Diagnostyka nakładek: OPENTAKEOFF_OVERLAY_TEST=<plik.png> – karta lądowania, pasek i okno na
    // ekranie głównym, prawdziwy zrzut ekranu (z przezroczystością) i koniec.
    if let Ok(file) = std::env::var("OPENTAKEOFF_OVERLAY_TEST") {
        slint::Timer::single_shot(Duration::from_millis(2500), || {
            cmd_by_name("ovl", "stream.enabled=1");
            cmd_by_name("ovl", "overlay.content=3");
            cmd_by_name("ovl", "overlay.enabled=1");
            cmd_by_name("ovl-landing-preview", "");
        });
        slint::Timer::single_shot(Duration::from_millis(6000), move || {
            if let Some(u) = app::ui() {
                if let Some(m) = screen::monitors(u.window()).first() {
                    if let Some(img) = screen::capture(m) {
                        let _ = img.save(&file);
                    }
                }
            }
            cmd_by_name("ovl", "stream.enabled=0");
            cmd_by_name("ovl", "overlay.enabled=0");
            quit();
        });
    }
    slint::run_event_loop_until_quit().expect("pętla zdarzeń");
    state().borrow().host.shutdown();
    log::info("Koniec pracy");
}

/// Polecenie interfejsu wywołane z kodu (diagnostyka).
fn cmd_by_name(id: &str, arg: &str) {
    if let Some(u) = app::ui() {
        cmd(&u, id, arg);
    }
}

fn quit() {
    overlays::close_all();
    save();
    state().borrow().host.shutdown();
    let _ = slint::quit_event_loop();
}

#[allow(dead_code)]
fn unused(m: &slint::ModelRc<FlightV>) -> usize {
    m.row_count()
}

fn snapshots(ui: &AppWindow, dir: std::path::PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    let w = ui.as_weak();
    let step = std::rc::Rc::new(std::cell::Cell::new(0i32));
    let t = slint::Timer::default();
    t.start(slint::TimerMode::Repeated, Duration::from_millis(4000), move || {
        let Some(ui) = w.upgrade() else { return };
        let i = step.get();
        if i > 0 {
            if let Ok(buf) = ui.window().take_snapshot() {
                let path = dir.join(format!("shot{}.png", i - 1));
                let _ = image::save_buffer(&path, buf.as_bytes(), buf.width(), buf.height(), image::ExtendedColorType::Rgba8);
            }
        }
        // Karty 0–11, potem 12: Mapa w widoku 3D (OPENTAKEOFF_SNAPSHOT_TABS="1,8,9,12" – wybrane)
        let order: Vec<i32> = std::env::var("OPENTAKEOFF_SNAPSHOT_TABS").ok().map(|t| t.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_else(|| (0..=12).collect());
        if i as usize >= order.len() {
            cmd(&ui, "nav-3d", "0");
            quit();
            return;
        }
        let tab = order[i as usize];
        let three_d = tab == 12;
        let tab = if three_d { 9 } else { tab };
        if tab == 9 {
            cmd(&ui, "nav-3d", if three_d { "1" } else { "0" });
        }
        ui.global::<App>().set_tab(tab);
        cmd(&ui, "tab", &tab.to_string());
        if tab == 2 {
            let first = state().borrow().list.first().cloned();
            if let Some(id) = first {
                cmd(&ui, "flight-open", &id);
            }
            // OPENTAKEOFF_SNAPSHOT_SCROLL=<px> – szczegóły lotu przewinięte (np. do analizy lądowania)
            if let Some(px) = std::env::var("OPENTAKEOFF_SNAPSHOT_SCROLL").ok().and_then(|v| v.parse::<f32>().ok()) {
                ui.global::<App>().set_detail_scroll(-px);
            }
        }
        if tab == 4 {
            cmd(&ui, "airport-open", "EPGD");
        }
        // OPENTAKEOFF_SNAPSHOT_CMDS="replay-3d:1;replay-seek:0.6" – polecenia po przejściu na kartę
        if let Ok(list) = std::env::var("OPENTAKEOFF_SNAPSHOT_CMDS") {
            for c in list.split(';').filter(|c| !c.trim().is_empty()) {
                let (id, arg) = c.split_once(':').unwrap_or((c, ""));
                cmd(&ui, id.trim(), arg.trim());
            }
        }
        step.set(i + 1);
    });
    std::mem::forget(t);
}
