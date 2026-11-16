//! Wyświetlacz silników w stylu kabiny: ECAM E/WD Airbusa, EICAS Boeinga (737/787), ekrany silnika
//! ATR i Embraera. Łuki N1 i EGT z cyfrowymi odczytami, N2, przepływ paliwa, pozycja dźwigni,
//! paliwo na pokładzie, klapy, podwozie, spoilery i trym.

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
