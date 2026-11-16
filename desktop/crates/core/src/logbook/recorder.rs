//! Rejestrator lotu na komputerze – ta sama logika co w telefonie, więc wpisy z obu urządzeń wyglądają tak samo.
//! Start: w powietrzu i GS > 40 kt. Koniec: na ziemi i GS < 5 kt przez 60 s (albo `finish` po rozłączeniu).
//! Skok pozycji większy niż możliwy przelot (restart lotu w symulatorze) albo zmiana samolotu zamyka wpis.

use super::landing::{self, RunwayGeom, Touchdown};
use super::model::{new_id, FlightLog, FlightMarker, LandingReport, TrackPoint};
use crate::navdata::math::distance_nm;
use crate::proto::FlightState;

pub type RunwaysNear = Box<dyn Fn(f64, f64) -> Vec<RunwayGeom> + Send>;
pub type NearestAirport = Box<dyn Fn(f64, f64) -> Option<String> + Send>;

pub const MAX_POINTS: usize = 60_000;

pub struct Recorder {
    runways_near: RunwaysNear,
    nearest_airport: NearestAirport,
    /// Odstęp między punktami toru (ms).
    pub interval_ms: i64,
    points: Vec<TrackPoint>,
    markers: Vec<FlightMarker>,
    aircraft: Option<String>,
    category: Option<String>,
    stopped_since: Option<i64>,
    last_td_seq: Option<i64>,
    landings: Vec<LandingReport>,
    landing: Option<Touchdown>,
    landing_airport: Option<String>,
    landing_at: i64,
    bounces: i32,
    prev: Option<FlightState>,
    last_at: i64,
    recording_id: String,
    airborne_since: Option<i64>,
    pub start_fuel_kg: Option<f64>,
    pub last_landing: Option<LandingReport>,
    pub recording: bool,
    pub hold_until_ground: bool,
}

fn point(s: &FlightState, now: i64) -> TrackPoint {
    let hdg = s.attitude.hdg_true_deg;
    let agl = s.avionics.as_ref().map(|a| a.radio_alt_ft).filter(|v| *v > 0.0).unwrap_or(s.position.alt_agl_ft);
    TrackPoint {
        t: now,
        lat: s.position.lat,
        lon: s.position.lon,
        alt_ft: s.position.alt_msl_ft as i32,
        gs_kt: s.speed.gs_kt as i32,
        vs_fpm: Some(s.speed.vs_fpm as i32),
        ias_kt: Some(s.speed.ias_kt as i32),
        hdg: Some((((hdg % 360.0) + 360.0) % 360.0) as i32),
        bank_deg: Some(s.attitude.bank_deg as f32),
        pitch_deg: Some(s.attitude.pitch_deg as f32),
        agl_ft: Some(agl as i32),
        g_load: s.avionics.as_ref().map(|a| a.g_force as f32),
        on_ground: Some(s.aircraft.on_ground),
    }
}

impl Recorder {
    pub fn new(runways_near: RunwaysNear, nearest_airport: NearestAirport) -> Self {
        Self {
            runways_near,
            nearest_airport,
            interval_ms: 1000,
            points: Vec::new(),
            markers: Vec::new(),
            aircraft: None,
            category: None,
            stopped_since: None,
            last_td_seq: None,
            landings: Vec::new(),
            landing: None,
            landing_airport: None,
            landing_at: 0,
            bounces: 0,
            prev: None,
            last_at: 0,
            recording_id: new_id(),
            airborne_since: None,
            start_fuel_kg: None,
            last_landing: None,
            recording: false,
            hold_until_ground: false,
        }
    }

    pub fn current_track(&self) -> &[TrackPoint] {
        &self.points
    }

    pub fn started_at(&self) -> Option<i64> {
        if self.recording {
            self.points.first().map(|p| p.t)
        } else {
            None
        }
    }

    pub fn mark(&mut self, label: &str, now: i64) -> bool {
        if !self.recording {
            return false;
        }
        self.markers.push(FlightMarker { t: now, label: label.to_string() });
        true
    }

