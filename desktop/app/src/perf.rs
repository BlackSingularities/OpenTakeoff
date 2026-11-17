//! Kalkulatory osiągów karty Narzędzia: start (V1, VR, V2, długość startu) i lądowanie (VREF, VAPP,
//! odległość lądowania, wymagana długość pasa) z masy, temperatury, ciśnienia, wiatru i pasa z bazy lotnisk.
//!
//! To szacunek do symulatora, nie dane z AFM: każdy typ ma wartości odniesienia (prędkości i odległości
//! przy MTOW/MLW na poziomie morza w ISA, bez wiatru) skalowane prostym modelem:
//! - prędkości ∝ √(masa / masa odniesienia),
//! - odległość startu ∝ (masa / MTOW)² / σ, lądowania ∝ (masa / MLW) / σ (σ – gęstość względna z wysokości
//!   gęstościowej),
//! - wiatr: czołowy skraca o (1 − w/V)², tylny wydłuża o (1 + 1,5·w/V)² (jak w przepisach – 150% tylnego),
//! - pas mokry: start ×1,15, lądowanie ×1,15; trawa: start ×1,2, lądowanie ×1,2,
//! - wymagana długość pasa do lądowania = odległość × 1,67 (samoloty transportowe) albo × 1,43 (GA).

use opentakeoff_core::aero::{airports, calc};
use opentakeoff_core::lang::tr;
use opentakeoff_core::proto::FlightState;

pub struct Profile {
    pub name: &'static str,
    /// Wzorzec nazwy samolotu z symulatora (bez rozróżniania wielkości liter).
    pub pattern: &'static str,
    pub mtow: f64,
    pub mlw: f64,
    /// Prędkości przy MTOW (KIAS); `v1` = 0 – samolot jednosilnikowy (bez V1, V2 to prędkość wznoszenia).
    pub v1: f64,
    pub vr: f64,
    pub v2: f64,
    /// VREF przy MLW (KIAS).
    pub vref: f64,
    /// Długość startu przy MTOW, SL, ISA (m) – do 35 ft (odrzutowce) albo 50 ft (pozostałe).
    pub tod: f64,
    /// Odległość lądowania przy MLW, SL, ISA, pas suchy (m) – od 50 ft do zatrzymania, bez współczynnika.
    pub ld: f64,
    /// Samolot transportowy (wymagana długość pasa ×1,67 zamiast ×1,43).
    pub transport: bool,
}

