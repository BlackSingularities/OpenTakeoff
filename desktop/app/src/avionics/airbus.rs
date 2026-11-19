//! PFD w stylu Airbusa (A320) – przeniesiony z AirbusPfd.kt: FMA, bugi FCU, dyrektor lotu, ILS.

use tiny_skia::{Path, PathBuilder};

use super::data::*;
use super::gfx::*;
use super::simlayer as sl;
use super::Pfd;

const CX: f32 = 431.0;
const CY: f32 = 512.0;
const PPD: f32 = 10.5;
const GREEN: Argb = 0xFF2BF22B;
const CYAN: Argb = 0xFF1FE6FF;
const YEL: Argb = 0xFFFFF200;
const GREY_LINE: Argb = 0xFFA0A0A0;
const MAGENTA: Argb = 0xFFFF55FF;
const SKY: Argb = 0xFF1C9AEA;
const GROUND: Argb = 0xFF8C4B1D;
const TAPE: Argb = 0xFF5D5D5D;
const VSI_BG: Argb = 0xFF3F3F3F;
const VMO: f32 = 350.0;

pub struct AirbusPfd {
    clip_circle: Path,
}

impl AirbusPfd {
    pub fn new() -> Self {
        let _ = PathBuilder::new();
        Self { clip_circle: circle_path(CX, CY, 292.0).unwrap() }
    }

    fn clip_att(&self, c: &mut Canvas) {
        c.clip_path(&self.clip_circle);
        c.clip_rect(199.0, 250.0, 663.0, 776.0);
    }

    fn attitude(&self, c: &mut Canvas, d: &FlightData) {
        if !d.att_valid {
            c.text("ATT", CX, CY - 60.0, 52.0, RED);
            aircraft(c);
            return;
        }
        c.save();
        self.clip_att(c);
        c.save();
        c.rotate(-d.roll, CX, CY);
        c.translate(0.0, d.pitch * PPD);
        c.rect(CX - 1000.0, CY - 3000.0, CX + 1000.0, CY, SKY);
        c.rect(CX - 1000.0, CY, CX + 1000.0, CY + 3000.0, GROUND);
        c.line(CX - 1000.0, CY, CX + 1000.0, CY, WHITE, 3.0);
        let mut h = floor_to(d.heading - 30.0, 10);
        while h as f32 <= d.heading + 30.0 {
            let x = CX + ang_diff(h as f32, d.heading) * PPD;
            c.line(x, CY, x, CY + 14.0, WHITE, 3.0);
            h += 10;
        }
        let i0 = ((d.pitch - 22.0) / 2.5).ceil() as i32;
        let i1 = ((d.pitch + 22.0) / 2.5).floor() as i32;
        for i in i0..=i1 {
            if i == 0 {
                continue;
            }
            let deg = i as f32 * 2.5;
            if deg.abs() > 90.0 {
                continue;
            }
            let y = CY - deg * PPD;
            let half = if i % 4 == 0 {
                62.0
            } else if i % 2 == 0 {
                30.0
            } else {
                13.0
            };
            c.line(CX - half, y, CX + half, y, WHITE, 3.2);
            if i % 4 == 0 {
                let lbl = roundi(deg.abs()).to_string();
                c.text(&lbl, CX - half - 34.0, y, 36.0, WHITE);
                c.text(&lbl, CX + half + 34.0, y, 36.0, WHITE);
            }
        }
        c.restore();
        if let Some((dx, dy)) = sl::fpv_offset(d, PPD) {
            c.save();
            c.rotate(-d.roll, CX, CY);
            sl::bird(c, CX + dx, CY - dy, GREEN, 14.0, 42.0, 30.0, 4.0);
            c.restore();
        }
        c.restore();

        if d.pitch > 30.0 || d.pitch < -20.0 {
            let dir = if d.pitch > 0.0 { 1.0 } else { -1.0 };
            c.save();
            self.clip_att(c);
            c.rotate(-d.roll, CX, CY);
            for k in 1..=2 {
                let y = CY + dir * (90.0 + k as f32 * 70.0);
                c.line(CX - 72.0, y + dir * 46.0, CX, y, RED, 6.0);
                c.line(CX, y, CX + 72.0, y + dir * 46.0, RED, 6.0);
            }
            c.restore();
        }
        roll_scale(c, d);
        sl::fd_cross_bars(c, d, CX, CY, PPD, 120.0, GREEN, 5.0);
        aircraft(c);
        sl::deviation_std(c, d, CX, 836.0, 700.0, CY, 40.0, MAGENTA);
        sl::marker(c, d, 620.0, 280.0, 30.0);
        if d.ra_valid && d.ra_ft < 2500.0 {
            let ra = if d.ra_ft < 50.0 { roundi(d.ra_ft) } else { roundi(d.ra_ft / 10.0) * 10 };
            c.text(&ra.to_string(), CX, 728.0, 48.0, if d.ra_ft < 200.0 { AMBER } else { GREEN });
        }
    }

