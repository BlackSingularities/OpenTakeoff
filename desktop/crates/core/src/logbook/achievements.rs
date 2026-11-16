//! Osiągnięcia i wyzwania tygodnia liczone lokalnie z dziennika – bez kont i rankingów w sieci.
//!
//! Osiągnięcia mają poziomy (np. 1, 10, 50, 100 lotów); wyzwania tygodnia są wybierane z listy
//! na podstawie numeru tygodnia ISO, więc każdy ma te same i można je sprawdzić także wstecz.

use std::collections::{BTreeMap, HashSet};

use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Timelike};

use super::model::{FlightLog, LandingReport};
use crate::lang::tr;
use crate::trf;

/// Rodzaj ikony (w interfejsie zamieniany na obrazek).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Plane,
    Clock,
    Route,
    Landing,
    Pin,
    Globe,
    Gauge,
    Sparkles,
    Star,
    Activity,
    Wind,
    Timer,
    Mountain,
    Ruler,
    Moon,
    Circuit,
    Crosshair,
    Scan,
    Trophy,
}

#[derive(Clone, Debug)]
pub struct Badge {
    pub id: &'static str,
    pub title: String,
    /// Co zrobić, żeby zdobyć następny poziom (albo „wszystkie poziomy zdobyte”).
    pub desc: String,
    pub icon: Icon,
    /// Zdobyte poziomy (0 – jeszcze nic).
    pub level: usize,
    pub levels: usize,
    /// Postęp do następnego poziomu (1 po zdobyciu wszystkich).
    pub progress: f32,
    /// „37 / 50”.
    pub value: String,
}

#[derive(Clone, Debug)]
pub struct Challenge {
    /// Stały klucz: „2026-W47-apt”.
    pub key: String,
    pub title: String,
    pub desc: String,
    pub icon: Icon,
    /// Lotnisko wyzwania (do briefingu).
    pub airport: Option<String>,
    pub done: bool,
    pub progress: f32,
    pub status: String,
    /// Lot, w którym wyzwanie zaliczono.
    pub flight_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub badges: Vec<Badge>,
    pub week: Vec<Challenge>,
    /// „Tydzień 47 · 16–22.11”.
    pub week_title: String,
    /// „zostało 3 dni”.
    pub week_left: String,
    pub unlocked: usize,
    pub total_levels: usize,
}

impl Summary {
    /// Klucze zdobytych poziomów i zaliczonych wyzwań (do powiadomień o nowych).
    pub fn keys(&self) -> Vec<String> {
        let mut k: Vec<String> = self.badges.iter().flat_map(|b| (1..=b.level).map(move |l| format!("{}:{l}", b.id))).collect();
        k.extend(self.week.iter().filter(|c| c.done).map(|c| c.key.clone()));
        k
    }
}

/// Trudne i ciekawe lotniska do wyzwań tygodnia (ICAO, nazwa).
const AIRPORTS: &[(&str, &str)] = &[
    ("LPMA", "Madera"),
    ("LOWI", "Innsbruck"),
    ("LXGB", "Gibraltar"),
    ("NZQN", "Queenstown"),
    ("KASE", "Aspen"),
    ("EGLC", "London City"),
    ("LSGS", "Sion"),
    ("TNCM", "Sint Maarten"),
    ("VQPR", "Paro"),
    ("SAWH", "Ushuaia"),
    ("PAJN", "Juneau"),
    ("ENSB", "Svalbard"),
    ("LGSK", "Skiathos"),
    ("BIIS", "Ísafjörður"),
    ("LFMN", "Nicea"),
    ("EPZP", "Zielona Góra"),
    ("LIRN", "Neapol"),
    ("EKVG", "Vágar"),
    ("LOWS", "Salzburg"),
    ("SBRJ", "Rio de Janeiro Santos Dumont"),
];

fn airports() -> &'static [(&'static str, &'static str)] {
    AIRPORTS
}

const SKILLS: usize = 7;

fn landing_time(f: &FlightLog, l: &LandingReport) -> i64 {
    l.at.unwrap_or(f.end)
}

fn local(ms: i64) -> Option<chrono::DateTime<Local>> {
    Local.timestamp_millis_opt(ms).single()
}

/// Poniedziałek tygodnia ISO, w którym wypada chwila `ms`.
fn monday_of(ms: i64) -> Option<NaiveDate> {
    let d = local(ms)?.date_naive();
    Some(d - Duration::days(d.weekday().num_days_from_monday() as i64))
}

