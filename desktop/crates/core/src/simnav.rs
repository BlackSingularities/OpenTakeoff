//! Dane nawigacyjne trybu symulacji: deterministyczne próbki dla EPWA i EPGD. Współrzędne progów zgodne
//! z lotem testowym, reszta wiarygodna, ale PRZYKŁADOWA – nie do prawdziwej nawigacji.

use crate::navdata::{fix_types as ft, math, AirportInfo, AirportProcedures, ApproachProcedure, FixResult, FrequencyInfo, IlsInfo, LegInfo, NamedLegs, NavDataProvider, NavError, NavResult, RunwayInfo, TerminalProcedure};

const FIXES: &[(&str, f64, f64, &str, Option<f64>, &str)] = &[
    ("WA531", 52.3000, 20.9000, ft::WAYPOINT, None, "EP"),
    ("SOXER", 52.6200, 20.5500, ft::WAYPOINT, None, "EP"),
    ("WA542", 52.1000, 21.1500, ft::WAYPOINT, None, "EP"),
    ("LOGDA", 51.8500, 21.4500, ft::WAYPOINT, None, "EP"),
    ("NEPOX", 51.5500, 21.9000, ft::WAYPOINT, None, "EP"),
    ("AGAVA", 52.7500, 21.4000, ft::WAYPOINT, None, "EP"),
    ("WA411", 52.4500, 21.1500, ft::WAYPOINT, None, "EP"),
    ("WA412", 52.3000, 21.2000, ft::WAYPOINT, None, "EP"),
    ("BAMSO", 51.6000, 20.4000, ft::WAYPOINT, None, "EP"),
    ("WA421", 51.9000, 20.6000, ft::WAYPOINT, None, "EP"),
    ("WA331", 52.0092, 21.1057, ft::WAYPOINT, None, "EP"),
    ("FI33", 52.0675, 21.0530, ft::WAYPOINT, None, "EP"),
    ("WA291", 52.0824, 21.2347, ft::WAYPOINT, None, "EP"),
    ("FA29", 52.1190, 21.1127, ft::WAYPOINT, None, "EP"),
    ("WA510", 52.2500, 20.8500, ft::WAYPOINT, None, "EP"),
    ("GD291", 54.3166, 18.7533, ft::WAYPOINT, None, "EP"),
    ("FI29", 54.3415, 18.6472, ft::WAYPOINT, None, "EP"),
    ("GD112", 54.4508, 18.1827, ft::WAYPOINT, None, "EP"),
    ("FA11", 54.4196, 18.3154, ft::WAYPOINT, None, "EP"),
    ("GD510", 54.4500, 18.3000, ft::WAYPOINT, None, "EP"),
    ("GD610", 54.3000, 18.7000, ft::WAYPOINT, None, "EP"),
    ("TOXAR", 53.9000, 19.3500, ft::WAYPOINT, None, "EP"),
    ("GD401", 54.1500, 19.0000, ft::WAYPOINT, None, "EP"),
    ("LUXAR", 54.5500, 17.6000, ft::WAYPOINT, None, "EP"),
    ("GD402", 54.5000, 17.9500, ft::WAYPOINT, None, "EP"),
    ("OKUPA", 54.6500, 18.0000, ft::WAYPOINT, None, "EP"),
    ("GD701", 54.4500, 18.3500, ft::WAYPOINT, None, "EP"),
    ("NIKOP", 54.2000, 19.0500, ft::WAYPOINT, None, "EP"),
    ("GD702", 54.3300, 18.6500, ft::WAYPOINT, None, "EP"),
    ("WAR", 52.1880, 20.9620, ft::VOR, Some(113.450), "EP"),
    ("GDA", 54.3700, 18.4000, ft::VOR, Some(115.500), "EP"),
    ("WA", 52.1300, 21.0100, ft::NDB, Some(0.389), "EP"),
    ("GD", 54.3600, 18.5500, ft::NDB, Some(0.418), "EP"),
    // Punkt o tej samej nazwie w innym regionie (test sortowania po odległości)
    ("SOXER", 50.1000, 8.6000, ft::WAYPOINT, None, "ED"),
];

fn ils(ident: &str, freq: f64, course: f64) -> Option<IlsInfo> {
    Some(IlsInfo { ident: ident.into(), freq_mhz: freq, loc_course_deg: course, gs_angle_deg: Some(3.0) })
}

#[allow(clippy::too_many_arguments)]
fn rwy(ident: &str, hdg: f64, length_m: f64, width_m: f64, lat: f64, lon: f64, elev: f64, ils: Option<IlsInfo>) -> RunwayInfo {
    RunwayInfo {
        ident: ident.into(),
        heading_true_deg: hdg,
        length_ft: (length_m * math::FT_PER_M).round(),
        width_ft: (width_m * math::FT_PER_M).round(),
        threshold_lat: lat,
        threshold_lon: lon,
        elev_ft: elev,
        ils,
    }
}

