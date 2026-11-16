//! Kwadratowe ekrany silników i systemów w stylu kabin (na wzór zdjęć prawdziwych kokpitów):
//! Boeing 737 (MAX: kolumna silników i prawa część z klapami, podwoziem i paliwem), Boeing 787 (EICAS),
//! Embraer E190 (EICAS) i ATR 72-600 (EWD). Każdy typ ma dwa kwadraty: [`draw_primary`] – parametry
//! silników, [`draw_secondary`] – reszta ekranu (komunikaty, klapy, podwozie, paliwo, trym).
//! Airbus ma własne E/WD i SD w [`super::ecam`].

use super::data::{DisplayStyle, Engine, FlightData};
use super::gfx::*;

const GREEN: Argb = 0xFF2BF22B;
const CYAN: Argb = 0xFF33DDFF;
const MAGENTA: Argb = 0xFFFF33E6;
const GREY: Argb = 0xFF8C8C8C;
const DIM: Argb = 0xFF505050;

/// Kwadrat wpisany w obszar: (x0, y0, bok).
fn square(w: f32, h: f32) -> (f32, f32, f32) {
    let s = w.min(h);
    ((w - s) / 2.0, (h - s) / 2.0, s)
}

fn engines(d: &FlightData) -> Vec<Engine> {
    if d.engines.is_empty() {
        vec![Engine::NAN; 2]
    } else {
        d.engines.iter().take(2).copied().collect()
    }
}

fn num(v: f32, digits: usize) -> String {
    if v.is_nan() {
        "--".into()
    } else {
        format!("{:.*}", digits, v)
    }
}

/// Wskaźnik łukowy. Kąty jak na zegarze (0 = do góry, + w prawo); `start` – kąt zera, `sweep` – zakres.
struct Gauge<'a> {
    r: f32,
    max: f32,
    start: f32,
    sweep: f32,
    amber: f32,
    red: f32,
    /// Kolorowe pasy na łuku (od, do, kolor) – np. zielony zakres ITT w ATR.
    bands: &'a [(f32, f32, Argb)],
    ticks: &'a [(f32, &'a str)],
    needle: Argb,
    fill: bool,
    /// Zielony znacznik nastawy (np. N1 z dźwigni) i jego kolor.
    bug: Option<(f32, Argb)>,
}

impl Gauge<'_> {
    fn ang(&self, v: f32) -> f32 {
        self.start + (v / self.max).clamp(0.0, 1.05) * self.sweep
    }

    fn draw(&self, c: &mut Canvas, cx: f32, cy: f32, v: f32) -> Argb {
        let r = self.r;
        let (a0, ared) = (self.ang(0.0), self.ang(self.red));
        c.arc(cx - r, cy - r, cx + r, cy + r, a0 - 90.0, ared - a0, WHITE, r * 0.05);
        for (from, to, col) in self.bands {
            let (b0, b1) = (self.ang(*from), self.ang(*to));
            c.arc(cx - r * 0.9, cy - r * 0.9, cx + r * 0.9, cy + r * 0.9, b0 - 90.0, b1 - b0, *col, r * 0.09);
        }
        if self.amber < self.red {
            c.radial(cx, cy, self.ang(self.amber), r * 0.82, r * 1.06, AMBER, r * 0.05);
        }
        c.radial(cx, cy, ared, r * 0.78, r * 1.14, RED, r * 0.07);
        for (tv, lbl) in self.ticks {
            let a = self.ang(*tv);
            c.radial(cx, cy, a, r * 0.84, r, WHITE, r * 0.035);
            if !lbl.is_empty() {
                let t = a.to_radians();
                c.text_f(lbl, cx + t.sin() * r * 0.66, cy - t.cos() * r * 0.66, r * 0.24, WHITE, Align::Center, Face::Cond);
            }
        }
        if let Some((bv, col)) = self.bug.filter(|b| !b.0.is_nan()) {
            let t = self.ang(bv).to_radians();
            let (bx, by) = (cx + t.sin() * r * 1.1, cy - t.cos() * r * 1.1);
            c.save();
            c.rotate(self.ang(bv), bx, by);
            c.poly(col, 0, 0.0, &[bx, by + r * 0.04, bx - r * 0.07, by - r * 0.1, bx + r * 0.07, by - r * 0.1]);
            c.restore();
        }
        let col = if v.is_nan() {
            AMBER
        } else if v >= self.red {
            RED
        } else if v >= self.amber {
            AMBER
        } else {
            self.needle
        };
        if !v.is_nan() {
            let av = self.ang(v);
            if self.fill {
                let mut pts = vec![cx, cy];
                for i in 0..=24 {
                    let t = (a0 + (av - a0) * i as f32 / 24.0).to_radians();
                    pts.push(cx + t.sin() * r * 0.96);
                    pts.push(cy - t.cos() * r * 0.96);
                }
                c.poly(alpha(col, 0x40), 0, 0.0, &pts);
            }
            c.radial(cx, cy, av, 0.0, r * 1.0, col, r * 0.075);
        }
        col
    }
}