fn week_seed(monday: NaiveDate) -> usize {
    let w = monday.iso_week();
    (w.year() as usize) * 53 + w.week() as usize
}

fn tiers(id: &'static str) -> &'static [f64] {
    match id {
        "flights" => &[1.0, 10.0, 50.0, 100.0, 250.0, 500.0],
        "hours" => &[1.0, 10.0, 50.0, 100.0, 250.0, 500.0],
        "distance" => &[500.0, 2500.0, 10_000.0, 25_000.0, 50_000.0, 100_000.0],
        "landings" => &[1.0, 25.0, 100.0, 250.0, 500.0],
        "airports" => &[5.0, 15.0, 30.0, 60.0, 100.0],
        "countries" => &[2.0, 5.0, 10.0, 20.0, 40.0],
        "aircraft" => &[2.0, 5.0, 10.0, 20.0],
        "butter" => &[1.0, 10.0, 25.0, 50.0],
        "great" => &[1.0, 10.0, 25.0, 50.0, 100.0],
        "streak" => &[3.0, 5.0, 10.0, 20.0],
        "crosswind" => &[10.0, 15.0, 20.0, 25.0],
        "long" => &[2.0, 4.0, 8.0, 12.0],
        "high" => &[10_000.0, 25_000.0, 35_000.0, 41_000.0],
        "short" => &[1.0, 5.0, 20.0],
        "night" => &[1.0, 10.0, 25.0],
        "circuits" => &[1.0, 5.0, 20.0],
        "centre" => &[1.0, 10.0, 25.0],
        "tdz" => &[1.0, 10.0, 50.0],
        _ => &[1.0, 5.0, 15.0, 30.0],
    }
}

fn num(v: f64) -> String {
    // Spacja jako separator tysięcy, jak w reszcie programu
    let s = format!("{:.0}", v.floor());
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('\u{2009}');
        }
        out.push(c);
    }
    out
}

fn value_text(id: &str, v: f64) -> String {
    match id {
        "hours" | "long" => format!("{} h", num(v)),
        "distance" => format!("{} NM", num(v)),
        "crosswind" => format!("{} kt", num(v)),
        "high" => format!("FL{:03.0}", (v / 100.0).floor()),
        _ => num(v),
    }
}

fn describe(id: &str, next: f64) -> String {
    let n = value_text(id, next);
    match id {
        "flights" => trf!("Zapisz w dzienniku {n} lotów"),
        "hours" => trf!("Spędź w powietrzu {n}"),
        "distance" => trf!("Przeleć łącznie {n}"),
        "landings" => trf!("Wyląduj {n} razy"),
        "airports" => trf!("Odwiedź {n} różnych lotnisk"),
        "countries" => trf!("Ląduj w {n} krajach"),
        "aircraft" => trf!("Leć {n} różnymi samolotami"),
        "butter" => trf!("Przyziemienie do 100 ft/min bez odbicia – {n} razy"),
        "great" => trf!("Lądowanie z oceną co najmniej 90 – {n} razy"),
        "streak" => trf!("{n} lądowań z rzędu z oceną co najmniej 80"),
        "crosswind" => trf!("Ocena co najmniej 70 przy wietrze bocznym {n}"),
        "long" => trf!("Lot dłuższy niż {n}"),
        "high" => trf!("Wznieś się na {n}"),
        "short" => trf!("Lądowanie na pasie krótszym niż 4000 ft z oceną co najmniej 70 – {n} razy"),
        "night" => trf!("Start między 21:00 a 5:00 – {n} razy"),
        "circuits" => trf!("Co najmniej trzy lądowania w jednym locie – {n} razy"),
        "centre" => trf!("Przyziemienie do 3 ft od osi pasa – {n} razy"),
        "tdz" => trf!("Przyziemienie 1000–2000 ft za progiem – {n} razy"),
        _ => trf!("Zalicz {n} wyzwań tygodnia"),
    }
}

