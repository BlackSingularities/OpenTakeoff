//! Briefing lotniska w formie wizualizacji (jak na tabletach EFB): diagram pasów z wiatrem, profil nieba
//! (chmury, podstawa, poziom zamarzania, opady, widzialność), oś czasu TAF z kategoriami lotu
//! oraz kafelki z najważniejszymi wartościami i trend z ostatnich METAR-ów.

use std::cell::RefCell;
use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate, Timelike, Utc};
use opentakeoff_core::aero::airports::Airport;
use opentakeoff_core::aero::weather::{flight_category, Metar, Taf};
use opentakeoff_core::aero::{calc, metar as metar_dec};
use opentakeoff_core::lang::tr;
use opentakeoff_core::trf;
use regex::Regex;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::app::{state, ui};
use crate::avionics::gfx::*;
use crate::*;

pub const WIND: i32 = 30;
pub const SKY: i32 = 31;
pub const TAF: i32 = 32;

const BG: Argb = 0xFF0B1016;
const DISC: Argb = 0xFF101820;
const DIM: Argb = 0xFF6F7C8A;
const FAINT: Argb = 0xFF3A4654;
const CYAN: Argb = 0xFF4FD8EB;
const GREEN: Argb = 0xFF3CCF7A;

struct Data {
    apt: Airport,
    metars: Vec<Metar>,
    taf: Option<Taf>,
}

thread_local! {
    static SIZES: RefCell<HashMap<i32, (f32, f32)>> = RefCell::new(HashMap::new());
    static DATA: RefCell<Option<Data>> = const { RefCell::new(None) };
}

fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

pub fn cat_color(c: &str) -> Argb {
    match c {
        "VFR" => 0xFF3CCF7A,
        "MVFR" => 0xFF4F9DFF,
        "IFR" => 0xFFFF5A5F,
        "LIFR" => 0xFFD55CFF,
        _ => DIM,
    }
}

/// Kategoria lotu z widzialności (m) i podstawy chmur (ft).
fn category(vis_m: f64, ceiling_ft: Option<i32>) -> &'static str {
    let vis = vis_m / 1609.34;
    let ceil = ceiling_ft.unwrap_or(99_999);
    if vis < 1.0 || ceil < 500 {
        "LIFR"
    } else if vis < 3.0 || ceil < 1000 {
        "IFR"
    } else if vis <= 5.0 || ceil <= 3000 {
        "MVFR"
    } else {
        "VFR"
    }
}

fn okta(cover: &str) -> u32 {
    match cover {
        "FEW" => 2,
        "SCT" => 4,
        "BKN" => 6,
        "OVC" | "OVX" => 8,
        "VV" => 8,
        _ => 0,
    }
}

fn vis_m(m: &Metar) -> Option<f64> {
    if m.raw.split_whitespace().any(|t| t == "CAVOK") {
        return Some(10_000.0);
    }
    // METAR europejski: 4 cyfry w metrach (dokładniej niż „6+ SM” z API)
    if let Some(t) = m.raw.split_whitespace().skip(2).find(|t| t.len() == 4 && t.chars().all(|c| c.is_ascii_digit())) {
        let v: f64 = t.parse().ok()?;
        return Some(if v >= 9999.0 { 10_000.0 } else { v });
    }
    m.visibility.as_deref().and_then(metar_dec::parse_sm).map(|sm| (sm * 1609.34).min(10_000.0) + if m.visibility.as_deref().is_some_and(|v| v.contains('+')) { 1.0 } else { 0.0 })
}

fn vis_text(v: f64) -> String {
    if v >= 9999.0 {
        "≥10 km".into()
    } else if v >= 5000.0 {
        format!("{:.0} km", v / 1000.0)
    } else {
        format!("{v:.0} m")
    }
}

fn ceiling(m: &Metar) -> Option<i32> {
    m.clouds.iter().filter(|c| matches!(c.cover.as_str(), "BKN" | "OVC" | "OVX" | "VV")).filter_map(|c| c.base_ft).min()
}

fn variable_sector(raw: &str) -> Option<(f32, f32)> {
    let re = Regex::new(r"^(\d{3})V(\d{3})$").ok()?;
    raw.split_whitespace().find_map(|t| re.captures(t).map(|c| (c[1].parse().unwrap_or(0.0), c[2].parse().unwrap_or(0.0))))
}

fn wx_tokens(m: &Metar) -> String {
    m.wx.clone().unwrap_or_default()
}

pub fn size(id: i32, w: f32, h: f32) {
    let changed = SIZES.with(|s| {
        let mut s = s.borrow_mut();
        let old = s.insert(id, (w, h));
        old.is_none_or(|(a, b)| (a - w).abs() > 0.5 || (b - h).abs() > 0.5)
    });
    if changed {
        render(id);
    }
}

fn px(id: i32) -> Option<(u32, u32, f32)> {
    let scale = state().borrow().scale.max(1.0);
    SIZES.with(|s| s.borrow().get(&id).copied()).filter(|(w, h)| *w > 8.0 && *h > 8.0).map(|(w, h)| ((w * scale) as u32, (h * scale) as u32, scale))
}

/// Nowe lotnisko albo nowa pogoda: zapamiętaj i narysuj wszystko.
pub fn set(apt: &Airport, metars: &[Metar], taf: &Option<Taf>) {
    DATA.with(|d| *d.borrow_mut() = Some(Data { apt: apt.clone(), metars: metars.to_vec(), taf: taf.clone() }));
    publish();
    for id in [WIND, SKY, TAF] {
        render(id);
    }
}

fn render(id: i32) {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    let Some((w, h, k)) = px(id) else { return };
    DATA.with(|d| {
        let d = d.borrow();
        let Some(d) = d.as_ref() else { return };
        let mut c = Canvas::new(w, h);
        let (wf, hf) = (w as f32, h as f32);
        let m = d.metars.first();
        match id {
            WIND => {
                draw_wind(&mut c, &d.apt, m, wf, hf, k);
                app.set_brief_wind(c.into_image());
            }
            SKY => {
                draw_sky(&mut c, &d.apt, m, wf, hf, k);
                app.set_brief_sky(c.into_image());
            }
            TAF => {
                let periods = d.taf.as_ref().and_then(|t| parse_taf(&t.raw));
                draw_taf(&mut c, periods.as_ref(), wf, hf, k);
                app.set_brief_taf(c.into_image());
            }
            _ => {}
        }
    });
}

