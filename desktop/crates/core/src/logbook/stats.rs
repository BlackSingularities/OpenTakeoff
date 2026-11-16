//! Statystyki dziennika: sumy, podział na miesiące, samoloty i lotniska, rozkład ocen lądowań i rekordy.

use std::collections::HashMap;

use chrono::{Datelike, TimeZone};

use super::model::FlightLog;

#[derive(Clone, Debug, Default)]
pub struct Counted {
    pub name: String,
    pub flights: usize,
    pub minutes: i64,
    pub distance_nm: i64,
}

#[derive(Clone, Debug, Default)]
pub struct Month {
    /// "2026-11"
    pub key: String,
    pub year: i32,
    pub month: u32,
    pub flights: usize,
    pub minutes: i64,
    pub distance_nm: i64,
}

#[derive(Clone, Debug, Default)]
pub struct Record {
    pub flight_id: String,
    pub value: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub flights: usize,
    pub minutes: i64,
    pub distance_nm: i64,
    pub landings: usize,
    pub avg_score: Option<f64>,
    pub avg_vs_fpm: Option<f64>,
    pub airports: usize,
    pub aircraft_types: usize,
    pub months: Vec<Month>,
    pub top_aircraft: Vec<Counted>,
    pub top_airports: Vec<Counted>,
    pub top_routes: Vec<Counted>,
    /// Oceny lądowań w przedziałach 0–9, 10–19 … 90–100.
    pub score_buckets: [usize; 10],
    pub longest: Option<Record>,
    pub farthest: Option<Record>,
    pub highest: Option<Record>,
    pub softest: Option<Record>,
    pub best_landing: Option<Record>,
    pub first_flight: Option<i64>,
    pub last_flight: Option<i64>,
}

fn top(map: HashMap<String, Counted>, n: usize) -> Vec<Counted> {
    let mut v: Vec<Counted> = map.into_values().collect();
    v.sort_by(|a, b| b.flights.cmp(&a.flights).then(b.minutes.cmp(&a.minutes)).then(a.name.cmp(&b.name)));
    v.truncate(n);
    v
}

fn better(slot: &mut Option<Record>, id: &str, value: f64, higher: bool) {
    let replace = match slot {
        None => true,
        Some(r) => {
            if higher {
                value > r.value
            } else {
                value < r.value
            }
        }
    };
    if replace {
        *slot = Some(Record { flight_id: id.to_string(), value });
    }
}

/// `flights` – już z poprawkami z komputera (lotniska, samolot).
pub fn compute(flights: &[FlightLog]) -> Stats {
    let mut s = Stats::default();
    let mut months: HashMap<String, Month> = HashMap::new();
    let mut aircraft: HashMap<String, Counted> = HashMap::new();
    let mut airports: HashMap<String, Counted> = HashMap::new();
    let mut routes: HashMap<String, Counted> = HashMap::new();
    let (mut score_sum, mut vs_sum) = (0.0, 0.0);
    for f in flights {
        let min = f.duration_min().max(0);
        s.flights += 1;
        s.minutes += min;
        s.distance_nm += f.distance_nm as i64;
        s.first_flight = Some(s.first_flight.map_or(f.start, |x| x.min(f.start)));
        s.last_flight = Some(s.last_flight.map_or(f.start, |x| x.max(f.start)));
        if let Some(dt) = chrono::Local.timestamp_millis_opt(f.start).single() {
            let key = format!("{:04}-{:02}", dt.year(), dt.month());
            let m = months.entry(key.clone()).or_insert(Month { key, year: dt.year(), month: dt.month(), ..Default::default() });
            m.flights += 1;
            m.minutes += min;
            m.distance_nm += f.distance_nm as i64;
        }
        let add = |map: &mut HashMap<String, Counted>, name: &str| {
            let c = map.entry(name.to_string()).or_insert(Counted { name: name.to_string(), ..Default::default() });
            c.flights += 1;
            c.minutes += min;
            c.distance_nm += f.distance_nm as i64;
        };
        if let Some(a) = f.aircraft.as_deref().filter(|a| !a.is_empty()) {
            add(&mut aircraft, a);
        }
        let mut seen = Vec::new();
        for ap in [f.from.as_deref(), f.to.as_deref()].into_iter().flatten() {
            if !seen.contains(&ap) {
                seen.push(ap);
                add(&mut airports, ap);
            }
        }
        if let (Some(a), Some(b)) = (f.from.as_deref(), f.to.as_deref()) {
            if a != b {
                add(&mut routes, &format!("{a} – {b}"));
            }
        }
        better(&mut s.longest, &f.id, min as f64, true);
        better(&mut s.farthest, &f.id, f.distance_nm as f64, true);
        better(&mut s.highest, &f.id, f.max_alt_ft as f64, true);
        for l in f.all_landings() {
            s.landings += 1;
            score_sum += l.score as f64;
            vs_sum += (l.vs_fpm as f64).abs();
            s.score_buckets[((l.score.clamp(0, 100) / 10) as usize).min(9)] += 1;
            better(&mut s.softest, &f.id, (l.vs_fpm as f64).abs(), false);
            better(&mut s.best_landing, &f.id, l.score as f64, true);
        }
    }
    if s.landings > 0 {
        s.avg_score = Some(score_sum / s.landings as f64);
        s.avg_vs_fpm = Some(vs_sum / s.landings as f64);
    }
    s.airports = airports.len();
    s.aircraft_types = aircraft.len();
    let mut m: Vec<Month> = months.into_values().collect();
    m.sort_by(|a, b| a.key.cmp(&b.key));
    s.months = m;
    s.top_aircraft = top(aircraft, 8);
    s.top_airports = top(airports, 10);
    s.top_routes = top(routes, 8);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logbook::model::LandingReport;

    #[test]
    fn totals() {
        let a = FlightLog {
            id: "a".into(),
            start: 1_700_000_000_000,
            end: 1_700_000_000_000 + 90 * 60_000,
            from: Some("EPWA".into()),
            to: Some("EPGD".into()),
            aircraft: Some("A320".into()),
            distance_nm: 160,
            max_alt_ft: 24000,
            landings: vec![LandingReport { score: 92, vs_fpm: -120, ..Default::default() }],
            ..Default::default()
        };
        let b = FlightLog {
            id: "b".into(),
            start: 1_700_100_000_000,
            end: 1_700_100_000_000 + 30 * 60_000,
            from: Some("EPGD".into()),
            to: Some("EPGD".into()),
            aircraft: Some("C172".into()),
            distance_nm: 40,
            max_alt_ft: 3000,
            landing: Some(LandingReport { score: 55, vs_fpm: -380, ..Default::default() }),
            ..Default::default()
        };
        let s = compute(&[a, b]);
        assert_eq!(s.flights, 2);
        assert_eq!(s.minutes, 120);
        assert_eq!(s.landings, 2);
        assert_eq!(s.airports, 2);
        assert_eq!(s.top_airports[0].name, "EPGD");
        assert_eq!(s.top_routes.len(), 1);
        assert_eq!(s.score_buckets[9], 1);
        assert_eq!(s.softest.unwrap().flight_id, "a");
        assert_eq!(s.highest.unwrap().value, 24000.0);
        assert!((s.avg_score.unwrap() - 73.5).abs() < 1e-9);
    }
}