/// Cyfrowy odczyt w ramce: (środek x, środek y), wysokość czcionki, kolor ramki i tekstu.
fn value_box(c: &mut Canvas, txt: &str, x: f32, y: f32, fs: f32, w: f32, frame: Argb, col: Argb) {
    c.rect(x - w / 2.0, y - fs * 0.62, x + w / 2.0, y + fs * 0.62, BLACK);
    c.frame(x - w / 2.0, y - fs * 0.62, x + w / 2.0, y + fs * 0.62, frame, fs * 0.08);
    c.text_f(txt, x + w / 2.0 - fs * 0.25, y, fs, col, Align::Right, Face::Mono);
}

/// Komunikaty na wzór CAS/memo z dostępnych danych: (tekst, kolor).
fn messages(d: &FlightData, style: DisplayStyle) -> Vec<(String, Argb)> {
    let mut m = Vec::new();
    if d.engines.is_empty() {
        m.push((
            match style {
                DisplayStyle::Atr => "ENG DATA NOT AVAIL",
                _ => "ENG DATA INVALID",
            }
            .to_string(),
            AMBER,
        ));
    }
    let airborne = !d.on_ground;
    if d.spoilers_pct > 5.0 && airborne {
        let t = match style {
            DisplayStyle::Embraer => "SPDBRK",
            DisplayStyle::Atr => "SPOILERS",
            _ => "SPEEDBRAKE EXTENDED",
        };
        m.push((t.to_string(), if style == DisplayStyle::Embraer { WHITE } else { AMBER }));
    }
    if d.on_ground && d.gear_down == Some(false) {
        m.push(("CONFIG GEAR".into(), RED));
    }
    if d.parking_brake == Some(true) {
        let (t, col) = match style {
            DisplayStyle::Embraer => ("PRKG BRK ON", CYAN),
            DisplayStyle::Atr => ("PARKING BRAKE ON", GREEN),
            _ => ("PARKING BRAKE SET", WHITE),
        };
        m.push((t.to_string(), col));
    }
    if d.spoilers_armed && style != DisplayStyle::Atr {
        let t = if style == DisplayStyle::Embraer { "SPDBRK ARMED" } else { "SPEEDBRAKE ARMED" };
        m.push((t.to_string(), WHITE));
    }
    m
}

/// Tryb ciągu nad zegarami N1 (przybliżony z fazy lotu – symulator nie podaje trybu FMC).
fn thrust_mode(d: &FlightData) -> &'static str {
    if d.on_ground {
        "TO"
    } else if d.vs_fpm > 400.0 {
        "CLB"
    } else {
        "CRZ"
    }
}

fn fmt_temp(t: f32) -> String {
    if t.is_nan() {
        "--".into()
    } else {
        format!("{:+}c", t.round() as i32)
    }
}

fn flap_label(style: DisplayStyle, d: &FlightData) -> (i32, i32, String) {
    let (fi, fp) = (d.flaps_index.unwrap_or(0).max(0), d.flaps_positions.unwrap_or(2).max(2));
    let name = |labels: &[&str]| -> String {
        if fp as usize == labels.len() {
            labels[fi.clamp(0, fp - 1) as usize].to_string()
        } else if fi == 0 {
            labels[0].to_string()
        } else {
            fi.to_string()
        }
    };
    let t = match style {
        DisplayStyle::Boeing => name(&["UP", "1", "2", "5", "10", "15", "25", "30", "40"]),
        DisplayStyle::B787 => name(&["UP", "1", "5", "15", "17", "18", "20", "25", "30"]),
        DisplayStyle::Embraer => name(&["0", "1", "2", "3", "4", "5", "FULL"]),
        DisplayStyle::Atr => name(&["0", "15", "30"]),
        _ => fi.to_string(),
    };
    (fi, fp, t)
}

// ---------- Boeing 737 / 787: kolumna silników ----------

