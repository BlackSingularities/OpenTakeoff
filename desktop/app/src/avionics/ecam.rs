//! Wyświetlacz silników w stylu kabiny: ECAM E/WD Airbusa, EICAS Boeinga (737/787), ekrany silnika
//! ATR i Embraera. Łuki N1 i EGT z cyfrowymi odczytami, N2, przepływ paliwa, pozycja dźwigni,
//! paliwo na pokładzie, klapy, podwozie, spoilery i trym. Dla Airbusa także kwadratowe E/WD i SD jak w A320.

use super::data::{DisplayStyle, Engine, FlightData};
use super::gfx::*;

struct Pal {
    bg: Argb,
    arc: Argb,
    needle: Argb,
    value: Argb,
    label: Argb,
    unit: Argb,
    boxed: bool,
}

fn pal(style: DisplayStyle) -> Pal {
    match style {
        DisplayStyle::Airbus => Pal { bg: 0xFF000000, arc: WHITE, needle: 0xFF2BF22B, value: 0xFF2BF22B, label: WHITE, unit: 0xFF1FE6FF, boxed: false },
        DisplayStyle::Boeing | DisplayStyle::B787 => Pal { bg: 0xFF000000, arc: WHITE, needle: WHITE, value: WHITE, label: WHITE, unit: 0xFF33DDFF, boxed: true },
        DisplayStyle::Atr => Pal { bg: 0xFF06080C, arc: WHITE, needle: 0xFF27E327, value: 0xFF27E327, label: 0xFF2BD9F0, unit: 0xFF2BD9F0, boxed: false },
        _ => Pal { bg: 0xFF05070A, arc: WHITE, needle: 0xFF35E635, value: 0xFF35E635, label: 0xFF2FD8F2, unit: 0xFF2FD8F2, boxed: true },
    }
}

/// Kąt na łuku (0 = do góry, + zgodnie z zegarem): od -130° do +`end`°.
fn ang(v: f32, max: f32, end: f32) -> f32 {
    -130.0 + (v / max).clamp(0.0, 1.06) * (130.0 + end)
}

#[allow(clippy::too_many_arguments)]
fn dial(c: &mut Canvas, p: &Pal, cx: f32, cy: f32, r: f32, v: f32, max: f32, end: f32, amber: f32, red: f32, ticks: &[(f32, &str)], digits: usize, cmd: Option<f32>) {
    let a0 = ang(0.0, max, end);
    let a1 = ang(max, max, end);
    // łuk skali (od 0 do czerwonej kreski), czerwony zakres za czerwoną kreską
    c.arc(cx - r, cy - r, cx + r, cy + r, a0 - 90.0, ang(red, max, end) - a0, p.arc, r * 0.05);
    c.arc(cx - r, cy - r, cx + r, cy + r, ang(red, max, end) - 90.0, a1 - ang(red, max, end), RED, r * 0.07);
    if amber < red {
        c.radial(cx, cy, ang(amber, max, end), r * 0.84, r * 1.08, AMBER, r * 0.05);
    }
    c.radial(cx, cy, ang(red, max, end), r * 0.80, r * 1.14, RED, r * 0.06);
    for (tv, lbl) in ticks {
        let a = ang(*tv, max, end);
        c.radial(cx, cy, a, r * 0.86, r, p.arc, r * 0.04);
        if !lbl.is_empty() {
            let t = a.to_radians();
            c.text(lbl, cx + t.sin() * r * 0.68, cy - t.cos() * r * 0.68, r * 0.22, p.arc);
        }
    }
    // zadany ciąg (pozycja dźwigni) – cyjanowe kółko na łuku
    if let Some(cv) = cmd.filter(|x| !x.is_nan()) {
        let t = ang(cv, max, end).to_radians();
        c.circle(cx + t.sin() * r * 1.12, cy - t.cos() * r * 1.12, r * 0.07, 0xFF1FE6FF, r * 0.035);
    }
    let col = if !v.is_nan() && v >= red { RED } else if !v.is_nan() && v >= amber { AMBER } else { p.needle };
    // wskazówka i wypełniony sektor
    if !v.is_nan() {
        let av = ang(v, max, end);
        let mut pts = vec![cx, cy];
        let n = 24;
        for i in 0..=n {
            let t = (a0 + (av - a0) * i as f32 / n as f32).to_radians();
            pts.push(cx + t.sin() * r * 0.97);
            pts.push(cy - t.cos() * r * 0.97);
        }
        c.poly(alpha(col, 0x30), 0, 0.0, &pts);
        c.radial(cx, cy, av, 0.0, r * 1.02, col, r * 0.07);
    }
    // odczyt cyfrowy w ramce po prawej-dole
    let txt = if v.is_nan() { "XX".to_string() } else { format!("{:.*}", digits, v) };
    let (bx, by) = (cx + r * 0.62, cy + r * 0.42);
    let fs = r * 0.36;
    let bw = text_width(&txt, fs, Face::Mono) + r * 0.2;
    if p.boxed {
        c.rect(bx - bw, by - fs * 0.62, bx + r * 0.08, by + fs * 0.62, BLACK);
        c.frame(bx - bw, by - fs * 0.62, bx + r * 0.08, by + fs * 0.62, p.arc, r * 0.03);
    } else {
        c.frame(bx - bw, by - fs * 0.62, bx + r * 0.08, by + fs * 0.62, 0xFF707070, r * 0.025);
    }
    c.text_f(&txt, bx, by, fs, if v.is_nan() { AMBER } else { col }, Align::Right, Face::Mono);
}

