//! Widok 3D rysowany w Rust: podkład mapy (te same kafelki co mapa 2D) rozpięty na płaszczyźnie terenu
//! w perspektywie, niebo z mgłą przy horyzoncie, ślad lotu w kolorach wysokości z „kurtyną” do ziemi,
//! model samolotu z cieniowaniem i cieniem na ziemi, pasy startowe, lotniska, VOR-y i róża kompasowa
//! na ziemi wokół samolotu. Kamera krąży wokół celu (za samolotem, swobodnie albo z góry).

use std::sync::Arc;

use tiny_skia::{FilterQuality, Paint, PathBuilder, Pattern, Pixmap, SpreadMode, Transform};

use crate::avionics::gfx::{self, alpha, Align, Argb, Canvas};
use crate::map::{self, Style};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CamMode {
    /// Za samolotem: kierunek kamery względem kursu.
    Chase,
    /// Swobodna: kierunek bezwzględny.
    Free,
    /// Z góry (rzut prawie pionowy).
    Top,
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub mode: CamMode,
    /// Odchylenie kierunku patrzenia (°): w trybie Chase względem kursu, w Free względem północy.
    pub yaw: f64,
    /// Pochylenie w dół (°).
    pub pitch: f64,
    /// Odległość od celu (m).
    pub dist: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self { mode: CamMode::Chase, yaw: 0.0, pitch: 18.0, dist: 1800.0 }
    }
}

impl Camera {
    pub fn drag(&mut self, dx: f64, dy: f64) {
        self.yaw = (self.yaw + dx * 0.35).rem_euclid(360.0);
        if self.mode != CamMode::Top {
            self.pitch = (self.pitch + dy * 0.25).clamp(2.0, 89.0);
        }
    }
    pub fn zoom(&mut self, wheel: f64) {
        self.dist = (self.dist * 0.85f64.powf(wheel / 120.0)).clamp(40.0, 600_000.0);
    }
}

#[derive(Clone, Debug)]
pub struct Label3 {
    pub lat: f64,
    pub lon: f64,
    pub text: String,
    /// 0 lotnisko, 1 VOR, 2 NDB, 3 punkt trasy
    pub kind: u8,
}

pub struct Scene<'a> {
    pub style: Style,
    pub dark: bool,
    pub w: u32,
    pub h: u32,
    pub scale: f32,
    /// Cel kamery: szerokość, długość, wysokość MSL (ft).
    pub lat: f64,
    pub lon: f64,
    pub alt_ft: f64,
    /// Kurs rzeczywisty, pochylenie, przechylenie (°).
    pub hdg: f64,
    pub pitch: f64,
    pub bank: f64,
    pub ground_ft: f64,
    pub mag_var: f64,
    pub show_aircraft: bool,
    pub track: &'a [(f64, f64, i32)],
    pub runways: &'a [(f64, f64, f64, f64, f64)],
    pub points: &'a [Label3],
    pub route: &'a [(f64, f64)],
    pub hud: Vec<String>,
    pub compass: bool,
    pub curtain: bool,
}

#[derive(Clone, Copy, Debug)]
struct V3 {
    x: f64,
    y: f64,
    z: f64,
}