/// Profile typów (dane orientacyjne z publicznych materiałów producentów).
pub const PROFILES: &[Profile] = &[
    Profile { name: "Airbus A320neo", pattern: r"a32[01]|a20n|a21n|a319|fenix|flybywire|\bfbw\b", mtow: 79_000.0, mlw: 67_400.0, v1: 143.0, vr: 147.0, v2: 151.0, vref: 133.0, tod: 2_100.0, ld: 870.0, transport: true },
    Profile { name: "Airbus A330-900", pattern: r"a33[0-9]|a339", mtow: 251_000.0, mlw: 191_000.0, v1: 150.0, vr: 160.0, v2: 166.0, vref: 140.0, tod: 2_770.0, ld: 1_080.0, transport: true },
    Profile { name: "Boeing 737-800", pattern: r"\bb?737|b73[6-9]|b38m|zibo|pmdg 737", mtow: 79_000.0, mlw: 66_360.0, v1: 147.0, vr: 151.0, v2: 157.0, vref: 147.0, tod: 2_300.0, ld: 980.0, transport: true },
    Profile { name: "Boeing 787-9", pattern: r"\bb?787|b78[89x]|dreamliner", mtow: 254_000.0, mlw: 192_800.0, v1: 160.0, vr: 165.0, v2: 172.0, vref: 150.0, tod: 2_900.0, ld: 1_080.0, transport: true },
    Profile { name: "Boeing 777-300ER", pattern: r"\bb?777|b77[23lw]", mtow: 351_500.0, mlw: 251_300.0, v1: 165.0, vr: 175.0, v2: 182.0, vref: 155.0, tod: 3_100.0, ld: 1_140.0, transport: true },
    Profile { name: "Embraer E190", pattern: r"e-?1[79][05]|erj|embraer", mtow: 51_800.0, mlw: 44_000.0, v1: 132.0, vr: 136.0, v2: 142.0, vref: 128.0, tod: 2_050.0, ld: 800.0, transport: true },
    Profile { name: "ATR 72-600", pattern: r"atr\s?-?\s?72", mtow: 23_000.0, mlw: 22_350.0, v1: 107.0, vr: 108.0, v2: 115.0, vref: 113.0, tod: 1_330.0, ld: 550.0, transport: true },
    Profile { name: "Dash 8 Q400", pattern: r"q400|dash ?8|dhc-?8", mtow: 29_574.0, mlw: 28_123.0, v1: 115.0, vr: 117.0, v2: 124.0, vref: 122.0, tod: 1_425.0, ld: 770.0, transport: true },
    Profile { name: "King Air 350", pattern: r"king ?air|\bb350|\bbe(20|35)\b|\bb200", mtow: 6_800.0, mlw: 6_800.0, v1: 98.0, vr: 100.0, v2: 106.0, vref: 105.0, tod: 1_006.0, ld: 823.0, transport: true },
    Profile { name: "TBM 930", pattern: r"tbm", mtow: 3_354.0, mlw: 3_186.0, v1: 0.0, vr: 90.0, v2: 124.0, vref: 85.0, tod: 726.0, ld: 741.0, transport: false },
    Profile { name: "Cessna 208B Caravan", pattern: r"\bc?208|caravan", mtow: 3_995.0, mlw: 3_855.0, v1: 0.0, vr: 72.0, v2: 104.0, vref: 80.0, tod: 738.0, ld: 547.0, transport: false },
    Profile { name: "Cirrus SR22", pattern: r"sr2[02]|cirrus", mtow: 1_633.0, mlw: 1_633.0, v1: 0.0, vr: 70.0, v2: 101.0, vref: 79.0, tod: 569.0, ld: 708.0, transport: false },
    Profile { name: "Diamond DA40 NG", pattern: r"da-?\s?40", mtow: 1_310.0, mlw: 1_310.0, v1: 0.0, vr: 67.0, v2: 72.0, vref: 73.0, tod: 655.0, ld: 518.0, transport: false },
    Profile { name: "Piper PA-28 Archer", pattern: r"pa-?28|archer|warrior|cherokee", mtow: 1_157.0, mlw: 1_157.0, v1: 0.0, vr: 60.0, v2: 76.0, vref: 66.0, tod: 504.0, ld: 415.0, transport: false },
    Profile { name: "Cessna 172S", pattern: r"\bc?172|skyhawk", mtow: 1_157.0, mlw: 1_157.0, v1: 0.0, vr: 55.0, v2: 74.0, vref: 65.0, tod: 497.0, ld: 407.0, transport: false },
    Profile { name: "Cessna 152", pattern: r"\bc?152\b", mtow: 757.0, mlw: 757.0, v1: 0.0, vr: 50.0, v2: 67.0, vref: 60.0, tod: 408.0, ld: 366.0, transport: false },
];

pub fn names() -> Vec<String> {
    PROFILES.iter().map(|p| p.name.to_string()).collect()
}

/// Profil dla samolotu z symulatora (indeks) – pierwszy pasujący wzorzec.
pub fn detect(s: &FlightState) -> Option<usize> {
    let a = &s.aircraft;
    let text = format!("{} {} {}", a.title, a.atc_model, a.atc_type);
    PROFILES.iter().position(|p| regex::RegexBuilder::new(p.pattern).case_insensitive(true).build().is_ok_and(|r| r.is_match(&text)))
}

/// Gęstość względna powietrza z wysokości gęstościowej (ft).
fn sigma(da_ft: f64) -> f64 {
    (1.0 - 6.875_6e-6 * da_ft).max(0.3).powf(4.2561)
}

/// Pas z bazy lotnisk: (opis, długość dostępna do startu m, do lądowania m, kurs, elewacja ft, trawa).
struct Rwy {
    label: String,
    tora: f64,
    lda: f64,
    hdg: f64,
    elev: f64,
    grass: bool,
}

fn runway(icao: &str, ident: &str) -> Option<Rwy> {
    let db = airports::db()?;
    let a = db.get(&icao.trim().to_uppercase())?;
    let want = ident.trim().to_uppercase();
    for r in a.runways.iter().filter(|r| !r.closed) {
        for (id, hdg, elev, displaced) in [(&r.le, r.le_heading(), r.le_elev, r.le_displaced), (&r.he, r.he_heading(), r.he_elev, r.he_displaced)] {
            let norm = |s: &str| s.trim_start_matches('0').to_uppercase();
            if norm(id) != norm(&want) {
                continue;
            }
            let hdg = hdg.or_else(|| id.trim_start_matches('0').chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<f64>().ok().map(|x| x * 10.0))?;
            let len = r.length_ft as f64 * 0.3048;
            let s = r.surface.to_lowercase();
            let grass = s.contains("grass") || s.contains("turf") || s.contains("gras") || s.contains("dirt") || s.contains("gravel");
            let elev = elev.or(a.elev_ft).unwrap_or(0) as f64;
            return Some(Rwy {
                label: format!("{} {} · {:.0} m · {:03.0}° · {:.0} ft", a.ident, id, len, hdg, elev),
                tora: len,
                lda: (len - displaced as f64 * 0.3048).max(0.0),
                hdg,
                elev,
                grass,
            });
        }
    }
    None
}

