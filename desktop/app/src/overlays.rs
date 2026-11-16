//! Nakładki na ekranie symulatora:
//!  - karta lądowania – pokazuje się sama po przyziemieniu (ocena, prędkość pionowa, przeciążenie,
//!    położenie na pasie, uwagi), na wybranym ekranie, w środku albo w rogu, na określony czas,
//!  - pasek do transmisji (stream) – wybrane pola lotu w wybranej kolejności przy krawędzi ekranu,
//!  - okno zawsze na wierzchu – PFD, ND, oba, duży odczyt lotu albo mapa.
//!
//! Okna nie zabierają fokusu symulatorowi i mogą przepuszczać kliknięcia. Ustawienia – karta Nakładki.

use std::cell::RefCell;
use std::time::Instant;

use opentakeoff_core::lang::tr;
use opentakeoff_core::logbook::{landing, LandingReport};
use opentakeoff_core::source::unix_ms;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::app::{self, state, ui};
use crate::screen::{self, Monitor};
use crate::settings::*;
use crate::*;

#[derive(Default)]
struct Ov {
    landing: Option<LandingWindow>,
    landing_shown: Option<Instant>,
    landing_secs: i32,
    landing_place: Option<(usize, i32, i32, bool)>,
    /// Klucz ostatniego znanego lądowania; None – jeszcze nie odczytany (bez karty przy starcie).
    last_key: Option<String>,
    last_report: Option<LandingReport>,
    stream: Option<StreamWindow>,
    stream_visible: bool,
    overlay: Option<OverlayWindow>,
    overlay_visible: bool,
    mons: Vec<Monitor>,
    tick: u64,
}

thread_local! {
    static OV: RefCell<Ov> = RefCell::new(Ov::default());
}

fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

fn minus(s: String) -> String {
    s.replace('-', "−")
}

fn accent() -> slint::Color {
    ui().map(|u| u.global::<Theme>().get_accent()).unwrap_or_else(|| slint::Color::from_rgb_u8(91, 140, 255))
}

fn monitor(i: i32) -> Option<Monitor> {
    OV.with(|o| {
        let o = o.borrow();
        o.mons.get(i.max(0) as usize).or(o.mons.first()).cloned()
    })
}

fn score_color(s: i32) -> slint::Color {
    let (r, g, b) = match s {
        90.. => (60, 207, 122),
        75..=89 => (139, 212, 80),
        55..=74 => (255, 181, 71),
        35..=54 => (255, 138, 76),
        _ => (255, 90, 95),
    };
    slint::Color::from_rgb_u8(r, g, b)
}

/// Lista ekranów od nowa (po podłączeniu monitora) i ustawienia do interfejsu.
pub fn refresh_monitors() {
    let Some(u) = ui() else { return };
    let list = screen::monitors(u.window());
    let labels = screen::monitor_labels(&list);
    OV.with(|o| o.borrow_mut().mons = list);
    u.global::<App>().set_monitors(model(labels.into_iter().map(SharedString::from).collect()));
}

/// Ustawienia nakładek, zrzutów i SimBrief → interfejs.
pub fn push_settings() {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    let s = state().borrow().s.clone();
    let l = &s.landing_card;
    app.set_ovl_landing(LandingCfgV {
        enabled: l.enabled,
        monitor: l.monitor,
        position: l.position,
        size: l.size,
        layout: l.layout,
        seconds: l.seconds,
        opacity: l.opacity as f32 / 100.0,
        margin: l.margin,
        tone: l.tone,
        show_score: l.show_score,
        show_vs: l.show_vs,
        show_g: l.show_g,
        show_speed: l.show_speed,
        show_runway: l.show_runway,
        show_offsets: l.show_offsets,
        show_bounces: l.show_bounces,
        show_wind: l.show_wind,
        show_notes: l.show_notes,
        show_airport: l.show_airport,
        show_aircraft: l.show_aircraft,
        show_scale: l.show_scale,
        click_through: l.click_through,
    });
    let b = &s.stream_bar;
    app.set_ovl_stream(StreamCfgV {
        enabled: b.enabled,
        monitor: b.monitor,
        edge: b.edge,
        width: b.width as f32 / 100.0,
        align: b.align,
        size: b.size,
        opacity: b.opacity as f32 / 100.0,
        style: b.style,
        show_labels: b.show_labels,
        show_logo: b.show_logo,
        accent_line: b.accent_line,
        click_through: b.click_through,
        title: ss(&b.title),
    });
    let w = &s.overlay;
    app.set_ovl_window(OverlayCfgV {
        enabled: w.enabled,
        monitor: w.monitor,
        position: w.position,
        content: w.content,
        size: w.size,
        opacity: w.opacity as f32 / 100.0,
        margin: w.margin,
        show_title: w.show_title,
        click_through: w.click_through,
    });
    // Pola paska: najpierw włączone (w kolejności), potem pozostałe
    let mut fields: Vec<StreamFieldV> = b.fields.iter().filter(|k| STREAM_FIELDS.contains(&k.as_str())).map(|k| StreamFieldV { key: ss(k), label: ss(field_label(k)), on: true }).collect();
    for k in STREAM_FIELDS {
        if !b.fields.iter().any(|f| f == k) {
            fields.push(StreamFieldV { key: ss(k), label: ss(field_label(k)), on: false });
        }
    }
    app.set_stream_fields(model(fields));
    app.set_shot_monitor(s.shot_monitor);
    app.set_shot_key(s.shot_key);
    app.set_simbrief_user(ss(&s.simbrief_user));
}