fn boeing_engines(c: &mut Canvas, d: &FlightData, s: f32, ox: f32, oy: f32, b787: bool) {
    let x = |f: f32| ox + f * s;
    let y = |f: f32| oy + f * s;
    let fs = s * 0.04;
    c.text_f("TAT", x(0.04), y(0.035), fs * 0.85, CYAN, Align::Left, Face::Cond);
    c.text_f(&fmt_temp(d.oat_c), x(0.12), y(0.035), fs, WHITE, Align::Left, Face::Mono);
    c.text_f(thrust_mode(d), x(0.44), y(0.035), fs, GREEN, Align::Center, Face::Cond);
    let es = engines(d);
    let cols: &[f32] = if es.len() == 1 { &[0.45] } else { &[0.24, 0.64] };
    // Boeing: zero na „dziewiątej”, łuk 210° przez górę, czerwona kreska po prawej; odczyt cyfrowy w ramce nad zegarem
    let n1 = Gauge { r: s * 0.11, max: 110.0, start: -90.0, sweep: 210.0, amber: 102.0, red: 104.0, bands: &[], ticks: &[(0.0, "0"), (20.0, "2"), (40.0, "4"), (60.0, "6"), (80.0, "8"), (100.0, "10")], needle: WHITE, fill: true, bug: None };
    let egt = Gauge { r: s * 0.095, max: 1100.0, start: -90.0, sweep: 210.0, amber: 950.0, red: 1000.0, bands: &[], ticks: &[], needle: WHITE, fill: true, bug: None };
    let n2 = Gauge { r: s * 0.085, max: 110.0, start: -90.0, sweep: 210.0, amber: 105.0, red: 105.5, bands: &[], ticks: &[], needle: WHITE, fill: true, bug: None };
    for (e, &cx) in es.iter().zip(cols) {
        let cmd = if e.thr.is_nan() { f32::NAN } else { 20.0 + e.thr / 100.0 * 82.0 };
        let g = Gauge { bug: Some((cmd, GREEN)), ..n1 };
        let col = g.draw(c, x(cx), y(0.25), e.n1);
        // N1 zadane z dźwigni (zielone) nad odczytem
        if !cmd.is_nan() {
            if b787 {
                c.rect(x(cx - 0.01), y(0.06), x(cx + 0.12), y(0.09), 0xFF103A10);
            }
            c.text_f(&format!("{cmd:.1}"), x(cx + 0.115), y(0.075), fs * 0.8, GREEN, Align::Right, Face::Mono);
        }
        value_box(c, &num(e.n1, 1), x(cx + 0.055), y(0.12), fs * 1.1, s * 0.13, WHITE, if col == WHITE { WHITE } else { col });
        let col = egt.draw(c, x(cx), y(0.50), e.egt);
        value_box(c, &num(e.egt, 0), x(cx + 0.05), y(0.385), fs * 1.0, s * 0.11, WHITE, if col == WHITE { WHITE } else { col });
        let col = n2.draw(c, x(cx), y(0.725), e.n2);
        value_box(c, &num(e.n2, 1), x(cx + 0.05), y(0.62), fs * 0.95, s * 0.11, WHITE, if col == WHITE { WHITE } else { col });
        let ff = if e.ff.is_nan() { "--".to_string() } else { format!("{:.2}", e.ff / 1000.0) };
        value_box(c, &ff, x(cx), y(0.88), fs * 0.95, s * 0.13, WHITE, WHITE);
    }
    if es.len() > 1 {
        for (t, yy) in [("N1", 0.25), ("EGT", 0.50), ("N2", 0.725)] {
            c.text_f(t, x(0.44), y(yy), fs * 0.8, CYAN, Align::Center, Face::Cond);
        }
        c.text_f("FF", x(0.44), y(0.875), fs * 0.8, CYAN, Align::Center, Face::Cond);
        c.text_f("KGH x 1000", x(0.44), y(0.92), fs * 0.55, CYAN, Align::Center, Face::Cond);
    }
}

// ---------- Boeing 737 MAX: klapy, podwozie, paliwo ----------

