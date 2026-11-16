//! Wyświetlacz nawigacyjny (ND/MFD) w stylu kabiny – przeniesiony z MfdRenderer.kt: Airbus ND, Boeing ND
//! (też 787, ATR, E190 w ich kolorach), Garmin G1000 MFD z paskiem EIS i mapa GPS w stylu GNS 530.
//! Tryb ARC (wycinek przed samolotem) albo ROSE (pełna róża).

use opentakeoff_core::navdata::math::{bearing_deg, distance_nm};

use super::data::*;
use super::gfx::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    Airbus,
    Boeing,
    Garmin,
    Gns,
}

#[derive(Clone, Copy)]
struct Skin {
    layout: Layout,
    bg: Argb,
    ink: Argb,
    label: Argb,
    active: Argb,
    route: Argb,
    to_wpt: Argb,
    hdg_bug: Argb,
    track: Argb,
    lubber: Argb,
    aircraft: Argb,
    airport: Argb,
    vor: Argb,
    ndb: Argb,
    fix: Argb,
    range: Argb,
    heading_up: bool,
}

fn skin(style: DisplayStyle) -> Skin {
    use DisplayStyle::*;
    let s = |layout, bg, ink, label, active, route, to_wpt, hdg_bug, track, lubber, aircraft, airport, vor, ndb, fix, range, heading_up| Skin { layout, bg, ink, label, active, route, to_wpt, hdg_bug, track, lubber, aircraft, airport, vor, ndb, fix, range, heading_up };
    match style {
        Airbus => s(Layout::Airbus, BLACK, WHITE, WHITE, 0xFF2BF22B, 0xFF2BF22B, WHITE, 0xFF1FE6FF, 0xFF2BF22B, 0xFFFFF200, 0xFFFFF200, 0xFFFF55FF, 0xFFFF55FF, 0xFFFF55FF, 0xFFFF55FF, 0xFF1FE6FF, true),
        Boeing | B787 => s(Layout::Boeing, BLACK, WHITE, WHITE, 0xFFEE55DD, WHITE, 0xFFEE55DD, 0xFFEE55DD, WHITE, WHITE, WHITE, 0xFF33DDFF, 0xFF33DDFF, 0xFF33DDFF, 0xFF33DDFF, WHITE, false),
        Atr => s(Layout::Boeing, 0xFF06080C, WHITE, 0xFF2BD9F0, 0xFFE54BD2, WHITE, 0xFFE54BD2, 0xFF2BD9F0, 0xFF27E327, 0xFFFFE000, 0xFFFFE000, 0xFF2BD9F0, 0xFF27E327, 0xFF27E327, WHITE, 0xFF2BD9F0, true),
        Embraer => s(Layout::Boeing, 0xFF05070A, WHITE, 0xFF2FD8F2, 0xFFE85AD6, WHITE, 0xFFE85AD6, 0xFF2FD8F2, 0xFFE85AD6, WHITE, WHITE, 0xFF2FD8F2, 0xFF35E635, 0xFF35E635, WHITE, 0xFF2FD8F2, true),
        Garmin => s(Layout::Garmin, 0xFF0A1218, WHITE, 0xFF3FE0FF, 0xFFFF33E6, WHITE, 0xFFFF33E6, 0xFF3FE0FF, 0xFFFF33E6, WHITE, WHITE, 0xFF3FE0FF, 0xFF4F8CFF, 0xFF6AC66A, 0xFFB0B8C0, WHITE, true),
        _ => s(Layout::Gns, 0xFF020A06, 0xFFE8F0E8, 0xFF5CE65C, 0xFFFF33E6, WHITE, 0xFFFF33E6, 0xFF3FE0FF, 0xFF5CE65C, WHITE, WHITE, 0xFF3FE0FF, 0xFF4F8CFF, 0xFF6AC66A, 0xFFB0B8C0, 0xFF5CE65C, false),
    }
}

/// Punkt trasy na ND.
#[derive(Clone, Debug)]
pub struct RoutePt {
    pub ident: String,
    pub lat: f64,
    pub lon: f64,
    pub constraint: Option<String>,
}