impl V3 {
    fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    fn sub(self, o: V3) -> V3 {
        V3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    fn add(self, o: V3) -> V3 {
        V3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    fn mul(self, k: f64) -> V3 {
        V3::new(self.x * k, self.y * k, self.z * k)
    }
    fn dot(self, o: V3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    fn cross(self, o: V3) -> V3 {
        V3::new(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }
    fn norm(self) -> V3 {
        let l = self.dot(self).sqrt().max(1e-9);
        self.mul(1.0 / l)
    }
}

const FT: f64 = 0.3048;
const NEAR: f64 = 2.0;

struct Proj {
    lat0: f64,
    lon0: f64,
    kx: f64,
    ky: f64,
    cam: V3,
    f: V3,
    r: V3,
    u: V3,
    focal: f64,
    cx: f64,
    cy: f64,
}

impl Proj {
    fn enu(&self, lat: f64, lon: f64, alt_m: f64) -> V3 {
        V3::new((lon - self.lon0) * self.kx, (lat - self.lat0) * self.ky, alt_m)
    }
    fn view(&self, p: V3) -> (f64, f64, f64) {
        let v = p.sub(self.cam);
        (v.dot(self.r), v.dot(self.u), v.dot(self.f))
    }
    fn screen(&self, xc: f64, yc: f64, zc: f64) -> (f32, f32) {
        ((self.cx + self.focal * xc / zc) as f32, (self.cy - self.focal * yc / zc) as f32)
    }
    fn project(&self, p: V3) -> Option<(f32, f32, f64)> {
        let (x, y, z) = self.view(p);
        if z < NEAR {
            return None;
        }
        let (sx, sy) = self.screen(x, y, z);
        Some((sx, sy, z))
    }
    /// Odcinek przycięty do płaszczyzny bliskiej.
    fn segment(&self, a: V3, b: V3) -> Option<((f32, f32), (f32, f32))> {
        let (ax, ay, az) = self.view(a);
        let (bx, by, bz) = self.view(b);
        if az < NEAR && bz < NEAR {
            return None;
        }
        let (mut a2, mut b2) = ((ax, ay, az), (bx, by, bz));
        if az < NEAR {
            let t = (NEAR - az) / (bz - az);
            a2 = (ax + (bx - ax) * t, ay + (by - ay) * t, NEAR);
        }
        if bz < NEAR {
            let t = (NEAR - bz) / (az - bz);
            b2 = (bx + (ax - bx) * t, by + (ay - by) * t, NEAR);
        }
        Some((self.screen(a2.0, a2.1, a2.2), self.screen(b2.0, b2.1, b2.2)))
    }
}

/// Afiniczne przekształcenie tekstury (u, v) → ekran z trzech par punktów.
fn affine(t: [(f32, f32); 3], s: [(f32, f32); 3]) -> Option<Transform> {
    let (du1, dv1) = (t[1].0 - t[0].0, t[1].1 - t[0].1);
    let (du2, dv2) = (t[2].0 - t[0].0, t[2].1 - t[0].1);
    let det = du1 * dv2 - du2 * dv1;
    if det.abs() < 1e-9 {
        return None;
    }
    let (dx1, dy1) = (s[1].0 - s[0].0, s[1].1 - s[0].1);
    let (dx2, dy2) = (s[2].0 - s[0].0, s[2].1 - s[0].1);
    let a = (dx1 * dv2 - dx2 * dv1) / det;
    let c = (dx2 * du1 - dx1 * du2) / det;
    let b = (dy1 * dv2 - dy2 * dv1) / det;
    let d = (dy2 * du1 - dy1 * du2) / det;
    let e = s[0].0 - a * t[0].0 - c * t[0].1;
    let f = s[0].1 - b * t[0].0 - d * t[0].1;
    Some(Transform::from_row(a, b, c, d, e, f))
}

fn tri_path(p: [(f32, f32); 3]) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(p[0].0, p[0].1);
    pb.line_to(p[1].0, p[1].1);
    pb.line_to(p[2].0, p[2].1);
    pb.close();
    pb.finish()
}

fn mix(a: Argb, b: Argb, t: f32) -> Argb {
    let t = t.clamp(0.0, 1.0);
    let ch = |s: u32| ((((a >> s) & 0xFF) as f32) * (1.0 - t) + (((b >> s) & 0xFF) as f32) * t) as u32;
    (0xFF << 24) | (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

fn shade(c: Argb, k: f32) -> Argb {
    let ch = |s: u32| ((((c >> s) & 0xFF) as f32) * k).clamp(0.0, 255.0) as u32;
    (c & 0xFF00_0000) | (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

fn rgba(c: [u8; 4]) -> Argb {
    ((c[3] as u32) << 24) | ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
}

pub fn render(sc: &Scene, cam: &Camera) -> slint::Image {
    let mut c = Canvas::new(sc.w, sc.h);
    draw(&mut c, sc, cam);
    c.into_image()
}

pub fn draw(c: &mut Canvas, sc: &Scene, cam: &Camera) {
    let (w, h) = (c.width() as f64, c.height() as f64);
    let s = sc.scale;
    let ground = sc.ground_ft * FT;
    let lat0 = sc.lat;
    let lon0 = sc.lon;
    let kx = 111_320.0 * lat0.to_radians().cos();
    let ky = 110_574.0;
    let target_alt = (sc.alt_ft * FT).max(ground + 1.0);
    let (yaw, pitch) = match cam.mode {
        CamMode::Chase => ((sc.hdg + cam.yaw).rem_euclid(360.0), cam.pitch),
        CamMode::Free => (cam.yaw, cam.pitch),
        CamMode::Top => (cam.yaw, 88.5),
    };
    let (yr, pr) = (yaw.to_radians(), pitch.to_radians());
    let f = V3::new(yr.sin() * pr.cos(), yr.cos() * pr.cos(), -pr.sin());
    let target = V3::new(0.0, 0.0, target_alt);
    let mut cam_pos = target.sub(f.mul(cam.dist));
    if cam_pos.z < ground + 8.0 {
        cam_pos.z = ground + 8.0;
    }
    let r = V3::new(yr.cos(), -yr.sin(), 0.0);
    let u = r.cross(f).norm();
    let fov = 55f64.to_radians();
    let p = Proj { lat0, lon0, kx, ky, cam: cam_pos, f, r, u, focal: (h / 2.0) / (fov / 2.0).tan(), cx: w / 2.0, cy: h / 2.0 };

    // ---------- niebo ----------
    let horizon = {
        let dir = V3::new(yr.sin(), yr.cos(), 0.0);
        let dz = dir.dot(f);
        if dz > 1e-6 {
            (p.cy - p.focal * dir.dot(u) / dz) as f32
        } else {
            -1.0e4
        }
    };
    let (sky_top, sky_hor, haze, ground_col) = if sc.dark { (0xFF060B16, 0xFF2A3A55, 0xFF33405A, 0xFF1A1F26) } else { (0xFF2C6BC4, 0xFFB9D3EE, 0xFFC8D8EA, 0xFF7F8C7A) };
    let (wf, hf) = (w as f32, h as f32);
    c.rect(0.0, 0.0, wf, hf, ground_col);
    if horizon > 0.0 {
        c.rect_vgrad(0.0, 0.0, wf, horizon.min(hf), 0.0, sky_top, horizon.max(1.0), sky_hor);
    }

    // ---------- teren z kafelków ----------
    let view_m = (cam.dist + (target_alt - ground).abs() * 0.5).max(150.0);
    let world_m = 40_075_016.0 * lat0.to_radians().cos().max(0.05);
    let zf = ((world_m / (view_m * 0.55)).log2().round() as i32).clamp(3, sc.style_max() as i32);
    let levels: Vec<(i32, i32)> = vec![(zf - 6, 3), (zf - 3, 4), (zf, 4)];
    let paint_q = FilterQuality::Bilinear;
    for (z, rad) in levels {
        if z < 1 {
            continue;
        }
        let z = z as u32;
        let n = 1i64 << z;
        let (tx, ty) = map::project(lat0, lon0, z as f64);
        let (ctx, cty) = ((tx / 256.0).floor() as i64, (ty / 256.0).floor() as i64);
        for j in cty - rad as i64..=cty + rad as i64 {
            if j < 0 || j >= n {
                continue;
            }
            for i in ctx - rad as i64..=ctx + rad as i64 {
                let xi = i.rem_euclid(n) as u32;
                let near_tile = (i - ctx).abs() <= 1 && (j - cty).abs() <= 1;
                let k = if near_tile { 8 } else { 4 };
                // Wierzchołki siatki kafelka
                let mut grid: Vec<Option<(f32, f32)>> = Vec::with_capacity((k + 1) * (k + 1));
                for gy in 0..=k {
                    for gx in 0..=k {
                        let px = (i as f64 + gx as f64 / k as f64) * 256.0;
                        let py = (j as f64 + gy as f64 / k as f64) * 256.0;
                        let (la, lo) = map::unproject(px, py, z as f64);
                        grid.push(p.project(p.enu(la, lo, ground)).map(|(x, y, _)| (x, y)));
                    }
                }
                // Cały kafelek poza ekranem – pomiń (bez pobierania)
                let vis = grid.iter().flatten().any(|(x, y)| *x > -wf * 0.5 && *x < wf * 1.5 && *y > horizon - 4.0 && *y < hf * 1.5);
                if !vis {
                    continue;
                }
                let tex = map::texture(sc.style, z, xi, j as u32);
                for gy in 0..k {
                    for gx in 0..k {
                        let ix = |a: usize, b: usize| a * (k + 1) + b;
                        let (Some(a), Some(b), Some(cc), Some(d)) = (grid[ix(gy, gx)], grid[ix(gy, gx + 1)], grid[ix(gy + 1, gx + 1)], grid[ix(gy + 1, gx)]) else { continue };
                        let minx = a.0.min(b.0).min(cc.0).min(d.0);
                        let maxx = a.0.max(b.0).max(cc.0).max(d.0);
                        let miny = a.1.min(b.1).min(cc.1).min(d.1);
                        let maxy = a.1.max(b.1).max(cc.1).max(d.1);
                        if maxx < 0.0 || minx > wf || maxy < 0.0 || miny > hf {
                            continue;
                        }
                        match &tex {
                            Some((pm, ox, oy, size)) => {
                                let t = |gx: usize, gy: usize| (ox + size * gx as f32 / k as f32, oy + size * gy as f32 / k as f32);
                                let (ta, tb, tc, td) = (t(gx, gy), t(gx + 1, gy), t(gx + 1, gy + 1), t(gx, gy + 1));
                                fill_tex(c, pm, [ta, tb, tc], [a, b, cc], paint_q);
                                fill_tex(c, pm, [ta, tc, td], [a, cc, d], paint_q);
                            }
                            None => {
                                c.poly(ground_col, 0, 0.0, &[a.0, a.1, b.0, b.1, cc.0, cc.1, d.0, d.1]);
                            }
                        }
                    }
                }
            }
        }
    }
    // Mgła przy horyzoncie
    if horizon > -hf && horizon < hf {
        let band = hf * 0.22;
        c.rect_vgrad(0.0, horizon - 2.0, wf, horizon + band, horizon, alpha(haze, 0xE6), horizon + band, alpha(haze, 0));
        c.rect_vgrad(0.0, horizon - hf * 0.08, wf, horizon + 1.0, horizon - hf * 0.08, alpha(sky_hor, 0), horizon, alpha(haze, 0xFF));
    }

    // ---------- pasy ----------
    for (la0, lo0, la1, lo1, wid_ft) in sc.runways {
        let a = p.enu(*la0, *lo0, ground);
        let b = p.enu(*la1, *lo1, ground);
        let dir = b.sub(a);
        let len = (dir.x * dir.x + dir.y * dir.y).sqrt().max(1.0);
        let n = V3::new(-dir.y / len, dir.x / len, 0.0).mul((wid_ft * FT).max(25.0) / 2.0);
        let q = [a.add(n), b.add(n), b.sub(n), a.sub(n)];
        let pts: Vec<(f32, f32)> = q.iter().filter_map(|v| p.project(*v).map(|(x, y, _)| (x, y))).collect();
        if pts.len() == 4 {
            c.poly(if sc.dark { 0xFF3A3D44 } else { 0xFF4A4D55 }, 0xFFE8E8E8, 1.0 * s, &pts.iter().flat_map(|(x, y)| [*x, *y]).collect::<Vec<_>>());
            if let Some((p0, p1)) = p.segment(a, b) {
                c.dash_line(p0.0, p0.1, p1.0, p1.1, 0xFFFFFFFF, 1.2 * s, 8.0 * s, 6.0 * s);
            }
        }
    }

    // ---------- róża kompasu na ziemi wokół samolotu ----------
    if sc.compass && sc.show_aircraft {
        let rr = (cam.dist * 0.42).clamp(60.0, 80_000.0);
        let ink = if sc.dark { 0xDDE8F0FF } else { 0xEEFFFFFF };
        let mut prev: Option<V3> = None;
        for d in (0..=360).step_by(5) {
            let a = (d as f64 + sc.mag_var).to_radians();
            let pt = V3::new(a.sin() * rr, a.cos() * rr, ground);
            if let Some(pp) = prev {
                if let Some((p0, p1)) = p.segment(pp, pt) {
                    c.line(p0.0, p0.1, p1.0, p1.1, alpha(ink, 0xAA), 1.6 * s);
                }
            }
            prev = Some(pt);
            if d < 360 {
                let len = if d % 30 == 0 { 0.12 } else if d % 10 == 0 { 0.07 } else { 0.035 };
                let inner = V3::new(a.sin() * rr * (1.0 - len), a.cos() * rr * (1.0 - len), ground);
                if let Some((p0, p1)) = p.segment(inner, pt) {
                    c.line(p0.0, p0.1, p1.0, p1.1, ink, if d % 30 == 0 { 2.2 } else { 1.2 } * s);
                }
                if d % 30 == 0 {
                    let lbl = match d {
                        0 => "N".to_string(),
                        90 => "E".to_string(),
                        180 => "S".to_string(),
                        270 => "W".to_string(),
                        _ => (d / 10).to_string(),
                    };
                    let lp = V3::new(a.sin() * rr * 0.8, a.cos() * rr * 0.8, ground);
                    if let Some((x, y, _)) = p.project(lp) {
                        c.text_halo(&lbl, x, y, 14.0 * s, if d == 0 { 0xFFFF5A5F } else { 0xFFFFFFFF }, 0xB0000000, Align::Center, true);
                    }
                }
            }
        }
        // kurs na ziemi
        let a = sc.hdg.to_radians();
        let tip = V3::new(a.sin() * rr, a.cos() * rr, ground);
        if let Some((p0, p1)) = p.segment(V3::new(0.0, 0.0, ground), tip) {
            c.line(p0.0, p0.1, p1.0, p1.1, 0xFFFFD646, 2.0 * s);
        }
    }

    // ---------- trasa GPS ----------
    if sc.route.len() > 1 {
        for wseg in sc.route.windows(2) {
            let a = p.enu(wseg[0].0, wseg[0].1, ground + 2.0);
            let b = p.enu(wseg[1].0, wseg[1].1, ground + 2.0);
            if let Some((p0, p1)) = p.segment(a, b) {
                c.dash_line(p0.0, p0.1, p1.0, p1.1, 0xE6FF33E6, 2.4 * s, 10.0 * s, 7.0 * s);
            }
        }
    }

    // ---------- punkty (lotniska, VOR, NDB) ----------
    let mut pts: Vec<(f64, &Label3, f32, f32)> = sc
        .points
        .iter()
        .filter_map(|l| {
            let v = p.enu(l.lat, l.lon, ground);
            p.project(v).map(|(x, y, z)| (z, l, x, y))
        })
        .filter(|(_, _, x, y)| *x > -50.0 && *x < wf + 50.0 && *y > horizon && *y < hf + 50.0)
        .collect();
    pts.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (z, l, x, y) in pts.iter().rev().take(60).rev() {
        let fade = ((1.0 - (*z / (cam.dist * 30.0 + 40_000.0))).clamp(0.25, 1.0) * 255.0) as u8;
        let col = match l.kind {
            0 => 0xFF5B8CFF,
            1 => 0xFF2EC4D6,
            2 => 0xFF9ACD32,
            _ => 0xFFFF33E6,
        };
        let r = (5.0 * s).max(3.0);
        match l.kind {
            1 => {
                let hex: Vec<f32> = (0..6).flat_map(|k| {
                    let a = (60.0 * k as f32).to_radians();
                    [x + r * 1.3 * a.cos(), y + r * 1.3 * a.sin() * 0.6]
                }).collect();
                c.poly(alpha(0xFF0A1218, 0xB0), alpha(col, fade), 1.8 * s, &hex);
                c.circle(*x, *y, 1.6 * s, alpha(col, fade), 0.0);
            }
            2 => {
                c.circle(*x, *y, r, alpha(col, fade), 1.5 * s);
                c.circle(*x, *y, r * 0.35, alpha(col, fade), 0.0);
            }
            _ => {
                c.circle(*x, *y, r, alpha(0xFF0A1218, 0xB0), 0.0);
                c.circle(*x, *y, r, alpha(col, fade), 2.0 * s);
            }
        }
        c.text_halo(&l.text, x + r + 4.0 * s, *y - r, 12.0 * s, alpha(0xFFFFFFFF, fade), alpha(0xFF000000, (fade as f32 * 0.7) as u8), Align::Left, l.kind == 0);
    }

    // ---------- ślad ----------
    let track = sc.track;
    if track.len() > 1 {
        let step = (track.len() / 1500).max(1);
        let pts: Vec<&(f64, f64, i32)> = track.iter().step_by(step).chain(track.last()).collect();
        let max_alt = pts.iter().map(|t| t.2).max().unwrap_or(1000);
        let v3 = |t: &(f64, f64, i32)| p.enu(t.0, t.1, (t.2 as f64 * FT).max(ground));
        if sc.curtain {
            for wseg in pts.windows(2) {
                let (a, b) = (v3(wseg[0]), v3(wseg[1]));
                let (ag, bg) = (V3::new(a.x, a.y, ground), V3::new(b.x, b.y, ground));
                let q = [a, b, bg, ag].map(|v| p.project(v));
                if q.iter().all(|x| x.is_some()) {
                    let q: Vec<f32> = q.iter().flatten().flat_map(|(x, y, _)| [*x, *y]).collect();
                    let col = rgba(map::alt_color((wseg[0].2 + wseg[1].2) / 2, max_alt));
                    c.poly(alpha(col, 0x38), 0, 0.0, &q);
                }
            }
        }
        for wseg in pts.windows(2) {
            if let Some((p0, p1)) = p.segment(v3(wseg[0]), v3(wseg[1])) {
                let col = rgba(map::alt_color((wseg[0].2 + wseg[1].2) / 2, max_alt));
                c.line_round(p0.0, p0.1, p1.0, p1.1, 0xA0000000, 4.6 * s);
                c.line_round(p0.0, p0.1, p1.0, p1.1, col, 2.8 * s);
            }
        }
    }

    // ---------- samolot ----------
    if sc.show_aircraft {
        let half_len_px = 0.06 * h;
        let scale = (half_len_px * cam.dist / (p.focal * 18.0)).max(1.0);
        aircraft(c, &p, sc, scale, ground, target_alt);
    }

    // ---------- HUD ----------
    if !sc.hud.is_empty() {
        let fs = 13.0 * s;
        let lh = fs * 1.45;
        let bw = sc.hud.iter().map(|l| gfx::text_width(l, fs, gfx::Face::Cond)).fold(0.0, f32::max) + 20.0 * s;
        let bh = lh * sc.hud.len() as f32 + 12.0 * s;
        c.round_rect(10.0 * s, 10.0 * s, 10.0 * s + bw, 10.0 * s + bh, 6.0 * s, 0xA0101418, 0x40FFFFFF, 1.0);
        for (i, l) in sc.hud.iter().enumerate() {
            c.text_l(l, 20.0 * s, 10.0 * s + 6.0 * s + lh * (i as f32 + 0.5), fs, if i == 0 { 0xFFFFFFFF } else { 0xFFC8D0DA });
        }
    }
    // Strzałka północy w rogu
    {
        let (nx, ny, nr) = (wf - 34.0 * s, hf - 34.0 * s, 20.0 * s);
        c.circle(nx, ny, nr, 0xA0101418, 0.0);
        c.circle(nx, ny, nr, 0x50FFFFFF, 1.0);
        c.save();
        c.rotate(-yaw as f32, nx, ny);
        c.poly(0xFFFF5A5F, 0, 0.0, &[nx, ny - nr * 0.8, nx - nr * 0.3, ny, nx + nr * 0.3, ny]);
        c.poly(0xFFE8E8E8, 0, 0.0, &[nx, ny + nr * 0.8, nx - nr * 0.3, ny, nx + nr * 0.3, ny]);
        c.text_halo("N", nx, ny - nr * 1.25, 10.0 * s, 0xFFFFFFFF, 0xA0000000, Align::Center, true);
        c.restore();
    }
}

impl Scene<'_> {
    fn style_max(&self) -> u32 {
        match self.style {
            Style::Dark | Style::Light => 16,
            Style::Satellite => 18,
            Style::Osm => 19,
        }
    }
}

fn fill_tex(c: &mut Canvas, pm: &Arc<Pixmap>, t: [(f32, f32); 3], s: [(f32, f32); 3], q: FilterQuality) {
    let Some(m) = affine(t, s) else { return };
    let Some(path) = tri_path(s) else { return };
    let mut paint = Paint::default();
    paint.anti_alias = false;
    paint.shader = Pattern::new(pm.as_ref().as_ref(), SpreadMode::Pad, q, 1.0, m);
    c.pm.fill_path(&path, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
}

/// Model samolotu (w metrach: x w prawo, y do przodu, z w górę), cieniowany i posortowany od tyłu.
fn aircraft(c: &mut Canvas, p: &Proj, sc: &Scene, scale: f64, ground: f64, alt: f64) {
    let body = 0xFFF2F4F7;
    let wing = 0xFFC9CED6;
    let fin = 0xFF5B8CFF;
    let ring = |y: f64, rw: f64, rz: f64, zc: f64| [V3::new(0.0, y, zc + rz), V3::new(rw, y, zc), V3::new(0.0, y, zc - rz), V3::new(-rw, y, zc)];
    let nose = V3::new(0.0, 18.0, -0.2);
    let r1 = ring(12.0, 1.9, 1.9, 0.0);
    let r2 = ring(-9.0, 1.9, 1.9, 0.2);
    let tail = V3::new(0.0, -18.0, 1.3);
    let mut tris: Vec<([V3; 3], Argb)> = Vec::new();
    for i in 0..4 {
        let j = (i + 1) % 4;
        tris.push(([nose, r1[i], r1[j]], body));
        tris.push(([r1[i], r2[i], r2[j]], body));
        tris.push(([r1[i], r2[j], r1[j]], body));
        tris.push(([r2[i], tail, r2[j]], body));
    }
    for sgn in [-1.0, 1.0] {
        let q = [V3::new(1.6 * sgn, 5.0, -0.6), V3::new(17.0 * sgn, -4.0, 0.9), V3::new(17.0 * sgn, -6.8, 0.9), V3::new(1.6 * sgn, -4.5, -0.6)];
        tris.push(([q[0], q[1], q[2]], wing));
        tris.push(([q[0], q[2], q[3]], wing));
        let t = [V3::new(0.6 * sgn, -13.0, 1.0), V3::new(6.5 * sgn, -16.2, 1.3), V3::new(6.5 * sgn, -18.0, 1.3), V3::new(0.6 * sgn, -17.5, 1.0)];
        tris.push(([t[0], t[1], t[2]], wing));
        tris.push(([t[0], t[2], t[3]], wing));
        // silniki pod skrzydłami
        let ex = 6.0 * sgn;
        let front = V3::new(ex, 4.5, -1.6);
        let e = [V3::new(ex, 2.0, -0.7), V3::new(ex + 0.9, 2.0, -1.6), V3::new(ex, 2.0, -2.5), V3::new(ex - 0.9, 2.0, -1.6)];
        let eb = V3::new(ex, -1.5, -1.6);
        for i in 0..4 {
            let j = (i + 1) % 4;
            tris.push(([front, e[i], e[j]], 0xFF9AA3AE));
            tris.push(([e[i], eb, e[j]], 0xFF7A838E));
        }
    }
    let f = [V3::new(0.0, -11.5, 1.6), V3::new(0.0, -18.0, 1.4), V3::new(0.0, -18.6, 8.2), V3::new(0.0, -15.8, 8.2)];
    tris.push(([f[0], f[1], f[2]], fin));
    tris.push(([f[0], f[2], f[3]], fin));

    let (h, pt, b) = (sc.hdg.to_radians(), sc.pitch.to_radians(), sc.bank.to_radians());
    let rot = |v: V3| -> V3 {
        let v = v.mul(scale);
        // przechylenie wokół osi podłużnej (prawe skrzydło w dół przy dodatnim)
        let (x, z) = (v.x * b.cos() + v.z * b.sin(), -v.x * b.sin() + v.z * b.cos());
        let v = V3::new(x, v.y, z);
        // pochylenie (nos w górę)
        let (y, z) = (v.y * pt.cos() - v.z * pt.sin(), v.y * pt.sin() + v.z * pt.cos());
        let v = V3::new(v.x, y, z);
        // kurs (zgodnie z ruchem wskazówek od północy)
        V3::new(v.x * h.cos() + v.y * h.sin(), -v.x * h.sin() + v.y * h.cos(), v.z + alt)
    };
    let world: Vec<([V3; 3], Argb)> = tris.iter().map(|(t, col)| ([rot(t[0]), rot(t[1]), rot(t[2])], *col)).collect();

    // cień na ziemi i linia pionu
    let agl = alt - ground;
    if agl < 8000.0 {
        let k = (1.0 - agl / 8000.0).clamp(0.15, 1.0);
        for (t, _) in &world {
            let sh: Vec<f32> = t.iter().filter_map(|v| p.project(V3::new(v.x, v.y, ground + 0.3)).map(|(x, y, _)| [x, y])).flatten().collect();
            if sh.len() == 6 {
                c.poly(alpha(0xFF000000, (70.0 * k) as u8), 0, 0.0, &sh);
            }
        }
    }
    if let Some((p0, p1)) = p.segment(V3::new(0.0, 0.0, alt), V3::new(0.0, 0.0, ground)) {
        c.dash_line(p0.0, p0.1, p1.0, p1.1, 0xB0FFFFFF, 1.2 * sc.scale, 5.0 * sc.scale, 4.0 * sc.scale);
        c.circle(p1.0, p1.1, 3.0 * sc.scale, 0xC0FFD646, 0.0);
    }

    let light = V3::new(-0.35, 0.45, 0.82).norm();
    let mut drawn: Vec<(f64, Vec<f32>, Argb)> = Vec::new();
    for (t, col) in &world {
        let n = t[1].sub(t[0]).cross(t[2].sub(t[0])).norm();
        let k = 0.55 + 0.45 * n.dot(light).abs() as f32;
        let mut depth = 0.0;
        let mut pts = Vec::with_capacity(6);
        let mut ok = true;
        for v in t {
            match p.project(*v) {
                Some((x, y, z)) => {
                    pts.push(x);
                    pts.push(y);
                    depth += z;
                }
                None => ok = false,
            }
        }
        if ok {
            drawn.push((depth, pts, shade(*col, k)));
        }
    }
    drawn.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, pts, col) in &drawn {
        c.poly(*col, mix(*col, 0xFF000000, 0.25), 0.6, pts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_3d_to_png() {
        let Ok(dir) = std::env::var("OT_RENDER_DIR") else { return };
        let track: Vec<(f64, f64, i32)> = (0..200).map(|i| (52.165 + i as f64 * 0.0012, 20.967 - i as f64 * 0.0025, (i * 40) as i32)).collect();
        let last = *track.last().unwrap();
        let rw = [(52.1716, 20.9586, 52.1489, 20.9813, 197.0)];
        let pts = vec![Label3 { lat: 52.1657, lon: 20.9671, text: "EPWA".into(), kind: 0 }, Label3 { lat: 52.20, lon: 20.85, text: "WAR".into(), kind: 1 }];
        for (name, cam) in [
            ("chase", Camera::default()),
            ("free", Camera { mode: CamMode::Free, yaw: 60.0, pitch: 30.0, dist: 9000.0 }),
            ("top", Camera { mode: CamMode::Top, yaw: 0.0, pitch: 88.0, dist: 20000.0 }),
        ] {
            let sc = Scene {
                style: Style::Satellite,
                dark: false,
                w: 1100,
                h: 650,
                scale: 1.0,
                lat: last.0,
                lon: last.1,
                alt_ft: last.2 as f64,
                hdg: 305.0,
                pitch: 6.0,
                bank: -15.0,
                ground_ft: 330.0,
                mag_var: 6.0,
                show_aircraft: true,
                track: &track,
                runways: &rw,
                points: &pts,
                route: &[],
                hud: vec!["SP-LWA · B738".into(), "ALT 7960 ft · GS 210 kt".into()],
                compass: true,
                curtain: true,
            };
            for _ in 0..40 {
                let mut c = Canvas::new(1, 1);
                c.pm = Pixmap::new(sc.w, sc.h).unwrap();
                draw(&mut c, &sc, &cam);
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            let mut c = Canvas::new(sc.w, sc.h);
            let t = std::time::Instant::now();
            draw(&mut c, &sc, &cam);
            eprintln!("{name}: {:?}", t.elapsed());
            c.pm.save_png(format!("{dir}/3d_{name}.png")).unwrap();
        }
    }
}
