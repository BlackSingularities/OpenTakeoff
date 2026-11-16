//! Dekoder METAR i TAF: każda grupa depeszy z opisem po ludzku (do podglądu „co znaczy ta grupa”).

use crate::lang::tr;
use crate::trf;

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub token: String,
    pub meaning: String,
}

/// Widzialność w milach lądowych: "10", "6+", "1/2", "1 1/2", "3/4SM".
pub fn parse_sm(v: &str) -> Option<f64> {
    let s = v.trim().trim_end_matches("SM").trim_end_matches('+');
    let mut total = 0.0;
    for part in s.split_whitespace() {
        if let Some((a, b)) = part.split_once('/') {
            total += a.parse::<f64>().ok()? / b.parse::<f64>().ok()?.max(1.0);
        } else {
            total += part.parse::<f64>().ok()?;
        }
    }
    Some(total)
}

fn cover(c: &str) -> &'static str {
    match c {
        "FEW" => tr("nieliczne (1–2/8)"),
        "SCT" => tr("rozproszone (3–4/8)"),
        "BKN" => tr("poprzerywane (5–7/8)"),
        "OVC" => tr("całkowite zachmurzenie (8/8)"),
        "VV" => tr("widzialność pionowa"),
        _ => "",
    }
}

fn weather_code(code: &str) -> Option<String> {
    let mut rest = code;
    let mut parts: Vec<&str> = Vec::new();
    if let Some(r) = rest.strip_prefix('+') {
        parts.push(tr("silne"));
        rest = r;
    } else if let Some(r) = rest.strip_prefix('-') {
        parts.push(tr("słabe"));
        rest = r;
    }
    if let Some(r) = rest.strip_prefix("VC") {
        parts.push(tr("w pobliżu"));
        rest = r;
    }
    if rest.is_empty() || rest.len() % 2 != 0 {
        return None;
    }
    let mut i = 0;
    while i < rest.len() {
        let w = match &rest[i..i + 2] {
            "MI" => tr("płytkie"),
            "BC" => tr("płaty"),
            "PR" => tr("częściowe"),
            "DR" => tr("zamieć niska"),
            "BL" => tr("zamieć wysoka"),
            "SH" => tr("przelotne"),
            "TS" => tr("burza"),
            "FZ" => tr("marznące"),
            "DZ" => tr("mżawka"),
            "RA" => tr("deszcz"),
            "SN" => tr("śnieg"),
            "SG" => tr("śnieg ziarnisty"),
            "IC" => tr("igły lodowe"),
            "PL" => tr("deszcz lodowy"),
            "GR" => tr("grad"),
            "GS" => tr("krupa"),
            "UP" => tr("nieznany opad"),
            "BR" => tr("zamglenie"),
            "FG" => tr("mgła"),
            "FU" => tr("dym"),
            "VA" => tr("pył wulkaniczny"),
            "DU" => tr("pył"),
            "SA" => tr("piasek"),
            "HZ" => tr("zmętnienie"),
            "PO" => tr("wiry pyłowe"),
            "SQ" => tr("szkwał"),
            "FC" => tr("trąba"),
            "SS" => tr("burza piaskowa"),
            "DS" => tr("burza pyłowa"),
            _ => return None,
        };
        parts.push(w);
        i += 2;
    }
    Some(parts.join(" "))
}

fn is_time(t: &str) -> bool {
    t.len() == 7 && t.ends_with('Z') && t[..6].chars().all(|c| c.is_ascii_digit())
}

fn wind(t: &str) -> Option<String> {
    let unit = if t.ends_with("KT") {
        "kt"
    } else if t.ends_with("MPS") {
        "m/s"
    } else {
        return None;
    };
    let body = t.trim_end_matches("KT").trim_end_matches("MPS");
    if body.len() < 5 {
        return None;
    }
    let (dir, rest) = body.split_at(3);
    let (spd, gust) = match rest.split_once('G') {
        Some((s, g)) => (s, Some(g)),
        None => (rest, None),
    };
    let spd: i32 = spd.parse().ok()?;
    let mut s = if dir == "VRB" {
        trf!("Wiatr zmienny {spd} {unit}")
    } else if body == "00000" {
        tr("Cisza").to_string()
    } else {
        let d: i32 = dir.parse().ok()?;
        trf!("Wiatr z {d}° z prędkością {spd} {unit}")
    };
    if let Some(g) = gust.and_then(|g| g.parse::<i32>().ok()) {
        s.push_str(&trf!(", w porywach {g} {unit}"));
    }
    Some(s)
}