/// Pomoc nawigacyjna albo lotnisko na ND.
#[derive(Clone, Debug)]
pub struct NdFix {
    pub ident: String,
    /// "airport", "vor", "ndb", "waypoint"
    pub kind: &'static str,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Layers {
    pub airports: bool,
    pub vor: bool,
    pub ndb: bool,
}

struct Next {
    id: String,
    brg: f32,
    dist: f32,
    ete_sec: f32,
}

pub struct Nd {
    sk: Skin,
    cx: f32,
    cy: f32,
    radius: f32,
    scale: f32,
    up: f32,
    u: f32,
}

fn ete(sec: f32) -> String {
    if sec.is_nan() {
        "--:--".into()
    } else {
        format!("{}:{:02}", (sec / 3600.0) as i32, ((sec % 3600.0) / 60.0) as i32)
    }
}
fn ete_min(sec: f32) -> String {
    if sec.is_nan() {
        "--:--".into()
    } else {
        format!("{:02}:{:02}", (sec / 60.0) as i32, (sec % 60.0) as i32)
    }
}

impl Nd {
    pub fn new(style: DisplayStyle) -> Self {
        Self { sk: skin(style), cx: 0.0, cy: 0.0, radius: 0.0, scale: 0.0, up: 0.0, u: 1.0 }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, c: &mut Canvas, d: &FlightData, w: f32, h: f32, range_nm: f32, rose: bool, route: &[RoutePt], fixes: &[NdFix], layers: Layers) {
        let sk = self.sk;
        self.u = h / 1000.0;
        let u = self.u;
        c.rect(0.0, 0.0, w, h, sk.bg);
        let left = if sk.layout == Layout::Garmin { w * 0.2 } else { 0.0 };
        let right = if sk.layout == Layout::Gns { w * 0.7 } else { w };
        let top = if sk.layout == Layout::Garmin { 70.0 * u } else { 0.0 };
        let (mw, mh) = (right - left, h - top);
        self.cx = left + mw / 2.0;
        if rose {
            self.cy = top + mh * 0.54;
            self.radius = (mh * 0.38).min(mw * 0.42);
        } else {
            self.cy = top + mh * 0.8;
            self.radius = (mh * 0.68).min(mw * 0.62);
        }
        self.scale = self.radius / range_nm;
        self.up = if sk.heading_up { d.heading } else { d.track };

        c.save();
        c.clip_rect(left, top, right, h);
        self.rings(c, range_nm, rose);
        if d.att_valid {
            self.fixes(c, d, fixes, layers, range_nm, rose);
            self.route(c, d, route);
        }
        self.compass(c, d, rose);
        self.course(c, d);
        self.aircraft(c);
        c.restore();

        match sk.layout {
            Layout::Airbus => self.airbus_data(c, d, w, h, route),
            Layout::Boeing => self.boeing_data(c, d, w, h, route),
            Layout::Garmin => {
                self.garmin_top(c, d, w, top, route);
                self.eis(c, d, left, h);
            }
            Layout::Gns => self.gns_column(c, d, right, w, h, route),
        }
        let rl = if range_nm.fract() == 0.0 { format!("{}", range_nm as i32) } else { format!("{range_nm:.1}") };
        c.text_l(&rl, left + 14.0 * u, h - 30.0 * u, 26.0 * u, sk.range);
    }

    fn project(&self, d: &FlightData, tlat: f64, tlon: f64) -> (f32, f32) {
        let dist = distance_nm(d.lat, d.lon, tlat, tlon) as f32;
        let rel = (bearing_deg(d.lat, d.lon, tlat, tlon) as f32 - d.mag_var - self.up).to_radians();
        (self.cx + rel.sin() * dist * self.scale, self.cy - rel.cos() * dist * self.scale)
    }

    fn rings(&self, c: &mut Canvas, range_nm: f32, rose: bool) {
        let (u, cx, cy) = (self.u, self.cx, self.cy);
        let col = alpha(self.sk.ink, 0x77);
        for f in [0.5, 1.0] {
            let r = self.radius * f;
            if rose {
                c.dash_circle(cx, cy, r, col, 2.0 * u, 14.0 * u, 12.0 * u);
            } else {
                c.dash_arc(cx - r, cy - r, cx + r, cy + r, 200.0, 140.0, col, 2.0 * u, 14.0 * u, 12.0 * u);
            }
        }
        if self.sk.layout != Layout::Garmin {
            let half = range_nm / 2.0;
            let lbl = if half.fract() == 0.0 { format!("{}", half as i32) } else { format!("{half:.1}") };
            let a = (-60.0f32).to_radians();
            c.text(&lbl, cx + a.sin() * self.radius * 0.5 - 16.0 * u, cy - a.cos() * self.radius * 0.5, 24.0 * u, self.sk.range);
        }
    }