fn b737_status(c: &mut Canvas, d: &FlightData, s: f32, ox: f32, oy: f32) {
    let x = |f: f32| ox + f * s;
    let y = |f: f32| oy + f * s;
    let fs = s * 0.04;
    // komunikaty (miejsce alertów silników u góry)
    for (i, (t, col)) in messages(d, DisplayStyle::Boeing).iter().take(3).enumerate() {
        c.text_f(t, x(0.06), y(0.06 + i as f32 * 0.05), fs * 0.85, *col, Align::Left, Face::Cond);
    }
    // okrągły wskaźnik klap jak w 737 MAX
    let (fi, fp, txt) = flap_label(DisplayStyle::Boeing, d);
    let (cx, cy, r) = (x(0.5), y(0.38), s * 0.17);
    let labels = ["UP", "1", "2", "5", "10", "15", "25", "30", "40"];
    let n = if fp as usize == labels.len() { labels.len() } else { fp as usize };
    let ang = |k: usize| -150.0 + 300.0 * k as f32 / (n.max(2) - 1) as f32;
    c.circle(cx, cy, r, DIM, s * 0.004);
    for k in 0..n {
        let a = ang(k);
        c.radial(cx, cy, a, r * 0.86, r, WHITE, s * 0.005);
        let t = a.to_radians();
        let l = if n == labels.len() { labels[k].to_string() } else { k.to_string() };
        c.text_f(&l, cx + t.sin() * r * 1.22, cy - t.cos() * r * 1.22, fs * 0.75, WHITE, Align::Center, Face::Cond);
    }
    let a = ang(fi.clamp(0, n as i32 - 1) as usize);
    c.radial(cx, cy, a, 0.0, r * 0.84, if fi == 0 { WHITE } else { GREEN }, s * 0.012);
    c.circle(cx, cy, s * 0.012, WHITE, 0.0);
    c.text_f("FLAPS", cx, cy + r * 0.62, fs * 0.8, CYAN, Align::Center, Face::Cond);
    c.text_f(&txt, cx, cy - r * 0.42, fs * 0.9, GREEN, Align::Center, Face::Mono);
    // światła podwozia (jak panel LANDING GEAR – zielone przy wypuszczonym)
    let gy = y(0.65);
    for (k, t) in ["LEFT", "NOSE", "RIGHT"].iter().enumerate() {
        let gx = x(0.2 + k as f32 * 0.3);
        let (l, r2, t2, b) = (gx - s * 0.12, gx + s * 0.12, gy - s * 0.04, gy + s * 0.04);
        match d.gear_down {
            Some(true) => {
                c.rect(l, t2, r2, b, 0xFF0F6E1F);
                c.text_f(&format!("{t} GEAR"), gx, gy, fs * 0.7, 0xFFB8FFB8, Align::Center, Face::CondBold);
            }
            Some(false) => {
                c.frame(l, t2, r2, b, DIM, s * 0.003);
                c.text_f(&format!("{t} GEAR"), gx, gy, fs * 0.7, DIM, Align::Center, Face::Cond);
            }
            None => c.text_f("--", gx, gy, fs, AMBER, Align::Center, Face::Mono),
        }
    }
    // paliwo: trzy łuki (CTR u góry, 1 i 2 po bokach) i suma
    let tanks = d.fuel_tanks;
    let max_wing = 3900.0;
    let max_ctr = 13000.0;
    let arc_gauge = |c: &mut Canvas, gx: f32, gy: f32, v: f32, max: f32, label: &str| {
        let g = Gauge { r: s * 0.075, max, start: -120.0, sweep: 240.0, amber: max * 2.0, red: max * 2.0, bands: &[], ticks: &[], needle: WHITE, fill: false, bug: None };
        g.draw(c, gx, gy, v);
        c.text_f(label, gx, gy - s * 0.035, fs * 0.65, WHITE, Align::Center, Face::Cond);
        c.text_f(&if v.is_nan() { "--".into() } else { format!("{}", v.round() as i32) }, gx, gy + s * 0.03, fs * 0.85, WHITE, Align::Center, Face::Mono);
    };
    let (l, ctr, r2) = tanks.unwrap_or((f32::NAN, f32::NAN, f32::NAN));
    arc_gauge(c, x(0.5), y(0.79), ctr, max_ctr, "CTR");
    arc_gauge(c, x(0.22), y(0.86), l, max_wing, "1");
    arc_gauge(c, x(0.78), y(0.86), r2, max_wing, "2");
    c.text_f("FUEL KG", x(0.5), y(0.885), fs * 0.75, CYAN, Align::Center, Face::Cond);
    c.text_f("TOTAL", x(0.36), y(0.955), fs * 0.75, CYAN, Align::Left, Face::Cond);
    c.text_f(&if d.fuel_kg.is_nan() { "--".into() } else { format!("{}", d.fuel_kg.round() as i32) }, x(0.66), y(0.955), fs * 1.05, WHITE, Align::Right, Face::Mono);
}

// ---------- Boeing 787: prawa część EICAS ----------

