//! PFD w stylu Garmin G1000 – przeniesiony z GarminPfd.kt: AFCS, radia, HSI, bugi, wiatr, transponder.

use super::data::*;
use super::gfx::*;
use super::simlayer as sl;
use super::Pfd;

const CX: f32 = 500.0;
const CY: f32 = 330.0;
const PPD: f32 = 8.0;
const PANEL: Argb = 0x99101820;
const PANEL_DARK: Argb = 0xCC0A0F16;
const GREEN: Argb = 0xFF2EF03A;
const CYAN: Argb = 0xFF3FE0FF;
const MAGENTA: Argb = 0xFFFF33E6;
const YEL: Argb = 0xFFF5C800;
const YELLOW_DARK: Argb = 0xFFB08A00;
const GREY: Argb = 0xFF9AA0A6;
const LADDER: Argb = 0xCCE6E6E6;
const VS0: f32 = 40.0;
const VFE: f32 = 85.0;
const VS1: f32 = 48.0;
const VNO: f32 = 129.0;
const VNE: f32 = 163.0;
const HX: f32 = CX;
const HY: f32 = 580.0;
const HR: f32 = 120.0;

pub struct GarminPfd;

impl GarminPfd {
    fn attitude(&self, c: &mut Canvas, d: &FlightData) {
        if !d.att_valid {
            c.rect(0.0, 0.0, 1000.0, 750.0, 0xFF20262E);
            c.line(350.0, 190.0, 650.0, 450.0, RED, 8.0);
            c.line(650.0, 190.0, 350.0, 450.0, RED, 8.0);
            c.text("AHRS FAIL", CX, 520.0, 40.0, RED);
            return;
        }
        c.save();
        c.rotate(-d.roll, CX, CY);
        c.translate(0.0, d.pitch * PPD);
        c.rect_vgrad(CX - 1200.0, CY - 3000.0, CX + 1200.0, CY, CY - 900.0, 0xFF0B3C8C, CY, 0xFF6FA8E8);
        c.rect_vgrad(CX - 1200.0, CY, CX + 1200.0, CY + 3000.0, CY, 0xFF3F7A2A, CY + 700.0, 0xFF153F12);
        c.line(CX - 1200.0, CY, CX + 1200.0, CY, WHITE, 4.0);
        let mut h = floor_to(d.heading - 60.0, 10);
        while h as f32 <= d.heading + 60.0 {
            let x = CX + ang_diff(h as f32, d.heading) * PPD;
            c.line(x, CY, x, CY + 14.0, WHITE, 3.0);
            if imod(h, 30) == 0 {
                let n = imod(h, 360);
                c.text(&if n == 0 { "360".into() } else { n.to_string() }, x, CY + 34.0, 28.0, WHITE);
            }
            h += 10;
        }
        c.save();
        c.clip_rect(CX - 170.0, CY - 200.0 - d.pitch * PPD, CX + 170.0, CY + 190.0 - d.pitch * PPD);
        let i0 = ((d.pitch - 25.0) / 2.5).ceil() as i32;
        let i1 = ((d.pitch + 25.0) / 2.5).floor() as i32;
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
                120.0
            } else if i % 2 == 0 {
                55.0
            } else {
                25.0
            };
            c.line(CX - half, y, CX + half, y, LADDER, 4.0);
            if i % 4 == 0 {
                let lbl = roundi(deg.abs()).to_string();
                c.text(&lbl, CX - half - 22.0, y, 26.0, LADDER);
                c.text(&lbl, CX + half + 22.0, y, 26.0, LADDER);
            }
        }
        c.restore();
        c.restore();

        if d.fpv && d.has_track && d.speed_kt > 5.0 {
            c.save();
            c.rotate(-d.roll, CX, CY);
            let x = CX + ang_diff(d.track, d.heading).clamp(-25.0, 25.0) * PPD;
            let y = CY - (d.fpa - d.pitch).clamp(-25.0, 25.0) * PPD;
            sl::bird(c, x, y, GREEN, 13.0, 36.0, 26.0, 4.0);
            c.restore();
        }
        if d.pitch > 30.0 || d.pitch < -20.0 {
            let dir = if d.pitch > 0.0 { 1.0 } else { -1.0 };
            c.save();
            c.rotate(-d.roll, CX, CY);
            for k in 1..=2 {
                let y = CY + dir * (64.0 + k as f32 * 56.0);
                c.line(CX - 58.0, y + dir * 38.0, CX, y, RED, 6.0);
                c.line(CX, y, CX + 58.0, y + dir * 38.0, RED, 6.0);
            }
            c.restore();
        }
        // skala przechylenia
        let r = 196.0;
        for a in [10, 20, 30, 45, 60] {
            let r2 = if a == 30 || a == 60 { r + 32.0 } else { r + 18.0 };
            c.radial(CX, CY, a as f32, r, r2, WHITE, 4.0);
            c.radial(CX, CY, -(a as f32), r, r2, WHITE, 4.0);
        }
        c.poly(WHITE, BLACK, 2.0, &[CX, CY - r - 4.0, CX - 16.0, CY - r - 34.0, CX + 16.0, CY - r - 34.0]);
        c.save();
        c.rotate(-d.roll, CX, CY);
        let col = if d.roll.abs() > 45.0 { YEL } else { WHITE };
        c.poly(col, BLACK, 2.0, &[CX, CY - r + 2.0, CX - 16.0, CY - r + 30.0, CX + 16.0, CY - r + 30.0]);
        let off = (d.slip * 180.0).clamp(-40.0, 40.0);
        c.poly(col, BLACK, 2.0, &[CX - 16.0 + off, CY - r + 34.0, CX + 16.0 + off, CY - r + 34.0, CX + 16.0 + off, CY - r + 44.0, CX - 16.0 + off, CY - r + 44.0]);
        c.restore();

        sl::fd_vbar(c, d, CX, CY, PPD, 95.0, MAGENTA);
        // symbol samolotu „single cue”
        c.poly(YEL, BLACK, 2.5, &[CX, CY, CX - 132.0, CY + 42.0, CX - 98.0, CY + 42.0]);
        c.poly(YELLOW_DARK, BLACK, 2.5, &[CX, CY, CX - 98.0, CY + 42.0, CX - 62.0, CY + 42.0]);
        c.poly(YEL, BLACK, 2.5, &[CX, CY, CX + 132.0, CY + 42.0, CX + 98.0, CY + 42.0]);
        c.poly(YELLOW_DARK, BLACK, 2.5, &[CX, CY, CX + 98.0, CY + 42.0, CX + 62.0, CY + 42.0]);
        c.poly(YEL, BLACK, 2.5, &[CX - 188.0, CY - 5.0, CX - 155.0, CY - 5.0, CX - 155.0, CY + 5.0, CX - 188.0, CY + 5.0]);
        c.poly(YEL, BLACK, 2.5, &[CX + 188.0, CY - 5.0, CX + 155.0, CY - 5.0, CX + 155.0, CY + 5.0, CX + 188.0, CY + 5.0]);
        sl::deviation(c, d, CX, 0.0, 676.0, CY, 30.0, GREEN, WHITE, false);
        sl::marker(c, d, 650.0, 110.0, 26.0);
        if d.stall_warn {
            c.text_b("STALL", CX, CY - 130.0, 46.0, RED);
        }
        if d.ra_valid && d.ra_ft < 2500.0 {
            let ra = if d.ra_ft < 100.0 { roundi(d.ra_ft) } else { roundi(d.ra_ft / 10.0) * 10 };
            c.text_l(&format!("RA {ra}"), CX + 110.0, CY + 160.0, 32.0, if d.ra_ft < 200.0 { YEL } else { GREEN });
        }
    }

    fn speed(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r, t, b, k) = (120.0, 250.0, 86.0, 510.0, 4.4);
        c.rect(l, t, r, b, PANEL);
        if !d.speed_valid {
            c.line(l, t, r, b, RED, 5.0);
            c.line(r, t, l, b, RED, 5.0);
            return;
        }
        let s = d.speed_kt;
        c.save();
        c.clip_rect(l, t, r + 14.0, b);
        let mut v = floor_to(s - 60.0, 5).max(0);
        while v as f32 <= s + 60.0 {
            let y = CY - (v as f32 - s) * k;
            let ten = v % 10 == 0;
            c.line(r - if ten { 22.0 } else { 12.0 }, y, r, y, WHITE, 3.0);
            if ten && v >= 20 {
                c.text_r(&v.to_string(), r - 30.0, y, 36.0, WHITE);
            }
            v += 5;
        }
        let band = |c: &mut Canvas, from: f32, to: f32, x0: f32, x1: f32, col: Argb| c.rect(x0, CY - (to - s) * k, x1, CY - (from - s) * k, col);
        let vs0 = if d.vs0.is_nan() { VS0 } else { d.vs0 };
        let vs1 = if d.vs1.is_nan() { VS1 } else { d.vs1 };
        let vno = if d.vc.is_nan() { VNO } else { d.vc };
        let vne = if d.vmo.is_nan() { VNE } else { d.vmo };
        let vfe = if d.vs0.is_nan() { VFE } else { (vs0 * 2.1).min(vno) };
        band(c, vs0, vfe, r + 1.0, r + 6.0, WHITE);
        band(c, vs1, vno, r + 6.0, r + 13.0, GREEN);
        if vne > vno {
            band(c, vno, vne, r + 6.0, r + 13.0, YEL);
        }
        band(c, vne, vne + 300.0, r + 1.0, r + 13.0, RED);
        if !d.spd_bug.is_nan() {
            sl::speed_bug(c, r - 2.0, (CY - (d.spd_bug - s) * k).clamp(t, b), CYAN, false);
        }
        c.restore();
        if let Some(sel) = sl::sel_speed(d) {
            c.rect(l, t - 44.0, r, t - 4.0, PANEL_DARK);
            c.text(&format!("{sel}{}", if d.mach_bug.is_nan() { "KT" } else { "" }), (l + r) / 2.0, t - 24.0, 30.0, CYAN);
        }
        let tr = d.speed_trend.clamp(-55.0, 55.0);
        if tr.abs() > 2.0 {
            c.rect(r - 6.0, CY.min(CY - tr * k), r, CY.max(CY - tr * k), MAGENTA);
        }
        c.poly(BLACK, WHITE, 2.5, &[l - 10.0, CY - 32.0, r - 50.0, CY - 32.0, r - 50.0, CY - 50.0, r - 14.0, CY - 50.0, r - 14.0, CY - 14.0, r + 4.0, CY, r - 14.0, CY + 14.0, r - 14.0, CY + 50.0, r - 50.0, CY + 50.0, r - 50.0, CY + 32.0, l - 10.0, CY + 32.0]);
        let tens = (s / 10.0).floor() as i32;
        if tens > 0 {
            c.text_r(&tens.to_string(), r - 52.0, CY, 52.0, WHITE);
        }
        c.drum(r - 49.0, CY - 48.0, r - 15.0, CY + 48.0, s, 44.0, 48.0, WHITE, r - 32.0, Align::Center, |kk| imod(kk, 10).to_string());
        c.rect(20.0, b + 8.0, 330.0, b + 50.0, PANEL_DARK);
        c.text_l("GS", 32.0, b + 29.0, 24.0, WHITE);
        c.text_l(&if d.gs_kt.is_nan() { "---".into() } else { format!("{}KT", roundi(d.gs_kt)) }, 78.0, b + 29.0, 28.0, WHITE);
        if !d.tas_kt.is_nan() {
            c.text_l("TAS", 184.0, b + 29.0, 24.0, WHITE);
            c.text_l(&format!("{}KT", roundi(d.tas_kt)), 240.0, b + 29.0, 28.0, WHITE);
        }
    }

    fn altitude(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r, t, b, k) = (700.0, 840.0, 86.0, 510.0, 0.5);
        c.rect(l, t, r, b, PANEL);
        if !d.alt_valid {
            c.line(l, t, r, b, RED, 5.0);
            c.line(r, t, l, b, RED, 5.0);
            return;
        }
        let a = d.alt_ft;
        c.save();
        c.clip_rect(l, t, r, b);
        let mut v = floor_to(a - 500.0, 20);
        while v as f32 <= a + 500.0 {
            let y = CY - (v as f32 - a) * k;
            let hundred = imod(v, 100) == 0;
            c.line(l, y, l + if hundred { 20.0 } else { 10.0 }, y, WHITE, 2.5);
            if hundred {
                c.text_r(&v.to_string(), r - 12.0, y, 34.0, WHITE);
            }
            v += 20;
        }
        if !d.alt_bug.is_nan() {
            sl::alt_bug(c, l, (CY - (d.alt_bug - a) * k).clamp(t, b), CYAN);
        }
        c.restore();
        if !d.alt_bug.is_nan() {
            c.rect(l, t - 44.0, r, t - 4.0, PANEL_DARK);
            c.text(&format!("{}FT", roundi(d.alt_bug)), (l + r) / 2.0, t - 24.0, 30.0, CYAN);
        }
        c.poly(BLACK, WHITE, 2.5, &[l - 16.0, CY, l, CY - 14.0, l, CY - 32.0, r - 50.0, CY - 32.0, r - 50.0, CY - 50.0, r + 4.0, CY - 50.0, r + 4.0, CY + 50.0, r - 50.0, CY + 50.0, r - 50.0, CY + 32.0, l, CY + 32.0, l, CY + 14.0]);
        let hundreds = (a / 100.0).floor() as i32;
        c.text_r(&hundreds.to_string(), r - 52.0, CY, 50.0, WHITE);
        c.drum(r - 49.0, CY - 48.0, r + 3.0, CY + 48.0, a / 20.0, 36.0, 36.0, WHITE, r - 22.0, Align::Center, |kk| format!("{:02}", imod(kk * 20, 100)));
    }

    fn vy(v: f32) -> f32 {
        let a = v.abs().min(4300.0);
        let g = if a <= 1000.0 {
            a * 0.065
        } else if a <= 2000.0 {
            65.0 + (a - 1000.0) * 0.055
        } else {
            120.0 + (a - 2000.0) * 0.032
        };
        CY - sign(v) * g
    }

    fn vsi(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r) = (848.0, 916.0);
        c.rect(l, 86.0, r, 510.0, PANEL);
        for v in [500, 1000, 1500, 2000, 4000] {
            for sg in [-1, 1] {
                let y = Self::vy((sg * v) as f32);
                let major = v == 1000 || v == 2000 || v == 4000;
                c.line(l, y, l + if major { 16.0 } else { 9.0 }, y, WHITE, 2.5);
                if major {
                    c.text_r(&((sg * v) / 1000).to_string(), r - 6.0, y, 26.0, WHITE);
                }
            }
        }
        c.line(l, CY, l + 20.0, CY, WHITE, 3.0);
        if !d.vs_bug.is_nan() {
            let yb = Self::vy(d.vs_bug);
            c.poly(0, CYAN, 3.0, &[l, yb - 10.0, l + 14.0, yb - 10.0, l + 8.0, yb, l + 14.0, yb + 10.0, l, yb + 10.0]);
        }
        if !d.alt_valid {
            return;
        }
        let vs = d.vs_fpm;
        let y = Self::vy(vs);
        let txt = if vs.abs() >= 100.0 { (roundi(vs / 50.0) * 50).to_string() } else { String::new() };
        c.poly(BLACK, WHITE, 2.5, &[l + 2.0, y, l + 20.0, y - 22.0, 992.0, y - 22.0, 992.0, y + 22.0, l + 20.0, y + 22.0]);
        c.text_r(&txt, 986.0, y, 30.0, WHITE);
    }

    fn hsi(&self, c: &mut Canvas, d: &FlightData) {
        c.circle(HX, HY, HR + 26.0, PANEL_DARK, 0.0);
        c.rect(HX - 58.0, HY - HR - 64.0, HX + 58.0, HY - HR - 20.0, BLACK);
        c.frame(HX - 58.0, HY - HR - 64.0, HX + 58.0, HY - HR - 20.0, GREY, 2.0);
        if !d.att_valid {
            c.text("HDG", HX, HY - HR - 42.0, 30.0, RED);
            return;
        }
        let h = d.heading;
        c.text(&format!("{:03}°", imod(roundi(h), 360)), HX, HY - HR - 42.0, 32.0, WHITE);
        c.poly(WHITE, BLACK, 2.0, &[HX, HY - HR - 2.0, HX - 12.0, HY - HR - 24.0, HX + 12.0, HY - HR - 24.0]);
        c.save();
        c.rotate(-h, HX, HY);
        let mut deg = 0;
        while deg < 360 {
            let ten = deg % 10 == 0;
            c.radial(HX, HY, deg as f32, HR - if ten { 22.0 } else { 12.0 }, HR, WHITE, if ten { 3.0 } else { 2.0 });
            if deg % 30 == 0 {
                let rad = (deg as f32).to_radians();
                let rr = HR - 44.0;
                let (tx, ty) = (HX + rr * rad.sin(), HY - rr * rad.cos());
                let lbl = match deg {
                    0 => "N".to_string(),
                    90 => "E".to_string(),
                    180 => "S".to_string(),
                    270 => "W".to_string(),
                    _ => (deg / 10).to_string(),
                };
                c.save();
                c.rotate(deg as f32, tx, ty);
                c.text(&lbl, tx, ty, 30.0, WHITE);
                c.restore();
            }
            deg += 5;
        }
        if !d.hdg_bug.is_nan() {
            c.save();
            c.rotate(d.hdg_bug, HX, HY);
            c.poly(CYAN, BLACK, 1.5, &[HX - 12.0, HY - HR - 2.0, HX - 12.0, HY - HR + 12.0, HX + 12.0, HY - HR + 12.0, HX + 12.0, HY - HR - 2.0, HX + 4.0, HY - HR - 2.0, HX, HY - HR + 6.0, HX - 4.0, HY - HR - 2.0]);
            c.restore();
        }
        c.restore();
        for (brg, double) in [(d.brg2, true), (d.brg_adf, false)] {
            if brg.is_nan() {
                continue;
            }
            c.save();
            c.rotate(ang_diff(brg, h), HX, HY);
            if double {
                c.line(HX - 4.0, HY - HR + 24.0, HX - 4.0, HY + HR - 24.0, CYAN, 2.0);
                c.line(HX + 4.0, HY - HR + 24.0, HX + 4.0, HY + HR - 24.0, CYAN, 2.0);
            } else {
                c.line(HX, HY - HR + 24.0, HX, HY + HR - 24.0, CYAN, 2.5);
            }
            c.poly(0, CYAN, 2.5, &[HX, HY - HR + 14.0, HX - 10.0, HY - HR + 30.0, HX + 10.0, HY - HR + 30.0]);
            c.restore();
        }
        let n = &d.nav;
        let col = if n.source == NavSource::Gps { MAGENTA } else { GREEN };
        let course = if !n.course.is_nan() {
            n.course
        } else if d.has_track {
            d.track
        } else {
            f32::NAN
        };
        if !course.is_nan() {
            c.save();
            c.rotate(ang_diff(course, h), HX, HY);
            c.poly(col, BLACK, 2.0, &[HX, HY - HR + 8.0, HX - 15.0, HY - HR + 38.0, HX + 15.0, HY - HR + 38.0]);
            c.line(HX, HY - HR + 26.0, HX, HY - 44.0, col, 5.0);
            c.line(HX, HY + 44.0, HX, HY + HR - 16.0, col, 5.0);
            for i in [-2.0, -1.0, 1.0, 2.0] {
                c.circle(HX + i * 26.0, HY, 5.0, WHITE, 2.0);
            }
            if !n.cdi.is_nan() {
                let x = HX + n.cdi.clamp(-2.5, 2.5) * 26.0;
                c.line(x, HY - 40.0, x, HY + 40.0, col, 6.0);
                if n.to_from != 0 {
                    let y = if n.to_from == 1 { HY - 56.0 } else { HY + 56.0 };
                    let dir = if n.to_from == 1 { -1.0 } else { 1.0 };
                    c.poly(WHITE, BLACK, 1.5, &[HX + 40.0, y + dir * 10.0, HX + 32.0, y - dir * 4.0, HX + 48.0, y - dir * 4.0]);
                }
            }
            c.restore();
        }
        if d.has_track {
            c.save();
            c.rotate(ang_diff(d.track, h), HX, HY);
            c.poly(MAGENTA, BLACK, 1.5, &[HX, HY - HR + 2.0, HX - 6.0, HY - HR + 10.0, HX, HY - HR + 18.0, HX + 6.0, HY - HR + 10.0]);
            c.restore();
        }
        let src = match n.source {
            NavSource::Gps => Some("GPS"),
            NavSource::Vor => Some("VOR1"),
            NavSource::Loc => Some("LOC1"),
            NavSource::None => None,
        };
        if let Some(s) = src {
            c.text(s, HX - 52.0, HY - 48.0, 22.0, col);
        }
        if n.source == NavSource::Gps {
            c.text(if d.gps.approach { "APR" } else { "ENR" }, HX + 52.0, HY - 48.0, 22.0, WHITE);
        } else if !n.dme_nm.is_nan() {
            c.text(&format!("{:.1}", n.dme_nm), HX + 52.0, HY - 48.0, 22.0, col);
        }
        if !n.course.is_nan() {
            c.text(&format!("CRS {:03}°", imod(roundi(n.course), 360)), HX + 182.0, HY - HR - 54.0, 24.0, col);
        }
        if !d.hdg_bug.is_nan() {
            c.text(&format!("HDG {:03}°", imod(roundi(d.hdg_bug), 360)), HX - 182.0, HY - HR - 54.0, 24.0, CYAN);
        }
        c.poly(WHITE, BLACK, 2.0, &[HX, HY - 20.0, HX - 4.0, HY - 7.0, HX - 25.0, HY + 3.0, HX - 25.0, HY + 10.0, HX - 4.0, HY + 5.0, HX - 4.0, HY + 18.0, HX - 11.0, HY + 23.0, HX - 11.0, HY + 27.0, HX + 11.0, HY + 27.0, HX + 11.0, HY + 23.0, HX + 4.0, HY + 18.0, HX + 4.0, HY + 5.0, HX + 25.0, HY + 10.0, HX + 25.0, HY + 3.0, HX + 4.0, HY - 7.0]);
    }

    fn top_bar(&self, c: &mut Canvas, d: &FlightData) {
        c.rect(0.0, 0.0, 1000.0, 70.0, PANEL_DARK);
        c.line(0.0, 70.0, 1000.0, 70.0, GREY, 2.0);
        for x in [220.0, 780.0] {
            c.line(x, 2.0, x, 68.0, GREY, 2.0);
        }
        c.line(220.0, 35.0, 780.0, 35.0, GREY, 1.5);
        let freq = |v: f32| if v.is_nan() { "---.--".to_string() } else { format!("{v:.2}") };
        let freq_c = |v: f32| if v.is_nan() { "---.---".to_string() } else { format!("{v:.3}") };
        c.text_l("NAV1", 14.0, 18.0, 20.0, WHITE);
        c.text_l(&freq(d.nav1_stby), 80.0, 18.0, 24.0, WHITE);
        c.text_l(&freq(d.nav1), 80.0, 50.0, 24.0, GREEN);
        c.text_r("COM1", 986.0, 18.0, 20.0, WHITE);
        c.text_r(&freq_c(d.com1), 920.0, 18.0, 24.0, GREEN);
        c.text_r(&freq_c(d.com1_stby), 920.0, 50.0, 24.0, WHITE);
        let f = &d.fma;
        let lat = match f.lateral {
            Some(Lateral::Hdg) => Some("HDG"),
            Some(Lateral::NavGps) => Some("GPS"),
            Some(Lateral::NavVor) => Some("VOR"),
            Some(Lateral::Loc) => Some("LOC"),
            Some(Lateral::Bc) => Some("BC"),
            Some(Lateral::Rol) => Some("ROL"),
            None => None,
        };
        if let Some(l) = lat {
            c.text(l, 270.0, 18.0, 24.0, GREEN);
        }
        if f.lateral_armed == Some(Lateral::Loc) {
            c.text("LOC", 350.0, 18.0, 24.0, WHITE);
        }
        if f.ap {
            c.text("AP", 470.0, 18.0, 24.0, GREEN);
        }
        if f.yd {
            c.text("YD", 530.0, 18.0, 24.0, GREEN);
        }
        let vert = match f.vertical {
            Some(Vertical::Alt) => Some("ALT"),
            Some(Vertical::Vs) => Some("VS"),
            Some(Vertical::Flc) => Some("FLC"),
            Some(Vertical::Gs) => Some("GS"),
            Some(Vertical::Pit) => Some("PIT"),
            _ => None,
        };
        if let Some(v) = vert {
            c.text(v, 600.0, 18.0, 24.0, GREEN);
        }
        let target = match f.vertical {
            Some(Vertical::Vs) if !d.vs_bug.is_nan() => Some(format!("{}FPM", roundi(d.vs_bug / 50.0) * 50)),
            Some(Vertical::Flc) => sl::sel_speed(d).map(|s| s + "KT"),
            Some(Vertical::Alt) if !d.alt_bug.is_nan() => Some(format!("{}FT", roundi(d.alt_bug))),
            _ => None,
        };
        if let Some(t) = target {
            c.text(&t, 680.0, 18.0, 22.0, GREEN);
        }
        let armed: Vec<&str> = f.vertical_armed.iter().map(|v| if *v == Vertical::Gs { "GS" } else { "ALTS" }).collect();
        if !armed.is_empty() {
            c.text_r(&armed.join(" "), 750.0, 18.0, 22.0, WHITE);
        }
        let g = &d.gps;
        if let Some(id) = &g.next_id {
            c.text_l(&format!("→ {id}"), 232.0, 52.0, 22.0, MAGENTA);
            if !g.next_dist_nm.is_nan() {
                c.text_l(&format!("DIS {:.1}NM", g.next_dist_nm), 400.0, 52.0, 22.0, WHITE);
            }
            if !g.dtk.is_nan() {
                c.text_l(&format!("DTK {:03}°", imod(roundi(g.dtk), 360)), 548.0, 52.0, 22.0, WHITE);
            }
        }
        if d.has_track {
            c.text_r(&format!("TRK {:03}°", imod(roundi(d.track), 360)), 768.0, 52.0, 22.0, MAGENTA);
        }
    }

    fn bottom_bar(&self, c: &mut Canvas, d: &FlightData) {
        c.rect(0.0, 706.0, 1000.0, 750.0, PANEL_DARK);
        c.line(0.0, 706.0, 1000.0, 706.0, GREY, 2.0);
        c.text_l("XPDR", 24.0, 728.0, 24.0, WHITE);
        c.text_l(d.squawk.as_deref().unwrap_or("----"), 98.0, 728.0, 26.0, GREEN);
        let mode = match d.xpdr_mode.as_deref() {
            Some("alt") => "ALT",
            Some("on") => "ON",
            Some("standby") => "STBY",
            Some("ground") => "GND",
            Some("test") => "TST",
            _ => "OFF",
        };
        c.text_l(mode, 176.0, 728.0, 24.0, if mode == "ALT" || mode == "ON" { GREEN } else { WHITE });
        if !d.oat_c.is_nan() {
            c.text_l("OAT", 420.0, 728.0, 24.0, WHITE);
            c.text_l(&format!("{}°C", roundi(d.oat_c)), 480.0, 728.0, 26.0, WHITE);
        }
        if let Some(t) = &d.sim_time {
            let t = t.trim_end_matches('Z');
            c.text_l("UTC", 816.0, 728.0, 24.0, WHITE);
            c.text_r(&format!("{}:{}", &t[..2.min(t.len())], t.get(2..).unwrap_or("")), 976.0, 728.0, 26.0, WHITE);
        } else {
            c.text_l("LCL", 816.0, 728.0, 24.0, WHITE);
            c.text_r(&chrono::Local::now().format("%H:%M:%S").to_string(), 976.0, 728.0, 26.0, WHITE);
        }
    }

    fn wind(&self, c: &mut Canvas, d: &FlightData) {
        c.rect(20.0, 566.0, 210.0, 638.0, PANEL_DARK);
        c.frame(20.0, 566.0, 210.0, 638.0, GREY, 2.0);
        if !d.wind_valid {
            c.text("NO WIND", 115.0, 590.0, 24.0, WHITE);
            c.text("DATA", 115.0, 616.0, 24.0, WHITE);
            return;
        }
        c.save();
        c.rotate(d.wind_dir - d.heading + 180.0, 60.0, 602.0);
        c.line(60.0, 624.0, 60.0, 580.0, WHITE, 4.0);
        c.poly(WHITE, 0, 0.0, &[60.0, 627.0, 51.0, 610.0, 69.0, 610.0]);
        c.restore();
        c.text(&format!("{:03}°", imod(roundi(d.wind_dir), 360)), 148.0, 590.0, 24.0, WHITE);
        c.text(&format!("{}KT", roundi(d.wind_kt)), 148.0, 616.0, 24.0, WHITE);
    }

    fn baro(&self, c: &mut Canvas, d: &FlightData) {
        let (l, t, r, b) = (700.0, 528.0, 862.0, 582.0);
        c.rect(l, t, r, b, PANEL_DARK);
        let y = (t + b) / 2.0;
        if !d.minimums_ft.is_nan() {
            c.text_l("MIN", 872.0, y - 12.0, 20.0, CYAN);
            c.text_l(&format!("{}FT", roundi(d.minimums_ft)), 872.0, y + 14.0, 24.0, CYAN);
        }
        if d.std {
            c.text("STD BARO", (l + r) / 2.0, y, 28.0, CYAN);
        } else {
            let txt = if d.in_hg { format!("{:.2}IN", d.qnh * 0.02953) } else { format!("{:.0}HPA", d.qnh) };
            c.text(&txt, (l + r) / 2.0, y, 30.0, CYAN);
        }
    }
}

impl Pfd for GarminPfd {
    fn vh(&self) -> f32 {
        750.0
    }
    fn draw(&mut self, c: &mut Canvas, d: &FlightData) {
        self.attitude(c, d);
        self.speed(c, d);
        self.altitude(c, d);
        self.vsi(c, d);
        self.hsi(c, d);
        self.top_bar(c, d);
        self.baro(c, d);
        self.wind(c, d);
        self.bottom_bar(c, d);
    }
}
