//! Zegary analogowe – przeniesione z CessnaPanel.kt: „six-pack” Cessny 172 (prędkościomierz, sztuczny
//! horyzont, wysokościomierz, zakrętomierz, żyrobusola, wariometr), śmigłowca (radiowysokościomierz)
//! i szybowca (wariometr TE, G-metr), a w Cessnie także wskaźnik VOR/ILS (CDI z belką ścieżki schodzenia).
//! Każdy zegar rysowany w układzie o promieniu 100.

use std::time::Instant;

use super::data::FlightData;
use super::gfx::*;

const FACE: Argb = 0xFF0E0E0F;
const BEZEL: Argb = 0xFF050505;
const RIM: Argb = 0xFF3C3E42;
const ORANGE: Argb = 0xFFFF8A00;
const GREEN: Argb = 0xFF1FA82E;
const YEL: Argb = 0xFFF2D21B;
const GRED: Argb = 0xFFE02020;
const SKY: Argb = 0xFF2F7FCF;
const EARTH: Argb = 0xFF6E4320;
const LABEL: Argb = 0xFFD8D8D8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Cessna,
    Heli,
    Glider,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum G {
    Asi,
    Ai,
    Alt,
    Tc,
    Hi,
    Vsi,
    Vario,
    GMeter,
    Ra,
    Cdi,
}

#[derive(Clone, Copy)]
struct Arc {
    from: f32,
    to: f32,
    r: f32,
    w: f32,
    color: Argb,
}

#[derive(Clone)]
struct AsiCfg {
    unit: &'static str,
    factor: f32,
    max: f32,
    first_label: i32,
    label_step: i32,
    arcs: Vec<Arc>,
    red: f32,
}

pub struct Panel {
    kind: Kind,
    gauges: Vec<G>,
    asi_cfg: AsiCfg,
    cfg_now: AsiCfg,
    g_max: f32,
    g_min: f32,
    last_hdg: f32,
    last_t: Option<Instant>,
    turn_rate: f32,
    avg_te: f32,
    last_vario: Option<Instant>,
}

impl Panel {
    pub fn new(kind: Kind) -> Self {
        let gauges = match kind {
            Kind::Cessna => vec![G::Asi, G::Ai, G::Alt, G::Cdi, G::Tc, G::Hi, G::Vsi],
            Kind::Heli => vec![G::Asi, G::Ai, G::Alt, G::Ra, G::Hi, G::Vsi],
            Kind::Glider => vec![G::Asi, G::Vario, G::Alt, G::Tc, G::Hi, G::GMeter],
        };
        let asi_cfg = match kind {
            Kind::Cessna => AsiCfg {
                unit: "KNOTS",
                factor: 1.0,
                max: 200.0,
                first_label: 40,
                label_step: 20,
                arcs: vec![Arc { from: 33.0, to: 85.0, r: 86.0, w: 5.0, color: 0xFFF0F0F0 }, Arc { from: 44.0, to: 129.0, r: 80.0, w: 7.0, color: GREEN }, Arc { from: 129.0, to: 163.0, r: 80.0, w: 7.0, color: YEL }],
                red: 163.0,
            },
            Kind::Heli => AsiCfg { unit: "KNOTS", factor: 1.0, max: 150.0, first_label: 20, label_step: 20, arcs: vec![Arc { from: 0.0, to: 130.0, r: 80.0, w: 7.0, color: GREEN }], red: 130.0 },
            Kind::Glider => AsiCfg {
                unit: "KM/H",
                factor: 1.852,
                max: 300.0,
                first_label: 40,
                label_step: 20,
                arcs: vec![Arc { from: 70.0, to: 85.0, r: 86.0, w: 5.0, color: 0xFFF0F0F0 }, Arc { from: 75.0, to: 180.0, r: 80.0, w: 7.0, color: GREEN }, Arc { from: 180.0, to: 280.0, r: 80.0, w: 7.0, color: YEL }],
                red: 280.0,
            },
        };
        Self { kind, gauges, cfg_now: asi_cfg.clone(), asi_cfg, g_max: 1.0, g_min: 1.0, last_hdg: f32::NAN, last_t: None, turn_rate: 0.0, avg_te: 0.0, last_vario: None }
    }