/// Opis jednej grupy METAR/TAF albo None, gdy grupa nieznana.
pub fn explain(t: &str, first: bool) -> Option<String> {
    if first && (t == "METAR" || t == "SPECI" || t == "TAF") {
        return Some(match t {
            "METAR" => tr("Depesza o bieżącej pogodzie na lotnisku").to_string(),
            "SPECI" => tr("Depesza specjalna (nagła zmiana pogody)").to_string(),
            _ => tr("Prognoza pogody dla lotniska").to_string(),
        });
    }
    if t.len() == 4 && t.chars().all(|c| c.is_ascii_uppercase()) && first {
        return Some(trf!("Lotnisko {t}"));
    }
    if is_time(t) {
        let (d, h, m) = (&t[0..2], &t[2..4], &t[4..6]);
        return Some(trf!("Obserwacja {d}. dnia miesiąca o {h}:{m} UTC"));
    }
    match t {
        "AUTO" => return Some(tr("Obserwacja automatyczna (bez obserwatora)").into()),
        "COR" => return Some(tr("Depesza poprawiona").into()),
        "CAVOK" => return Some(tr("Widzialność ≥ 10 km, brak chmur poniżej 5000 ft i CB, brak zjawisk").into()),
        "NOSIG" => return Some(tr("Bez istotnych zmian w ciągu 2 godzin").into()),
        "NSC" => return Some(tr("Brak istotnych chmur").into()),
        "SKC" | "CLR" => return Some(tr("Bezchmurnie").into()),
        "NCD" => return Some(tr("Czujnik nie wykrył chmur").into()),
        "BECMG" => return Some(tr("Stopniowa zmiana").into()),
        "TEMPO" => return Some(tr("Okresowo").into()),
        "RMK" => return Some(tr("Uwagi (dalsza część w kodzie krajowym)").into()),
        "9999" => return Some(tr("Widzialność 10 km lub więcej").into()),
        _ => {}
    }
    if let Some(w) = wind(t) {
        return Some(w);
    }
    if t.len() == 7 && t.as_bytes()[3] == b'V' && t[..3].chars().all(|c| c.is_ascii_digit()) && t[4..].chars().all(|c| c.is_ascii_digit()) {
        return Some(trf!("Kierunek wiatru zmienny między {a}° a {b}°", a = &t[..3], b = &t[4..]));
    }
    if t.len() == 4 && t.chars().all(|c| c.is_ascii_digit()) {
        let m: i32 = t.parse().ok()?;
        return Some(trf!("Widzialność {m} m"));
    }
    if let Some(sm) = t.strip_suffix("SM") {
        let v = parse_sm(sm)?;
        return Some(trf!("Widzialność {v} mil lądowych"));
    }
    if let Some(rest) = t.strip_prefix('R') {
        if let Some((rwy, val)) = rest.split_once('/') {
            if rwy.len() >= 2 && rwy[..2].chars().all(|c| c.is_ascii_digit()) {
                let v: String = val.chars().filter(|c| c.is_ascii_digit()).collect();
                return Some(trf!("Zasięg widzenia na pasie {rwy}: {v} m"));
            }
        }
    }
    for c in ["FEW", "SCT", "BKN", "OVC", "VV"] {
        if let Some(rest) = t.strip_prefix(c) {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.len() == 3 {
                let base = digits.parse::<i32>().ok()? * 100;
                let kind = if rest.ends_with("CB") {
                    tr(", chmury burzowe Cumulonimbus")
                } else if rest.ends_with("TCU") {
                    tr(", wypiętrzone Cumulusy")
                } else {
                    ""
                };
                let what = cover(c);
                return Some(trf!("Chmury {what} na {base} ft{kind}"));
            }
        }
    }
    if let Some((a, b)) = t.split_once('/') {
        let temp = |s: &str| -> Option<i32> {
            let (neg, d) = match s.strip_prefix('M') {
                Some(d) => (true, d),
                None => (false, s),
            };
            if d.len() != 2 {
                return None;
            }
            let v: i32 = d.parse().ok()?;
            Some(if neg { -v } else { v })
        };
        if let (Some(ta), Some(td)) = (temp(a), temp(b)) {
            return Some(trf!("Temperatura {ta} °C, punkt rosy {td} °C"));
        }
    }
    if let Some(q) = t.strip_prefix('Q') {
        if q.len() == 4 {
            let h: i32 = q.parse().ok()?;
            return Some(trf!("Ciśnienie QNH {h} hPa"));
        }
    }
    if let Some(a) = t.strip_prefix('A') {
        if a.len() == 4 && a.chars().all(|c| c.is_ascii_digit()) {
            let inhg = a.parse::<f64>().ok()? / 100.0;
            let hpa = (inhg * 33.8639).round();
            return Some(trf!("Ciśnienie {inhg:.2} inHg ({hpa} hPa)"));
        }
    }
    if let Some(p) = t.strip_prefix("FM") {
        if p.len() == 6 {
            return Some(trf!("Od {d}. dnia {h}:{m} UTC", d = &p[..2], h = &p[2..4], m = &p[4..6]));
        }
    }
    if let Some(p) = t.strip_prefix("PROB") {
        return Some(trf!("Prawdopodobieństwo {p}%"));
    }
    if t.len() == 9 && t.as_bytes()[4] == b'/' && t.chars().filter(|c| c.is_ascii_digit()).count() == 8 {
        return Some(trf!("Ważna od {a}. dnia {b}:00 do {c}. dnia {d}:00 UTC", a = &t[..2], b = &t[2..4], c = &t[5..7], d = &t[7..9]));
    }
    if let Some(r) = t.strip_prefix("RE") {
        if let Some(w) = weather_code(r) {
            return Some(trf!("Niedawno: {w}"));
        }
    }
    weather_code(t).map(|w| {
        let mut c = w.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => w,
        }
    })
}