// ---------- kafelki, trend, składowe ----------

fn publish() {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    DATA.with(|d| {
        let d = d.borrow();
        let Some(d) = d.as_ref() else { return };
        let m = d.metars.first();
        let mut tiles = Vec::new();
        let tile = |label: &str, value: String, sub: String, tone: i32, cat: &str| BriefTileV { label: ss(label), value: ss(value), sub: ss(sub), tone, cat: ss(cat) };
        if let Some(m) = m {
            let cat = flight_category(m);
            tiles.push(tile(tr("Kategoria"), cat.clone(), match cat.as_str() {
                "VFR" => tr("lot z widocznością").to_string(),
                "MVFR" => tr("warunki pogorszone").to_string(),
                "IFR" => tr("lot według przyrządów").to_string(),
                _ => tr("bardzo złe warunki").to_string(),
            }, 0, &cat));
            let (wv, ws) = match (m.wind_variable, m.wind_dir, m.wind_kt) {
                (_, _, Some(0)) => (tr("Cisza").to_string(), "00000KT".to_string()),
                (true, _, Some(s)) => (format!("VRB {s} kt"), tr("kierunek zmienny").to_string()),
                (_, Some(dir), Some(s)) => (
                    format!("{dir:03}° / {s} kt"),
                    match (m.gust_kt, variable_sector(&m.raw)) {
                        (Some(g), _) => trf!("porywy do {g} kt"),
                        (None, Some((a, b))) => trf!("zmienny {a:03.0}°–{b:03.0}°"),
                        _ => tr("stały kierunek").to_string(),
                    },
                ),
                _ => ("—".into(), String::new()),
            };
            tiles.push(tile(tr("Wiatr"), wv, ws, if m.gust_kt.unwrap_or(0) >= 25 || m.wind_kt.unwrap_or(0) >= 20 { 2 } else { 0 }, ""));
            let v = vis_m(m);
            tiles.push(tile(tr("Widzialność"), v.map(vis_text).unwrap_or("—".into()), m.visibility.clone().map(|s| format!("{s} SM")).unwrap_or_default(), if v.is_some_and(|v| v < 5000.0) { 2 } else { 0 }, ""));
            let ceil = ceiling(m);
            let low = m.clouds.iter().filter_map(|c| Some(format!("{}{:03}", c.cover, c.base_ft? / 100))).collect::<Vec<_>>().join(" ");
            tiles.push(tile(
                tr("Podstawa chmur"),
                ceil.map(|c| format!("{c} ft")).unwrap_or_else(|| tr("brak pułapu").to_string()),
                if m.raw.contains("CAVOK") { "CAVOK".into() } else if low.is_empty() { tr("bez znaczących chmur").to_string() } else { low },
                if ceil.is_some_and(|c| c < 1000) { 3 } else if ceil.is_some_and(|c| c <= 3000) { 2 } else { 0 },
                "",
            ));
            if let Some(t) = m.temp_c {
                let dp = m.dewpoint_c.unwrap_or(t);
                let spread = t - dp;
                tiles.push(tile(
                    tr("Temperatura"),
                    format!("{t:.0} °C"),
                    if spread <= 2.0 { trf!("punkt rosy {dp:.0} °C · ryzyko mgły") } else { trf!("punkt rosy {dp:.0} °C · różnica {spread:.0}°") },
                    if spread <= 2.0 { 2 } else { 0 },
                    "",
                ));
            }
            if let Some(q) = m.altim_hpa {
                let trend = d.metars.last().filter(|o| o.obs_time < m.obs_time).and_then(|o| Some((q - o.altim_hpa?, (m.obs_time - o.obs_time) as f64 / 3600.0)));
                let sub = match trend {
                    Some((dq, h)) if dq.abs() >= 0.5 => format!("{} {:+.0} hPa / {:.0} h", if dq > 0.0 { "▲" } else { "▼" }, dq, h),
                    Some((_, h)) => trf!("stałe od {h:.0} h"),
                    None => format!("{:.2} inHg", q * 0.02953),
                };
                tiles.push(tile("QNH", format!("{q:.0} hPa"), sub, if trend.is_some_and(|(dq, _)| dq <= -3.0) { 2 } else { 0 }, ""));
                if let (Some(e), Some(t)) = (d.apt.elev_ft, m.temp_c) {
                    let pa = calc::pressure_altitude(e as f64, q);
                    let da = pa + 120.0 * (t - (15.0 - 2.0 * pa / 1000.0));
                    tiles.push(tile(tr("Wysokość gęstościowa"), format!("{:.0} ft", da), trf!("ciśnieniowa {pa:.0} ft"), if da - e as f64 > 2000.0 { 2 } else { 0 }, ""));
                }
            }
        }
        app.set_brief_tiles(model(tiles));

        // Trend: od najstarszej do najnowszej depeszy; sześć ostatnich mieści się obok składowych wiatru
        // także w oknie 1920 px (osiem wychodziło poza briefing)
        let hist: Vec<WxHistV> = d
            .metars
            .iter()
            .take(6)
            .rev()
            .map(|m| {
                let t = chrono::DateTime::from_timestamp(m.obs_time, 0).map(|t| t.format("%H:%MZ").to_string()).unwrap_or_default();
                WxHistV {
                    time: ss(t),
                    cat: ss(flight_category(m)),
                    dir: m.wind_dir.unwrap_or(0) as f32,
                    has_dir: m.wind_dir.is_some() && m.wind_kt.unwrap_or(0) > 0,
                    wind: ss(match (m.wind_dir, m.wind_kt) {
                        (_, Some(0)) => tr("cisza").to_string(),
                        (Some(d), Some(s)) => format!("{d:03}/{s}{}", m.gust_kt.map(|g| format!("G{g}")).unwrap_or_default()),
                        (None, Some(s)) => format!("VRB/{s}"),
                        _ => "—".into(),
                    }),
                    vis: ss(vis_m(m).map(vis_text).unwrap_or_default()),
                    temp: ss(m.temp_c.map(|t| format!("{t:.0}°")).unwrap_or_default()),
                    qnh: ss(m.altim_hpa.map(|q| format!("{q:.0}")).unwrap_or_default()),
                    ceiling: ss(ceiling(m).map(|c| format!("{c} ft")).unwrap_or_else(|| "—".into())),
                }
            })
            .collect();
        app.set_brief_history(model(hist));
        let lanes = d.taf.as_ref().and_then(|t| parse_taf(&t.raw)).map(|p| lanes(&p.overlays).1).unwrap_or(0);
        app.set_brief_taf_lanes(lanes as i32);
    });
}