    pub fn draw(&mut self, c: &mut Canvas, d: &FlightData, w: f32, h: f32) {
        self.update_turn_rate(d);
        if let Some(p) = rect_path(0.0, 0.0, w, h) {
            c.fill_radial(&p, w / 2.0, h / 2.0, w.max(h) * 0.75, 0xFF34373B, 0xFF1A1B1D);
        }
        let hb = (h * 0.055).clamp(26.0, 64.0);
        let n = self.gauges.len();
        let cols = if w >= h * 1.6 { if n > 6 { 4 } else { 3 } } else if w >= h { 3 } else { 2 };
        let rows = n.div_ceil(cols);
        let cw = w / cols as f32;
        let ch = (h - hb) / rows as f32;
        let r = cw.min(ch) * 0.46;
        for i in 0..n {
            let cx = cw * (i % cols) as f32 + cw / 2.0;
            let cy = ch * (i / cols) as f32 + ch / 2.0;
            c.save();
            c.translate(cx, cy);
            c.scale(r / 100.0, r / 100.0);
            bezel_back(c);
            match self.gauges[i] {
                G::Asi => self.asi(c, d),
                G::Ai => ai(c, d),
                G::Alt => alt(c, d, self.kind == Kind::Glider),
                G::Tc => self.tc(c, d),
                G::Hi => hi(c, d),
                G::Vsi => vsi(c, d),
                G::Vario => self.vario(c, d),
                G::GMeter => self.gmeter(c, d),
                G::Ra => radio_alt(c, d),
                G::Cdi => cdi(c, d),
            }
            bezel_front(c);
            c.restore();
            screws(c, cx, cy, r);
        }
        status_bar(c, d, w, h, hb);
    }

    fn update_turn_rate(&mut self, d: &FlightData) {
        if !d.turn_rate.is_nan() {
            self.turn_rate = d.turn_rate;
            return;
        }
        let now = Instant::now();
        if let (false, Some(t)) = (self.last_hdg.is_nan(), self.last_t) {
            let dt = (now - t).as_secs_f32();
            if (0.001..=1.0).contains(&dt) {
                let rate = ang_diff(d.heading, self.last_hdg) / dt;
                self.turn_rate += dt / (0.8 + dt) * (rate - self.turn_rate);
            }
        }
        self.last_hdg = d.heading;
        self.last_t = Some(now);
    }

    fn asi_angle(&self, v: f32) -> f32 {
        20.0 + v.clamp(0.0, self.cfg_now.max) / self.cfg_now.max * 320.0
    }

    fn cfg_for(&self, d: &FlightData) -> AsiCfg {
        if d.vs1.is_nan() || d.vc.is_nan() || self.kind == Kind::Heli {
            return self.asi_cfg.clone();
        }
        let f = self.asi_cfg.factor;
        let vs0 = (if d.vs0.is_nan() { d.vs1 * 0.85 } else { d.vs0 }) * f;
        let vs1 = d.vs1 * f;
        let vno = d.vc * f;
        let vne = (if d.vmo.is_nan() { d.vc * 1.27 } else { d.vmo }) * f;
        let max = ((vne * 1.2 / 20.0).ceil() * 20.0).max(self.asi_cfg.max);
        AsiCfg {
            max,
            red: vne,
            arcs: vec![Arc { from: vs0, to: (vs0 * 2.1).min(vno), r: 86.0, w: 5.0, color: 0xFFF0F0F0 }, Arc { from: vs1, to: vno, r: 80.0, w: 7.0, color: GREEN }, Arc { from: vno, to: vne, r: 80.0, w: 7.0, color: YEL }],
            ..self.asi_cfg.clone()
        }
    }

    fn asi(&mut self, c: &mut Canvas, d: &FlightData) {
        self.cfg_now = self.cfg_for(d);
        let cfg = self.cfg_now.clone();
        for a in &cfg.arcs {
            arc_band(c, self.asi_angle(a.from), self.asi_angle(a.to), a.r, a.w, a.color);
        }
        c.radial(0.0, 0.0, self.asi_angle(cfg.red), 70.0, 90.0, GRED, 4.0);
        let minor = if cfg.max > 200.0 { 10 } else { 5 };
        let mut v = 0;
        while v as f32 <= cfg.max {
            let major = v % (minor * 2) == 0;
            if v >= cfg.first_label - minor || self.kind == Kind::Heli {
                c.radial(0.0, 0.0, self.asi_angle(v as f32), if major { 72.0 } else { 78.0 }, 90.0, WHITE, if major { 2.5 } else { 1.5 });
            }
            if v >= cfg.first_label && (v - cfg.first_label) % cfg.label_step == 0 {
                let a = self.asi_angle(v as f32).to_radians();
                c.text_b(&v.to_string(), a.sin() * 58.0, -a.cos() * 58.0, if cfg.max > 200.0 { 12.0 } else { 15.0 }, WHITE);
            }
            v += minor;
        }
        c.text("AIRSPEED", 0.0, -26.0, 10.0, LABEL);
        c.text(cfg.unit, 0.0, 28.0, 10.0, LABEL);
        c.rect(-34.0, 40.0, 34.0, 64.0, BLACK);
        c.frame(-34.0, 40.0, 34.0, 64.0, 0xFF707070, 1.5);
        if d.speed_valid {
            c.text_b(&roundi(d.speed_kt * cfg.factor).to_string(), 0.0, 52.0, 16.0, WHITE);
        } else {
            c.text("NO DATA", 0.0, 52.0, 11.0, ORANGE);
        }
        let ang = self.asi_angle(if d.speed_valid { d.speed_kt * cfg.factor } else { 0.0 });
        needle(c, ang, 86.0, 3.5, WHITE);
        hub(c);
    }

