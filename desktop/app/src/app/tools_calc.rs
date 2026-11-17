//! Karta Narzędzia: obliczenia kalkulatorów lotniczych (App.calc).
//! Część modułu `app` – wspólne pomocnicze funkcje i importy z `super`.

use super::*;

// ---------- kalkulatory ----------

// ---------- narzędzia: obliczenia pomocnicze ----------

/// Atmosfera wzorcowa: (temperatura °C, ciśnienie hPa, gęstość względna, prędkość dźwięku kt).
pub(super) fn isa(alt_ft: f64) -> (f64, f64, f64, f64) {
    let h = alt_ft.max(-2000.0);
    let t = isa_temp(h);
    let p = if h <= 36_089.0 { 1013.25 * (1.0 - 6.875_6e-6 * h).powf(5.2559) } else { 226.32 * (-(h - 36_089.0) / 20_806.0).exp() };
    let tk = t + 273.15;
    (t, p, (p / 1013.25) / (tk / 288.15), 661.47 * (tk / 288.15).sqrt())
}

pub(super) fn isa_temp(alt_ft: f64) -> f64 {
    (15.0 - 1.98 * alt_ft / 1000.0).max(-56.5)
}

/// Ścieżki trójkąta wiatru w polu 100×100 (północ u góry): wektor kursu z TAS, wiatru i drogi.
pub(super) fn triangle_paths(hdg: f64, tas: f64, wind_dir: f64, wind_kt: f64) -> (String, String, String) {
    let v = |deg: f64, len: f64| (len * deg.to_radians().sin(), len * deg.to_radians().cos());
    let a = v(hdg, tas);
    let w = v(wind_dir + 180.0, wind_kt);
    let b = (a.0 + w.0, a.1 + w.1);
    let pts = [(0.0, 0.0), a, b];
    let (minx, maxx) = pts.iter().fold((f64::MAX, f64::MIN), |m, p| (m.0.min(p.0), m.1.max(p.0)));
    let (miny, maxy) = pts.iter().fold((f64::MAX, f64::MIN), |m, p| (m.0.min(p.1), m.1.max(p.1)));
    let span = (maxx - minx).max(maxy - miny).max(1.0);
    let k = 76.0 / span;
    let (cx, cy) = ((minx + maxx) / 2.0, (miny + maxy) / 2.0);
    let map = |p: (f64, f64)| (50.0 + (p.0 - cx) * k, 50.0 - (p.1 - cy) * k);
    let (o, a, b) = (map((0.0, 0.0)), map(a), map(b));
    (arrow(o, a), arrow(a, b), arrow(o, b))
}

