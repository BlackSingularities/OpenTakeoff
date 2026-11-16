//! PFD Boeinga 737NG/MAX – przeniesiony z BoeingPfd.kt: FMA, bugi MCP, FD, ILS, róża kompasowa.

use tiny_skia::Path;

use super::data::*;
use super::gfx::*;
use super::simlayer as sl;
use super::Pfd;

const BX: f32 = 470.0;
const BY: f32 = 470.0;
const AW: f32 = 230.0;
const AH: f32 = 220.0;
const PPD: f32 = 9.5;
const GREEN: Argb = 0xFF3EEA4C;
const CYAN: Argb = 0xFF33DDFF;
const SKY: Argb = 0xFF1A8FD8;
const GROUND: Argb = 0xFF8A4A17;
const TAPE: Argb = 0xFF5E5F63;
const VSI_BG: Argb = 0xFF4A4B4F;
const ROSE: Argb = 0xFF55565A;
const FMA_BG: Argb = 0xFF3A3B3E;
const VMO: f32 = 340.0;
const MAGENTA: Argb = 0xFFEE55DD;
const RCX: f32 = BX;
const RCY: f32 = 1215.0;
const RAD: f32 = 330.0;

pub struct BoeingPfd {
    clip: Path,
}

impl BoeingPfd {
    pub fn new() -> Self {
        Self { clip: round_rect_path(BX - AW, BY - AH, BX + AW, BY + AH, 48.0).unwrap() }
    }

    fn attitude(&self, c: &mut Canvas, d: &FlightData) {
        if !d.att_valid {
            c.frame(BX - 40.0, BY - 110.0, BX + 40.0, BY - 60.0, AMBER, 3.0);
            c.text("ATT", BX, BY - 85.0, 40.0, AMBER);
            aircraft(c);
            return;
        }
        c.save();
        c.clip_path(&self.clip);
        c.save();
        c.rotate(-d.roll, BX, BY);
        c.translate(0.0, d.pitch * PPD);
        c.rect(BX - 1000.0, BY - 3000.0, BX + 1000.0, BY, SKY);
        c.rect(BX - 1000.0, BY, BX + 1000.0, BY + 3000.0, GROUND);
        c.line(BX - 1000.0, BY, BX + 1000.0, BY, WHITE, 3.0);
        let mut h = floor_to(d.heading - 30.0, 5);
        while h as f32 <= d.heading + 30.0 {
            let x = BX + ang_diff(h as f32, d.heading) * PPD;
            let ten = imod(h, 10) == 0;
            c.line(x, BY, x, BY + if ten { 14.0 } else { 8.0 }, WHITE, 2.5);
            if ten {
                c.text(&(imod(h, 360) / 10).to_string(), x, BY + 30.0, 28.0, WHITE);
            }
            h += 5;
        }
        c.save();
        c.clip_rect(BX - 140.0, BY - 170.0 - d.pitch * PPD, BX + 140.0, BY + 150.0 - d.pitch * PPD);
        let i0 = ((d.pitch - 20.0) / 2.5).ceil() as i32;
        let i1 = ((d.pitch + 20.0) / 2.5).floor() as i32;
        for i in i0..=i1 {
            if i == 0 {
                continue;
            }
            let deg = i as f32 * 2.5;
            if deg.abs() > 90.0 {
                continue;
            }
            let y = BY - deg * PPD;
            let half = if i % 4 == 0 {
                82.0
            } else if i % 2 == 0 {
                40.0
            } else {
                18.0
            };
            c.line(BX - half, y, BX + half, y, WHITE, 3.0);
            if i % 4 == 0 {
                let lbl = roundi(deg.abs()).to_string();
                c.text(&lbl, BX - half - 30.0, y, 36.0, WHITE);
                c.text(&lbl, BX + half + 30.0, y, 36.0, WHITE);
            }
        }
        c.restore();
        c.restore();
        if let Some((dx, dy)) = sl::fpv_offset(d, PPD) {
            c.save();
            c.rotate(-d.roll, BX, BY);
            sl::bird(c, BX + dx, BY - dy, WHITE, 13.0, 38.0, 26.0, 3.5);
            c.restore();
        }
        c.restore();

        sl::pitch_chevrons(c, d, BX, BY, &self.clip, 70.0, 60.0, 64.0, 40.0);
        roll_scale(c, d);
        sl::fd_cross_bars(c, d, BX, BY, PPD, 110.0, MAGENTA, 5.0);
        aircraft(c);
        sl::deviation_std(c, d, BX, 722.0, 742.0, BY, 36.0, MAGENTA);
        sl::marker(c, d, 680.0, 290.0, 30.0);
        if !d.minimums_ft.is_nan() {
            c.text_r("RADIO", 690.0, 625.0, 26.0, GREEN);
            c.text_r(&roundi(d.minimums_ft).to_string(), 690.0, 655.0, 32.0, GREEN);
        }
        if let Some(n) = sl::nav_line(d) {
            c.text_l(&n, 250.0, 215.0, 28.0, WHITE);
        }
        if d.stall_warn {
            c.text_b("STALL", BX, BY - 150.0, 50.0, RED);
        }
        if d.ra_valid && d.ra_ft < 2500.0 {
            let ra = if d.ra_ft < 100.0 { roundi(d.ra_ft) } else { roundi(d.ra_ft / 10.0) * 10 };
            c.text(&ra.to_string(), BX, BY + 175.0, 46.0, if d.ra_ft < 200.0 { AMBER } else { WHITE });
        }
    }

