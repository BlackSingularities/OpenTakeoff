//! Karta Mapa: rozbudowana mapa nawigacyjna 2D i widok 3D.
//!
//! 2D: podkład z kafelków, lotniska, pasy, VOR/NDB z bazy OurAirports (róże VOR z radialami względem
//! deklinacji stacji), róża kursowa na samolocie (kurs, tor, nastawa AP, namiar na punkt), pierścienie
//! odległości, wektor przewidywanego toru (1, 2, 5 min, z bieżącym zakrętem), ślad i trasa z GPS.
//! Narzędzia: radiale od punktu (jak w telefonie: radial i odległość samolotu, wybrany radial IN/OUT
//! z kursem przechwycenia i poprawką na wiatr) oraz pomiar odległości i namiaru.
//! 3D: [`crate::scene3d`] – kamera za samolotem, swobodna albo z góry.

use std::cell::RefCell;
use std::time::Instant;

use opentakeoff_core::aero::{airports, navaids};
use opentakeoff_core::lang::tr;
use opentakeoff_core::navdata::math::{bearing_deg, distance_nm};
use opentakeoff_core::proto::FlightState;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::app::{self, state, ui};
use crate::avionics::gfx::{alpha, Align, Canvas};
use crate::map::{self, Overlay, Point, Style, Track, View};
use crate::scene3d::{self, CamMode, Camera, Label3};
use crate::settings::*;
use crate::*;

#[derive(Clone, Debug)]
struct RefPt {
    lat: f64,
    lon: f64,
    name: String,
    /// Deklinacja w punkcie (E +): stacji VOR albo z symulatora.
    var: f64,
}

struct Nav {
    view: View,
    cam: Camera,
    tool: i32,
    ref_pt: Option<RefPt>,
    sel_radial: Option<i32>,
    inbound: bool,
    measure: Vec<(f64, f64)>,
    hover: Option<(f64, f64)>,
    pick: Option<Vec<(String, String)>>,
    last: Instant,
}

thread_local! {
    static NAV: RefCell<Nav> = RefCell::new(Nav {
        view: View { zoom: 8.0, ..Default::default() },
        cam: Camera::default(),
        tool: 0,
        ref_pt: None,
        sel_radial: None,
        inbound: true,
        measure: Vec::new(),
        hover: None,
        pick: None,
        last: Instant::now(),
    });
}

fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

fn norm360(a: f64) -> f64 {
    a.rem_euclid(360.0)
}
fn norm180(a: f64) -> f64 {
    let x = norm360(a);
    if x > 180.0 {
        x - 360.0
    } else {
        x
    }
}
fn deg3(a: f64) -> String {
    let v = norm360(a).round() as i32;
    format!("{:03}", if v == 0 { 360 } else { v })
}
fn radial_of(deg: f64) -> i32 {
    let v = norm360(deg).round() as i32;
    if v == 0 {
        360
    } else {
        v
    }
}

/// Punkt odległy o `nm` w kierunku `brg` (°, rzeczywisty) – sfera.
pub fn dest(lat: f64, lon: f64, brg: f64, nm: f64) -> (f64, f64) {
    let d = nm / 3440.065;
    let (la, lo, b) = (lat.to_radians(), lon.to_radians(), brg.to_radians());
    let la2 = (la.sin() * d.cos() + la.cos() * d.sin() * b.cos()).asin();
    let lo2 = lo + (b.sin() * d.sin() * la.cos()).atan2(d.cos() - la.sin() * la2.sin());
    (la2.to_degrees(), ((lo2.to_degrees() + 540.0) % 360.0) - 180.0)
}

fn dms(lat: f64, lon: f64) -> String {
    let f = |v: f64, pos: char, neg: char, w: usize| {
        let a = v.abs();
        let d = a.floor();
        let m = (a - d) * 60.0;
        format!("{}{:0w$}°{:04.1}'", if v >= 0.0 { pos } else { neg }, d as i32, m, w = w)
    };
    format!("{} {}", f(lat, 'N', 'S', 2), f(lon, 'E', 'W', 3))
}

fn from_screen(v: &View, x: f64, y: f64) -> (f64, f64) {
    let (cx, cy) = map::project(v.lat, v.lon, v.zoom);
    let (lat, lon) = map::unproject(cx + x - v.w / 2.0, cy + y - v.h / 2.0, v.zoom);
    (lat, ((lon + 540.0) % 360.0) - 180.0)
}