fn field_label(k: &str) -> &'static str {
    match k {
        "route" => tr("Trasa"),
        "aircraft" => tr("Samolot"),
        "phase" => tr("Faza lotu"),
        "alt" => tr("Wysokość"),
        "ias" => tr("Prędkość przyrządowa"),
        "gs" => tr("Prędkość nad ziemią"),
        "vs" => tr("Prędkość pionowa"),
        "hdg" => tr("Kurs"),
        "wind" => tr("Wiatr"),
        "oat" => tr("Temperatura"),
        "qnh" => "QNH",
        "fuel" => tr("Paliwo"),
        "next" => tr("Następny punkt"),
        "dest" => tr("Do celu"),
        "ap" => tr("Autopilot"),
        "com1" => "COM1",
        "xpdr" => tr("Transponder"),
        "nearest" => tr("Najbliższe lotnisko"),
        "utc" => tr("Czas UTC w symulatorze"),
        "flight" => tr("Czas lotu"),
        "landing" => tr("Ostatnie lądowanie"),
        "title" => tr("Własny tekst"),
        _ => "",
    }
}

// ---------- karta lądowania ----------

fn landing_window() -> Option<LandingWindow> {
    let existing = OV.with(|o| o.borrow().landing.as_ref().map(|w| w.clone_strong()));
    if existing.is_some() {
        return existing;
    }
    let w = LandingWindow::new().ok()?;
    w.on_dismiss(|| hide_landing());
    OV.with(|o| o.borrow_mut().landing = Some(w.clone_strong()));
    Some(w)
}

pub fn hide_landing() {
    let w = OV.with(|o| {
        let mut o = o.borrow_mut();
        o.landing_shown = None;
        o.landing.as_ref().map(|w| w.clone_strong())
    });
    if let Some(w) = w {
        let _ = w.hide();
    }
}

fn sample_report() -> LandingReport {
    landing::evaluate(&landing::Touchdown {
        vs_fpm: -138.0,
        g_force: Some(1.14),
        bank_deg: Some(1.2),
        pitch_deg: Some(3.4),
        ias_kt: Some(134.0),
        runway: Some("29".into()),
        along_ft: Some(1240.0),
        cross_ft: Some(6.0),
        runway_length_ft: Some(9186.0),
        crosswind_kt: Some(8.0),
        bounces: 0,
    })
}