    fn compass(&self, c: &mut Canvas, d: &FlightData, rose: bool) {
        let (u, cx, cy, sk) = (self.u, self.cx, self.cy, self.sk);
        let mut deg = 0;
        while deg < 360 {
            let rel = ang_diff(deg as f32, self.up);
            if !rose && rel.abs() > 62.0 {
                deg += 5;
                continue;
            }
            let ten = deg % 10 == 0;
            c.radial(cx, cy, rel, self.radius - if ten { 22.0 * u } else { 12.0 * u }, self.radius, sk.ink, if ten { 3.0 * u } else { 2.0 * u });
            if deg % 30 == 0 {
                let a = rel.to_radians();
                let rr = self.radius - 44.0 * u;
                let big = deg % 90 == 0;
                let lbl = if matches!(sk.layout, Layout::Garmin | Layout::Gns) {
                    match deg {
                        0 => "N".into(),
                        90 => "E".into(),
                        180 => "S".into(),
                        270 => "W".into(),
                        _ => (deg / 10).to_string(),
                    }
                } else {
                    (deg / 10).to_string()
                };
                c.text(&lbl, cx + a.sin() * rr, cy - a.cos() * rr, if big { 32.0 } else { 26.0 } * u, sk.ink);
            }
            deg += 5;
        }
        c.line(cx, cy - self.radius - 26.0 * u, cx, cy - self.radius + 4.0 * u, sk.lubber, 5.0 * u);
        if !d.hdg_bug.is_nan() {
            let rel = ang_diff(d.hdg_bug, self.up);
            if rose || rel.abs() <= 62.0 {
                c.save();
                c.rotate(rel, cx, cy);
                let y = cy - self.radius;
                c.poly(0, sk.hdg_bug, 3.5 * u, &[cx - 18.0 * u, y - 16.0 * u, cx + 18.0 * u, y - 16.0 * u, cx + 18.0 * u, y, cx + 6.0 * u, y, cx, y + 8.0 * u, cx - 6.0 * u, y, cx - 18.0 * u, y]);
                c.restore();
            } else {
                let txt = format!("{:03}", imod(roundi(d.hdg_bug), 360));
                c.text(&txt, cx + if rel > 0.0 { 1.0 } else { -1.0 } * self.radius * 0.82, cy - self.radius * 0.72, 26.0 * u, sk.hdg_bug);
            }
        }
        if d.has_track && sk.heading_up {
            let rel = ang_diff(d.track, self.up);
            c.save();
            c.rotate(rel, cx, cy);
            let r = self.radius;
            c.poly(0, sk.track, 3.0 * u, &[cx, cy - r + 2.0 * u, cx + 9.0 * u, cy - r + 16.0 * u, cx, cy - r + 30.0 * u, cx - 9.0 * u, cy - r + 16.0 * u]);
            c.restore();
        } else if !sk.heading_up {
            let rel = ang_diff(d.heading, self.up);
            c.save();
            c.rotate(rel, cx, cy);
            let r = self.radius;
            c.poly(0, sk.ink, 3.0 * u, &[cx, cy - r + 4.0 * u, cx - 12.0 * u, cy - r + 26.0 * u, cx + 12.0 * u, cy - r + 26.0 * u]);
            c.restore();
            c.dash_line(cx, cy, cx, cy - self.radius, sk.ink, 2.0 * u, 16.0 * u, 10.0 * u);
        }
    }