// ---------- diagram pasów z wiatrem ----------

fn draw_wind(c: &mut Canvas, a: &Airport, m: Option<&Metar>, w: f32, h: f32, k: f32) {
    c.rect(0.0, 0.0, w, h, BG);
    // u góry miejsce na podpisy wiatru i pasa, na dole na opis
    let (cx, cy) = (w / 2.0, 54.0 * k + (h - 74.0 * k) / 2.0);
    let r = (w.min(h - 74.0 * k) * 0.5 - 16.0 * k).max(20.0 * k);
    if let Some(p) = circle_path(cx, cy, r) {
        c.fill_radial(&p, cx, cy, r, 0xFF16212C, DISC);
    }
    c.circle(cx, cy, r, FAINT, 1.2 * k);
    // róża kursów co 5°, opisy co 30°
    for i in 0..72 {
        let a = i as f32 * 5.0;
        let (r1, col, lw) = if i % 6 == 0 { (r * 0.90, WHITE, 1.8 * k) } else if i % 2 == 0 { (r * 0.94, DIM, 1.3 * k) } else { (r * 0.97, FAINT, 1.0 * k) };
        c.radial(cx, cy, a, r1, r, col, lw);
    }
    for i in 0..12 {
        let a = i as f32 * 30.0;
        let t = a.to_radians();
        let lbl = match i {
            0 => "N".to_string(),
            3 => "E".to_string(),
            6 => "S".to_string(),
            9 => "W".to_string(),
            _ => (i * 3).to_string(),
        };
        let col = if i % 3 == 0 { WHITE } else { DIM };
        c.text_f(&lbl, cx + t.sin() * r * 1.12, cy - t.cos() * r * 1.12, (if i % 3 == 0 { 14.0 } else { 11.5 }) * k, col, Align::Center, if i % 3 == 0 { Face::CondBold } else { Face::Cond });
    }
    let wind = m.and_then(|m| Some((m.wind_dir? as f64, m.wind_kt? as f64)));
    // sektor zmienności wiatru
    if let Some((v1, v2)) = m.and_then(|m| variable_sector(&m.raw)) {
        let sweep = (v2 - v1).rem_euclid(360.0);
        c.arc_band(cx, cy, r * 0.985, r * 0.05, v1 - 90.0, sweep, alpha(CYAN, 0x70));
    }
    // pasy w skali, położone jak na lotnisku (kursy rzeczywiste)
    let kx = 60.0 * a.lat.to_radians().cos();
    let rwys: Vec<_> = a.runways.iter().filter(|r| !r.closed && (r.has_coords() || r.le_heading().is_some())).collect();
    let mut ends: Vec<((f64, f64), (f64, f64))> = Vec::new();
    for rw in &rwys {
        if rw.has_coords() {
            ends.push((((rw.le_lon - a.lon) * kx, (rw.le_lat - a.lat) * 60.0), ((rw.he_lon - a.lon) * kx, (rw.he_lat - a.lat) * 60.0)));
        } else {
            let hd = rw.le_heading().unwrap_or(0.0).to_radians();
            let half = rw.length_ft as f64 / 6076.0 / 2.0;
            ends.push(((-hd.sin() * half, -hd.cos() * half), (hd.sin() * half, hd.cos() * half)));
        }
    }
    let ext = ends.iter().flat_map(|(p, q)| [p.0.abs(), p.1.abs(), q.0.abs(), q.1.abs()]).fold(0.15f64, f64::max);
    let sc = (r * 0.56) as f64 / ext;
    let pt = |p: (f64, f64)| (cx + (p.0 * sc) as f32, cy - (p.1 * sc) as f32);
    // najlepszy koniec pasa: największa składowa czołowa
    let mut best: Option<(usize, bool, f64)> = None;
    if let Some((wd, ws)) = wind.filter(|w| w.1 > 0.0) {
        for (i, rw) in rwys.iter().enumerate() {
            for (le, hd) in [(true, rw.le_heading()), (false, rw.he_heading())] {
                if let Some(hd) = hd {
                    let (hw, _) = calc::wind_components(wd, ws, hd);
                    if best.is_none_or(|b| hw > b.2) {
                        best = Some((i, le, hw));
                    }
                }
            }
        }
    }
    for (i, (rw, (p, q))) in rwys.iter().zip(&ends).enumerate() {
        let (a1, a2) = (pt(*p), pt(*q));
        let (dx, dy) = (a2.0 - a1.0, a2.1 - a1.1);
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let (ux, uy) = (dx / len, dy / len);
        let half = ((rw.width_ft as f64 / 6076.0 * sc) as f32 * 2.5).clamp(3.5 * k, 8.0 * k);
        let (nx, ny) = (-uy * half, ux * half);
        let is_best = best.is_some_and(|b| b.0 == i);
        c.poly(if is_best { 0xFF2C3A33 } else { 0xFF2A323C }, if is_best { GREEN } else { 0xFF566270 }, (if is_best { 2.0 } else { 1.2 }) * k, &[a1.0 + nx, a1.1 + ny, a2.0 + nx, a2.1 + ny, a2.0 - nx, a2.1 - ny, a1.0 - nx, a1.1 - ny]);
        c.dash_line(a1.0 + ux * 6.0 * k, a1.1 + uy * 6.0 * k, a2.0 - ux * 6.0 * k, a2.1 - uy * 6.0 * k, alpha(WHITE, 0x90), 1.0 * k, 5.0 * k, 4.0 * k);
        // oznaczenia końców na zewnątrz pasa
        for (end, ident, sgn) in [(a1, &rw.le, -1.0f32), (a2, &rw.he, 1.0f32)] {
            if ident.is_empty() {
                continue;
            }
            let lx = end.0 + ux * sgn * 13.0 * k;
            let ly = end.1 + uy * sgn * 13.0 * k;
            let b = best.is_some_and(|b| b.0 == i && (b.1 == (sgn < 0.0)));
            c.text_halo(ident, lx, ly, 12.5 * k, if b { GREEN } else { WHITE }, alpha(BG, 0xE0), Align::Center, true);
        }
        // strzałka podejścia na najlepszym końcu: samolot przylatuje od strony progu, leci wzdłuż kursu pasa
        if let Some((_, le, _)) = best.filter(|b| b.0 == i) {
            let (thr, dir) = if le { (a1, (ux, uy)) } else { (a2, (-ux, -uy)) };
            let s0 = (thr.0 - dir.0 * 46.0 * k, thr.1 - dir.1 * 46.0 * k);
            let s1 = (thr.0 - dir.0 * 24.0 * k, thr.1 - dir.1 * 24.0 * k);
            c.line(s0.0, s0.1, s1.0, s1.1, GREEN, 2.4 * k);
            let (px, py) = (-dir.1, dir.0);
            c.poly(GREEN, 0, 0.0, &[s1.0 + dir.0 * 8.0 * k, s1.1 + dir.1 * 8.0 * k, s1.0 + px * 5.0 * k, s1.1 + py * 5.0 * k, s1.0 - px * 5.0 * k, s1.1 - py * 5.0 * k]);
        }
    }
    if rwys.is_empty() {
        c.text_f(tr("brak danych o pasach"), cx, cy, 12.0 * k, DIM, Align::Center, Face::Cond);
    }
    // wiatr: strzałka z kierunku, z którego wieje, do środka
    match (m, wind) {
        (Some(m), Some((wd, ws))) if ws > 0.0 => {
            let a = wd as f32;
            let t = a.to_radians();
            let (sx, sy) = (cx + t.sin() * r * 0.99, cy - t.cos() * r * 0.99);
            let (ex, ey) = (cx + t.sin() * r * 0.66, cy - t.cos() * r * 0.66);
            let col = if m.gust_kt.unwrap_or(0) >= 25 || ws >= 20.0 { AMBER } else { CYAN };
            c.line(sx, sy, ex, ey, alpha(BG, 0xC0), 7.0 * k);
            c.line(sx, sy, ex, ey, col, 3.4 * k);
            let (px, py) = (t.cos(), t.sin());
            let (dx, dy) = (-t.sin(), t.cos());
            c.poly(col, 0, 0.0, &[ex + dx * 10.0 * k, ey + dy * 10.0 * k, ex + px * 7.0 * k, ey + py * 7.0 * k, ex - px * 7.0 * k, ey - py * 7.0 * k]);
        }
        _ => {}
    }
    // podpis wiatru w rogu
    let wtxt = match m {
        Some(m) => match (m.wind_variable, m.wind_dir, m.wind_kt) {
            (_, _, Some(0)) => "CALM".to_string(),
            (true, _, Some(s)) => format!("VRB {s:02} KT"),
            (_, Some(d), Some(s)) => format!("{d:03}° / {s:02} KT{}", m.gust_kt.map(|g| format!(" G{g}")).unwrap_or_default()),
            _ => "—".into(),
        },
        None => "—".into(),
    };
    c.text_f(tr("WIATR"), 10.0 * k, 12.0 * k, 10.0 * k, DIM, Align::Left, Face::CondBold);
    c.text_f(&wtxt, 10.0 * k, 28.0 * k, 14.0 * k, CYAN, Align::Left, Face::MonoBold);
    if let Some((i, le, hw)) = best {
        let rw = rwys[i];
        let ident = if le { &rw.le } else { &rw.he };
        let hd = if le { rw.le_heading() } else { rw.he_heading() };
        let (_, cw) = wind.zip(hd).map(|((wd, ws), h)| calc::wind_components(wd, ws, h)).unwrap_or((0.0, 0.0));
        c.text_f(tr("PAS W UŻYCIU"), w - 10.0 * k, 12.0 * k, 10.0 * k, DIM, Align::Right, Face::CondBold);
        c.text_f(&format!("RWY {ident}"), w - 10.0 * k, 28.0 * k, 14.0 * k, GREEN, Align::Right, Face::MonoBold);
        c.text_f(&format!("{} {:.0} · X {:.0} KT", if hw >= 0.0 { "HW" } else { "TW" }, hw.abs(), cw.abs()), w - 10.0 * k, 44.0 * k, 11.0 * k, WHITE, Align::Right, Face::Mono);
    }
    c.text_f(tr("kursy rzeczywiste"), 10.0 * k, h - 10.0 * k, 9.5 * k, FAINT, Align::Left, Face::Cond);
}