    fn tc(&self, c: &mut Canvas, d: &FlightData) {
        for a in [-90.0, 90.0, -110.0, 110.0] {
            c.radial(0.0, 0.0, a, 66.0, 88.0, WHITE, 4.0);
        }
        c.text_b("L", -66.0, 40.0, 16.0, WHITE);
        c.text_b("R", 66.0, 40.0, 16.0, WHITE);
        c.text("TURN COORDINATOR", 0.0, -54.0, 8.5, LABEL);
        c.text(&format!("{:+.1}°/s", self.turn_rate), 0.0, -34.0, 11.0, LABEL);
        c.text("2 MIN", 0.0, 32.0, 10.0, LABEL);
        c.text("NO PITCH", 0.0, 80.0, 7.0, LABEL);
        c.text("INFORMATION", 0.0, 87.0, 7.0, LABEL);
        c.arc(-110.0, -150.0, 110.0, 70.0, 62.0, 56.0, 0xFFE8E4D0, 16.0);
        let ball = (90.0 - (d.slip * 120.0).clamp(-24.0, 24.0)).to_radians();
        let (bx, by) = (ball.cos() * 110.0, -40.0 + ball.sin() * 110.0);
        c.circle(bx, by, 7.0, BLACK, 0.0);
        c.line(-9.0, 58.0, -9.0, 76.0, BLACK, 2.0);
        c.line(9.0, 58.0, 9.0, 76.0, BLACK, 2.0);
        let tilt = (self.turn_rate / 3.0 * 20.0).clamp(-35.0, 35.0);
        c.save();
        c.rotate(tilt, 0.0, 0.0);
        c.line(-62.0, 0.0, 62.0, 0.0, WHITE, 6.0);
        c.circle(0.0, 0.0, 10.0, WHITE, 0.0);
        c.line(0.0, -10.0, 0.0, -24.0, WHITE, 4.0);
        c.line(-14.0, -18.0, 14.0, -18.0, WHITE, 3.0);
        c.restore();
    }

    fn vario(&mut self, c: &mut Canvas, d: &FlightData) {
        let va = |v: f32| -90.0 + v.clamp(-5.0, 5.0) / 5.0 * 170.0;
        for k in -10i32..=10 {
            let v = k as f32 * 0.5;
            let major = k % 2 == 0;
            c.radial(0.0, 0.0, va(v), if major { 70.0 } else { 80.0 }, 90.0, WHITE, if major { 3.0 } else { 1.5 });
            if major && k != 0 {
                let a = va(v).to_radians();
                c.text_b(&(k / 2).abs().to_string(), a.sin() * 56.0, -a.cos() * 56.0, 17.0, WHITE);
            }
        }
        arc_band(c, va(0.0), va(5.0), 86.0, 4.0, GREEN);
        arc_band(c, va(-5.0), va(0.0), 86.0, 4.0, GRED);
        c.text_b("0", -56.0, 0.0, 17.0, WHITE);
        c.text("VARIO", 22.0, -16.0, 10.0, LABEL);
        c.text("m/s  TE", 22.0, 14.0, 9.0, LABEL);
        let te = if !d.alt_valid {
            0.0
        } else {
            let vs = d.vs_fpm * 0.00508;
            if d.speed_valid {
                vs + d.speed_kt * 0.514444 * (d.speed_trend * 0.0514444) / 9.81
            } else {
                vs
            }
        };
        let now = Instant::now();
        if let Some(t) = self.last_vario {
            let dt = (now - t).as_secs_f32().clamp(0.0, 1.0);
            self.avg_te += dt / (20.0 + dt) * (te - self.avg_te);
        }
        self.last_vario = Some(now);
        c.text_b(&format!("{te:+.1}"), 22.0, 40.0, 14.0, if te >= 0.0 { GREEN } else { ORANGE });
        c.text("AVG", 22.0, 58.0, 8.0, LABEL);
        c.text(&format!("{:+.1}", self.avg_te), 22.0, 72.0, 12.0, ORANGE);
        needle(c, va(self.avg_te), 70.0, 2.0, ORANGE);
        needle(c, va(te), 86.0, 3.5, WHITE);
        hub(c);
    }