    fn course(&self, c: &mut Canvas, d: &FlightData) {
        let n = &d.nav;
        if n.course.is_nan() || matches!(n.source, NavSource::Gps | NavSource::None) {
            return;
        }
        let (u, cx, cy) = (self.u, self.cx, self.cy);
        let col = if self.sk.layout == Layout::Airbus { 0xFFFF55FF } else { 0xFF33DDFF };
        c.save();
        c.rotate(ang_diff(n.course, self.up), cx, cy);
        c.dash_line(cx, cy + self.radius, cx, cy - self.radius, col, 3.0 * u, 20.0 * u, 10.0 * u);
        if !n.cdi.is_nan() {
            let x = cx + n.cdi.clamp(-2.5, 2.5) * 24.0 * u;
            c.line(x, cy - 70.0 * u, x, cy + 70.0 * u, col, 5.0 * u);
        }
        c.restore();
    }

    fn aircraft(&self, c: &mut Canvas) {
        let (k, cx, cy, col) = (self.u, self.cx, self.cy, self.sk.aircraft);
        match self.sk.layout {
            Layout::Airbus => {
                c.line(cx, cy - 30.0 * k, cx, cy + 34.0 * k, col, 6.0 * k);
                c.line(cx - 30.0 * k, cy - 4.0 * k, cx + 30.0 * k, cy - 4.0 * k, col, 6.0 * k);
                c.line(cx - 12.0 * k, cy + 26.0 * k, cx + 12.0 * k, cy + 26.0 * k, col, 5.0 * k);
            }
            Layout::Boeing => c.poly(0, col, 3.5 * k, &[cx, cy - 34.0 * k, cx - 18.0 * k, cy + 20.0 * k, cx + 18.0 * k, cy + 20.0 * k]),
            _ => {
                let p: Vec<f32> = [(0.0, -22.0), (-4.0, -8.0), (-24.0, 2.0), (-24.0, 9.0), (-4.0, 5.0), (-4.0, 17.0), (-10.0, 22.0), (-10.0, 26.0), (10.0, 26.0), (10.0, 22.0), (4.0, 17.0), (4.0, 5.0), (24.0, 9.0), (24.0, 2.0), (4.0, -8.0)]
                    .iter()
                    .flat_map(|(x, y)| [cx + x * k, cy + y * k])
                    .collect();
                c.poly(col, BLACK, 2.0 * k, &p);
            }
        }
    }

    fn fixes(&self, c: &mut Canvas, d: &FlightData, fixes: &[NdFix], layers: Layers, range_nm: f32, rose: bool) {
        let (u, sk) = (self.u, self.sk);
        for f in fixes {
            let on = match f.kind {
                "airport" => layers.airports,
                "vor" => layers.vor,
                "ndb" => layers.ndb,
                _ => true,
            };
            if !on || distance_nm(d.lat, d.lon, f.lat, f.lon) as f32 > range_nm * 1.05 {
                continue;
            }
            let (x, y) = self.project(d, f.lat, f.lon);
            if !rose && y > self.cy + 10.0 * u {
                continue;
            }
            let col = match f.kind {
                "airport" => sk.airport,
                "vor" => sk.vor,
                "ndb" => sk.ndb,
                _ => sk.fix,
            };
            match f.kind {
                "airport" => {
                    c.circle(x, y, 9.0 * u, col, 3.0 * u);
                    if sk.layout == Layout::Airbus {
                        c.line(x - 14.0 * u, y, x + 14.0 * u, y, col, 3.0 * u);
                    }
                }
                "vor" => {
                    if matches!(sk.layout, Layout::Airbus | Layout::Boeing) {
                        c.line(x - 10.0 * u, y, x + 10.0 * u, y, col, 3.0 * u);
                        c.line(x, y - 10.0 * u, x, y + 10.0 * u, col, 3.0 * u);
                    } else {
                        let pts: Vec<f32> = (0..6).flat_map(|k| {
                            let a = (60.0 * k as f32).to_radians();
                            [x + 10.0 * u * a.cos(), y + 10.0 * u * a.sin()]
                        }).collect();
                        c.poly(0, col, 2.5 * u, &pts);
                        c.circle(x, y, 2.5 * u, col, 0.0);
                    }
                }
                "ndb" => {
                    c.circle(x, y, 3.5 * u, col, 0.0);
                    c.circle(x, y, 10.0 * u, col, 1.5 * u);
                }
                _ => c.poly(0, col, 2.5 * u, &[x, y - 9.0 * u, x + 8.0 * u, y + 6.0 * u, x - 8.0 * u, y + 6.0 * u]),
            }
            if f.kind != "waypoint" || range_nm <= 40.0 {
                c.text_l(&f.ident, x + 14.0 * u, y - 14.0 * u, 22.0 * u, col);
            }
        }
    }