// ---------- profil nieba ----------

fn draw_sky(c: &mut Canvas, a: &Airport, m: Option<&Metar>, w: f32, h: f32, k: f32) {
    c.rect_vgrad(0.0, 0.0, w, h, 0.0, 0xFF07101E, h, 0xFF17304C);
    let Some(m) = m else {
        c.text_f(tr("brak METAR-u"), w / 2.0, h / 2.0, 13.0 * k, DIM, Align::Center, Face::Cond);
        return;
    };
    let left = 50.0 * k;
    let ground_h = 46.0 * k;
    let gy = h - ground_h;
    let top_pad = 14.0 * k;
    let highest = m.clouds.iter().filter_map(|c| c.base_ft).max().unwrap_or(0);
    let top = (((highest + 2500).max(5000) as f32 / 1000.0).ceil() * 1000.0).min(30_000.0);
    let y = |ft: f32| gy - (ft / top).clamp(0.0, 1.05) * (gy - top_pad);
    let step = if top <= 6000.0 { 1000 } else if top <= 12_000.0 { 2000 } else { 5000 };
    let mut f = step;
    while f as f32 <= top {
        let yy = y(f as f32);
        c.dash_line(left, yy, w, yy, alpha(WHITE, 0x18), 1.0 * k, 3.0 * k, 5.0 * k);
        c.text_f(&format!("{f}"), left - 6.0 * k, yy, 10.0 * k, DIM, Align::Right, Face::Mono);
        f += step;
    }
    c.text_f("FT AGL", left - 6.0 * k, gy - 8.0 * k, 8.5 * k, FAINT, Align::Right, Face::CondBold);
    let wx = wx_tokens(m);
    let lowest = m.clouds.iter().filter_map(|c| c.base_ft).min();
    // zamglenie / mgła przy ziemi
    if wx.contains("FG") || wx.contains("BR") || wx.contains("HZ") {
        let hh = if wx.contains("FG") { 600.0 } else { 1500.0 };
        c.rect_vgrad(left, y(hh), w, gy, y(hh), 0x00B0B8C4, gy, if wx.contains("FG") { 0xB0B0B8C4 } else { 0x60B0B8C4 });
    }
    // opady spod najniższej chmury
    let rain_top = lowest.map(|b| y(b as f32)).unwrap_or(top_pad + 20.0 * k);
    if ["RA", "DZ", "SH", "TS", "GR", "PL"].iter().any(|t| wx.contains(t)) {
        let heavy = wx.contains('+');
        let n = if heavy { 60 } else { 34 };
        for i in 0..n {
            let x0 = left + (i as f32 * 37.0 % 100.0) / 100.0 * (w - left) + (i as f32 * 3.1) % 9.0;
            let mut yy = rain_top + 8.0 * k + (i as f32 * 53.0 % 40.0) * k;
            while yy < gy - 4.0 * k {
                c.line(x0, yy, x0 - 3.0 * k, yy + 9.0 * k, 0x9946A0FF, 1.2 * k);
                yy += 26.0 * k;
            }
        }
    }
    if wx.contains("SN") || wx.contains("SG") {
        for i in 0..70 {
            let x0 = left + (i as f32 * 41.0 % 100.0) / 100.0 * (w - left);
            let yy = rain_top + 10.0 * k + (i as f32 * 29.0 % 100.0) / 100.0 * (gy - rain_top - 14.0 * k);
            c.circle(x0, yy, 1.6 * k, alpha(WHITE, 0xC0), 0.0);
        }
    }
    // poziom zamarzania (gradient 2 °C / 1000 ft)
    if let Some(t) = m.temp_c {
        if t > 0.0 {
            let fzl = (t / 2.0 * 1000.0) as f32;
            if fzl < top {
                let yy = y(fzl);
                c.dash_line(left, yy, w, yy, alpha(CYAN, 0xB0), 1.4 * k, 8.0 * k, 5.0 * k);
                c.text_f(&format!("0°C ~{:.0} ft", (fzl / 100.0).round() * 100.0), left + 6.0 * k, yy - 8.0 * k, 10.0 * k, CYAN, Align::Left, Face::Cond);
            }
        } else {
            c.text_f(tr("0°C przy ziemi – oblodzenie"), left + 6.0 * k, gy - 12.0 * k, 10.0 * k, CYAN, Align::Left, Face::Cond);
        }
    }
    // warstwy chmur: chmurki rozłożone wg pokrycia (oktanty)
    let plot_w = w - left - 70.0 * k;
    for (li, cl) in m.clouds.iter().enumerate() {
        let Some(base) = cl.base_ft else { continue };
        let ok = okta(&cl.cover);
        let yy = y(base as f32);
        if cl.cover == "VV" {
            c.rect(left, yy, w, gy, 0x80A0A8B4);
            continue;
        }
        let cols = 16;
        let cw = plot_w / cols as f32;
        let cb = m.raw.split_whitespace().any(|t| t.starts_with(&format!("{}{:03}", cl.cover, base / 100)) && (t.ends_with("CB") || t.ends_with("TCU")));
        let shade = if cb { 0xFF5A6474 } else if ok >= 8 { 0xFFD8DEE6 } else { 0xFFE8ECF2 };
        for j in 0..cols {
            if ((j * 5 + li * 3) % 8) as u32 >= ok {
                continue;
            }
            let x0 = left + 6.0 * k + j as f32 * cw + cw / 2.0;
            let rr = cw * 0.42;
            let s = alpha(shade, 0xE6);
            c.circle(x0 - rr * 0.6, yy - rr * 0.5, rr * 0.7, s, 0.0);
            c.circle(x0, yy - rr * 0.9, rr, s, 0.0);
            c.circle(x0 + rr * 0.7, yy - rr * 0.5, rr * 0.75, s, 0.0);
            c.rect(x0 - rr * 1.25, yy - rr * 0.55, x0 + rr * 1.4, yy, s);
            if cb {
                // wieża burzowa
                c.poly(alpha(shade, 0xE6), 0, 0.0, &[x0 - rr, yy - rr, x0 - rr * 0.4, yy - rr * 4.2, x0 + rr * 0.9, yy - rr * 4.6, x0 + rr * 1.2, yy - rr]);
            }
        }
        let col = if matches!(cl.cover.as_str(), "BKN" | "OVC") { if base < 1000 { 0xFFFF5A5F } else if base <= 3000 { 0xFF4F9DFF } else { WHITE } } else { WHITE };
        c.text_f(&format!("{} {:03}{}", cl.cover, base / 100, if cb { " CB" } else { "" }), w - 8.0 * k, yy - 9.0 * k, 12.0 * k, col, Align::Right, Face::MonoBold);
        c.text_f(&format!("{base} ft"), w - 8.0 * k, yy + 6.0 * k, 9.5 * k, DIM, Align::Right, Face::Mono);
    }
    if wx.contains("TS") {
        let x0 = left + plot_w * 0.55;
        let y0 = rain_top + 6.0 * k;
        c.poly(YELLOW, 0, 0.0, &[x0, y0, x0 - 8.0 * k, y0 + 22.0 * k, x0 - 1.0 * k, y0 + 22.0 * k, x0 - 6.0 * k, y0 + 42.0 * k, x0 + 9.0 * k, y0 + 15.0 * k, x0 + 2.0 * k, y0 + 15.0 * k]);
    }
    // pułap
    if let Some(ce) = ceiling(m) {
        if (ce as f32) < top {
            let yy = y(ce as f32);
            c.dash_line(left, yy, w - 74.0 * k, yy, AMBER, 1.6 * k, 10.0 * k, 5.0 * k);
            c.text_f(tr("PUŁAP"), left + 6.0 * k, yy + 9.0 * k, 9.5 * k, AMBER, Align::Left, Face::CondBold);
        }
    }
    if m.clouds.is_empty() {
        let t = if m.raw.contains("CAVOK") { "CAVOK" } else { tr("BEZ CHMUR") };
        c.text_f(t, left + (w - left) / 2.0, gy / 2.0, 22.0 * k, alpha(WHITE, 0x50), Align::Center, Face::CondBold);
    }
    // ziemia z pasem
    c.rect_vgrad(0.0, gy, w, h, gy, 0xFF1C2A22, h, 0xFF101812);
    c.line(0.0, gy, w, gy, 0xFF3C5A44, 1.4 * k);
    let (rl, rr) = (left + (w - left) * 0.25, left + (w - left) * 0.75);
    c.rect(rl, gy + 4.0 * k, rr, gy + 9.0 * k, 0xFF3A424C);
    c.dash_line(rl + 4.0 * k, gy + 6.5 * k, rr - 4.0 * k, gy + 6.5 * k, alpha(WHITE, 0xA0), 0.9 * k, 6.0 * k, 5.0 * k);
    c.text_f(&a.ident, rl - 6.0 * k, gy + 6.5 * k, 10.0 * k, DIM, Align::Right, Face::MonoBold);
    // pasek widzialności 0–10 km
    let v = vis_m(m).unwrap_or(10_000.0);
    let (bl, br, by) = (left, w - 74.0 * k, gy + 30.0 * k);
    c.text_f("VIS", bl - 6.0 * k, by, 9.5 * k, DIM, Align::Right, Face::CondBold);
    c.round_rect(bl, by - 4.0 * k, br, by + 4.0 * k, 4.0 * k, 0xFF22303A, 0, 0.0);
    let vc = cat_color(category(v, Some(99_999)));
    c.round_rect(bl, by - 4.0 * k, bl + (br - bl) * (v as f32 / 10_000.0).clamp(0.02, 1.0), by + 4.0 * k, 4.0 * k, vc, 0, 0.0);
    for km in [1.5f32, 5.0, 8.0] {
        let x = bl + (br - bl) * km / 10.0;
        c.line(x, by - 6.0 * k, x, by + 6.0 * k, alpha(WHITE, 0x50), 1.0 * k);
    }
    c.text_f(&vis_text(v), w - 8.0 * k, by, 12.0 * k, WHITE, Align::Right, Face::MonoBold);
    if !wx.is_empty() {
        c.text_f(&wx, w - 8.0 * k, gy - 12.0 * k, 12.0 * k, AMBER, Align::Right, Face::MonoBold);
    }
}

