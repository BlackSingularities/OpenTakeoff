//! Migawka stanu samolotu dla rendererów PFD/MFD i zegarów – przeniesiona z FlightData.kt / SimData.kt.
//! Brak danych: NaN (liczby) albo None.

use std::sync::OnceLock;

use opentakeoff_core::proto::FlightState;
use regex::Regex;

use super::gfx::{ang_diff, wrap180, wrap360};

const NAN: f32 = f32::NAN;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpeedMode {
    Spd,
    Mach,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lateral {
    Hdg,
    NavGps,
    NavVor,
    Loc,
    Bc,
    Rol,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Vertical {
    Alt,
    Vs,
    Flc,
    Gs,
    Pit,
    Alts,
}

#[derive(Clone, Default, Debug)]
pub struct Fma {
    pub ap: bool,
    pub fd: bool,
    pub yd: bool,
    pub athr_armed: bool,
    pub athr_active: bool,
    pub toga: bool,
    pub speed: Option<SpeedMode>,
    pub lateral: Option<Lateral>,
    pub lateral_armed: Option<Lateral>,
    pub vertical: Option<Vertical>,
    pub vertical_armed: Vec<Vertical>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NavSource {
    #[default]
    None,
    Gps,
    Vor,
    Loc,
}

#[derive(Clone, Debug)]
pub struct NavInfo {
    pub source: NavSource,
    pub ident: Option<String>,
    pub course: f32,
    pub cdi: f32,
    pub gs: f32,
    pub dme_nm: f32,
    pub to_from: i32,
    pub loc: bool,
}
impl Default for NavInfo {
    fn default() -> Self {
        Self { source: NavSource::None, ident: None, course: NAN, cdi: NAN, gs: NAN, dme_nm: NAN, to_from: 0, loc: false }
    }
}

#[derive(Clone, Debug)]
pub struct GpsInfo {
    pub active: bool,
    pub next_id: Option<String>,
    pub prev_id: Option<String>,
    pub next_dist_nm: f32,
    pub next_brg: f32,
    pub dtk: f32,
    pub xtk_nm: f32,
    pub next_ete_sec: f32,
    pub dest_ete_sec: f32,
    pub approach: bool,
    pub next: Option<(f64, f64)>,
    pub prev: Option<(f64, f64)>,
    pub wp_index: i32,
    pub wp_count: i32,
}
impl Default for GpsInfo {
    fn default() -> Self {
        Self { active: false, next_id: None, prev_id: None, next_dist_nm: NAN, next_brg: NAN, dtk: NAN, xtk_nm: NAN, next_ete_sec: NAN, dest_ete_sec: NAN, approach: false, next: None, prev: None, wp_index: 0, wp_count: 0 }
    }
}

#[derive(Clone, Debug)]
pub struct FlightData {
    pub pitch: f32,
    pub roll: f32,
    pub heading: f32,
    pub att_valid: bool,
    pub heading_magnetic: bool,
    pub slip: f32,
    pub speed_kt: f32,
    pub speed_valid: bool,
    pub speed_trend: f32,
    pub alt_ft: f32,
    pub alt_valid: bool,
    pub vs_fpm: f32,
    pub track: f32,
    pub has_track: bool,
    pub fpa: f32,
    pub qnh: f32,
    pub std: bool,
    pub in_hg: bool,
    pub fpv: bool,
    pub ra_ft: f32,
    pub ra_valid: bool,
    pub terrain_ft: f32,
    pub lat: f64,
    pub lon: f64,
    pub msl_ft: f32,
    pub wind_dir: f32,
    pub wind_kt: f32,
    pub wind_valid: bool,
    pub tas_kt: f32,
    pub oat_c: f32,
    pub drift: f32,
    pub g_load: f32,
    pub mag_var: f32,
    pub gs_kt: f32,
    pub mach: f32,
    pub on_ground: bool,
    pub spd_bug: f32,
    pub mach_bug: f32,
    pub alt_bug: f32,
    pub hdg_bug: f32,
    pub vs_bug: f32,
    pub fma: Fma,
    pub fd_on: bool,
    pub fd_pitch: f32,
    pub fd_roll: f32,
    pub nav: NavInfo,
    pub gps: GpsInfo,
    pub brg2: f32,
    pub brg_adf: f32,
    pub minimums_ft: f32,
    pub marker: i32,
    pub vs0: f32,
    pub vs1: f32,
    pub vc: f32,
    pub vmo: f32,
    pub stall_warn: bool,
    pub overspeed_warn: bool,
    pub turn_rate: f32,
    pub ai_pitch: f32,
    pub ai_roll: f32,
    pub hi_heading: f32,
    pub suction: f32,
    pub com1: f32,
    pub com1_stby: f32,
    pub nav1: f32,
    pub nav1_stby: f32,
    pub squawk: Option<String>,
    pub xpdr_mode: Option<String>,
    pub gear_down: Option<bool>,
    pub flaps_index: Option<i32>,
    pub flaps_positions: Option<i32>,
    pub spoilers_pct: f32,
    pub sim_time: Option<String>,
    /// Silniki (do pasków EIS na MFD i zegarów silnika).
    pub engines: Vec<Engine>,
    pub fuel_kg: f32,
    pub piston: bool,
    pub trim_pct: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Engine {
    pub n1: f32,
    pub n2: f32,
    pub rpm: f32,
    pub egt: f32,
    pub ff: f32,
    pub thr: f32,
}

impl Default for FlightData {
    fn default() -> Self {
        FlightData {
            pitch: 0.0,
            roll: 0.0,
            heading: 0.0,
            att_valid: false,
            heading_magnetic: true,
            slip: 0.0,
            speed_kt: 0.0,
            speed_valid: false,
            speed_trend: 0.0,
            alt_ft: 0.0,
            alt_valid: false,
            vs_fpm: 0.0,
            track: 0.0,
            has_track: false,
            fpa: 0.0,
            qnh: 1013.25,
            std: false,
            in_hg: false,
            fpv: true,
            ra_ft: 0.0,
            ra_valid: false,
            terrain_ft: NAN,
            lat: 0.0,
            lon: 0.0,
            msl_ft: 0.0,
            wind_dir: 0.0,
            wind_kt: 0.0,
            wind_valid: false,
            tas_kt: NAN,
            oat_c: NAN,
            drift: 0.0,
            g_load: 1.0,
            mag_var: 0.0,
            gs_kt: NAN,
            mach: NAN,
            on_ground: false,
            spd_bug: NAN,
            mach_bug: NAN,
            alt_bug: NAN,
            hdg_bug: NAN,
            vs_bug: NAN,
            fma: Fma::default(),
            fd_on: false,
            fd_pitch: NAN,
            fd_roll: NAN,
            nav: NavInfo::default(),
            gps: GpsInfo::default(),
            brg2: NAN,
            brg_adf: NAN,
            minimums_ft: NAN,
            marker: 0,
            vs0: NAN,
            vs1: NAN,
            vc: NAN,
            vmo: NAN,
            stall_warn: false,
            overspeed_warn: false,
            turn_rate: NAN,
            ai_pitch: NAN,
            ai_roll: NAN,
            hi_heading: NAN,
            suction: NAN,
            com1: NAN,
            com1_stby: NAN,
            nav1: NAN,
            nav1_stby: NAN,
            squawk: None,
            xpdr_mode: None,
            gear_down: None,
            flaps_index: None,
            flaps_positions: None,
            spoilers_pct: NAN,
            sim_time: None,
            engines: Vec::new(),
            fuel_kg: NAN,
            piston: false,
            trim_pct: NAN,
        }
    }
}

fn some_str(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Pozycja punktu GPS: MSFS podaje 0/0, gdy punktu nie ma.
fn valid_pos(lat: f64, lon: f64) -> Option<(f64, f64)> {
    if (lat != 0.0 || lon != 0.0) && lat.is_finite() && lon.is_finite() {
        Some((lat, lon))
    } else {
        None
    }
}

/// Cale słupa rtęci jak w Ameryce Północnej (USA, Kanada), w pozostałych regionach hektopaskale.
pub fn uses_in_hg(lat: f64, lon: f64) -> bool {
    (14.0..=72.0).contains(&lat) && (-170.0..=-50.0).contains(&lon)
}

impl FlightData {
    /// Wypełnia dane z symulatora (jak `FlightState.toFlightData` w telefonie).
    pub fn from_state(s: &FlightState, speed_trend: f32) -> FlightData {
        let av = s.avionics.as_ref();
        let mag_var = av.map(|a| a.mag_var_deg as f32).unwrap_or_else(|| wrap180((s.attitude.hdg_true_deg - s.attitude.hdg_mag_deg) as f32));
        let hdg = s.attitude.hdg_mag_deg as f32;
        let gs = s.speed.gs_kt as f32;
        let trk = wrap360(s.attitude.track_true_deg as f32 - mag_var);
        let vs = s.speed.vs_fpm as f32;
        let alt = s.position.alt_ind_ft as f32;
        let agl = s.position.alt_agl_ft as f32;
        let ra = av.map(|a| a.radio_alt_ft as f32).unwrap_or(agl);
        let (lat, lon) = (s.position.lat, s.position.lon);
        let baro = av.map(|a| a.baro_hpa).unwrap_or(s.environment.qnh_hpa) as f32;
        let baro = if baro > 800.0 { baro } else { 1013.25 };
        let ap = &s.autopilot;
        let ex = av.map(|a| &a.ap);

        let lateral = if ex.map(|e| e.back_course).unwrap_or(false) {
            Some(Lateral::Bc)
        } else if ap.modes.loc || ap.modes.apr {
            Some(Lateral::Loc)
        } else if ap.modes.nav {
            Some(if av.map(|a| a.gps.drives_nav1).unwrap_or(false) { Lateral::NavGps } else { Lateral::NavVor })
        } else if ap.modes.hdg {
            Some(Lateral::Hdg)
        } else if ex.map(|e| e.wing_leveler).unwrap_or(false) || ap.master {
            Some(Lateral::Rol)
        } else {
            None
        };
        let vertical = if ex.map(|e| e.gs_active).unwrap_or(false) {
            Some(Vertical::Gs)
        } else if ap.modes.alt {
            Some(Vertical::Alt)
        } else if ap.modes.flc {
            Some(Vertical::Flc)
        } else if ap.modes.vs {
            Some(Vertical::Vs)
        } else if ex.map(|e| e.pitch_hold).unwrap_or(false) || ap.master {
            Some(Vertical::Pit)
        } else {
            None
        };
        let mut v_armed = Vec::new();
        if ex.map(|e| e.alt_armed).unwrap_or(false) && vertical != Some(Vertical::Alt) {
            v_armed.push(Vertical::Alts);
        }
        if ex.map(|e| e.gs_armed).unwrap_or(false) && vertical != Some(Vertical::Gs) {
            v_armed.push(Vertical::Gs);
        }
        let fma = Fma {
            ap: ap.master,
            fd: ap.fd || av.map(|a| a.fd.active).unwrap_or(false),
            yd: ex.map(|e| e.yaw_damper).unwrap_or(false),
            athr_armed: ap.athr,
            athr_active: ap.athr && ex.map(|e| e.airspeed_hold || e.mach_hold).unwrap_or(true),
            toga: ex.map(|e| e.toga).unwrap_or(false),
            speed: if ex.map(|e| e.mach_hold).unwrap_or(false) {
                Some(SpeedMode::Mach)
            } else if ex.map(|e| e.airspeed_hold).unwrap_or(false) || ap.athr {
                Some(SpeedMode::Spd)
            } else {
                None
            },
            lateral,
            lateral_armed: if ap.modes.apr && lateral != Some(Lateral::Loc) { Some(Lateral::Loc) } else { None },
            vertical,
            vertical_armed: v_armed,
        };

        let mut d = FlightData::default();
        if let Some(a) = av {
            let n1 = &a.nav1;
            let g = &a.gps;
            let gps_nav = g.active_flight_plan && (g.drives_nav1 || !n1.has_nav);
            d.nav = if gps_nav {
                NavInfo {
                    source: NavSource::Gps,
                    ident: some_str(&g.next_id),
                    course: g.desired_track_mag_deg as f32,
                    cdi: ((-g.cross_track_nm / if g.approach_active { 1.0 } else { 5.0 } * 2.5).clamp(-2.5, 2.5)) as f32,
                    gs: if n1.has_gs { n1.gs_dots as f32 } else { NAN },
                    dme_nm: g.next_dist_nm as f32,
                    to_from: 1,
                    loc: false,
                }
            } else if n1.has_nav {
                NavInfo {
                    source: if n1.has_loc { NavSource::Loc } else { NavSource::Vor },
                    ident: some_str(&n1.ident),
                    course: (if n1.has_loc { n1.loc_course_deg } else { n1.obs_deg }) as f32,
                    cdi: n1.cdi_dots as f32,
                    gs: if n1.has_gs { n1.gs_dots as f32 } else { NAN },
                    dme_nm: if n1.has_dme { n1.dme_nm as f32 } else { NAN },
                    to_from: match n1.to_from.as_str() {
                        "to" => 1,
                        "from" => 2,
                        _ => 0,
                    },
                    loc: n1.has_loc,
                }
            } else {
                NavInfo::default()
            };
            d.gps = GpsInfo {
                active: g.active_flight_plan,
                next_id: some_str(&g.next_id),
                prev_id: some_str(&g.prev_id),
                next_dist_nm: g.next_dist_nm as f32,
                next_brg: g.next_bearing_mag_deg as f32,
                dtk: g.desired_track_mag_deg as f32,
                xtk_nm: g.cross_track_nm as f32,
                next_ete_sec: g.next_ete_sec as f32,
                dest_ete_sec: g.dest_ete_sec as f32,
                approach: g.approach_active,
                next: valid_pos(g.next_lat, g.next_lon),
                prev: valid_pos(g.prev_lat, g.prev_lon),
                wp_index: g.wp_index,
                wp_count: g.wp_count,
            };
            let n2 = &a.nav2;
            d.brg2 = if n2.has_nav && !n2.has_loc { wrap360(n2.radial_deg as f32 + 180.0) } else { NAN };
            d.brg_adf = if a.adf.has_signal { wrap360(a.adf.rel_bearing_deg as f32 + hdg) } else { NAN };
            d.minimums_ft = if a.decision_height_ft > 0.0 { a.decision_height_ft as f32 } else { NAN };
            d.marker = match a.marker.as_str() {
                "outer" => 1,
                "middle" => 2,
                "inner" => 3,
                _ => 0,
            };
            let pos = |v: f64| if v > 1.0 { v as f32 } else { NAN };
            d.vs0 = pos(a.speeds.vs0_kt);
            d.vs1 = pos(a.speeds.vs1_kt);
            d.vc = pos(a.speeds.vc_kt);
            d.vmo = pos(a.speeds.vmo_kt);
            d.stall_warn = a.stall_warning;
            d.overspeed_warn = a.overspeed_warning;
            d.turn_rate = a.turn_rate_dps as f32;
            d.ai_pitch = a.att_ind_pitch_deg as f32;
            d.ai_roll = a.att_ind_bank_deg as f32;
            d.hi_heading = a.hdg_ind_deg as f32;
            d.suction = a.suction_in_hg as f32;
            d.fd_on = fma.fd;
            d.fd_pitch = a.fd.pitch_deg as f32;
            d.fd_roll = a.fd.bank_deg as f32;
            d.slip = (a.slip_ball * 0.25) as f32;
            d.g_load = a.g_force as f32;
            d.std = a.baro_std;
            d.mach_bug = if a.ap.mach_hold && a.ap.mach_var > 0.0 { a.ap.mach_var as f32 } else { NAN };
        }
        d.pitch = s.attitude.pitch_deg as f32;
        d.roll = s.attitude.bank_deg as f32;
        d.heading = wrap360(hdg);
        d.att_valid = true;
        d.speed_kt = s.speed.ias_kt as f32;
        d.speed_valid = true;
        d.speed_trend = speed_trend;
        d.alt_ft = alt;
        d.alt_valid = true;
        d.vs_fpm = vs;
        d.track = trk;
        d.has_track = gs > 30.0;
        d.fpa = if gs > 30.0 { (vs / (gs * 101.27)).atan().to_degrees() } else { 0.0 };
        d.qnh = baro;
        d.in_hg = uses_in_hg(lat, lon);
        d.ra_ft = ra;
        d.ra_valid = true;
        d.terrain_ft = alt - agl;
        d.lat = lat;
        d.lon = lon;
        d.msl_ft = s.position.alt_msl_ft as f32;
        d.wind_dir = wrap360(s.environment.wind_dir_deg as f32 - mag_var);
        d.wind_kt = s.environment.wind_spd_kt as f32;
        d.wind_valid = true;
        d.tas_kt = s.speed.tas_kt as f32;
        d.oat_c = s.environment.oat_c as f32;
        d.drift = ang_diff(trk, hdg);
        d.mag_var = mag_var;
        d.gs_kt = gs;
        d.mach = s.speed.mach as f32;
        d.on_ground = s.aircraft.on_ground;
        d.spd_bug = if ap.spd_kt > 0.0 { ap.spd_kt as f32 } else { NAN };
        d.alt_bug = ap.alt_ft as f32;
        d.hdg_bug = ap.hdg_deg as f32;
        d.vs_bug = if ap.modes.vs { ap.vs_fpm as f32 } else { NAN };
        d.fma = fma;
        d.com1 = s.radios.com1.active_mhz as f32;
        d.com1_stby = s.radios.com1.standby_mhz as f32;
        d.nav1 = s.radios.nav1.active_mhz as f32;
        d.nav1_stby = s.radios.nav1.standby_mhz as f32;
        d.squawk = some_str(&s.radios.transponder.code);
        d.xpdr_mode = some_str(&s.radios.transponder.mode);
        d.gear_down = Some(s.controls.gear_down);
        d.flaps_index = Some(s.controls.flaps_index);
        d.flaps_positions = Some(s.controls.flaps_positions);
        d.spoilers_pct = s.controls.spoilers_pct as f32;
        d.sim_time = s.sim_time_utc.as_ref().and_then(|t| {
            let i = t.find('T')?;
            let hm = t.get(i + 1..i + 6)?;
            Some(format!("{}{}Z", &hm[0..2], &hm[3..5]))
        });
        d.engines = s.engines.iter().map(|e| Engine { n1: e.n1_pct as f32, n2: e.n2_pct as f32, rpm: e.rpm as f32, egt: e.egt_c as f32, ff: e.fuel_flow_kg_h as f32, thr: e.throttle_pct as f32 }).collect();
        d.fuel_kg = s.fuel.total_kg as f32;
        d.piston = s.aircraft.engine_type == "piston" || s.engines.iter().all(|e| e.n1_pct < 0.5);
        d.trim_pct = s.controls.elevator_trim_pct as f32;
        d
    }
}

// ---------- styl wyświetlaczy ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum DisplayStyle {
    Airbus,
    Boeing,
    B787,
    Atr,
    Embraer,
    Garmin,
    Cessna,
    Heli,
    Glider,
}

impl DisplayStyle {
    pub const ALL: [DisplayStyle; 9] = [DisplayStyle::Airbus, DisplayStyle::Boeing, DisplayStyle::B787, DisplayStyle::Atr, DisplayStyle::Embraer, DisplayStyle::Garmin, DisplayStyle::Cessna, DisplayStyle::Heli, DisplayStyle::Glider];

    pub fn label(self) -> &'static str {
        match self {
            DisplayStyle::Airbus => "A320",
            DisplayStyle::Boeing => "737",
            DisplayStyle::B787 => "787",
            DisplayStyle::Atr => "ATR 72",
            DisplayStyle::Embraer => "E190",
            DisplayStyle::Garmin => "G1000",
            DisplayStyle::Cessna => "C172",
            DisplayStyle::Heli => "R44",
            DisplayStyle::Glider => "Glider",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            DisplayStyle::Airbus => "A320",
            DisplayStyle::Boeing => "B737",
            DisplayStyle::B787 => "B787",
            DisplayStyle::Atr => "ATR72",
            DisplayStyle::Embraer => "E190",
            DisplayStyle::Garmin => "G1000",
            DisplayStyle::Cessna => "C172",
            DisplayStyle::Heli => "R44",
            DisplayStyle::Glider => "GLIDER",
        }
    }
    pub fn from_key(k: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == k)
    }
    /// Panel zegarów analogowych (rysowany na całym polu, bez wirtualnego kwadratu PFD).
    pub fn analog(self) -> bool {
        matches!(self, DisplayStyle::Cessna | DisplayStyle::Heli | DisplayStyle::Glider)
    }

    /// Styl rozpoznany z danych samolotu (jak AircraftProfile.detect w telefonie).
    pub fn detect(s: &FlightState) -> Self {
        struct Re {
            airbus: Regex,
            heavy: Regex,
            boeing: Regex,
            atr: Regex,
            primus: Regex,
            glider: Regex,
            classic: Regex,
        }
        static RE: OnceLock<Re> = OnceLock::new();
        let re = RE.get_or_init(|| Re {
            airbus: Regex::new(r"(?i)\b(airbus|a3[0-9]{2}|a2[0-9]{2}|a220|fenix|fbw|flybywire)").unwrap(),
            heavy: Regex::new(r"(?i)\b(b?78[789]|b?77[0-9w]|b?74[0-9]|dreamliner)").unwrap(),
            boeing: Regex::new(r"(?i)\b(boeing|b?7[0-9]7|pmdg|md-?11|md-?8[0-9])").unwrap(),
            atr: Regex::new(r"(?i)\b(atr\s?-?\s?[47]2)").unwrap(),
            primus: Regex::new(r"(?i)\b(e-?1[79]0|e-?1[79]5|erj|embraer|crj|q400|dash ?8|dhc-?8)").unwrap(),
            glider: Regex::new(r"(?i)\b(glider|szybowiec|ask ?2[1-3]|ls ?[468]|discus|dg-?[0-9]{3}|arcus|duo|ventus|nimbus|swift s1|astir|jantar|pirat|bocian|junior|grob 10[23])").unwrap(),
            classic: Regex::new(r"(?i)\b(classic|steam|analog|c152|cessna 152|cub|j-?3|super cub|citabria|decathlon|extra ?330|cap ?10|zlin|savage|husky|aviat|pitts|dr1|stearman|spitfire|p-?51|vl3|icon a5|wilga|an-?2)").unwrap(),
        });
        let a = &s.aircraft;
        let text = format!("{} {} {}", a.title, a.atc_model, a.atc_type);
        let (cat, eng) = (a.category.as_str(), a.engine_type.as_str());
        if cat == "helicopter" || eng == "helicopter" {
            DisplayStyle::Heli
        } else if re.glider.is_match(&text) || (eng == "none" && cat == "ga") {
            DisplayStyle::Glider
        } else if re.airbus.is_match(&text) {
            DisplayStyle::Airbus
        } else if re.heavy.is_match(&text) {
            DisplayStyle::B787
        } else if re.boeing.is_match(&text) {
            DisplayStyle::Boeing
        } else if re.atr.is_match(&text) {
            DisplayStyle::Atr
        } else if re.primus.is_match(&text) {
            DisplayStyle::Embraer
        } else if cat == "airliner" {
            if eng == "turboprop" {
                DisplayStyle::Atr
            } else {
                DisplayStyle::Airbus
            }
        } else if re.classic.is_match(&text) {
            DisplayStyle::Cessna
        } else {
            DisplayStyle::Garmin
        }
    }
}