pub fn draw(c: &mut Canvas, d: &FlightData, style: DisplayStyle, w: f32, h: f32) {
    let p = pal(style);
    c.rect(0.0, 0.0, w, h, p.bg);
    let engines: Vec<Engine> = if d.engines.is_empty() { vec![Engine { n1: f32::NAN, n2: f32::NAN, rpm: f32::NAN, egt: f32::NAN, ff: f32::NAN, thr: f32::NAN }; 2] } else { d.engines.iter().take(4).copied().collect() };
    let n = engines.len();
    let side = (w * 0.24).max(170.0).min(w * 0.4);
    let ew = w - side;
    let cw = ew / n as f32;
    // na każdy silnik: N1 (duży) i EGT obok albo pod spodem
    let wide = cw / h > 1.25;
    let r_n1 = if wide { (cw * 0.24).min(h * 0.30) } else { (cw * 0.36).min(h * 0.17) };
    let r_egt = r_n1 * 0.78;
    let fs = (r_n1 * 0.26).clamp(9.0, 30.0);
    let mid_y = h * 0.76;
    let step = if wide { fs * 1.2 } else { fs * 1.5 };
    for (i, e) in engines.iter().enumerate() {
        let x0 = cw * i as f32;
        let (n1c, egtc) = if wide { ((x0 + cw * 0.30, h * 0.42), (x0 + cw * 0.72, h * 0.44)) } else { ((x0 + cw * 0.5, h * 0.22), (x0 + cw * 0.5, h * 0.52)) };
        let thr_n1 = if e.thr.is_nan() { f32::NAN } else { 20.0 + e.thr / 100.0 * 82.0 };
        dial(c, &p, n1c.0, n1c.1, r_n1, e.n1, 110.0, 70.0, 100.0, 104.0, &[(50.0, "5"), (60.0, ""), (70.0, ""), (80.0, ""), (90.0, ""), (100.0, "10")], 1, Some(thr_n1));
        dial(c, &p, egtc.0, egtc.1, r_egt, e.egt, 1100.0, 40.0, 900.0, 950.0, &[(0.0, ""), (500.0, ""), (950.0, "")], 0, None);
        // N2 i FF
        let ty = if wide { mid_y } else { h * 0.74 };
        c.text_f(&if e.n2.is_nan() { "XX".into() } else { format!("{:.1}", e.n2) }, x0 + cw * 0.5, ty, fs, p.value, Align::Center, Face::Mono);
        c.text_f(&if e.ff.is_nan() { "XX".into() } else { format!("{:.0}", e.ff) }, x0 + cw * 0.5, ty + step, fs, p.value, Align::Center, Face::Mono);
        c.text_f(&format!("{}", i + 1), x0 + fs * 0.6, fs * 0.9, fs * 0.8, p.label, Align::Center, Face::CondBold);
    }
    // podpisy między silnikami
    let lx = if n >= 2 { cw } else { cw * 0.5 };
    let (ly1, ly2) = if wide { (h * 0.42, h * 0.44) } else { (h * 0.22, h * 0.52) };
    let lab = |c: &mut Canvas, x: f32, y: f32, t: &str, u: &str| {
        if wide {
            c.text_f(t, x - fs * 0.15, y, fs * 0.8, p.label, Align::Right, Face::Cond);
            c.text_f(u, x + fs * 0.15, y, fs * 0.65, p.unit, Align::Left, Face::Cond);
        } else {
            c.text_f(t, x, y - fs * 0.45, fs * 0.8, p.label, Align::Center, Face::Cond);
            c.text_f(u, x, y + fs * 0.45, fs * 0.65, p.unit, Align::Center, Face::Cond);
        }
    };
    if wide {
        lab(c, lx, ly1, "N1", "%");
    } else {
        lab(c, lx, ly1, "N1", "%");
        lab(c, lx, ly2, "EGT", "°C");
    }
    let ty = if wide { mid_y } else { h * 0.74 };
    lab(c, lx, ty, "N2", "%");
    lab(c, lx, ty + step, "FF", "KG/H");
    if wide {
        for i in 0..n {
            let x0 = cw * i as f32;
            c.text_f("EGT °C", x0 + cw * 0.72, h * 0.44 + r_egt * 1.05, fs * 0.6, p.unit, Align::Center, Face::Cond);
        }
    }
    // separator i panel boczny
    c.line(ew, h * 0.06, ew, h * 0.94, 0xFF505050, 1.5);
    let sx = ew + side * 0.08;
    let sfs = (side * 0.075).clamp(10.0, 24.0);
    let mut y = h * 0.14;
    let row = |c: &mut Canvas, y: f32, k: &str, v: String, col: Argb| {
        c.text_f(k, sx, y, sfs * 0.85, p.label, Align::Left, Face::Cond);
        c.text_f(&v, ew + side * 0.92, y, sfs, col, Align::Right, Face::Mono);
    };
    row(c, y, if style == DisplayStyle::Airbus { "FOB" } else { "FUEL" }, if d.fuel_kg.is_nan() { "XX".into() } else { format!("{} KG", roundi(d.fuel_kg)) }, p.value);
    y += sfs * 1.9;
    // klapy: wskaźnik słupkowy
    let (fi, fp) = (d.flaps_index.unwrap_or(0), d.flaps_positions.unwrap_or(1).max(1));
    c.text_f(if style == DisplayStyle::Airbus { "FLAP" } else { "FLAPS" }, sx, y, sfs * 0.85, p.label, Align::Left, Face::Cond);
    let txt = if style == DisplayStyle::Airbus {
        match (fi, fp) {
            (0, _) => "0".to_string(),
            (i, 5) if i == 4 => "FULL".to_string(),
            (i, _) => i.to_string(),
        }
    } else {
        format!("{fi}/{}", fp - 1)
    };
    c.text_f(&txt, ew + side * 0.92, y, sfs, p.value, Align::Right, Face::Mono);
    y += sfs * 1.1;
    let (bx0, bx1) = (sx, ew + side * 0.92);
    let steps = (fp - 1).max(1);
    c.line(bx0, y, bx1, y, 0xFF606060, sfs * 0.15);
    for k in 0..=steps {
        let x = bx0 + (bx1 - bx0) * k as f32 / steps as f32;
        c.line(x, y - sfs * 0.25, x, y + sfs * 0.25, 0xFF909090, 1.5);
    }
    let fx = bx0 + (bx1 - bx0) * fi as f32 / steps as f32;
    c.poly(p.value, 0, 0.0, &[bx0, y - sfs * 0.18, fx, y - sfs * 0.18, fx + sfs * 0.3, y, fx, y + sfs * 0.18, bx0, y + sfs * 0.18]);
    y += sfs * 1.6;
    if let Some(g) = d.gear_down {
        row(c, y, "GEAR", if g { "DOWN".into() } else { "UP".into() }, if g { 0xFF2BF22B } else { WHITE });
        y += sfs * 1.6;
    }
    row(c, y, "SPD BRK", if d.spoilers_pct.is_nan() { "XX".into() } else { format!("{}%", roundi(d.spoilers_pct)) }, if d.spoilers_pct > 5.0 { AMBER } else { p.value });
    y += sfs * 1.6;
    row(c, y, "TRIM", if d.trim_pct.is_nan() { "XX".into() } else { format!("{:+}%", roundi(d.trim_pct)) }, p.value);
    y += sfs * 1.6;
    if !d.oat_c.is_nan() && y < h * 0.95 {
        row(c, y, "SAT", format!("{:+} °C", roundi(d.oat_c)), p.value);
    }
}

