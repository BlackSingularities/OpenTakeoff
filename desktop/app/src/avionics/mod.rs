//! Przyrządy przeniesione z aplikacji na telefon: PFD w sześciu stylach kabin (A320, 737, 787, ATR 72,
//! E190, G1000), zegary analogowe (C172, R44, szybowiec), ND/MFD i zegary silnika. Wszystko rysowane
//! w Rust na płótnie [`gfx::Canvas`] i podawane do Slinta jako obraz.

pub mod airbus;
pub mod atr;
pub mod b787;
pub mod boeing;
pub mod data;
pub mod e190;
pub mod ecam;
pub mod garmin;
pub mod gauges;
pub mod gfx;
pub mod nd;
pub mod simlayer;

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

pub use data::{DisplayStyle, FlightData};
use gfx::*;

/// Wspólny interfejs PFD: każdy rysuje w wirtualnym prostokącie vw × vh o proporcjach prawdziwego wyświetlacza.
pub trait Pfd {
    fn vw(&self) -> f32 {
        1000.0
    }
    fn vh(&self) -> f32 {
        1000.0
    }
    fn draw(&mut self, c: &mut Canvas, d: &FlightData);
}

struct State {
    pfds: HashMap<DisplayStyle, Box<dyn Pfd>>,
    panels: HashMap<DisplayStyle, gauges::Panel>,
    nds: HashMap<DisplayStyle, nd::Nd>,
    trend: (f32, Option<Instant>, f32),
}

thread_local! {
    static ST: RefCell<State> = RefCell::new(State { pfds: HashMap::new(), panels: HashMap::new(), nds: HashMap::new(), trend: (f32::NAN, None, 0.0) });
}

fn make_pfd(s: DisplayStyle) -> Box<dyn Pfd> {
    match s {
        DisplayStyle::Airbus => Box::new(airbus::AirbusPfd::new()),
        DisplayStyle::Boeing => Box::new(boeing::BoeingPfd::new()),
        DisplayStyle::B787 => Box::new(b787::B787Pfd),
        DisplayStyle::Atr => Box::new(atr::AtrPfd),
        DisplayStyle::Embraer => Box::new(e190::E190Pfd::new()),
        _ => Box::new(garmin::GarminPfd),
    }
}

/// Przyspieszenie (kt/s) z kolejnych odczytów IAS, wygładzone; zwraca przyrost w ciągu 10 s.
pub fn speed_trend(ias: f32) -> f32 {
    ST.with(|s| {
        let mut s = s.borrow_mut();
        let now = Instant::now();
        let (last, t, tr) = s.trend;
        match t {
            Some(t0) if !last.is_nan() => {
                let dt = (now - t0).as_secs_f32();
                if dt >= 0.2 {
                    let a = (ias - last) / dt;
                    s.trend = (ias, Some(now), tr * 0.7 + a * 0.3);
                }
            }
            _ => s.trend = (ias, Some(now), 0.0),
        }
        s.trend.2 * 10.0
    })
}

/// Główny wyświetlacz lotu w danym stylu (dla stylów analogowych – panel zegarów).
pub fn render_pfd(style: DisplayStyle, d: &FlightData, w: u32, h: u32) -> slint::Image {
    let mut c = Canvas::new(w, h);
    let (wf, hf) = (c.width(), c.height());
    c.rect(0.0, 0.0, wf, hf, BLACK);
    ST.with(|s| {
        let mut s = s.borrow_mut();
        if style.analog() {
            let kind = match style {
                DisplayStyle::Heli => gauges::Kind::Heli,
                DisplayStyle::Glider => gauges::Kind::Glider,
                _ => gauges::Kind::Cessna,
            };
            s.panels.entry(style).or_insert_with(|| gauges::Panel::new(kind)).draw(&mut c, d, wf, hf);
        } else {
            let r = s.pfds.entry(style).or_insert_with(|| make_pfd(style));
            let k = (wf / r.vw()).min(hf / r.vh());
            c.save();
            c.translate((wf - r.vw() * k) / 2.0, (hf - r.vh() * k) / 2.0);
            c.scale(k, k);
            let (vw, vh) = (r.vw(), r.vh());
            c.clip_rect(0.0, 0.0, vw, vh);
            r.draw(&mut c, d);
            c.restore();
        }
    });
    c.into_image()
}

/// Panel zegarów analogowych niezależnie od stylu PFD.
pub fn render_panel(kind: gauges::Kind, d: &FlightData, w: u32, h: u32) -> slint::Image {
    let style = match kind {
        gauges::Kind::Cessna => DisplayStyle::Cessna,
        gauges::Kind::Heli => DisplayStyle::Heli,
        gauges::Kind::Glider => DisplayStyle::Glider,
    };
    render_pfd(style, d, w, h)
}