    fn next_index(&self, d: &FlightData, pts: &[RoutePt]) -> Option<usize> {
        if let Some(id) = &d.gps.next_id {
            if let Some(i) = pts.iter().position(|p| p.ident.eq_ignore_ascii_case(id)) {
                return Some(i);
            }
        }
        let mut best = None;
        let mut best_d = f64::MAX;
        for (i, p) in pts.iter().enumerate() {
            let brg = bearing_deg(d.lat, d.lon, p.lat, p.lon) as f32 - d.mag_var;
            if ang_diff(brg, d.track).abs() < 90.0 {
                let dist = distance_nm(d.lat, d.lon, p.lat, p.lon);
                if dist < best_d {
                    best_d = dist;
                    best = Some(i);
                }
            }
        }
        best
    }

    fn route(&self, c: &mut Canvas, d: &FlightData, pts: &[RoutePt]) {
        if pts.is_empty() {
            return;
        }
        let (u, sk) = (self.u, self.sk);
        let xy: Vec<(f32, f32)> = pts.iter().map(|p| self.project(d, p.lat, p.lon)).collect();
        let next = self.next_index(d, pts);
        for i in 1..xy.len() {
            let active = Some(i) == next;
            c.line(xy[i - 1].0, xy[i - 1].1, xy[i].0, xy[i].1, if active { sk.active } else { sk.route }, if active { 5.0 } else { 3.5 } * u);
        }
        for (i, p) in pts.iter().enumerate() {
            let (x, y) = xy[i];
            let col = if Some(i) == next { sk.to_wpt } else { sk.route };
            c.poly(0, col, 2.5 * u, &[x, y - 10.0 * u, x + 10.0 * u, y, x, y + 10.0 * u, x - 10.0 * u, y]);
            c.text_l(&p.ident, x + 16.0 * u, y - 14.0 * u, 22.0 * u, col);
            if let Some(k) = &p.constraint {
                c.text_l(k, x + 16.0 * u, y + 12.0 * u, 18.0 * u, 0xFFFF55FF);
            }
        }
    }

    fn next(&self, d: &FlightData, route: &[RoutePt]) -> Option<Next> {
        let g = &d.gps;
        if let (Some(id), false) = (&g.next_id, g.next_dist_nm.is_nan()) {
            return Some(Next { id: id.clone(), brg: g.next_brg, dist: g.next_dist_nm, ete_sec: g.next_ete_sec });
        }
        if !d.att_valid {
            return None;
        }
        let i = self.next_index(d, route)?;
        let p = &route[i];
        let dist = distance_nm(d.lat, d.lon, p.lat, p.lon) as f32;
        let brg = wrap360(bearing_deg(d.lat, d.lon, p.lat, p.lon) as f32 - d.mag_var);
        let ete = if !d.gs_kt.is_nan() && d.gs_kt > 40.0 { dist / d.gs_kt * 3600.0 } else { f32::NAN };
        Some(Next { id: p.ident.clone(), brg, dist, ete_sec: ete })
    }

    fn wind_arrow(&self, c: &mut Canvas, x: f32, y: f32, d: &FlightData, col: Argb) {
        let u = self.u;
        c.save();
        c.rotate(d.wind_dir + 180.0 - self.up, x, y);
        c.line(x, y + 22.0 * u, x, y - 22.0 * u, col, 4.0 * u);
        c.poly(col, col, 1.0, &[x, y + 26.0 * u, x - 9.0 * u, y + 10.0 * u, x + 9.0 * u, y + 10.0 * u]);
        c.restore();
    }

