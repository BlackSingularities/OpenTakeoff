//! PFD Embraera E-Jet (Honeywell Primus Epic) – przeniesiony z E190Pfd.kt.

use tiny_skia::Path;

use super::data::*;
use super::gfx::*;
use super::simlayer as sl;
use super::Pfd;

const ADI_L: f32 = 155.0;
const ADI_R: f32 = 645.0;
const ADI_T: f32 = 96.0;
const ADI_B: f32 = 520.0;
const CX: f32 = 400.0;
const CY: f32 = 308.0;
const PPD: f32 = 9.4;
const RX: f32 = 400.0;
const RY: f32 = 790.0;
const RR: f32 = 168.0;
const SKY: Argb = 0xFF1B8FE0;
const GROUND: Argb = 0xFF7A4A1E;
const TAPE: Argb = 0xFF1C1E22;
const GREEN: Argb = 0xFF35E635;
const CYAN: Argb = 0xFF2FD8F2;
const MAGENTA: Argb = 0xFFE85AD6;
const GREY: Argb = 0xFF8E8E8E;

pub struct E190Pfd {
    clip: Path,
}

impl E190Pfd {
    pub fn new() -> Self {
        Self { clip: round_rect_path(ADI_L, ADI_T, ADI_R, ADI_B, 16.0).unwrap() }
    }

    fn fma(&self, c: &mut Canvas, d: &FlightData) {
        let (y, y2, sz) = (26.0, 50.0, 28.0);
        c.frame(155.0, 6.0, 645.0, 64.0, GREY, 2.0);
        for x in [277.0, 400.0, 522.0] {
            c.line(x, 8.0, x, 62.0, GREY, 2.0);
        }
        let f = &d.fma;
        let athr = if f.toga {
            Some("TO")
        } else if f.athr_active && f.vertical == Some(Vertical::Flc) {
            Some(if d.vs_fpm >= 0.0 { "THR" } else { "RETD" })
        } else if f.athr_active {
            Some("SPD")
        } else if f.athr_armed {
            Some("AT")
        } else {
            None
        };
        if let Some(a) = athr {
            c.text(a, 216.0, y, sz, if f.athr_active || f.toga { GREEN } else { WHITE });
        }
        if f.ap {
            c.text("AP", 338.0, y, sz, GREEN);
        }
        if f.yd {
            c.text("YD", 338.0, y2, 22.0, GREEN);
        }
        let lat = match f.lateral {
            Some(Lateral::Hdg) => Some("HDG"),
            Some(Lateral::NavGps) => Some("LNAV"),
            Some(Lateral::NavVor) => Some("VOR"),
            Some(Lateral::Loc) => Some("LOC"),
            Some(Lateral::Bc) => Some("BC"),
            Some(Lateral::Rol) => Some("ROLL"),
            None => None,
        };
        if let Some(l) = lat {
            c.text(l, 461.0, y, sz, GREEN);
        }
        if f.lateral_armed == Some(Lateral::Loc) {
            c.text("LOC", 461.0, y2, 22.0, WHITE);
        }
        let vert = match f.vertical {
            Some(Vertical::Alt) => Some("ALT"),
            Some(Vertical::Vs) => Some("VS"),
            Some(Vertical::Flc) => Some("FLCH"),
            Some(Vertical::Gs) => Some("GS"),
            Some(Vertical::Pit) => Some("PTCH"),
            _ => None,
        };
        if let Some(v) = vert {
            c.text(v, 583.0, y, sz, GREEN);
        }
        let armed: Vec<&str> = f.vertical_armed.iter().map(|v| if *v == Vertical::Gs { "GS" } else { "ASEL" }).collect();
        if !armed.is_empty() {
            c.text(&armed.join(" "), 583.0, y2, 22.0, WHITE);
        }
    }

