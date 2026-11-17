// OpenTakeoff dla Windows: dziennik lotów z synchronizacją z telefonem, podgląd lotu, statystyki,
// briefing lotnisk, narzędzia i most MSFS ↔ telefon (dawny OpenTakeoff Link).
#![windows_subsystem = "windows"]

slint::include_modules!();

mod analysis;
mod app;
mod avionics;
mod briefing;
mod chart;
mod checklists;
mod commands;
mod feedback;
mod flightplan;
mod fmt;
mod host;
mod instruments;
mod map;
mod navmap;
mod overlays;
mod params;
mod perf;
mod planbrief;
mod popout;
mod scene3d;
mod screen;
mod settings;
mod shots;
mod simbrief;
mod simlaunch;
mod web;
mod wxmap;
mod win;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use opentakeoff_core::lang::tr;
use opentakeoff_core::{log, trf};
use slint::{ComponentHandle, Model};

use commands::cmd;
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
    app.set_sim_launch(s.sim_launch);
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
    // Proces czuwania na symulator (X-Plane, MSFS z nietypowej instalacji) – bez okna
    if args.iter().any(|a| a == "--sim-watch") {
        simlaunch::watch_main();
        return;
    }
    // Deinstalator: usuń wpisy z exe.xml symulatorów
    if args.iter().any(|a| a == "--sim-launch-off") {
        let _ = simlaunch::set(false);
        return;
    }
    let settings = Settings::load();
    let code = lang::init(&settings.language);
    let _ = slint::select_bundled_translation(&code);
    feedback::install("opentakeoff", "OpenTakeoff", opentakeoff_core::version(), opentakeoff_core::paths::cache(), || lang::current() == "pl", true);
    // Jedna kopia programu: druga tylko pokazuje okno pierwszej
    // Tryb zrzutów (diagnostyka) działa obok uruchomionego programu
    if std::env::var_os("OPENTAKEOFF_SNAPSHOT").is_none() && !win::single_instance() {
        // Start z symulatorem przy działającym już programie: nie wyciągaj okna na wierzch
        if !args.iter().any(|a| a == "--from-sim") {
            win::signal_show();
        }
        return;
    }
    // Start z symulatorem włączony: odśwież wpisy (nowa ścieżka po reinstalacji, MSFS doinstalowany później)
    if settings.sim_launch {
        std::thread::spawn(|| {
            if let Err(e) = simlaunch::set_refresh() {
                log::warn(&format!("exe.xml symulatora: {e}"));
            }
        });
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
        popout::restore();
        // Pierwsze uruchomienie: przewodnik
        if !state().borrow().s.welcome_done {
            if let Some(u) = app::ui() {
                u.global::<App>().set_welcome_step(0);
            }
        }
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
            // Przyrządy w osobnych oknach (co 100 ms)
            if tick % 2 == 1 {
                popout::tick();
            }
            // Checklisty: samolot, warunki i odhaczanie przez symulator (także przy innej karcie)
            if tick % 5 == 2 {
                checklists::tick(ui.global::<App>().get_tab() == 13);
            }
            // Nakładki (karta lądowania, pasek do transmisji, okno), skrót zrzutu ekranu, postęp planu
            overlays::tick();
            shots::poll_key();
            if tick % 20 == 0 {
                simbrief::refresh();
                // historia parametrów do wykresów trybu Dane (ostatnie 10 minut)
                let snap = state().borrow().host.snapshot();
                if let Some(s) = snap {
                    params::sample(&s);
                }
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
    popout::close_all();
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
    // OPENTAKEOFF_SNAPSHOT_SIZE="1100x720" – rozmiar okna w pikselach fizycznych (test wąskiego okna)
    if let Some((sw, sh)) = std::env::var("OPENTAKEOFF_SNAPSHOT_SIZE").ok().and_then(|v| {
        let (a, b) = v.split_once('x')?;
        Some((a.trim().parse::<u32>().ok()?, b.trim().parse::<u32>().ok()?))
    }) {
        ui.window().set_maximized(false);
        ui.window().set_size(slint::PhysicalSize::new(sw, sh));
    }
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
        // Karty 0–13 (12: Plan lotu, 13: Checklisty), potem 99: Mapa w widoku 3D (OPENTAKEOFF_SNAPSHOT_TABS="1,8,9,99" – wybrane)
        let order: Vec<i32> = std::env::var("OPENTAKEOFF_SNAPSHOT_TABS").ok().map(|t| t.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_else(|| (0..=13).chain([99]).collect());
        if i as usize >= order.len() {
            cmd(&ui, "nav-3d", "0");
            quit();
            return;
        }
        let tab = order[i as usize];
        let three_d = tab == 99;
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