    fn gmeter(&mut self, c: &mut Canvas, d: &FlightData) {
        let ga = |g: f32| -135.0 + (g.clamp(-1.0, 4.0) + 1.0) / 5.0 * 270.0;
        self.g_max = self.g_max.max(d.g_load);
        self.g_min = self.g_min.min(d.g_load);
        arc_band(c, ga(-1.0), ga(3.5), 82.0, 6.0, GREEN);
        arc_band(c, ga(3.5), ga(4.0), 82.0, 6.0, GRED);
        let mut k = -10;
        while k <= 40 {
            let g = k as f32 / 10.0;
            let major = k % 10 == 0;
            c.radial(0.0, 0.0, ga(g), if major { 70.0 } else { 78.0 }, 90.0, WHITE, if major { 3.0 } else { 1.5 });
            if major {
                let a = ga(g).to_radians();
                c.text_b(&(k / 10).to_string(), a.sin() * 56.0, -a.cos() * 56.0, 17.0, WHITE);
            }
            k += 5;
        }
        c.text_b("G", 0.0, 28.0, 18.0, LABEL);
        c.text(&format!("{:.2}", d.g_load), 0.0, 50.0, 13.0, WHITE);
        c.text(&format!("MAX {:.1}   MIN {:.1}", self.g_max, self.g_min), 0.0, 70.0, 9.0, ORANGE);
        needle(c, ga(self.g_max), 70.0, 2.0, ORANGE);
        needle(c, ga(self.g_min), 70.0, 2.0, ORANGE);
        needle(c, ga(d.g_load), 86.0, 3.5, WHITE);
        hub(c);
    }
}

fn status_bar(c: &mut Canvas, d: &FlightData, w: f32, h: f32, hb: f32) {
    c.rect(0.0, h - hb, w, h, 0xFF0C0D0F);
    c.line(0.0, h - hb, w, h - hb, 0xFF3C3E42, 1.5);
    let y = h - hb / 2.0;
    let sz = (hb * 0.42).clamp(11.0, 26.0);
    let mut radios = Vec::new();
    if !d.com1.is_nan() {
        radios.push(format!("COM1 {:.3}", d.com1));
    }
    if !d.nav1.is_nan() {
        radios.push(format!("NAV1 {:.2}", d.nav1));
    }
    if let Some(s) = &d.squawk {
        radios.push(format!("XPDR {s}"));
    }
    c.text_l(&radios.join("   "), w * 0.02, y, sz, LABEL);
    let baro = if d.std {
        "STD 1013".to_string()
    } else if d.in_hg {
        format!("ALT {:.2} inHg", d.qnh * 0.02953)
    } else {
        format!("QNH {}", roundi(d.qnh))
    };
    c.text(&baro, w * 0.58, y, sz, LABEL);
    let mut right = String::new();
    if !d.oat_c.is_nan() {
        right += &format!("{}°C   ", roundi(d.oat_c));
    }
    if !d.tas_kt.is_nan() {
        right += &format!("TAS {}   ", roundi(d.tas_kt));
    }
    right += &d.sim_time.clone().unwrap_or_else(|| chrono::Local::now().format("%H:%M").to_string());
    c.text_r(&right, w * 0.98, y, sz, LABEL);
}

fn bezel_back(c: &mut Canvas) {
    c.circle(0.0, 0.0, 104.0, RIM, 0.0);
    c.circle(0.0, 0.0, 99.0, BEZEL, 0.0);
    c.circle(0.0, 0.0, 93.0, FACE, 0.0);
}

fn bezel_front(c: &mut Canvas) {
    c.circle(0.0, 0.0, 96.0, BLACK, 6.0);
    c.arc(-82.0, -82.0, 82.0, 82.0, 200.0, 70.0, 0x22FFFFFF, 10.0);
}

fn screws(c: &mut Canvas, cx: f32, cy: f32, r: f32) {
    let o = r * 0.95;
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let (x, y) = (cx + sx * o, cy + sy * o);
        c.circle(x, y, r * 0.055, 0xFF4A4C50, 0.0);
        c.line(x - r * 0.04, y - r * 0.02, x + r * 0.04, y + r * 0.02, 0xFF1A1A1A, r * 0.012);
    }
}