fn badge(id: &'static str, title: &'static str, icon: Icon, v: f64) -> Badge {
    let t = tiers(id);
    let level = t.iter().filter(|x| v >= **x).count();
    let (desc, progress, value) = match t.get(level) {
        Some(next) => {
            let prev = if level == 0 { 0.0 } else { t[level - 1] };
            let p = ((v - prev) / (next - prev)).clamp(0.0, 1.0) as f32;
            (describe(id, *next), p, format!("{} / {}", value_text(id, v), value_text(id, *next)))
        }
        None => (tr("Wszystkie poziomy zdobyte").to_string(), 1.0, value_text(id, v)),
    };
    Badge { id, title: tr(title).to_string(), desc, icon, level, levels: t.len(), progress, value }
}

/// Lotniska lotu: początek, koniec i miejsca lądowań.
fn visited(f: &FlightLog) -> Vec<String> {
    let mut v: Vec<String> = [f.from.clone(), f.to.clone()].into_iter().flatten().collect();
    v.extend(f.all_landings().into_iter().filter_map(|l| l.airport));
    v.retain(|a| !a.is_empty());
    v
}

/// Wynik wyzwania w danym tygodniu dla lotów z tego tygodnia.
struct Done {
    done: bool,
    progress: f32,
    status: String,
    flight: Option<String>,
}

fn skill_challenge(kind: usize, week: &[(&FlightLog, Vec<LandingReport>)]) -> (String, String, Icon, Done) {
    let all: Vec<(&FlightLog, &LandingReport)> = week.iter().flat_map(|(f, ls)| ls.iter().map(move |l| (*f, l))).collect();
    let first = |pred: &dyn Fn(&LandingReport) -> bool| all.iter().find(|(_, l)| pred(l)).map(|(f, _)| f.id.clone());
    let simple = |flight: Option<String>, best: String| Done { done: flight.is_some(), progress: if flight.is_some() { 1.0 } else { 0.0 }, status: best, flight };
    match kind {
        0 => {
            let best = all.iter().filter(|(_, l)| l.bounces == 0).map(|(_, l)| l.vs_fpm.abs()).min();
            let f = first(&|l| l.vs_fpm.abs() < 120 && l.bounces == 0);
            (
                tr("Miękko").into(),
                tr("Przyziemienie wolniej niż 120 ft/min, bez odbicia").into(),
                Icon::Sparkles,
                simple(f, best.map(|b| trf!("Najlepiej w tym tygodniu: {b} ft/min")).unwrap_or_default()),
            )
        }
        1 => {
            let f = first(&|l| l.along_ft.is_some_and(|a| (1000..=2000).contains(&a)));
            let best = all.iter().filter_map(|(_, l)| l.along_ft).min_by_key(|a| (a - 1500).abs());
            (
                tr("W strefie").into(),
                tr("Przyziemienie 1000–2000 ft za progiem pasa").into(),
                Icon::Scan,
                simple(f, best.map(|b| trf!("Najbliżej: {b} ft za progiem")).unwrap_or_default()),
            )
        }
        2 => {
            let f = first(&|l| l.cross_ft.is_some_and(|c| c.abs() <= 5));
            let best = all.iter().filter_map(|(_, l)| l.cross_ft.map(|c| c.abs())).min();
            (
                tr("Na osi").into(),
                tr("Przyziemienie najwyżej 5 ft od osi pasa").into(),
                Icon::Crosshair,
                simple(f, best.map(|b| trf!("Najbliżej osi: {b} ft")).unwrap_or_default()),
            )
        }
        3 => {
            let f = first(&|l| l.score >= 75 && l.crosswind_kt.is_some_and(|c| c.abs() >= 10));
            let best = all.iter().filter(|(_, l)| l.score >= 75).filter_map(|(_, l)| l.crosswind_kt.map(|c| c.abs())).max();
            (
                tr("Wiatr z boku").into(),
                tr("Ocena co najmniej 75 przy wietrze bocznym od 10 kt").into(),
                Icon::Wind,
                simple(f, best.map(|b| trf!("Najsilniej z dobrą oceną: {b} kt")).unwrap_or_default()),
            )
        }
        4 => {
            let best = week.iter().map(|(f, ls)| (f, ls.len())).max_by_key(|x| x.1);
            let done = best.filter(|b| b.1 >= 3).map(|b| b.0.id.clone());
            let n = best.map(|b| b.1).unwrap_or(0);
            (
                tr("Kręgi").into(),
                tr("Trzy lądowania w jednym locie (dotyk i start)").into(),
                Icon::Circuit,
                Done { done: done.is_some(), progress: (n as f32 / 3.0).min(1.0), status: if n > 0 { trf!("Najwięcej w jednym locie: {n}") } else { String::new() }, flight: done },
            )
        }
        5 => {
            let ok = week.iter().find(|(f, ls)| f.duration_min() >= 120 && ls.iter().any(|l| l.score >= 80)).map(|(f, _)| f.id.clone());
            let longest = week.iter().map(|(f, _)| f.duration_min()).max().unwrap_or(0);
            (
                tr("Długa trasa").into(),
                tr("Lot dłuższy niż 2 godziny zakończony lądowaniem z oceną co najmniej 80").into(),
                Icon::Timer,
                Done {
                    done: ok.is_some(),
                    progress: if ok.is_some() { 1.0 } else { (longest as f32 / 120.0).min(0.95) },
                    status: if longest > 0 { trf!("Najdłuższy lot: {h}:{m:02}", h = longest / 60, m = longest % 60) } else { String::new() },
                    flight: ok,
                },
            )
        }
        _ => {
            let good: Vec<&(&FlightLog, &LandingReport)> = all.iter().filter(|(_, l)| l.score >= 80).collect();
            let n = good.len();
            (
                tr("Równa forma").into(),
                tr("Pięć lądowań w tygodniu z oceną co najmniej 80").into(),
                Icon::Star,
                Done {
                    done: n >= 5,
                    progress: (n as f32 / 5.0).min(1.0),
                    status: trf!("{n} z 5"),
                    flight: if n >= 5 { good.get(4).map(|x| x.0.id.clone()) } else { None },
                },
            )
        }
    }
}