// ---------- TAF ----------

#[derive(Clone, Debug, Default)]
struct Cond {
    wind: Option<String>,
    vis_m: Option<f64>,
    clouds: Vec<(String, i32)>,
    wx: Vec<String>,
    nsc: bool,
}

impl Cond {
    fn ceiling(&self) -> Option<i32> {
        self.clouds.iter().filter(|c| matches!(c.0.as_str(), "BKN" | "OVC" | "VV")).map(|c| c.1).min()
    }
    fn cat(&self) -> &'static str {
        category(self.vis_m.unwrap_or(10_000.0), self.ceiling())
    }
    fn short(&self) -> String {
        let mut p = Vec::new();
        if let Some(w) = &self.wind {
            p.push(w.clone());
        }
        if let Some(v) = self.vis_m {
            p.push(if v >= 9999.0 { "9999".into() } else { format!("{v:.0}") });
        }
        p.extend(self.wx.iter().cloned());
        if self.nsc && self.clouds.is_empty() {
            p.push("NSC".into());
        }
        p.extend(self.clouds.iter().take(2).map(|(c, b)| format!("{c}{:03}", b / 100)));
        p.join(" ")
    }
}

#[derive(Clone, Debug)]
struct Period {
    kind: String,
    from: f64,
    to: f64,
    cond: Cond,
}

