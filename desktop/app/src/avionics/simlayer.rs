//! Elementy PFD zasilane z symulatora, wspólne dla wszystkich stylów (SimLayer.kt): dyrektor lotu,
//! skale odchylenia LOC/GS, bugi nastaw autopilota, markery.

use super::data::{FlightData, NavSource};
use super::gfx::*;

/// Dyrektor lotu jako dwie belki (Airbus, Boeing, Embraer, ATR).
pub fn fd_cross_bars(c: &mut Canvas, d: &FlightData, cx: f32, cy: f32, ppd: f32, half: f32, color: Argb, w: f32) {
    if !d.fd_on || d.fd_pitch.is_nan() || d.fd_roll.is_nan() || d.on_ground && d.speed_kt < 40.0 {
        return;
    }
    let dy = (d.fd_pitch - d.pitch).clamp(-10.0, 10.0) * ppd;
    let dx = ((d.fd_roll - d.roll).clamp(-30.0, 30.0) / 3.0) * ppd;
    c.line(cx - half, cy - dy, cx + half, cy - dy, color, w);
    c.line(cx + dx, cy - half, cx + dx, cy + half, color, w);
}

/// Dyrektor lotu jako „V” (Garmin, Collins).
pub fn fd_vbar(c: &mut Canvas, d: &FlightData, cx: f32, cy: f32, ppd: f32, span: f32, color: Argb) {
    if !d.fd_on || d.fd_pitch.is_nan() || d.fd_roll.is_nan() || d.on_ground && d.speed_kt < 40.0 {
        return;
    }
    let dy = (d.fd_pitch - d.pitch).clamp(-10.0, 10.0) * ppd;
    c.save();
    c.rotate(d.fd_roll - d.roll, cx, cy);
    let y = cy - dy;
    c.poly(color, BLACK, 2.0, &[cx, y, cx - span, y + span * 0.32, cx - span, y + span * 0.42, cx, y + span * 0.12]);
    c.poly(color, BLACK, 2.0, &[cx, y, cx + span, y + span * 0.32, cx + span, y + span * 0.42, cx, y + span * 0.12]);
    c.restore();
}

/// Skale odchylenia: boczna pod horyzontem i pionowa z prawej.
#[allow(clippy::too_many_arguments)]
pub fn deviation(c: &mut Canvas, d: &FlightData, cx: f32, loc_y: f32, gs_x: f32, cy: f32, dot: f32, pointer: Argb, scale: Argb, show_lateral: bool) {
    let n = &d.nav;
    if show_lateral && !n.cdi.is_nan() {
        for i in [-2, -1, 1, 2] {
            c.circle(cx + i as f32 * dot, loc_y, dot * 0.16, scale, 2.5);
        }
        c.line(cx, loc_y - dot * 0.45, cx, loc_y + dot * 0.45, YELLOW, 3.0);
        let x = cx + n.cdi.clamp(-2.4, 2.4) * dot;
        let pts = [x, loc_y - dot * 0.3, x + dot * 0.3, loc_y, x, loc_y + dot * 0.3, x - dot * 0.3, loc_y];
        if n.cdi.abs() >= 2.4 {
            c.poly(0, pointer, 3.0, &pts);
        } else {
            c.poly(pointer, pointer, 2.0, &pts);
        }
    }
    if !n.gs.is_nan() {
        for i in [-2, -1, 1, 2] {
            c.circle(gs_x, cy + i as f32 * dot, dot * 0.16, scale, 2.5);
        }
        c.line(gs_x - dot * 0.45, cy, gs_x + dot * 0.45, cy, YELLOW, 3.0);
        let y = cy - n.gs.clamp(-2.4, 2.4) * dot;
        c.poly(pointer, pointer, 2.0, &[gs_x, y - dot * 0.3, gs_x + dot * 0.3, y, gs_x, y + dot * 0.3, gs_x - dot * 0.3, y]);
    }
}

pub fn deviation_std(c: &mut Canvas, d: &FlightData, cx: f32, loc_y: f32, gs_x: f32, cy: f32, dot: f32, pointer: Argb) {
    deviation(c, d, cx, loc_y, gs_x, cy, dot, pointer, WHITE, true);
}

/// Opis źródła nawigacji (np. „ILS IGD 4.2NM”).
pub fn nav_line(d: &FlightData) -> Option<String> {
    let n = &d.nav;
    let src = match n.source {
        NavSource::Loc => {
            if !n.gs.is_nan() {
                "ILS"
            } else {
                "LOC"
            }
        }
        NavSource::Vor => "VOR",
        NavSource::Gps => "GPS",
        NavSource::None => return None,
    };
    let dme = if n.dme_nm.is_nan() { String::new() } else { format!(" {:.1}NM", n.dme_nm) };
    Some(format!("{src}{}{dme}", n.ident.as_ref().map(|i| format!(" {i}")).unwrap_or_default()))
}

/// Bug prędkości na prawej krawędzi taśmy.
pub fn speed_bug(c: &mut Canvas, x: f32, y: f32, color: Argb, wedge: bool) {
    if wedge {
        c.poly(color, color, 2.0, &[x, y, x + 22.0, y - 12.0, x + 22.0, y + 12.0]);
    } else {
        c.poly(0, color, 3.5, &[x, y - 18.0, x + 18.0, y - 18.0, x + 18.0, y - 8.0, x + 6.0, y, x + 18.0, y + 8.0, x + 18.0, y + 18.0, x, y + 18.0]);
    }
}