    fn attitude(&self, c: &mut Canvas, d: &FlightData) {
        if !d.att_valid {
            c.rect(ADI_L, ADI_T, ADI_R, ADI_B, BLACK);
            c.frame(ADI_L, ADI_T, ADI_R, ADI_B, AMBER, 3.0);
            c.text("ATT", CX, CY, 48.0, AMBER);
            aircraft(c);
            return;
        }
        c.save();
        c.clip_path(&self.clip);
        c.save();
        c.rotate(-d.roll, CX, CY);
        c.translate(0.0, d.pitch * PPD);
        c.rect(CX - 1200.0, CY - 3000.0, CX + 1200.0, CY, SKY);
        c.rect(CX - 1200.0, CY, CX + 1200.0, CY + 3000.0, GROUND);
        c.line(CX - 1200.0, CY, CX + 1200.0, CY, WHITE, 3.0);
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
                88.0
            } else if i % 2 == 0 {
                44.0
            } else {
                20.0
            };
            c.line(CX - half, y, CX + half, y, WHITE, 3.0);
            if i % 4 == 0 {
                let lbl = roundi(deg.abs()).to_string();
                c.text(&lbl, CX - half - 26.0, y, 30.0, WHITE);
                c.text(&lbl, CX + half + 26.0, y, 30.0, WHITE);
            }
        }
        c.restore();
        if d.fpv && d.has_track && d.speed_kt > 5.0 {
            c.save();
            c.rotate(-d.roll, CX, CY);
            let dx = ang_diff(d.track, d.heading).clamp(-16.0, 16.0) * PPD;
            let dy = (d.fpa - d.pitch).clamp(-16.0, 16.0) * PPD;
            sl::bird(c, CX + dx, CY - dy, GREEN, 12.0, 34.0, 26.0, 3.5);
            c.restore();
        }
        c.restore();
        sl::pitch_chevrons(c, d, CX, CY, &self.clip, 70.0, 62.0, 66.0, 42.0);
        c.save();
        c.clip_rect(ADI_L, ADI_T, ADI_R, ADI_B);
        let r = 196.0;
        for a in [10, 20, 30, 45, 60] {
            for s in [-1, 1] {
                let len = if a == 30 || a == 60 { 22.0 } else { 14.0 };
                c.radial(CX, CY, (s * a) as f32, r, r + len, WHITE, 3.0);
            }
        }
        c.poly(0, WHITE, 3.0, &[CX, CY - r, CX - 14.0, CY - r - 24.0, CX + 14.0, CY - r - 24.0]);
        c.save();
        c.rotate(-d.roll, CX, CY);
        let col = if d.roll.abs() > 35.0 { AMBER } else { WHITE };
        c.poly(col, col, 2.0, &[CX, CY - r + 4.0, CX - 14.0, CY - r + 28.0, CX + 14.0, CY - r + 28.0]);
        let off = (d.slip * 160.0).clamp(-40.0, 40.0);
        c.poly(0, if d.slip.abs() > 0.2 { AMBER } else { col }, 3.0, &[CX - 15.0 + off, CY - r + 33.0, CX + 15.0 + off, CY - r + 33.0, CX + 20.0 + off, CY - r + 45.0, CX - 20.0 + off, CY - r + 45.0]);
        c.restore();
        c.restore();
        sl::fd_cross_bars(c, d, CX, CY, PPD, 100.0, MAGENTA, 5.0);
        aircraft(c);
        c.frame(ADI_L, ADI_T, ADI_R, ADI_B, GREY, 2.0);
        sl::deviation_std(c, d, CX, ADI_B - 14.0, ADI_R - 26.0, CY, 34.0, MAGENTA);
        sl::marker(c, d, ADI_R - 50.0, ADI_T + 30.0, 26.0);
        if d.stall_warn {
            c.text_b("STALL", CX, CY - 110.0, 46.0, RED);
        }
        if !d.minimums_ft.is_nan() {
            c.text_r(&format!("RA {}", roundi(d.minimums_ft)), ADI_R - 12.0, ADI_B - 50.0, 26.0, CYAN);
        }
        if let Some(n) = sl::nav_line(d) {
            c.text_l(&n, ADI_L + 12.0, ADI_T + 24.0, 24.0, GREEN);
        }
        if d.ra_valid && d.ra_ft < 2500.0 {
            let ra = if d.ra_ft < 50.0 { roundi(d.ra_ft) } else { roundi(d.ra_ft / 10.0) * 10 };
            c.text(&ra.to_string(), CX, ADI_B - 42.0, 40.0, if d.ra_ft < 200.0 { AMBER } else { GREEN });
        }
    }

    fn speed(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r, t, b, k) = (36.0, 140.0, 110.0, 506.0, 4.6);
        if !d.speed_valid {
            c.text("SPD", (l + r) / 2.0, CY, 42.0, AMBER);
            return;
        }
        let s = d.speed_kt;
        c.rect(l, t, r, b, TAPE);
        c.save();
        c.clip_rect(l, t, r, b);
        let mut v = floor_to(s - 60.0, 10).max(0);
        while v as f32 <= s + 60.0 {
            let y = CY - (v as f32 - s) * k;
            if v % 20 == 0 {
                c.line(r - 20.0, y, r, y, WHITE, 3.0);
                c.text_r(&v.to_string(), r - 28.0, y, 38.0, WHITE);
            } else {
                c.line(r - 12.0, y, r, y, WHITE, 3.0);
            }
            v += 10;
        }
        let vmo = sl::vmo(d, 320.0);
        if s + 60.0 > vmo {
            sl::barber_up(c, r, 12.0, t, CY - (vmo - s) * k);
        }
        sl::low_speed(c, d, r - 12.0, 12.0, |v| CY - (v - s) * k, b);
        if !d.spd_bug.is_nan() {
            sl::speed_bug(c, r - 2.0, (CY - (d.spd_bug - s) * k).clamp(t, b), CYAN, false);
        }
        c.restore();
        if let Some(t2) = sl::sel_speed(d) {
            c.text(&t2, (l + r) / 2.0, t - 22.0, 32.0, CYAN);
        }
        c.frame(l, t, r, b, GREY, 2.0);
        sl::trend_arrow(c, r + 12.0, CY, d.speed_trend.clamp(-50.0, 50.0), k, CYAN, 4.0, 14.0);
        c.poly(BLACK, WHITE, 3.0, &[l - 16.0, CY - 30.0, r + 2.0, CY - 30.0, r + 2.0, CY - 14.0, r + 24.0, CY, r + 2.0, CY + 14.0, r + 2.0, CY + 30.0, l - 16.0, CY + 30.0]);
        c.text_r(&roundi(s).to_string(), r - 4.0, CY, 44.0, WHITE);
        if let Some(m) = sl::mach_text(d) {
            c.text_l(&m, l, b + 34.0, 32.0, WHITE);
        }
    }

    fn altitude(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r, t, b, k) = (648.0, 742.0, 110.0, 506.0, 0.42);
        if !d.alt_valid {
            c.text("ALT", (l + r) / 2.0, CY, 42.0, AMBER);
            return;
        }
        let a = d.alt_ft;
        c.rect(l, t, r, b, TAPE);
        c.save();
        c.clip_rect(l, t, r, b);
        let mut v = floor_to(a - 700.0, 100);
        while v as f32 <= a + 700.0 {
            let y = CY - (v as f32 - a) * k;
            c.line(l, y, l + 16.0, y, WHITE, 3.0);
            if imod(v, 500) == 0 {
                c.text_l(&v.to_string(), l + 24.0, y, 32.0, WHITE);
            }
            v += 100;
        }
        if !d.terrain_ft.is_nan() && !d.std {
            let yt = CY - (d.terrain_ft - a) * k;
            if yt < b {
                c.rect(l, yt.max(t), l + 10.0, b, RED);
                c.line(l, yt, r, yt, RED, 3.0);
            }
        }
        if !d.alt_bug.is_nan() {
            sl::alt_bug(c, l, (CY - (d.alt_bug - a) * k).clamp(t, b), CYAN);
        }
        c.restore();
        c.frame(l, t, r, b, GREY, 2.0);
        if !d.alt_bug.is_nan() {
            c.text(&roundi(d.alt_bug).to_string(), (l + r) / 2.0, t - 22.0, 32.0, CYAN);
        }
        c.poly(BLACK, WHITE, 3.0, &[l - 20.0, CY, l - 2.0, CY - 14.0, l - 2.0, CY - 32.0, r + 14.0, CY - 32.0, r + 14.0, CY + 32.0, l - 2.0, CY + 32.0, l - 2.0, CY + 14.0]);
        let hundreds = (a / 100.0).floor() as i32;
        c.text_r(&if hundreds < 0 { format!("-{}", hundreds.abs()) } else { hundreds.to_string() }, l + 62.0, CY, 44.0, WHITE);
        c.drum(l + 64.0, CY - 30.0, r + 12.0, CY + 30.0, a / 20.0, 26.0, 28.0, WHITE, l + 84.0, Align::Left, |kk| format!("{:02}", imod(kk * 20, 100)));
    }

    fn vy(v: f32) -> f32 {
        let av = v.abs().min(6000.0);
        let g = if av <= 1000.0 {
            av * 0.12
        } else if av <= 2000.0 {
            120.0 + (av - 1000.0) * 0.055
        } else {
            175.0 + (av - 2000.0) * 0.0115
        };
        CY - sign(v) * g
    }

    fn vsi(&self, c: &mut Canvas, d: &FlightData) {
        let l = 748.0;
        c.line(l, 120.0, l, 500.0, GREY, 2.0);
        for v in [500, 1000, 2000, 6000] {
            for sg in [-1, 1] {
                let y = Self::vy((sg * v) as f32);
                c.line(l, y, l + if v == 500 { 10.0 } else { 20.0 }, y, WHITE, 3.0);
                if v != 500 {
                    c.text(&(v / 1000).to_string(), l + 40.0, y, 26.0, WHITE);
                }
            }
        }
        if !d.vs_bug.is_nan() {
            c.line(l - 4.0, Self::vy(d.vs_bug), l + 24.0, Self::vy(d.vs_bug), CYAN, 6.0);
        }
        if !d.alt_valid {
            return;
        }
        let vs = d.vs_fpm;
        let col = if vs.abs() > 2000.0 { AMBER } else { GREEN };
        c.save();
        c.clip_rect(l - 40.0, 115.0, 800.0, 505.0);
        c.line(l - 40.0, CY, l + 30.0, Self::vy(vs), col, 5.0);
        c.restore();
    }

    fn baro(&self, c: &mut Canvas, d: &FlightData) {
        let y = 514.0;
        c.text_l("BARO", 626.0, y, 26.0, CYAN);
        c.text_l(&if d.std { "STD".into() } else { sl::baro_text(d) }, 626.0, y + 34.0, 34.0, GREEN);
    }

    fn data_line(&self, c: &mut Canvas, d: &FlightData) {
        let y = 566.0;
        c.text_l("GSPD", 24.0, y, 26.0, CYAN);
        c.text_l(&if d.gs_kt.is_nan() { "--- KT".into() } else { format!("{} KT", roundi(d.gs_kt)) }, 24.0, y + 34.0, 30.0, GREEN);
        c.text_l("HDG", 200.0, y, 26.0, CYAN);
        let h = if !d.hdg_bug.is_nan() { d.hdg_bug } else { d.heading };
        c.text_l(&format!("{:03}", imod(roundi(h), 360)), 200.0, y + 34.0, 30.0, GREEN);
        c.text_l("MAG", 268.0, y + 34.0, 24.0, CYAN);
        let n = &d.nav;
        let src = match n.source {
            NavSource::Gps => Some("FMS1"),
            NavSource::Loc => Some("LOC1"),
            NavSource::Vor => Some("VOR1"),
            NavSource::None => None,
        };
        let nav_col = if n.source == NavSource::Gps { MAGENTA } else { GREEN };
        if let Some(s) = src {
            c.text_l(s, 24.0, y + 76.0, 26.0, nav_col);
        }
        if let Some(i) = &n.ident {
            c.text_l(i, 24.0, y + 108.0, 26.0, nav_col);
        }
        if !n.dme_nm.is_nan() {
            c.text_l(&format!("{:.1} NM", n.dme_nm), 24.0, y + 140.0, 26.0, nav_col);
        }
        if !n.course.is_nan() {
            c.text_l(&format!("CRS {:03}", imod(roundi(n.course), 360)), 24.0, y + 172.0, 26.0, nav_col);
        }
        c.text_l("TRK", 580.0, y, 26.0, CYAN);
        c.text_l(&if d.has_track { format!("{:03}", imod(roundi(d.track), 360)) } else { "---".into() }, 580.0, y + 34.0, 30.0, MAGENTA);
        if !d.tas_kt.is_nan() {
            c.text_l("TAS", 690.0, y, 26.0, CYAN);
            c.text_l(&roundi(d.tas_kt).to_string(), 690.0, y + 34.0, 30.0, GREEN);
        }
        if !d.oat_c.is_nan() {
            c.text_l("SAT", 690.0, y + 76.0, 26.0, CYAN);
            c.text_l(&format!("{}°C", roundi(d.oat_c)), 690.0, y + 110.0, 28.0, GREEN);
        }
        if d.wind_valid && d.wind_kt >= 1.0 {
            c.text_l(&format!("{:03}°/{}", imod(roundi(d.wind_dir), 360), roundi(d.wind_kt)), 24.0, 974.0, 26.0, WHITE);
        }
        if let Some(id) = &d.gps.next_id {
            let dist = if d.gps.next_dist_nm.is_nan() { String::new() } else { format!(" {:.1} NM", d.gps.next_dist_nm) };
            c.text_r(&format!("{id}{dist}"), 776.0, 974.0, 26.0, MAGENTA);
        }
    }

    fn rose(&self, c: &mut Canvas, d: &FlightData) {
        if !d.att_valid {
            c.circle(RX, RY, RR, GREY, 3.0);
            c.text("HDG", RX, RY, 42.0, AMBER);
            return;
        }
        let h = d.heading;
        c.poly(MAGENTA, MAGENTA, 2.0, &[RX, RY - RR + 2.0, RX - 13.0, RY - RR - 24.0, RX + 13.0, RY - RR - 24.0]);
        c.save();
        c.rotate(-h, RX, RY);
        let mut deg = 0;
        while deg < 360 {
            let ten = deg % 10 == 0;
            c.radial(RX, RY, deg as f32, RR - if ten { 24.0 } else { 13.0 }, RR, WHITE, if ten { 3.5 } else { 2.5 });
            if deg % 30 == 0 {
                let rad = (deg as f32).to_radians();
                let rr = RR - 46.0;
                let (tx, ty) = (RX + rr * rad.sin(), RY - rr * rad.cos());
                let lbl = match deg {
                    0 => "N".into(),
                    90 => "E".into(),
                    180 => "S".into(),
                    270 => "W".into(),
                    _ => (deg / 10).to_string(),
                };
                c.save();
                c.rotate(deg as f32, tx, ty);
                c.text(&lbl, tx, ty, 28.0, WHITE);
                c.restore();
            }
            deg += 5;
        }
        if !d.hdg_bug.is_nan() {
            c.save();
            c.rotate(d.hdg_bug, RX, RY);
            c.poly(0, CYAN, 3.5, &[RX - 14.0, RY - RR - 2.0, RX - 14.0, RY - RR + 12.0, RX + 14.0, RY - RR + 12.0, RX + 14.0, RY - RR - 2.0, RX + 5.0, RY - RR - 2.0, RX, RY - RR + 6.0, RX - 5.0, RY - RR - 2.0]);
            c.restore();
        }
        let n = &d.nav;
        if !n.course.is_nan() {
            let col = if n.source == NavSource::Gps { MAGENTA } else { GREEN };
            c.save();
            c.rotate(n.course, RX, RY);
            c.line(RX, RY - RR + 30.0, RX, RY - 60.0, col, 4.0);
            c.poly(col, col, 2.0, &[RX, RY - RR + 22.0, RX - 10.0, RY - RR + 42.0, RX + 10.0, RY - RR + 42.0]);
            c.line(RX, RY + 60.0, RX, RY + RR - 30.0, col, 4.0);
            for i in [-2.0, -1.0, 1.0, 2.0] {
                c.circle(RX + i * 40.0, RY, 5.0, WHITE, 2.5);
            }
            if !n.cdi.is_nan() {
                let dx = n.cdi.clamp(-2.5, 2.5) * 40.0;
                c.line(RX + dx, RY - 54.0, RX + dx, RY + 54.0, col, 5.0);
            }
            c.restore();
        } else if d.has_track {
            c.save();
            c.rotate(d.track, RX, RY);
            c.line(RX, RY - RR, RX, RY + RR, MAGENTA, 3.0);
            c.restore();
        }
        c.restore();
        c.poly(0, WHITE, 3.5, &[RX, RY - 26.0, RX - 22.0, RY + 16.0, RX + 22.0, RY + 16.0]);
        c.line(RX, RY + 16.0, RX, RY + 28.0, WHITE, 3.5);
    }
}