fn needle(c: &mut Canvas, angle: f32, len: f32, width: f32, color: Argb) {
    c.save();
    c.rotate(angle, 0.0, 0.0);
    c.poly(color, 0, 0.0, &[-width, 14.0, width, 14.0, width * 0.5, -len + 8.0, 0.0, -len, -width * 0.5, -len + 8.0]);
    c.restore();
}

fn hub(c: &mut Canvas) {
    c.circle(0.0, 0.0, 8.0, 0xFF202020, 0.0);
    c.circle(0.0, 0.0, 8.0, 0xFF606060, 1.5);
}

/// Łuk z kątów zegara (0 = do góry, zgodnie z ruchem wskazówek).
fn arc_band(c: &mut Canvas, a1: f32, a2: f32, r: f32, w: f32, color: Argb) {
    c.arc(-r, -r, r, r, a1 - 90.0, a2 - a1, color, w);
}

fn ai(c: &mut Canvas, d: &FlightData) {
    let k = 2.4;
    c.save();
    c.clip_circle(0.0, 0.0, 92.0);
    let roll = if d.ai_roll.is_nan() { d.roll } else { d.ai_roll };
    c.rotate(-roll, 0.0, 0.0);
    let pitch = (if d.ai_pitch.is_nan() { d.pitch } else { d.ai_pitch }).clamp(-30.0, 30.0);
    c.rect(-200.0, -200.0, 200.0, pitch * k, SKY);
    c.rect(-200.0, pitch * k, 200.0, 200.0, EARTH);
    c.line(-200.0, pitch * k, 200.0, pitch * k, WHITE, 2.5);
    for deg in [5.0, 10.0, 15.0, 20.0] {
        let half = if deg as i32 % 10 == 0 { 22.0 } else { 12.0 };
        c.line(-half, pitch * k - deg * k, half, pitch * k - deg * k, WHITE, 2.0);
        c.line(-half, pitch * k + deg * k, half, pitch * k + deg * k, WHITE, 2.0);
    }
    for a in [10, 20, 30, 60, 90] {
        for sg in [-1, 1] {
            let long = a == 30 || a == 60 || a == 90;
            c.radial(0.0, 0.0, (sg * a) as f32, if long { 72.0 } else { 78.0 }, 92.0, WHITE, 2.5);
        }
    }
    c.poly(WHITE, 0, 0.0, &[0.0, -74.0, -7.0, -90.0, 7.0, -90.0]);
    c.restore();
    c.poly(ORANGE, 0, 0.0, &[0.0, -70.0, -7.0, -58.0, 7.0, -58.0]);
    c.line(-56.0, 0.0, -18.0, 0.0, ORANGE, 5.0);
    c.line(18.0, 0.0, 56.0, 0.0, ORANGE, 5.0);
    c.line(-18.0, 0.0, -10.0, 7.0, ORANGE, 5.0);
    c.line(18.0, 0.0, 10.0, 7.0, ORANGE, 5.0);
    c.circle(0.0, 0.0, 4.0, ORANGE, 0.0);
    if !d.att_valid {
        c.text_b("GYRO", 0.0, 50.0, 16.0, ORANGE);
    }
    if !d.suction.is_nan() && d.suction > 0.0 && d.suction < 3.5 && !d.on_ground {
        c.text_b("LOW VAC", 0.0, 50.0, 13.0, ORANGE);
    }
}