fn fill_landing(w: &LandingWindow, r: &LandingReport, cfg: &LandingCardCfg, preview: bool) {
    let snap = state().borrow().host.snapshot();
    let live = ui().map(|u| u.global::<App>().get_live());
    w.set_score(r.score);
    w.set_grade(ss(landing::grade_of(r.score)));
    w.set_vs_fpm(r.vs_fpm);
    w.set_vs(ss(minus(format!("{}", r.vs_fpm))));
    w.set_g(ss(r.g_force.map(|g| format!("{g:.2}")).unwrap_or_default()));
    w.set_ias(ss(r.ias_kt.map(|v| v.to_string()).unwrap_or_default()));
    w.set_runway(ss(r.runway.clone().unwrap_or_default()));
    w.set_along(ss(r.along_ft.map(|a| minus(format!("{a} ft"))).unwrap_or_default()));
    w.set_cross(ss(r.cross_ft.map(|c| format!("{} ft {}", c.abs(), if c >= 0 { tr("w prawo") } else { tr("w lewo") })).unwrap_or_default()));
    w.set_bounces(ss(r.bounces.to_string()));
    w.set_wind(ss(r.crosswind_kt.map(|c| format!("{} kt", c.abs())).unwrap_or_default()));
    let airport = if preview {
        "EPGD".to_string()
    } else {
        r.airport.clone().filter(|a| !a.is_empty()).or_else(|| live.as_ref().map(|l| l.nearest.split(' ').next().unwrap_or("").to_string())).unwrap_or_default()
    };
    w.set_airport(ss(airport));
    w.set_aircraft(ss(if preview { "Airbus A320neo".to_string() } else { snap.as_ref().map(|s| s.aircraft.title.clone()).unwrap_or_default() }));
    w.set_time(ss(crate::fmt::time(r.at.unwrap_or_else(unix_ms))));
    w.set_notes(model(landing::notes_now(r).into_iter().map(SharedString::from).collect()));
    w.set_heading_text(ss(tr("Lądowanie")));
    w.set_label_along(ss(tr("Za progiem")));
    w.set_label_cross(ss(tr("Od osi pasa")));
    w.set_label_bounces(ss(tr("Odbicia")));
    w.set_label_wind(ss(tr("Wiatr boczny")));
    w.set_label_g(ss(tr("Przeciążenie")));
    w.set_label_ias(ss(tr("Prędkość")));
    w.set_preview_text(ss(tr("Podgląd")));
    w.set_layout(cfg.layout);
    w.set_scale([0.8, 1.0, 1.25, 1.55][cfg.size.clamp(0, 3) as usize]);
    w.set_bg_alpha(cfg.opacity.clamp(30, 100) as f32 / 100.0);
    w.set_tone_mode(cfg.tone);
    w.set_accent(accent());
    w.set_show_score(cfg.show_score);
    w.set_show_vs(cfg.show_vs);
    w.set_show_g(cfg.show_g);
    w.set_show_speed(cfg.show_speed);
    w.set_show_runway(cfg.show_runway);
    w.set_show_offsets(cfg.show_offsets);
    w.set_show_bounces(cfg.show_bounces);
    w.set_show_wind(cfg.show_wind);
    w.set_show_notes(cfg.show_notes);
    w.set_show_airport(cfg.show_airport);
    w.set_show_aircraft(cfg.show_aircraft);
    w.set_show_scale(cfg.show_scale);
    w.set_preview(preview);
    w.set_timed(effective_secs(cfg) > 0);
    w.set_progress(1.0);
}

fn effective_secs(cfg: &LandingCardCfg) -> i32 {
    if cfg.seconds <= 0 && cfg.click_through {
        15
    } else {
        cfg.seconds.max(0)
    }
}

/// Pokazuje kartę lądowania (po przyziemieniu albo jako podgląd z ustawień).
pub fn show_landing(r: &LandingReport, preview: bool) {
    let cfg = state().borrow().s.landing_card.clone();
    if !cfg.enabled && !preview {
        return;
    }
    if OV.with(|o| o.borrow().mons.is_empty()) {
        refresh_monitors();
    }
    let Some(m) = monitor(cfg.monitor) else { return };
    let Some(w) = landing_window() else { return };
    fill_landing(&w, r, &cfg, preview);
    let prev = screen::foreground();
    let _ = w.show();
    screen::position(&w, &m, cfg.position, cfg.margin);
    screen::make_overlay(w.window(), cfg.click_through && !preview);
    screen::restore_foreground(prev, w.window());
    OV.with(|o| {
        let mut o = o.borrow_mut();
        o.landing_shown = Some(Instant::now());
        o.landing_secs = if preview && effective_secs(&cfg) == 0 { 0 } else { effective_secs(&cfg) };
        o.landing_place = Some((cfg.monitor.max(0) as usize, cfg.position, cfg.margin, true));
    });
}

fn landing_tick() {
    let (w, shown, secs, place) = OV.with(|o| {
        let o = o.borrow();
        (o.landing.as_ref().map(|w| w.clone_strong()), o.landing_shown, o.landing_secs, o.landing_place)
    });
    let (Some(w), Some(t0)) = (w, shown) else { return };
    let e = t0.elapsed().as_secs_f32();
    // Rozmiar okna ustala się po pokazaniu – przez pierwszą sekundę poprawiamy położenie
    if e < 1.2 {
        if let Some((mi, pos, margin, _)) = place {
            if let Some(m) = monitor(mi as i32) {
                screen::position(&w, &m, pos, margin);
            }
        }
    }
    if secs > 0 {
        let left = 1.0 - e / secs as f32;
        if left <= 0.0 {
            hide_landing();
        } else {
            w.set_progress(left);
        }
    }
}

