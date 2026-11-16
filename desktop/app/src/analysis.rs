//! Analiza lądowania z przebiegu podejścia (`LandingReport::profile`, ~30 s przed przyziemieniem
//! i 6 s po nim, co 200 ms): wykresy prędkości pionowej, przeciążenia, prędkości i przechylenia,
//! wnioski (wysokość nad progiem, wyrównanie, stabilizacja) oraz ślad na tle pasa – z góry i z boku.
//!
//! Ścieżki są w układzie viewbox Slinta: wykresy 1000 × 100, pas z góry 1000 × 200, profil 1000 × 100.

use opentakeoff_core::lang::tr;
use opentakeoff_core::trf;
use opentakeoff_core::logbook::{landing, LandingReport, LandingSample};
use slint::{ModelRc, SharedString, VecModel};

use crate::{AnalysisV, FactV, MiniChartV, RwyViewV};

const T0: f64 = -30_000.0;
const T1: f64 = 6_000.0;
/// Szerokość pasa do rysunku (ft) – w bazie lotnisk nie ma szerokości wszystkich pasów.
const RWY_W: f64 = 150.0;
/// Punkt celowania (ft za progiem) i strefa przyziemienia.
const AIM: f64 = 1_000.0;
const TDZ: (f64, f64) = (500.0, 3_000.0);

fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

fn tx(dt: i32) -> f64 {
    ((dt as f64 - T0) / (T1 - T0) * 1000.0).clamp(0.0, 1000.0)
}

fn path(pts: impl IntoIterator<Item = (f64, f64)>) -> String {
    let mut s = String::new();
    for (i, (x, y)) in pts.into_iter().enumerate() {
        s.push_str(if i == 0 { "M " } else { " L " });
        s.push_str(&format!("{x:.1} {y:.2}"));
    }
    s
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    format!("M {x0:.1} {y0:.2} L {x1:.1} {y0:.2} L {x1:.1} {y1:.2} L {x0:.1} {y1:.2} Z ")
}

fn minus(s: String) -> String {
    s.replace('-', "−")
}

/// Wartość w chwili przyziemienia (najbliższa próbka dt ≥ 0).
fn at_td(p: &[LandingSample]) -> Option<&LandingSample> {
    p.iter().find(|s| s.dt >= 0)
}

/// Próbka, w której samolot ostatni raz był na danej wysokości przed przyziemieniem.
fn at_height(p: &[LandingSample], ft: i32) -> Option<&LandingSample> {
    p.iter().filter(|s| s.dt < 0 && !s.gnd).rev().find(|s| s.agl >= ft)
}

fn chart(p: &[LandingSample], title: &str, unit: &str, tone: i32, v: impl Fn(&LandingSample) -> f64, floor: (f64, f64), fmt_v: impl Fn(f64) -> String) -> MiniChartV {
    let vals: Vec<f64> = p.iter().map(&v).collect();
    let lo = vals.iter().cloned().fold(f64::INFINITY, f64::min).min(floor.0);
    let hi = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max).max(floor.1);
    let span = (hi - lo).max(1e-6);
    let pad = span * 0.08;
    let (lo, hi) = (lo - pad, hi + pad);
    let y = |val: f64| 100.0 - (val - lo) / (hi - lo) * 100.0;
    let line = path(p.iter().zip(&vals).map(|(s, val)| (tx(s.dt), y(*val))));
    let zero = if lo < 0.0 && hi > 0.0 { (y(0.0) / 100.0) as f32 } else { -1.0 };
    // Wartość w chwili kontaktu: ostatnia próbka w powietrzu; przeciążenie – szczyt po przyziemieniu
    let td = if tone == 3 {
        p.iter().filter(|s| (0..=2_000).contains(&s.dt)).map(&v).fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))))
    } else {
        p.iter().filter(|s| s.dt <= 0 && !s.gnd).last().or_else(|| at_td(p)).map(&v)
    };
    MiniChartV {
        title: ss(title),
        unit: ss(unit),
        path: ss(line),
        zero,
        td: (tx(0) / 1000.0) as f32,
        top: ss(minus(fmt_v(hi - pad))),
        bottom: ss(minus(fmt_v(lo + pad))),
        value: ss(td.map(|x| minus(format!("{} {unit}", fmt_v(x)))).unwrap_or_default()),
        tone,
    }
}