fn alt(c: &mut Canvas, d: &FlightData, metric: bool) {
    for i in 0..50 {
        let major = i % 5 == 0;
        c.radial(0.0, 0.0, i as f32 * 7.2, if major { 74.0 } else { 82.0 }, 90.0, WHITE, if major { 2.5 } else { 1.5 });
    }
    for n in 0..=9 {
        let a = (n as f32 * 36.0).to_radians();
        c.text_b(&n.to_string(), a.sin() * 62.0, -a.cos() * 62.0, 18.0, WHITE);
    }
    c.text("ALT", 0.0, -30.0, 11.0, LABEL);
    c.text(if metric { "100 METERS" } else { "100 FEET" }, 0.0, 32.0, 9.0, LABEL);
    c.rect(30.0, -14.0, 86.0, 14.0, BLACK);
    c.frame(30.0, -14.0, 86.0, 14.0, 0xFF707070, 1.5);
    let txt = if d.std {
        if d.in_hg {
            "29.92".to_string()
        } else {
            "1013".to_string()
        }
    } else if d.in_hg {
        format!("{:.2}", d.qnh * 0.02953)
    } else {
        format!("{:.0}", d.qnh)
    };
    c.text(&txt, 58.0, 0.0, 15.0, if d.std { 0xFF33DDFF } else { WHITE });
    c.text(if d.in_hg { "inHg" } else { "hPa" }, 58.0, 22.0, 8.0, LABEL);
    if !d.alt_valid {
        c.text("NO DATA", 0.0, 52.0, 11.0, ORANGE);
        hub(c);
        return;
    }
    let a = (if metric { d.alt_ft * 0.3048 } else { d.alt_ft }).max(0.0);
    c.save();
    c.rotate((a % 100000.0) / 100000.0 * 360.0, 0.0, 0.0);
    c.line(0.0, 0.0, 0.0, -80.0, WHITE, 1.5);
    c.poly(WHITE, 0, 0.0, &[0.0, -92.0, -6.0, -80.0, 6.0, -80.0]);
    c.restore();
    needle(c, (a % 10000.0) / 10000.0 * 360.0, 50.0, 6.0, WHITE);
    needle(c, (a % 1000.0) / 1000.0 * 360.0, 84.0, 3.5, WHITE);
    hub(c);
}

fn hi(c: &mut Canvas, d: &FlightData) {
    let hdg = if d.hi_heading.is_nan() { d.heading } else { d.hi_heading };
    let names = |deg: i32| -> Option<&'static str> {
        Some(match deg {
            0 => "N",
            30 => "3",
            60 => "6",
            90 => "E",
            120 => "12",
            150 => "15",
            180 => "S",
            210 => "21",
            240 => "24",
            270 => "W",
            300 => "30",
            330 => "33",
            _ => return None,
        })
    };
    let mut deg = 0;
    while deg < 360 {
        c.save();
        c.rotate(deg as f32 - hdg, 0.0, 0.0);
        let ten = deg % 10 == 0;
        c.line(0.0, -90.0, 0.0, if ten { -76.0 } else { -83.0 }, WHITE, if ten { 2.5 } else { 1.5 });
        if let Some(n) = names(deg) {
            c.text_b(n, 0.0, -62.0, if n.len() == 1 { 18.0 } else { 15.0 }, WHITE);
        }
        c.restore();
        deg += 5;
    }
    for a in (45..360).step_by(45) {
        c.radial(0.0, 0.0, a as f32, 93.0, 98.0, ORANGE, 3.0);
    }
    c.poly(ORANGE, 0, 0.0, &[0.0, -91.0, -6.0, -100.0, 6.0, -100.0]);
    c.line(0.0, -88.0, 0.0, -70.0, ORANGE, 3.0);
    if !d.hdg_bug.is_nan() {
        c.save();
        c.rotate(ang_diff(d.hdg_bug, hdg), 0.0, 0.0);
        c.poly(ORANGE, 0, 0.0, &[-9.0, -90.0, -9.0, -82.0, -3.0, -82.0, 0.0, -86.0, 3.0, -82.0, 9.0, -82.0, 9.0, -90.0]);
        c.restore();
    }
    c.line(0.0, -34.0, 0.0, 30.0, ORANGE, 6.0);
    c.line(-32.0, -4.0, 32.0, -4.0, ORANGE, 6.0);
    c.line(-13.0, 26.0, 13.0, 26.0, ORANGE, 5.0);
    if d.has_track {
        c.save();
        c.rotate(ang_diff(d.track, hdg), 0.0, 0.0);
        c.poly(0xFFFF55FF, 0, 0.0, &[0.0, -74.0, 5.0, -66.0, 0.0, -58.0, -5.0, -66.0]);
        c.restore();
    }
    c.rect(-32.0, 48.0, 32.0, 72.0, BLACK);
    c.frame(-32.0, 48.0, 32.0, 72.0, 0xFF707070, 1.5);
    if d.att_valid {
        c.text_b(&format!("{:03}", imod(roundi(hdg), 360)), 0.0, 60.0, 16.0, WHITE);
    } else {
        c.text_b("OFF", 0.0, 60.0, 14.0, ORANGE);
    }
}

