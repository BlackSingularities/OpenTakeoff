//! Mapy lotnisk z OpenStreetMap (Overpass API, bez klucza): pasy, drogi kołowania z oznaczeniami, płyty,
//! terminale, hangary, stanowiska, bramki i miejsca oczekiwania przed pasem. Z dróg kołowania i pasów
//! powstaje graf, po którym liczona jest trasa kołowania („A B2 H 27” albo sam cel: pas lub stanowisko)
//! i położenie samolotu („na A, dalej B2”). Ta sama logika jest w aplikacji na Androida (`AirportLayout.kt`).
//!
//! Mapa lotniska jest pobierana raz, gdy mapa jest blisko lotniska, i trzymana na dysku przez 30 dni
//! (potem odświeżana; bez sieci zostaje stara kopia). Naraz jedno pobranie – publiczne serwery Overpass
//! mają limity. Po wczytaniu rośnie [`SERIAL`], a widoki rysują się od nowa.

use std::cmp::Ordering as CmpOrdering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use parking_lot::Mutex;
use serde_json::Value;

use crate::aero::airports::{self, Airport};
use crate::lang::tr;
use crate::navdata::math::bearing_deg;
use crate::trf;

/// (szerokość, długość) w stopniach.
pub type LL = (f64, f64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Runway,
    Taxiway,
    Taxilane,
    Hold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AreaKind {
    Apron,
    Terminal,
    Hangar,
    Runway,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpotKind {
    Hold,
    Stand,
    Gate,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub kind: LineKind,
    /// Oznaczenie drogi („A”, „B2”) albo pasa („09/27”).
    pub code: String,
    pub pts: Vec<LL>,
    pub width_m: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Area {
    pub kind: AreaKind,
    pub name: String,
    pub ring: Vec<LL>,
}

#[derive(Clone, Debug)]
pub struct Spot {
    pub kind: SpotKind,
    pub code: String,
    pub lat: f64,
    pub lon: f64,
}

/// Punkt opisany na trasie: przecięcie pasa (zatrzymaj się przed) albo cel.
#[derive(Clone, Debug)]
pub struct Mark {
    pub lat: f64,
    pub lon: f64,
    pub label: String,
    pub runway: bool,
}

#[derive(Clone, Debug)]
pub struct Route {
    pub pts: Vec<LL>,
    pub length_m: f64,
    /// Kolejne drogi kołowania na trasie (bez powtórzeń).
    pub via: Vec<String>,
    /// Pasy przecinane po drodze – przed każdym trzeba się zatrzymać.
    pub crossings: Vec<Mark>,
    pub end: Option<Mark>,
}

/// Położenie samolotu na lotnisku.
#[derive(Clone, Debug, Default)]
pub struct TaxiInfo {
    pub on: Option<String>,
    pub on_runway: Option<String>,
    /// Najbliższe skrzyżowanie w kierunku jazdy: nazwa i odległość (m).
    pub next: Option<(String, f64)>,
    /// Pas przed samolotem: oznaczenie i odległość (m).
    pub runway_ahead: Option<(String, f64)>,
    pub hold_ahead_m: Option<f64>,
}

const M_PER_DEG: f64 = 111_320.0;

pub fn meters(a: LL, b: LL) -> f64 {
    let dy = (b.0 - a.0) * M_PER_DEG;
    let dx = (b.1 - a.1) * M_PER_DEG * ((a.0 + b.0) / 2.0).to_radians().cos();
    (dx * dx + dy * dy).sqrt()
}

/// Oznaczenie końca pasa bez zer wiodących i przedrostka: „RWY 09L” → „9L”; None, gdy to nie pas.
pub fn runway_key(s: &str) -> Option<String> {
    let s = s.trim().to_uppercase();
    let s = s.strip_prefix("RWY").or_else(|| s.strip_prefix("RW")).unwrap_or(&s).trim();
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    let rest = &s[digits.len()..];
    if digits.is_empty() || digits.len() > 2 || rest.len() > 1 || !(rest.is_empty() || matches!(rest, "L" | "R" | "C" | "W")) {
        return None;
    }
    let n: u32 = digits.parse().ok()?;
    if !(1..=36).contains(&n) {
        return None;
    }
    Some(format!("{n}{rest}"))
}

/// Końce pasa z oznaczenia OSM („09/27”, „09L-27R”).
pub fn runway_ends(code: &str) -> Vec<String> {
    code.split(['/', '-', ' ', ';']).filter_map(runway_key).collect()
}

/// Kąt między kierunkami (0–180°).
fn angle_diff(a: f64, b: f64) -> f64 {
    (((a - b).rem_euclid(360.0) + 180.0).rem_euclid(360.0) - 180.0).abs()
}

fn key(p: LL) -> (i64, i64) {
    ((p.0 * 1e7).round() as i64, (p.1 * 1e7).round() as i64)
}

/// Rzut punktu na odcinek: (punkt na odcinku, odległość w m).
fn project(a: LL, b: LL, p: LL) -> (LL, f64) {
    let k = p.0.to_radians().cos();
    let (ax, ay, bx, by, px, py) = (a.1 * k, a.0, b.1 * k, b.0, p.1 * k, p.0);
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= 0.0 { 0.0 } else { (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0) };
    let q = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    (q, meters(q, p))
}

struct Edge {
    to: usize,
    line: usize,
    m: f64,
}

#[derive(Default)]
struct Graph {
    nodes: Vec<LL>,
    adj: Vec<Vec<Edge>>,
    node_lines: Vec<Vec<usize>>,
    index: HashMap<(i64, i64), usize>,
}

/// Najbliższy odcinek: linia, indeks odcinka, rzut, odległość.
#[derive(Clone, Copy)]
struct Proj {
    line: usize,
    seg: usize,
    p: LL,
    d: f64,
}

enum Dest {
    None,
    Runway(String),
    Stand(usize),
}

#[derive(PartialEq)]
struct Q(f64, usize);
impl Eq for Q {}
impl PartialOrd for Q {
    fn partial_cmp(&self, o: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(o))
    }
}
impl Ord for Q {
    // odwrotnie – BinaryHeap wyjmuje największy, a potrzebny jest najmniejszy koszt
    fn cmp(&self, o: &Self) -> CmpOrdering {
        o.0.total_cmp(&self.0).then_with(|| o.1.cmp(&self.1))
    }
}

pub struct Layout {
    pub ident: String,
    pub lines: Vec<Line>,
    pub areas: Vec<Area>,
    pub spots: Vec<Spot>,
    /// Środek danych – do wyboru mapy lotniska pod samolotem.
    pub center: Option<LL>,
    /// Oznaczenia końców pasów (do przycisków celu trasy).
    pub runway_ends: Vec<String>,
    graph: Graph,
}

impl Layout {
    pub fn new(ident: &str, lines: Vec<Line>, areas: Vec<Area>, spots: Vec<Spot>) -> Self {
        let all: Vec<LL> = lines.iter().flat_map(|l| l.pts.iter().copied()).chain(areas.iter().flat_map(|a| a.ring.iter().copied())).collect();
        let center = (!all.is_empty()).then(|| (all.iter().map(|p| p.0).sum::<f64>() / all.len() as f64, all.iter().map(|p| p.1).sum::<f64>() / all.len() as f64));
        let mut ends: Vec<String> = lines.iter().filter(|l| l.kind == LineKind::Runway).flat_map(|l| runway_ends(&l.code)).collect();
        ends.sort_by_key(|e| (e.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().unwrap_or(0), e.clone()));
        ends.dedup();
        let mut g = Graph::default();
        for (li, l) in lines.iter().enumerate() {
            if l.kind == LineKind::Hold {
                continue;
            }
            let mut prev: Option<usize> = None;
            for &p in &l.pts {
                let n = match g.index.get(&key(p)) {
                    Some(&n) => n,
                    None => {
                        g.nodes.push(p);
                        g.adj.push(Vec::new());
                        g.node_lines.push(Vec::new());
                        let n = g.nodes.len() - 1;
                        g.index.insert(key(p), n);
                        n
                    }
                };
                if !g.node_lines[n].contains(&li) {
                    g.node_lines[n].push(li);
                }
                if let Some(pr) = prev {
                    if pr != n {
                        let m = meters(g.nodes[pr], g.nodes[n]);
                        g.adj[pr].push(Edge { to: n, line: li, m });
                        g.adj[n].push(Edge { to: pr, line: li, m });
                    }
                }
                prev = Some(n);
            }
        }
        Self { ident: ident.to_string(), lines, areas, spots, center, runway_ends: ends, graph: g }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.areas.is_empty() && self.spots.is_empty()
    }

    fn line_name(&self, li: usize) -> String {
        let l = &self.lines[li];
        if l.kind == LineKind::Runway {
            format!("RWY {}", l.code)
        } else {
            l.code.clone()
        }
    }

    fn nearest(&self, p: LL, accept: impl Fn(&Line) -> bool) -> Option<Proj> {
        let mut best: Option<Proj> = None;
        for (li, l) in self.lines.iter().enumerate() {
            if !accept(l) {
                continue;
            }
            for i in 0..l.pts.len().saturating_sub(1) {
                let (q, d) = project(l.pts[i], l.pts[i + 1], p);
                if best.is_none_or(|b| d < b.d) {
                    best = Some(Proj { line: li, seg: i, p: q, d });
                }
            }
        }
        best
    }

    // ---------- trasa kołowania ----------

    /// Trasa od punktu `from` (samolot albo środek mapy). `text`: drogi kołowania po kolei i na końcu cel –
    /// pas („27”, „RWY 09L”) albo stanowisko/bramka; sam cel liczy najkrótszą trasę.
    pub fn route(&self, from: LL, text: &str) -> Result<Route, String> {
        let g = &self.graph;
        if g.nodes.is_empty() {
            return Err(tr("Na tym lotnisku w OpenStreetMap nie ma dróg kołowania.").to_string());
        }
        const FILLER: [&str; 13] = ["VIA", "TWY", "TAXI", "TO", "DO", "PRZEZ", "RWY", "RW", "PAS", "HOLD", "SHORT", "STAND", "GATE"];
        let up = text.to_uppercase();
        let tokens: Vec<String> = up.split(|c: char| c.is_whitespace() || ",;>→".contains(c)).map(|t| t.trim().to_string()).filter(|t| !t.is_empty() && !FILLER.contains(&t.as_str())).collect();
        if tokens.is_empty() {
            return Err(tr("Wpisz drogi kołowania i cel, np. A B2 27.").to_string());
        }
        let twy: HashSet<String> = self.lines.iter().filter(|l| matches!(l.kind, LineKind::Taxiway | LineKind::Taxilane) && !l.code.is_empty()).map(|l| l.code.to_uppercase()).collect();
        let last = tokens.last().cloned().unwrap_or_default();
        let mut via: Vec<String> = tokens.clone();
        let mut dest = Dest::None;
        if let Some(k) = runway_key(&last).filter(|k| self.runway_ends.contains(k)) {
            dest = Dest::Runway(k);
            via.pop();
        } else if !twy.contains(&last) {
            match self.spots.iter().position(|s| s.kind != SpotKind::Hold && s.code.eq_ignore_ascii_case(&last)) {
                Some(i) => {
                    dest = Dest::Stand(i);
                    via.pop();
                }
                None => return Err(trf!("Nie ma drogi, pasa ani stanowiska „{t}”.", t = last)),
            }
        }
        if let Some(bad) = via.iter().find(|t| !twy.contains(*t)) {
            return Err(trf!("Nie ma drogi, pasa ani stanowiska „{t}”.", t = bad));
        }
        if via.is_empty() && matches!(dest, Dest::None) {
            return Err(tr("Wpisz drogi kołowania i cel, np. A B2 27.").to_string());
        }

        let Some(start) = self.nearest(from, |l| l.kind != LineKind::Hold).filter(|s| s.d <= 500.0) else {
            return Err(tr("Punkt startu jest za daleko od dróg kołowania.").to_string());
        };
        let sl = &self.lines[start.line];
        let (Some(&sa), Some(&sb)) = (g.index.get(&key(sl.pts[start.seg])), g.index.get(&key(sl.pts[start.seg + 1]))) else {
            return Err(tr("Na tym lotnisku w OpenStreetMap nie ma dróg kołowania.").to_string());
        };

        let auto = via.is_empty();
        let nl = via.len();
        // Warstwy: 0 – dojazd do pierwszej drogi, 1..nl – kolejne drogi, nl+1 – dojazd z ostatniej drogi do stanowiska
        let layers = if auto { 1 } else { nl + 2 };
        let n = g.nodes.len();
        let mut dist = vec![f64::INFINITY; layers * n];
        let mut prev = vec![usize::MAX; layers * n];
        let mut prev_line = vec![usize::MAX; layers * n];
        let stand_node = match &dest {
            Dest::Stand(i) => {
                let s = &self.spots[*i];
                let p = (s.lat, s.lon);
                let best = (0..n).filter(|&u| g.node_lines[u].iter().any(|&l| self.lines[l].kind != LineKind::Runway)).min_by(|&a, &b| meters(g.nodes[a], p).total_cmp(&meters(g.nodes[b], p)));
                match best.filter(|&u| meters(g.nodes[u], p) < 300.0) {
                    Some(u) => Some(u),
                    None => return Err(trf!("Nie da się dojechać do stanowiska {code}.", code = s.code)),
                }
            }
            _ => None,
        };
        let touches = |u: usize, code: &str| g.node_lines[u].iter().any(|&l| self.lines[l].kind != LineKind::Runway && self.lines[l].code.eq_ignore_ascii_case(code));
        let touches_runway = |u: usize, k: &str| g.node_lines[u].iter().any(|&l| self.lines[l].kind == LineKind::Runway && runway_ends(&self.lines[l].code).iter().any(|e| e == k));
        let cost = |layer: usize, e: &Edge| -> Option<f64> {
            let l = &self.lines[e.line];
            if auto {
                return match l.kind {
                    LineKind::Runway => Some(e.m * 6.0),
                    LineKind::Taxiway | LineKind::Taxilane => Some(e.m),
                    LineKind::Hold => None,
                };
            }
            if layer == 0 || layer == nl + 1 {
                match l.kind {
                    LineKind::Runway | LineKind::Hold => None,
                    _ if l.code.is_empty() || l.kind == LineKind::Taxilane => Some(e.m),
                    _ => Some(e.m * 3.0),
                }
            } else if l.kind != LineKind::Runway && l.code.eq_ignore_ascii_case(&via[layer - 1]) {
                Some(e.m)
            } else {
                None
            }
        };

        let mut heap = BinaryHeap::new();
        let mut relax = |dist: &mut Vec<f64>, heap: &mut BinaryHeap<Q>, state: usize, d: f64, src: usize, line: usize| {
            if d < dist[state] {
                dist[state] = d;
                prev[state] = src;
                prev_line[state] = line;
                heap.push(Q(d, state));
            }
        };
        relax(&mut dist, &mut heap, sa, meters(start.p, g.nodes[sa]), usize::MAX, start.line);
        relax(&mut dist, &mut heap, sb, meters(start.p, g.nodes[sb]), usize::MAX, start.line);
        let mut found: Option<usize> = None;
        while let Some(Q(d, s)) = heap.pop() {
            if d > dist[s] {
                continue;
            }
            let (layer, u) = (s / n, s % n);
            let done = match &dest {
                Dest::Runway(k) => (auto || layer == nl) && touches_runway(u, k),
                Dest::Stand(_) => Some(u) == stand_node && (auto || layer >= nl),
                Dest::None => false,
            };
            if done {
                found = Some(s);
                break;
            }
            if !auto {
                if layer < nl && touches(u, &via[layer]) {
                    relax(&mut dist, &mut heap, (layer + 1) * n + u, d, s, usize::MAX);
                }
                if layer == nl && matches!(dest, Dest::Stand(_)) {
                    relax(&mut dist, &mut heap, (nl + 1) * n + u, d, s, usize::MAX);
                }
            }
            for e in &g.adj[u] {
                if let Some(c) = cost(layer, e) {
                    relax(&mut dist, &mut heap, layer * n + e.to, d + c, s, e.line);
                }
            }
        }
        drop(relax);
        if found.is_none() && matches!(dest, Dest::None) {
            // Bez celu: do końca ostatniej drogi (najdalszy osiągalny punkt na niej)
            found = (0..n).map(|u| nl * n + u).filter(|&s| dist[s].is_finite()).max_by(|&a, &b| dist[a].total_cmp(&dist[b]));
        }
        let Some(found) = found else {
            return Err(match &dest {
                Dest::Runway(k) => trf!("Tą trasą nie da się dojechać do pasa {k}."),
                Dest::Stand(i) => trf!("Nie da się dojechać do stanowiska {code}.", code = self.spots[*i].code),
                Dest::None => tr("Nie da się wyznaczyć trasy tymi drogami.").to_string(),
            });
        };

        // Odtworzenie: (węzeł, linia, którą do niego dojechano) od startu do celu
        let mut chain: Vec<(usize, usize)> = Vec::new();
        let mut s = found;
        loop {
            let u = s % n;
            if chain.last().is_some_and(|l| l.0 == u) {
                if prev_line[s] != usize::MAX {
                    if let Some(last) = chain.last_mut() {
                        last.1 = prev_line[s];
                    }
                }
            } else {
                chain.push((u, prev_line[s]));
            }
            if prev[s] == usize::MAX {
                break;
            }
            s = prev[s];
        }
        chain.reverse();
        let mut pts: Vec<LL> = vec![start.p];
        pts.extend(chain.iter().map(|&(u, _)| g.nodes[u]));
        if let Dest::Stand(i) = &dest {
            pts.push((self.spots[*i].lat, self.spots[*i].lon));
        }
        let length_m: f64 = pts.windows(2).map(|w| meters(w[0], w[1])).sum();
        let mut via_out: Vec<String> = Vec::new();
        for &(_, li) in &chain {
            if li == usize::MAX {
                continue;
            }
            let l = &self.lines[li];
            if l.kind == LineKind::Runway || l.code.is_empty() {
                continue;
            }
            if via_out.last() != Some(&l.code) {
                via_out.push(l.code.clone());
            }
        }
        // Znacznik ok. 50 m przed osią pasa, po trasie
        let back_off = |idx: usize, m: f64| -> LL {
            let mut left = m;
            let mut i = idx + 1; // indeks w pts (pts[0] = rzut startu)
            while i > 0 {
                let (a, b) = (pts[i], pts[i - 1]);
                let seg = meters(a, b);
                if seg >= left && seg > 0.0 {
                    let f = left / seg;
                    return (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
                }
                left -= seg;
                i -= 1;
            }
            pts[0]
        };
        let mut crossings: Vec<Mark> = Vec::new();
        let end_idx = if matches!(dest, Dest::Runway(_)) { chain.len() - 1 } else { chain.len() };
        let start_on_runway = sl.kind == LineKind::Runway;
        for (i, &(u, _)) in chain.iter().enumerate().take(end_idx) {
            if i == 0 && start_on_runway {
                continue;
            }
            let Some(&rw) = g.node_lines[u].iter().find(|&&l| self.lines[l].kind == LineKind::Runway) else { continue };
            let code = self.lines[rw].code.clone();
            if crossings.last().is_some_and(|m| m.label == code && meters((m.lat, m.lon), g.nodes[u]) < 250.0) {
                continue;
            }
            let p = back_off(i, 50.0);
            crossings.push(Mark { lat: p.0, lon: p.1, label: code, runway: true });
        }
        let end = match &dest {
            Dest::Runway(k) => {
                let p = back_off(chain.len() - 1, 50.0);
                Mark { lat: p.0, lon: p.1, label: k.clone(), runway: true }
            }
            Dest::Stand(i) => Mark { lat: self.spots[*i].lat, lon: self.spots[*i].lon, label: self.spots[*i].code.clone(), runway: false },
            Dest::None => {
                let p = *pts.last().unwrap_or(&start.p);
                Mark { lat: p.0, lon: p.1, label: via_out.last().cloned().unwrap_or_default(), runway: false }
            }
        };
        Ok(Route { pts, length_m, via: via_out, crossings, end: Some(end) })
    }

    // ---------- położenie samolotu ----------

    /// Gdzie jest samolot: droga pod nim, następne skrzyżowanie w kierunku jazdy, pas i miejsce oczekiwania przed nim.
    pub fn taxi_info(&self, lat: f64, lon: f64, hdg_true: f64) -> Option<TaxiInfo> {
        let g = &self.graph;
        if g.nodes.is_empty() {
            return None;
        }
        let p = (lat, lon);
        let on_runway = self.nearest(p, |l| l.kind == LineKind::Runway).filter(|r| r.d < (self.lines[r.line].width_m.unwrap_or(45.0) / 2.0).max(15.0));
        let twy = self.nearest(p, |l| matches!(l.kind, LineKind::Taxiway | LineKind::Taxilane));
        let hold = self.hold_ahead(p, hdg_true);
        let Some(cur) = on_runway.or(twy.filter(|t| t.d < 35.0)) else {
            return Some(TaxiInfo { hold_ahead_m: hold, ..Default::default() });
        };
        let code = self.lines[cur.line].code.clone();
        let mut li = cur.line;
        let mut pts = &self.lines[li].pts;
        let (a, b) = (pts[cur.seg], pts[cur.seg + 1]);
        let mut forward = angle_diff(bearing_deg(a.0, a.1, b.0, b.1), hdg_true) <= 90.0;
        let mut j = if forward { cur.seg + 1 } else { cur.seg };
        let mut d = meters(cur.p, pts[j]);
        let mut next: Option<(String, f64)> = None;
        let mut rwy_ahead: Option<(String, f64)> = None;
        let mut hops = 0;
        while d < 2500.0 {
            let u = g.index.get(&key(pts[j])).copied();
            if let Some(u) = u.filter(|_| d > 3.0) {
                let others: Vec<usize> = g.node_lines[u].iter().copied().filter(|&o| o != li && !self.lines[o].code.is_empty() && !self.lines[o].code.eq_ignore_ascii_case(&code) && self.lines[o].kind != LineKind::Hold).collect();
                if !others.is_empty() {
                    if let Some(&rw) = others.iter().find(|&&o| self.lines[o].kind == LineKind::Runway) {
                        if on_runway.is_none() && rwy_ahead.is_none() {
                            rwy_ahead = Some((self.lines[rw].code.clone(), d));
                        }
                    }
                    if next.is_none() {
                        let mut names: Vec<String> = others.iter().map(|&o| self.line_name(o)).collect();
                        names.dedup();
                        next = Some((names.join(" / "), d));
                    }
                    if rwy_ahead.is_some() || d > 600.0 {
                        break;
                    }
                }
            }
            let step_ok = if forward { j + 1 < pts.len() } else { j > 0 };
            if !step_ok {
                // Koniec kawałka drogi w OSM: dalej ta sama droga zaczynająca się w tym węźle
                let Some(u) = u else { break };
                if code.is_empty() || hops >= 8 {
                    break;
                }
                let here = pts[j];
                let Some(&cont) = g.node_lines[u].iter().find(|&&o| o != li && self.lines[o].code.eq_ignore_ascii_case(&code) && (self.lines[o].pts.first() == Some(&here) || self.lines[o].pts.last() == Some(&here))) else { break };
                hops += 1;
                li = cont;
                pts = &self.lines[li].pts;
                forward = pts.first() == Some(&here);
                j = if forward { 0 } else { pts.len() - 1 };
                if pts.len() < 2 {
                    break;
                }
            }
            let nj = if forward { j + 1 } else { j - 1 };
            d += meters(pts[j], pts[nj]);
            j = nj;
        }
        Some(TaxiInfo {
            on: if on_runway.is_some() { None } else { Some(self.line_name(cur.line)).filter(|s| !s.is_empty()) },
            on_runway: on_runway.map(|r| self.lines[r.line].code.clone()),
            next,
            runway_ahead: rwy_ahead,
            hold_ahead_m: hold,
        })
    }

    /// Miejsce oczekiwania przed pasem do 80 m przed samolotem.
    fn hold_ahead(&self, p: LL, hdg: f64) -> Option<f64> {
        self.spots
            .iter()
            .filter(|s| s.kind == SpotKind::Hold)
            .map(|s| (s, meters(p, (s.lat, s.lon))))
            .filter(|(s, d)| (4.0..=80.0).contains(d) && angle_diff(bearing_deg(p.0, p.1, s.lat, s.lon), hdg) < 50.0)
            .map(|(_, d)| d)
            .min_by(|a, b| a.total_cmp(b))
    }
}

impl Route {
    /// Pozostała długość trasy od samolotu (m) albo None, gdy samolot jest daleko od niej.
    pub fn remaining_from(&self, lat: f64, lon: f64) -> Option<f64> {
        if self.pts.len() < 2 {
            return None;
        }
        let p = (lat, lon);
        let (i, q, d) = (0..self.pts.len() - 1)
            .map(|i| {
                let (q, d) = project(self.pts[i], self.pts[i + 1], p);
                (i, q, d)
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))?;
        if d > 120.0 {
            return None;
        }
        Some(meters(q, self.pts[i + 1]) + self.pts[i + 1..].windows(2).map(|w| meters(w[0], w[1])).sum::<f64>())
    }
}

// ---------- odpowiedź Overpass ----------

fn geometry(o: &Value) -> Vec<LL> {
    o.get("geometry")
        .and_then(|g| g.as_array())
        .map(|a| a.iter().filter_map(|p| Some((p.get("lat")?.as_f64()?, p.get("lon")?.as_f64()?))).collect())
        .unwrap_or_default()
}

fn width_of(tags: &Value) -> Option<f64> {
    let w = tags.get("width")?.as_str()?.trim().replace(',', ".");
    let num: String = w.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    num.parse().ok()
}

fn mid(pts: &[LL]) -> LL {
    let n = pts.len().max(1) as f64;
    (pts.iter().map(|p| p.0).sum::<f64>() / n, pts.iter().map(|p| p.1).sum::<f64>() / n)
}

/// Łączy kawałki obrysu wielokąta (drogi relacji) w zamknięte pierścienie.
fn join_rings(parts: Vec<Vec<LL>>) -> Vec<Vec<LL>> {
    let mut rings = Vec::new();
    let mut open: Vec<Vec<LL>> = Vec::new();
    for p in parts {
        if p.len() >= 4 && p.first() == p.last() {
            rings.push(p);
        } else {
            open.push(p);
        }
    }
    while !open.is_empty() {
        let mut cur = open.remove(0);
        loop {
            if cur.first() == cur.last() {
                break;
            }
            let end = *cur.last().unwrap();
            let Some(i) = open.iter().position(|o| o.first() == Some(&end) || o.last() == Some(&end)) else { break };
            let mut p = open.remove(i);
            if p.first() != Some(&end) {
                p.reverse();
            }
            cur.extend(p.into_iter().skip(1));
        }
        if cur.len() >= 4 && cur.first() == cur.last() {
            rings.push(cur);
        }
    }
    rings
}

/// Odpowiedź Overpass API (`out geom`) → mapa lotniska.
pub fn parse(ident: &str, text: &str) -> Result<Layout, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let mut lines = Vec::new();
    let mut areas = Vec::new();
    let mut spots = Vec::new();
    let empty = Vec::new();
    let els = root.get("elements").and_then(|e| e.as_array()).unwrap_or(&empty);
    let area_kind = |a: &str| match a {
        "apron" => Some(AreaKind::Apron),
        "terminal" => Some(AreaKind::Terminal),
        "hangar" => Some(AreaKind::Hangar),
        _ => None,
    };
    for o in els {
        let Some(tags) = o.get("tags") else { continue };
        let Some(aeroway) = tags.get("aeroway").and_then(|v| v.as_str()) else { continue };
        let code = tags.get("ref").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let name = tags.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let code_or_name = if code.is_empty() { name.clone() } else { code.clone() };
        match o.get("type").and_then(|v| v.as_str()) {
            Some("node") => {
                let (Some(lat), Some(lon)) = (o.get("lat").and_then(|v| v.as_f64()), o.get("lon").and_then(|v| v.as_f64())) else { continue };
                let kind = match aeroway {
                    "holding_position" => SpotKind::Hold,
                    "parking_position" => SpotKind::Stand,
                    "gate" => SpotKind::Gate,
                    _ => continue,
                };
                let code = if kind == SpotKind::Hold { code } else { code_or_name };
                spots.push(Spot { kind, code, lat, lon });
            }
            Some("way") => {
                let pts = geometry(o);
                if pts.len() < 2 {
                    continue;
                }
                let closed = pts.len() >= 4 && pts.first() == pts.last();
                let area = closed && tags.get("area").and_then(|v| v.as_str()) == Some("yes");
                let m = mid(&pts);
                match aeroway {
                    "runway" if area => areas.push(Area { kind: AreaKind::Runway, name: code, ring: pts }),
                    "runway" => lines.push(Line { kind: LineKind::Runway, code, pts, width_m: width_of(tags) }),
                    "taxiway" if area => areas.push(Area { kind: AreaKind::Apron, name: String::new(), ring: pts }),
                    "taxiway" => lines.push(Line { kind: LineKind::Taxiway, code, pts, width_m: width_of(tags) }),
                    "taxilane" => lines.push(Line { kind: LineKind::Taxilane, code, pts, width_m: width_of(tags) }),
                    "holding_position" => {
                        spots.push(Spot { kind: SpotKind::Hold, code: code.clone(), lat: m.0, lon: m.1 });
                        lines.push(Line { kind: LineKind::Hold, code, pts, width_m: None });
                    }
                    "parking_position" => spots.push(Spot { kind: SpotKind::Stand, code: code_or_name, lat: m.0, lon: m.1 }),
                    a => {
                        if let Some(k) = area_kind(a).filter(|_| closed) {
                            areas.push(Area { kind: k, name, ring: pts });
                        }
                    }
                }
            }
            Some("relation") => {
                let Some(k) = area_kind(aeroway) else { continue };
                let parts: Vec<Vec<LL>> = o
                    .get("members")
                    .and_then(|m| m.as_array())
                    .map(|ms| {
                        ms.iter()
                            .filter(|m| m.get("type").and_then(|v| v.as_str()) == Some("way") && m.get("role").and_then(|v| v.as_str()).unwrap_or("outer") != "inner")
                            .map(geometry)
                            .filter(|g| g.len() >= 2)
                            .collect()
                    })
                    .unwrap_or_default();
                for ring in join_rings(parts) {
                    areas.push(Area { kind: k, name: name.clone(), ring });
                }
            }
            _ => {}
        }
    }
    Ok(Layout::new(ident, lines, areas, spots))
}

/// Zapytanie Overpass API o obiekty lotniska w prostokącie (s, w, n, e).
pub fn query(s: f64, w: f64, n: f64, e: f64) -> String {
    let b = format!("{s:.5},{w:.5},{n:.5},{e:.5}");
    format!(
        "[out:json][timeout:40];(way[\"aeroway\"~\"^(runway|taxiway|taxilane|apron|terminal|hangar|holding_position|parking_position)$\"]({b});node[\"aeroway\"~\"^(holding_position|parking_position|gate)$\"]({b});relation[\"aeroway\"~\"^(apron|terminal|hangar)$\"]({b}););out geom qt;"
    )
}

// ---------- pobieranie i pamięć ----------

/// Rośnie, gdy wczyta się nowa mapa lotniska.
pub static SERIAL: AtomicU64 = AtomicU64::new(0);

const ENDPOINTS: [&str; 2] = ["https://overpass-api.de/api/interpreter", "https://overpass.private.coffee/api/interpreter"];
const TTL: Duration = Duration::from_secs(30 * 24 * 3600);
const RETRY: Duration = Duration::from_secs(300);
const KEEP: usize = 6;

#[derive(Default)]
struct Store {
    mem: Vec<Arc<Layout>>,
    pending: HashSet<String>,
    failed: HashMap<String, Instant>,
}

fn store() -> &'static Mutex<Store> {
    static S: OnceLock<Mutex<Store>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Store::default()))
}