#[derive(Debug)]
struct TafPlan {
    start: f64,
    end: f64,
    /// Godzina UTC początku ważności (do opisu osi).
    h0: u32,
    d0: u32,
    now: Option<f64>,
    base: Vec<Period>,
    overlays: Vec<Period>,
}

fn apply(c: &mut Cond, t: &str, fresh_clouds: &mut bool) -> bool {
    let wind = Regex::new(r"^(\d{3}|VRB)(\d{2,3})(G\d{2,3})?(KT|MPS)$").unwrap();
    let cloud = Regex::new(r"^(FEW|SCT|BKN|OVC|VV)(\d{3})(CB|TCU)?$").unwrap();
    let wx = Regex::new(r"^[-+]?(VC)?(MI|BC|PR|DR|BL|SH|TS|FZ)?(DZ|RA|SN|SG|IC|PL|GR|GS|UP|BR|FG|FU|VA|DU|SA|HZ|PO|SQ|FC|SS|DS)*$").unwrap();
    if wind.is_match(t) {
        c.wind = Some(t.trim_end_matches("KT").to_string());
    } else if t == "CAVOK" {
        c.vis_m = Some(10_000.0);
        c.clouds.clear();
        c.wx.clear();
        c.nsc = true;
    } else if t.len() == 4 && t.chars().all(|x| x.is_ascii_digit()) {
        let v: f64 = t.parse().unwrap_or(9999.0);
        c.vis_m = Some(if v >= 9999.0 { 10_000.0 } else { v });
    } else if t == "P6SM" {
        c.vis_m = Some(10_000.0);
    } else if let Some(sm) = t.strip_suffix("SM").and_then(metar_dec::parse_sm) {
        c.vis_m = Some((sm * 1609.34).min(10_000.0));
    } else if let Some(cap) = cloud.captures(t) {
        if *fresh_clouds {
            c.clouds.clear();
            c.nsc = false;
            *fresh_clouds = false;
        }
        c.clouds.push((cap[1].to_string(), cap[2].parse::<i32>().unwrap_or(0) * 100));
    } else if matches!(t, "NSC" | "SKC" | "CLR" | "NCD") {
        c.clouds.clear();
        c.nsc = true;
    } else if t == "NSW" {
        c.wx.clear();
    } else if t.len() >= 2 && wx.is_match(t) && t != "VC" {
        c.wx.push(t.to_string());
    } else {
        return false;
    }
    true
}