/// Wskaźnik VOR/ILS (CDI): karta kursu z nastawą OBS u góry, belka lokalizera / radialu (5 kropek),
/// belka ścieżki schodzenia przy ILS, chorągiewki TO/FR, NAV i GS.
fn cdi(c: &mut Canvas, d: &FlightData) {
    use super::data::NavSource;
    let n = &d.nav;
    let course = if n.course.is_nan() { 0.0 } else { n.course };
    let mut deg = 0;
    while deg < 360 {
        c.save();
        c.rotate(deg as f32 - course, 0.0, 0.0);
        let ten = deg % 10 == 0;
        c.line(0.0, -90.0, 0.0, if ten { -80.0 } else { -85.0 }, WHITE, if ten { 2.0 } else { 1.2 });
        if deg % 30 == 0 {
            let label = match deg {
                0 => "N".to_string(),
                90 => "E".to_string(),
                180 => "S".to_string(),
                270 => "W".to_string(),
                x => (x / 10).to_string(),
            };
            c.text_b(&label, 0.0, -68.0, 13.0, WHITE);
        }
        c.restore();
        deg += 5;
    }
    // Znacznik kursu (u góry) i kursu odwrotnego (u dołu)
    c.poly(ORANGE, 0, 0.0, &[0.0, -91.0, -6.0, -100.0, 6.0, -100.0]);
    c.poly(WHITE, 0, 0.0, &[0.0, 91.0, -5.0, 99.0, 5.0, 99.0]);
    let dot = 10.0;
    for i in 1..=5 {
        let x = i as f32 * dot;
        c.circle(x, 0.0, 2.0, WHITE, 0.0);
        c.circle(-x, 0.0, 2.0, WHITE, 0.0);
    }
    c.circle(0.0, 0.0, 4.5, WHITE, 1.5);
    let has_nav = n.source != NavSource::None && !n.cdi.is_nan();
    let has_gs = !n.gs.is_nan();
    if has_gs {
        for i in 1..=4 {
            let y = i as f32 * dot;
            c.circle(62.0, y, 2.0, WHITE, 0.0);
            c.circle(62.0, -y, 2.0, WHITE, 0.0);
        }
    }
    // Belka boczna: 2,5 kropki PFD = pełne wychylenie (5 kropek)
    let x = if has_nav { n.cdi.clamp(-2.5, 2.5) * 2.0 * dot } else { 0.0 };
    c.line(x, -56.0, x, 56.0, WHITE, 3.5);
    if has_gs {
        let y = -n.gs.clamp(-2.5, 2.5) * 1.6 * dot;
        c.line(-56.0, y, 56.0, y, WHITE, 3.5);
    }
    // Chorągiewki: TO/FR przy VOR, NAV bez sygnału, GS przy lokalizerze bez ścieżki
    if !has_nav {
        c.rect(-48.0, -40.0, -22.0, -26.0, GRED);
        c.text_b("NAV", -35.0, -33.0, 10.0, WHITE);
    } else if n.source == NavSource::Vor {
        let t = match n.to_from {
            1 => "TO",
            2 => "FR",
            _ => "",
        };
        if !t.is_empty() {
            c.text_b(t, 32.0, -30.0, 12.0, WHITE);
        }
    }
    if n.loc && !has_gs {
        c.rect(48.0, -14.0, 74.0, 0.0, GRED);
        c.text_b("GS", 61.0, -7.0, 10.0, WHITE);
    }
    let src = match n.source {
        NavSource::Loc if has_gs => "ILS",
        NavSource::Loc => "LOC",
        NavSource::Vor => "VOR 1",
        NavSource::Gps => "GPS",
        NavSource::None => "NAV 1",
    };
    c.text_b(src, 0.0, 34.0, 11.0, LABEL);
    c.text_b(&format!("{:03}", imod(roundi(course), 360)), 0.0, 50.0, 13.0, ORANGE);
    if let Some(id) = &n.ident {
        c.text_b(id, 0.0, -40.0, 11.0, LABEL);
    }
}

fn radio_alt(c: &mut Canvas, d: &FlightData) {
    let ra_a = |ft: f32| ft.clamp(0.0, 1000.0) / 1000.0 * 320.0 - 160.0;
    arc_band(c, ra_a(0.0), ra_a(100.0), 82.0, 6.0, GRED);
    let mut v = 0;
    while v <= 1000 {
        let major = v % 100 == 0;
        c.radial(0.0, 0.0, ra_a(v as f32), if major { 72.0 } else { 80.0 }, 90.0, WHITE, if major { 2.5 } else { 1.5 });
        if v % 200 == 0 {
            let a = ra_a(v as f32).to_radians();
            c.text_b(&(v / 100).to_string(), a.sin() * 58.0, -a.cos() * 58.0, 16.0, WHITE);
        }
        v += 50;
    }
    c.text("RADIO ALT", 0.0, -32.0, 10.0, LABEL);
    c.text("100 FEET", 0.0, 30.0, 9.0, LABEL);
    c.rect(-34.0, 42.0, 34.0, 66.0, BLACK);
    c.frame(-34.0, 42.0, 34.0, 66.0, 0xFF707070, 1.5);
    if !d.ra_valid {
        c.text("NO TERR", 0.0, 54.0, 10.0, ORANGE);
        hub(c);
        return;
    }
    let ra = d.ra_ft.max(0.0);
    let col = if ra < 100.0 { ORANGE } else { WHITE };
    c.text_b(&roundi(ra).to_string(), 0.0, 54.0, 15.0, col);
    needle(c, ra_a(ra), 86.0, 3.5, col);
    hub(c);
}