static NET: Mutex<()> = Mutex::new(());

/// Prostokąt (s, w, n, e) obejmujący pasy lotniska z zapasem na płyty i terminale.
fn bbox(a: &Airport) -> (f64, f64, f64, f64) {
    let (mut s, mut n, mut w, mut e) = (a.lat, a.lat, a.lon, a.lon);
    for r in a.runways.iter().filter(|r| r.has_coords()) {
        for (la, lo) in [(r.le_lat, r.le_lon), (r.he_lat, r.he_lon)] {
            s = s.min(la);
            n = n.max(la);
            w = w.min(lo);
            e = e.max(lo);
        }
    }
    let k = a.lat.to_radians().cos().max(0.05);
    let (pad_la, pad_lo) = (1200.0 / M_PER_DEG, 1200.0 / (M_PER_DEG * k));
    let (min_la, min_lo) = (1500.0 / M_PER_DEG, 1500.0 / (M_PER_DEG * k));
    ((s - pad_la).min(a.lat - min_la), (w - pad_lo).min(a.lon - min_lo), (n + pad_la).max(a.lat + min_la), (e + pad_lo).max(a.lon + min_lo))
}

fn download(q: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(60)).user_agent(&format!("OpenTakeoff-Windows/{}", crate::version())).build();
    let mut last = String::from("Overpass");
    for url in ENDPOINTS {
        match agent.post(url).send_form(&[("data", q)]) {
            Ok(resp) => match resp.into_string() {
                Ok(body) if body.trim_start().starts_with('{') => return Ok(body),
                Ok(_) => last = "Overpass: brak JSON".to_string(),
                Err(e) => last = e.to_string(),
            },
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

fn load(a: &Airport) -> Option<String> {
    let dir = crate::paths::cache().join("airport-layouts");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(format!("{}.json", a.ident));
    let fresh = std::fs::metadata(&file).ok().and_then(|m| m.modified().ok()).and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age < TTL);
    if fresh {
        if let Ok(t) = std::fs::read_to_string(&file) {
            return Some(t);
        }
    }
    let got = {
        let _g = NET.lock();
        let (s, w, n, e) = bbox(a);
        download(&query(s, w, n, e))
    };
    match got {
        Ok(body) => {
            let tmp = file.with_extension("json.part");
            if std::fs::write(&tmp, &body).is_ok() {
                let _ = std::fs::rename(&tmp, &file);
            }
            Some(body)
        }
        Err(e) => {
            crate::log::warn(&format!("Mapa lotniska {}: {e}", a.ident));
            std::fs::read_to_string(&file).ok()
        }
    }
}

/// Zamawia mapę lotniska (z dysku albo z sieci) w tle, jeśli jeszcze jej nie ma.
pub fn ensure(a: &Airport) {
    {
        let mut st = store().lock();
        if st.mem.iter().any(|l| l.ident == a.ident) || st.pending.contains(&a.ident) {
            return;
        }
        if st.failed.get(&a.ident).is_some_and(|t| t.elapsed() < RETRY) {
            return;
        }
        st.pending.insert(a.ident.clone());
    }
    let a = a.clone();
    let _ = std::thread::Builder::new().name("airport-layout".into()).spawn(move || {
        let parsed = load(&a).and_then(|t| parse(&a.ident, &t).ok());
        let mut st = store().lock();
        st.pending.remove(&a.ident);
        match parsed {
            Some(l) => {
                st.mem.retain(|m| m.ident != a.ident);
                st.mem.push(Arc::new(l));
                while st.mem.len() > KEEP {
                    st.mem.remove(0);
                }
                SERIAL.fetch_add(1, Ordering::SeqCst);
            }
            None => {
                st.failed.insert(a.ident.clone(), Instant::now());
            }
        }
    });
}

/// Mapa blisko ziemi: zamawia mapy lotnisk w pobliżu punktu (najwyżej trzech).
pub fn request_near(lat: f64, lon: f64, zoom: f64) {
    if zoom < 12.5 {
        return;
    }
    let Some(db) = airports::db() else { return };
    let radius = if zoom >= 14.0 { 1.6 } else { 3.2 };
    for (_, a) in db.within(lat, lon, radius, false).into_iter().filter(|(_, a)| a.kind <= 2 && !a.runways.is_empty()).take(3) {
        ensure(a);
    }
}

/// Wczytane mapy lotnisk.
pub fn all() -> Vec<Arc<Layout>> {
    store().lock().mem.clone()
}

/// Mapa lotniska, na którym leży punkt (do ok. 6 km od środka danych).
pub fn at(lat: f64, lon: f64) -> Option<Arc<Layout>> {
    store()
        .lock()
        .mem
        .iter()
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.center.map(|c| (meters(c, (lat, lon)), l)))
        .filter(|(d, _)| *d < 6000.0)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, l)| l.clone())
}