/// Odcinek ze strzałką na końcu (polecenia ścieżki SVG).
pub(super) fn arrow(p0: (f64, f64), p1: (f64, f64)) -> String {
    let (dx, dy) = (p1.0 - p0.0, p1.1 - p0.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.5 {
        return String::new();
    }
    let (ux, uy) = (dx / len, dy / len);
    let h = 5.0f64.min(len * 0.4);
    let l = (p1.0 - h * ux + h * 0.5 * uy, p1.1 - h * uy - h * 0.5 * ux);
    let r = (p1.0 - h * ux - h * 0.5 * uy, p1.1 - h * uy + h * 0.5 * ux);
    format!("M {:.2} {:.2} L {:.2} {:.2} M {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2}", p0.0, p0.1, p1.0, p1.1, l.0, l.1, p1.0, p1.1, r.0, r.1)
}

/// Wiatr na wszystkich pasach lotniska: wiersze (pas, kurs, czołowy/tylny, boczny, najlepszy).
/// Pierwszy element: nazwa lotniska albo pusty, gdy nie znaleziono.
pub(super) fn airport_winds(icao: &str, dir: Option<f64>, spd: Option<f64>) -> Vec<String> {
    let Some(db) = airports::db() else { return vec![tr("Baza lotnisk jeszcze się wczytuje").to_string()] };
    let Some(a) = db.get(&icao.trim().to_uppercase()) else { return vec![String::new()] };
    let (dir, spd) = (dir.unwrap_or(0.0), spd.unwrap_or(0.0));
    let mut ends: Vec<(String, f64, f64, f64, i32)> = Vec::new();
    for r in a.runways.iter().filter(|r| !r.closed) {
        for (id, hdg) in [(&r.le, r.le_heading()), (&r.he, r.he_heading())] {
            let hdg = hdg.or_else(|| id.trim_start_matches('0').chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<f64>().ok().map(|x| x * 10.0));
            if let Some(h) = hdg {
                let (hw, xw) = calc::wind_components(dir, spd, h);
                ends.push((id.clone(), h, hw, xw, r.length_ft));
            }
        }
    }
    let best = ends.iter().enumerate().max_by(|x, y| (x.1 .2 - x.1 .3.abs() * 0.5).partial_cmp(&(y.1 .2 - y.1 .3.abs() * 0.5)).unwrap_or(std::cmp::Ordering::Equal)).map(|x| x.0);
    let mut out = vec![format!("{} – {}", a.ident, a.name)];
    for (i, e) in ends.iter().enumerate() {
        out.push(e.0.clone());
        out.push(format!("{:03.0}°", e.1));
        out.push(format!("{} {:.0}", if e.2 >= 0.0 { "▲" } else { "▼" }, e.2.abs()));
        out.push(format!("{:.0} {}", e.3.abs(), if e.3 >= 0.0 { "→" } else { "←" }));
        out.push(if e.4 > 0 { format!("{} ft", e.4) } else { String::new() });
        out.push(if Some(i) == best { "1".into() } else if e.2 < -0.5 { "-1".into() } else { String::new() });
    }
    out
}

/// Trasa po ortodromie między lotniskami: [ok, nazwa A, nazwa B, dystans, kurs początkowy, kurs końcowy,
/// czas przy TAS, elewacja A, elewacja B, różnica stref ±, paliwo] albo [pusty, komunikat].
pub(super) fn route_calc(a: &str, b: &str, tas: Option<f64>, burn: Option<f64>) -> Vec<String> {
    let Some(db) = airports::db() else { return vec![String::new(), tr("Baza lotnisk jeszcze się wczytuje").to_string()] };
    let (Some(x), Some(y)) = (db.get(&a.trim().to_uppercase()), db.get(&b.trim().to_uppercase())) else {
        return vec![String::new(), tr("Podaj dwa kody ICAO lotnisk, np. EPWA i EGLL").to_string()];
    };
    let d = math::distance_nm(x.lat, x.lon, y.lat, y.lon);
    let c1 = math::bearing_deg(x.lat, x.lon, y.lat, y.lon).rem_euclid(360.0);
    let c2 = (math::bearing_deg(y.lat, y.lon, x.lat, x.lon) + 180.0).rem_euclid(360.0);
    let min = tas.filter(|t| *t > 1.0).map(|t| d / t * 60.0);
    let elev = |e: Option<i32>| e.map(|e| format!("{e} ft")).unwrap_or_default();
    vec![
        "1".into(),
        format!("{} · {}", x.ident, x.name),
        format!("{} · {}", y.ident, y.name),
        format!("{d:.0} NM"),
        format!("{c1:03.0}°"),
        format!("{c2:03.0}°"),
        min.map(|m| fmt::duration_min(m.round() as i64)).unwrap_or_default(),
        elev(x.elev_ft),
        elev(y.elev_ft),
        format!("{:.0} km", d * 1.852),
        match (min, burn.filter(|b| *b > 0.0)) {
            (Some(m), Some(b)) => format!("{:.0} kg", m / 60.0 * b),
            _ => String::new(),
        },
        if c1 < 180.0 { tr("nieparzyste (na wschód)").to_string() } else { tr("parzyste (na zachód)").to_string() },
    ]
}

pub(super) fn parse_hm(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    match s.split_once(':') {
        Some((h, m)) => Some(h.trim().parse::<f64>().ok()? * 60.0 + m.trim().parse::<f64>().ok()?),
        None => s.replace(',', ".").parse().ok(),
    }
}

/// Czas–prędkość–dystans: liczy brakującą wartość. [dystans, prędkość, czas, która policzona 0/1/2].
pub(super) fn tsd(d: Option<f64>, s: Option<f64>, t: Option<f64>) -> Vec<String> {
    let hm = |m: f64| format!("{}:{:02}", (m / 60.0).floor() as i64, (m.rem_euclid(60.0)).round() as i64);
    match (d, s, t) {
        (Some(d), Some(s), _) if s > 0.0 => vec![format!("{d:.0} NM"), format!("{s:.0} kt"), hm(d / s * 60.0), "2".into()],
        (Some(d), None, Some(t)) if t > 0.0 => vec![format!("{d:.0} NM"), format!("{:.0} kt", d / (t / 60.0)), hm(t), "1".into()],
        (None, Some(s), Some(t)) => vec![format!("{:.1} NM", s * t / 60.0), format!("{s:.0} kt"), hm(t), "0".into()],
        _ => vec![String::new(), String::new(), String::new(), String::new()],
    }
}

/// Alfabet ICAO: pary (znak, słowo).
pub(super) fn phonetic(text: &str) -> Vec<String> {
    const W: [&str; 26] = ["ALFA", "BRAVO", "CHARLIE", "DELTA", "ECHO", "FOXTROT", "GOLF", "HOTEL", "INDIA", "JULIETT", "KILO", "LIMA", "MIKE", "NOVEMBER", "OSCAR", "PAPA", "QUEBEC", "ROMEO", "SIERRA", "TANGO", "UNIFORM", "VICTOR", "WHISKEY", "X-RAY", "YANKEE", "ZULU"];
    const D: [&str; 10] = ["ZERO", "ONE", "TWO", "TREE", "FOWER", "FIFE", "SIX", "SEVEN", "AIT", "NINER"];
    let mut out = Vec::new();
    for c in text.to_uppercase().chars().take(40) {
        let w = match c {
            'A'..='Z' => W[(c as u8 - b'A') as usize],
            '0'..='9' => D[(c as u8 - b'0') as usize],
            '.' => "DECIMAL",
            ' ' => continue,
            _ => continue,
        };
        out.push(c.to_string());
        out.push(w.to_string());
    }
    out
}

/// Bieżące wartości z symulatora do narzędzi: [kierunek wiatru, prędkość wiatru, kurs magn., TAS, GS,
/// wysokość, OAT, QNH, droga, najbliższe lotnisko, elewacja najbliższego, punkt rosy (brak)].
pub(super) fn live_numbers() -> Vec<String> {
    let Some(snap) = state().borrow().host.snapshot() else { return Vec::new() };
    let near = airports::db().and_then(|d| d.nearest(snap.position.lat, snap.position.lon).map(|a| (a.ident.clone(), a.elev_ft)));
    vec![
        format!("{:.0}", snap.environment.wind_dir_deg),
        format!("{:.0}", snap.environment.wind_spd_kt),
        format!("{:.0}", snap.attitude.hdg_mag_deg.rem_euclid(360.0)),
        format!("{:.0}", snap.speed.tas_kt),
        format!("{:.0}", snap.speed.gs_kt),
        format!("{:.0}", snap.position.alt_ind_ft),
        format!("{:.0}", snap.environment.oat_c),
        format!("{:.0}", snap.environment.qnh_hpa),
        format!("{:.0}", snap.attitude.track_true_deg.rem_euclid(360.0)),
        near.as_ref().map(|n| n.0.clone()).unwrap_or_default(),
        near.and_then(|n| n.1).map(|e| e.to_string()).unwrap_or_default(),
        format!("{:.0}", snap.fuel.total_kg),
        format!("{:.0}", snap.engines.iter().map(|e| e.fuel_flow_kg_h).sum::<f64>()),
    ]
}

pub fn calc(kind: &str, input: &[String]) -> Vec<String> {
    let n = |i: usize| input.get(i).and_then(|s| s.trim().replace(',', ".").parse::<f64>().ok());
    let all = |k: usize| (0..k).map(n).collect::<Option<Vec<f64>>>();
    match kind {
        "runway-wind" => match all(3) {
            Some(v) => {
                let (h, c) = calc::wind_components(v[0], v[1], v[2]);
                vec![format!("{} {:.0} kt", if h >= 0.0 { tr("czołowy") } else { tr("tylny") }, h.abs()), format!("{:.0} kt {}", c.abs(), if c >= 0.0 { tr("z prawej") } else { tr("z lewej") })]
            }
            None => vec![String::new(); 2],
        },
        "wind-triangle" => match all(4) {
            Some(v) => {
                let (wca, gs, hdg) = calc::wind_triangle(v[0], v[1], v[2], v[3]);
                let (ph, pw, pg) = triangle_paths(hdg, v[1], v[2], v[3]);
                let drift = (v[0] - hdg + 540.0).rem_euclid(360.0) - 180.0;
                vec![format!("{hdg:03.0}°"), format!("{wca:+.0}°"), format!("{gs:.0} kt"), ph, pw, pg, format!("{:.0} kt", gs - v[1]), format!("{drift:+.0}°")]
            }
            None => vec![String::new(); 8],
        },
        "density" => match all(4) {
            Some(v) => {
                let pa = calc::pressure_altitude(v[0], v[1]);
                let da = calc::density_altitude(v[0], v[1], v[2]);
                let tas = calc::tas_from_ias(v[3], da);
                let isa = isa_temp(pa);
                vec![format!("{pa:.0} ft"), format!("{da:.0} ft"), format!("{tas:.0} kt"), format!("{isa:.0} °C"), format!("ISA {:+.0} °C", v[2] - isa), format!("M {:.2}", calc::mach(tas, v[2]))]
            }
            None => vec![String::new(); 6],
        },
        "descent" => match all(4) {
            Some(v) => {
                let (nm, vs) = calc::descent(v[0], v[1], v[2], v[3]);
                let min = nm / v[2].max(1.0) * 60.0;
                let mut out = vec![format!("{nm:.0} NM"), format!("{vs:.0} ft/min"), format!("{min:.0} min"), format!("{:.0}", (v[0] - v[1]).max(0.0) / 1000.0 * 3.0)];
                // Tabela: odległość do końca zniżania → wysokość na ścieżce
                for k in (0..=8).rev() {
                    let d = nm * k as f64 / 8.0;
                    out.push(format!("{d:.0} NM"));
                    out.push(format!("{:.0} ft", v[1] + d * 6076.12 * v[3].to_radians().tan()));
                }
                out
            }
            None => vec![String::new(); 22],
        },
        "fuel" => match all(4) {
            Some(v) => {
                let (min, trip, total) = calc::fuel(v[0], v[1], v[2], v[3]);
                let mut out = vec![fmt::duration_min(min.round() as i64), format!("{trip:.0} kg"), format!("{total:.0} kg")];
                // Paliwo na pokładzie (opcjonalne): zasięg w czasie i zapas na celu
                match n(4).filter(|f| *f > 0.0) {
                    Some(fob) => {
                        let endur = fob / v[2].max(1.0) * 60.0;
                        out.push(fmt::duration_min(endur.round() as i64));
                        out.push(format!("{:.0} kg", fob - trip));
                        out.push(if fob >= total { "1".into() } else { String::new() });
                        out.push(format!("{:.0} NM", endur / 60.0 * v[1]));
                    }
                    None => out.extend([String::new(), String::new(), String::new(), String::new()]),
                }
                out
            }
            None => vec![String::new(); 7],
        },
        "cloudbase" => match all(2) {
            Some(v) => {
                let rh = calc::relative_humidity(v[0], v[1]);
                let spread = v[0] - v[1];
                let fz = if v[0] > 0.0 { format!("{:.0} ft AGL", v[0] / 1.98 * 1000.0) } else { tr("przy ziemi").to_string() };
                vec![format!("{:.0} ft AGL", calc::cloud_base_ft(v[0], v[1])), format!("{rh:.0} %"), (if spread <= 2.0 { tr("duże") } else if spread <= 4.0 { tr("umiarkowane") } else { tr("małe") }).to_string(), fz, format!("{spread:.0} °C"), format!("{:.0} °F", calc::celsius_to_f(v[0]))]
            }
            None => vec![String::new(); 6],
        },
        "unit-names" => {
            let g = n(0).unwrap_or(0.0) as usize;
            calc::UNITS.get(g).map(|u| u.1.iter().map(|x| x.0.to_string()).collect()).unwrap_or_default()
        }
        "convert" => match all(4) {
            Some(v) => {
                let r = calc::convert(v[0], v[1] as usize, v[2] as usize, v[3] as usize);
                let unit = calc::UNITS.get(v[1] as usize).and_then(|u| u.1.get(v[3] as usize)).map(|u| u.0).unwrap_or("");
                vec![format!("{} {unit}", if r.abs() >= 100.0 { format!("{r:.1}") } else { format!("{r:.4}").trim_end_matches('0').trim_end_matches('.').to_string() })]
            }
            None => vec![String::new()],
        },
        "airport-winds" => airport_winds(input.first().map(|s| s.as_str()).unwrap_or(""), n(1), n(2)),
        "route" => route_calc(input.first().map(|s| s.as_str()).unwrap_or(""), input.get(1).map(|s| s.as_str()).unwrap_or(""), n(2), n(3)),
        "turn" => match all(2) {
            Some(v) => {
                let ms = v[0] * 0.514_444;
                let g = 9.806_65;
                let rate1 = (ms * 3f64.to_radians() / g).atan().to_degrees();
                let bank = v[1].clamp(1.0, 75.0).to_radians();
                let r_m = ms * ms / (g * bank.tan());
                let rate = (g * bank.tan() / ms.max(1.0)).to_degrees();
                let n_load = 1.0 / bank.cos();
                vec![
                    format!("{rate1:.0}°"),
                    format!("{:.2} NM", r_m / 1852.0),
                    format!("{rate:.1}°/s"),
                    format!("{:.0} s", 360.0 / rate.max(0.01)),
                    format!("{n_load:.2} G"),
                    format!("+{:.0} %", (n_load.sqrt() - 1.0) * 100.0),
                    format!("{:.1} NM", r_m / 1852.0),
                    format!("{:.0} m", r_m),
                ]
            }
            None => vec![String::new(); 8],
        },
        "tsd" => tsd(n(0), input.get(1).and_then(|s| s.trim().replace(',', ".").parse::<f64>().ok()), input.get(2).and_then(|s| parse_hm(s))),
        "approach" => match all(3) {
            Some(v) => {
                let t = v[0].to_radians().tan();
                let mut out = vec![format!("{:.0} ft/min", v[1] * 6076.12 / 60.0 * t), format!("{:.0} ft/NM", 6076.12 * t), format!("{:.1} %", t * 100.0)];
                for gs in [90.0, 110.0, 130.0, 140.0, 150.0, 160.0, 180.0] {
                    out.push(format!("{gs:.0} kt"));
                    out.push(format!("{:.0}", gs * 6076.12 / 60.0 * t));
                }
                for d in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0] {
                    out.push(format!("{d:.0} NM"));
                    out.push(format!("{:.0} ft", v[2] + d * 6076.12 * t));
                }
                out
            }
            None => vec![String::new(); 35],
        },
        "climb" => match all(2) {
            Some(v) => {
                let ftnm = 6076.12 * v[1] / 100.0;
                let mut out = vec![format!("{:.0} ft/min", v[0] / 60.0 * ftnm), format!("{ftnm:.0} ft/NM"), format!("{:.1}°", (v[1] / 100.0).atan().to_degrees())];
                for gs in [100.0, 140.0, 180.0, 220.0, 260.0, 300.0] {
                    out.push(format!("{gs:.0} kt"));
                    out.push(format!("{:.0}", gs / 60.0 * ftnm));
                }
                out
            }
            None => vec![String::new(); 15],
        },
        "isa" => {
            let alt = n(0).unwrap_or(0.0);
            let (t, p, sigma, a) = isa(alt);
            let mut out = vec![format!("{t:.1} °C"), format!("{p:.1} hPa"), format!("{sigma:.3}"), format!("{a:.0} kt"), format!("{:.2} inHg", p / 33.8639)];
            for h in (0..=45_000).step_by(5_000) {
                let (t, p, sigma, a) = isa(h as f64);
                out.extend([if h == 0 { "MSL".to_string() } else if h >= 10_000 { format!("FL{:03}", h / 100) } else { format!("{h} ft") }, format!("{t:.1}"), format!("{p:.0}"), format!("{sigma:.3}"), format!("{a:.0}"), if (alt - h as f64).abs() < 2_500.0 { "1".into() } else { String::new() }]);
            }
            out
        }
        "units-all" => match all(3) {
            Some(v) => {
                let g = v[1] as usize;
                calc::UNITS
                    .get(g)
                    .map(|u| {
                        u.1.iter()
                            .enumerate()
                            .flat_map(|(i, x)| {
                                let r = calc::convert(v[0], g, v[2] as usize, i);
                                [x.0.to_string(), if r.abs() >= 100.0 { format!("{r:.1}") } else { format!("{r:.4}").trim_end_matches('0').trim_end_matches('.').to_string() }]
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }
            None => Vec::new(),
        },
        "phonetic" => phonetic(input.first().map(|s| s.as_str()).unwrap_or("")),
        "live" => live_numbers(),
        // Osiągi: start i lądowanie (crate::perf)
        "perf-to" => crate::perf::takeoff(input),
        "perf-ldg" => crate::perf::landing(input),
        "perf-names" => crate::perf::names(),
        "perf-info" => crate::perf::info(n(0).unwrap_or(0.0) as usize),
        "perf-ofp-to" => crate::simbrief::perf_inputs(true),
        "perf-ofp-ldg" => crate::simbrief::perf_inputs(false),
        "perf-detect" => state().borrow().host.snapshot().and_then(|s| crate::perf::detect(&s)).map(|i| vec![i.to_string()]).unwrap_or_default(),
        "decode" => metar_dec::decode(input.first().map(|s| s.as_str()).unwrap_or("")).into_iter().filter(|g| !g.meaning.is_empty()).flat_map(|g| [g.token, g.meaning]).collect(),
        _ => Vec::new(),
    }
}