fn parse_taf(raw: &str) -> Option<TafPlan> {
    let toks: Vec<&str> = raw.split_whitespace().collect();
    let range = Regex::new(r"^(\d{2})(\d{2})/(\d{2})(\d{2})$").unwrap();
    let fm = Regex::new(r"^FM(\d{2})(\d{2})(\d{2})$").unwrap();
    let vi = toks.iter().position(|t| range.is_match(t))?;
    let cap = range.captures(toks[vi])?;
    let (d0, h0): (u32, u32) = (cap[1].parse().ok()?, cap[2].parse().ok()?);
    // długość miesiąca, w którym zaczyna się prognoza
    let now = Utc::now();
    let start_month = if now.day() + 5 < d0 { now - Duration::days(now.day() as i64) } else { now };
    let dim = {
        let (y, mo) = (start_month.year(), start_month.month());
        let next = if mo == 12 { NaiveDate::from_ymd_opt(y + 1, 1, 1) } else { NaiveDate::from_ymd_opt(y, mo + 1, 1) };
        next.and_then(|n| n.pred_opt()).map(|d| d.day()).unwrap_or(31)
    };
    let rel = |d: u32, h: u32, mi: u32| -> f64 {
        let dd = if d + 10 < d0 { d + dim } else { d };
        (dd as f64 - d0 as f64) * 24.0 + h as f64 + mi as f64 / 60.0 - h0 as f64
    };
    let start = 0.0;
    let end = rel(cap[3].parse().ok()?, cap[4].parse().ok()?, 0);
    let now_rel = {
        let d = now.day();
        let r = rel(d, now.hour(), now.minute());
        (r >= -6.0 && r <= end + 6.0).then_some(r)
    };
    let mut base = vec![Period { kind: "BASE".into(), from: start, to: end, cond: Cond::default() }];
    let mut overlays: Vec<Period> = Vec::new();
    // cel bieżących grup: true – prognoza główna, false – ostatnia nakładka
    let mut to_base = true;
    let mut fresh = true;
    let mut i = vi + 1;
    let mut pending_prob: Option<String>;
    while i < toks.len() {
        let t = toks[i];
        if let Some(c) = fm.captures(t) {
            let at = rel(c[1].parse().unwrap_or(d0), c[2].parse().unwrap_or(0), c[3].parse().unwrap_or(0));
            if let Some(last) = base.last_mut() {
                last.to = at;
            }
            base.push(Period { kind: "FM".into(), from: at, to: end, cond: Cond::default() });
            to_base = true;
            fresh = true;
        } else if t == "BECMG" || t == "TEMPO" || t.starts_with("PROB") {
            if t.starts_with("PROB") && toks.get(i + 1) == Some(&"TEMPO") {
                pending_prob = Some(format!("{t} TEMPO"));
                i += 2;
            } else if t.starts_with("PROB") {
                pending_prob = Some(t.to_string());
                i += 1;
            } else {
                pending_prob = Some(t.to_string());
                i += 1;
            }
            let kind = pending_prob.take().unwrap_or_default();
            let Some(c) = toks.get(i).and_then(|x| range.captures(x)) else { continue };
            let from = rel(c[1].parse().unwrap_or(d0), c[2].parse().unwrap_or(0), 0);
            let to = rel(c[3].parse().unwrap_or(d0), c[4].parse().unwrap_or(0), 0);
            if kind == "BECMG" {
                let prev = base.last().map(|p| p.cond.clone()).unwrap_or_default();
                let fm_end = base.last().map(|p| p.to).unwrap_or(end);
                if let Some(last) = base.last_mut() {
                    last.to = from;
                }
                base.push(Period { kind: "BECMG".into(), from, to: fm_end, cond: prev });
                to_base = true;
            } else {
                overlays.push(Period { kind, from, to, cond: Cond::default() });
                to_base = false;
            }
            fresh = true;
            let _ = to;
        } else {
            let tgt = if to_base { base.last_mut() } else { overlays.last_mut() };
            if let Some(p) = tgt {
                apply(&mut p.cond, t, &mut fresh);
            }
        }
        i += 1;
    }
    base.retain(|p| p.to > p.from);
    Some(TafPlan { start, end, h0, d0, now: now_rel, base, overlays })
}

/// Rozkład nakładek (TEMPO/PROB) na wiersze bez nachodzenia: (indeks wiersza dla każdej, liczba wierszy).
fn lanes(ov: &[Period]) -> (Vec<usize>, usize) {
    let mut ends: Vec<f64> = Vec::new();
    let mut idx = Vec::new();
    for p in ov {
        match ends.iter().position(|e| *e <= p.from + 0.01) {
            Some(l) => {
                ends[l] = p.to;
                idx.push(l);
            }
            None => {
                ends.push(p.to);
                idx.push(ends.len() - 1);
            }
        }
    }
    (idx, ends.len())
}