/// Depesza rozbita na grupy z opisami. Grupy po RMK zostają bez opisu.
pub fn decode(raw: &str) -> Vec<Group> {
    let mut out = Vec::new();
    let mut remarks = false;
    for (i, t) in raw.split_whitespace().enumerate() {
        let t = t.trim_end_matches('=');
        if t.is_empty() {
            continue;
        }
        let meaning = if remarks {
            String::new()
        } else {
            let first = i < 2 || (i == 2 && out.iter().any(|g: &Group| g.token == "METAR" || g.token == "TAF" || g.token == "SPECI"));
            explain(t, first).unwrap_or_default()
        };
        if t == "RMK" {
            remarks = true;
        }
        out.push(Group { token: t.to_string(), meaning });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_groups() {
        let g = decode("METAR EPWA 151200Z 27015G25KT 240V300 9999 -SHRA BKN012CB 08/M01 Q1013 NOSIG");
        let m: Vec<&str> = g.iter().map(|g| g.meaning.as_str()).collect();
        assert!(m[1].contains("EPWA"));
        assert!(m[3].contains("270") && m[3].contains("25"));
        assert!(m[4].contains("240") && m[4].contains("300"));
        assert!(m[6].starts_with("Słabe"));
        assert!(m[7].contains("1200") && m[7].contains("Cumulonimbus"));
        assert!(m[8].contains("-1"));
        assert!(m[9].contains("1013"));
        assert!(!m[10].is_empty());
    }

    #[test]
    fn visibility() {
        assert_eq!(parse_sm("1 1/2"), Some(1.5));
        assert_eq!(parse_sm("6+"), Some(6.0));
        assert_eq!(parse_sm("3/4SM"), Some(0.75));
        assert!(explain("A2992", false).unwrap().contains("1013"));
        assert!(explain("2400/2506", false).is_some());
        assert!(explain("ZZZZ1", false).is_none());
    }
}
