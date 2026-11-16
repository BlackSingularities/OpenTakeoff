//! Ocena lądowania 0–100 i położenie przyziemienia względem pasa – te same progi co w aplikacji na telefon:
//! miękkie przyziemienie 60–240 ft/min, twarde od 400, ciężkie od 600 ft/min lub 2 g; strefa przyziemienia
//! 500–3000 ft za progiem.

use super::model::LandingReport;
use crate::lang::tr;
use crate::trf;

/// Geometria pasa: próg, kurs rzeczywisty i długość.
#[derive(Clone, Debug, PartialEq)]
pub struct RunwayGeom {
    pub ident: String,
    pub threshold_lat: f64,
    pub threshold_lon: f64,
    pub heading_true_deg: f64,
    pub length_ft: Option<f64>,
}

/// Położenie względem progu: wzdłuż osi (ft, + = za progiem) i w poprzek (ft, + = w prawo).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunwayOffset {
    pub along_ft: f64,
    pub cross_ft: f64,
}

pub fn offset(rw: &RunwayGeom, lat: f64, lon: f64) -> RunwayOffset {
    let ft_lat = 364_000.0;
    let ft_lon = 364_000.0 * rw.threshold_lat.to_radians().cos();
    let north = (lat - rw.threshold_lat) * ft_lat;
    let east = (lon - rw.threshold_lon) * ft_lon;
    let h = rw.heading_true_deg.to_radians();
    RunwayOffset { along_ft: north * h.cos() + east * h.sin(), cross_ft: east * h.cos() - north * h.sin() }
}

/// Pas, na którym wylądował samolot: kurs zgodny (±30°) i najmniejsze odchylenie od osi.
pub fn pick(candidates: &[RunwayGeom], lat: f64, lon: f64, heading_true: Option<f64>) -> Option<RunwayGeom> {
    candidates
        .iter()
        .filter(|rw| heading_true.is_none_or(|h| (((rw.heading_true_deg - h + 540.0) % 360.0) - 180.0).abs() <= 30.0))
        .map(|rw| (rw, offset(rw, lat, lon)))
        .filter(|(rw, o)| o.along_ft > -1500.0 && o.along_ft < rw.length_ft.unwrap_or(15000.0) + 1000.0 && o.cross_ft.abs() < 500.0)
        .min_by(|a, b| a.1.cross_ft.abs().total_cmp(&b.1.cross_ft.abs()))
        .map(|(rw, _)| rw.clone())
}

/// Składowe wiatru względem pasa: (czołowa +, boczna + z prawej).
pub fn wind_components(wind_dir: f64, wind_kt: f64, runway_true: f64) -> (f64, f64) {
    let a = (wind_dir - runway_true).to_radians();
    (wind_kt * a.cos(), wind_kt * a.sin())
}

/// Dane przyziemienia do oceny. Brakujące wartości nie wpływają na wynik.
#[derive(Clone, Debug, Default)]
pub struct Touchdown {
    pub vs_fpm: f64,
    pub g_force: Option<f64>,
    pub bank_deg: Option<f64>,
    pub pitch_deg: Option<f64>,
    pub ias_kt: Option<f64>,
    pub runway: Option<String>,
    pub along_ft: Option<f64>,
    pub cross_ft: Option<f64>,
    pub runway_length_ft: Option<f64>,
    pub crosswind_kt: Option<f64>,
    pub bounces: i32,
}

pub fn grade_of(s: i32) -> &'static str {
    match s {
        90.. => tr("Doskonałe"),
        75..=89 => tr("Dobre"),
        55..=74 => tr("Poprawne"),
        35..=54 => tr("Twarde"),
        _ => tr("Bardzo twarde"),
    }
}

