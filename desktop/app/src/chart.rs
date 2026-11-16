//! Profil lotu jako ścieżki SVG dla Slinta (układ 1000 × 100) i sztuczny horyzont dla karty Na żywo.

use opentakeoff_core::logbook::{FlightLog, TrackPoint};

pub struct Label {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub kind: i32,
}

#[derive(Default)]
pub struct Chart {
    pub alt: String,
    pub spd: String,
    pub gs: String,
    pub vs: String,
    pub ground: String,
    pub grid: String,
    pub y_labels: Vec<Label>,
    pub y2_labels: Vec<Label>,
    pub x_labels: Vec<Label>,
    pub markers: Vec<Label>,
    pub has_ias: bool,
    pub has_vs: bool,
}

fn nice_max(v: f64) -> f64 {
    let v = v.max(1.0);
    let mag = 10f64.powf(v.log10().floor());
    for m in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if m * mag >= v {
            return m * mag;
        }
    }
    10.0 * mag
}

fn path(points: impl Iterator<Item = (f64, f64)>) -> String {
    let mut s = String::new();
    for (i, (x, y)) in points.enumerate() {
        s.push_str(if i == 0 { "M " } else { " L " });
        s.push_str(&format!("{x:.1} {y:.2}"));
    }
    s
}

fn hm(ms: i64) -> String {
    let m = ms / 60_000;
    format!("{}:{:02}", m / 60, m % 60)
}

/// `alt_m` – wysokość w metrach zamiast stóp; `kmh` – prędkość w km/h.
pub fn build(f: &FlightLog, alt_m: bool, kmh: bool) -> Chart {
    let t = &f.track;
    if t.len() < 2 {
        return Chart::default();
    }
    let step = (t.len() / 1500).max(1);
    let pts: Vec<&TrackPoint> = t.iter().step_by(step).chain(t.last()).collect();
    let (t0, t1) = (t[0].t, t[t.len() - 1].t.max(t[0].t + 1));
    let x = |p: &TrackPoint| (p.t - t0) as f64 / (t1 - t0) as f64 * 1000.0;
    let alt_k = if alt_m { 0.3048 } else { 1.0 };
    let spd_k = if kmh { 1.852 } else { 1.0 };
    let max_alt = nice_max(pts.iter().map(|p| p.alt_ft as f64 * alt_k).fold(0.0, f64::max) * 1.08);
    let y_alt = |a: f64| 100.0 - (a * alt_k / max_alt * 100.0).clamp(0.0, 100.0);
    let mut c = Chart::default();
    let mut area: Vec<(f64, f64)> = vec![(0.0, 100.0)];
    area.extend(pts.iter().map(|p| (x(p), y_alt(p.alt_ft as f64))));
    area.push((1000.0, 100.0));
    c.alt = path(area.into_iter()) + " Z";
    if pts.iter().any(|p| p.agl_ft.is_some()) {
        let mut g: Vec<(f64, f64)> = vec![(0.0, 100.0)];
        g.extend(pts.iter().map(|p| (x(p), y_alt((p.alt_ft - p.agl_ft.unwrap_or(p.alt_ft)).max(0) as f64))));
        g.push((1000.0, 100.0));
        c.ground = path(g.into_iter()) + " Z";
    }
    let max_spd = nice_max(pts.iter().map(|p| p.gs_kt.max(p.ias_kt.unwrap_or(0)) as f64 * spd_k).fold(0.0, f64::max) * 1.08);
    let y_spd = |v: f64| 100.0 - (v * spd_k / max_spd * 100.0).clamp(0.0, 100.0);
    c.gs = path(pts.iter().map(|p| (x(p), y_spd(p.gs_kt as f64))));
    c.has_ias = pts.iter().any(|p| p.ias_kt.is_some());
    if c.has_ias {
        c.spd = path(pts.iter().map(|p| (x(p), y_spd(p.ias_kt.unwrap_or(0) as f64))));
    }
    let max_vs = nice_max(pts.iter().filter_map(|p| p.vs_fpm).map(|v| (v as f64).abs()).fold(0.0, f64::max) * 1.1);
    c.has_vs = pts.iter().any(|p| p.vs_fpm.is_some());
    if c.has_vs {
        c.vs = path(pts.iter().map(|p| (x(p), 50.0 - (p.vs_fpm.unwrap_or(0) as f64 / max_vs * 50.0).clamp(-50.0, 50.0))));
    }
    c.grid = "M 0 0 L 1000 0 M 0 25 L 1000 25 M 0 50 L 1000 50 M 0 75 L 1000 75 M 0 100 L 1000 100".into();
    let unit_alt = if alt_m { "m" } else { "ft" };
    let unit_spd = if kmh { "km/h" } else { "kt" };
    for i in 0..=4 {
        let frac = i as f64 / 4.0;
        c.y_labels.push(Label { x: 0.0, y: frac as f32, text: format!("{:.0}{}", max_alt * (1.0 - frac), if i == 0 { format!(" {unit_alt}") } else { String::new() }), kind: 0 });
        c.y2_labels.push(Label { x: 0.0, y: frac as f32, text: format!("{:.0}{}", max_spd * (1.0 - frac), if i == 0 { format!(" {unit_spd}") } else { String::new() }), kind: 0 });
        c.x_labels.push(Label { x: frac as f32, y: 0.0, text: hm(((t1 - t0) as f64 * frac) as i64), kind: 0 });
    }
    for l in f.all_landings() {
        if let Some(at) = l.at {
            if at >= t0 && at <= t1 {
                c.markers.push(Label { x: ((at - t0) as f64 / (t1 - t0) as f64) as f32, y: 0.0, text: format!("{} · {} ft/min", l.score, l.vs_fpm), kind: l.score });
            }
        }
    }
    c
}

/// Etykiety osi pionowej dla trybu prędkości pionowej (−max … +max).
pub fn vs_labels(f: &FlightLog) -> Vec<Label> {
    let max_vs = nice_max(f.track.iter().filter_map(|p| p.vs_fpm).map(|v| (v as f64).abs()).fold(0.0, f64::max) * 1.1);
    (0..=4).map(|i| Label { x: 0.0, y: i as f32 / 4.0, text: format!("{:+.0}", max_vs * (1.0 - i as f64 / 2.0)), kind: 0 }).collect()
}