    /// Kolejny stan; zwraca zakończony lot, gdy samolot stanął po lądowaniu.
    pub fn on_state(&mut self, s: &FlightState, now: i64) -> Option<FlightLog> {
        if s.position.lat == 0.0 && s.position.lon == 0.0 {
            return None;
        }
        if s.aircraft.paused {
            return None;
        }
        let gs = s.speed.gs_kt;
        let on_ground = s.aircraft.on_ground;
        if !self.recording {
            if self.hold_until_ground {
                if on_ground {
                    self.hold_until_ground = false;
                }
                self.prev = Some(s.clone());
                self.last_at = now;
                return None;
            }
            if !on_ground && gs > 40.0 {
                self.recording = true;
                self.recording_id = new_id();
                self.points.clear();
                self.aircraft = Some(s.aircraft.title.clone()).filter(|t| !t.is_empty());
                self.category = Some(s.aircraft.category.clone()).filter(|t| !t.is_empty());
                self.stopped_since = None;
                self.landing = None;
                self.bounces = 0;
                self.last_landing = None;
                self.landings.clear();
                self.markers.clear();
                self.airborne_since = Some(now);
                self.start_fuel_kg = Some(s.fuel.total_kg);
                self.last_td_seq = s.touchdown.as_ref().map(|t| t.seq);
                self.points.push(point(s, now));
            }
            self.prev = Some(s.clone());
            self.last_at = now;
            return None;
        }
        if let Some(p) = &self.prev {
            let dt_h = (now - self.last_at).max(1) as f64 / 3_600_000.0;
            let possible = (gs.max(p.speed.gs_kt) + 100.0) * dt_h * 1.5 + 1.0;
            let jumped = distance_nm(p.position.lat, p.position.lon, s.position.lat, s.position.lon) > possible;
            let other = self.aircraft.as_deref().is_some_and(|a| !s.aircraft.title.is_empty() && s.aircraft.title != a);
            if jumped || other {
                let done = self.finish(self.last_at);
                self.prev = None;
                let next = self.on_state(s, now);
                return done.or(next);
            }
        }
        if !on_ground && self.airborne_since.is_none() {
            self.airborne_since = Some(now);
        }
        self.detect_touchdown(s, now);
        self.prev = Some(s.clone());
        self.last_at = now;
        if self.points.last().is_some_and(|l| now - l.t >= self.interval_ms) {
            self.points.push(point(s, now));
        }
        if on_ground && gs < 5.0 {
            let since = *self.stopped_since.get_or_insert(now);
            if now - since >= 60_000 {
                return self.finish(now);
            }
        } else {
            self.stopped_since = None;
        }
        None
    }

    fn detect_touchdown(&mut self, s: &FlightState, now: i64) {
        let td = s.touchdown.as_ref();
        let from_link = td.is_some_and(|t| Some(t.seq) != self.last_td_seq);
        let prev_air = self.prev.as_ref().is_some_and(|p| !p.aircraft.on_ground);
        let from_transition = !from_link && prev_air && s.aircraft.on_ground;
        if !from_link && !from_transition {
            return;
        }
        if from_link {
            self.last_td_seq = td.map(|t| t.seq);
        }
        let flown_long_enough = self.airborne_since.map(|a| now - a).unwrap_or(0) >= 60_000;
        if self.landing.is_some() && (now - self.landing_at <= 15_000 || !flown_long_enough) {
            self.bounces += 1;
            return;
        }
        if !flown_long_enough {
            return;
        }
        let prev = self.prev.clone();
        let (vs, lat, lon, hdg, g, bank, pitch, ias) = match (from_link, td) {
            (true, Some(t)) => (t.vs_fpm, t.lat, t.lon, t.heading_true_deg, Some(t.g_force), t.bank_deg, t.pitch_deg, t.ias_kt),
            _ => {
                let p = prev.as_ref();
                (
                    p.map(|p| p.speed.vs_fpm).unwrap_or(0.0),
                    s.position.lat,
                    s.position.lon,
                    s.attitude.hdg_true_deg,
                    None,
                    p.map(|p| p.attitude.bank_deg).unwrap_or(0.0),
                    p.map(|p| p.attitude.pitch_deg).unwrap_or(0.0),
                    p.map(|p| p.speed.ias_kt).unwrap_or(0.0),
                )
            }
        };
        let rw = landing::pick(&(self.runways_near)(lat, lon), lat, lon, Some(hdg));
        let off = rw.as_ref().map(|r| landing::offset(r, lat, lon));
        let cross = rw.as_ref().map(|r| landing::wind_components(s.environment.wind_dir_deg, s.environment.wind_spd_kt, r.heading_true_deg).1);
        if let Some(r) = self.report() {
            self.landings.push(r);
        }
        self.landing = Some(Touchdown {
            vs_fpm: vs,
            g_force: g,
            bank_deg: Some(bank),
            pitch_deg: Some(pitch),
            ias_kt: Some(ias),
            runway: rw.as_ref().map(|r| r.ident.clone()),
            along_ft: off.map(|o| o.along_ft),
            cross_ft: off.map(|o| o.cross_ft),
            runway_length_ft: rw.as_ref().and_then(|r| r.length_ft),
            crosswind_kt: cross,
            bounces: 0,
        });
        self.landing_at = now;
        self.landing_airport = (self.nearest_airport)(lat, lon);
        self.bounces = 0;
        self.airborne_since = None;
        self.last_landing = self.report();
    }

    fn report(&self) -> Option<LandingReport> {
        self.landing.as_ref().map(|l| {
            let mut t = l.clone();
            t.bounces = self.bounces;
            let mut r = landing::evaluate(&t);
            r.airport = self.landing_airport.clone();
            r.at = Some(self.landing_at);
            r
        })
    }

    pub fn stop_by_user(&mut self, now: i64) -> Option<FlightLog> {
        let log = self.finish(now);
        self.hold_until_ground = true;
        log
    }

    pub fn resume(&mut self) {
        self.hold_until_ground = false;
    }