fn b787_status(c: &mut Canvas, d: &FlightData, s: f32, ox: f32, oy: f32) {
    let x = |f: f32| ox + f * s;
    let y = |f: f32| oy + f * s;
    let fs = s * 0.04;
    for (i, (t, col)) in messages(d, DisplayStyle::B787).iter().take(6).enumerate() {
        c.text_f(t, x(0.05), y(0.06 + i as f32 * 0.05), fs * 0.85, *col, Align::Left, Face::Cond);
    }
    // podwozie: zielona ramka DOWN, biała UP
    let (gx, gy) = (x(0.72), y(0.40));
    match d.gear_down {
        Some(true) => {
            c.rect(gx - s * 0.08, gy - s * 0.035, gx + s * 0.08, gy + s * 0.035, GREEN);
            c.text_f("DOWN", gx, gy, fs, BLACK, Align::Center, Face::CondBold);
        }
        Some(false) => {
            c.frame(gx - s * 0.08, gy - s * 0.035, gx + s * 0.08, gy + s * 0.035, WHITE, s * 0.004);
            c.text_f("UP", gx, gy, fs, WHITE, Align::Center, Face::CondBold);
        }
        None => c.text_f("--", gx, gy, fs, AMBER, Align::Center, Face::Mono),
    }
    c.text_f("GEAR", gx, gy + s * 0.065, fs * 0.8, CYAN, Align::Center, Face::Cond);
    // klapy: pionowa taśma z zielonym wypełnieniem i wartością w ramce obok
    let (fi, fp, txt) = flap_label(DisplayStyle::B787, d);
    let (tx, t0, t1) = (x(0.30), y(0.33), y(0.62));
    c.frame(tx - s * 0.018, t0, tx + s * 0.018, t1, WHITE, s * 0.004);
    let f = fi as f32 / (fp - 1).max(1) as f32;
    if fi > 0 {
        c.rect(tx - s * 0.014, t0 + s * 0.004, tx + s * 0.014, t0 + (t1 - t0) * f, GREEN);
    }
    let my = t0 + (t1 - t0) * f;
    c.poly(MAGENTA, 0, 0.0, &[tx + s * 0.022, my, tx + s * 0.045, my - s * 0.014, tx + s * 0.045, my + s * 0.014]);
    value_box(c, &if fi == 0 { "UP".to_string() } else { txt }, x(0.17), my.clamp(t0 + s * 0.02, t1 - s * 0.02), fs, s * 0.1, GREEN, GREEN);
    c.text_f("FLAPS", tx, t1 + s * 0.04, fs * 0.8, CYAN, Align::Center, Face::Cond);
    // trym stabilizatora (787 pokazuje go na ziemi)
    if d.on_ground && !d.trim_pct.is_nan() {
        c.text_f("STAB", x(0.62), y(0.56), fs * 0.8, CYAN, Align::Left, Face::Cond);
        c.text_f(&format!("{:+}%", d.trim_pct.round() as i32), x(0.92), y(0.56), fs, GREEN, Align::Right, Face::Mono);
    }
    // dolny pas: paliwo w zbiornikach, suma, SAT
    let (b0, b1) = (y(0.70), y(0.98));
    c.rect(x(0.02), b0, x(0.98), b1, 0xFF262230);
    c.text_f("FUEL QTY", x(0.5), b0 + s * 0.035, fs * 0.75, CYAN, Align::Center, Face::Cond);
    c.frame(x(0.08), b0 + s * 0.06, x(0.92), b0 + s * 0.13, CYAN, s * 0.003);
    let (l, ctr, r) = d.fuel_tanks.unwrap_or((f32::NAN, f32::NAN, f32::NAN));
    for (v, xf) in [(l, 0.22), (ctr, 0.5), (r, 0.78)] {
        let t = if v.is_nan() { "--".to_string() } else { format!("{:.1}", v / 1000.0) };
        c.text_f(&t, x(xf), b0 + s * 0.095, fs, WHITE, Align::Center, Face::Mono);
    }
    c.text_f("KGS X 1000", x(0.5), b0 + s * 0.16, fs * 0.6, CYAN, Align::Center, Face::Cond);
    c.text_f("TOTAL FUEL", x(0.62), b0 + s * 0.2, fs * 0.7, CYAN, Align::Left, Face::Cond);
    let tot = if d.fuel_kg.is_nan() { "--".to_string() } else { format!("{:.1}", d.fuel_kg / 1000.0) };
    c.text_f(&tot, x(0.94), b0 + s * 0.245, fs * 1.1, WHITE, Align::Right, Face::Mono);
    c.text_f("SAT", x(0.06), b0 + s * 0.235, fs * 0.75, CYAN, Align::Left, Face::Cond);
    c.text_f(&fmt_temp(d.oat_c), x(0.15), b0 + s * 0.235, fs, WHITE, Align::Left, Face::Mono);
}

// ---------- Embraer E190 ----------