/// Wspólne wejście: [profil, masa kg, ICAO, pas, OAT °C, QNH hPa, kierunek wiatru, prędkość wiatru, mokry 0/1].
struct Input {
    p: &'static Profile,
    w: f64,
    rwy: Option<Rwy>,
    elev: f64,
    oat: f64,
    qnh: f64,
    wdir: f64,
    wspd: f64,
    wet: bool,
}

fn input(v: &[String]) -> Option<Input> {
    let n = |i: usize| v.get(i).and_then(|s| s.trim().replace(',', ".").parse::<f64>().ok());
    let p = PROFILES.get(n(0)? as usize)?;
    let rwy = runway(v.get(2).map(|s| s.as_str()).unwrap_or(""), v.get(3).map(|s| s.as_str()).unwrap_or(""));
    let elev = rwy.as_ref().map(|r| r.elev).unwrap_or(0.0);
    Some(Input { p, w: n(1)?, rwy, elev, oat: n(4).unwrap_or(15.0), qnh: n(5).unwrap_or(1013.25), wdir: n(6).unwrap_or(0.0), wspd: n(7).unwrap_or(0.0), wet: v.get(8).is_some_and(|s| s == "1") })
}

/// Składowe wiatru na pasie: (czołowy +, boczny), bez pasa – zero.
fn wind(i: &Input) -> (f64, f64) {
    match &i.rwy {
        Some(r) => calc::wind_components(i.wdir, i.wspd, r.hdg),
        None => (0.0, 0.0),
    }
}

fn wind_factor(hw: f64, v: f64) -> f64 {
    let v = v.max(30.0);
    if hw >= 0.0 {
        (1.0 - hw / v).max(0.4).powi(2)
    } else {
        (1.0 - 1.5 * hw / v).powi(2)
    }
}

fn kt(v: f64) -> String {
    format!("{v:.0} kt")
}

fn m(v: f64) -> String {
    format!("{v:.0} m")
}

fn wind_text(hw: f64, xw: f64) -> String {
    format!(
        "{} {:.0} kt · {} {:.0} kt",
        if hw >= 0.0 { tr("czołowy") } else { tr("tylny") },
        hw.abs(),
        tr("boczny"),
        xw.abs()
    )
}

/// Start: [stan (1 mieści się, 0 nie, puste – brak pasa), V1, VR, V2, długość startu, dostępna, zapas,
/// pas, wiatr, wysokość gęstościowa, masa/MTOW, ostrzeżenie, wymagana/dostępna (0–1,5)].
pub fn takeoff(v: &[String]) -> Vec<String> {
    let Some(i) = input(v) else { return vec![String::new(); 13] };
    let p = i.p;
    let r = i.w / p.mtow;
    let k = r.clamp(0.4, 1.15).sqrt();
    let (vr, v2) = (p.vr * k, p.v2 * k);
    let (hw, xw) = wind(&i);
    let da = calc::density_altitude(i.elev, i.qnh, i.oat);
    let mut dist = p.tod * r.powi(2) / sigma(da) * wind_factor(hw, vr);
    if i.wet {
        dist *= 1.15;
    }
    if i.rwy.as_ref().is_some_and(|x| x.grass) {
        dist *= 1.2;
    }
    // V1 na mokrym pasie i krótkim pasie niższe (mniejsza droga zatrzymania)
    let v1 = if p.v1 > 0.0 { (p.v1 * k - if i.wet { 4.0 } else { 0.0 }).min(vr) } else { 0.0 };
    let mut warn = Vec::new();
    if r > 1.0 {
        warn.push(format!("{} ({:.0} kg)", tr("Masa powyżej MTOW"), p.mtow));
    }
    if hw < -10.0 {
        warn.push(tr("Wiatr tylny ponad 10 kt").to_string());
    }
    let (ok, avail, margin, frac, label) = match &i.rwy {
        Some(rw) => {
            let ok = dist <= rw.tora;
            (if ok { "1" } else { "0" }.to_string(), m(rw.tora), format!("{:+.0} m", rw.tora - dist), format!("{:.3}", (dist / rw.tora.max(1.0)).min(1.5)), rw.label.clone())
        }
        None => (String::new(), String::new(), String::new(), String::new(), tr("Pas nie znaleziony w bazie lotnisk").to_string()),
    };
    vec![
        ok,
        if v1 > 0.0 { kt(v1) } else { "–".into() },
        kt(vr),
        kt(v2),
        m(dist),
        avail,
        margin,
        label,
        wind_text(hw, xw),
        format!("{da:.0} ft"),
        format!("{:.0}%", r * 100.0),
        warn.join(" · "),
        frac,
    ]
}