fn airport_challenge(icao: &str, name: &str, week: &[(&FlightLog, Vec<LandingReport>)]) -> (String, String, Icon, Done) {
    let at = |f: &FlightLog, l: &LandingReport| l.airport.as_deref().or(f.to.as_deref()).is_some_and(|a| a.eq_ignore_ascii_case(icao));
    let mut best: Option<(i32, String)> = None;
    for (f, ls) in week {
        for l in ls.iter().filter(|l| at(f, l)) {
            if best.as_ref().is_none_or(|b| l.score > b.0) {
                best = Some((l.score, f.id.clone()));
            }
        }
    }
    let done = best.as_ref().filter(|b| b.0 >= 70).map(|b| b.1.clone());
    (
        trf!("Lotnisko tygodnia: {name}"),
        trf!("Wyląduj na {icao} z oceną co najmniej 70"),
        Icon::Pin,
        Done {
            done: done.is_some(),
            progress: best.as_ref().map(|b| (b.0 as f32 / 70.0).min(1.0)).unwrap_or(0.0),
            status: best.map(|b| trf!("Najlepsza ocena tutaj: {s}", s = b.0)).unwrap_or_default(),
            flight: done,
        },
    )
}

/// Wyzwania tygodnia zaczynającego się w `monday` dla podanych lotów (tylko z tego tygodnia).
fn week_challenges(monday: NaiveDate, flights: &[&FlightLog]) -> Vec<Challenge> {
    let start = Local.from_local_datetime(&monday.and_hms_opt(0, 0, 0).unwrap()).earliest().map(|d| d.timestamp_millis()).unwrap_or(0);
    let end = start + 7 * 86_400_000;
    let week: Vec<(&FlightLog, Vec<LandingReport>)> = flights
        .iter()
        .map(|f| (*f, f.all_landings().into_iter().filter(|l| (start..end).contains(&landing_time(f, l))).collect::<Vec<_>>()))
        .filter(|(f, ls)| !ls.is_empty() || (start..end).contains(&f.start))
        .collect();
    let seed = week_seed(monday);
    let list = airports();
    let (icao, name) = list[seed % list.len()];
    let w = monday.iso_week();
    let key = format!("{}-W{:02}", w.year(), w.week());
    let mut out = Vec::new();
    for (k, (title, desc, icon, d), apt) in [
        ("apt", airport_challenge(icao, name, &week), Some(icao.to_string())),
        ("skill", skill_challenge((seed * 7 + 3) % SKILLS, &week), None),
    ] {
        out.push(Challenge { key: format!("{key}-{k}"), title, desc, icon, airport: apt, done: d.done, progress: d.progress, status: d.status, flight_id: d.flight });
    }
    out
}