// ---------- Airbus A320: kwadratowe E/WD i SD ----------

const GREEN: Argb = 0xFF2BF22B;
const CYAN: Argb = 0xFF1FE6FF;
const GREY: Argb = 0xFFB4B4B4;

/// Kwadrat wpisany w obszar: (x0, y0, bok).
fn square(w: f32, h: f32) -> (f32, f32, f32) {
    let s = w.min(h);
    ((w - s) / 2.0, (h - s) / 2.0, s)
}

/// Podpis między silnikami: nazwa (biała) nad jednostką (cyjan), jak na E/WD.
fn mid_label(c: &mut Canvas, x: f32, y: f32, s: f32, name: &str, unit: &str) {
    c.text_f(name, x, y - s * 0.017, s * 0.036, WHITE, Align::Center, Face::Cond);
    c.text_f(unit, x, y + s * 0.02, s * 0.03, CYAN, Align::Center, Face::Cond);
}

/// Wartość z mniejszą częścią po przecinku (N2 na E/WD: „83.2” z mniejszą dwójką).
fn split_value(c: &mut Canvas, v: f32, x: f32, y: f32, fs: f32, decimals: bool) {
    if v.is_nan() {
        c.text_f("XX", x, y, fs, AMBER, Align::Center, Face::Mono);
        return;
    }
    if !decimals {
        c.text_f(&format!("{}", roundi(v)), x, y, fs, GREEN, Align::Center, Face::Mono);
        return;
    }
    let whole = format!("{}.", v.trunc() as i32);
    let frac = format!("{}", ((v.fract().abs() * 10.0).round() as i32).min(9));
    let ww = text_width(&whole, fs, Face::Mono);
    let fw = text_width(&frac, fs * 0.75, Face::Mono);
    let left = x - (ww + fw) / 2.0;
    c.text_f(&whole, left, y, fs, GREEN, Align::Left, Face::Mono);
    c.text_f(&frac, left + ww, y + fs * 0.08, fs * 0.75, GREEN, Align::Left, Face::Mono);
}