fn fact(label: &str, value: String, note: String, tone: i32) -> FactV {
    FactV { label: ss(label), value: ss(minus(value)), note: ss(note), tone }
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

fn stdev(v: &[f64]) -> f64 {
    let m = mean(v);
    (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len().max(1) as f64).sqrt()
}

fn facts(r: &LandingReport, p: &[LandingSample]) -> Vec<FactV> {
    let mut out = Vec::new();
    let td = at_td(p);
    // Wysokość nad progiem (ok. 50 ft przy ścieżce 3°)
    let cross = p.windows(2).find(|w| w[0].x.is_some_and(|x| x < 0) && w[1].x.is_some_and(|x| x >= 0));
    if let Some(w) = cross {
        let (x0, x1) = (w[0].x.unwrap_or(0) as f64, w[1].x.unwrap_or(0) as f64);
        let k = if (x1 - x0).abs() > 0.1 { -x0 / (x1 - x0) } else { 0.0 };
        let h = w[0].agl as f64 + (w[1].agl - w[0].agl) as f64 * k;
        let (note, tone) = match h as i32 {
            ..=0 => (tr("przyziemienie przed progiem").to_string(), 2),
            1..=29 => (tr("nisko – ryzyko przyziemienia przed progiem").to_string(), 1),
            30..=75 => (tr("w normie (ok. 50 ft)").to_string(), 0),
            _ => (tr("wysoko – dłuższe przyziemienie").to_string(), 1),
        };
        out.push(fact(tr("Nad progiem"), format!("{:.0} ft", h.max(0.0)), note, tone));
    }
    // Wyrównanie: prędkość pionowa od 50 ft do przyziemienia
    if let (Some(h50), Some(t)) = (at_height(p, 50), td) {
        let secs = (t.dt - h50.dt) as f64 / 1000.0;
        let cut = h50.vs - r.vs_fpm;
        let (note, tone) = if h50.vs < -400 && cut < 150 {
            (tr("bez wyraźnego wyrównania").to_string(), 1)
        } else if secs > 12.0 {
            (tr("długie wytrzymanie – samolot płynął nad pasem").to_string(), 1)
        } else {
            (trf_secs(secs), 0)
        };
        out.push(fact(tr("Wyrównanie od 50 ft"), format!("{} → {} ft/min", h50.vs, r.vs_fpm), note, tone));
        // Prędkość: spadek od 50 ft
        let loss = h50.ias - t.ias;
        out.push(fact(
            tr("Prędkość 50 ft → przyziemienie"),
            format!("{} → {} kt", h50.ias, t.ias),
            match loss {
                0 => tr("bez zmian").to_string(),
                1.. => format!("−{loss} kt"),
                _ => format!("+{} kt", -loss),
            },
            if (0..=15).contains(&loss) { 0 } else { 1 },
        ));
    }
    // Ostatnie 5 s przed przyziemieniem
    let last5: Vec<f64> = p.iter().filter(|s| (-5_000..0).contains(&s.dt)).map(|s| s.vs as f64).collect();
    if !last5.is_empty() {
        let m = mean(&last5);
        out.push(fact(tr("Średnia V/S w ostatnich 5 s"), format!("{:.0} ft/min", m), String::new(), if m > -400.0 { 0 } else if m > -700.0 { 1 } else { 2 }));
    }
    // Stabilizacja na podejściu (od −30 do −8 s): rozrzut prędkości pionowej i prędkości
    let app: Vec<&LandingSample> = p.iter().filter(|s| s.dt < -8_000 && !s.gnd).collect();
    if app.len() > 10 {
        let vs: Vec<f64> = app.iter().map(|s| s.vs as f64).collect();
        let ias_min = app.iter().map(|s| s.ias).min().unwrap_or(0);
        let ias_max = app.iter().map(|s| s.ias).max().unwrap_or(0);
        let sd = stdev(&vs);
        let ok = sd < 200.0 && ias_max - ias_min <= 12;
        out.push(fact(
            tr("Stabilizacja podejścia"),
            if ok { tr("stabilne").to_string() } else { tr("niestabilne").to_string() },
            format!("V/S ±{:.0} ft/min · IAS {}–{} kt", sd, ias_min, ias_max),
            if ok { 0 } else { 1 },
        ));
    }
    // Przechylenie nisko nad ziemią
    let bank = p.iter().filter(|s| s.dt <= 0 && s.agl <= 200).map(|s| s.bank.abs()).fold(0.0f32, f32::max);
    if p.iter().any(|s| s.dt <= 0 && s.agl <= 200) {
        out.push(fact(tr("Maks. przechylenie poniżej 200 ft"), format!("{bank:.1}°"), String::new(), if bank < 5.0 { 0 } else if bank < 10.0 { 1 } else { 2 }));
    }
    // Przeciążenie po przyziemieniu
    let gmax = p.iter().filter(|s| (0..=2_000).contains(&s.dt)).map(|s| s.g).fold(0.0f32, f32::max);
    if gmax > 0.0 {
        out.push(fact(tr("Szczyt przeciążenia"), format!("{gmax:.2} g"), String::new(), if gmax < 1.4 { 0 } else if gmax < 1.8 { 1 } else { 2 }));
    }
    // Punkt przyziemienia i dobieg
    if let Some(a) = r.along_ft {
        let (note, tone) = if (a as f64) < 0.0 {
            (tr("przed progiem").to_string(), 2)
        } else if (a as f64) < TDZ.0 {
            (tr("krótko, przed strefą przyziemienia").to_string(), 1)
        } else if (a as f64) <= TDZ.1 {
            (tr("w strefie przyziemienia").to_string(), 0)
        } else {
            (tr("za strefą przyziemienia – długie lądowanie").to_string(), 1)
        };
        out.push(fact(tr("Punkt przyziemienia"), format!("{a} ft"), note, tone));
    }
    let roll = p.iter().filter(|s| s.dt > 0).filter_map(|s| s.y).map(|y| y.abs()).max();
    if let Some(y) = roll {
        out.push(fact(tr("Maks. zejście z osi na dobiegu"), format!("{y} ft"), String::new(), if y < 15 { 0 } else if y < 35 { 1 } else { 2 }));
    }
    out
}

fn trf_secs(secs: f64) -> String {
    trf!("{s} s od 50 ft do przyziemienia", s = format!("{secs:.1}"))
}

/// Widok pasa z góry (oś poprzeczna powiększona) i z boku (wysokość nad pasem i ścieżka 3°).
fn runway(r: &LandingReport, p: &[LandingSample]) -> RwyViewV {
    let xs: Vec<f64> = p.iter().filter_map(|s| s.x).map(|x| x as f64).collect();
    if xs.len() < 5 {
        return RwyViewV::default();
    }
    let len = r.runway_length_ft.map(|l| l as f64).unwrap_or(8_000.0).max(3_000.0);
    let xmin = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let xmax = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let x0 = xmin.max(-3_500.0).min(-800.0);
    let x1 = (xmax + 600.0).max(TDZ.1 + 500.0).min(len + 300.0).max(x0 + 2_000.0);
    let fx = |x: f64| ((x - x0) / (x1 - x0) * 1000.0).clamp(-5.0, 1005.0);
    // Z góry: ±half ft w pionie (200 jednostek)
    let ymax = p.iter().filter(|s| s.x.is_some_and(|x| x as f64 >= x0)).filter_map(|s| s.y).map(|y| y.abs() as f64).fold(0.0, f64::max);
    let half = (ymax * 1.25).max(RWY_W * 1.4);
    let fy = |y: f64| 100.0 + (y / half * 100.0).clamp(-100.0, 100.0);
    let mut paint = rect(fx(0.0), fy(-RWY_W / 2.0), fx(len), fy(RWY_W / 2.0));
    // Oznakowanie: próg (pasy), linia centralna, punkt celowania i strefa przyziemienia
    let mut marks = String::new();
    for i in 0..8 {
        let off = -RWY_W / 2.0 + 8.0 + i as f64 * (RWY_W - 16.0) / 7.0;
        marks.push_str(&rect(fx(20.0), fy(off - 3.0), fx(170.0), fy(off + 3.0)));
    }
    let mut x = 250.0;
    while x + 120.0 < len.min(x1) {
        marks.push_str(&rect(fx(x), fy(-1.5), fx(x + 120.0), fy(1.5)));
        x += 200.0;
    }
    for side in [-1.0, 1.0] {
        marks.push_str(&rect(fx(AIM), fy(side * 22.0 - side * 9.0), fx(AIM + 150.0), fy(side * 22.0 + side * 9.0)));
        for t in [500.0, 1_500.0, 2_000.0, 2_500.0, 3_000.0] {
            if t < len {
                marks.push_str(&rect(fx(t), fy(side * 40.0 - side * 4.0), fx(t + 75.0), fy(side * 40.0 + side * 4.0)));
            }
        }
    }
    paint.push_str(" ");
    let vis = |s: &&LandingSample| s.x.is_some_and(|x| x as f64 >= x0 - 200.0);
    let air = path(p.iter().filter(vis).filter(|s| s.dt <= 0).map(|s| (fx(s.x.unwrap_or(0) as f64), fy(s.y.unwrap_or(0) as f64))));
    let ground = path(p.iter().filter(vis).filter(|s| s.dt >= 0).map(|s| (fx(s.x.unwrap_or(0) as f64), fy(s.y.unwrap_or(0) as f64))));
    // Z boku: wysokość nad pasem 0…hmax, ścieżka 3° do punktu celowania
    let hmax = p.iter().filter(|s| s.x.is_some_and(|x| x as f64 >= x0)).map(|s| s.agl as f64).fold(0.0, f64::max).max(120.0) * 1.1;
    let fh = |h: f64| 92.0 - (h / hmax * 86.0).clamp(-6.0, 92.0);
    let tan3 = 3f64.to_radians().tan();
    let gp_start = x0;
    let glide = path([(fx(gp_start), fh((AIM - gp_start) * tan3)), (fx(AIM), fh(0.0))]);
    let side = path(p.iter().filter(vis).map(|s| (fx(s.x.unwrap_or(0) as f64), fh(s.agl.max(0) as f64))));
    let tdp = p.iter().find(|s| s.dt >= 0);
    let (tdx, tdy) = tdp.map(|s| (fx(s.x.unwrap_or(0) as f64) / 1000.0, fy(s.y.unwrap_or(0) as f64) / 200.0)).unwrap_or((-1.0, -1.0));
    let h50 = fh(50.0) / 100.0;
    RwyViewV {
        has: true,
        runway: ss(paint),
        marks: ss(marks),
        air: ss(air),
        ground: ss(ground),
        td_x: tdx as f32,
        td_y: tdy as f32,
        side: ss(side),
        glide: ss(glide),
        ground_line: (fh(0.0) / 100.0) as f32,
        line_50: h50 as f32,
        threshold_x: (fx(0.0) / 1000.0) as f32,
        aim_x: (fx(AIM) / 1000.0) as f32,
        title: ss(r.runway.as_ref().map(|x| format!("RWY {x}{}", r.airport.as_ref().map(|a| format!(" · {a}")).unwrap_or_default())).unwrap_or_default()),
        scale_note: ss(trf!("Skala poprzeczna powiększona: ±{f} ft", f = half.round() as i32)),
        hmax: ss(format!("{:.0} ft", hmax)),
    }
}

/// Pełna analiza jednego lądowania (pusta, gdy lot nie ma zapisanego przebiegu – np. z telefonu).
pub fn build(r: &LandingReport, title: String) -> AnalysisV {
    let p = &r.profile;
    if p.len() < 10 {
        return AnalysisV { has: false, title: ss(title), ..Default::default() };
    }
    let charts = vec![
        chart(p, tr("Prędkość pionowa"), "ft/min", 0, |s| s.vs as f64, (-200.0, 0.0), |v| format!("{v:.0}")),
        chart(p, tr("Wysokość nad ziemią"), "ft", 1, |s| s.agl.max(0) as f64, (0.0, 50.0), |v| format!("{v:.0}")),
        chart(p, tr("Prędkość przyrządowa"), "kt", 2, |s| s.ias as f64, (f64::INFINITY, f64::NEG_INFINITY), |v| format!("{v:.0}")),
        chart(p, tr("Przeciążenie"), "g", 3, |s| s.g as f64, (0.9, 1.2), |v| format!("{v:.2}")),
        chart(p, tr("Przechylenie"), "°", 4, |s| s.bank as f64, (-3.0, 3.0), |v| format!("{v:.1}")),
        chart(p, tr("Pochylenie"), "°", 5, |s| s.pitch as f64, (0.0, 5.0), |v| format!("{v:.1}")),
    ];
    let fs = facts(r, p);
    AnalysisV {
        has: true,
        title: ss(title),
        summary: ss(format!("{} · {} · {}", r.score, landing::grade_of(r.score), minus(format!("{} ft/min", r.vs_fpm)))),
        score: r.score,
        charts: model(charts),
        facts_a: model(fs.iter().step_by(2).cloned().collect()),
        facts_b: model(fs.iter().skip(1).step_by(2).cloned().collect()),
        rwy: runway(r, p),
    }
}