fn draw_taf(c: &mut Canvas, plan: Option<&TafPlan>, w: f32, h: f32, k: f32) {
    c.rect(0.0, 0.0, w, h, BG);
    let Some(p) = plan else {
        c.text_f(tr("brak prognozy TAF dla tego lotniska"), w / 2.0, h / 2.0, 12.0 * k, DIM, Align::Center, Face::Cond);
        return;
    };
    let left = 78.0 * k;
    let right = w - 10.0 * k;
    let span = (p.end - p.start).max(1.0);
    let x = |t: f64| left + ((t - p.start) / span) as f32 * (right - left);
    let axis_y = 18.0 * k;
    // oś czasu: godziny UTC
    for hh in 0..=(span as i32) {
        let xx = x(hh as f64);
        let abs_h = (p.h0 as i32 + hh).rem_euclid(24);
        let major = abs_h % 3 == 0;
        c.line(xx, axis_y + 6.0 * k, xx, h - 6.0 * k, alpha(WHITE, if abs_h == 0 { 0x40 } else if major { 0x18 } else { 0x0A }), 1.0 * k);
        if major {
            c.text_f(&format!("{abs_h:02}Z"), xx, axis_y - 4.0 * k, 9.5 * k, if abs_h == 0 { WHITE } else { DIM }, Align::Center, Face::Mono);
        }
        if abs_h == 0 {
            let day = p.d0 + ((p.h0 as i32 + hh) / 24) as u32;
            c.text_f(&format!("{day}."), xx + 3.0 * k, axis_y + 12.0 * k, 9.0 * k, WHITE, Align::Left, Face::CondBold);
        }
    }
    // prognoza główna
    let (b0, b1) = (axis_y + 10.0 * k, axis_y + 58.0 * k);
    c.text_f(tr("PROGNOZA"), 10.0 * k, (b0 + b1) / 2.0, 10.0 * k, DIM, Align::Left, Face::CondBold);
    for per in &p.base {
        let (l, r) = (x(per.from), x(per.to));
        let col = cat_color(per.cond.cat());
        c.round_rect(l + 1.0 * k, b0, r - 1.0 * k, b1, 4.0 * k, alpha(col, 0x38), alpha(col, 0x90), 1.0 * k);
        c.rect(l + 1.0 * k, b0, r - 1.0 * k, b0 + 4.0 * k, col);
        if r - l > 70.0 * k {
            c.save();
            c.clip_rect(l + 4.0 * k, b0, r - 4.0 * k, b1);
            let tag = if per.kind == "BASE" { per.cond.cat().to_string() } else { format!("{} · {}", per.kind, per.cond.cat()) };
            c.text_f(&tag, l + 7.0 * k, b0 + 16.0 * k, 11.0 * k, col, Align::Left, Face::CondBold);
            c.text_f(&per.cond.short(), l + 7.0 * k, b0 + 34.0 * k, 11.0 * k, WHITE, Align::Left, Face::Mono);
            c.restore();
        }
    }
    // nakładki TEMPO / PROB
    let (idx, n) = lanes(&p.overlays);
    let lane_h = 30.0 * k;
    let o0 = b1 + 10.0 * k;
    if n > 0 {
        c.text_f("TEMPO / PROB", 10.0 * k, o0 + lane_h / 2.0, 10.0 * k, DIM, Align::Left, Face::CondBold);
    }
    for (per, l) in p.overlays.iter().zip(idx) {
        let (lx, rx) = (x(per.from), x(per.to));
        let (t, b) = (o0 + l as f32 * (lane_h + 4.0 * k), o0 + l as f32 * (lane_h + 4.0 * k) + lane_h);
        let col = cat_color(per.cond.cat());
        c.save();
        c.clip_rect(lx + 1.0 * k, t, rx - 1.0 * k, b);
        c.rect(lx, t, rx, b, alpha(col, 0x22));
        let mut sx = lx - (b - t);
        while sx < rx {
            c.line(sx, b, sx + (b - t), t, alpha(col, 0x40), 2.0 * k);
            sx += 9.0 * k;
        }
        c.frame(lx + 1.0 * k, t, rx - 1.0 * k, b, alpha(col, 0xB0), 1.0 * k);
        c.text_f(&format!("{} {}", per.kind, per.cond.short()), lx + 6.0 * k, (t + b) / 2.0, 10.5 * k, WHITE, Align::Left, Face::Mono);
        c.restore();
    }
    // teraz
    if let Some(nw) = p.now {
        if nw >= p.start && nw <= p.end {
            let xx = x(nw);
            c.line(xx, axis_y + 4.0 * k, xx, h - 4.0 * k, alpha(WHITE, 0xE0), 1.6 * k);
            c.poly(WHITE, 0, 0.0, &[xx - 5.0 * k, axis_y + 2.0 * k, xx + 5.0 * k, axis_y + 2.0 * k, xx, axis_y + 9.0 * k]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taf_periods() {
        let p = parse_taf("TAF EPWA 161430Z 1615/1715 18010KT 9999 SCT040 TEMPO 1615/1712 -RA BKN014 PROB40 TEMPO 1617/1709 4000 RADZ BR BKN007 FM170900 22015KT CAVOK").unwrap();
        assert_eq!(p.end, 24.0);
        assert_eq!(p.base.len(), 2);
        assert_eq!(p.base[0].cond.cat(), "VFR");
        assert_eq!(p.base[1].from, 18.0);
        assert_eq!(p.overlays.len(), 2);
        assert_eq!(p.overlays[0].cond.cat(), "MVFR");
        assert_eq!(p.overlays[1].cond.cat(), "IFR");
        assert_eq!(p.overlays[1].kind, "PROB40 TEMPO");
        assert_eq!(lanes(&p.overlays).1, 2);
    }

    #[test]
    fn becmg_keeps_previous() {
        let p = parse_taf("TAF EPKK 161100Z 1612/1712 24008KT 9999 BKN030 BECMG 1618/1620 2000 BR OVC004").unwrap();
        assert_eq!(p.base.len(), 2);
        assert_eq!(p.base[1].cond.wind.as_deref(), Some("24008"));
        assert_eq!(p.base[1].cond.cat(), "LIFR");
    }
}