pub fn evaluate(t: &Touchdown) -> LandingReport {
    let mut score = 100.0f64;
    let mut notes: Vec<String> = Vec::new();
    let vs = t.vs_fpm.abs();
    let v = vs.round() as i64;
    if vs < 60.0 {
        score -= 5.0;
        notes.push(trf!("Bardzo miękko ({v} ft/min) – uważaj na przeciąganie wyrównania."));
    } else if vs <= 240.0 {
        notes.push(trf!("Miękkie przyziemienie: {v} ft/min."));
    } else if vs <= 400.0 {
        score -= 10.0 + (vs - 240.0) / 160.0 * 15.0;
        notes.push(trf!("Wyczuwalne przyziemienie: {v} ft/min."));
    } else if vs <= 600.0 {
        score -= 25.0 + (vs - 400.0) / 200.0 * 25.0;
        notes.push(trf!("Twarde lądowanie: {v} ft/min."));
    } else {
        score -= 60.0;
        notes.push(trf!("Bardzo twarde lądowanie: {v} ft/min – w realnym lotnictwie przegląd techniczny."));
    }
    if let Some(g) = t.g_force {
        let g_text = format!("{} {:.2} g.", tr("Przeciążenie"), g);
        if g >= 2.0 {
            score -= 30.0;
            notes.push(g_text);
        } else if g >= 1.6 {
            score -= 15.0;
            notes.push(g_text);
        } else if g >= 1.4 {
            score -= 5.0;
        }
    }
    if let Some(b) = t.bank_deg {
        let a = b.abs();
        let deg = a.round() as i64;
        if a > 8.0 {
            score -= 20.0;
            notes.push(trf!("Duże przechylenie przy przyziemieniu: {deg}°."));
        } else if a > 4.0 {
            score -= 8.0;
            notes.push(trf!("Przechylenie {deg}° – przyziemienie na jedno koło."));
        }
    }
    if let Some(p) = t.pitch_deg {
        if p < 0.0 {
            score -= 20.0;
            let pitch = format!("{p:.1}");
            notes.push(trf!("Przyziemienie na przednie koło (pochylenie {pitch}°)."));
        } else if p > 10.0 {
            score -= 10.0;
            let deg = p.round() as i64;
            notes.push(trf!("Duże pochylenie {deg}° – ryzyko uderzenia ogonem."));
        }
    }
    if let Some(a) = t.along_ft {
        let ft = a.round() as i64;
        if a < 0.0 {
            score -= 40.0;
            let before = -ft;
            notes.push(trf!("Przyziemienie przed progiem pasa ({before} ft)."));
        } else if a < 500.0 {
            score -= 5.0;
            notes.push(trf!("Przyziemienie tuż za progiem ({ft} ft)."));
        } else if a <= 3000.0 {
            notes.push(trf!("W strefie przyziemienia: {ft} ft za progiem."));
        } else {
            score -= if a > 4500.0 { 25.0 } else { 10.0 };
            let mut n = trf!("Długie lądowanie: {ft} ft za progiem");
            match t.runway_length_ft.map(|l| (l - a).round() as i64) {
                Some(rem) => n.push_str(&trf!(", zostało {rem} ft pasa.")),
                None => n.push('.'),
            }
            notes.push(n);
        }
    }
    if let Some(x) = t.cross_ft {
        let a = x.abs();
        let ft = a.round() as i64;
        let side = if x >= 0.0 { tr("w prawo") } else { tr("w lewo") };
        if a > 50.0 {
            score -= 25.0;
            notes.push(trf!("Daleko od osi pasa: {ft} ft {side}."));
        } else if a > 25.0 {
            score -= 12.0;
            notes.push(trf!("Odchylenie od osi: {ft} ft {side}."));
        } else if a > 12.0 {
            score -= 4.0;
        }
    }
    if t.bounces > 0 {
        score -= 12.0 * t.bounces as f64;
        let n = t.bounces;
        notes.push(trf!("Odbicia: {n}."));
    }
    if let Some(cw) = t.crosswind_kt {
        if cw.abs() >= 15.0 {
            score += 5.0;
            let kt = cw.abs().round() as i64;
            notes.push(trf!("Wiatr boczny {kt} kt – wynik złagodzony."));
        }
    }
    let s = (score.round() as i32).clamp(0, 100);
    LandingReport {
        score: s,
        grade: grade_of(s).to_string(),
        vs_fpm: t.vs_fpm.round() as i32,
        g_force: t.g_force,
        bank_deg: t.bank_deg,
        pitch_deg: t.pitch_deg,
        ias_kt: t.ias_kt.map(|v| v.round() as i32),
        runway: t.runway.clone(),
        along_ft: t.along_ft.map(|v| v.round() as i32),
        cross_ft: t.cross_ft.map(|v| v.round() as i32),
        crosswind_kt: t.crosswind_kt.map(|v| v.round() as i32),
        bounces: t.bounces,
        notes,
        airport: None,
        at: None,
        runway_length_ft: t.runway_length_ft.map(|v| v.round() as i32),
        profile: Vec::new(),
    }
}

/// Uwagi w bieżącym języku programu odtworzone z liczb (zapisane uwagi są w języku z chwili lądowania).
pub fn notes_now(r: &LandingReport) -> Vec<String> {
    evaluate(&Touchdown {
        vs_fpm: r.vs_fpm as f64,
        g_force: r.g_force,
        bank_deg: r.bank_deg,
        pitch_deg: r.pitch_deg,
        ias_kt: r.ias_kt.map(|v| v as f64),
        runway: r.runway.clone(),
        along_ft: r.along_ft.map(|v| v as f64),
        cross_ft: r.cross_ft.map(|v| v as f64),
        runway_length_ft: r.runway_length_ft.map(|v| v as f64),
        crosswind_kt: r.crosswind_kt.map(|v| v as f64),
        bounces: r.bounces,
    })
    .notes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_landing_in_zone() {
        let r = evaluate(&Touchdown { vs_fpm: -150.0, g_force: Some(1.1), along_ft: Some(1200.0), cross_ft: Some(5.0), ..Default::default() });
        assert_eq!(r.score, 100);
        assert_eq!(r.notes.len(), 2);
    }

    #[test]
    fn hard_and_bounced() {
        let r = evaluate(&Touchdown { vs_fpm: -650.0, g_force: Some(2.1), bounces: 2, ..Default::default() });
        assert_eq!(r.score, 0);
        let r = evaluate(&Touchdown { vs_fpm: -320.0, ..Default::default() });
        assert_eq!(r.score, 83);
    }

    #[test]
    fn runway_pick_and_offset() {
        let rw = RunwayGeom { ident: "29".into(), threshold_lat: 54.3790, threshold_lon: 18.4880, heading_true_deg: 292.0, length_ft: Some(9186.0) };
        let (lat, lon) = crate::navdata::math::offset(54.3790, 18.4880, 292.0, 400.0);
        let o = offset(&rw, lat, lon);
        assert!((o.along_ft - 1312.0).abs() < 30.0, "{o:?}");
        assert!(o.cross_ft.abs() < 10.0);
        assert_eq!(pick(&[rw.clone()], lat, lon, Some(290.0)).map(|r| r.ident), Some("29".into()));
        assert_eq!(pick(&[rw], lat, lon, Some(110.0)), None);
    }

    #[test]
    fn wind() {
        let (h, c) = wind_components(270.0, 20.0, 290.0);
        assert!((h - 18.79).abs() < 0.1);
        assert!((c + 6.84).abs() < 0.1);
    }
}