/// Wykrywa nowe lądowanie (raport z rejestratora lotu) i pokazuje kartę.
fn detect_landing() {
    let last = state().borrow().host.recording().4;
    let key = last.as_ref().map(|r| serde_json::to_string(r).unwrap_or_default()).unwrap_or_default();
    let (first, changed) = OV.with(|o| {
        let o = o.borrow();
        (o.last_key.is_none(), o.last_key.as_deref() != Some(key.as_str()))
    });
    if !changed {
        return;
    }
    OV.with(|o| {
        let mut o = o.borrow_mut();
        o.last_key = Some(key);
        if last.is_some() {
            o.last_report = last.clone();
        }
    });
    if let Some(r) = &last {
        push_last_landing(r);
        if !first {
            show_landing(r, false);
        }
    }
}

fn push_last_landing(r: &LandingReport) {
    let Some(u) = ui() else { return };
    u.global::<App>().set_last_landing(LastLandingV {
        has: true,
        score: r.score,
        grade: ss(landing::grade_of(r.score)),
        vs: ss(minus(format!("{} ft/min", r.vs_fpm))),
        g: ss(r.g_force.map(|g| format!("{g:.2} g")).unwrap_or_default()),
        runway: ss(r.runway.as_ref().map(|x| format!("RWY {x}")).unwrap_or_default()),
        airport: ss(r.airport.clone().unwrap_or_default()),
        along: ss(r.along_ft.map(|a| minus(format!("{a} ft"))).unwrap_or_default()),
        time: ss(r.at.map(crate::fmt::date_time).unwrap_or_default()),
    });
}

// ---------- pasek do transmisji ----------

fn stream_window() -> Option<StreamWindow> {
    let existing = OV.with(|o| o.borrow().stream.as_ref().map(|w| w.clone_strong()));
    if existing.is_some() {
        return existing;
    }
    let w = StreamWindow::new().ok()?;
    OV.with(|o| o.borrow_mut().stream = Some(w.clone_strong()));
    Some(w)
}