    fn airbus_data(&self, c: &mut Canvas, d: &FlightData, w: f32, h: f32, route: &[RoutePt]) {
        let u = self.u;
        let (green, cyan) = (0xFF2BF22B, 0xFF1FE6FF);
        let x0 = 16.0 * u;
        c.text_l("GS", x0, 30.0 * u, 28.0 * u, WHITE);
        c.text_l(&if d.gs_kt.is_nan() { "---".into() } else { roundi(d.gs_kt).to_string() }, x0 + 48.0 * u, 30.0 * u, 32.0 * u, green);
        c.text_l("TAS", x0 + 130.0 * u, 30.0 * u, 28.0 * u, WHITE);
        c.text_l(&if d.tas_kt.is_nan() { "---".into() } else { roundi(d.tas_kt).to_string() }, x0 + 196.0 * u, 30.0 * u, 32.0 * u, green);
        if d.wind_valid {
            c.text_l(&format!("{:03}/{}", imod(roundi(d.wind_dir), 360), roundi(d.wind_kt)), x0, 66.0 * u, 28.0 * u, green);
            self.wind_arrow(c, x0 + 26.0 * u, 112.0 * u, d, green);
        }
        if let Some(n) = self.next(d, route) {
            let xr = w - 16.0 * u;
            c.text_l(&n.id, xr - 170.0 * u, 30.0 * u, 30.0 * u, WHITE);
            c.text_r(&format!("{:03}°", imod(roundi(n.brg), 360)), xr, 30.0 * u, 30.0 * u, green);
            c.text_r(&format!("{:.1}", n.dist), xr - 50.0 * u, 66.0 * u, 30.0 * u, green);
            c.text_r("NM", xr, 66.0 * u, 24.0 * u, cyan);
            c.text_r(&ete(n.ete_sec), xr, 100.0 * u, 28.0 * u, green);
        }
        let n = &d.nav;
        if matches!(n.source, NavSource::Vor | NavSource::Loc) {
            c.text_l(if n.source == NavSource::Loc { "ILS1" } else { "VOR1" }, x0, h - 100.0 * u, 26.0 * u, WHITE);
            if let Some(i) = &n.ident {
                c.text_l(i, x0, h - 68.0 * u, 26.0 * u, 0xFFFF55FF);
            }
            if !n.dme_nm.is_nan() {
                c.text_l(&format!("{:.1} NM", n.dme_nm), x0 + 110.0 * u, h - 68.0 * u, 26.0 * u, green);
            }
        }
        if !d.brg_adf.is_nan() {
            c.text_r("ADF1", w - 16.0 * u, h - 100.0 * u, 26.0 * u, green);
        }
    }

    fn boeing_data(&self, c: &mut Canvas, d: &FlightData, w: f32, h: f32, route: &[RoutePt]) {
        let (u, sk) = (self.u, self.sk);
        let x0 = 16.0 * u;
        c.text_l("GS", x0, 30.0 * u, 26.0 * u, sk.label);
        c.text_l(&if d.gs_kt.is_nan() { "---".into() } else { roundi(d.gs_kt).to_string() }, x0 + 44.0 * u, 30.0 * u, 32.0 * u, WHITE);
        c.text_l("TAS", x0 + 124.0 * u, 30.0 * u, 26.0 * u, sk.label);
        c.text_l(&if d.tas_kt.is_nan() { "---".into() } else { roundi(d.tas_kt).to_string() }, x0 + 184.0 * u, 30.0 * u, 32.0 * u, WHITE);
        if d.wind_valid {
            c.text_l(&format!("{:03}°/{}", imod(roundi(d.wind_dir), 360), roundi(d.wind_kt)), x0, 66.0 * u, 28.0 * u, WHITE);
            self.wind_arrow(c, x0 + 26.0 * u, 112.0 * u, d, WHITE);
        }
        let top = if sk.heading_up { "HDG" } else { "TRK" };
        let v = imod(roundi(if sk.heading_up { d.heading } else { d.track }), 360);
        c.text_r(top, w / 2.0 - 60.0 * u, 30.0 * u, 26.0 * u, sk.label);
        c.rect(w / 2.0 - 50.0 * u, 10.0 * u, w / 2.0 + 50.0 * u, 50.0 * u, BLACK);
        c.frame(w / 2.0 - 50.0 * u, 10.0 * u, w / 2.0 + 50.0 * u, 50.0 * u, WHITE, 2.0 * u);
        c.text(&format!("{v:03}"), w / 2.0, 30.0 * u, 32.0 * u, WHITE);
        c.text_l("MAG", w / 2.0 + 60.0 * u, 30.0 * u, 24.0 * u, sk.label);
        if let Some(n) = self.next(d, route) {
            let xr = w - 16.0 * u;
            c.text_r(&n.id, xr, 30.0 * u, 30.0 * u, sk.to_wpt);
            c.text_r(&format!("ETE {}", ete(n.ete_sec)), xr, 66.0 * u, 26.0 * u, WHITE);
            c.text_r(&format!("{:.1} NM", n.dist), xr, 100.0 * u, 28.0 * u, WHITE);
        }
        let n = &d.nav;
        if matches!(n.source, NavSource::Vor | NavSource::Loc) {
            c.text_l(if n.source == NavSource::Loc { "ILS L" } else { "VOR L" }, x0, h - 100.0 * u, 26.0 * u, 0xFF3EEA4C);
            if let Some(i) = &n.ident {
                c.text_l(i, x0, h - 68.0 * u, 26.0 * u, WHITE);
            }
            if !n.dme_nm.is_nan() {
                c.text_l(&format!("DME {:.1}", n.dme_nm), x0, h - 36.0 * u, 24.0 * u, WHITE);
            }
        }
    }

