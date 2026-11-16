//! PFD ATR 72-600 (Thales) – przeniesiony z AtrPfd.kt: prostokątny horyzont, wąskie taśmy,
//! pełna róża kursowa (HSI) w dolnej połowie.

use super::data::*;
use super::gfx::*;
use super::simlayer as sl;
use super::Pfd;

const ADI_L: f32 = 160.0;
const ADI_R: f32 = 600.0;
const ADI_T: f32 = 100.0;
const ADI_B: f32 = 573.0;
const CX: f32 = 380.0;
const CY: f32 = 340.0;
const PPD: f32 = 10.0;
const BALL_R: f32 = 222.0;
const RX: f32 = 380.0;
const RY: f32 = 790.0;
const RR: f32 = 150.0;
const SKY: Argb = 0xFF2A4FA8;
const GROUND: Argb = 0xFF6E4A1E;
const BALL: Argb = 0x22FFFFFF;
const GREEN: Argb = 0xFF27E327;
const CYAN: Argb = 0xFF2BD9F0;
const MAGENTA: Argb = 0xFFE54BD2;
const TAPE: Argb = 0xFF2A2A2A;
const GREY: Argb = 0xFF8C8C8C;

pub struct AtrPfd;

impl AtrPfd {
    fn fma(&self, c: &mut Canvas, d: &FlightData) {
        for x in [190.0, 340.0, 468.0, 536.0] {
            c.line(x, 6.0, x, 56.0, GREY, 2.5);
        }
        let (y, y2, sz) = (22.0, 48.0, 28.0);
        let f = &d.fma;
        let lat = match f.lateral {
            Some(Lateral::Hdg) => Some("HDG SEL"),
            Some(Lateral::NavGps) => Some("LNAV"),
            Some(Lateral::NavVor) => Some("VOR"),
            Some(Lateral::Loc) => Some("LOC*"),
            Some(Lateral::Bc) => Some("BC"),
            Some(Lateral::Rol) => Some("HDG HLD"),
            None => None,
        };
        if let Some(l) = lat {
            c.text(l, 264.0, y, sz, GREEN);
        }
        if f.lateral_armed == Some(Lateral::Loc) {
            c.text("LOC", 264.0, y2, 24.0, CYAN);
        }
        let vert = match f.vertical {
            Some(Vertical::Alt) => Some("ALT"),
            Some(Vertical::Vs) => Some("VS"),
            Some(Vertical::Flc) => Some("IAS"),
            Some(Vertical::Gs) => Some("GS"),
            Some(Vertical::Pit) => Some("PITCH HLD"),
            _ => None,
        };
        if let Some(v) = vert {
            c.text(v, 404.0, y, sz, GREEN);
        }
        let armed: Vec<&str> = f.vertical_armed.iter().map(|v| if *v == Vertical::Gs { "GS" } else { "ALT*" }).collect();
        if !armed.is_empty() {
            c.text(&armed.join(" "), 404.0, y2, 24.0, CYAN);
        }
        let target = match f.vertical {
            Some(Vertical::Vs) if !d.vs_bug.is_nan() => Some((roundi(d.vs_bug / 100.0) * 100).to_string()),
            Some(Vertical::Flc) if !d.spd_bug.is_nan() => Some(roundi(d.spd_bug).to_string()),
            _ => None,
        };
        if let Some(t) = target {
            c.text(&t, 502.0, y, 26.0, CYAN);
        }
        if f.ap {
            c.text("AP", 562.0, y, sz, GREEN);
        }
        c.text("|", 583.0, y, sz, GREY);
        if f.fd {
            c.text("FD", 604.0, y, sz, GREEN);
        }
        if f.yd {
            c.text("YD", 650.0, y, 24.0, GREEN);
        }
    }