/// E/WD Airbusa A320 w kwadracie: N1 i EGT na zegarach, N2 i FF cyfrowo, podpisy między silnikami,
/// FOB, wskaźnik slotów i klap, pod kreską pole komunikatów (memo).
pub fn draw_ewd(c: &mut Canvas, d: &FlightData, w: f32, h: f32) {
    c.rect(0.0, 0.0, w, h, BLACK);
    let (ox, oy, s) = square(w, h);
    let x = |f: f32| ox + f * s;
    let y = |f: f32| oy + f * s;
    let p = pal(DisplayStyle::Airbus);
    let nan = Engine { n1: f32::NAN, n2: f32::NAN, rpm: f32::NAN, egt: f32::NAN, ff: f32::NAN, thr: f32::NAN };
    let engines: Vec<Engine> = if d.engines.is_empty() { vec![nan; 2] } else { d.engines.iter().take(2).copied().collect() };
    let cols: &[f32] = if engines.len() == 1 { &[0.33] } else { &[0.17, 0.49] };
    let mid = 0.33;
    for (e, &cx) in engines.iter().zip(cols) {
        let thr_n1 = if e.thr.is_nan() { f32::NAN } else { 20.0 + e.thr / 100.0 * 82.0 };
        dial(c, &p, x(cx), y(0.19), s * 0.115, e.n1, 110.0, 70.0, 100.0, 104.0, &[(50.0, "5"), (60.0, ""), (70.0, ""), (80.0, ""), (90.0, ""), (100.0, "10")], 1, Some(thr_n1));
        dial(c, &p, x(cx), y(0.40), s * 0.078, e.egt, 1100.0, 40.0, 900.0, 950.0, &[(0.0, ""), (500.0, ""), (950.0, "")], 0, None);
        split_value(c, e.n2, x(cx), y(0.525), s * 0.046, true);
        split_value(c, if e.ff.is_nan() { f32::NAN } else { (e.ff / 20.0).round() * 20.0 }, x(cx), y(0.625), s * 0.046, false);
    }
    if engines.len() > 1 {
        mid_label(c, x(mid), y(0.19), s, "N1", "%");
        mid_label(c, x(mid), y(0.40), s, "EGT", "°C");
        mid_label(c, x(mid), y(0.525), s, "N2", "%");
        mid_label(c, x(mid), y(0.625), s, "FF", "KG/H");
    }

    // prawa kolumna: paliwo na pokładzie
    let fs = s * 0.042;
    c.text_f("FOB :", x(0.68), y(0.40), fs * 0.95, WHITE, Align::Left, Face::Cond);
    let fob = if d.fuel_kg.is_nan() { "XX".to_string() } else { format!("{}", (d.fuel_kg / 20.0).round() as i32 * 20) };
    c.text_f(&fob, x(0.905), y(0.40), fs * 1.05, if d.fuel_kg.is_nan() { AMBER } else { GREEN }, Align::Right, Face::Mono);
    c.text_f("KG", x(0.915), y(0.40), fs * 0.75, CYAN, Align::Left, Face::Cond);

    // wskaźnik slotów (S) i klap (F)
    let (fi, fp) = (d.flaps_index.unwrap_or(0).max(0), d.flaps_positions.unwrap_or(5).max(2));
    let nf = (fp - 1).max(1);
    let (cx, cy) = (x(0.815), y(0.525));
    c.poly(0, WHITE, s * 0.004, &[cx - s * 0.03, cy - s * 0.012, cx + s * 0.035, cy - s * 0.012, cx + s * 0.04, cy, cx - s * 0.035, cy]);
    c.text_f("S", x(0.69), y(0.505), fs * 0.85, WHITE, Align::Center, Face::Cond);
    c.text_f("F", x(0.945), y(0.505), fs * 0.85, WHITE, Align::Center, Face::Cond);
    let track = |c: &mut Canvas, sx: f32, sy: f32, dx: f32, dy: f32, steps: i32, pos: f32| {
        for k in 1..=steps {
            let f = k as f32 / steps as f32;
            c.circle(sx + dx * f, sy + dy * f, s * 0.0045, WHITE, 0.0);
        }
        if pos > 0.0 {
            let (ex, ey) = (sx + dx * pos, sy + dy * pos);
            let n = (dy, -dx);
            let l = (n.0 * n.0 + n.1 * n.1).sqrt().max(1e-3);
            let (nx, ny) = (n.0 / l * s * 0.008, n.1 / l * s * 0.008);
            c.poly(GREEN, 0, 0.0, &[sx + nx, sy + ny, ex + nx, ey + ny, ex - nx, ey - ny, sx - nx, sy - ny]);
        }
    };
    let slat = match fi {
        0 => 0.0,
        1 => 1.0 / 3.0,
        2 | 3 => 2.0 / 3.0,
        _ => 1.0,
    };
    track(c, cx - s * 0.035, cy + s * 0.004, -s * 0.095, s * 0.045, 3, slat);
    track(c, cx + s * 0.04, cy + s * 0.004, s * 0.1, s * 0.055, nf, fi as f32 / nf as f32);
    let conf = match (fi, fp) {
        (0, _) => String::new(),
        (i, 5) if i == 4 => "FULL".into(),
        (i, _) => i.to_string(),
    };
    c.text_f(&conf, cx, y(0.595), fs * 1.05, GREEN, Align::Center, Face::Mono);

    // pole komunikatów pod kreską
    c.line(x(0.02), y(0.68), x(0.98), y(0.68), GREY, s * 0.004);
    c.line(x(0.64), y(0.70), x(0.64), y(0.96), GREY, s * 0.004);
    let mfs = s * 0.04;
    let mut left: Vec<(String, Argb)> = Vec::new();
    let mut right: Vec<(String, Argb)> = Vec::new();
    if d.engines.is_empty() {
        left.push(("ENG DATA NOT AVAIL".into(), AMBER));
    }
    match d.gear_down {
        Some(true) if !d.on_ground => left.push(("LDG GEAR DN".into(), GREEN)),
        Some(false) if d.on_ground => left.push(("L/G NOT DOWN".into(), RED)),
        _ => {}
    }
    if !d.on_ground && d.ra_valid && d.ra_ft < 2000.0 && d.vs_fpm < -300.0 && d.flaps_index.unwrap_or(0) == 0 {
        left.push(("FLAPS NOT IN LDG CONF".into(), AMBER));
    }
    if d.spoilers_pct > 5.0 {
        right.push(("SPEED BRK".into(), if !d.on_ground && engines.iter().any(|e| e.n1 > 50.0) { AMBER } else { GREEN }));
    }
    if d.on_ground && engines.iter().all(|e| e.n1.is_nan() || e.n1 < 25.0) && d.gs_kt < 1.0 {
        right.push(("PARK BRK".into(), GREEN));
    }
    if d.std {
        right.push(("STD BARO".into(), CYAN));
    }
    for (i, (t, col)) in left.iter().take(5).enumerate() {
        c.text_f(t, x(0.05), y(0.72 + i as f32 * 0.055), mfs, *col, Align::Left, Face::Cond);
    }
    for (i, (t, col)) in right.iter().take(5).enumerate() {
        c.text_f(t, x(0.67), y(0.72 + i as f32 * 0.055), mfs, *col, Align::Left, Face::Cond);
    }
}