    fn garmin_top(&self, c: &mut Canvas, d: &FlightData, w: f32, top: f32, route: &[RoutePt]) {
        let u = self.u;
        let x0 = w * 0.2;
        c.rect(x0, 0.0, w, top, 0xCC0A0F16);
        c.line(x0, top, w, top, 0xFF9AA0A6, 2.0 * u);
        let n = self.next(d, route);
        let fields = [
            ("GS", if d.gs_kt.is_nan() { "---KT".into() } else { format!("{}KT", roundi(d.gs_kt)) }),
            ("DTK", if d.gps.dtk.is_nan() { "---°".into() } else { format!("{:03}°", imod(roundi(d.gps.dtk), 360)) }),
            ("TRK", if d.has_track { format!("{:03}°", imod(roundi(d.track), 360)) } else { "---°".into() }),
            ("ETE", n.map(|n| ete(n.ete_sec)).unwrap_or_else(|| "--:--".into())),
        ];
        let fw = (w - x0) / fields.len() as f32;
        for (i, (k, v)) in fields.iter().enumerate() {
            let x = x0 + fw * i as f32 + 10.0 * u;
            c.text_l(k, x, top / 2.0, 22.0 * u, WHITE);
            c.text_l(v, x + 54.0 * u, top / 2.0, 28.0 * u, 0xFFFF33E6);
        }
    }