/// Bug wysokości na lewej krawędzi taśmy wysokości.
pub fn alt_bug(c: &mut Canvas, x: f32, y: f32, color: Argb) {
    c.poly(0, color, 3.5, &[x, y - 26.0, x + 16.0, y - 26.0, x + 16.0, y - 10.0, x + 6.0, y, x + 16.0, y + 10.0, x + 16.0, y + 26.0, x, y + 26.0]);
}

pub fn now_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

/// Marker: OM (cyjan), MM (bursztyn), IM (biały) w ramce, miga 2 Hz.
pub fn marker(c: &mut Canvas, d: &FlightData, x: f32, y: f32, size: f32) {
    let (t, col) = match d.marker {
        1 => ("OM", 0xFF33DDFF),
        2 => ("MM", AMBER),
        3 => ("IM", WHITE),
        _ => return,
    };
    if (now_ms() / 250) % 2 == 0 {
        return;
    }
    c.frame(x - size * 0.9, y - size * 0.6, x + size * 0.9, y + size * 0.6, col, 3.0);
    c.text_b(t, x, y, size, col);
}

/// Wybrana prędkość w kt albo Mach.
pub fn sel_speed(d: &FlightData) -> Option<String> {
    if !d.mach_bug.is_nan() {
        Some(format!(".{:02}", roundi(d.mach_bug * 100.0)))
    } else if !d.spd_bug.is_nan() {
        Some(roundi(d.spd_bug).to_string())
    } else {
        None
    }
}

/// Mach do wyświetlenia (od 0,40).
pub fn mach_text(d: &FlightData) -> Option<String> {
    if !d.mach.is_nan() && d.mach >= 0.4 {
        Some(format!(".{:03}", roundi(d.mach * 1000.0).min(999)))
    } else {
        None
    }
}

pub fn vmo(d: &FlightData, fallback: f32) -> f32 {
    if d.vmo.is_nan() {
        fallback
    } else {
        d.vmo
    }
}

/// Pas niskiej prędkości: od przeciągnięcia (czerwono-czarny) i zapas 1,23 Vs (bursztyn).
pub fn low_speed(c: &mut Canvas, d: &FlightData, x: f32, w: f32, y_of: impl Fn(f32) -> f32, bottom: f32) {
    if d.on_ground {
        return;
    }
    let vs = if d.flaps_index.unwrap_or(0) > 0 && !d.vs0.is_nan() { d.vs0 } else { d.vs1 };
    if vs.is_nan() {
        return;
    }
    let y_stall = y_of(vs);
    if y_stall < bottom {
        let mut y = y_stall;
        let mut red = true;
        while y < bottom {
            c.rect(x, y, x + w, y + 12.0, if red { RED } else { BLACK });
            y += 12.0;
            red = !red;
        }
    }
    let y_min = y_of(vs * 1.23);
    c.rect(x + w * 0.3, y_min, x + w * 0.7, y_stall.min(bottom), AMBER);
}

pub fn baro_text(d: &FlightData) -> String {
    if d.in_hg {
        format!("{:.2}", d.qnh * 0.029_53)
    } else {
        roundi(d.qnh).to_string()
    }
}

/// Barber pole (Vmo) w górę od prędkości `vmo` na taśmie.
pub fn barber_up(c: &mut Canvas, r: f32, w: f32, top: f32, y_vmo: f32) {
    let mut y = y_vmo;
    let mut red = true;
    while y > top {
        c.rect(r - w, y - 12.0, r, y, if red { RED } else { BLACK });
        y -= 12.0;
        red = !red;
    }
}

/// Strzałka trendu prędkości.
pub fn trend_arrow(c: &mut Canvas, x: f32, cy: f32, tr: f32, k: f32, color: Argb, w: f32, head: f32) {
    if tr.abs() <= 2.0 {
        return;
    }
    let y2 = cy - tr * k;
    c.line(x, cy, x, y2, color, w);
    let dir = sign(tr);
    c.line(x, y2, x - head * 0.62, y2 + dir * head, color, w);
    c.line(x, y2, x + head * 0.62, y2 + dir * head, color, w);
}

/// Czerwone chevrony wskazujące najkrótszą drogę do horyzontu.
pub fn pitch_chevrons(c: &mut Canvas, d: &FlightData, cx: f32, cy: f32, clip: &tiny_skia::Path, base: f32, step: f32, half: f32, drop: f32) {
    if d.pitch <= 30.0 && d.pitch >= -20.0 {
        return;
    }
    let dir = if d.pitch > 0.0 { 1.0 } else { -1.0 };
    c.save();
    c.clip_path(clip);
    c.rotate(-d.roll, cx, cy);
    for k in 1..=2 {
        let y = cy + dir * (base + k as f32 * step);
        c.line(cx - half, y + dir * drop, cx, y, RED, 6.0);
        c.line(cx, y, cx + half, y + dir * drop, RED, 6.0);
    }
    c.restore();
}

/// Wektor toru lotu („bird”) w układzie ADI obróconym o przechylenie.
pub fn bird(c: &mut Canvas, x: f32, y: f32, color: Argb, r: f32, wing: f32, tail: f32, w: f32) {
    c.circle(x, y, r, color, w);
    c.line(x - r, y, x - wing, y, color, w);
    c.line(x + r, y, x + wing, y, color, w);
    c.line(x, y - r, x, y - tail, color, w);
}

pub fn fpv_offset(d: &FlightData, ppd: f32) -> Option<(f32, f32)> {
    if d.fpv && d.has_track && d.speed_kt > 5.0 {
        Some((ang_diff(d.track, d.heading).clamp(-20.0, 20.0) * ppd, (d.fpa - d.pitch).clamp(-20.0, 20.0) * ppd))
    } else {
        None
    }
}