    fn attitude(&self, c: &mut Canvas, d: &FlightData) {
        if !d.att_valid {
            c.rect(ADI_L, ADI_T, ADI_R, ADI_B, BLACK);
            c.text("ATT", CX, CY, 56.0, RED);
            aircraft(c);
            return;
        }
        c.save();
        c.clip_rect(ADI_L, ADI_T, ADI_R, ADI_B);
        c.save();
        c.rotate(-d.roll, CX, CY);
        c.translate(0.0, d.pitch * PPD);
        c.rect(CX - 1200.0, CY - 3000.0, CX + 1200.0, CY, SKY);
        c.rect(CX - 1200.0, CY, CX + 1200.0, CY + 3000.0, GROUND);
        c.line(CX - 1200.0, CY, CX + 1200.0, CY, WHITE, 3.0);
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
                62.0
            } else if i % 2 == 0 {
                34.0
            } else {
                16.0
            };
            c.line(CX - half, y, CX + half, y, WHITE, 3.0);
            if i % 4 == 0 {
                let lbl = roundi(deg.abs()).to_string();
                c.text(&lbl, CX - half - 30.0, y, 32.0, WHITE);
                c.text(&lbl, CX + half + 30.0, y, 32.0, WHITE);
            }
        }
        c.restore();
        if d.fpv && d.has_track && d.speed_kt > 5.0 {
            c.save();
            c.rotate(-d.roll, CX, CY);
            let dx = ang_diff(d.track, d.heading).clamp(-18.0, 18.0) * PPD;
            let dy = (d.fpa - d.pitch).clamp(-18.0, 18.0) * PPD;
            sl::bird(c, CX + dx, CY - dy, GREEN, 13.0, 40.0, 28.0, 4.0);
            c.restore();
        }
        c.restore();
        c.save();
        c.clip_circle(CX, CY, BALL_R);
        c.clip_rect(ADI_L, ADI_T, ADI_R, ADI_B);
        c.rect(ADI_L, ADI_T, ADI_R, ADI_B, BALL);
        c.restore();
        if d.pitch > 30.0 || d.pitch < -20.0 {
            let dir = if d.pitch > 0.0 { 1.0 } else { -1.0 };
            c.save();
            c.clip_rect(ADI_L, ADI_T, ADI_R, ADI_B);
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
        c.clip_rect(ADI_L, ADI_T, ADI_R, ADI_B);
        for a in [10, 20, 30, 45, 60] {
            for s in [-1, 1] {
                let len = if a == 30 || a == 60 { 22.0 } else { 14.0 };
                c.radial(CX, CY, (s * a) as f32, BALL_R - 24.0, BALL_R - 24.0 + len, WHITE, 3.0);
            }
        }
        c.poly(0, WHITE, 3.0, &[CX, CY - BALL_R + 24.0, CX - 15.0, CY - BALL_R + 2.0, CX + 15.0, CY - BALL_R + 2.0]);
        c.save();
        c.rotate(-d.roll, CX, CY);
        let col = if d.roll.abs() > 35.0 { AMBER } else { YELLOW };
        let top = CY - BALL_R + 28.0;
        c.poly(col, col, 2.0, &[CX, top, CX - 15.0, top + 24.0, CX + 15.0, top + 24.0]);
        let off = (d.slip * 170.0).clamp(-40.0, 40.0);
        c.poly(0, if d.slip.abs() > 0.2 { AMBER } else { col }, 3.0, &[CX - 16.0 + off, top + 29.0, CX + 16.0 + off, top + 29.0, CX + 21.0 + off, top + 41.0, CX - 21.0 + off, top + 41.0]);
        c.restore();
        c.restore();
        sl::fd_cross_bars(c, d, CX, CY, PPD, 100.0, GREEN, 5.0);
        aircraft(c);
        sl::deviation_std(c, d, CX, ADI_B - 14.0, ADI_R - 24.0, CY, 34.0, MAGENTA);
        sl::marker(c, d, ADI_R - 50.0, ADI_T + 30.0, 26.0);
        if d.stall_warn {
            c.text_b("STALL", CX, CY - 110.0, 46.0, RED);
        }
        if !d.minimums_ft.is_nan() {
            c.text_r(&format!("DH {}", roundi(d.minimums_ft)), ADI_R - 10.0, ADI_B - 50.0, 26.0, CYAN);
        }
        if d.ra_valid && d.ra_ft < 2500.0 {
            let ra = if d.ra_ft < 50.0 { roundi(d.ra_ft) } else { roundi(d.ra_ft / 10.0) * 10 };
            let col = if d.ra_ft < 200.0 { AMBER } else { GREEN };
            c.rect(CX - 42.0, ADI_B - 74.0, CX + 42.0, ADI_B - 24.0, BLACK);
            c.frame(CX - 42.0, ADI_B - 74.0, CX + 42.0, ADI_B - 24.0, col, 2.5);
            c.text(&ra.to_string(), CX, ADI_B - 49.0, 36.0, col);
        }
    }

    fn speed(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r, t, b, k) = (58.0, 140.0, 120.0, 541.0, 4.3);
        if !d.speed_valid {
            c.text("SPD", (l + r) / 2.0, CY, 44.0, RED);
            return;
        }
        let s = d.speed_kt;
        c.rect(l, t, r, b, TAPE);
        c.save();
        c.clip_rect(l, t, r, b);
        let mut v = floor_to(s - 70.0, 10).max(0);
        while v as f32 <= s + 70.0 {
            let y = CY - (v as f32 - s) * k;
            if v % 20 == 0 {
                c.line(l + 4.0, y, l + 24.0, y, WHITE, 3.0);
                c.text_r(&v.to_string(), r - 8.0, y, 36.0, WHITE);
            } else {
                c.line(l + 4.0, y, l + 16.0, y, WHITE, 3.0);
            }
            v += 10;
        }
        let vmo = sl::vmo(d, 250.0);
        if s + 70.0 > vmo {
            let mut y = CY - (vmo - s) * k;
            let mut red = true;
            while y > t {
                c.rect(r - 14.0, y - 12.0, r, y, if red { RED } else { WHITE });
                y -= 12.0;
                red = !red;
            }
        }
        sl::low_speed(c, d, r - 12.0, 12.0, |v| CY - (v - s) * k, b);
        if !d.spd_bug.is_nan() {
            sl::speed_bug(c, r - 2.0, (CY - (d.spd_bug - s) * k).clamp(t, b), CYAN, false);
        }
        c.restore();
        if let Some(t2) = sl::sel_speed(d) {
            c.text(&t2, (l + r) / 2.0, t - 18.0, 32.0, CYAN);
        }
        c.frame(l, t, r, b, GREY, 2.5);
        sl::trend_arrow(c, r + 34.0, CY, d.speed_trend.clamp(-60.0, 60.0), k, YELLOW, 4.0, 15.0);
        c.poly(BLACK, YELLOW, 3.5, &[l - 4.0, CY - 26.0, r + 10.0, CY - 26.0, r + 30.0, CY, r + 10.0, CY + 26.0, l - 4.0, CY + 26.0]);
        c.text_r(&roundi(s).to_string(), r + 4.0, CY, 42.0, GREEN);
    }

    fn altitude(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r, t, b, k) = (608.0, 684.0, 120.0, 541.0, 0.43);
        if !d.alt_valid {
            c.text("ALT", (l + r) / 2.0, CY, 44.0, RED);
            return;
        }
        let a = d.alt_ft;
        c.rect(l, t, r, b, TAPE);
        c.save();
        c.clip_rect(l, t, r, b);
        let mut v = floor_to(a - 800.0, 100);
        while v as f32 <= a + 800.0 {
            let y = CY - (v as f32 - a) * k;
            if imod(v, 500) == 0 {
                c.line(l + 2.0, y, l + 20.0, y, WHITE, 3.0);
                c.text_l(&format!("{}{:03}", if v < 0 { "-" } else { "" }, v.abs() / 100), l + 26.0, y, 34.0, WHITE);
            } else {
                c.line(l + 2.0, y, l + 14.0, y, WHITE, 3.0);
            }
            v += 100;
        }
        if !d.terrain_ft.is_nan() && !d.std {
            let yt = CY - (d.terrain_ft - a) * k;
            if yt < b {
                c.rect(l, yt.max(t), l + 12.0, b, RED);
                c.line(l, yt, r, yt, RED, 3.0);
            }
        }
        if !d.alt_bug.is_nan() {
            sl::alt_bug(c, l, (CY - (d.alt_bug - a) * k).clamp(t, b), CYAN);
        }
        c.restore();
        c.frame(l, t, r, b, GREY, 2.5);
        if !d.alt_bug.is_nan() {
            c.text(&roundi(d.alt_bug).to_string(), (l + r) / 2.0, t - 18.0, 32.0, CYAN);
        }
        c.poly(BLACK, WHITE, 3.0, &[l - 6.0, CY - 30.0, 744.0, CY - 30.0, 744.0, CY + 30.0, l - 6.0, CY + 30.0]);
        let hundreds = (a / 100.0).floor() as i32;
        c.text_r(&if hundreds < 0 { format!("-{}", hundreds.abs()) } else { hundreds.to_string() }, 700.0, CY, 44.0, GREEN);
        c.drum(702.0, CY - 28.0, 742.0, CY + 28.0, a / 20.0, 26.0, 26.0, GREEN, 722.0, Align::Center, |kk| format!("{:02}", imod(kk * 20, 100)));
    }

    fn vy(v: f32) -> f32 {
        let av = v.abs().min(6000.0);
        let g = if av <= 1000.0 {
            av * 0.125
        } else if av <= 2000.0 {
            125.0 + (av - 1000.0) * 0.055
        } else {
            180.0 + (av - 2000.0) * 0.0125
        };
        CY - sign(v) * g
    }

    fn vsi(&self, c: &mut Canvas, d: &FlightData) {
        let (l, r) = (688.0, 744.0);
        c.rect(l, 120.0, r, 541.0, TAPE);
        c.frame(l, 120.0, r, 541.0, GREY, 2.5);
        for v in [500, 1000, 2000, 6000] {
            for sg in [-1, 1] {
                let y = Self::vy((sg * v) as f32);
                c.line(l + 2.0, y, l + 2.0 + if v == 500 { 10.0 } else { 18.0 }, y, WHITE, 3.0);
                if v != 500 {
                    c.text(&(v / 1000).to_string(), l + 38.0, y, 24.0, WHITE);
                }
            }
        }
        if !d.alt_valid {
            c.text("V/S", (l + r) / 2.0, CY - 70.0, 28.0, RED);
            return;
        }
        let vs = d.vs_fpm;
        let col = if vs.abs() > 2000.0 { AMBER } else { GREEN };
        c.save();
        c.clip_rect(l, 122.0, r, 539.0);
        c.line(r + 30.0, CY, l + 4.0, Self::vy(vs), col, 5.0);
        c.restore();
    }

    fn heading_line(&self, c: &mut Canvas, d: &FlightData) {
        let y = 596.0;
        if d.att_valid {
            c.text_l("HDG", 162.0, y, 30.0, WHITE);
            let h = imod(roundi(if d.hdg_bug.is_nan() { d.heading } else { d.hdg_bug }), 360);
            c.text_l(&format!("{h:03}°"), 232.0, y, 32.0, CYAN);
        } else {
            c.text_l("HDG", 162.0, y, 30.0, RED);
        }
        if d.std {
            c.text_r("STD", 600.0, y, 32.0, CYAN);
        } else {
            c.text_r(&roundi(d.qnh).to_string(), 584.0, y, 30.0, CYAN);
            c.text_l("/", 586.0, y, 30.0, WHITE);
            c.text_l(&format!("{:.2}", d.qnh * 0.02953), 600.0, y, 30.0, CYAN);
        }
    }

    fn rose(&self, c: &mut Canvas, d: &FlightData) {
        if !d.att_valid {
            c.circle(RX, RY, RR, GREY, 3.0);
            c.text("HDG", RX, RY, 44.0, RED);
            return;
        }
        let h = d.heading;
        c.poly(WHITE, WHITE, 2.0, &[RX, RY - RR - 4.0, RX - 12.0, RY - RR - 26.0, RX + 12.0, RY - RR - 26.0]);
        c.save();
        c.rotate(-h, RX, RY);
        let mut deg = 0;
        while deg < 360 {
            let ten = deg % 10 == 0;
            c.radial(RX, RY, deg as f32, RR - if ten { 26.0 } else { 14.0 }, RR, WHITE, if ten { 3.5 } else { 2.5 });
            if deg % 30 == 0 {
                let rad = (deg as f32).to_radians();
                let rr = RR - 48.0;
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
                c.text(&lbl, tx, ty, 30.0, WHITE);
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
        if d.has_track {
            c.save();
            c.rotate(d.track, RX, RY);
            c.line(RX, RY - RR, RX, RY + RR, GREEN, 3.0);
            c.circle(RX, RY - RR + 34.0, 12.0, GREEN, 3.0);
            c.restore();
        }
        c.restore();
        let n = &d.nav;
        if !n.course.is_nan() {
            let col = if n.source == NavSource::Gps { MAGENTA } else { GREEN };
            c.save();
            c.rotate(ang_diff(n.course, h), RX, RY);
            c.line(RX, RY - RR + 30.0, RX, RY - 60.0, col, 4.0);
            c.poly(col, col, 2.0, &[RX, RY - RR + 22.0, RX - 10.0, RY - RR + 42.0, RX + 10.0, RY - RR + 42.0]);
            c.line(RX, RY + 60.0, RX, RY + RR - 30.0, col, 4.0);
            if !n.cdi.is_nan() {
                let dx = n.cdi.clamp(-2.5, 2.5) * 46.0;
                c.line(RX + dx, RY - 56.0, RX + dx, RY + 56.0, col, 5.0);
            }
            c.restore();
        }
        for i in [-2.0, -1.0, 1.0, 2.0] {
            c.circle(RX + i * 46.0, RY, 6.0, WHITE, 2.5);
        }
        c.line(RX - 60.0, RY, RX + 60.0, RY, YELLOW, 4.0);
        c.line(RX, RY - 24.0, RX, RY + 28.0, YELLOW, 4.0);
        c.line(RX - 20.0, RY + 22.0, RX + 20.0, RY + 22.0, YELLOW, 4.0);
        for s in [-1.0, 1.0] {
            c.line(RX + s * (RR + 6.0), RY, RX + s * (RR + 30.0), RY, WHITE, 3.0);
        }
    }

    fn side_info(&self, c: &mut Canvas, d: &FlightData) {
        let xr = 740.0;
        if let Some(s) = &d.squawk {
            c.text_r(&format!("XPDR {s}"), xr, 566.0, 26.0, GREEN);
        }
        if let Some(m) = &d.xpdr_mode {
            c.text_r(&m.to_uppercase(), xr, 594.0, 26.0, if m == "alt" || m == "on" { GREEN } else { AMBER });
        }
        let n = &d.nav;
        let src = match n.source {
            NavSource::Gps => Some("FMS1"),
            NavSource::Loc => Some("LOC1"),
            NavSource::Vor => Some("VOR1"),
            NavSource::None => None,
        };
        let nav_col = if n.source == NavSource::Gps { MAGENTA } else { GREEN };
        if let Some(s) = src {
            c.text_l(s, 566.0, 680.0, 26.0, nav_col);
            c.line(566.0, 696.0, 700.0, 696.0, nav_col, 2.0);
            if let Some(i) = &n.ident {
                c.text_l(i, 566.0, 716.0, 26.0, nav_col);
            }
            if !n.course.is_nan() {
                c.text_l(&format!("CRS {:03}°", imod(roundi(n.course), 360)), 566.0, 742.0, 24.0, WHITE);
            }
            if !n.dme_nm.is_nan() {
                c.text_l(&format!("{:.1} NM", n.dme_nm), 566.0, 768.0, 24.0, nav_col);
            }
        }
        c.text_l(&if d.gs_kt.is_nan() { "GS ---".into() } else { format!("GS {}", roundi(d.gs_kt)) }, 566.0, 800.0, 26.0, GREEN);
        if !d.tas_kt.is_nan() {
            c.text_l(&format!("TAS {}", roundi(d.tas_kt)), 566.0, 828.0, 26.0, GREEN);
        }
        if !d.oat_c.is_nan() {
            c.text_l(&format!("SAT {}°C", roundi(d.oat_c)), 566.0, 856.0, 26.0, GREEN);
        }
        c.text_l("TRK", 24.0, 700.0, 26.0, WHITE);
        c.text_l(&if d.has_track { format!("{:03}°", imod(roundi(d.track), 360)) } else { "---°".into() }, 90.0, 700.0, 26.0, GREEN);
        if d.wind_valid && d.wind_kt >= 1.0 {
            c.text_l(&format!("{:03}°/{}", imod(roundi(d.wind_dir), 360), roundi(d.wind_kt)), 24.0, 732.0, 26.0, WHITE);
        }
        if let Some(id) = &d.gps.next_id {
            c.frame(18.0, 928.0, 236.0, 990.0, GREY, 2.5);
            c.text_l(id, 32.0, 948.0, 24.0, MAGENTA);
            let dist = if d.gps.next_dist_nm.is_nan() { String::new() } else { format!("{:.1} NM", d.gps.next_dist_nm) };
            let ete = if d.gps.next_ete_sec.is_nan() { String::new() } else { format!("  {}:{:02}", (d.gps.next_ete_sec / 60.0) as i32, (d.gps.next_ete_sec % 60.0) as i32) };
            c.text_l(&(dist + &ete), 32.0, 974.0, 24.0, GREEN);
        }
    }
}

fn aircraft(c: &mut Canvas) {
    c.poly(BLACK, YELLOW, 4.0, &[CX - 218.0, CY - 7.0, CX - 118.0, CY - 7.0, CX - 118.0, CY + 32.0, CX - 104.0, CY + 32.0, CX - 104.0, CY + 7.0, CX - 218.0, CY + 7.0]);
    c.poly(BLACK, YELLOW, 4.0, &[CX + 218.0, CY - 7.0, CX + 118.0, CY - 7.0, CX + 118.0, CY + 32.0, CX + 104.0, CY + 32.0, CX + 104.0, CY + 7.0, CX + 218.0, CY + 7.0]);
    c.rect(CX - 8.0, CY - 8.0, CX + 8.0, CY + 8.0, BLACK);
    c.frame(CX - 8.0, CY - 8.0, CX + 8.0, CY + 8.0, YELLOW, 4.0);
}

impl Pfd for AtrPfd {
    fn vw(&self) -> f32 {
        750.0
    }
    fn draw(&mut self, c: &mut Canvas, d: &FlightData) {
        self.fma(c, d);
        self.attitude(c, d);
        self.speed(c, d);
        self.altitude(c, d);
        self.vsi(c, d);
        self.heading_line(c, d);
        self.rose(c, d);
        self.side_info(c, d);
    }
}