fn e190_engines(c: &mut Canvas, d: &FlightData, s: f32, ox: f32, oy: f32) {
    let x = |f: f32| ox + f * s;
    let y = |f: f32| oy + f * s;
    let fs = s * 0.042;
    let es = engines(d);
    let cols: &[f32] = if es.len() == 1 { &[0.5] } else { &[0.25, 0.75] };
    // E-Jet: tarcza prawie pełna, zero na „siódmej”, pole cyfrowe pod zegarem, wskazówka zielona
    let n1 = Gauge { r: s * 0.15, max: 110.0, start: -140.0, sweep: 280.0, amber: 101.0, red: 102.0, bands: &[], ticks: &[(0.0, ""), (50.0, ""), (100.0, "")], needle: GREEN, fill: false, bug: None };
    let itt = Gauge { r: s * 0.12, max: 1100.0, start: -140.0, sweep: 280.0, amber: 980.0, red: 1006.0, bands: &[], ticks: &[], needle: GREEN, fill: false, bug: None };
    c.text_f(thrust_mode(d), x(0.5), y(0.035), fs * 0.85, GREEN, Align::Center, Face::Cond);
    for (e, &cx) in es.iter().zip(cols) {
        let cmd = if e.thr.is_nan() { f32::NAN } else { 20.0 + e.thr / 100.0 * 82.0 };
        c.text_f(&num(cmd, 1), x(cx), y(0.035), fs * 0.8, GREEN, Align::Center, Face::Mono);
        let g = Gauge { bug: Some((cmd, CYAN)), ..n1 };
        let col = g.draw(c, x(cx), y(0.22), e.n1);
        value_box(c, &num(e.n1, 1), x(cx), y(0.36), fs, s * 0.15, WHITE, if col == GREEN { GREEN } else { col });
        let col = itt.draw(c, x(cx), y(0.56), e.egt);
        value_box(c, &num(e.egt, 0), x(cx), y(0.67), fs, s * 0.13, WHITE, if col == GREEN { GREEN } else { col });
        c.text_f(&num(e.n2, 1), x(cx), y(0.78), fs, GREEN, Align::Center, Face::Mono);
        c.text_f(&num(e.ff, 0), x(cx), y(0.86), fs, GREEN, Align::Center, Face::Mono);
    }
    if es.len() > 1 {
        c.text_f("N1", x(0.5), y(0.36), fs * 0.85, WHITE, Align::Center, Face::Cond);
        c.text_f("ITT", x(0.5), y(0.67), fs * 0.85, WHITE, Align::Center, Face::Cond);
        c.text_f("N2", x(0.5), y(0.78), fs * 0.85, WHITE, Align::Center, Face::Cond);
        c.text_f("FF KPH", x(0.5), y(0.86), fs * 0.75, WHITE, Align::Center, Face::Cond);
    }
}

fn e190_status(c: &mut Canvas, d: &FlightData, s: f32, ox: f32, oy: f32) {
    let x = |f: f32| ox + f * s;
    let y = |f: f32| oy + f * s;
    let fs = s * 0.04;
    for (i, (t, col)) in messages(d, DisplayStyle::Embraer).iter().take(5).enumerate() {
        c.text_f(t, x(0.05), y(0.05 + i as f32 * 0.05), fs * 0.85, *col, Align::Left, Face::Cond);
    }
    let sep = |c: &mut Canvas, yy: f32| c.line(x(0.02), y(yy), x(0.98), y(yy), GREY, s * 0.003);
    let head = |c: &mut Canvas, t: &str, xx: f32, yy: f32| c.text_f(t, x(xx), y(yy), fs * 0.6, WHITE, Align::Left, Face::Cond);
    // paliwo: lewy, suma w ramce, prawy
    sep(c, 0.31);
    head(c, "FUEL QTY", 0.04, 0.34);
    let (l, _ctr, r) = d.fuel_tanks.unwrap_or((f32::NAN, f32::NAN, f32::NAN));
    c.text_f(&num(l, 0), x(0.18), y(0.40), fs, GREEN, Align::Center, Face::Mono);
    c.text_f(&num(r, 0), x(0.82), y(0.40), fs, GREEN, Align::Center, Face::Mono);
    c.text_f("FQ KG", x(0.5), y(0.385), fs * 0.75, WHITE, Align::Center, Face::Cond);
    value_box(c, &num(d.fuel_kg, 0), x(0.5), y(0.44), fs, s * 0.2, WHITE, GREEN);
    // podwozie: trzy zielone kółka DN
    sep(c, 0.50);
    head(c, "LG POSITION", 0.04, 0.53);
    for (gx, gy) in [(0.5, 0.58), (0.32, 0.64), (0.68, 0.64)] {
        match d.gear_down {
            Some(true) => {
                c.circle(x(gx), y(gy), s * 0.04, GREEN, s * 0.006);
                c.text_f("DN", x(gx), y(gy), fs * 0.8, GREEN, Align::Center, Face::CondBold);
            }
            Some(false) => c.text_f("UP", x(gx), y(gy), fs * 0.8, WHITE, Align::Center, Face::CondBold),
            None => c.text_f("--", x(gx), y(gy), fs * 0.8, AMBER, Align::Center, Face::Mono),
        }
    }
    // sloty i klapy: profil skrzydła i pozycja w ramce; trym wysokości
    sep(c, 0.71);
    c.line(x(0.5), y(0.71), x(0.5), y(0.98), GREY, s * 0.003);
    head(c, "SLAT/FLAP", 0.04, 0.74);
    let (fi, fp, txt) = flap_label(DisplayStyle::Embraer, d);
    let (wx, wy) = (x(0.1), y(0.82));
    c.poly(0, WHITE, s * 0.004, &[wx, wy, wx + s * 0.05, wy - s * 0.02, wx + s * 0.28, wy - s * 0.005, wx + s * 0.3, wy + s * 0.003, wx + s * 0.05, wy + s * 0.012]);
    let f = fi as f32 / (fp - 1).max(1) as f32;
    let (fx, fy) = (wx + s * 0.3, wy + s * 0.003);
    let t = (f * 40.0).to_radians();
    c.line(fx - s * 0.02, fy, fx - s * 0.02 + t.cos() * s * 0.06, fy + t.sin() * s * 0.06, GREEN, s * 0.01);
    value_box(c, &txt, x(0.24), y(0.92), fs, s * 0.12, WHITE, GREEN);
    head(c, "TRIM", 0.54, 0.74);
    c.text_f("PITCH", x(0.76), y(0.80), fs * 0.75, WHITE, Align::Center, Face::Cond);
    let trim = if d.trim_pct.is_nan() { "--".into() } else { format!("{} {:.0}", if d.trim_pct >= 0.0 { "UP" } else { "DN" }, d.trim_pct.abs()) };
    value_box(c, &trim, x(0.76), y(0.88), fs, s * 0.18, WHITE, GREEN);
}