fn freq(kind: &str, name: &str, mhz: f64) -> FrequencyInfo {
    FrequencyInfo { kind: kind.into(), name: name.into(), mhz }
}

pub fn airports() -> Vec<AirportInfo> {
    vec![
        AirportInfo {
            icao: "EPWA".into(),
            name: "Warsaw Chopin".into(),
            lat: 52.1657,
            lon: 20.9671,
            elev_ft: 362.0,
            runways: vec![
                rwy("11", 116.0, 3690.0, 60.0, 52.1700, 20.9420, 354.0, ils("IWA", 109.900, 110.0)),
                rwy("29", 296.0, 3690.0, 60.0, 52.1555, 20.9906, 350.0, None),
                rwy("15", 151.0, 2800.0, 50.0, 52.1770, 20.9541, 341.0, None),
                rwy("33", 331.0, 2800.0, 50.0, 52.1550, 20.9740, 362.0, ils("IWS", 110.300, 325.0)),
            ],
            frequencies: vec![
                freq("ATIS", "Okecie Information", 120.455),
                freq("CLR", "Okecie Delivery", 121.605),
                freq("GND", "Okecie Ground", 121.900),
                freq("TWR", "Okecie Tower", 118.300),
                freq("APP", "Warszawa Approach", 128.800),
                freq("DEP", "Warszawa Departure", 128.800),
            ],
        },
        AirportInfo {
            icao: "EPGD".into(),
            name: "Gdansk Lech Walesa".into(),
            lat: 54.3776,
            lon: 18.4662,
            elev_ft: 489.0,
            runways: vec![
                rwy("11", 112.0, 2800.0, 45.0, 54.3884, 18.4480, 470.0, ils("IGDB", 110.900, 106.0)),
                rwy("29", 292.0, 2800.0, 45.0, 54.3790, 18.4880, 489.0, ils("IGDA", 109.300, 286.0)),
            ],
            frequencies: vec![
                freq("ATIS", "Rebiechowo Information", 126.775),
                freq("GND", "Rebiechowo Ground", 121.750),
                freq("TWR", "Rebiechowo Tower", 118.100),
                freq("APP", "Gdansk Approach", 128.075),
            ],
        },
    ]
}

/// Budowanie odcinków z kontekstem lotniska (punkty RWxx to progi pasów tego lotniska).
struct Legs {
    airport: AirportInfo,
}