#[allow(clippy::too_many_arguments)]
pub fn render_nd(style: DisplayStyle, d: &FlightData, w: u32, h: u32, range_nm: f32, rose: bool, route: &[nd::RoutePt], fixes: &[nd::NdFix], layers: nd::Layers) -> slint::Image {
    let mut c = Canvas::new(w, h);
    let (wf, hf) = (c.width(), c.height());
    ST.with(|s| {
        let mut s = s.borrow_mut();
        s.nds.entry(style).or_insert_with(|| nd::Nd::new(style)).draw(&mut c, d, wf, hf, range_nm, rose, route, fixes, layers);
    });
    c.into_image()
}

/// Zegary silnika i systemów (każdy silnik: N1/RPM, EGT, przepływ paliwa) + paliwo, klapy, podwozie, trym.
pub fn render_engines(style: DisplayStyle, d: &FlightData, w: u32, h: u32) -> slint::Image {
    let mut c = Canvas::new(w, h);
    let (wf, hf) = (c.width(), c.height());
    // Samoloty komunikacyjne: ekran silników w stylu kabiny (ECAM / EICAS)
    if matches!(style, DisplayStyle::Airbus | DisplayStyle::Boeing | DisplayStyle::B787 | DisplayStyle::Atr | DisplayStyle::Embraer) {
        ecam::draw(&mut c, d, style, wf, hf);
        return c.into_image();
    }
    if let Some(p) = rect_path(0.0, 0.0, wf, hf) {
        c.fill_radial(&p, wf / 2.0, hf / 2.0, wf.max(hf) * 0.75, 0xFF2C2F33, 0xFF16171A);
    }
    let engines: Vec<data::Engine> = if d.engines.is_empty() { vec![data::Engine { n1: f32::NAN, n2: f32::NAN, rpm: f32::NAN, egt: f32::NAN, ff: f32::NAN, thr: f32::NAN }] } else { d.engines.iter().take(4).copied().collect() };
    let n = engines.len();
    let info_h = (hf * 0.16).clamp(34.0, 70.0);
    let area_h = hf - info_h;
    // Szeroki pas: wszystkie tarcze w jednym rzędzie; wysoki: kolumna na silnik, trzy tarcze pod sobą
    let horizontal = wf / area_h > 1.6;
    let (cols, rows) = if horizontal { (n * 3, 1) } else { (n, 3) };
    let cell_w = wf / cols as f32;
    let cell_h = area_h / rows as f32;
    let r = (cell_w.min(cell_h) * 0.44).max(10.0);
    for (i, e) in engines.iter().enumerate() {
        let pos = |k: usize| -> (f32, f32) {
            if horizontal {
                (cell_w * ((i * 3 + k) as f32 + 0.5), cell_h * 0.5)
            } else {
                (cell_w * (i as f32 + 0.5), cell_h * (k as f32 + 0.5))
            }
        };
        let (x, y) = pos(0);
        if d.piston {
            gauges::engine_dial(&mut c, x, y, r, &format!("RPM {}", i + 1), e.rpm, 3000.0, (2100.0, 2700.0), 2700.0, "RPM", 0);
        } else {
            gauges::engine_dial(&mut c, x, y, r, &format!("N1 {}", i + 1), e.n1, 110.0, (20.0, 100.0), 104.0, "%", 1);
        }
        let (x, y) = pos(1);
        gauges::engine_dial(&mut c, x, y, r, &format!("EGT {}", i + 1), e.egt, if d.piston { 900.0 } else { 1100.0 }, if d.piston { (500.0, 800.0) } else { (300.0, 850.0) }, if d.piston { 850.0 } else { 950.0 }, "°C", 0);
        let (x, y) = pos(2);
        gauges::engine_dial(&mut c, x, y, r, &format!("FF {}", i + 1), e.ff, if d.piston { 80.0 } else { 3000.0 }, (0.0, if d.piston { 60.0 } else { 2600.0 }), if d.piston { 80.0 } else { 3000.0 }, "kg/h", 0);
    }
    let y0 = hf - info_h;
    c.rect(0.0, y0, wf, hf, 0xFF0C0D0F);
    c.line(0.0, y0, wf, y0, 0xFF3C3E42, 1.5);
    let sz = (info_h * 0.28).clamp(10.0, 22.0);
    let lbl = 0xFFB8BCC2;
    let cols = [
        ("FUEL", if d.fuel_kg.is_nan() { "---".into() } else { format!("{} kg", roundi(d.fuel_kg)) }),
        ("FLAPS", d.flaps_index.map(|f| format!("{}/{}", f, d.flaps_positions.unwrap_or(1) - 1)).unwrap_or_else(|| "---".into())),
        ("GEAR", d.gear_down.map(|g| if g { "DOWN".to_string() } else { "UP".to_string() }).unwrap_or_else(|| "---".into())),
        ("SPLR", if d.spoilers_pct.is_nan() { "---".into() } else { format!("{}%", roundi(d.spoilers_pct)) }),
        ("TRIM", if d.trim_pct.is_nan() { "---".into() } else { format!("{:+}%", roundi(d.trim_pct)) }),
    ];
    let cw = wf / cols.len() as f32;
    for (i, (k, v)) in cols.iter().enumerate() {
        let x = cw * (i as f32 + 0.5);
        c.text(k, x, y0 + info_h * 0.32, sz * 0.8, lbl);
        let col = if *k == "GEAR" && v == "DOWN" { 0xFF3FE07A } else { WHITE };
        c.text_b(v, x, y0 + info_h * 0.68, sz, col);
    }
    c.into_image()
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn sample() -> FlightData {
        let mut d = FlightData::default();
        d.att_valid = true;
        d.speed_valid = true;
        d.alt_valid = true;
        d.pitch = 4.0;
        d.roll = -12.0;
        d.heading = 274.0;
        d.track = 270.0;
        d.has_track = true;
        d.speed_kt = 248.0;
        d.speed_trend = 6.0;
        d.alt_ft = 11240.0;
        d.vs_fpm = 1450.0;
        d.fpa = 3.0;
        d.gs_kt = 286.0;
        d.tas_kt = 291.0;
        d.mach = 0.45;
        d.alt_bug = 15000.0;
        d.hdg_bug = 280.0;
        d.spd_bug = 250.0;
        d.wind_dir = 310.0;
        d.wind_kt = 24.0;
        d.wind_valid = true;
        d.oat_c = -8.0;
        d.ra_ft = 2400.0;
        d.ra_valid = true;
        d.fd_on = true;
        d.fd_pitch = 6.0;
        d.fd_roll = -8.0;
        d.fma.ap = true;
        d.fma.fd = true;
        d.fma.athr_armed = true;
        d.fma.athr_active = true;
        d.fma.lateral = Some(data::Lateral::NavGps);
        d.fma.vertical = Some(data::Vertical::Vs);
        d.vs_bug = 1500.0;
        d.nav.source = data::NavSource::Gps;
        d.nav.course = 268.0;
        d.nav.cdi = 0.6;
        d.nav.ident = Some("ROXIL".into());
        d.nav.dme_nm = 22.4;
        d.gps.next_id = Some("ROXIL".into());
        d.gps.next_dist_nm = 22.4;
        d.gps.dtk = 268.0;
        d.lat = 52.2;
        d.lon = 20.9;
        d.com1 = 118.3;
        d.nav1 = 110.3;
        d.squawk = Some("4721".into());
        d.engines = vec![data::Engine { n1: 88.2, n2: 94.0, rpm: 2350.0, egt: 640.0, ff: 1450.0, thr: 85.0 }; 2];
        d.fuel_kg = 6400.0;
        d.flaps_index = Some(0);
        d.flaps_positions = Some(5);
        d.gear_down = Some(false);
        d
    }

    #[test]
    fn render_all_to_png() {
        let Ok(dir) = std::env::var("OT_RENDER_DIR") else { return };
        let d = sample();
        for s in DisplayStyle::ALL {
            let mut c = Canvas::new(800, 800);
            let mut r = make_pfd(s);
            if s.analog() {
                let kind = match s {
                    DisplayStyle::Heli => gauges::Kind::Heli,
                    DisplayStyle::Glider => gauges::Kind::Glider,
                    _ => gauges::Kind::Cessna,
                };
                let mut c = Canvas::new(1200, 800);
                gauges::Panel::new(kind).draw(&mut c, &d, 1200.0, 800.0);
                c.pm.save_png(format!("{dir}/panel_{}.png", s.key())).unwrap();
                continue;
            }
            let k = (800.0 / r.vw()).min(800.0 / r.vh());
            c.scale(k, k);
            r.draw(&mut c, &d);
            c.pm.save_png(format!("{dir}/pfd_{}.png", s.key())).unwrap();
            let mut c = Canvas::new(800, 800);
            let route = vec![nd::RoutePt { ident: "WA123".into(), lat: 52.25, lon: 20.4, constraint: Some("FL150".into()) }, nd::RoutePt { ident: "ROXIL".into(), lat: 52.3, lon: 20.0, constraint: None }];
            let fixes = vec![nd::NdFix { ident: "WAR".into(), kind: "vor", lat: 52.1, lon: 20.6 }, nd::NdFix { ident: "EPMO".into(), kind: "airport", lat: 52.45, lon: 20.65 }];
            nd::Nd::new(s).draw(&mut c, &d, 800.0, 800.0, 40.0, s == DisplayStyle::Garmin, &route, &fixes, nd::Layers { airports: true, vor: true, ndb: true });
            c.pm.save_png(format!("{dir}/nd_{}.png", s.key())).unwrap();
        }
        for (s, name) in [(DisplayStyle::Airbus, "a320"), (DisplayStyle::Boeing, "b737")] {
            let mut c = Canvas::new(2400, 330);
            ecam::draw(&mut c, &d, s, 2400.0, 330.0);
            c.pm.save_png(format!("{dir}/ecam_wide_{name}.png")).unwrap();
            let mut c = Canvas::new(900, 700);
            ecam::draw(&mut c, &d, s, 900.0, 700.0);
            c.pm.save_png(format!("{dir}/ecam_{name}.png")).unwrap();
        }
    }
}