    fn eis(&self, c: &mut Canvas, d: &FlightData, x1: f32, h: f32) {
        let u = self.u;
        c.rect(0.0, 0.0, x1, h, 0xFF101418);
        c.line(x1, 0.0, x1, h, 0xFF9AA0A6, 2.0 * u);
        let e = d.engines.first().copied();
        let turbine = !d.piston;
        let mut y = 30.0 * u;
        let mut gauge = |c: &mut Canvas, label: &str, value: Option<f32>, max: f32, unit: &str, green: Option<(f32, f32)>| {
            c.text_l(label, 12.0 * u, y, 22.0 * u, WHITE);
            let txt = value.map(|v| if max >= 100.0 { format!("{}{unit}", roundi(v)) } else { format!("{v:.1}{unit}") }).unwrap_or_else(|| "---".into());
            c.text_r(&txt, x1 - 12.0 * u, y, 24.0 * u, WHITE);
            y += 26.0 * u;
            let (bx0, bx1) = (14.0 * u, x1 - 14.0 * u);
            c.rect(bx0, y - 6.0 * u, bx1, y + 6.0 * u, 0xFF2A3038);
            if let Some((g0, g1)) = green {
                c.rect(bx0 + g0 / max * (bx1 - bx0), y - 6.0 * u, bx0 + g1 / max * (bx1 - bx0), y + 6.0 * u, 0xFF1F8A2A);
            }
            if let Some(v) = value {
                let x = bx0 + (v / max).clamp(0.0, 1.0) * (bx1 - bx0);
                c.poly(WHITE, BLACK, 1.0, &[x, y + 8.0 * u, x - 8.0 * u, y - 10.0 * u, x + 8.0 * u, y - 10.0 * u]);
            }
            y += 34.0 * u;
        };
        if turbine {
            gauge(c, "N1", e.map(|e| e.n1), 110.0, "%", Some((20.0, 100.0)));
            gauge(c, "N2", e.map(|e| e.n2), 110.0, "%", Some((50.0, 102.0)));
            gauge(c, "ITT", e.map(|e| e.egt), 1000.0, "°C", Some((300.0, 800.0)));
        } else {
            gauge(c, "RPM", e.map(|e| e.rpm), 3000.0, "", Some((2100.0, 2700.0)));
            gauge(c, "EGT", e.map(|e| e.egt), 900.0, "°C", Some((500.0, 800.0)));
        }
        gauge(c, "FFLOW", e.map(|e| e.ff), if turbine { 1200.0 } else { 80.0 }, "", None);
        c.text_r("kg/h", x1 - 12.0 * u, y - 52.0 * u, 18.0 * u, 0xFF9AA0A6);
        c.text_l("FUEL QTY kg", 12.0 * u, y, 22.0 * u, WHITE);
        y += 30.0 * u;
        c.text_l(&format!("TOT {}", if d.fuel_kg.is_nan() { "--".into() } else { roundi(d.fuel_kg).to_string() }), 12.0 * u, y, 24.0 * u, 0xFF3FE07A);
        y += 40.0 * u;
        if let Some(f) = d.flaps_index {
            c.text_l(&format!("FLAPS {}/{}", f, d.flaps_positions.unwrap_or(1) - 1), 12.0 * u, y, 22.0 * u, WHITE);
            y += 30.0 * u;
        }
        if let Some(g) = d.gear_down {
            c.text_l(if g { "GEAR DN" } else { "GEAR UP" }, 12.0 * u, y, 22.0 * u, if g { 0xFF3FE07A } else { WHITE });
            y += 30.0 * u;
        }
        if !d.oat_c.is_nan() {
            c.text_l(&format!("OAT {}°C", roundi(d.oat_c)), 12.0 * u, y, 22.0 * u, WHITE);
        }
    }

    fn gns_column(&self, c: &mut Canvas, d: &FlightData, x0: f32, w: f32, h: f32, route: &[RoutePt]) {
        let u = self.u;
        let green = 0xFF5CE65C;
        c.rect(x0, 0.0, w, h, 0xFF041008);
        c.line(x0, 0.0, x0, h, green, 2.0 * u);
        let n = self.next(d, route);
        let rows = [
            ("GS", if d.gs_kt.is_nan() { "---kt".into() } else { format!("{}kt", roundi(d.gs_kt)) }),
            ("DTK", if d.gps.dtk.is_nan() { "---°".into() } else { format!("{:03}°", imod(roundi(d.gps.dtk), 360)) }),
            ("TRK", if d.has_track { format!("{:03}°", imod(roundi(d.track), 360)) } else { "---°".into() }),
            ("DIS", n.as_ref().map(|n| format!("{:.1}nm", n.dist)).unwrap_or_else(|| "---".into())),
            ("ETE", n.as_ref().map(|n| ete_min(n.ete_sec)).unwrap_or_else(|| "--:--".into())),
            ("BRG", n.as_ref().map(|n| if n.brg.is_nan() { "---°".into() } else { format!("{:03}°", imod(roundi(n.brg), 360)) }).unwrap_or_else(|| "---°".into())),
        ];
        let rh = (h - 80.0 * u) / rows.len() as f32;
        c.text(n.as_ref().map(|n| n.id.as_str()).unwrap_or("----"), (x0 + w) / 2.0, 40.0 * u, 32.0 * u, 0xFFFF33E6);
        for (i, (k, v)) in rows.iter().enumerate() {
            let y = 80.0 * u + rh * i as f32 + rh / 2.0;
            c.text_l(k, x0 + 12.0 * u, y - rh * 0.22, 20.0 * u, green);
            c.text_r(v, w - 12.0 * u, y + rh * 0.12, 30.0 * u, WHITE);
            c.line(x0 + 8.0 * u, y + rh / 2.0, w - 8.0 * u, y + rh / 2.0, 0xFF1C3A22, 1.5 * u);
        }
    }
}
