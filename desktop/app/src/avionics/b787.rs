//! PFD Boeinga 787 (Rockwell Collins) – przeniesiony z B787Pfd.kt: horyzont na pełną szerokość,
//! półprzezroczyste taśmy, łukowa róża kursowa u dołu.

use super::data::*;
use super::gfx::*;
use super::simlayer as sl;
use super::Pfd;

const ADI_T: f32 = 62.0;
const ADI_B: f32 = 600.0;
const CX: f32 = 450.0;
const CY: f32 = 318.0;
const PPD: f32 = 9.2;
const RCX: f32 = 450.0;
const RCY: f32 = 980.0;
const RAD: f32 = 300.0;
const SKY_TOP: Argb = 0xFF0A62C8;
const SKY: Argb = 0xFF1E90E8;
const GROUND: Argb = 0xFF8A4A18;
const GROUND_BOT: Argb = 0xFF5A2E0C;
const TAPE: Argb = 0x99303338;
const FMA_BG: Argb = 0xFF26282C;
const GREEN: Argb = 0xFF3EEA4C;
const CYAN: Argb = 0xFF33DDFF;
const MAGENTA: Argb = 0xFFEE55DD;
const GREY: Argb = 0xFF9A9A9A;

pub struct B787Pfd;

impl B787Pfd {
    fn attitude(&self, c: &mut Canvas, d: &FlightData) {
        if !d.att_valid {
            c.rect(0.0, ADI_T, 900.0, ADI_B, BLACK);
            c.frame(CX - 56.0, CY - 30.0, CX + 56.0, CY + 30.0, AMBER, 3.0);
            c.text("ATT", CX, CY, 44.0, AMBER);
            aircraft(c);
            return;
        }
        c.save();
        c.clip_rect(0.0, ADI_T, 900.0, ADI_B);
        c.save();
        c.rotate(-d.roll, CX, CY);
        c.translate(0.0, d.pitch * PPD);
        c.rect_vgrad(CX - 1400.0, CY - 3000.0, CX + 1400.0, CY, ADI_T, SKY_TOP, CY, SKY);
        c.rect_vgrad(CX - 1400.0, CY, CX + 1400.0, CY + 3000.0, CY, GROUND, ADI_B, GROUND_BOT);
        c.line(CX - 1400.0, CY, CX + 1400.0, CY, WHITE, 3.0);
        c.save();
        c.clip_rect(CX - 210.0, CY - 230.0 - d.pitch * PPD, CX + 210.0, CY + 210.0 - d.pitch * PPD);
        let i0 = ((d.pitch - 24.0) / 2.5).ceil() as i32;
        let i1 = ((d.pitch + 24.0) / 2.5).floor() as i32;
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
                96.0
            } else if i % 2 == 0 {
                46.0
            } else {
                22.0
            };
            c.line(CX - half, y, CX + half, y, WHITE, 3.0);
            if i % 4 == 0 {
                let lbl = roundi(deg.abs()).to_string();
                c.text(&lbl, CX - half - 28.0, y, 30.0, WHITE);
                c.text(&lbl, CX + half + 28.0, y, 30.0, WHITE);
            }
        }
        c.restore();
        c.restore();
        if d.fpv && d.has_track && d.speed_kt > 5.0 {
            c.save();
            c.rotate(-d.roll, CX, CY);
            let dx = ang_diff(d.track, d.heading).clamp(-18.0, 18.0) * PPD;
            let dy = (d.fpa - d.pitch).clamp(-18.0, 18.0) * PPD;
            sl::bird(c, CX + dx, CY - dy, WHITE, 13.0, 38.0, 28.0, 3.5);
            c.restore();
        }
        c.restore();
        if d.pitch > 30.0 || d.pitch < -20.0 {
            let dir = if d.pitch > 0.0 { 1.0 } else { -1.0 };
            c.save();
            c.clip_rect(0.0, ADI_T, 900.0, ADI_B);
            c.rotate(-d.roll, CX, CY);
            for k in 1..=2 {
                let y = CY + dir * (70.0 + k as f32 * 62.0);
                c.line(CX - 66.0, y + dir * 42.0, CX, y, RED, 6.0);
                c.line(CX, y, CX + 66.0, y + dir * 42.0, RED, 6.0);
            }
            c.restore();
        }
        // skala przechylenia
        c.save();
        c.clip_rect(0.0, ADI_T, 900.0, ADI_B);
        let r = 232.0;
        for a in [10, 20, 30, 45, 60] {
            for s in [-1, 1] {
                let len = if a == 30 || a == 60 { 26.0 } else { 16.0 };
                c.radial(CX, CY, (s * a) as f32, r, r + len, WHITE, 3.0);
            }
        }
        c.poly(0, WHITE, 3.0, &[CX, CY - r, CX - 16.0, CY - r - 26.0, CX + 16.0, CY - r - 26.0]);
        c.save();
        c.rotate(-d.roll, CX, CY);
        let col = if d.roll.abs() > 35.0 { AMBER } else { WHITE };
        c.poly(col, col, 2.0, &[CX, CY - r + 4.0, CX - 16.0, CY - r + 30.0, CX + 16.0, CY - r + 30.0]);
        let off = (d.slip * 180.0).clamp(-44.0, 44.0);
        c.poly(0, if d.slip.abs() > 0.2 { AMBER } else { col }, 3.0, &[CX - 17.0 + off, CY - r + 36.0, CX + 17.0 + off, CY - r + 36.0, CX + 22.0 + off, CY - r + 48.0, CX - 22.0 + off, CY - r + 48.0]);
        c.restore();
        c.restore();
        sl::fd_cross_bars(c, d, CX, CY, PPD, 110.0, MAGENTA, 5.0);
        aircraft(c);
        sl::deviation_std(c, d, CX, 545.0, 650.0, CY, 34.0, MAGENTA);
        sl::marker(c, d, 630.0, 110.0, 28.0);
        if let Some(n) = sl::nav_line(d) {
            c.text_l(&n, 240.0, 92.0, 26.0, WHITE);
        }
        if d.stall_warn {
            c.text_b("STALL", CX, CY - 120.0, 50.0, RED);
        }
    }

    fn speed(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r, t, b, k) = (120.0, 215.0, 92.0, 560.0, 5.2);
        if !d.speed_valid {
            c.text("SPD", (l + r) / 2.0, CY, 44.0, AMBER);
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
                c.line(r - 22.0, y, r, y, WHITE, 3.0);
                c.text_r(&v.to_string(), r - 32.0, y, 40.0, WHITE);
            } else {
                c.line(r - 12.0, y, r, y, WHITE, 3.0);
            }
            v += 10;
        }
        let vmo = sl::vmo(d, 350.0);
        if s + 60.0 > vmo {
            sl::barber_up(c, r, 12.0, t, CY - (vmo - s) * k);
        }
        sl::low_speed(c, d, r - 12.0, 12.0, |v| CY - (v - s) * k, b);
        if !d.spd_bug.is_nan() && d.mach_bug.is_nan() {
            sl::speed_bug(c, r - 2.0, (CY - (d.spd_bug - s) * k).clamp(t, b), MAGENTA, false);
        }
        c.restore();
        sl::trend_arrow(c, r + 10.0, CY, d.speed_trend.clamp(-50.0, 50.0), k, GREEN, 4.0, 15.0);
        c.poly(BLACK, WHITE, 3.0, &[l - 20.0, CY - 30.0, r + 6.0, CY - 30.0, r + 6.0, CY - 16.0, r + 26.0, CY, r + 6.0, CY + 16.0, r + 6.0, CY + 30.0, l - 20.0, CY + 30.0]);
        c.text_r(&roundi(s).to_string(), r, CY, 46.0, WHITE);
        if let Some(t2) = sl::sel_speed(d) {
            c.text(&t2, (l + r) / 2.0, t - 16.0, 36.0, MAGENTA);
        }
    }

    fn altitude(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r, t, b, k) = (690.0, 798.0, 92.0, 560.0, 0.52);
        if !d.alt_valid {
            c.text("ALT", (l + r) / 2.0, CY, 44.0, AMBER);
            return;
        }
        let a = d.alt_ft;
        c.rect(l, t, r, b, TAPE);
        c.save();
        c.clip_rect(l, t, r, b);
        let mut v = floor_to(a - 500.0, 100);
        while v as f32 <= a + 500.0 {
            let y = CY - (v as f32 - a) * k;
            c.line(l, y, l + 16.0, y, WHITE, 3.0);
            if imod(v, 200) == 0 {
                c.text_l(&v.to_string(), l + 26.0, y, 34.0, WHITE);
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
            sl::alt_bug(c, l, (CY - (d.alt_bug - a) * k).clamp(t, b), MAGENTA);
        }
        c.restore();
        if !d.alt_bug.is_nan() {
            c.text(&roundi(d.alt_bug).to_string(), (l + r) / 2.0, t - 16.0, 36.0, MAGENTA);
        }
        c.poly(BLACK, WHITE, 3.0, &[l - 24.0, CY - 34.0, l - 4.0, CY, l - 24.0, CY + 34.0, r + 6.0, CY + 34.0, r + 6.0, CY - 34.0]);
        let hundreds = (a / 100.0).floor() as i32;
        c.text_r(&if hundreds < 0 { format!("-{}", hundreds.abs()) } else { hundreds.to_string() }, l + 62.0, CY, 46.0, WHITE);
        c.drum(l + 64.0, CY - 32.0, r + 4.0, CY + 32.0, a / 20.0, 28.0, 28.0, WHITE, l + 86.0, Align::Left, |kk| format!("{:02}", imod(kk * 20, 100)));
    }

    fn vy(v: f32) -> f32 {
        let av = v.abs().min(6000.0);
        let g = if av <= 1000.0 {
            av * 0.13
        } else if av <= 2000.0 {
            130.0 + (av - 1000.0) * 0.06
        } else {
            190.0 + (av - 2000.0) * 0.0125
        };
        CY - sign(v) * g
    }

    fn vsi(&self, c: &mut Canvas, d: &FlightData) {
        let l = 810.0;
        c.line(l, 110.0, l, 545.0, GREY, 2.5);
        for v in [500, 1000, 2000, 6000] {
            for sg in [-1, 1] {
                let y = Self::vy((sg * v) as f32);
                c.line(l, y, l + if v == 500 { 12.0 } else { 22.0 }, y, WHITE, 3.0);
                if v != 500 {
                    c.text(&(v / 1000).to_string(), l + 42.0, y, 28.0, WHITE);
                }
            }
        }
        if !d.vs_bug.is_nan() {
            c.line(l - 4.0, Self::vy(d.vs_bug), l + 26.0, Self::vy(d.vs_bug), MAGENTA, 6.0);
        }
        if !d.alt_valid {
            return;
        }
        let vs = d.vs_fpm;
        let col = if vs.abs() > 2000.0 { AMBER } else { WHITE };
        c.save();
        c.clip_rect(l - 60.0, 100.0, 900.0, 555.0);
        c.line(l - 60.0, CY, l + 36.0, Self::vy(vs), col, 5.0);
        c.restore();
        if vs.abs() >= 400.0 {
            c.text(&roundi(vs.abs() / 100.0).to_string(), l + 30.0, if vs > 0.0 { 96.0 } else { 560.0 }, 30.0, col);
        }
    }

    fn fma(&self, c: &mut Canvas, d: &FlightData) {
        c.rect(0.0, 0.0, 900.0, ADI_T, FMA_BG);
        c.line(0.0, ADI_T, 900.0, ADI_T, GREY, 2.0);
        for x in [300.0, 505.0, 710.0] {
            c.line(x, 6.0, x, ADI_T - 6.0, GREY, 2.0);
        }
        let f = &d.fma;
        let (y, y2, sz) = (22.0, 48.0, 30.0);
        let athr = if f.toga {
            Some("TO/GA")
        } else if f.athr_active && f.vertical == Some(Vertical::Flc) {
            Some(if d.vs_fpm >= 0.0 { "THR REF" } else { "IDLE" })
        } else if f.athr_active {
            Some("SPD")
        } else if f.athr_armed {
            Some("HOLD")
        } else {
            None
        };
        if let Some(a) = athr {
            c.text(a, 150.0, y, sz, GREEN);
        }
        let roll = match f.lateral {
            Some(Lateral::Hdg) => Some("HDG SEL"),
            Some(Lateral::NavGps) => Some("LNAV"),
            Some(Lateral::NavVor) => Some("VOR"),
            Some(Lateral::Loc) => Some("LOC"),
            Some(Lateral::Bc) => Some("B/CRS"),
            Some(Lateral::Rol) => Some("HDG HOLD"),
            None => None,
        };
        if let Some(r) = roll {
            c.text(r, 402.0, y, sz, GREEN);
        }
        if f.lateral_armed == Some(Lateral::Loc) {
            c.text("LOC", 402.0, y2, 24.0, WHITE);
        }
        let pitch = match f.vertical {
            Some(Vertical::Alt) => Some("ALT"),
            Some(Vertical::Vs) => Some("V/S"),
            Some(Vertical::Flc) => Some("FLCH SPD"),
            Some(Vertical::Gs) => Some("G/S"),
            Some(Vertical::Pit) => Some("FPA"),
            _ => None,
        };
        if let Some(p) = pitch {
            c.text(p, 607.0, y, sz, GREEN);
        }
        if f.vertical_armed.contains(&Vertical::Gs) {
            c.text("G/S", 607.0, y2, 24.0, WHITE);
        }
        let status = if f.ap && f.lateral == Some(Lateral::Loc) && f.vertical == Some(Vertical::Gs) {
            Some("LAND 3")
        } else if f.ap {
            Some("A/P")
        } else if f.fd {
            Some("FLT DIR")
        } else {
            None
        };
        if let Some(s) = status {
            c.text(s, 805.0, y + 8.0, sz, GREEN);
        }
    }

    fn below_adi(&self, c: &mut Canvas, d: &FlightData) {
        c.rect(0.0, ADI_B, 900.0, 1000.0, BLACK);
        if let Some(m) = sl::mach_text(d) {
            c.text_l(&m, 130.0, ADI_B - 34.0, 36.0, WHITE);
        }
        if d.ra_valid && d.ra_ft < 2500.0 {
            let ra = if d.ra_ft < 50.0 { roundi(d.ra_ft) } else { roundi(d.ra_ft / 10.0) * 10 };
            c.text_l("RADIO", 560.0, ADI_B - 64.0, 26.0, GREEN);
            c.text_l(&ra.to_string(), 560.0, ADI_B - 30.0, 40.0, if d.ra_ft < 200.0 { AMBER } else { GREEN });
        }
        if !d.minimums_ft.is_nan() {
            c.text_l("MINS", 440.0, ADI_B - 64.0, 24.0, GREEN);
            c.text_l(&roundi(d.minimums_ft).to_string(), 440.0, ADI_B - 30.0, 34.0, GREEN);
        }
        let y = 642.0;
        if d.std {
            c.text("STD", 790.0, y, 36.0, GREEN);
            c.frame(735.0, y - 26.0, 845.0, y + 26.0, AMBER, 2.5);
        } else {
            c.text_l("QNH", 690.0, y, 28.0, WHITE);
            c.text(&sl::baro_text(d), 800.0, y, 36.0, GREEN);
            c.frame(745.0, y - 26.0, 855.0, y + 26.0, AMBER, 2.5);
            c.text_r(if d.in_hg { "IN" } else { "HPA" }, 895.0, y, 24.0, CYAN);
        }
        if !d.gs_kt.is_nan() {
            c.text_l(&format!("GS {}", roundi(d.gs_kt)), 24.0, 642.0, 30.0, WHITE);
        }
        if !d.tas_kt.is_nan() {
            c.text_l(&format!("TAS {}", roundi(d.tas_kt)), 24.0, 682.0, 28.0, WHITE);
        }
        if d.wind_valid && d.wind_kt >= 1.0 {
            c.text_l(&format!("{:03}°/{}", imod(roundi(d.wind_dir), 360), roundi(d.wind_kt)), 24.0, 762.0, 28.0, WHITE);
        }
        if !d.oat_c.is_nan() {
            c.text_l(&format!("OAT {}°C", roundi(d.oat_c)), 24.0, 722.0, 28.0, GREEN);
        }
    }

    fn arc_rose(&self, c: &mut Canvas, d: &FlightData) {
        if !d.att_valid {
            c.text("HDG", RCX, RCY - RAD / 2.0, 44.0, AMBER);
            return;
        }
        let h = d.heading;
        c.poly(WHITE, WHITE, 2.0, &[RCX, RCY - RAD + 4.0, RCX - 14.0, RCY - RAD - 22.0, RCX + 14.0, RCY - RAD - 22.0]);
        c.save();
        c.clip_rect(0.0, ADI_B + 60.0, 900.0, 1000.0);
        c.save();
        c.rotate(-h, RCX, RCY);
        let mut deg = 0;
        while deg < 360 {
            let ten = deg % 10 == 0;
            c.radial(RCX, RCY, deg as f32, RAD - if ten { 26.0 } else { 14.0 }, RAD, WHITE, if ten { 3.5 } else { 2.5 });
            if deg % 30 == 0 {
                let rad = (deg as f32).to_radians();
                let rr = RAD - 56.0;
                let (tx, ty) = (RCX + rr * rad.sin(), RCY - rr * rad.cos());
                let lbl = match deg {
                    0 => "N".into(),
                    90 => "E".into(),
                    180 => "S".into(),
                    270 => "W".into(),
                    _ => (deg / 10).to_string(),
                };
                c.save();
                c.rotate(deg as f32, tx, ty);
                c.text(&lbl, tx, ty, 32.0, WHITE);
                c.restore();
            }
            deg += 5;
        }
        if !d.hdg_bug.is_nan() {
            c.save();
            c.rotate(d.hdg_bug, RCX, RCY);
            let y = RCY - RAD;
            c.poly(0, MAGENTA, 4.0, &[RCX - 20.0, y - 2.0, RCX - 20.0, y + 16.0, RCX + 20.0, y + 16.0, RCX + 20.0, y - 2.0, RCX + 7.0, y - 2.0, RCX, y + 8.0, RCX - 7.0, y - 2.0]);
            c.restore();
        }
        if d.has_track {
            c.save();
            c.rotate(d.track, RCX, RCY);
            c.line(RCX, RCY, RCX, RCY - RAD, GREEN, 3.0);
            c.restore();
        }
        c.restore();
        c.restore();
        c.line(RCX - 14.0, RCY - RAD / 2.0, RCX + 14.0, RCY - RAD / 2.0, WHITE, 3.0);
        c.poly(BLACK, WHITE, 3.5, &[RCX, RCY - 44.0, RCX - 26.0, RCY + 4.0, RCX + 26.0, RCY + 4.0]);
        if d.has_track {
            c.text_l(&format!("TRK {:03}°", imod(roundi(d.track), 360)), 24.0, 880.0, 28.0, MAGENTA);
        }
        if let Some(id) = &d.gps.next_id {
            c.text_r(id, 876.0, 880.0, 30.0, MAGENTA);
            if !d.gps.next_dist_nm.is_nan() {
                c.text_r(&format!("{:.1} NM", d.gps.next_dist_nm), 876.0, 918.0, 26.0, WHITE);
            }
        }
    }
}

fn aircraft(c: &mut Canvas) {
    c.poly(BLACK, WHITE, 3.5, &[CX - 215.0, CY - 6.0, CX - 95.0, CY - 6.0, CX - 95.0, CY + 34.0, CX - 76.0, CY + 34.0, CX - 76.0, CY + 10.0, CX - 215.0, CY + 10.0]);
    c.poly(BLACK, WHITE, 3.5, &[CX + 215.0, CY - 6.0, CX + 95.0, CY - 6.0, CX + 95.0, CY + 34.0, CX + 76.0, CY + 34.0, CX + 76.0, CY + 10.0, CX + 215.0, CY + 10.0]);
    c.circle(CX, CY, 13.0, BLACK, 0.0);
    c.circle(CX, CY, 13.0, WHITE, 3.5);
}

impl Pfd for B787Pfd {
    fn vw(&self) -> f32 {
        900.0
    }
    fn draw(&mut self, c: &mut Canvas, d: &FlightData) {
        self.attitude(c, d);
        self.speed(c, d);
        self.altitude(c, d);
        self.vsi(c, d);
        self.fma(c, d);
        self.below_adi(c, d);
        self.arc_rose(c, d);
    }
}