fn style(dark: bool) -> Style {
    match state().borrow().s.map_style {
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

fn layers() -> u32 {
    state().borrow().s.nav_layers
}

fn mag_var(s: Option<&FlightState>) -> f64 {
    s.map(|s| s.avionics.as_ref().map(|a| a.mag_var_deg).unwrap_or(norm180(s.attitude.hdg_true_deg - s.attitude.hdg_mag_deg))).unwrap_or(0.0)
}

/// Pozycja odniesienia: samolot, a bez niego środek mapy.
fn origin(n: &Nav, s: Option<&FlightState>) -> (f64, f64, bool) {
    match s {
        Some(s) => (s.position.lat, s.position.lon, true),
        None => (n.view.lat, n.view.lon, false),
    }
}

// ---------- zdarzenia ----------

pub fn event(kind: &str, x: f32, y: f32, dx: f32, dy: f32) {
    let three_d = state().borrow().s.nav_3d;
    let mut rerender = true;
    NAV.with(|n| {
        let mut n = n.borrow_mut();
        match kind {
            "size" => {
                if (n.view.w - x as f64).abs() < 0.5 && (n.view.h - y as f64).abs() < 0.5 {
                    rerender = false;
                    return;
                }
                n.view.w = (x as f64).max(16.0);
                n.view.h = (y as f64).max(16.0);
            }
            "pan" => {
                if three_d {
                    n.cam.drag(dx as f64, dy as f64);
                } else {
                    n.view.pan(dx as f64, dy as f64);
                    n.view.follow = false;
                    if let Some(u) = ui() {
                        u.global::<App>().set_follow(false);
                    }
                }
            }
            "zoom" => {
                if three_d {
                    n.cam.zoom(dy as f64);
                } else {
                    n.view.zoom_at(dy as f64, x as f64, y as f64);
                }
            }
            "follow" => {
                n.view.follow = x > 0.5;
                n.view.user_moved = false;
            }
            "fit" => {
                let track = state().borrow().host.live_track();
                if let Some(b) = map::bounds(track.iter().map(|p| (p.lat, p.lon))) {
                    n.view.fit(b);
                    n.view.follow = false;
                }
                n.cam.dist = 1800.0;
                n.cam.pitch = 18.0;
                n.cam.yaw = 0.0;
            }
            "hover" => {
                if three_d {
                    rerender = false;
                    return;
                }
                n.hover = Some(from_screen(&n.view, x as f64, y as f64));
                rerender = n.tool == 2 && n.measure.len() == 1;
                if !rerender {
                    drop(n);
                    side_hover();
                }
            }
            "leave" => {
                n.hover = None;
                rerender = false;
                drop(n);
                side_hover();
            }
            "click" => {
                if three_d {
                    rerender = false;
                    return;
                }
                let (lat, lon) = from_screen(&n.view, x as f64, y as f64);
                let snap = snap(&n.view, x as f64, y as f64);
                match n.tool {
                    1 => {
                        if n.ref_pt.is_none() {
                            let snap_var = snap.as_ref().and_then(|s| s.3);
                            let var = snap_var.unwrap_or_else(|| mag_var(state().borrow().host.snapshot().as_ref()));
                            let (la, lo, name) = snap.map(|s| (s.0, s.1, Some(s.2))).unwrap_or((lat, lon, None));
                            n.ref_pt = Some(RefPt { lat: la, lon: lo, name: name.unwrap_or_else(|| dms(la, lo)), var });
                            n.sel_radial = None;
                        } else if let Some(r) = n.ref_pt.clone() {
                            n.sel_radial = Some(radial_of(bearing_deg(r.lat, r.lon, lat, lon) - r.var));
                        }
                    }
                    2 => {
                        let p = snap.map(|s| (s.0, s.1)).unwrap_or((lat, lon));
                        if n.measure.len() >= 2 {
                            n.measure.clear();
                        }
                        n.measure.push(p);
                    }
                    _ => {
                        n.pick = snap_info(&n.view, x as f64, y as f64);
                    }
                }
            }
            _ => rerender = false,
        }
    });
    if rerender {
        render(true);
    }
}

/// Najbliższy obiekt pod kursorem: (lat, lon, nazwa, deklinacja stacji).
fn snap(v: &View, x: f64, y: f64) -> Option<(f64, f64, String, Option<f64>)> {
    let (lat, lon) = from_screen(v, x, y);
    let px_nm = 156_543.03 * lat.to_radians().cos() / 2f64.powf(v.zoom) / 1852.0;
    let radius = 16.0 * px_nm;
    let mut best: Option<(f64, (f64, f64, String, Option<f64>))> = None;
    let lay = layers();
    if lay & (NAV_VOR | NAV_NDB) != 0 {
        if let Some(db) = navaids::db() {
            for (d, a) in db.within(lat, lon, radius) {
                let on = if a.kind.is_ndb() { lay & NAV_NDB != 0 } else { lay & NAV_VOR != 0 };
                if on && best.as_ref().is_none_or(|b| d < b.0) {
                    best = Some((d, (a.lat, a.lon, format!("{} {}", a.ident, a.kind.label()), Some(a.mag_var))));
                }
            }
        }
    }
    if let Some(db) = airports::db() {
        for (d, a) in db.within(lat, lon, radius, false) {
            if best.as_ref().is_none_or(|b| d < b.0 * 0.8) {
                best = Some((d, (a.lat, a.lon, a.ident.clone(), None)));
            }
        }
    }
    best.map(|b| b.1)
}

/// Opis obiektu wskazanego kliknięciem (bez narzędzia).
fn snap_info(v: &View, x: f64, y: f64) -> Option<Vec<(String, String)>> {
    let (lat, lon) = from_screen(v, x, y);
    let px_nm = 156_543.03 * lat.to_radians().cos() / 2f64.powf(v.zoom) / 1852.0;
    let radius = 16.0 * px_nm;
    let snapshot = state().borrow().host.snapshot();
    let rel = |la: f64, lo: f64| -> Option<(String, String)> {
        snapshot.as_ref().map(|s| {
            let var = mag_var(Some(s));
            (tr("Od samolotu").to_string(), format!("{}° · {:.1} NM", deg3(bearing_deg(s.position.lat, s.position.lon, la, lo) - var), distance_nm(s.position.lat, s.position.lon, la, lo)))
        })
    };
    if let Some(db) = navaids::db() {
        if let Some((_, a)) = db.within(lat, lon, radius).into_iter().next() {
            let mut v = vec![
                (tr("Pomoc nawigacyjna").to_string(), format!("{} · {}", a.ident, a.kind.label())),
                (tr("Nazwa").to_string(), a.name.clone()),
                (tr("Częstotliwość").to_string(), a.freq_text()),
                (tr("Deklinacja").to_string(), format!("{:+.1}°", a.mag_var)),
                (tr("Pozycja").to_string(), dms(a.lat, a.lon)),
            ];
            if let Some(e) = a.elev_ft {
                v.push((tr("Wysokość").to_string(), format!("{e} ft")));
            }
            if let Some(s) = snapshot.as_ref() {
                let r = bearing_deg(a.lat, a.lon, s.position.lat, s.position.lon) - a.mag_var;
                v.push((tr("Samolot na radialu").to_string(), format!("R-{} · {:.1} NM", deg3(r), distance_nm(a.lat, a.lon, s.position.lat, s.position.lon))));
            }
            if !a.airport.is_empty() {
                v.push((tr("Lotnisko").to_string(), a.airport.clone()));
            }
            return Some(v);
        }
    }
    if let Some(db) = airports::db() {
        if let Some((_, a)) = db.within(lat, lon, radius, false).into_iter().next() {
            let mut v = vec![(tr("Lotnisko").to_string(), format!("{} · {}", a.ident, a.name)), (tr("Rodzaj").to_string(), a.kind_name().to_string()), (tr("Pozycja").to_string(), dms(a.lat, a.lon))];
            if let Some(e) = a.elev_ft {
                v.push((tr("Wysokość").to_string(), format!("{e} ft")));
            }
            let rw: Vec<String> = a.runways.iter().filter(|r| !r.closed).take(4).map(|r| format!("{}/{} {} ft", r.le, r.he, r.length_ft)).collect();
            if !rw.is_empty() {
                v.push((tr("Pasy").to_string(), rw.join(", ")));
            }
            for f in a.freqs.iter().take(4) {
                v.push((f.kind.clone(), format!("{:.3}", f.mhz)));
            }
            if let Some(r) = rel(a.lat, a.lon) {
                v.push(r);
            }
            return Some(v);
        }
    }
    let mut v = vec![(tr("Punkt").to_string(), dms(lat, lon))];
    if let Some(r) = rel(lat, lon) {
        v.push(r);
    }
    Some(v)
}

// ---------- polecenia ze wstążki ----------

pub fn cmd(id: &str, arg: &str) -> bool {
    let bit = |name: &str| match name {
        "apt" => NAV_APT,
        "vor" => NAV_VOR,
        "ndb" => NAV_NDB,
        "vor-rose" => NAV_VOR_ROSE,
        "rose" => NAV_ROSE,
        "rings" => NAV_RINGS,
        "vector" => NAV_VECTOR,
        "track" => NAV_TRACK,
        "route" => NAV_ROUTE,
        "curtain" => NAV_CURTAIN,
        "rwy" => NAV_RWY,
        _ => 0,
    };
    match id {
        "nav-layer" => {
            let st = state();
            let mut s = st.borrow_mut();
            match arg {
                "plan" => s.s.nav_plan = !s.s.nav_plan,
                "shots" => s.s.nav_shots = !s.s.nav_shots,
                _ => s.s.nav_layers ^= bit(arg),
            }
            s.s.save();
        }
        "nav-3d" => {
            let st = state();
            let mut s = st.borrow_mut();
            s.s.nav_3d = arg == "1";
            s.s.save();
        }
        "nav-cam" => {
            let m: i32 = arg.parse().unwrap_or(0);
            state().borrow_mut().s.nav_cam = m;
            state().borrow().s.save();
            NAV.with(|n| {
                let mut n = n.borrow_mut();
                n.cam.mode = match m {
                    1 => CamMode::Free,
                    2 => CamMode::Top,
                    _ => CamMode::Chase,
                };
                n.cam.yaw = 0.0;
                if m == 2 {
                    n.cam.dist = n.cam.dist.max(8000.0);
                }
            });
        }
        "nav-tool" => {
            let t: i32 = arg.parse().unwrap_or(0);
            NAV.with(|n| {
                let mut n = n.borrow_mut();
                n.tool = if n.tool == t { 0 } else { t };
                n.measure.clear();
                if n.tool != 1 {
                    n.ref_pt = None;
                    n.sel_radial = None;
                }
            });
        }
        "nav-clear" => NAV.with(|n| {
            let mut n = n.borrow_mut();
            n.ref_pt = None;
            n.sel_radial = None;
            n.measure.clear();
            n.pick = None;
        }),
        "nav-inbound" => NAV.with(|n| {
            let mut n = n.borrow_mut();
            n.inbound = arg == "1";
        }),
        "nav-radial-step" => NAV.with(|n| {
            let mut n = n.borrow_mut();
            let d: i32 = arg.parse().unwrap_or(0);
            let base = n.sel_radial.unwrap_or(360);
            n.sel_radial = Some(radial_of((base + d) as f64));
        }),
        "nav-find" => {
            let q = arg.trim().to_uppercase();
            if q.is_empty() {
                return true;
            }
            let found: Option<(f64, f64, String, Option<f64>)> = navaids::db()
                .and_then(|db| {
                    let snapshot = state().borrow().host.snapshot();
                    let (la, lo) = snapshot.map(|s| (s.position.lat, s.position.lon)).unwrap_or_else(|| NAV.with(|n| (n.borrow().view.lat, n.borrow().view.lon)));
                    let mut v = db.find(&q);
                    v.sort_by(|a, b| distance_nm(la, lo, a.lat, a.lon).total_cmp(&distance_nm(la, lo, b.lat, b.lon)));
                    v.first().map(|a| (a.lat, a.lon, format!("{} {}", a.ident, a.kind.label()), Some(a.mag_var)))
                })
                .or_else(|| airports::db().and_then(|db| db.get(&q).map(|a| (a.lat, a.lon, a.ident.clone(), None))))
                .or_else(|| airports::db().and_then(|db| db.search(&q, 1).first().map(|a| (a.lat, a.lon, a.ident.clone(), None))))
                .or_else(|| navaids::db().and_then(|db| db.search(&q, 1).first().map(|a| (a.lat, a.lon, format!("{} {}", a.ident, a.kind.label()), Some(a.mag_var)))));
            match found {
                Some((lat, lon, name, var)) => {
                    let sim_var = mag_var(state().borrow().host.snapshot().as_ref());
                    NAV.with(|n| {
                        let mut n = n.borrow_mut();
                        n.view.lat = lat;
                        n.view.lon = lon;
                        n.view.zoom = n.view.zoom.max(9.0);
                        n.view.follow = false;
                        n.view.user_moved = true;
                        n.tool = 1;
                        n.ref_pt = Some(RefPt { lat, lon, name, var: var.unwrap_or(sim_var) });
                        n.sel_radial = None;
                    });
                    if let Some(u) = ui() {
                        u.global::<App>().set_follow(false);
                    }
                }
                None => app::toast(format!("{}: {q}", tr("Nie znaleziono"))),
            }
        }
        _ => return false,
    }
    render(true);
    true
}

// ---------- rysowanie ----------

pub fn render(force: bool) {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    if app.get_tab() != 9 {
        return;
    }
    let due = NAV.with(|n| n.borrow().last.elapsed().as_millis() > 180);
    if !force && !due {
        return;
    }
    NAV.with(|n| n.borrow_mut().last = Instant::now());
    let dark = u.global::<Theme>().get_dark();
    let (host, scale, set) = {
        let st = state();
        let s = st.borrow();
        (s.host.clone(), s.scale, s.s.clone())
    };
    let snap = host.snapshot();
    let lay = set.nav_layers;
    NAV.with(|n| {
        let mut n = n.borrow_mut();
        n.cam.mode = match set.nav_cam {
            1 => CamMode::Free,
            2 => CamMode::Top,
            _ => CamMode::Chase,
        };
        if let Some(s) = &snap {
            if n.view.follow {
                n.view.lat = s.position.lat;
                n.view.lon = s.position.lon;
                if !n.view.user_moved && n.view.zoom < 7.0 {
                    n.view.zoom = 9.0;
                }
            }
        }
    });
    let track = if lay & NAV_TRACK != 0 { host.live_track() } else { Vec::new() };
    let st_style = style(dark);
    let image = if set.nav_3d { render_3d(&snap, &track, st_style, dark, scale, lay) } else { render_2d(&snap, &track, st_style, dark, scale, lay) };
    app.set_map_nav(image.0);
    app.set_map_nav_labels(model(image.1.into_iter().map(|(x, y, t, k)| LabelV { x, y, text: ss(t), kind: k }).collect()));
    app.set_map_nav_scale(ss(if set.nav_3d { String::new() } else { NAV.with(|n| n.borrow().view.scale_text()) }));
    app.set_map_attribution(st_style.attribution().into());
    side_panel(&snap, set.nav_3d);
}

fn render_2d(snap: &Option<FlightState>, track: &[crate::host::LivePoint], style: Style, dark: bool, scale: f32, lay: u32) -> (slint::Image, Vec<(f32, f32, String, i32)>) {
    let v = NAV.with(|n| n.borrow().view.clone());
    let s = scale;
    let var = mag_var(snap.as_ref());
    let mut ov = Overlay::default();
    if track.len() > 1 {
        ov.tracks.push(Track { pts: track.iter().map(|p| (p.lat, p.lon, p.alt)).collect(), color: None, width: 3.0 });
    }
    if let Some(st) = snap {
        ov.aircraft = Some((st.position.lat, st.position.lon, st.attitude.hdg_true_deg));
        if lay & NAV_ROUTE != 0 {
            if let Some(av) = &st.avionics {
                if av.gps.next_lat != 0.0 && av.gps.prev_lat != 0.0 {
                    ov.route = vec![(av.gps.prev_lat, av.gps.prev_lon), (av.gps.next_lat, av.gps.next_lon)];
                }
            }
        }
    }
    let view_radius = (v.w.max(v.h) * 156_543.0 * v.lat.to_radians().cos() / 2f64.powf(v.zoom) / 1852.0).min(400.0);
    if let Some(db) = airports::db() {
        if lay & NAV_APT != 0 && v.zoom > 6.0 {
            for (_, a) in db.within(v.lat, v.lon, view_radius, false).into_iter().filter(|(_, a)| a.kind == 0 || (a.kind == 1 && v.zoom > 7.0) || v.zoom > 9.5).take(150) {
                ov.points.push(Point { lat: a.lat, lon: a.lon, label: if v.zoom > 7.5 || a.kind == 0 { a.ident.clone() } else { String::new() }, kind: if a.kind <= 1 { 0 } else { 3 } });
                if lay & NAV_RWY != 0 && v.zoom > 10.5 {
                    for r in a.runways.iter().filter(|r| r.has_coords() && !r.closed) {
                        ov.runways.push((r.le_lat, r.le_lon, r.he_lat, r.he_lon));
                    }
                }
            }
        }
    }
    // Plan lotu z SimBrief i miejsca zrzutów ekranu
    let (nav_plan, nav_shots) = {
        let st = state();
        let b = st.borrow();
        (b.s.nav_plan, b.s.nav_shots)
    };
    if nav_plan {
        let pts = crate::simbrief::route();
        if pts.len() > 1 {
            ov.tracks.push(Track { pts: pts.iter().map(|p| (p.0, p.1, p.2)).collect(), color: Some([79, 216, 235, 230]), width: 2.5 });
            if v.zoom > 5.0 {
                for p in &pts {
                    ov.points.push(Point { lat: p.0, lon: p.1, label: p.3.clone(), kind: 1 });
                }
            }
        }
    }
    if nav_shots {
        for (lat, lon, label) in crate::shots::points() {
            ov.points.push(Point { lat, lon, label, kind: 2 });
        }
    }
    let (pm, mut labels) = map::render_pm(&v, style, &ov, scale, dark);
    let mut c = Canvas::from_pixmap(pm);
    let to = |lat: f64, lon: f64| -> (f32, f32) {
        let (x, y) = v.to_screen(lat, lon);
        ((x * s as f64) as f32, (y * s as f64) as f32)
    };
    let px_per_nm = (1852.0 / (156_543.03 * v.lat.to_radians().cos() / 2f64.powf(v.zoom))) as f32 * s;
    let (wpx, hpx) = (c.width(), c.height());
    let ink = if dark { 0xFFE8EEF6 } else { 0xFF1A2230 };
    let halo = if dark { 0xC0000000 } else { 0xD0FFFFFF };

    // ---------- VOR / NDB ----------
    if lay & (NAV_VOR | NAV_NDB) != 0 && v.zoom > 5.5 {
        if let Some(db) = navaids::db() {
            let mut roses: Vec<(f32, f32, f32)> = Vec::new();
            for (_, a) in db.within(v.lat, v.lon, view_radius).into_iter().take(250) {
                let is_ndb = a.kind.is_ndb();
                if (is_ndb && lay & NAV_NDB == 0) || (!is_ndb && lay & NAV_VOR == 0) || (is_ndb && v.zoom < 7.5) {
                    continue;
                }
                let (x, y) = to(a.lat, a.lon);
                if x < -80.0 * s || y < -80.0 * s || x > wpx + 80.0 * s || y > hpx + 80.0 * s {
                    continue;
                }
                let col = if is_ndb { 0xFF9ACD32 } else if dark { 0xFF2EC4D6 } else { 0xFF0B7A8C };
                if is_ndb {
                    for k in 0..12 {
                        let a2 = (k as f32 * 30.0).to_radians();
                        c.circle(x + a2.cos() * 7.0 * s, y + a2.sin() * 7.0 * s, 1.1 * s, col, 0.0);
                    }
                    c.circle(x, y, 2.6 * s, col, 0.0);
                } else {
                    // róża VOR z radialami względem deklinacji stacji
                    let r = (52.0 * s).max((10.0 * px_per_nm).min(140.0 * s));
                    // róże nie nachodzą na siebie (pierwszeństwo bliższych środka mapy)
                    let free = roses.iter().all(|(rx, ry, rr)| ((rx - x).powi(2) + (ry - y).powi(2)).sqrt() > (rr + r) * 0.92);
                    if lay & NAV_VOR_ROSE != 0 && v.zoom > 7.0 && roses.len() < 14 && a.kind.is_vor() && free {
                        roses.push((x, y, r));
                        c.circle(x, y, r, alpha(col, 0x90), 1.2 * s);
                        for d in (0..360).step_by(10) {
                            let ang = (d as f64 + a.mag_var) as f32;
                            let len = if d % 30 == 0 { 0.16 } else { 0.07 };
                            c.radial(x, y, ang, r * (1.0 - len), r, alpha(col, 0xC0), if d % 30 == 0 { 1.6 } else { 1.0 } * s);
                            if d % 30 == 0 && r > 45.0 * s {
                                let t = ang.to_radians();
                                let lr = r * 0.74;
                                c.text_halo(&(d / 10).to_string(), x + t.sin() * lr, y - t.cos() * lr, 9.5 * s, col, halo, Align::Center, false);
                            }
                        }
                        // strzałka północy magnetycznej
                        let t = (a.mag_var as f32).to_radians();
                        let (tx, ty) = (x + t.sin() * r, y - t.cos() * r);
                        c.poly(col, 0, 0.0, &[tx + t.sin() * 9.0 * s, ty - t.cos() * 9.0 * s, tx + t.cos() * 4.0 * s, ty + t.sin() * 4.0 * s, tx - t.cos() * 4.0 * s, ty - t.sin() * 4.0 * s]);
                    }
                    let hex: Vec<f32> = (0..6).flat_map(|k| {
                        let a2 = (60.0 * k as f32).to_radians();
                        [x + 7.0 * s * a2.cos(), y + 7.0 * s * a2.sin()]
                    }).collect();
                    c.poly(alpha(if dark { 0xFF0E1116 } else { 0xFFFFFFFF }, 0xC0), col, 1.8 * s, &hex);
                    c.circle(x, y, 1.8 * s, col, 0.0);
                    if matches!(a.kind, navaids::Kind::VorDme | navaids::Kind::Vortac) {
                        c.frame(x - 9.5 * s, y - 8.0 * s, x + 9.5 * s, y + 8.0 * s, col, 1.2 * s);
                    }
                }
                if v.zoom > 7.0 {
                    labels.push((x / s + 6.0, y / s + 12.0, format!("{} {}", a.ident, a.freq_text().replace(" MHz", "").replace(" kHz", "")), 1));
                }
            }
        }
    }

    let (nav_tool, ref_pt, sel_radial, inbound, measure, hover) = NAV.with(|n| {
        let n = n.borrow();
        (n.tool, n.ref_pt.clone(), n.sel_radial, n.inbound, n.measure.clone(), n.hover)
    });

    // ---------- samolot: pierścienie, róża, wektor ----------
    if let Some(st) = snap {
        let (ax, ay) = to(st.position.lat, st.position.lon);
        let hdg_t = st.attitude.hdg_true_deg;
        let trk_t = st.attitude.track_true_deg;
        let gs = st.speed.gs_kt;
        if lay & NAV_RINGS != 0 {
            let target = wpx.min(hpx) / 8.0;
            let steps = [0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0];
            let step = steps.iter().copied().find(|st| st * px_per_nm >= target).unwrap_or(200.0);
            for k in 1..=4 {
                let r = step * k as f32 * px_per_nm;
                c.dash_circle(ax, ay, r, alpha(ink, 0x70), 1.2 * s, 6.0 * s, 5.0 * s);
                let lbl = if step < 1.0 { format!("{:.2} NM", step * k as f32) } else { format!("{} NM", step * k as f32) };
                c.text_halo(&lbl, ax + r * 0.7071 + 3.0 * s, ay + r * 0.7071, 10.0 * s, ink, halo, Align::Left, false);
            }
        }
        if lay & NAV_VECTOR != 0 && gs > 20.0 {
            // przewidywany tor: bieżąca prędkość nad ziemią i prędkość zakrętu
            let turn = st.avionics.as_ref().map(|a| a.turn_rate_dps).unwrap_or(0.0);
            let turn = if turn.abs() < 0.15 { 0.0 } else { turn };
            let (mut la, mut lo, mut t) = (st.position.lat, st.position.lon, trk_t);
            let mut pts = vec![(ax, ay)];
            let mut marks = Vec::new();
            for i in 1..=60 {
                t += turn * 5.0;
                let (a2, b2) = dest(la, lo, t, gs * 5.0 / 3600.0);
                la = a2;
                lo = b2;
                let p = to(la, lo);
                pts.push(p);
                if i == 12 || i == 24 || i == 60 {
                    marks.push((p, i / 12));
                }
            }
            let flat: Vec<f32> = pts.iter().flat_map(|(x, y)| [*x, *y]).collect();
            c.polyline(alpha(0xFF000000, 0x80), 4.0 * s, &flat);
            c.polyline(0xFF3CCF7A, 2.2 * s, &flat);
            for ((x, y), m) in marks {
                c.circle(x, y, 3.5 * s, 0xFF3CCF7A, 0.0);
                c.text_halo(&format!("{m}'"), x + 6.0 * s, y - 6.0 * s, 10.5 * s, 0xFF3CCF7A, halo, Align::Left, true);
            }
        }
        if lay & NAV_ROSE != 0 {
            let r = wpx.min(hpx) * 0.28;
            let rose_ink = if dark { 0xE6E8EEF6 } else { 0xE6202836 };
            c.circle(ax, ay, r, alpha(rose_ink, 0x60), 1.4 * s);
            for d in (0..360).step_by(5) {
                let ang = (d as f64 + var) as f32;
                let len = if d % 30 == 0 { 0.1 } else if d % 10 == 0 { 0.06 } else { 0.03 };
                c.radial(ax, ay, ang, r * (1.0 - len), r, rose_ink, if d % 10 == 0 { 1.6 } else { 1.0 } * s);
                if d % 30 == 0 {
                    let t = ang.to_radians();
                    let lr = r * 0.84;
                    let lbl = match d {
                        0 => "N".to_string(),
                        90 => "E".to_string(),
                        180 => "S".to_string(),
                        270 => "W".to_string(),
                        _ => (d / 10).to_string(),
                    };
                    c.text_halo(&lbl, ax + t.sin() * lr, ay - t.cos() * lr, 12.0 * s, if d == 0 { 0xFFFF5A5F } else { rose_ink }, halo, Align::Center, true);
                }
            }
            // kurs (żółta linia z okienkiem)
            c.radial(ax, ay, hdg_t as f32, 14.0 * s, r * 1.08, 0xFFFFD646, 2.2 * s);
            let t = (hdg_t as f32).to_radians();
            let (bx, by) = (ax + t.sin() * r * 1.17, ay - t.cos() * r * 1.17);
            let hdg_txt = format!("HDG {}", deg3(st.attitude.hdg_mag_deg));
            let bw = crate::avionics::gfx::text_width(&hdg_txt, 12.0 * s, crate::avionics::gfx::Face::CondBold) + 12.0 * s;
            c.round_rect(bx - bw / 2.0, by - 10.0 * s, bx + bw / 2.0, by + 10.0 * s, 4.0 * s, 0xE0101418, 0xFFFFD646, 1.2 * s);
            c.text_f(&hdg_txt, bx, by, 12.0 * s, 0xFFFFD646, Align::Center, crate::avionics::gfx::Face::CondBold);
            // tor (magentowy romb na obwodzie)
            if gs > 30.0 {
                c.save();
                c.rotate(trk_t as f32, ax, ay);
                c.poly(0xFFFF33E6, 0xFF000000, 1.0, &[ax, ay - r - 2.0 * s, ax + 5.0 * s, ay - r + 6.0 * s, ax, ay - r + 14.0 * s, ax - 5.0 * s, ay - r + 6.0 * s]);
                c.restore();
            }
            // nastawa kursu autopilota (cyjanowy bug)
            let bug = st.autopilot.hdg_deg;
            c.save();
            c.rotate((bug + var) as f32, ax, ay);
            c.poly(0xFF1FE6FF, 0xFF000000, 1.0, &[ax - 8.0 * s, ay - r - 6.0 * s, ax + 8.0 * s, ay - r - 6.0 * s, ax + 8.0 * s, ay - r + 2.0 * s, ax + 3.0 * s, ay - r + 2.0 * s, ax, ay - r + 6.0 * s, ax - 3.0 * s, ay - r + 2.0 * s, ax - 8.0 * s, ay - r + 2.0 * s]);
            c.restore();
            // namiar na następny punkt GPS
            if let Some(av) = &st.avionics {
                if av.gps.active_flight_plan && (av.gps.next_lat != 0.0 || av.gps.next_lon != 0.0) {
                    let b = bearing_deg(st.position.lat, st.position.lon, av.gps.next_lat, av.gps.next_lon) as f32;
                    c.save();
                    c.rotate(b, ax, ay);
                    c.poly(0, 0xFFFF33E6, 2.0 * s, &[ax, ay - r * 0.92, ax - 6.0 * s, ay - r * 0.78, ax + 6.0 * s, ay - r * 0.78]);
                    c.restore();
                }
            }
        }
    }

    // ---------- narzędzie: radiale ----------
    if nav_tool == 1 {
        if let Some(r) = &ref_pt {
            let (rx, ry) = to(r.lat, r.lon);
            let col = 0xFF2EC4D6;
            let rr = (wpx.min(hpx) * 0.22).max(90.0 * s);
            c.circle(rx, ry, rr, alpha(col, 0xA0), 1.4 * s);
            for d in (0..360).step_by(5) {
                let ang = (d as f64 + r.var) as f32;
                let len = if d % 30 == 0 { 0.12 } else if d % 10 == 0 { 0.07 } else { 0.035 };
                c.radial(rx, ry, ang, rr * (1.0 - len), rr, col, if d % 10 == 0 { 1.5 } else { 0.9 } * s);
                if d % 30 == 0 {
                    let t = ang.to_radians();
                    c.text_halo(&format!("{:03}", if d == 0 { 360 } else { d }), rx + t.sin() * rr * 0.82, ry - t.cos() * rr * 0.82, 10.5 * s, col, halo, Align::Center, true);
                }
            }
            c.circle(rx, ry, 5.0 * s, col, 0.0);
            c.text_halo(&r.name, rx, ry + rr * 0.25, 12.0 * s, col, halo, Align::Center, true);
            let o = snap.as_ref().map(|s| (s.position.lat, s.position.lon)).or(hover);
            if let Some((la, lo)) = o {
                let (px, py) = to(la, lo);
                let rad = bearing_deg(r.lat, r.lon, la, lo) - r.var;
                let dist = distance_nm(r.lat, r.lon, la, lo);
                c.dash_line(rx, ry, px, py, ink, 1.6 * s, 7.0 * s, 5.0 * s);
                let (mx, my) = ((rx + px) / 2.0, (ry + py) / 2.0);
                c.text_halo(&format!("R-{} · {:.1} NM", deg3(rad), dist), mx, my - 10.0 * s, 12.0 * s, ink, halo, Align::Center, true);
            }
            if let Some(sel) = sel_radial {
                let brg = sel as f64 + r.var;
                let len_nm = o.map(|(la, lo)| distance_nm(r.lat, r.lon, la, lo) * 1.6).unwrap_or(30.0).max(15.0) as f64;
                let (ela, elo) = dest(r.lat, r.lon, brg, len_nm);
                let (ex, ey) = to(ela, elo);
                c.line(rx, ry, ex, ey, 0xFFFF33E6, 2.6 * s);
                // grotki kierunku lotu po radialu
                for k in 1..=4 {
                    let f = k as f32 / 5.0;
                    let (gx, gy) = (rx + (ex - rx) * f, ry + (ey - ry) * f);
                    let ang = if inbound { brg + 180.0 } else { brg } as f32;
                    c.save();
                    c.rotate(ang, gx, gy);
                    c.poly(0xFFFF33E6, 0, 0.0, &[gx, gy - 7.0 * s, gx - 5.0 * s, gy + 4.0 * s, gx + 5.0 * s, gy + 4.0 * s]);
                    c.restore();
                }
                let course = if inbound { norm360(sel as f64 + 180.0) } else { sel as f64 };
                let (lx, ly) = (rx + (ex - rx) * 0.55, ry + (ey - ry) * 0.55);
                c.text_halo(&format!("R-{} {} · {}°", deg3(sel as f64), if inbound { "IN" } else { "OUT" }, deg3(course)), lx + 8.0 * s, ly, 13.0 * s, 0xFFFF33E6, halo, Align::Left, true);
            }
        }
    }

    // ---------- narzędzie: pomiar ----------
    if nav_tool == 2 && !measure.is_empty() {
        let a = measure[0];
        let b = if measure.len() > 1 { Some(measure[1]) } else { hover };
        let (ax, ay) = to(a.0, a.1);
        c.circle(ax, ay, 4.0 * s, 0xFFFFB547, 0.0);
        if let Some(b) = b {
            let (bx, by) = to(b.0, b.1);
            c.line(ax, ay, bx, by, 0xFF000000, 4.0 * s);
            c.line(ax, ay, bx, by, 0xFFFFB547, 2.2 * s);
            c.circle(bx, by, 4.0 * s, 0xFFFFB547, 0.0);
            let d = distance_nm(a.0, a.1, b.0, b.1);
            let brg = bearing_deg(a.0, a.1, b.0, b.1);
            c.text_halo(&format!("{:.1} NM · {}°M", d, deg3(brg - var)), (ax + bx) / 2.0 + 8.0 * s, (ay + by) / 2.0, 13.0 * s, 0xFFFFB547, halo, Align::Left, true);
        }
    }

    // ---------- wiatr i północ ----------
    if let Some(st) = snap {
        let (wx, wy) = (36.0 * s, 36.0 * s);
        c.circle(wx, wy, 26.0 * s, 0xC0101418, 0.0);
        c.circle(wx, wy, 26.0 * s, 0x50FFFFFF, 1.0);
        if st.environment.wind_spd_kt >= 1.0 {
            c.save();
            c.rotate((st.environment.wind_dir_deg + 180.0) as f32, wx, wy);
            c.line(wx, wy + 16.0 * s, wx, wy - 12.0 * s, 0xFFFFFFFF, 2.5 * s);
            c.poly(0xFFFFFFFF, 0, 0.0, &[wx, wy - 18.0 * s, wx - 6.0 * s, wy - 8.0 * s, wx + 6.0 * s, wy - 8.0 * s]);
            c.restore();
        }
        c.text_halo(&format!("{}°/{:.0} kt", deg3(st.environment.wind_dir_deg - var), st.environment.wind_spd_kt), wx, wy + 38.0 * s, 11.0 * s, ink, halo, Align::Center, true);
    }
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(c.pm.data(), c.pm.width(), c.pm.height());
    (slint::Image::from_rgba8_premultiplied(buf), labels)
}

fn render_3d(snap: &Option<FlightState>, track: &[crate::host::LivePoint], style: Style, dark: bool, scale: f32, lay: u32) -> (slint::Image, Vec<(f32, f32, String, i32)>) {
    let (v, cam) = NAV.with(|n| (n.borrow().view.clone(), n.borrow().cam));
    let w = ((v.w * scale as f64) as u32).max(16);
    let h = ((v.h * scale as f64) as u32).max(16);
    let (lat, lon, alt, hdg, pitch, bank, ground, show) = match snap {
        Some(s) => (s.position.lat, s.position.lon, s.position.alt_msl_ft, s.attitude.hdg_true_deg, s.attitude.pitch_deg, s.attitude.bank_deg, (s.position.alt_msl_ft - s.position.alt_agl_ft).max(-1000.0), true),
        None => (v.lat, v.lon, 3000.0, 0.0, 0.0, 0.0, 0.0, false),
    };
    let radius = (cam.dist / 1852.0 * 14.0).clamp(8.0, 160.0);
    let mut points = Vec::new();
    let mut runways = Vec::new();
    if let Some(db) = airports::db() {
        if lay & NAV_APT != 0 {
            for (_, a) in db.within(lat, lon, radius, false).into_iter().filter(|(_, a)| a.kind <= 1 || cam.dist < 15_000.0).take(80) {
                points.push(Label3 { lat: a.lat, lon: a.lon, text: a.ident.clone(), kind: 0 });
            }
        }
        if lay & NAV_RWY != 0 {
            for (_, a) in db.within(lat, lon, radius.min(60.0), false).into_iter().take(60) {
                for r in a.runways.iter().filter(|r| r.has_coords() && !r.closed) {
                    runways.push((r.le_lat, r.le_lon, r.he_lat, r.he_lon, r.width_ft.max(60) as f64));
                }
            }
        }
    }
    if let Some(db) = navaids::db() {
        for (_, a) in db.within(lat, lon, radius).into_iter().take(80) {
            let ndb = a.kind.is_ndb();
            if (ndb && lay & NAV_NDB != 0) || (!ndb && lay & NAV_VOR != 0) {
                points.push(Label3 { lat: a.lat, lon: a.lon, text: a.ident.clone(), kind: if ndb { 2 } else { 1 } });
            }
        }
    }
    let route: Vec<(f64, f64)> = snap
        .as_ref()
        .and_then(|s| s.avionics.as_ref())
        .filter(|a| lay & NAV_ROUTE != 0 && a.gps.next_lat != 0.0 && a.gps.prev_lat != 0.0)
        .map(|a| vec![(a.gps.prev_lat, a.gps.prev_lon), (a.gps.next_lat, a.gps.next_lon)])
        .unwrap_or_default();
    let pts: Vec<(f64, f64, i32)> = track.iter().map(|p| (p.lat, p.lon, p.alt)).collect();
    let hud = match snap {
        Some(s) => vec![
            if s.aircraft.title.is_empty() { "OpenTakeoff 3D".to_string() } else { s.aircraft.title.clone() },
            format!("ALT {:.0} ft · AGL {:.0} ft", s.position.alt_ind_ft, s.position.alt_agl_ft),
            format!("HDG {}° · GS {:.0} kt · V/S {:+.0}", deg3(s.attitude.hdg_mag_deg), s.speed.gs_kt, s.speed.vs_fpm),
        ],
        None => vec![tr("Brak danych z symulatora – widok mapy").to_string()],
    };
    let sc = scene3d::Scene {
        style,
        dark,
        w,
        h,
        scale,
        lat,
        lon,
        alt_ft: alt,
        hdg,
        pitch,
        bank,
        ground_ft: ground,
        mag_var: mag_var(snap.as_ref()),
        show_aircraft: show,
        track: &pts,
        runways: &runways,
        points: &points,
        route: &route,
        hud,
        compass: lay & NAV_ROSE != 0,
        curtain: lay & NAV_CURTAIN != 0,
    };
    (scene3d::render(&sc, &cam), Vec::new())
}

// ---------- panel boczny ----------

fn side_hover() {
    let Some(u) = ui() else { return };
    let snap = state().borrow().host.snapshot();
    let txt = NAV.with(|n| {
        let n = n.borrow();
        n.hover.map(|(la, lo)| {
            let mut t = dms(la, lo);
            if let Some(s) = &snap {
                let var = mag_var(Some(s));
                t += &format!("\n{}°M · {:.1} NM {}", deg3(bearing_deg(s.position.lat, s.position.lon, la, lo) - var), distance_nm(s.position.lat, s.position.lon, la, lo), tr("od samolotu"));
            }
            t
        })
    });
    u.global::<App>().set_nav_hover(ss(txt.unwrap_or_default()));
}

fn side_panel(snap: &Option<FlightState>, three_d: bool) {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    let set = state().borrow().s.clone();
    let units = crate::fmt::Units { meters: set.units_alt == 1, kmh: set.units_speed == 1 };
    let var = mag_var(snap.as_ref());
    let mut info: Vec<KV> = Vec::new();
    let kv = |k: &str, v: String| KV { k: ss(k), v: ss(v) };
    if let Some(s) = snap {
        info.push(kv(&tr("Pozycja"), dms(s.position.lat, s.position.lon)));
        info.push(kv(&tr("Wysokość"), format!("{} · AGL {}", units.alt_full(s.position.alt_ind_ft), units.alt_full(s.position.alt_agl_ft))));
        info.push(kv(&tr("Prędkość"), format!("GS {} · TAS {}", units.spd(s.speed.gs_kt), units.spd(s.speed.tas_kt))));
        info.push(kv(&tr("Kurs / ścieżka"), format!("{}°M / {}°M", deg3(s.attitude.hdg_mag_deg), deg3(s.attitude.track_true_deg - var))));
        info.push(kv(&tr("Wiatr"), format!("{}° / {:.0} kt", deg3(s.environment.wind_dir_deg - var), s.environment.wind_spd_kt)));
        info.push(kv(&tr("Deklinacja"), format!("{:+.1}°", var)));
    }
    app.set_nav_info(model(info));

    let (la, lo, has_ac) = NAV.with(|n| origin(&n.borrow(), snap.as_ref()));
    let row = |name: String, a: String, b: String, level: f32, id: String| RowV { name: ss(name), a: ss(a), b: ss(b), level, id: ss(id) };
    let apts: Vec<RowV> = airports::db()
        .map(|db| {
            db.within(la, lo, 80.0, false)
                .into_iter()
                .filter(|(_, a)| a.kind <= 2)
                .take(7)
                .map(|(d, a)| row(format!("{} {}", a.ident, a.name), format!("{}° · {:.1} NM", deg3(bearing_deg(la, lo, a.lat, a.lon) - var), d), a.runways.first().map(|r| format!("RWY {}/{} · {} ft", r.le, r.he, r.length_ft)).unwrap_or_default(), (1.0 - d / 80.0) as f32, a.ident.clone()))
                .collect()
        })
        .unwrap_or_default();
    app.set_nav_airports(model(apts));
    let vors: Vec<RowV> = navaids::db()
        .map(|db| {
            db.within(la, lo, 120.0)
                .into_iter()
                .filter(|(_, a)| a.kind.is_vor() || a.kind.is_ndb())
                .take(7)
                .map(|(d, a)| {
                    let radial = bearing_deg(a.lat, a.lon, la, lo) - a.mag_var;
                    row(format!("{} {} · {}", a.ident, a.kind.label(), a.freq_text()), if a.kind.is_vor() { format!("R-{} · {:.1} NM", deg3(radial), d) } else { format!("{}° · {:.1} NM", deg3(bearing_deg(la, lo, a.lat, a.lon) - var), d) }, a.name.clone(), (1.0 - d / 120.0) as f32, a.ident.clone())
                })
                .collect()
        })
        .unwrap_or_default();
    app.set_nav_vors(model(vors));

    // karta narzędzia
    let (tool, title, rows) = NAV.with(|n| {
        let n = n.borrow();
        let mut rows: Vec<(String, String)> = Vec::new();
        let mut title = String::new();
        match n.tool {
            1 => {
                title = tr("Radiale").to_string();
                match &n.ref_pt {
                    None => rows.push((tr("Krok 1").to_string(), tr("Kliknij VOR, lotnisko albo dowolny punkt na mapie").to_string())),
                    Some(r) => {
                        rows.push((tr("Punkt").to_string(), r.name.clone()));
                        rows.push((tr("Deklinacja").to_string(), format!("{:+.1}°", r.var)));
                        let o = if has_ac { Some((la, lo)) } else { n.hover };
                        if let Some((pla, plo)) = o {
                            let rad = bearing_deg(r.lat, r.lon, pla, plo) - r.var;
                            let to = bearing_deg(pla, plo, r.lat, r.lon) - r.var;
                            let dist = distance_nm(r.lat, r.lon, pla, plo);
                            rows.push((if has_ac { tr("Samolot na radialu") } else { tr("Kursor na radialu") }.to_string(), format!("R-{} · {:.1} NM", deg3(rad), dist)));
                            rows.push((tr("Kurs do punktu").to_string(), format!("{}°", deg3(to))));
                            if let (Some(sel), Some(s)) = (n.sel_radial, snap.as_ref()) {
                                let off = norm180(rad - sel as f64).to_radians();
                                let cw = dist * off.sin();
                                let out = dist * off.cos();
                                let course = if n.inbound { norm360(sel as f64 + 180.0) } else { sel as f64 };
                                let xtk = if n.inbound { -cw } else { cw };
                                let behind = out < -0.3;
                                let intercept = if behind { 0.0 } else { (xtk.abs() * 30.0).min(45.0) };
                                let track = if behind { to } else { norm360(course - xtk.signum() * intercept) };
                                let tas = s.speed.tas_kt;
                                let wfrom = s.environment.wind_dir_deg - var;
                                let wca = if tas > 40.0 { ((s.environment.wind_spd_kt * (wfrom - track).to_radians().sin()) / tas).clamp(-1.0, 1.0).asin().to_degrees() } else { 0.0 };
                                rows.push((tr("Wybrany radial").to_string(), format!("R-{} {}", deg3(sel as f64), if n.inbound { "IN" } else { "OUT" })));
                                rows.push((tr("Kurs po radialu").to_string(), format!("{}°", deg3(course))));
                                rows.push((tr("Odchylenie boczne").to_string(), format!("{:.2} NM {}", xtk.abs(), if xtk > 0.0 { tr("w prawo") } else { tr("w lewo") })));
                                if behind {
                                    rows.push((tr("Uwaga").to_string(), tr("Samolot po złej stronie punktu – leć najpierw do punktu").to_string()));
                                } else if xtk.abs() < 0.3 {
                                    rows.push((tr("Stan").to_string(), tr("Na radialu").to_string()));
                                }
                                rows.push((tr("Kąt przechwycenia").to_string(), format!("{intercept:.0}°")));
                                rows.push((tr("Ścieżka do przechwycenia").to_string(), format!("{}°", deg3(track))));
                                rows.push((tr("Kurs z poprawką na wiatr").to_string(), format!("{}° (WCA {:+.0}°)", deg3(track + wca), wca)));
                            } else if n.sel_radial.is_none() {
                                rows.push((tr("Krok 2").to_string(), tr("Kliknij na mapie, aby wybrać radial").to_string()));
                            } else if let Some(sel) = n.sel_radial {
                                rows.push((tr("Wybrany radial").to_string(), format!("R-{} {}", deg3(sel as f64), if n.inbound { "IN" } else { "OUT" })));
                            }
                        }
                    }
                }
            }
            2 => {
                title = tr("Pomiar").to_string();
                match n.measure.len() {
                    0 => rows.push((tr("Krok 1").to_string(), tr("Kliknij punkt początkowy").to_string())),
                    _ => {
                        let a = n.measure[0];
                        rows.push((tr("Od").to_string(), dms(a.0, a.1)));
                        if let Some(b) = n.measure.get(1).copied().or(n.hover) {
                            let d = distance_nm(a.0, a.1, b.0, b.1);
                            let brg = bearing_deg(a.0, a.1, b.0, b.1);
                            rows.push((tr("Do").to_string(), dms(b.0, b.1)));
                            rows.push((tr("Odległość").to_string(), format!("{:.2} NM · {:.1} km", d, d * 1.852)));
                            rows.push((tr("Namiar").to_string(), format!("{}°M ({}°T)", deg3(brg - var), deg3(brg))));
                            rows.push((tr("Namiar odwrotny").to_string(), format!("{}°M", deg3(brg + 180.0 - var))));
                            if let Some(s) = snap.as_ref().filter(|s| s.speed.gs_kt > 30.0) {
                                rows.push((tr("Czas przy obecnej GS").to_string(), crate::fmt::ete(d / s.speed.gs_kt * 3600.0)));
                            }
                        }
                    }
                }
            }
            _ => {
                if let Some(p) = &n.pick {
                    title = tr("Wskazany obiekt").to_string();
                    rows = p.clone();
                }
            }
        }
        (n.tool, title, rows)
    });
    app.set_nav_tool_title(ss(title));
    app.set_nav_tool(model(rows.into_iter().map(|(k, v)| KV { k: ss(k), v: ss(v) }).collect()));
    let lay = set.nav_layers;
    let (inbound, has_ref, has_sel) = NAV.with(|n| {
        let n = n.borrow();
        (n.inbound, n.ref_pt.is_some(), n.sel_radial.is_some())
    });
    app.set_nav(NavV {
        apt: lay & NAV_APT != 0,
        vor: lay & NAV_VOR != 0,
        ndb: lay & NAV_NDB != 0,
        vor_rose: lay & NAV_VOR_ROSE != 0,
        rose: lay & NAV_ROSE != 0,
        rings: lay & NAV_RINGS != 0,
        vector: lay & NAV_VECTOR != 0,
        track: lay & NAV_TRACK != 0,
        route: lay & NAV_ROUTE != 0,
        curtain: lay & NAV_CURTAIN != 0,
        rwy: lay & NAV_RWY != 0,
        tool,
        view3d: three_d,
        cam: set.nav_cam,
        inbound,
        has_ref,
        has_sel,
        has_data: snap.is_some(),
        plan: set.nav_plan,
        shots: set.nav_shots,
    });
}

/// Dopasowuje widok do punktów (trasa planu) – bez śledzenia samolotu.
pub fn fit_points(pts: &[(f64, f64)]) {
    if let Some(b) = map::bounds(pts.iter().copied()) {
        NAV.with(|n| {
            let mut n = n.borrow_mut();
            n.view.fit(b);
            n.view.follow = false;
            n.view.user_moved = true;
        });
        if let Some(u) = ui() {
            u.global::<App>().set_follow(false);
        }
    }
}

/// Pokazuje miejsce na mapie (zrzut ekranu, punkt planu) – bez śledzenia samolotu.
pub fn show_at(lat: f64, lon: f64) {
    NAV.with(|n| {
        let mut n = n.borrow_mut();
        n.view.lat = lat;
        n.view.lon = lon;
        n.view.zoom = n.view.zoom.max(11.0);
        n.view.follow = false;
        n.view.user_moved = true;
    });
    if let Some(u) = ui() {
        u.global::<App>().set_follow(false);
    }
}

/// Etykiety mapy z obrazu (nieużywane poza 2D).
#[allow(dead_code)]
pub fn reset() {
    NAV.with(|n| n.borrow_mut().view.follow = true);
}