fn aircraft(c: &mut Canvas) {
    c.poly(BLACK, WHITE, 3.5, &[CX - 178.0, CY - 6.0, CX - 86.0, CY - 6.0, CX - 86.0, CY + 30.0, CX - 70.0, CY + 30.0, CX - 70.0, CY + 8.0, CX - 178.0, CY + 8.0]);
    c.poly(BLACK, WHITE, 3.5, &[CX + 178.0, CY - 6.0, CX + 86.0, CY - 6.0, CX + 86.0, CY + 30.0, CX + 70.0, CY + 30.0, CX + 70.0, CY + 8.0, CX + 178.0, CY + 8.0]);
    c.rect(CX - 7.0, CY - 7.0, CX + 7.0, CY + 7.0, BLACK);
    c.frame(CX - 7.0, CY - 7.0, CX + 7.0, CY + 7.0, WHITE, 3.5);
}

impl Pfd for E190Pfd {
    fn vw(&self) -> f32 {
        800.0
    }
    fn draw(&mut self, c: &mut Canvas, d: &FlightData) {
        self.fma(c, d);
        self.attitude(c, d);
        self.speed(c, d);
        self.altitude(c, d);
        self.vsi(c, d);
        self.baro(c, d);
        self.data_line(c, d);
        self.rose(c, d);
    }
}