fn stream_cells(cfg: &StreamBarCfg) -> Vec<StreamCell> {
    let Some(u) = ui() else { return Vec::new() };
    let live = u.global::<App>().get_live();
    let has = live.has_data;
    let (started, report, snap) = {
        let st = state();
        let b = st.borrow();
        let r = b.host.recording();
        (r.1, r.4, b.host.snapshot())
    };
    let white = slint::Color::from_rgb_u8(255, 255, 255);
    let d = |v: String| if has { v } else { String::new() };
    let mut out = Vec::new();
    for k in &cfg.fields {
        let (label, value, kind, tone): (String, String, i32, slint::Color) = match k.as_str() {
            "route" => (tr("Trasa").into(), crate::simbrief::route_text(), 1, white),
            "aircraft" => (tr("Samolot").into(), d(live.title.to_string()), 0, white),
            "phase" => (tr("Faza lotu").into(), d(live.phase.to_string()), 4, accent()),
            "alt" => ("ALT".into(), d(format!("{} {}", live.alt, live.alt_unit)), 0, white),
            "ias" => ("IAS".into(), d(format!("{} kt", live.ias)), 0, white),
            "gs" => ("GS".into(), d(format!("{} kt", live.gs)), 0, white),
            "vs" => ("V/S".into(), d(minus(format!("{} fpm", live.vs))), 0, white),
            "hdg" => ("HDG".into(), d(format!("{}°", live.hdg)), 0, white),
            "wind" => (tr("Wiatr").into(), d(live.wind.to_string()), 0, white),
            "oat" => ("OAT".into(), d(live.oat.to_string()), 0, white),
            "qnh" => ("QNH".into(), d(live.qnh.to_string()), 0, white),
            "fuel" => (tr("Paliwo").into(), d(live.fuel.to_string()), 0, white),
            "next" => (tr("Następny punkt").into(), d(format!("{} {}", live.next_wp, live.next_dist).trim().to_string()), 0, white),
            "dest" => {
                let rem = crate::simbrief::remaining_text(snap.as_ref());
                let ete = live.dest_ete.to_string();
                (tr("Do celu").into(), d([rem, ete].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ")), 0, white)
            }
            "ap" => (tr("Autopilot").into(), d(if live.ap_master { format!("AP {}", live.ap_line) } else { "AP OFF".to_string() }), 0, white),
            "com1" => ("COM1".into(), d(live.com1.split_whitespace().next().unwrap_or("").to_string()), 0, white),
            "xpdr" => ("XPDR".into(), d(live.xpdr.to_string()), 0, white),
            "nearest" => (tr("Najbliższe lotnisko").into(), d(live.nearest.to_string()), 0, white),
            "utc" => ("UTC".into(), d(live.sim_time.split_whitespace().nth(1).map(|t| t.chars().take(5).collect::<String>()).unwrap_or_default()), 0, white),
            "flight" => (tr("Czas lotu").into(), started.map(|t| crate::fmt::ete(((unix_ms() - t) / 1000) as f64)).unwrap_or_default(), 0, white),
            "landing" => match &report {
                Some(r) => (tr("Ostatnie lądowanie").into(), minus(format!("{} · {} fpm", r.score, r.vs_fpm)), 2, score_color(r.score)),
                None => (tr("Ostatnie lądowanie").into(), String::new(), 0, white),
            },
            "title" => (String::new(), cfg.title.clone(), 3, white),
            _ => continue,
        };
        out.push(StreamCell { label: ss(label), value: ss(value), kind, tone });
    }
    out
}

fn apply_stream() {
    let cfg = state().borrow().s.stream_bar.clone();
    if !cfg.enabled {
        let w = OV.with(|o| {
            let mut o = o.borrow_mut();
            o.stream_visible = false;
            o.stream.as_ref().map(|w| w.clone_strong())
        });
        if let Some(w) = w {
            let _ = w.hide();
        }
        return;
    }
    let Some(m) = monitor(cfg.monitor) else { return };
    let Some(w) = stream_window() else { return };
    w.set_scale([0.85, 1.0, 1.3][cfg.size.clamp(0, 2) as usize]);
    w.set_bg_alpha(cfg.opacity.clamp(30, 100) as f32 / 100.0);
    w.set_style(cfg.style);
    w.set_edge(cfg.edge);
    w.set_show_labels(cfg.show_labels);
    w.set_show_logo(cfg.show_logo);
    w.set_accent_line(cfg.accent_line);
    w.set_accent(accent());
    w.set_cells(model(stream_cells(&cfg)));
    // Szerokość przed pokazaniem – okno od razu ma właściwy rozmiar i położenie
    let sf0 = ui().map(|u| u.window().scale_factor()).unwrap_or(1.0).max(0.5);
    let phys_w = m.w as f32 * cfg.width.clamp(30, 100) as f32 / 100.0;
    w.set_bar_w(phys_w / sf0);
    let prev = screen::foreground();
    let _ = w.show();
    let sf = w.window().scale_factor().max(0.5);
    if (sf - sf0).abs() > 0.01 {
        w.set_bar_w(phys_w / sf);
    }
    place_stream(&w, &cfg, &m);
    screen::make_overlay(w.window(), cfg.click_through);
    screen::restore_foreground(prev, w.window());
    OV.with(|o| o.borrow_mut().stream_visible = true);
}

fn place_stream(w: &StreamWindow, cfg: &StreamBarCfg, m: &Monitor) {
    let pos = match (cfg.edge, cfg.align) {
        (0, 0) => 3,
        (0, 2) => 4,
        (0, _) => 1,
        (_, 0) => 5,
        (_, 2) => 6,
        _ => 2,
    };
    screen::position(w, m, pos, 0);
}

fn stream_tick() {
    let w = OV.with(|o| {
        let o = o.borrow();
        if o.stream_visible {
            o.stream.as_ref().map(|w| w.clone_strong())
        } else {
            None
        }
    });
    let Some(w) = w else { return };
    let cfg = state().borrow().s.stream_bar.clone();
    w.set_cells(model(stream_cells(&cfg)));
    // Rozmiar okna ustala się po pokazaniu (i po zmianie skali ekranu) – położenie na bieżąco
    if let Some(m) = monitor(cfg.monitor) {
        place_stream(&w, &cfg, &m);
    }
}

// ---------- okno zawsze na wierzchu ----------

fn overlay_window() -> Option<OverlayWindow> {
    let existing = OV.with(|o| o.borrow().overlay.as_ref().map(|w| w.clone_strong()));
    if existing.is_some() {
        return existing;
    }
    let w = OverlayWindow::new().ok()?;
    OV.with(|o| o.borrow_mut().overlay = Some(w.clone_strong()));
    Some(w)
}

fn overlay_dims(cfg: &OverlayCfg) -> (f32, f32) {
    let base = [260.0, 340.0, 440.0, 560.0][cfg.size.clamp(0, 3) as usize];
    match cfg.content {
        3 => (base, base * 0.8),
        4 => (base * 1.3, base),
        _ => (base, base),
    }
}

fn apply_overlay() {
    let cfg = state().borrow().s.overlay.clone();
    if !cfg.enabled {
        let w = OV.with(|o| {
            let mut o = o.borrow_mut();
            o.overlay_visible = false;
            o.overlay.as_ref().map(|w| w.clone_strong())
        });
        if let Some(w) = w {
            let _ = w.hide();
        }
        return;
    }
    let Some(m) = monitor(cfg.monitor) else { return };
    let Some(w) = overlay_window() else { return };
    let (iw, ih) = overlay_dims(&cfg);
    w.set_content(cfg.content);
    w.set_img_w(iw);
    w.set_img_h(ih);
    w.set_show_title(cfg.show_title);
    w.set_bg_alpha(cfg.opacity.clamp(30, 100) as f32 / 100.0);
    w.set_accent(accent());
    w.set_no_data_text(ss(tr("Brak danych lotu")));
    let prev = screen::foreground();
    let _ = w.show();
    render_overlay(&w, &cfg);
    screen::position(&w, &m, cfg.position, cfg.margin);
    screen::make_overlay(w.window(), cfg.click_through);
    screen::restore_foreground(prev, w.window());
    OV.with(|o| o.borrow_mut().overlay_visible = true);
}

fn render_overlay(w: &OverlayWindow, cfg: &OverlayCfg) {
    let Some(u) = ui() else { return };
    let live = u.global::<App>().get_live();
    let snap = state().borrow().host.snapshot();
    let (iw, ih) = overlay_dims(cfg);
    let sf = w.window().scale_factor().max(0.5);
    let (pw, ph) = ((iw * sf) as u32, (ih * sf) as u32);
    let content_name = match cfg.content {
        0 => "PFD",
        1 => "ND",
        2 => "PFD · ND",
        3 => tr("Odczyt lotu"),
        _ => tr("Mapa"),
    };
    w.set_title_text(ss(if live.has_data { format!("{content_name}  ·  {}", live.title) } else { content_name.to_string() }));
    match cfg.content {
        0..=2 => {
            let style = crate::instruments::style_for(snap.as_ref());
            let d = crate::instruments::data(snap.as_ref());
            if cfg.content != 1 {
                w.set_image_a(crate::avionics::render_pfd(style, &d, pw, ph));
            }
            if cfg.content >= 1 {
                let set = state().borrow().s.clone();
                let range = ND_RANGES[set.nd_range.clamp(0, ND_RANGES.len() as i32 - 1) as usize];
                let (route, fixes) = snap.as_ref().map(|s| crate::instruments::nd_inputs(s, range)).unwrap_or_default();
                let layers = crate::avionics::nd::Layers { airports: set.nd_layers & 1 != 0, vor: set.nd_layers & 2 != 0, ndb: set.nd_layers & 4 != 0 };
                let img = crate::avionics::render_nd(style, &d, pw, ph, range, set.nd_rose, &route, &fixes, layers);
                if cfg.content == 1 {
                    w.set_image_a(img);
                } else {
                    w.set_image_b(img);
                }
            }
        }
        3 => {
            w.set_has_data(live.has_data);
            w.set_alt(live.alt.clone());
            w.set_alt_unit(live.alt_unit.clone());
            w.set_ias(live.ias.clone());
            w.set_gs(live.gs.clone());
            w.set_vs(ss(minus(live.vs.to_string())));
            w.set_hdg(live.hdg.clone());
            w.set_phase(live.phase.clone());
            w.set_next(ss(format!("{} {}", live.next_wp, live.next_dist).trim()));
        }
        _ => {
            let mut v = crate::map::View { w: iw as f64, h: ih as f64, zoom: 9.5, ..Default::default() };
            let mut ov = crate::map::Overlay::default();
            if let Some(s) = &snap {
                v.lat = s.position.lat;
                v.lon = s.position.lon;
                ov.aircraft = Some((s.position.lat, s.position.lon, s.attitude.hdg_true_deg));
                let track = state().borrow().host.live_track();
                if track.len() > 1 {
                    ov.tracks.push(crate::map::Track { pts: track.iter().map(|p| (p.lat, p.lon, p.alt)).collect(), color: None, width: 3.0 });
                }
            }
            let plan = crate::simbrief::route();
            if plan.len() > 1 {
                ov.tracks.push(crate::map::Track { pts: plan.iter().map(|p| (p.0, p.1, p.2)).collect(), color: Some([79, 216, 235, 220]), width: 2.0 });
            }
            let r = crate::map::render(&v, crate::map::Style::Dark, &ov, sf, true);
            w.set_image_a(r.image);
        }
    }
}

fn overlay_tick(tick: u64) {
    let w = OV.with(|o| {
        let o = o.borrow();
        if o.overlay_visible {
            o.overlay.as_ref().map(|w| w.clone_strong())
        } else {
            None
        }
    });
    let Some(w) = w else { return };
    let cfg = state().borrow().s.overlay.clone();
    let every = if cfg.content <= 2 { 2 } else { 10 };
    if tick % every == 0 {
        render_overlay(&w, &cfg);
    }
}

// ---------- wspólne ----------

/// Co 50 ms z głównego zegara.
pub fn tick() {
    let t = OV.with(|o| {
        let mut o = o.borrow_mut();
        o.tick += 1;
        o.tick
    });
    if t % 5 == 0 {
        detect_landing();
    }
    landing_tick();
    if t % 10 == 0 {
        stream_tick();
    }
    overlay_tick(t);
}

/// Pokazuje albo chowa okna zgodnie z ustawieniami (po starcie i po każdej zmianie).
pub fn apply() {
    if OV.with(|o| o.borrow().mons.is_empty()) {
        refresh_monitors();
    }
    apply_stream();
    apply_overlay();
}

/// Zamknięcie programu: chowa nakładki.
pub fn close_all() {
    let (a, b, c) = OV.with(|o| {
        let o = o.borrow();
        (o.landing.as_ref().map(|w| w.clone_strong()), o.stream.as_ref().map(|w| w.clone_strong()), o.overlay.as_ref().map(|w| w.clone_strong()))
    });
    if let Some(w) = a {
        let _ = w.hide();
    }
    if let Some(w) = b {
        let _ = w.hide();
    }
    if let Some(w) = c {
        let _ = w.hide();
    }
}

fn set_bool(v: &str) -> bool {
    v == "1" || v == "true"
}

fn set_int(v: &str) -> i32 {
    v.parse::<f32>().map(|f| f.round() as i32).unwrap_or(0)
}

/// Zmiana jednego ustawienia: „landing.size=2”, „stream.enabled=1”, „overlay.content=4”.
fn set_option(path: &str, v: &str) -> bool {
    let st = state();
    let mut b = st.borrow_mut();
    let s = &mut b.s;
    let pct = |v: &str| -> i32 {
        let f: f32 = v.parse().unwrap_or(1.0);
        (if f <= 1.0 { f * 100.0 } else { f }).round().clamp(30.0, 100.0) as i32
    };
    match path {
        "landing.enabled" => s.landing_card.enabled = set_bool(v),
        "landing.monitor" => s.landing_card.monitor = set_int(v).max(0),
        "landing.position" => s.landing_card.position = set_int(v).clamp(0, POSITIONS - 1),
        "landing.size" => s.landing_card.size = set_int(v).clamp(0, 3),
        "landing.layout" => s.landing_card.layout = set_int(v).clamp(0, 2),
        "landing.seconds" => s.landing_card.seconds = set_int(v).clamp(0, 300),
        "landing.opacity" => s.landing_card.opacity = pct(v),
        "landing.margin" => s.landing_card.margin = set_int(v).clamp(0, 400),
        "landing.tone" => s.landing_card.tone = set_int(v).clamp(0, 2),
        "landing.show-score" => s.landing_card.show_score = set_bool(v),
        "landing.show-vs" => s.landing_card.show_vs = set_bool(v),
        "landing.show-g" => s.landing_card.show_g = set_bool(v),
        "landing.show-speed" => s.landing_card.show_speed = set_bool(v),
        "landing.show-runway" => s.landing_card.show_runway = set_bool(v),
        "landing.show-offsets" => s.landing_card.show_offsets = set_bool(v),
        "landing.show-bounces" => s.landing_card.show_bounces = set_bool(v),
        "landing.show-wind" => s.landing_card.show_wind = set_bool(v),
        "landing.show-notes" => s.landing_card.show_notes = set_bool(v),
        "landing.show-airport" => s.landing_card.show_airport = set_bool(v),
        "landing.show-aircraft" => s.landing_card.show_aircraft = set_bool(v),
        "landing.show-scale" => s.landing_card.show_scale = set_bool(v),
        "landing.click-through" => s.landing_card.click_through = set_bool(v),
        "stream.enabled" => s.stream_bar.enabled = set_bool(v),
        "stream.monitor" => s.stream_bar.monitor = set_int(v).max(0),
        "stream.edge" => s.stream_bar.edge = set_int(v).clamp(0, 1),
        "stream.width" => s.stream_bar.width = pct(v),
        "stream.align" => s.stream_bar.align = set_int(v).clamp(0, 2),
        "stream.size" => s.stream_bar.size = set_int(v).clamp(0, 2),
        "stream.opacity" => s.stream_bar.opacity = pct(v),
        "stream.style" => s.stream_bar.style = set_int(v).clamp(0, 2),
        "stream.show-labels" => s.stream_bar.show_labels = set_bool(v),
        "stream.show-logo" => s.stream_bar.show_logo = set_bool(v),
        "stream.accent-line" => s.stream_bar.accent_line = set_bool(v),
        "stream.click-through" => s.stream_bar.click_through = set_bool(v),
        "stream.title" => s.stream_bar.title = v.to_string(),
        "overlay.enabled" => s.overlay.enabled = set_bool(v),
        "overlay.monitor" => s.overlay.monitor = set_int(v).max(0),
        "overlay.position" => s.overlay.position = set_int(v).clamp(0, POSITIONS - 1),
        "overlay.content" => s.overlay.content = set_int(v).clamp(0, 4),
        "overlay.size" => s.overlay.size = set_int(v).clamp(0, 3),
        "overlay.opacity" => s.overlay.opacity = pct(v),
        "overlay.margin" => s.overlay.margin = set_int(v).clamp(0, 400),
        "overlay.show-title" => s.overlay.show_title = set_bool(v),
        "overlay.click-through" => s.overlay.click_through = set_bool(v),
        _ => return false,
    }
    s.save();
    true
}

pub fn cmd(id: &str, arg: &str) -> bool {
    match id {
        "ovl" => {
            let (path, v) = arg.split_once('=').unwrap_or((arg, ""));
            if !set_option(path, v) {
                return true;
            }
            push_settings();
            if path.starts_with("landing.") {
                // Widoczna karta (podgląd) od razu w nowym wyglądzie
                let visible = OV.with(|o| o.borrow().landing_shown.is_some());
                if visible && path != "landing.enabled" {
                    show_landing(&sample_report(), true);
                }
            } else if path.starts_with("stream.") {
                apply_stream();
            } else {
                apply_overlay();
            }
        }
        "ovl-landing-preview" => show_landing(&sample_report(), true),
        "ovl-landing-last" => {
            let r = OV.with(|o| o.borrow().last_report.clone());
            match r {
                Some(r) => show_landing(&r, true),
                None => app::toast(tr("Nie było jeszcze lądowania w tej sesji")),
            }
        }
        "ovl-landing-hide" => hide_landing(),
        "ovl-monitors" => {
            refresh_monitors();
            apply();
        }
        "stream-field" | "stream-field-up" | "stream-field-down" => {
            {
                let st = state();
                let mut b = st.borrow_mut();
                let f = &mut b.s.stream_bar.fields;
                let pos = f.iter().position(|x| x == arg);
                match (id, pos) {
                    ("stream-field", Some(i)) => {
                        f.remove(i);
                    }
                    ("stream-field", None) => {
                        if STREAM_FIELDS.contains(&arg) {
                            f.push(arg.to_string());
                        }
                    }
                    ("stream-field-up", Some(i)) if i > 0 => f.swap(i, i - 1),
                    ("stream-field-down", Some(i)) if i + 1 < f.len() => f.swap(i, i + 1),
                    _ => {}
                }
                b.s.save();
            }
            push_settings();
            stream_tick();
        }
        _ => return false,
    }
    true
}