// ---------- ATR 72-600 ----------

fn atr_engines(c: &mut Canvas, d: &FlightData, s: f32, ox: f32, oy: f32) {
    let x = |f: f32| ox + f * s;
    let y = |f: f32| oy + f * s;
    let fs = s * 0.042;
    let es = engines(d);
    let cols: &[f32] = if es.len() == 1 { &[0.5] } else { &[0.25, 0.75] };
    let tq = Gauge { r: s * 0.14, max: 120.0, start: -135.0, sweep: 270.0, amber: 100.0, red: 106.0, bands: &[], ticks: &[(0.0, "0"), (40.0, "4"), (80.0, "8"), (120.0, "12")], needle: GREEN, fill: false, bug: None };
    // ITT: łuk górny, zielony zakres pracy, bursztynowy i czerwony powyżej 800 °C
    let itt = Gauge { r: s * 0.11, max: 1000.0, start: -110.0, sweep: 220.0, amber: 765.0, red: 800.0, bands: &[(0.0, 765.0, 0xFF1F9F3F), (765.0, 800.0, 0xFFB07000), (800.0, 1000.0, 0xFFB01A1A)], ticks: &[], needle: GREEN, fill: false, bug: None };
    for (e, &cx) in es.iter().zip(cols) {
        let col = tq.draw(c, x(cx), y(0.19), e.tq);
        c.text_f(&num(e.tq, 1), x(cx), y(0.19) + s * 0.14 * 0.92, fs * 1.1, if col == GREEN { GREEN } else { col }, Align::Center, Face::Mono);
        let np = if e.np.is_nan() { f32::NAN } else { e.np / 1200.0 * 100.0 };
        c.text_f(&num(np, 0), x(cx), y(0.41), fs * 1.05, GREEN, Align::Center, Face::Mono);
        let col = itt.draw(c, x(cx), y(0.57), e.egt);
        c.text_f(&num(e.egt, 0), x(cx), y(0.62), fs, if col == GREEN { GREEN } else { col }, Align::Center, Face::Mono);
        c.text_f(&num(e.n1, 1), x(cx), y(0.78), fs, GREEN, Align::Center, Face::Mono);
        c.text_f(&num(e.ff, 0), x(cx), y(0.87), fs, GREEN, Align::Center, Face::Mono);
    }
    if es.len() > 1 {
        let lab = |c: &mut Canvas, t: &str, u: &str, yy: f32| {
            c.text_f(t, x(0.5), y(yy - 0.018), fs * 0.8, WHITE, Align::Center, Face::Cond);
            c.text_f(u, x(0.5), y(yy + 0.02), fs * 0.65, CYAN, Align::Center, Face::Cond);
        };
        lab(c, "TQ", "%", 0.19);
        lab(c, "NP", "%", 0.40);
        lab(c, "ITT", "°C", 0.57);
        c.rect(x(0.03), y(0.72), x(0.97), y(0.725), DIM);
        lab(c, "NH", "%", 0.78);
        lab(c, "FF", "KG/H", 0.87);
    }
}