fn vsi(c: &mut Canvas, d: &FlightData) {
    let va = |v: f32| -90.0 + v.clamp(-2000.0, 2000.0) / 2000.0 * 170.0;
    let mut v: i32 = -2000;
    while v <= 2000 {
        let major = v % 500 == 0;
        c.radial(0.0, 0.0, va(v as f32), if major { 72.0 } else { 80.0 }, 90.0, WHITE, if major { 2.5 } else { 1.5 });
        if major && v != 0 {
            let a = va(v as f32).to_radians();
            c.text_b(&(v.abs() / 100).to_string(), a.sin() * 58.0, -a.cos() * 58.0, 16.0, WHITE);
        }
        v += 100;
    }
    c.text_b("0", -58.0, 0.0, 16.0, WHITE);
    c.text("UP", -30.0, -40.0, 10.0, LABEL);
    c.text("DOWN", -30.0, 40.0, 10.0, LABEL);
    c.text("VERTICAL SPEED", 22.0, -14.0, 8.0, LABEL);
    c.text("100 FT PER MIN", 22.0, 14.0, 8.0, LABEL);
    if d.alt_valid {
        let v100 = roundi(d.vs_fpm / 50.0) * 50;
        c.text_b(&format!("{v100:+}"), 22.0, 44.0, 13.0, if d.vs_fpm.abs() < 100.0 { LABEL } else { WHITE });
    } else {
        c.text("NO DATA", 22.0, 44.0, 10.0, ORANGE);
    }
    needle(c, va(if d.alt_valid { d.vs_fpm } else { 0.0 }), 86.0, 3.5, WHITE);
    hub(c);
}

// ---------- zegary silnika (dodatkowe na komputerze) ----------

/// Okrągły zegar silnika: łuk 240°, zielony zakres, wartość cyfrowo.
pub fn engine_dial(c: &mut Canvas, cx: f32, cy: f32, r: f32, label: &str, value: f32, max: f32, green: (f32, f32), red: f32, unit: &str, digits: usize) {
    c.save();
    c.translate(cx, cy);
    c.scale(r / 100.0, r / 100.0);
    bezel_back(c);
    let ang = |v: f32| -120.0 + (v / max).clamp(0.0, 1.0) * 240.0;
    arc_band(c, ang(green.0), ang(green.1), 82.0, 7.0, GREEN);
    if red < max {
        arc_band(c, ang(red), ang(max), 82.0, 7.0, GRED);
    }
    for i in 0..=10 {
        let v = max * i as f32 / 10.0;
        c.radial(0.0, 0.0, ang(v), if i % 2 == 0 { 70.0 } else { 78.0 }, 90.0, WHITE, if i % 2 == 0 { 2.5 } else { 1.5 });
        if i % 2 == 0 {
            let a = ang(v).to_radians();
            let lbl = if max >= 1000.0 { format!("{}", roundi(v / 100.0)) } else { format!("{}", roundi(v)) };
            c.text_b(&lbl, a.sin() * 56.0, -a.cos() * 56.0, 13.0, WHITE);
        }
    }
    c.text(label, 0.0, -26.0, 12.0, LABEL);
    c.rect(-40.0, 38.0, 40.0, 64.0, BLACK);
    c.frame(-40.0, 38.0, 40.0, 64.0, 0xFF707070, 1.5);
    let txt = if value.is_nan() { "---".to_string() } else { format!("{:.*}", digits, value) };
    c.text_b(&txt, 0.0, 51.0, 15.0, if !value.is_nan() && value >= red { 0xFFFF5050 } else { WHITE });
    c.text(unit, 0.0, 78.0, 9.0, LABEL);
    if !value.is_nan() {
        needle(c, ang(value), 84.0, 3.5, WHITE);
    }
    hub(c);
    bezel_front(c);
    c.restore();
}