    /// Kończy nagrywanie; krótkie loty (< 2 min) są odrzucane.
    pub fn finish(&mut self, now: i64) -> Option<FlightLog> {
        if !self.recording {
            return None;
        }
        self.recording = false;
        let pts = std::mem::take(&mut self.points);
        self.build(&pts, now)
    }

    /// Bieżący lot jako wpis (bez kończenia) – szkic zapisywany okresowo; ten sam id co wpis końcowy.
    pub fn snapshot(&self, now: i64) -> Option<FlightLog> {
        if self.recording {
            self.build(&self.points, now)
        } else {
            None
        }
    }

    fn build(&self, pts: &[TrackPoint], now: i64) -> Option<FlightLog> {
        if pts.len() < 2 || now - pts[0].t < 120_000 {
            return None;
        }
        let dist: f64 = pts.windows(2).map(|w| distance_nm(w[0].lat, w[0].lon, w[1].lat, w[1].lon)).sum();
        let report = self.report();
        let mut landings = self.landings.clone();
        landings.extend(report.clone());
        let track = if pts.len() <= MAX_POINTS {
            pts.to_vec()
        } else {
            let step = pts.len() / MAX_POINTS + 1;
            let mut v: Vec<TrackPoint> = pts.iter().step_by(step).cloned().collect();
            v.push(pts[pts.len() - 1].clone());
            v
        };
        Some(FlightLog {
            id: self.recording_id.clone(),
            start: pts[0].t,
            end: now,
            aircraft: self.aircraft.clone(),
            from: (self.nearest_airport)(pts[0].lat, pts[0].lon),
            to: (self.nearest_airport)(pts[pts.len() - 1].lat, pts[pts.len() - 1].lon),
            max_alt_ft: pts.iter().map(|p| p.alt_ft).max().unwrap_or(0),
            distance_nm: dist as i32,
            track,
            landing: report,
            category: self.category.clone(),
            landings,
            markers: self.markers.clone(),
            scenario: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axes::AxisOutput;
    use crate::simulation::FlightSimulation;

    #[test]
    fn records_simulated_flight() {
        let mut rec = Recorder::new(
            Box::new(|_, _| {
                vec![RunwayGeom { ident: "29".into(), threshold_lat: 54.3790, threshold_lon: 18.4880, heading_true_deg: 292.0, length_ft: Some(9186.0) }]
            }),
            Box::new(|lat, _| Some(if lat > 53.0 { "EPGD".into() } else { "EPWA".into() })),
        );
        let mut sim = FlightSimulation::new();
        sim.position_on_final(12.0);
        let mut t = 1_000_000i64;
        let mut done = None;
        for _ in 0..20_000 {
            sim.step(0.25, AxisOutput::NEUTRAL, t);
            t += 250;
            if let Some(f) = rec.on_state(sim.state(), t) {
                done = Some(f);
                break;
            }
        }
        let f = done.expect("lot zakończony");
        assert_eq!(f.to.as_deref(), Some("EPGD"));
        assert!(f.track.len() > 100);
        let l = f.last_landing().unwrap();
        assert_eq!(l.vs_fpm, -180);
        assert_eq!(l.airport.as_deref(), Some("EPGD"));
        // Symulacja przyziemia tuż przed progiem 29 – ocena to wyłapuje
        assert_eq!(l.runway.as_deref(), Some("29"), "{:?}", l.notes);
        assert!(l.notes.iter().any(|n| n.contains("progiem")), "{:?}", l.notes);
    }
}

#[cfg(test)]
mod sample {
    /// Przykładowy dziennik do podglądu: `OTK_SAMPLE=<folder> cargo test -p opentakeoff-core sample_logbook -- --ignored`.
    #[test]
    #[ignore]
    fn sample_logbook() {
        use crate::axes::AxisOutput;
        use crate::simulation::FlightSimulation;
        let dir = std::path::PathBuf::from(std::env::var("OTK_SAMPLE").expect("OTK_SAMPLE"));
        let mut rec = super::Recorder::new(Box::new(|_, _| Vec::new()), Box::new(|lat, _| Some(if lat > 53.0 { "EPGD".into() } else { "EPWA".into() })));
        rec.interval_ms = 2000;
        let mut flights = Vec::new();
        let mut t = 1_792_000_000_000i64;
        for k in 0..3 {
            let mut sim = FlightSimulation::new();
            if k > 0 {
                sim.position_on_final(15.0 + k as f64 * 10.0);
            }
            for _ in 0..200_000 {
                sim.step(0.5, AxisOutput::NEUTRAL, t);
                t += 500;
                if let Some(f) = rec.on_state(sim.state(), t) {
                    flights.push(f);
                    break;
                }
            }
            t += 86_400_000 * 3;
        }
        assert!(!flights.is_empty());
        std::fs::create_dir_all(dir.join("logbook")).unwrap();
        std::fs::write(dir.join("logbook").join("flights.oab"), crate::logbook::archive::encode(&flights)).unwrap();
    }
}