    fn speed(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r, t, b, k) = (18.0, 150.0, 245.0, 780.0, 4.75);
        if !d.speed_valid {
            c.text("SPD", (l + r) / 2.0, CY, 50.0, RED);
            return;
        }
        let s = d.speed_kt;
        let vmo = sl::vmo(d, VMO);
        c.rect(l, t, r, b, TAPE);
        c.save();
        c.clip_rect(l, t, r, b);
        sl::low_speed(c, d, r - 14.0, 14.0, |v| CY - (v - s) * k, b);
        let mut v = floor_to(s - 70.0, 10).max(0);
        while v as f32 <= s + 70.0 {
            let y = CY - (v as f32 - s) * k;
            c.line(r - 20.0, y, r, y, WHITE, 3.2);
            if v % 20 == 0 {
                c.text_r(&v.to_string(), r - 28.0, y, 42.0, WHITE);
            }
            v += 10;
        }
        if s + 70.0 > vmo {
            sl::barber_up(c, r, 14.0, t, CY - (vmo - s) * k);
        }
        if !d.spd_bug.is_nan() && d.mach_bug.is_nan() {
            let y = CY - (d.spd_bug - s) * k;
            if (t..=b).contains(&y) {
                sl::speed_bug(c, r, y, if d.fma.athr_active { MAGENTA } else { CYAN }, true);
            }
        }
        c.restore();
        if !d.spd_bug.is_nan() && d.mach_bug.is_nan() {
            let y = CY - (d.spd_bug - s) * k;
            if y < t {
                c.text(&roundi(d.spd_bug).to_string(), (l + r) / 2.0, t - 24.0, 38.0, CYAN);
            }
            if y > b {
                c.text(&roundi(d.spd_bug).to_string(), (l + r) / 2.0, b + 24.0, 38.0, CYAN);
            }
        }
        c.line(l, t, r + 4.0, t, WHITE, 3.0);
        c.line(l, b, r + 4.0, b, WHITE, 3.0);
        c.line(r, t, r, b, WHITE, 3.0);
        if let Some(m) = sl::mach_text(d) {
            c.text(&m, (l + r) / 2.0, b + 62.0, 40.0, GREEN);
        }
        if d.stall_warn {
            c.text_b("STALL", CX, 300.0, 54.0, RED);
        }
        sl::trend_arrow(c, r + 12.0, CY, d.speed_trend.clamp(-55.0, 55.0), k, YEL, 4.0, 16.0);
        c.line(l, CY, r + 2.0, CY, YEL, 6.0);
        c.poly(YEL, YEL, 1.0, &[r, CY, r + 24.0, CY - 14.0, r + 24.0, CY + 14.0]);
    }

    fn altitude(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r, t, b, k) = (745.0, 862.0, 245.0, 780.0, 0.475);
        if !d.alt_valid {
            c.text("ALT", (l + r) / 2.0, CY, 50.0, RED);
            return;
        }
        let a = d.alt_ft;
        c.rect(l, t, r, b, TAPE);
        c.save();
        c.clip_rect(l, t, r, b);
        let mut v = floor_to(a - 700.0, 100);
        while v as f32 <= a + 700.0 {
            let y = CY - (v as f32 - a) * k;
            c.line(r - 18.0, y, r, y, WHITE, 3.2);
            if imod(v, 500) == 0 {
                let lbl = format!("{}{:03}", if v < 0 { "-" } else { "" }, v.abs() / 100);
                c.text_l(&lbl, l + 12.0, y, 40.0, WHITE);
            }
            v += 100;
        }
        if !d.terrain_ft.is_nan() && !d.std {
            let yt = CY - (d.terrain_ft - a) * k;
            if yt < b {
                c.rect(r - 16.0, yt.max(t), r, b, RED);
                c.line(l, yt, r, yt, RED, 3.0);
            }
        }
        if !d.alt_bug.is_nan() {
            let yb = CY - (d.alt_bug - a) * k;
            if (t..=b).contains(&yb) {
                sl::alt_bug(c, l, yb, CYAN);
            }
        }
        c.restore();
        c.line(l - 4.0, t, r, t, WHITE, 3.0);
        c.line(l - 4.0, b, r, b, WHITE, 3.0);
        c.line(r, t, r, b, WHITE, 3.0);
        if !d.alt_bug.is_nan() {
            let sel = if d.std && d.alt_bug >= 1000.0 { format!("FL{}", roundi(d.alt_bug / 100.0)) } else { roundi(d.alt_bug).to_string() };
            c.text(&sel, (l + r) / 2.0, t - 26.0, 38.0, CYAN);
        }
        // Okno wysokości jak w A320: stała szerokość na trzy cyfry setek (do FL390 i „-5”), żółta ramka
        // z wyższą częścią bębna; wcześniej okno mieściło dwie cyfry i od 10 000 ft setki wychodziły
        // w lewo poza ramkę na taśmę.
        let wl = l - 10.0;
        c.poly(BLACK, YEL, 4.0, &[wl, CY - 30.0, 822.0, CY - 30.0, 822.0, CY - 50.0, 878.0, CY - 50.0, 878.0, CY + 50.0, 822.0, CY + 50.0, 822.0, CY + 30.0, wl, CY + 30.0]);
        let hundreds = (a / 100.0).floor() as i32;
        let big = if hundreds < 0 { format!("-{}", hundreds.abs()) } else { hundreds.to_string() };
        c.save();
        c.clip_rect(wl + 2.0, CY - 28.0, 820.0, CY + 28.0);
        c.text_r(&big, 818.0, CY, if big.chars().count() > 3 { 42.0 } else { 50.0 }, GREEN);
        c.restore();
        c.drum(824.0, CY - 48.0, 876.0, CY + 48.0, a / 20.0, 38.0, 36.0, GREEN, 850.0, Align::Center, |kk| format!("{:02}", imod(kk * 20, 100)));
    }

    fn vy(v: f32) -> f32 {
        let a = v.abs().min(6000.0);
        let g = if a <= 1000.0 {
            a * 0.15
        } else if a <= 2000.0 {
            150.0 + (a - 1000.0) * 0.07
        } else {
            220.0 + (a - 2000.0) * 0.0175
        };
        CY - sign(v) * g
    }

    fn vsi(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r) = (900.0, 992.0);
        c.poly(VSI_BG, 0, 0.0, &[l, 280.0, l + 32.0, 208.0, r, 208.0, r, 816.0, l + 32.0, 816.0, l, 744.0]);
        for v in [500, 1000, 1500, 2000, 6000] {
            for sg in [-1, 1] {
                let y = Self::vy((sg * v) as f32);
                let len = if v == 500 || v == 1500 { 10.0 } else { 18.0 };
                c.line(l + 34.0, y, l + 34.0 + len, y, WHITE, 3.0);
                if v == 1000 || v == 2000 || v == 6000 {
                    c.text(&(v / 1000).to_string(), l + 18.0, y, 30.0, WHITE);
                }
            }
        }
        c.line(l + 30.0, CY, l + 58.0, CY, YEL, 5.0);
        if !d.alt_valid {
            c.text("V/S", (l + r) / 2.0 + 8.0, CY - 80.0, 34.0, RED);
            return;
        }
        let vs = d.vs_fpm;
        let col = if vs.abs() > 6000.0 { AMBER } else { GREEN };
        let tip = Self::vy(vs);
        c.save();
        c.clip_rect(l + 34.0, 205.0, r, 820.0);
        c.line(1110.0, CY, l + 34.0, tip, col, 5.0);
        c.restore();
        if vs.abs() >= 200.0 {
            let txt = format!("{:02}", roundi(vs.abs() / 100.0));
            let ty = (tip + if vs > 0.0 { -30.0 } else { 30.0 }).clamp(230.0, 795.0);
            c.rect(l + 40.0, ty - 18.0, l + 84.0, ty + 18.0, BLACK);
            c.text(&txt, l + 62.0, ty, 32.0, col);
        }
    }

    fn heading(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r, t, b, k) = (199.0, 663.0, 902.0, 982.0, 10.3);
        if !d.att_valid {
            c.text("HDG", CX, (t + b) / 2.0, 46.0, RED);
            return;
        }
        let h = d.heading;
        c.rect(l, t, r, b, TAPE);
        c.save();
        c.clip_rect(l, t, r, b);
        let mut deg = floor_to(h - 28.0, 5);
        while deg as f32 <= h + 28.0 {
            let x = CX + (deg as f32 - h) * k;
            let ten = imod(deg, 10) == 0;
            c.line(x, t, x, t + if ten { 22.0 } else { 12.0 }, WHITE, 3.0);
            if ten {
                let n = imod(deg, 360) / 10;
                c.text(&n.to_string(), x, t + 50.0, if n % 3 == 0 { 42.0 } else { 32.0 }, WHITE);
            }
            deg += 5;
        }
        c.restore();
        c.line(l, t, r, t, WHITE, 3.0);
        if !d.hdg_bug.is_nan() {
            let x = CX + ang_diff(d.hdg_bug, h).clamp(-22.0, 22.0) * k;
            c.poly(CYAN, CYAN, 2.0, &[x, t, x - 13.0, t - 22.0, x + 13.0, t - 22.0]);
        }
        if d.nav.loc && !d.nav.course.is_nan() {
            let dx = ang_diff(d.nav.course, h);
            if dx.abs() < 22.0 {
                let x = CX + dx * k;
                c.line(x, t - 30.0, x, t + 30.0, MAGENTA, 4.0);
                c.line(x - 10.0, t - 10.0, x + 10.0, t - 10.0, MAGENTA, 4.0);
            }
        }
        if d.has_track {
            let dx = ang_diff(d.track, h);
            if dx.abs() < 22.0 {
                let x = CX + dx * k;
                c.poly(0, GREEN, 3.5, &[x, t + 2.0, x + 10.0, t + 14.0, x, t + 26.0, x - 10.0, t + 14.0]);
            }
        }
        c.line(CX, t - 34.0, CX, t + 18.0, YEL, 6.0);
    }

    fn fma(&self, c: &mut Canvas, d: &FlightData) {
        for x in [205.0, 410.0, 615.0, 805.0] {
            c.line(x, 8.0, x, 182.0, GREY_LINE, 3.0);
        }
        let (y1, y2, y3, sz) = (40.0, 88.0, 136.0, 38.0);
        let f = &d.fma;
        if f.toga {
            c.text("MAN TOGA", 102.0, y1, sz, WHITE);
        } else if f.athr_active && f.speed == Some(SpeedMode::Mach) {
            c.text("MACH", 102.0, y1, sz, GREEN);
        } else if f.athr_active && f.vertical == Some(Vertical::Flc) {
            c.text(if d.vs_fpm >= 0.0 { "THR CLB" } else { "THR IDLE" }, 102.0, y1, sz, GREEN);
        } else if f.athr_active {
            c.text("SPEED", 102.0, y1, sz, GREEN);
        }
        match f.vertical {
            Some(Vertical::Alt) => c.text("ALT", 307.0, y1, sz, GREEN),
            Some(Vertical::Vs) => {
                let v = roundi((if d.vs_bug.is_nan() { d.vs_fpm } else { d.vs_bug }) / 100.0) * 100;
                let txt = format!("{}{}", if v > 0 { "+" } else { "" }, v);
                let w1 = c.tw("V/S ", sz);
                let x0 = 307.0 - (w1 + c.tw(&txt, sz)) / 2.0;
                c.text_l("V/S ", x0, y1, sz, GREEN);
                c.text_l(&txt, x0 + w1, y1, sz, CYAN);
            }
            Some(Vertical::Flc) => c.text(if d.alt_bug.is_nan() || d.alt_bug >= d.alt_ft { "OP CLB" } else { "OP DES" }, 307.0, y1, sz, GREEN),
            Some(Vertical::Gs) => {
                if f.lateral != Some(Lateral::Loc) {
                    c.text("G/S", 307.0, y1, sz, GREEN)
                }
            }
            _ => {}
        }
        let armed: Vec<&str> = f.vertical_armed.iter().map(|v| if *v == Vertical::Gs { "G/S" } else { "ALT" }).collect();
        if !armed.is_empty() {
            c.text(&armed.join(" "), 307.0, y2, 34.0, CYAN);
        }
        let lat = match f.lateral {
            Some(Lateral::Hdg) => Some("HDG"),
            Some(Lateral::NavGps) | Some(Lateral::NavVor) => Some("NAV"),
            Some(Lateral::Loc) => {
                if f.vertical == Some(Vertical::Gs) {
                    None
                } else {
                    Some("LOC")
                }
            }
            Some(Lateral::Bc) => Some("LOC B/C"),
            _ => None,
        };
        if let Some(l) = lat {
            c.text(l, 512.0, y1, sz, GREEN);
        }
        if f.lateral_armed == Some(Lateral::Loc) {
            c.text("LOC", 512.0, y2, 34.0, CYAN);
        }
        if f.lateral == Some(Lateral::Loc) && f.vertical == Some(Vertical::Gs) {
            c.text("LAND", 410.0, y1, sz, GREEN);
        }
        if f.lateral == Some(Lateral::Loc) || f.lateral_armed == Some(Lateral::Loc) {
            c.text(if d.nav.gs.is_nan() { "NPA" } else { "CAT 1" }, 710.0, y1, 34.0, WHITE);
        }
        if !d.minimums_ft.is_nan() {
            c.text_l("DH", 640.0, y2, 30.0, WHITE);
            c.text_l(&roundi(d.minimums_ft).to_string(), 690.0, y2, 34.0, CYAN);
        }
        if f.ap {
            c.text("AP1", 902.0, y1, sz, WHITE);
        }
        if f.fd {
            c.text("1 FD 2", 902.0, y2, sz, WHITE);
        }
        if f.athr_armed {
            c.text("A/THR", 902.0, y3, sz, if f.athr_active { WHITE } else { CYAN });
        }
        if let Some(n) = sl::nav_line(d) {
            c.text_l(&n, 20.0, 880.0, 30.0, MAGENTA);
        }
    }

    fn baro(&self, c: &mut Canvas, d: &FlightData) {
        let y = 855.0;
        if d.std {
            c.frame(770.0, y - 26.0, 860.0, y + 26.0, YEL, 3.0);
            c.text("STD", 815.0, y, 42.0, CYAN);
        } else {
            c.text_l("QNH", 735.0, y, 36.0, WHITE);
            c.text_r(&sl::baro_text(d), 990.0, y, 42.0, CYAN);
        }
    }
}