/// Lądowanie: [stan, VREF, VAPP, odległość lądowania, wymagana długość pasa, dostępna (LDA), zapas, pas, wiatr,
/// wysokość gęstościowa, masa/MLW, ostrzeżenie, wymagana/dostępna].
pub fn landing(v: &[String]) -> Vec<String> {
    let Some(i) = input(v) else { return vec![String::new(); 13] };
    let p = i.p;
    let r = i.w / p.mlw;
    let vref = p.vref * r.clamp(0.4, 1.2).sqrt();
    let (hw, xw) = wind(&i);
    let vapp = vref + (hw.max(0.0) / 3.0).clamp(5.0, 15.0);
    let da = calc::density_altitude(i.elev, i.qnh, i.oat);
    let mut dist = p.ld * r / sigma(da) * wind_factor(hw, vref);
    if i.rwy.as_ref().is_some_and(|x| x.grass) {
        dist *= 1.2;
    }
    let mut req = dist * if p.transport { 1.67 } else { 1.43 };
    if i.wet {
        req *= 1.15;
    }
    let mut warn = Vec::new();
    if r > 1.0 {
        warn.push(format!("{} ({:.0} kg)", tr("Masa powyżej MLW"), p.mlw));
    }
    if hw < -10.0 {
        warn.push(tr("Wiatr tylny ponad 10 kt").to_string());
    }
    let (ok, avail, margin, frac, label) = match &i.rwy {
        Some(rw) => {
            let ok = req <= rw.lda;
            (if ok { "1" } else { "0" }.to_string(), m(rw.lda), format!("{:+.0} m", rw.lda - req), format!("{:.3}", (req / rw.lda.max(1.0)).min(1.5)), rw.label.clone())
        }
        None => (String::new(), String::new(), String::new(), String::new(), tr("Pas nie znaleziony w bazie lotnisk").to_string()),
    };
    vec![ok, kt(vref), kt(vapp), m(dist), m(req), avail, margin, label, wind_text(hw, xw), format!("{da:.0} ft"), format!("{:.0}%", r * 100.0), warn.join(" · "), frac]
}

/// Dane profilu do pól: [MTOW, MLW, masa domyślna startu (85% MTOW), masa domyślna lądowania (90% MLW)].
pub fn info(idx: usize) -> Vec<String> {
    match PROFILES.get(idx) {
        Some(p) => vec![format!("{:.0}", p.mtow), format!("{:.0}", p.mlw), format!("{:.0}", (p.mtow * 0.85 / 10.0).round() * 10.0), format!("{:.0}", (p.mlw * 0.9 / 10.0).round() * 10.0)],
        None => vec![String::new(); 4],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(p: usize, w: f64) -> Vec<String> {
        vec![p.to_string(), w.to_string(), String::new(), String::new(), "15".into(), "1013.25".into(), "0".into(), "0".into(), "0".into()]
    }

    #[test]
    fn reference_values_at_mtow() {
        let out = takeoff(&v(0, PROFILES[0].mtow));
        assert_eq!(out[2], "147 kt");
        assert_eq!(out[4], "2100 m");
        let out = landing(&v(0, PROFILES[0].mlw));
        assert_eq!(out[1], "133 kt");
        assert_eq!(out[3], "870 m");
    }

    #[test]
    fn lighter_is_shorter_and_slower() {
        let heavy = takeoff(&v(2, 79_000.0));
        let light = takeoff(&v(2, 60_000.0));
        let num = |s: &str| s.split(' ').next().unwrap().parse::<f64>().unwrap();
        assert!(num(&light[4]) < num(&heavy[4]));
        assert!(num(&light[2]) < num(&heavy[2]));
    }

    #[test]
    fn single_engine_has_no_v1() {
        assert_eq!(takeoff(&v(14, 1_000.0))[1], "–");
    }

    #[test]
    fn hot_and_high_is_longer() {
        assert!(sigma(8000.0) < sigma(0.0));
        assert!(wind_factor(-10.0, 140.0) > 1.0 && wind_factor(10.0, 140.0) < 1.0);
    }
}