    fn speed(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r, t, b, k) = (40.0, 160.0, 170.0, 770.0, 4.2);
        c.rect(l, t, r, b, TAPE);
        if !d.speed_valid {
            c.frame(l + 18.0, BY - 26.0, r - 18.0, BY + 26.0, AMBER, 3.0);
            c.text("SPD", (l + r) / 2.0, BY, 40.0, AMBER);
            return;
        }
        let s = d.speed_kt;
        let vmo = sl::vmo(d, VMO);
        c.save();
        c.clip_rect(l, t, r, b);
        sl::low_speed(c, d, r - 12.0, 12.0, |v| BY - (v - s) * k, b);
        let mut v = floor_to(s - 80.0, 10).max(0);
        while v as f32 <= s + 80.0 {
            let y = BY - (v as f32 - s) * k;
            c.line(r - 20.0, y, r, y, WHITE, 3.0);
            if v % 20 == 0 {
                c.text_r(&v.to_string(), r - 28.0, y, 40.0, WHITE);
            }
            v += 10;
        }
        if s + 80.0 > vmo {
            sl::barber_up(c, r, 12.0, t, BY - (vmo - s) * k);
        }
        if !d.spd_bug.is_nan() && d.mach_bug.is_nan() {
            let y = (BY - (d.spd_bug - s) * k).clamp(t, b);
            sl::speed_bug(c, r - 2.0, y, MAGENTA, false);
        }
        c.restore();
        sl::trend_arrow(c, r + 6.0, BY, d.speed_trend.clamp(-65.0, 65.0), k, GREEN, 3.5, 14.0);
        c.poly(BLACK, WHITE, 3.0, &[14.0, BY - 32.0, 118.0, BY - 32.0, 118.0, BY - 54.0, 152.0, BY - 54.0, 152.0, BY - 12.0, 170.0, BY, 152.0, BY + 12.0, 152.0, BY + 54.0, 118.0, BY + 54.0, 118.0, BY + 32.0, 14.0, BY + 32.0]);
        let tens = (s / 10.0).floor() as i32;
        if tens > 0 {
            c.text_r(&tens.to_string(), 117.0, BY, 54.0, WHITE);
        }
        c.drum(119.0, BY - 52.0, 151.0, BY + 52.0, s, 46.0, 50.0, WHITE, 135.0, Align::Center, |kk| imod(kk, 10).to_string());
        if let Some(t2) = sl::sel_speed(d) {
            c.text(&t2, (l + r) / 2.0, t - 26.0, 42.0, MAGENTA);
        }
        if let Some(m) = sl::mach_text(d) {
            c.text(&m, (l + r) / 2.0, b + 36.0, 38.0, WHITE);
        } else if !d.gs_kt.is_nan() {
            c.text_l("GS", l - 18.0, b + 36.0, 30.0, WHITE);
            c.text_r(&roundi(d.gs_kt).to_string(), r, b + 34.0, 40.0, WHITE);
        }
    }

    fn altitude(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r, t, b, k) = (790.0, 905.0, 170.0, 770.0, 0.3);
        c.rect(l, t, r, b, TAPE);
        if !d.alt_valid {
            c.frame(l + 18.0, BY - 26.0, r - 18.0, BY + 26.0, AMBER, 3.0);
            c.text("ALT", (l + r) / 2.0, BY, 40.0, AMBER);
            return;
        }
        let a = d.alt_ft;
        c.save();
        c.clip_rect(l, t, r, b);
        let mut v = floor_to(a - 1100.0, 100);
        while v as f32 <= a + 1100.0 {
            let y = BY - (v as f32 - a) * k;
            c.line(l, y, l + 16.0, y, WHITE, 3.0);
            if imod(v, 200) == 0 {
                let av = v.abs();
                let th = av / 1000;
                let rest = format!("{:03}", av % 1000);
                let mut x = l + 24.0;
                if th > 0 || v < 0 {
                    let big = format!("{}{}", if v < 0 { "-" } else { "" }, if th > 0 { th.to_string() } else { String::new() });
                    c.text_l(&big, x, y, 42.0, WHITE);
                    x += c.tw(&big, 42.0);
                }
                c.text_l(&rest, x, y + 2.0, 32.0, WHITE);
                if imod(v, 1000) == 0 {
                    c.line(l + 20.0, y - 24.0, r - 6.0, y - 24.0, WHITE, 2.5);
                    c.line(l + 20.0, y + 24.0, r - 6.0, y + 24.0, WHITE, 2.5);
                }
            }
            v += 100;
        }
        if !d.alt_bug.is_nan() {
            sl::alt_bug(c, l, (BY - (d.alt_bug - a) * k).clamp(t, b), MAGENTA);
        }
        c.restore();
        if !d.alt_bug.is_nan() {
            c.text(&roundi(d.alt_bug).to_string(), (l + r) / 2.0 + 10.0, t - 26.0, 42.0, MAGENTA);
        }
        c.poly(BLACK, WHITE, 3.0, &[772.0, BY, 790.0, BY - 12.0, 790.0, BY - 32.0, 880.0, BY - 32.0, 880.0, BY - 54.0, 935.0, BY - 54.0, 935.0, BY + 54.0, 880.0, BY + 54.0, 880.0, BY + 32.0, 790.0, BY + 32.0, 790.0, BY + 12.0]);
        let av = a.abs();
        let th = (av / 1000.0).floor() as i32;
        let hu = ((av % 1000.0) / 100.0).floor() as i32;
        if th > 0 || a < 0.0 {
            let big = format!("{}{}", if a < 0.0 { "-" } else { "" }, if th > 0 { th.to_string() } else { String::new() });
            c.text_r(&big, 850.0, BY, 54.0, WHITE);
        }
        c.text(&hu.to_string(), 865.0, BY + 2.0, 42.0, WHITE);
        c.drum(881.0, BY - 52.0, 934.0, BY + 52.0, av / 20.0, 36.0, 34.0, WHITE, 908.0, Align::Center, |kk| format!("{:02}", imod(kk * 20, 100)));
        let m = roundi(a * 0.3048);
        c.rect(800.0, BY - 104.0, 935.0, BY - 62.0, BLACK);
        c.frame(800.0, BY - 104.0, 935.0, BY - 62.0, WHITE, 2.5);
        c.text_r(&m.to_string(), 905.0, BY - 83.0, 36.0, WHITE);
        c.text_l("M", 910.0, BY - 81.0, 26.0, CYAN);
    }

    fn vy(v: f32) -> f32 {
        let a = v.abs().min(6000.0);
        let g = if a <= 1000.0 {
            a * 0.11
        } else if a <= 2000.0 {
            110.0 + (a - 1000.0) * 0.055
        } else {
            165.0 + (a - 2000.0) * 0.01375
        };
        BY - sign(v) * g
    }

    fn vsi(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r) = (942.0, 998.0);
        c.poly(VSI_BG, 0, 0.0, &[l, 290.0, l + 22.0, 230.0, r, 230.0, r, 710.0, l + 22.0, 710.0, l, 650.0]);
        for v in [500, 1000, 1500, 2000, 6000] {
            for sg in [-1, 1] {
                let y = Self::vy((sg * v) as f32);
                let len = if v == 500 || v == 1500 { 8.0 } else { 14.0 };
                c.line(l + 24.0, y, l + 24.0 + len, y, WHITE, 3.0);
                if v == 1000 || v == 2000 || v == 6000 {
                    c.text(&(v / 1000).to_string(), l + 12.0, y, 26.0, WHITE);
                }
            }
        }
        c.line(l + 22.0, BY, l + 44.0, BY, WHITE, 4.0);
        if !d.vs_bug.is_nan() {
            let y = Self::vy(d.vs_bug);
            c.line(l + 20.0, y, l + 46.0, y, MAGENTA, 6.0);
        }
        if !d.alt_valid {
            return;
        }
        let vs = d.vs_fpm;
        let tip = Self::vy(vs);
        c.save();
        c.clip_rect(l + 24.0, 228.0, r, 712.0);
        c.line(1080.0, BY, l + 24.0, tip, WHITE, 4.0);
        c.restore();
        if vs.abs() >= 400.0 {
            let v50 = roundi(vs.abs() / 50.0) * 50;
            c.text(&v50.to_string(), 972.0, if vs > 0.0 { 208.0 } else { 734.0 }, 30.0, WHITE);
        }
    }

    fn compass(&self, c: &mut Canvas, d: &FlightData) {
        c.save();
        c.clip_rect(0.0, 862.0, 1000.0, 1000.0);
        c.circle(RCX, RCY, RAD, ROSE, 0.0);
        if !d.att_valid {
            c.restore();
            c.text("HDG", RCX, 930.0, 40.0, AMBER);
            return;
        }
        let h = d.heading;
        let mut deg = floor_to(h - 40.0, 5);
        while deg as f32 <= h + 40.0 {
            let ang = ang_diff(deg as f32, h);
            c.save();
            c.rotate(ang, RCX, RCY);
            let ten = imod(deg, 10) == 0;
            c.line(RCX, RCY - RAD, RCX, RCY - RAD + if ten { 24.0 } else { 13.0 }, WHITE, 3.0);
            if ten {
                let n = imod(deg, 360) / 10;
                c.text(&n.to_string(), RCX, RCY - RAD + 50.0, if n % 3 == 0 { 40.0 } else { 30.0 }, WHITE);
            }
            c.restore();
            deg += 5;
        }
        if d.has_track {
            c.save();
            c.rotate(ang_diff(d.track, h), RCX, RCY);
            c.line(RCX, RCY - RAD, RCX, RCY - RAD + 120.0, WHITE, 2.5);
            c.line(RCX - 12.0, RCY - RAD + 70.0, RCX + 12.0, RCY - RAD + 70.0, WHITE, 2.5);
            c.restore();
        }
        if !d.nav.course.is_nan() && d.nav.source != NavSource::Gps {
            c.save();
            c.rotate(ang_diff(d.nav.course, h), RCX, RCY);
            c.line(RCX, RCY - RAD - 10.0, RCX, RCY - RAD + 90.0, MAGENTA, 3.0);
            c.restore();
        }
        if !d.hdg_bug.is_nan() {
            c.save();
            c.rotate(ang_diff(d.hdg_bug, h).clamp(-40.0, 40.0), RCX, RCY);
            let y = RCY - RAD;
            c.poly(0, MAGENTA, 4.0, &[RCX - 22.0, y - 2.0, RCX - 22.0, y + 16.0, RCX + 22.0, y + 16.0, RCX + 22.0, y - 2.0, RCX + 8.0, y - 2.0, RCX, y + 8.0, RCX - 8.0, y - 2.0]);
            c.restore();
        }
        c.restore();
        c.poly(0, WHITE, 3.0, &[RCX, RCY - RAD + 2.0, RCX - 14.0, RCY - RAD - 22.0, RCX + 14.0, RCY - RAD - 22.0]);
        c.text_r(&format!("{:03}", imod(roundi(h), 360)), RCX - 14.0, 968.0, 38.0, MAGENTA);
        c.text_l("MAG", RCX + 14.0, 968.0, 32.0, GREEN);
    }

    fn fma(&self, c: &mut Canvas, d: &FlightData) {
        c.rect(200.0, 12.0, 740.0, 110.0, FMA_BG);
        c.line(380.0, 16.0, 380.0, 106.0, WHITE, 2.5);
        c.line(560.0, 16.0, 560.0, 106.0, WHITE, 2.5);
        let f = &d.fma;
        let athr = if f.toga {
            Some("TO/GA")
        } else if f.athr_active && f.vertical == Some(Vertical::Flc) {
            Some(if d.vs_fpm >= 0.0 { "N1" } else { "RETARD" })
        } else if f.athr_active {
            Some("MCP SPD")
        } else if f.athr_armed {
            Some("ARM")
        } else {
            None
        };
        if let Some(a) = athr {
            c.text(a, 290.0, 42.0, 36.0, GREEN);
        }
        let roll = match f.lateral {
            Some(Lateral::Hdg) => Some("HDG SEL"),
            Some(Lateral::NavGps) => Some("LNAV"),
            Some(Lateral::NavVor) | Some(Lateral::Loc) => Some("VOR/LOC"),
            Some(Lateral::Bc) => Some("B/CRS"),
            Some(Lateral::Rol) if f.ap => Some("CWS R"),
            _ => None,
        };
        if let Some(r) = roll {
            c.text(r, 470.0, 42.0, 36.0, GREEN);
        }
        if f.lateral_armed == Some(Lateral::Loc) {
            c.text("VOR/LOC", 470.0, 88.0, 28.0, WHITE);
        }
        let pitch = match f.vertical {
            Some(Vertical::Alt) => Some("ALT HOLD"),
            Some(Vertical::Vs) => Some("V/S"),
            Some(Vertical::Flc) => Some("MCP SPD"),
            Some(Vertical::Gs) => Some("G/S"),
            Some(Vertical::Pit) if f.ap => Some("CWS P"),
            _ => None,
        };
        if let Some(p) = pitch {
            c.text(p, 650.0, 42.0, 36.0, GREEN);
        }
        if f.vertical_armed.contains(&Vertical::Gs) {
            c.text("G/S", 650.0, 88.0, 28.0, WHITE);
        }
        let status = if f.ap && f.lateral == Some(Lateral::Loc) && f.vertical == Some(Vertical::Gs) {
            Some("SINGLE CH")
        } else if f.ap {
            Some("CMD")
        } else if f.fd {
            Some("FD")
        } else {
            None
        };
        if let Some(s) = status {
            c.text(s, BX, 150.0, 44.0, GREEN);
        }
    }

    fn baro(&self, c: &mut Canvas, d: &FlightData) {
        let y = 822.0;
        let unit = if d.in_hg { " IN." } else { " HPA" };
        if d.std {
            c.text("STD", 850.0, y, 44.0, GREEN);
            c.text_r(&(sl::baro_text(d) + unit), 985.0, 862.0, 28.0, WHITE);
        } else {
            c.text_r(&(sl::baro_text(d) + unit), 985.0, y, 42.0, GREEN);
        }
    }
}