/// Osiągnięcia i wyzwania bieżącego tygodnia (`now_ms` – chwila „teraz”).
pub fn compute(flights: &[FlightLog], now_ms: i64) -> Summary {
    let mut landings: Vec<(i64, &FlightLog, LandingReport)> = Vec::new();
    for f in flights {
        for l in f.all_landings() {
            landings.push((landing_time(f, &l), f, l));
        }
    }
    landings.sort_by_key(|x| x.0);

    let minutes: i64 = flights.iter().map(|f| f.duration_min().max(0)).sum();
    let distance: i64 = flights.iter().map(|f| f.distance_nm.max(0) as i64).sum();
    let mut airports_set: HashSet<String> = HashSet::new();
    let mut countries: HashSet<String> = HashSet::new();
    let db = crate::aero::airports::db();
    for f in flights {
        for a in visited(f) {
            let a = a.to_uppercase();
            if let Some(c) = db.as_ref().and_then(|d| d.get(&a)).map(|x| x.country.clone()).filter(|c| !c.is_empty()) {
                countries.insert(c);
            }
            airports_set.insert(a);
        }
    }
    let aircraft: HashSet<String> = flights.iter().filter_map(|f| f.aircraft.as_ref()).filter(|a| !a.is_empty()).map(|a| a.to_lowercase()).collect();
    let count = |p: &dyn Fn(&LandingReport) -> bool| landings.iter().filter(|x| p(&x.2)).count() as f64;
    let butter = count(&|l| l.vs_fpm.abs() <= 100 && l.bounces == 0);
    let great = count(&|l| l.score >= 90);
    let short = count(&|l| l.score >= 70 && l.runway_length_ft.is_some_and(|r| r > 0 && r < 4000));
    let centre = count(&|l| l.cross_ft.is_some_and(|c| c.abs() <= 3));
    let tdz = count(&|l| l.along_ft.is_some_and(|a| (1000..=2000).contains(&a)));
    let crosswind = landings.iter().filter(|x| x.2.score >= 70).filter_map(|x| x.2.crosswind_kt.map(|c| c.abs())).max().unwrap_or(0) as f64;
    let mut streak = 0;
    let mut best_streak = 0;
    for x in &landings {
        if x.2.score >= 80 {
            streak += 1;
            best_streak = best_streak.max(streak);
        } else {
            streak = 0;
        }
    }
    let longest = flights.iter().map(|f| f.duration_min()).max().unwrap_or(0) as f64 / 60.0;
    let high = flights.iter().map(|f| f.max_alt_ft).max().unwrap_or(0) as f64;
    let night = flights.iter().filter(|f| local(f.start).is_some_and(|d| d.hour() >= 21 || d.hour() < 5)).count() as f64;
    let circuits = flights.iter().filter(|f| f.all_landings().len() >= 3).count() as f64;

    // Wyzwania: bieżący tydzień i wszystkie tygodnie z lotami (licznik zaliczonych)
    let mut by_week: BTreeMap<NaiveDate, Vec<&FlightLog>> = BTreeMap::new();
    for f in flights {
        let mut weeks: Vec<NaiveDate> = f.all_landings().iter().filter_map(|l| monday_of(landing_time(f, l))).collect();
        weeks.extend(monday_of(f.start));
        weeks.sort();
        weeks.dedup();
        for w in weeks {
            by_week.entry(w).or_default().push(f);
        }
    }
    let this_monday = monday_of(now_ms).unwrap_or_else(|| Local::now().date_naive());
    let mut done_total = 0usize;
    for (monday, fs) in &by_week {
        if *monday != this_monday {
            done_total += week_challenges(*monday, fs).iter().filter(|c| c.done).count();
        }
    }
    let week = week_challenges(this_monday, by_week.get(&this_monday).map(|v| v.as_slice()).unwrap_or(&[]));
    done_total += week.iter().filter(|c| c.done).count();

    let badges = vec![
        badge("flights", "Loty", Icon::Plane, flights.len() as f64),
        badge("hours", "Godziny w powietrzu", Icon::Clock, minutes as f64 / 60.0),
        badge("distance", "Dystans", Icon::Route, distance as f64),
        badge("landings", "Lądowania", Icon::Landing, landings.len() as f64),
        badge("great", "Świetne lądowania", Icon::Star, great),
        badge("butter", "Jak po maśle", Icon::Sparkles, butter),
        badge("streak", "Seria", Icon::Activity, best_streak as f64),
        badge("weekly", "Wyzwania tygodnia", Icon::Trophy, done_total as f64),
        badge("airports", "Odwiedzone lotniska", Icon::Pin, airports_set.len() as f64),
        badge("countries", "Kraje", Icon::Globe, countries.len() as f64),
        badge("aircraft", "Samoloty", Icon::Gauge, aircraft.len() as f64),
        badge("crosswind", "Wiatr boczny", Icon::Wind, crosswind),
        badge("centre", "Na osi", Icon::Crosshair, centre),
        badge("tdz", "W strefie przyziemienia", Icon::Scan, tdz),
        badge("short", "Krótki pas", Icon::Ruler, short),
        badge("circuits", "Kręgi", Icon::Circuit, circuits),
        badge("long", "Długi lot", Icon::Timer, longest),
        badge("high", "Wysoko", Icon::Mountain, high),
        badge("night", "Nocne loty", Icon::Moon, night),
    ];
    let unlocked = badges.iter().map(|b| b.level).sum();
    let total_levels = badges.iter().map(|b| b.levels).sum();
    let sunday = this_monday + Duration::days(6);
    let left = (sunday - local(now_ms).map(|d| d.date_naive()).unwrap_or(this_monday)).num_days() + 1;
    Summary {
        badges,
        week,
        week_title: trf!("Tydzień {w} · {a}–{b}", w = this_monday.iso_week().week(), a = this_monday.format("%d.%m"), b = sunday.format("%d.%m")),
        week_left: if left <= 1 { tr("ostatni dzień").to_string() } else { trf!("zostało {left} dni") },
        unlocked,
        total_levels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flight(id: &str, start: i64, mins: i64, to: &str, l: Vec<LandingReport>) -> FlightLog {
        FlightLog { id: id.into(), start, end: start + mins * 60_000, from: Some("EPWA".into()), to: Some(to.into()), aircraft: Some("A320".into()), distance_nm: 300, max_alt_ft: 36000, landings: l, ..Default::default() }
    }

    #[test]
    fn badges_and_week() {
        let now = Local.with_ymd_and_hms(2026, 11, 18, 12, 0, 0).unwrap().timestamp_millis();
        let monday = monday_of(now).unwrap();
        let list = airports();
        let (icao, _) = list[week_seed(monday) % list.len()];
        let good = |s: i32, vs: i32| LandingReport { score: s, vs_fpm: vs, at: Some(now - 3_600_000), ..Default::default() };
        let flights = vec![
            flight("a", now - 4 * 3_600_000, 130, icao, vec![good(92, -90)]),
            flight("b", now - 30 * 86_400_000, 60, "EPGD", vec![good(85, -150), good(81, -200), good(40, -600)]),
        ];
        let s = compute(&flights, now);
        let get = |id: &str| s.badges.iter().find(|b| b.id == id).unwrap().clone();
        assert_eq!(get("flights").level, 1);
        assert_eq!(get("landings").level, 1);
        assert_eq!(get("great").level, 1);
        assert_eq!(get("butter").level, 1);
        assert_eq!(get("circuits").level, 1);
        assert_eq!(get("high").level, 3);
        assert_eq!(get("long").level, 1);
        // seria: lądowania o tej samej chwili w kolejności lotów – 92, 85, 81, potem 40 przerywa → 3
        assert_eq!(get("streak").level, 1);
        assert!(get("flights").value.contains("/ 10"));
        // lotnisko tygodnia zaliczone lądowaniem z oceną 92
        let apt = s.week.iter().find(|c| c.airport.is_some()).unwrap();
        assert!(apt.done, "{apt:?}");
        assert_eq!(apt.flight_id.as_deref(), Some("a"));
        assert!(s.keys().iter().any(|k| k.ends_with("-apt")));
        assert!(get("weekly").level >= 1);
        assert!(!s.week_title.is_empty());
    }

    #[test]
    fn same_week_same_challenges() {
        let a = Local.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).unwrap().timestamp_millis();
        let b = Local.with_ymd_and_hms(2026, 11, 22, 20, 0, 0).unwrap().timestamp_millis();
        let x = compute(&[], a);
        let y = compute(&[], b);
        assert_eq!(x.week[0].key, y.week[0].key);
        assert_eq!(x.week[1].title, y.week[1].title);
        let next = compute(&[], b + 86_400_000);
        assert_ne!(x.week[0].key, next.week[0].key);
    }
}