/// Czy jakaś mapa lotniska jest właśnie pobierana.
pub fn loading() -> bool {
    !store().lock().pending.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mały, zmyślony układ: pas 09/27 z zachodu na wschód, równoległa droga A na południe od niego,
    /// łączniki B (zachód) i C (wschód) z A do pasa, stanowisko S1 na końcu krótkiej drogi bez nazwy od A.
    fn sample() -> Layout {
        let y0 = 50.0;
        let ya = 49.997;
        let line = |kind, code: &str, pts: Vec<LL>| Line { kind, code: code.into(), pts, width_m: None };
        Layout::new(
            "TEST",
            vec![
                line(LineKind::Runway, "09/27", vec![(y0, 20.0), (y0, 20.01), (y0, 20.02)]),
                line(LineKind::Taxiway, "A", vec![(ya, 20.0), (ya, 20.005), (ya, 20.01), (ya, 20.02)]),
                line(LineKind::Taxiway, "B", vec![(ya, 20.0), (y0, 20.0)]),
                line(LineKind::Taxiway, "C", vec![(ya, 20.02), (y0, 20.02)]),
                line(LineKind::Taxilane, "", vec![(ya, 20.005), (49.995, 20.005)]),
            ],
            vec![],
            vec![Spot { kind: SpotKind::Stand, code: "S1".into(), lat: 49.995, lon: 20.005 }, Spot { kind: SpotKind::Hold, code: String::new(), lat: 49.9985, lon: 20.0 }],
        )
    }

    #[test]
    fn runway_keys() {
        assert_eq!(runway_key("09").as_deref(), Some("9"));
        assert_eq!(runway_key("RWY27L").as_deref(), Some("27L"));
        assert_eq!(runway_key("A"), None);
        assert_eq!(runway_key("37"), None);
        assert_eq!(runway_ends("09L/27R"), vec!["9L".to_string(), "27R".to_string()]);
    }

    #[test]
    fn route_by_taxiways_to_runway() {
        let l = sample();
        // od stanowiska drogą A i B do pasa 09
        let r = l.route((49.995, 20.005), "A B 09").expect("trasa");
        assert_eq!(r.via, vec!["A".to_string(), "B".to_string()]);
        assert!(r.end.as_ref().is_some_and(|e| e.runway && e.label == "9"));
        assert!(r.length_m > 500.0 && r.length_m < 1200.0, "{}", r.length_m);
        // sam cel: najkrótsza trasa
        let r2 = l.route((49.995, 20.005), "27").expect("trasa");
        assert!(r2.via.contains(&"A".to_string()));
        // nieznana droga
        assert!(l.route((49.995, 20.005), "X 27").is_err());
    }

    #[test]
    fn route_to_stand_and_position() {
        let l = sample();
        let r = l.route((50.0, 20.02), "C A S1").expect("trasa");
        assert_eq!(r.via.first().map(|s| s.as_str()), Some("C"));
        let rem = r.remaining_from(50.0, 20.02).expect("na trasie");
        assert!((rem - r.length_m).abs() < 5.0);
        // na A jadąc na zachód: dalej skrzyżowanie z B (a za nim pas)
        let t = l.taxi_info(49.997, 20.008, 270.0).expect("info");
        assert_eq!(t.on.as_deref(), Some("A"));
        assert!(t.next.as_ref().is_some_and(|(n, _)| n == "B"), "{:?}", t.next);
    }

    #[test]
    fn parse_overpass() {
        let json = r#"{"elements":[
            {"type":"way","id":1,"tags":{"aeroway":"taxiway","ref":"A"},"geometry":[{"lat":50.0,"lon":20.0},{"lat":50.0,"lon":20.01}]},
            {"type":"node","id":2,"lat":50.001,"lon":20.0,"tags":{"aeroway":"parking_position","ref":"12"}},
            {"type":"way","id":3,"tags":{"aeroway":"apron"},"geometry":[{"lat":50.0,"lon":20.0},{"lat":50.001,"lon":20.0},{"lat":50.001,"lon":20.001},{"lat":50.0,"lon":20.0}]},
            {"type":"relation","id":4,"tags":{"aeroway":"terminal","name":"T1"},"members":[
                {"type":"way","role":"outer","geometry":[{"lat":50.0,"lon":20.0},{"lat":50.0,"lon":20.002}]},
                {"type":"way","role":"outer","geometry":[{"lat":50.001,"lon":20.0},{"lat":50.0,"lon":20.0}]},
                {"type":"way","role":"outer","geometry":[{"lat":50.0,"lon":20.002},{"lat":50.001,"lon":20.0}]}]}
        ]}"#;
        let l = parse("TEST", json).expect("parse");
        assert_eq!(l.lines.len(), 1);
        assert_eq!(l.spots.len(), 1);
        assert_eq!(l.areas.len(), 2);
        assert!(l.areas.iter().any(|a| a.kind == AreaKind::Terminal && a.name == "T1"));
    }
}