impl Legs {
    fn pos(&self, fix: &str) -> (Option<f64>, Option<f64>) {
        if let Some(f) = FIXES.iter().find(|f| f.0 == fix && f.5 == "EP") {
            return (Some(f.1), Some(f.2));
        }
        if let Some(r) = fix.strip_prefix("RW").and_then(|id| self.airport.runways.iter().find(|r| r.ident == id)) {
            return (Some(r.threshold_lat), Some(r.threshold_lon));
        }
        (None, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn leg(&self, kind: &str, fix: &str, desc: Option<&str>, alt1: Option<f64>, alt2: Option<f64>, speed: Option<f64>, course: Option<f64>, fly_over: bool) -> LegInfo {
        let (lat, lon) = if fix.is_empty() { (None, None) } else { self.pos(fix) };
        LegInfo {
            fix: fix.into(),
            lat,
            lon,
            leg_type: kind.into(),
            alt_desc: desc.map(str::to_string),
            alt1_ft: alt1,
            alt2_ft: alt2,
            speed_kt: speed,
            course_deg: course,
            dist_nm: None,
            fly_over,
        }
    }
    fn if_(&self, fix: &str, desc: &str, alt: f64) -> LegInfo {
        self.leg("IF", fix, Some(desc), Some(alt), None, None, None, false)
    }
    fn tf(&self, fix: &str, desc: &str, alt: f64) -> LegInfo {
        self.leg("TF", fix, Some(desc), Some(alt), None, None, None, false)
    }
    fn tfs(&self, fix: &str, desc: &str, alt: f64, speed: f64) -> LegInfo {
        self.leg("TF", fix, Some(desc), Some(alt), None, Some(speed), None, false)
    }
    fn tf_between(&self, fix: &str, alt: f64, alt2: f64, speed: f64) -> LegInfo {
        self.leg("TF", fix, Some(BETWEEN), Some(alt), Some(alt2), Some(speed), None, false)
    }
    fn tf_over(&self, fix: &str, desc: &str, alt: f64) -> LegInfo {
        self.leg("TF", fix, Some(desc), Some(alt), None, None, None, true)
    }
    fn df(&self, fix: &str, desc: &str, alt: f64, speed: Option<f64>) -> LegInfo {
        self.leg("DF", fix, Some(desc), Some(alt), None, speed, None, false)
    }
    fn cf(&self, fix: &str, course: f64, alt: f64, speed: Option<f64>, fly_over: bool) -> LegInfo {
        self.leg("CF", fix, Some(AT), Some(alt), None, speed, Some(course), fly_over)
    }
    fn va(&self, heading: f64, alt: f64) -> LegInfo {
        self.leg("VA", "", Some(ABOVE), Some(alt), None, None, Some(heading), false)
    }
    fn ca(&self, course: f64, alt: f64) -> LegInfo {
        self.leg("CA", "", Some(ABOVE), Some(alt), None, None, Some(course), false)
    }
}

const AT: &str = "AT";
const ABOVE: &str = "ABOVE";
const BETWEEN: &str = "BETWEEN";

fn terminal(name: &str, rwy: Vec<NamedLegs>, common: Vec<LegInfo>, enroute: Vec<NamedLegs>) -> TerminalProcedure {
    TerminalProcedure { name: name.into(), common_legs: common, runway_transitions: rwy, enroute_transitions: enroute }
}

fn approach(name: &str, kind: &str, runway: &str, suffix: Option<&str>, transitions: Vec<NamedLegs>, final_legs: Vec<LegInfo>, missed: Vec<LegInfo>) -> ApproachProcedure {
    ApproachProcedure { name: name.into(), kind: kind.into(), runway: Some(runway.into()), suffix: suffix.map(str::to_string), transitions, final_legs, missed_legs: missed }
}

fn epwa(l: &Legs) -> AirportProcedures {
    AirportProcedures {
        icao: "EPWA".into(),
        departures: vec![
            terminal(
                "SOXER1G",
                vec![NamedLegs::new("29", vec![l.va(296.0, 1000.0), l.df("WA531", ABOVE, 4000.0, None)]), NamedLegs::new("33", vec![l.va(331.0, 1000.0), l.df("WA531", ABOVE, 4000.0, None)])],
                vec![l.tf("SOXER", ABOVE, 7000.0)],
                vec![],
            ),
            terminal(
                "LOGDA1K",
                vec![
                    NamedLegs::new("11", vec![l.va(116.0, 1000.0), l.df("WA542", ABOVE, 3000.0, Some(220.0))]),
                    NamedLegs::new("15", vec![l.va(151.0, 1000.0), l.df("WA542", ABOVE, 3000.0, Some(220.0))]),
                ],
                vec![l.tf("LOGDA", ABOVE, 6000.0)],
                vec![NamedLegs::new("NEPOX", vec![l.tf("NEPOX", ABOVE, 9000.0)])],
            ),
        ],
        arrivals: vec![
            terminal(
                "AGAVA1W",
                vec![NamedLegs::new("33", vec![l.tf("WA412", AT, 4000.0), l.tf("WA331", AT, 4000.0)]), NamedLegs::new("29", vec![l.tf("WA412", AT, 4000.0)])],
                vec![l.if_("AGAVA", ABOVE, 9000.0), l.tfs("WA411", AT, 6000.0, 220.0)],
                vec![],
            ),
            terminal(
                "BAMSO1S",
                vec![NamedLegs::new("29", vec![l.tf("WA291", AT, 4000.0)]), NamedLegs::new("33", vec![l.tf("WA331", AT, 4000.0)])],
                vec![l.if_("BAMSO", ABOVE, 8000.0), l.tf_between("WA421", 6000.0, 5000.0, 230.0)],
                vec![],
            ),
        ],
        approaches: vec![
            approach(
                "ILS 33",
                "ILS",
                "33",
                None,
                vec![NamedLegs::new("WA412", vec![l.if_("WA412", AT, 4000.0), l.tf("WA331", AT, 4000.0)])],
                vec![l.if_("WA331", AT, 4000.0), l.cf("FI33", 331.0, 2300.0, Some(180.0), false), l.cf("RW33", 331.0, 410.0, None, true)],
                vec![l.ca(331.0, 1500.0), l.df("WA510", AT, 4000.0, None)],
            ),
            approach(
                "RNAV Z 29",
                "RNAV",
                "29",
                Some("Z"),
                vec![NamedLegs::new("BAMSO", vec![l.if_("BAMSO", ABOVE, 8000.0), l.tf("WA421", AT, 5000.0), l.tf("WA291", AT, 4000.0)])],
                vec![l.if_("WA291", AT, 4000.0), l.tf("FA29", AT, 2000.0), l.tf_over("RW29", AT, 400.0)],
                vec![l.ca(296.0, 1500.0), l.df("WA531", AT, 4000.0, None)],
            ),
        ],
    }
}

fn epgd(l: &Legs) -> AirportProcedures {
    AirportProcedures {
        icao: "EPGD".into(),
        departures: vec![
            terminal("OKUPA1A", vec![NamedLegs::new("29", vec![l.va(292.0, 1000.0), l.df("GD701", ABOVE, 3000.0, None)])], vec![l.tf("OKUPA", ABOVE, 6000.0)], vec![]),
            terminal("NIKOP1B", vec![NamedLegs::new("11", vec![l.va(112.0, 1000.0), l.df("GD702", ABOVE, 3000.0, None)])], vec![l.tf("NIKOP", ABOVE, 7000.0)], vec![]),
        ],
        arrivals: vec![
            terminal(
                "TOXAR1R",
                vec![NamedLegs::new("29", vec![l.tf("GD291", AT, 3500.0)]), NamedLegs::new("11", vec![l.tf("GD510", AT, 4000.0), l.tf("GD112", AT, 3500.0)])],
                vec![l.if_("TOXAR", ABOVE, 9000.0), l.tfs("GD401", AT, 6000.0, 220.0)],
                vec![],
            ),
            terminal(
                "LUXAR1L",
                vec![NamedLegs::new("11", vec![l.tf("GD112", AT, 3500.0)]), NamedLegs::new("29", vec![l.tf("GD610", AT, 4000.0), l.tf("GD291", AT, 3500.0)])],
                vec![l.if_("LUXAR", ABOVE, 8000.0), l.tfs("GD402", AT, 5000.0, 220.0)],
                vec![],
            ),
        ],
        approaches: vec![
            approach(
                "ILS 29",
                "ILS",
                "29",
                None,
                vec![NamedLegs::new("GD401", vec![l.if_("GD401", AT, 6000.0), l.tf("GD291", AT, 3500.0)])],
                vec![l.if_("GD291", AT, 3500.0), l.cf("FI29", 286.0, 2400.0, Some(180.0), false), l.cf("RW29", 286.0, 540.0, None, true)],
                vec![l.ca(292.0, 1500.0), l.df("GD510", AT, 4000.0, None)],
            ),
            approach(
                "RNAV 11",
                "RNAV",
                "11",
                None,
                vec![NamedLegs::new("GD402", vec![l.if_("GD402", AT, 5000.0), l.tf("GD112", AT, 3500.0)])],
                vec![l.if_("GD112", AT, 3500.0), l.tf("FA11", AT, 2100.0), l.tf_over("RW11", AT, 520.0)],
                vec![l.ca(112.0, 1500.0), l.df("GD610", AT, 4000.0, None)],
            ),
        ],
    }
}

fn not_found(icao: &str) -> NavError {
    NavError::not_found(format!("{icao}: w trybie symulacji dostępne są tylko EPWA i EPGD"))
}

pub struct SimulationNavData;

impl NavDataProvider for SimulationNavData {
    fn airport(&self, icao: &str) -> NavResult<AirportInfo> {
        airports().into_iter().find(|a| a.icao == icao).ok_or_else(|| not_found(icao))
    }

    fn procedures(&self, icao: &str) -> NavResult<AirportProcedures> {
        let a = self.airport(icao)?;
        let l = Legs { airport: a };
        Ok(if icao == "EPWA" { epwa(&l) } else { epgd(&l) })
    }

    fn find_fix(&self, ident: &str) -> NavResult<Vec<FixResult>> {
        let mut out: Vec<FixResult> = FIXES
            .iter()
            .filter(|f| f.0 == ident)
            .map(|f| FixResult { ident: f.0.into(), kind: f.3.into(), lat: f.1, lon: f.2, freq_mhz: f.4, region: Some(f.5.into()) })
            .collect();
        if let Some(a) = airports().into_iter().find(|a| a.icao == ident) {
            out.push(FixResult { ident: a.icao, kind: ft::AIRPORT.into(), lat: a.lat, lon: a.lon, freq_mhz: None, region: Some("EP".into()) });
        }
        Ok(out)
    }

    fn nearby(&self, _lat: f64, _lon: f64, _radius: f64) -> NavResult<Vec<FixResult>> {
        let mut out: Vec<FixResult> =
            FIXES.iter().map(|f| FixResult { ident: f.0.into(), kind: f.3.into(), lat: f.1, lon: f.2, freq_mhz: f.4, region: Some(f.5.into()) }).collect();
        out.extend(airports().into_iter().map(|a| FixResult { ident: a.icao, kind: ft::AIRPORT.into(), lat: a.lat, lon: a.lon, freq_mhz: None, region: Some("EP".into()) }));
        Ok(out)
    }
}