fn roll_scale(c: &mut Canvas, d: &FlightData) {
    for a in [10.0, 20.0, 30.0, 45.0] {
        let r2 = if a == 30.0 { 324.0 } else { 310.0 };
        c.radial(CX, CY, a, 293.0, r2, WHITE, 3.5);
        c.radial(CX, CY, -a, 293.0, r2, WHITE, 3.5);
    }
    for s in [-1.0, 1.0] {
        c.radial(CX, CY, s * 65.5, 292.0, 308.0, GREEN, 3.0);
        c.radial(CX, CY, s * 68.5, 292.0, 308.0, GREEN, 3.0);
    }
    c.poly(0, YEL, 3.5, &[CX, CY - 293.0, CX - 15.0, CY - 318.0, CX + 15.0, CY - 318.0]);
    c.save();
    c.rotate(-d.roll, CX, CY);
    let col = if d.roll.abs() > 45.0 { AMBER } else { YEL };
    c.poly(0, col, 3.5, &[CX, CY - 290.0, CX - 15.0, CY - 266.0, CX + 15.0, CY - 266.0]);
    let off = (d.slip * 180.0).clamp(-45.0, 45.0);
    let slip_col = if d.slip.abs() > 0.2 { AMBER } else { col };
    c.poly(0, slip_col, 3.5, &[CX - 17.0 + off, CY - 262.0, CX + 17.0 + off, CY - 262.0, CX + 22.0 + off, CY - 250.0, CX - 22.0 + off, CY - 250.0]);
    c.restore();
}

fn aircraft(c: &mut Canvas) {
    let y = CY;
    c.poly(BLACK, YEL, 4.0, &[212.0, y - 8.0, 312.0, y - 8.0, 312.0, y + 36.0, 297.0, y + 36.0, 297.0, y + 8.0, 212.0, y + 8.0]);
    let m = 2.0 * CX;
    c.poly(BLACK, YEL, 4.0, &[m - 212.0, y - 8.0, m - 312.0, y - 8.0, m - 312.0, y + 36.0, m - 297.0, y + 36.0, m - 297.0, y + 8.0, m - 212.0, y + 8.0]);
    c.rect(CX - 9.0, y - 9.0, CX + 9.0, y + 9.0, BLACK);
    c.frame(CX - 9.0, y - 9.0, CX + 9.0, y + 9.0, YEL, 4.0);
}

impl Pfd for AirbusPfd {
    fn draw(&mut self, c: &mut Canvas, d: &FlightData) {
        self.attitude(c, d);
        self.speed(c, d);
        self.altitude(c, d);
        self.vsi(c, d);
        self.heading(c, d);
        self.fma(c, d);
        self.baro(c, d);
    }
}