fn roll_scale(c: &mut Canvas, d: &FlightData) {
    let r = 238.0;
    for a in [10, 20, 30, 45, 60] {
        let r2 = if a == 30 || a == 60 { 272.0 } else { 256.0 };
        for sg in [-1.0, 1.0] {
            if a == 45 {
                c.radial(BX, BY, sg * 45.0, r, 252.0, WHITE, 3.0);
            } else {
                c.radial(BX, BY, sg * a as f32, r, r2, WHITE, 3.0);
            }
        }
    }
    c.poly(0, WHITE, 3.0, &[BX, BY - r, BX - 14.0, BY - r - 22.0, BX + 14.0, BY - r - 22.0]);
    c.save();
    c.rotate(-d.roll, BX, BY);
    let col = if d.roll.abs() > 35.0 { AMBER } else { WHITE };
    c.poly(0, col, 3.0, &[BX, BY - r + 2.0, BX - 14.0, BY - r + 24.0, BX + 14.0, BY - r + 24.0]);
    let off = (d.slip * 180.0).clamp(-40.0, 40.0);
    let slip_col = if d.slip.abs() > 0.2 { AMBER } else { col };
    c.poly(0, slip_col, 3.0, &[BX - 14.0 + off, BY - r + 28.0, BX + 14.0 + off, BY - r + 28.0, BX + 14.0 + off, BY - r + 38.0, BX - 14.0 + off, BY - r + 38.0]);
    c.restore();
}

fn aircraft(c: &mut Canvas) {
    let (y, o, i) = (BY, 270.0, 380.0);
    c.poly(BLACK, WHITE, 4.0, &[o, y - 9.0, i, y - 9.0, i, y + 40.0, i - 16.0, y + 40.0, i - 16.0, y + 9.0, o, y + 9.0]);
    let m = 2.0 * BX;
    c.poly(BLACK, WHITE, 4.0, &[m - o, y - 9.0, m - i, y - 9.0, m - i, y + 40.0, m - i + 16.0, y + 40.0, m - i + 16.0, y + 9.0, m - o, y + 9.0]);
    c.rect(BX - 9.0, y - 9.0, BX + 9.0, y + 9.0, BLACK);
    c.frame(BX - 9.0, y - 9.0, BX + 9.0, y + 9.0, WHITE, 4.0);
}

impl Pfd for BoeingPfd {
    fn draw(&mut self, c: &mut Canvas, d: &FlightData) {
        self.attitude(c, d);
        self.speed(c, d);
        self.altitude(c, d);
        self.vsi(c, d);
        self.compass(c, d);
        self.fma(c, d);
        self.baro(c, d);
    }
}