fn atr_status(c: &mut Canvas, d: &FlightData, s: f32, ox: f32, oy: f32) {
    let x = |f: f32| ox + f * s;
    let y = |f: f32| oy + f * s;
    let fs = s * 0.04;
    // trym wysokości: pionowa skala z zielonym zakresem do startu i białym wskaźnikiem
    let (tx, t0, t1) = (x(0.12), y(0.08), y(0.48));
    c.line(tx, t0, tx, t1, WHITE, s * 0.004);
    c.line(tx - s * 0.012, y(0.24), tx - s * 0.012, y(0.34), GREEN, s * 0.012);
    c.text_f("UP", tx + s * 0.03, t0, fs * 0.6, WHITE, Align::Left, Face::Cond);
    c.text_f("DN", tx + s * 0.03, t1, fs * 0.6, WHITE, Align::Left, Face::Cond);
    if !d.trim_pct.is_nan() {
        let ty = t0 + (t1 - t0) * (0.5 - d.trim_pct.clamp(-100.0, 100.0) / 200.0);
        c.poly(WHITE, 0, 0.0, &[tx + s * 0.004, ty, tx + s * 0.03, ty - s * 0.014, tx + s * 0.03, ty + s * 0.014]);
    }
    c.text_f("PITCH", tx, t1 + s * 0.04, fs * 0.65, CYAN, Align::Center, Face::Cond);
    // klapy 0 / 15 / 30 z pozycją
    let (fi, fp, txt) = flap_label(DisplayStyle::Atr, d);
    let (fx, f0, f1) = (x(0.38), y(0.12), y(0.42));
    c.text_f("FLAPS", fx, y(0.06), fs * 0.75, WHITE, Align::Center, Face::Cond);
    c.line(fx, f0, fx, f1, WHITE, s * 0.004);
    for (k, l) in ["0", "15", "30"].iter().enumerate() {
        let yy = f0 + (f1 - f0) * k as f32 / 2.0;
        c.line(fx - s * 0.015, yy, fx + s * 0.015, yy, WHITE, s * 0.004);
        c.text_f(l, fx - s * 0.03, yy, fs * 0.65, WHITE, Align::Right, Face::Cond);
    }
    let fy = f0 + (f1 - f0) * (fi as f32 / (fp - 1).max(1) as f32);
    c.poly(GREEN, 0, 0.0, &[fx + s * 0.006, fy, fx + s * 0.04, fy - s * 0.016, fx + s * 0.04, fy + s * 0.016]);
    c.text_f(&txt, fx + s * 0.06, fy, fs, GREEN, Align::Left, Face::Mono);
    // podwozie i paliwo
    let gear = match d.gear_down {
        Some(true) => ("LDG GEAR DOWN", GREEN),
        Some(false) => ("LDG GEAR UP", WHITE),
        None => ("LDG GEAR --", AMBER),
    };
    c.text_f(gear.0, x(0.62), y(0.12), fs * 0.8, gear.1, Align::Left, Face::Cond);
    c.text_f("FQ", x(0.62), y(0.22), fs * 0.8, WHITE, Align::Left, Face::Cond);
    c.text_f(&num(d.fuel_kg, 0), x(0.92), y(0.22), fs, GREEN, Align::Right, Face::Mono);
    c.text_f("KG", x(0.93), y(0.22), fs * 0.6, CYAN, Align::Left, Face::Cond);
    c.text_f("SAT", x(0.62), y(0.30), fs * 0.8, WHITE, Align::Left, Face::Cond);
    c.text_f(&fmt_temp(d.oat_c).replace('c', " °C"), x(0.97), y(0.30), fs, GREEN, Align::Right, Face::Mono);
    // pole komunikatów jak na EWD
    c.line(x(0.02), y(0.56), x(0.98), y(0.56), GREY, s * 0.004);
    let msgs = messages(d, DisplayStyle::Atr);
    for (i, (t, col)) in msgs.iter().take(7).enumerate() {
        c.text_f(t, x(0.06), y(0.62 + i as f32 * 0.052), fs * 0.85, *col, Align::Left, Face::Cond);
    }
}

/// Parametry silników w kwadracie (lewy ekran).
pub fn draw_primary(c: &mut Canvas, d: &FlightData, style: DisplayStyle, w: f32, h: f32) {
    c.rect(0.0, 0.0, w, h, BLACK);
    let (ox, oy, s) = square(w, h);
    match style {
        DisplayStyle::Boeing => boeing_engines(c, d, s, ox, oy, false),
        DisplayStyle::B787 => boeing_engines(c, d, s, ox, oy, true),
        DisplayStyle::Embraer => e190_engines(c, d, s, ox, oy),
        DisplayStyle::Atr => atr_engines(c, d, s, ox, oy),
        _ => super::ecam::draw_ewd(c, d, w, h),
    }
}

/// Reszta ekranu w kwadracie (prawy ekran): komunikaty, klapy, podwozie, paliwo, trym.
pub fn draw_secondary(c: &mut Canvas, d: &FlightData, style: DisplayStyle, w: f32, h: f32) {
    c.rect(0.0, 0.0, w, h, BLACK);
    let (ox, oy, s) = square(w, h);
    match style {
        DisplayStyle::Boeing => b737_status(c, d, s, ox, oy),
        DisplayStyle::B787 => b787_status(c, d, s, ox, oy),
        DisplayStyle::Embraer => e190_status(c, d, s, ox, oy),
        DisplayStyle::Atr => atr_status(c, d, s, ox, oy),
        _ => super::ecam::draw_sd(c, d, w, h),
    }
}

/// Czy styl ma kwadratowe ekrany silników.
pub fn has_square(style: DisplayStyle) -> bool {
    matches!(style, DisplayStyle::Airbus | DisplayStyle::Boeing | DisplayStyle::B787 | DisplayStyle::Embraer | DisplayStyle::Atr)
}
