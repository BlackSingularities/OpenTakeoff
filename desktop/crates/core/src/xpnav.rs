//! Dane nawigacyjne z plików X-Plane 11/12 (bez uruchamiania symulatora): punkty (`earth_fix.dat`),
//! VOR/NDB (`earth_nav.dat`), drogi lotnicze (`earth_awy.dat`) i procedury lotnisk (`CIFP/<ICAO>.dat`).
//! Najpierw `Custom Data` (aktualny AIRAC, np. z Navigraph), potem `Resources/default data`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::navdata::{fix_types as ft, math, AirportInfo, AirportProcedures, ApproachProcedure, FixResult, LegInfo, NamedLegs, NavDataProvider, NavError, NavResult, RunwayInfo, TerminalProcedure};

/// Foldery instalacji X-Plane zapisane przez instalator (`%LOCALAPPDATA%\x-plane_install_12.txt` i `_11`).
pub fn detect() -> Vec<PathBuf> {
    let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else { return Vec::new() };
    let mut out = Vec::new();
    for f in ["x-plane_install_12.txt", "x-plane_install_11.txt"] {
        if let Ok(t) = std::fs::read_to_string(local.join(f)) {
            for line in t.lines().map(str::trim).filter(|l| !l.is_empty()) {
                let p = PathBuf::from(line);
                if is_root(&p) && !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    // Steam: biblioteki z libraryfolders.vdf (także na innych dyskach)
    let mut roots: Vec<PathBuf> = Vec::new();
    for steam in [std::env::var_os("ProgramFiles(x86)").map(|p| PathBuf::from(p).join("Steam")), Some(PathBuf::from(r"C:\Steam"))].into_iter().flatten() {
        roots.push(steam.clone());
        if let Ok(vdf) = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) {
            for l in vdf.lines() {
                let t = l.trim();
                if let Some(rest) = t.strip_prefix("\"path\"") {
                    let p = rest.trim().trim_matches('"').replace("\\\\", "\\");
                    roots.push(PathBuf::from(p));
                }
            }
        }
    }
    for r in roots {
        for v in ["X-Plane 12", "X-Plane 11"] {
            let p = r.join("steamapps").join("common").join(v);
            if is_root(&p) && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    // Typowe miejsca instalatora: Pulpit, katalog główny dysku, Games
    let desktop = std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Desktop"));
    let mut bases: Vec<PathBuf> = desktop.into_iter().collect();
    for d in b'C'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", d as char));
        if root.exists() {
            bases.push(root.join("Games"));
            bases.push(root);
        }
    }
    for b in bases {
        for v in ["X-Plane 12", "X-Plane 11", "X-Plane 10"] {
            let p = b.join(v);
            if is_root(&p) && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// Czy to folder X-Plane z danymi nawigacyjnymi.
pub fn is_root(p: &Path) -> bool {
    p.join("Resources").join("default data").join("earth_fix.dat").is_file() || p.join("Custom Data").join("earth_fix.dat").is_file()
}

#[derive(Clone, Debug)]
struct Fix {
    ident: String,
    lat: f64,
    lon: f64,
    region: String,
    kind: &'static str,
    freq: Option<f64>,
    /// Lotnisko dla punktów terminalowych (pusty – trasowy).
    area: String,
}

#[derive(Clone, Debug)]
pub struct AirwaySeg {
    pub from: String,
    pub from_region: String,
    pub to: String,
    pub to_region: String,
    /// N – w obie strony, F – tylko from→to, B – tylko to→from.
    pub dir: char,
}

#[derive(Default)]
struct Db {
    fixes: HashMap<String, Vec<Fix>>,
    airways: HashMap<String, Vec<AirwaySeg>>,
    cycle: String,
}

pub struct XPlaneNavData {
    root: PathBuf,
    db: Mutex<Option<Arc<Db>>>,
}

impl XPlaneNavData {
    pub fn new(root: PathBuf) -> Self {
        Self { root, db: Mutex::new(None) }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn file(&self, name: &str) -> Option<PathBuf> {
        [self.root.join("Custom Data").join(name), self.root.join("Resources").join("default data").join(name)].into_iter().find(|p| p.exists())
    }

    fn db(&self) -> Arc<Db> {
        let mut g = self.db.lock();
        if let Some(d) = g.as_ref() {
            return d.clone();
        }
        let d = Arc::new(self.load());
        *g = Some(d.clone());
        d
    }

    fn load(&self) -> Db {
        let mut db = Db::default();
        let push = |db: &mut Db, f: Fix| db.fixes.entry(f.ident.clone()).or_default().push(f);
        // earth_fix.dat: 1101/1200 (X-Plane 11/12) „lat lon ident obszar region typ”, 600 (X-Plane 10) „lat lon ident”
        if let Some(p) = self.file("earth_fix.dat") {
            let text = read(&p);
            db.cycle = cycle(&text);
            for l in text.lines().skip(2) {
                let t: Vec<&str> = l.split_whitespace().collect();
                if t.len() < 3 || t[0] == "99" {
                    continue;
                }
                let (Ok(lat), Ok(lon)) = (t[0].parse::<f64>(), t[1].parse::<f64>()) else { continue };
                let (area, region) = if t.len() >= 5 { (if t[3] == "ENRT" { String::new() } else { t[3].to_string() }, t[4].to_string()) } else { (String::new(), String::new()) };
                push(&mut db, Fix { ident: t[2].to_string(), lat, lon, region, kind: ft::WAYPOINT, freq: None, area });
            }
        }
        // earth_nav.dat: 1100/1150/1200 „typ lat lon elew częst zasięg dekl ident obszar region nazwa”, 810 (X-Plane 10) bez obszaru i regionu
        if let Some(p) = self.file("earth_nav.dat") {
            let text = read(&p);
            let ver = version(&text);
            if db.cycle.is_empty() {
                db.cycle = cycle(&text);
            }
            for l in text.lines().skip(2) {
                let t: Vec<&str> = l.split_whitespace().collect();
                if t.len() < 9 {
                    continue;
                }
                let kind = match t[0] {
                    "2" => ft::NDB,
                    "3" => ft::VOR,
                    _ => continue,
                };
                let (Ok(lat), Ok(lon), Ok(fr)) = (t[1].parse::<f64>(), t[2].parse::<f64>(), t[4].parse::<f64>()) else { continue };
                let freq = if kind == ft::VOR { fr / 100.0 } else { fr };
                let (area, region) = if ver >= 1100 && t.len() >= 11 { (if t[8] == "ENRT" { String::new() } else { t[8].to_string() }, t[9].to_string()) } else { (String::new(), String::new()) };
                push(&mut db, Fix { ident: t[7].to_string(), lat, lon, region, kind, freq: Some(freq), area });
            }
        }
        // earth_awy.dat: 1100 „fix1 reg1 typ1 fix2 reg2 typ2 kierunek poziom dół góra nazwy”, 640 (X-Plane 10) „fix1 lat lon fix2 lat lon poziom dół góra nazwy”
        if let Some(p) = self.file("earth_awy.dat") {
            let text = read(&p);
            for l in text.lines().skip(2) {
                let t: Vec<&str> = l.split_whitespace().collect();
                if t.len() == 10 && t[1].parse::<f64>().is_ok() && t[2].parse::<f64>().is_ok() {
                    let (Ok(la1), Ok(lo1), Ok(la2), Ok(lo2)) = (t[1].parse::<f64>(), t[2].parse::<f64>(), t[4].parse::<f64>(), t[5].parse::<f64>()) else { continue };
                    for (id, la, lo) in [(t[0], la1, lo1), (t[3], la2, lo2)] {
                        if !db.fixes.get(id).is_some_and(|v| v.iter().any(|f| math::distance_nm(f.lat, f.lon, la, lo) < 1.0)) {
                            push(&mut db, Fix { ident: id.to_string(), lat: la, lon: lo, region: String::new(), kind: ft::WAYPOINT, freq: None, area: String::new() });
                        }
                    }
                    for name in t[9].split('-') {
                        db.airways.entry(name.to_string()).or_default().push(AirwaySeg { from: t[0].into(), from_region: String::new(), to: t[3].into(), to_region: String::new(), dir: 'N' });
                    }
                    continue;
                }
                if t.len() < 11 {
                    continue;
                }
                let dir = t[6].chars().next().unwrap_or('N');
                for name in t[10].split('-') {
                    db.airways.entry(name.to_string()).or_default().push(AirwaySeg { from: t[0].into(), from_region: t[1].into(), to: t[3].into(), to_region: t[4].into(), dir });
                }
            }
        }
        db
    }

    /// Cykl AIRAC z nagłówka plików (np. „2511”).
    pub fn cycle(&self) -> String {
        self.db().cycle.clone()
    }

    pub fn has_airway(&self, name: &str) -> bool {
        self.db().airways.contains_key(name)
    }

    /// Punkty drogi lotniczej od `from` do `to` (bez `from`, z `to`): (ident, region).
    pub fn airway_path(&self, name: &str, from: &str, to: &str) -> Option<Vec<(String, String)>> {
        let db = self.db();
        let segs = db.airways.get(name)?;
        let mut adj: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        for s in segs {
            if s.dir != 'B' {
                adj.entry(&s.from).or_default().push((&s.to, &s.to_region));
            }
            if s.dir != 'F' {
                adj.entry(&s.to).or_default().push((&s.from, &s.from_region));
            }
        }
        // BFS po grafie drogi
        let mut prev: HashMap<&str, (&str, &str)> = HashMap::new();
        let mut q = std::collections::VecDeque::from([from]);
        let mut seen = std::collections::HashSet::from([from]);
        while let Some(n) = q.pop_front() {
            if n == to {
                break;
            }
            for (m, reg) in adj.get(n).into_iter().flatten() {
                if seen.insert(m) {
                    prev.insert(m, (n, reg));
                    q.push_back(m);
                }
            }
        }
        if !seen.contains(to) {
            return None;
        }
        let mut path = Vec::new();
        let mut cur = to;
        while cur != from {
            let (p, reg) = prev.get(cur)?;
            path.push((cur.to_string(), reg.to_string()));
            cur = p;
        }
        path.reverse();
        Some(path)
    }

    /// Współrzędne punktu po identyfikatorze i regionie (np. z drogi lotniczej).
    pub fn fix_at(&self, ident: &str, region: &str) -> Option<FixResult> {
        self.db().fixes.get(ident)?.iter().find(|f| f.region == region || region.is_empty()).map(to_result)
    }

    fn lookup(&self, ident: &str, region: &str, area: &str) -> Option<(f64, f64)> {
        let db = self.db();
        let list = db.fixes.get(ident)?;
        list.iter()
            .find(|f| f.region == region && (f.area == area || f.area.is_empty()))
            .or_else(|| list.iter().find(|f| f.region == region))
            .or_else(|| list.first())
            .map(|f| (f.lat, f.lon))
    }

    fn parse_cifp(&self, icao: &str) -> NavResult<AirportProcedures> {
        let path = self.root.join("Custom Data").join("CIFP").join(format!("{icao}.dat"));
        let path = if path.exists() { path } else { self.root.join("Resources").join("default data").join("CIFP").join(format!("{icao}.dat")) };
        let text = std::fs::read(&path).map(|b| String::from_utf8_lossy(&b).to_string()).map_err(|_| NavError::not_found(format!("Brak procedur {icao} w danych X-Plane")))?;
        let apt = crate::aero::airports::db().and_then(|d| d.get(icao).cloned());
        let rwy_pos = |id: &str| -> Option<(f64, f64)> {
            let id = id.trim_start_matches("RW");
            let a = apt.as_ref()?;
            a.runways.iter().find_map(|r| {
                if r.le == id && r.has_coords() {
                    Some((r.le_lat, r.le_lon))
                } else if r.he == id && r.has_coords() {
                    Some((r.he_lat, r.he_lon))
                } else {
                    None
                }
            })
        };
        let mut out = AirportProcedures { icao: icao.to_string(), ..Default::default() };
        // klucz: (typ, nazwa, część, przejście) → odcinki, w kolejności z pliku
        let mut parts: Vec<((String, String, char, String), Vec<LegInfo>, Vec<bool>)> = Vec::new();
        for line in text.lines() {
            let Some((head, rest)) = line.split_once(':') else { continue };
            if !matches!(head, "SID" | "STAR" | "APPCH") {
                continue;
            }
            let f: Vec<&str> = rest.split(';').next().unwrap_or("").split(',').collect();
            if f.len() < 29 {
                continue;
            }
            let route = f[1].trim().chars().next().unwrap_or(' ');
            let name = f[2].trim().to_string();
            let trans = f[3].trim().to_string();
            let fix = f[4].trim().to_string();
            let region = f[5].trim();
            let desc = f[8];
            let term = f[11].trim().to_string();
            let part = match (head, route) {
                ("SID", '1' | '4' | 'F' | 'T') => 'R',
                ("SID", '2' | '5' | '8' | 'M') => 'C',
                ("SID", _) => 'E',
                ("STAR", '1' | '4' | '7' | 'F') => 'E',
                ("STAR", '2' | '5' | '8' | 'M') => 'C',
                ("STAR", _) => 'R',
                (_, 'A') => 'E',
                _ => 'C',
            };
            let alt = |s: &str| -> Option<f64> {
                let s = s.trim();
                if let Some(fl) = s.strip_prefix("FL") {
                    fl.parse::<f64>().ok().map(|v| v * 100.0)
                } else {
                    s.parse::<f64>().ok().filter(|v| *v > 0.0)
                }
            };
            let pos = if fix.starts_with("RW") && f[6].trim() == "P" && f[7].trim() == "G" { rwy_pos(&fix) } else if fix.is_empty() { None } else { self.lookup(&fix, region, icao) };
            let leg = LegInfo {
                fix: fix.clone(),
                lat: pos.map(|p| p.0),
                lon: pos.map(|p| p.1),
                leg_type: term,
                alt_desc: Some(f[23].trim().to_string()).filter(|s| !s.is_empty()),
                alt1_ft: alt(f[24]),
                alt2_ft: alt(f[25]),
                speed_kt: f[28].trim().parse::<f64>().ok().filter(|v| *v > 0.0),
                course_deg: f[20].trim().parse::<f64>().ok().map(|v| v / 10.0),
                dist_nm: f[21].trim().parse::<f64>().ok().map(|v| v / 10.0),
                fly_over: desc.chars().nth(1) == Some('Y'),
            };
            let missed = head == "APPCH" && desc.chars().nth(2) == Some('M');
            let key = (head.to_string(), name, part, trans);
            match parts.iter_mut().find(|p| p.0 == key) {
                Some(p) => {
                    p.1.push(leg);
                    p.2.push(missed);
                }
                None => parts.push((key, vec![leg], vec![missed])),
            }
        }
        let rwy_name = |t: &str| -> Vec<String> {
            let r = t.trim_start_matches("RW");
            // RW29B – oba pasy 29L/29R
            if let Some(base) = r.strip_suffix('B') {
                vec![format!("{base}L"), format!("{base}R")]
            } else {
                vec![r.to_string()]
            }
        };
        for ((head, name, part, trans), legs, missed) in parts {
            match head.as_str() {
                "SID" | "STAR" => {
                    let list = if head == "SID" { &mut out.departures } else { &mut out.arrivals };
                    if !list.iter().any(|p| p.name == name) {
                        list.push(TerminalProcedure { name: name.clone(), ..Default::default() });
                    }
                    let p = list.iter_mut().find(|p| p.name == name).unwrap();
                    match part {
                        'R' => {
                            for r in rwy_name(&trans) {
                                p.runway_transitions.push(NamedLegs::new(&r, legs.clone()));
                            }
                        }
                        'C' => p.common_legs = legs,
                        _ => {
                            if trans.starts_with("RW") {
                                for r in rwy_name(&trans) {
                                    p.runway_transitions.push(NamedLegs::new(&r, legs.clone()));
                                }
                            } else {
                                p.enroute_transitions.push(NamedLegs::new(&trans, legs));
                            }
                        }
                    }
                }
                _ => {
                    if !out.approaches.iter().any(|p| p.name == name) {
                        let kind = match name.chars().next() {
                            Some('I') => "ILS",
                            Some('L') => "LOC",
                            Some('R') => "RNAV",
                            Some('D') => "VOR/DME",
                            Some('V') => "VOR",
                            Some('N') => "NDB",
                            Some('Q') => "NDB/DME",
                            Some('B') => "LOC BC",
                            Some('X') => "LDA",
                            _ => "",
                        };
                        let digits: String = name.chars().skip(1).take_while(|c| c.is_ascii_digit()).collect();
                        let rest: String = name.chars().skip(1 + digits.len()).collect();
                        let runway = (!digits.is_empty()).then(|| format!("{digits}{}", rest.chars().next().filter(|c| matches!(c, 'L' | 'R' | 'C')).map(String::from).unwrap_or_default()));
                        out.approaches.push(ApproachProcedure { name: name.clone(), kind: kind.into(), runway, suffix: None, ..Default::default() });
                    }
                    let a = out.approaches.iter_mut().find(|p| p.name == name).unwrap();
                    if part == 'E' {
                        a.transitions.push(NamedLegs::new(&trans, legs));
                    } else {
                        for (l, m) in legs.into_iter().zip(missed) {
                            if m || !a.missed_legs.is_empty() {
                                a.missed_legs.push(l);
                            } else {
                                a.final_legs.push(l);
                            }
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}

/// Pliki X-Plane bywają w Latin-1 (nazwy stacji) – czytaj bez wymagania UTF-8.
fn read(p: &Path) -> String {
    std::fs::read(p).map(|b| String::from_utf8(b.clone()).unwrap_or_else(|_| b.iter().map(|&c| c as char).collect())).unwrap_or_default()
}

/// Numer wersji formatu z drugiej linii („1200 Version …”).
fn version(text: &str) -> u32 {
    text.lines().nth(1).and_then(|l| l.split_whitespace().next()).and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn cycle(text: &str) -> String {
    // „1200 Version - data cycle 2511, build …”
    text.lines().take(3).find_map(|l| l.split("cycle").nth(1).map(|s| s.trim().chars().take_while(|c| c.is_ascii_digit()).collect::<String>())).unwrap_or_default()
}

fn to_result(f: &Fix) -> FixResult {
    FixResult { ident: f.ident.clone(), kind: f.kind.to_string(), lat: f.lat, lon: f.lon, freq_mhz: f.freq, region: Some(f.region.clone()) }
}

impl NavDataProvider for XPlaneNavData {
    fn airport(&self, icao: &str) -> NavResult<AirportInfo> {
        let a = crate::aero::airports::db().and_then(|d| d.get(icao).cloned()).ok_or_else(|| NavError::not_found(format!("Nie znaleziono lotniska {icao}")))?;
        Ok(AirportInfo {
            icao: a.ident.clone(),
            name: a.name.clone(),
            lat: a.lat,
            lon: a.lon,
            elev_ft: a.elev_ft.unwrap_or(0) as f64,
            runways: a
                .runways
                .iter()
                .filter(|r| r.has_coords())
                .flat_map(|r| {
                    [(r.le.clone(), r.le_lat, r.le_lon, r.le_heading()), (r.he.clone(), r.he_lat, r.he_lon, r.he_heading())]
                        .into_iter()
                        .map(|(id, la, lo, h)| RunwayInfo { ident: id, threshold_lat: la, threshold_lon: lo, heading_true_deg: h.unwrap_or(0.0), length_ft: r.length_ft as f64, width_ft: r.width_ft as f64, ..Default::default() })
                        .collect::<Vec<_>>()
                })
                .collect(),
            ..Default::default()
        })
    }

    fn procedures(&self, icao: &str) -> NavResult<AirportProcedures> {
        self.parse_cifp(icao)
    }

    fn find_fix(&self, ident: &str) -> NavResult<Vec<FixResult>> {
        let db = self.db();
        let v: Vec<FixResult> = db.fixes.get(ident).map(|l| l.iter().map(to_result).collect()).unwrap_or_default();
        if v.is_empty() {
            Err(NavError::not_found(format!("Nie znaleziono punktu {ident}")))
        } else {
            Ok(v)
        }
    }

    fn nearby(&self, lat: f64, lon: f64, radius_nm: f64) -> NavResult<Vec<FixResult>> {
        let db = self.db();
        Ok(db.fixes.values().flatten().filter(|f| math::distance_nm(lat, lon, f.lat, f.lon) <= radius_nm).map(to_result).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ot_xp_{}", std::process::id()));
        let d = dir.join("Custom Data");
        std::fs::create_dir_all(d.join("CIFP")).unwrap();
        std::fs::write(d.join("earth_fix.dat"), "I\n1200 Version - data cycle 2511, build 20251030\n 52.300000000   20.900000000 WA531 EPWA EP 2105430\n 51.600000000   20.400000000 BAMSO ENRT EP 2105430\n 51.000000000   20.000000000 ABCDE ENRT EP 2105430\n 50.500000000   19.900000000 FGHIJ ENRT EP 2105430\n99\n").unwrap();
        std::fs::write(d.join("earth_nav.dat"), "I\n1200 Version\n3  52.1    20.7 0 11430 130 5.0 WAR ENRT EP WARSZAWA VOR/DME\n99\n").unwrap();
        std::fs::write(d.join("earth_awy.dat"), "I\n1100 Version\nBAMSO EP 11 ABCDE EP 11 N 2 100 460 UL29\nABCDE EP 11 FGHIJ EP 11 N 2 100 460 UL29-M985\n99\n").unwrap();
        let line = |rt: &str, name: &str, tr: &str, fix: &str, term: &str, alt: &str| {
            let mut f = vec![" "; 40];
            f[0] = "010";
            f[1] = rt;
            f[2] = name;
            f[3] = tr;
            f[4] = fix;
            f[5] = "EP";
            f[6] = "P";
            f[7] = "C";
            f[8] = "E   ";
            f[11] = term;
            f[23] = "+";
            f[24] = alt;
            f.join(",")
        };
        let cifp = [
            format!("SID:{};", line("1", "BAMS1G", "RW29", "WA531", "IF", "03000")),
            format!("SID:{};", line("2", "BAMS1G", "", "BAMSO", "TF", "FL100")),
            format!("APPCH:{};", line("A", "I29", "WAR", "WAR", "IF", "04000")),
            format!("APPCH:{};", line("I", "I29", "", "WA531", "TF", "02000")),
        ]
        .join("\n");
        std::fs::write(d.join("CIFP").join("EPWA.dat"), cifp).unwrap();
        dir
    }

    #[test]
    fn reads_xplane10_formats() {
        let dir = std::env::temp_dir().join(format!("ot_xp10_{}", std::process::id()));
        let d = dir.join("Resources").join("default data");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("earth_fix.dat"), "I\n600 Version - data cycle 1802\n 51.600000  20.400000 BAMSO\n99\n").unwrap();
        std::fs::write(d.join("earth_nav.dat"), "I\n810 Version\n3  52.1 20.7 0 11430 130 5.0 WAR WARSZAWA VOR/DME\n99\n").unwrap();
        std::fs::write(d.join("earth_awy.dat"), "I\n640 Version\nBAMSO 51.6 20.4 KOXAR 51.0 20.0 2 100 460 UL29\n99\n").unwrap();
        let x = XPlaneNavData::new(dir.clone());
        assert!(is_root(&dir));
        assert_eq!(x.cycle(), "1802");
        assert_eq!(x.find_fix("WAR").unwrap()[0].freq_mhz, Some(114.3));
        assert_eq!(x.airway_path("UL29", "BAMSO", "KOXAR").unwrap()[0].0, "KOXAR");
        assert!(x.fix_at("KOXAR", "").is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reads_fixes_airways_and_cifp() {
        let root = sample_root();
        assert!(is_root(&root));
        let x = XPlaneNavData::new(root.clone());
        assert_eq!(x.cycle(), "2511");
        assert_eq!(x.find_fix("WAR").unwrap()[0].freq_mhz, Some(114.3));
        let path = x.airway_path("UL29", "BAMSO", "FGHIJ").unwrap();
        assert_eq!(path.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(), ["ABCDE", "FGHIJ"]);
        assert!(x.airway_path("M985", "FGHIJ", "ABCDE").is_some());
        let p = x.procedures("EPWA").unwrap();
        assert_eq!(p.departures[0].name, "BAMS1G");
        assert_eq!(p.departures[0].runway_transitions[0].name, "29");
        assert_eq!(p.departures[0].common_legs[0].alt1_ft, Some(10_000.0));
        assert_eq!(p.approaches[0].kind, "ILS");
        assert_eq!(p.approaches[0].runway.as_deref(), Some("29"));
        assert_eq!(p.approaches[0].transitions[0].name, "WAR");
        let _ = std::fs::remove_dir_all(root);
    }
}