/// SD Airbusa w kwadracie: strona WHEEL (spoilery, podwozie, trym) i dolny pasek SAT / UTC / FOB.
pub fn draw_sd(c: &mut Canvas, d: &FlightData, w: f32, h: f32) {
    c.rect(0.0, 0.0, w, h, BLACK);
    let (ox, oy, s) = square(w, h);
    let x = |f: f32| ox + f * s;
    let y = |f: f32| oy + f * s;
    let fs = s * 0.045;
    c.text_f("WHEEL", x(0.04), y(0.06), fs * 1.1, WHITE, Align::Left, Face::Cond);
    let tw = text_width("WHEEL", fs * 1.1, Face::Cond);
    c.line(x(0.04), y(0.06) + fs * 0.7, x(0.04) + tw, y(0.06) + fs * 0.7, WHITE, s * 0.004);

    // spoilery: 5 paneli na skrzydło, zielona strzałka w górę, gdy wysunięte
    let up = d.spoilers_pct > 5.0;
    for side in [-1.0f32, 1.0] {
        for k in 0..5 {
            let px = x(0.5) + side * s * (0.09 + k as f32 * 0.07);
            let py = y(0.20) - k as f32 * s * 0.008;
            c.line(px - s * 0.025, py, px + s * 0.025, py, if d.spoilers_pct.is_nan() { AMBER } else { GREEN }, s * 0.006);
            if up {
                c.line(px, py, px, py - s * 0.05, GREEN, s * 0.006);
                c.poly(GREEN, 0, 0.0, &[px, py - s * 0.07, px - s * 0.014, py - s * 0.045, px + s * 0.014, py - s * 0.045]);
            }
        }
    }
    c.text_f("SPD BRK", x(0.5), y(0.27), fs * 0.75, WHITE, Align::Center, Face::Cond);

    // podwozie: przednie i dwa główne, po dwa trójkąty (zielone – wypuszczone i zablokowane)
    let gear = |c: &mut Canvas, gx: f32, gy: f32, label: &str| {
        c.frame(gx - s * 0.075, gy - s * 0.045, gx + s * 0.075, gy + s * 0.045, 0xFF606060, s * 0.003);
        for dx in [-0.035f32, 0.035] {
            let tx = gx + dx * s;
            match d.gear_down {
                Some(true) => c.poly(GREEN, 0, 0.0, &[tx - s * 0.028, gy - s * 0.022, tx + s * 0.028, gy - s * 0.022, tx, gy + s * 0.03]),
                Some(false) => {}
                None => c.text_f("X", tx, gy, fs, AMBER, Align::Center, Face::Mono),
            }
        }
        c.text_f(label, gx, gy + s * 0.075, fs * 0.7, WHITE, Align::Center, Face::Cond);
    };
    gear(c, x(0.5), y(0.40), "NOSE");
    gear(c, x(0.24), y(0.56), "LH");
    gear(c, x(0.76), y(0.56), "RH");
    if d.gear_down == Some(false) {
        c.text_f("UP LOCK", x(0.5), y(0.56), fs * 0.8, GREEN, Align::Center, Face::Cond);
    }

    // trym wysokości
    c.text_f("PITCH TRIM", x(0.06), y(0.74), fs * 0.85, WHITE, Align::Left, Face::Cond);
    let trim = if d.trim_pct.is_nan() { "XX".to_string() } else { format!("{:+}%", roundi(d.trim_pct)) };
    c.text_f(&trim, x(0.42), y(0.74), fs, if d.trim_pct.is_nan() { AMBER } else { GREEN }, Align::Left, Face::Mono);
    c.text_f("G LOAD", x(0.62), y(0.74), fs * 0.85, WHITE, Align::Left, Face::Cond);
    let g = if d.g_load.is_nan() { "XX".to_string() } else { format!("{:.1}", d.g_load) };
    let gc = if d.g_load.is_nan() { AMBER } else if d.g_load > 1.8 || d.g_load < 0.2 { AMBER } else { GREEN };
    c.text_f(&g, x(0.95), y(0.74), fs, gc, Align::Right, Face::Mono);

    // dolny pasek jak na SD: SAT, UTC, FOB
    c.line(x(0.02), y(0.84), x(0.98), y(0.84), GREY, s * 0.004);
    for xf in [0.34f32, 0.66] {
        c.line(x(xf), y(0.85), x(xf), y(0.98), GREY, s * 0.004);
    }
    let by = y(0.91);
    c.text_f("SAT", x(0.04), by, fs * 0.8, WHITE, Align::Left, Face::Cond);
    let sat = if d.oat_c.is_nan() { "XX".to_string() } else { format!("{:+}", roundi(d.oat_c)) };
    c.text_f(&sat, x(0.25), by, fs, if d.oat_c.is_nan() { AMBER } else { GREEN }, Align::Right, Face::Mono);
    c.text_f("°C", x(0.26), by, fs * 0.7, CYAN, Align::Left, Face::Cond);
    let utc = d.sim_time.clone().unwrap_or_default();
    let hm: String = utc.chars().filter(|ch| ch.is_ascii_digit()).take(4).collect();
    c.text_f("UTC", x(0.5), by - fs * 0.6, fs * 0.65, WHITE, Align::Center, Face::Cond);
    if hm.len() == 4 {
        c.text_f(&format!("{} {}", &hm[..2], &hm[2..]), x(0.5), by + fs * 0.35, fs, GREEN, Align::Center, Face::Mono);
    }
    c.text_f("FOB", x(0.69), by, fs * 0.8, WHITE, Align::Left, Face::Cond);
    let fob = if d.fuel_kg.is_nan() { "XX".to_string() } else { format!("{}", (d.fuel_kg / 20.0).round() as i32 * 20) };
    c.text_f(&fob, x(0.89), by, fs, if d.fuel_kg.is_nan() { AMBER } else { GREEN }, Align::Right, Face::Mono);
    c.text_f("KG", x(0.9), by, fs * 0.7, CYAN, Align::Left, Face::Cond);
}
