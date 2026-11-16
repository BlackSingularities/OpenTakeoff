//! Jedna definicja danych SimConnect (kolejność pól = kolejność w definicji, dane upakowane bez wyrównania)
//! i zamiana na stan lotu protokołu. Nazwy i jednostki SimVarów z dokumentacji MSFS SDK.

use opentakeoff_core::commands::decode_squawk;
use opentakeoff_core::proto::*;
use opentakeoff_core::touchdown::{classify, clean_atc_string, engine_type_name};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    F,
    S256,
    S64,
}

pub struct Cursor<'a> {
    b: &'a [u8],
    o: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Self { b, o: 0 }
    }
    pub fn f64(&mut self) -> f64 {
        let v = self.b.get(self.o..self.o + 8).map(|s| f64::from_le_bytes(s.try_into().unwrap())).unwrap_or(0.0);
        self.o += 8;
        v
    }
    pub fn f32(&mut self) -> f32 {
        let v = self.b.get(self.o..self.o + 4).map(|s| f32::from_le_bytes(s.try_into().unwrap())).unwrap_or(0.0);
        self.o += 4;
        v
    }
    pub fn i32(&mut self) -> i32 {
        let v = self.b.get(self.o..self.o + 4).map(|s| i32::from_le_bytes(s.try_into().unwrap())).unwrap_or(0);
        self.o += 4;
        v
    }
    pub fn u32(&mut self) -> u32 {
        self.i32() as u32
    }
    pub fn str(&mut self, n: usize) -> String {
        let s = self.b.get(self.o..(self.o + n).min(self.b.len())).map(ascii).unwrap_or_default();
        self.o += n;
        s
    }
}

/// Tekst ASCII zakończony zerem.
pub fn ascii(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

macro_rules! ty {
    (F) => { f64 };
    (S256) => { String };
    (S64) => { String };
}

macro_rules! rd {
    ($c:ident, F) => { $c.f64() };
    ($c:ident, S256) => { $c.str(256) };
    ($c:ident, S64) => { $c.str(64) };
}

macro_rules! simvars {
    ($( $field:ident : $kind:ident = $name:literal , $unit:literal ; )*) => {
        #[derive(Clone, Debug, Default)]
        pub struct MsfsData { $( pub $field: ty!($kind), )* }
        pub const VARS: &[(&str, &str, Kind)] = &[ $( ($name, $unit, Kind::$kind), )* ];
        impl MsfsData {
            pub fn parse(b: &[u8]) -> Self {
                let mut c = Cursor::new(b);
                Self { $( $field: rd!(c, $kind), )* }
            }
        }
    };
}

simvars! {
    title: S256 = "TITLE", "";
    on_ground: F = "SIM ON GROUND", "bool";
    zulu_time: F = "ZULU TIME", "seconds";
    zulu_day_of_year: F = "ZULU DAY OF YEAR", "number";
    zulu_year: F = "ZULU YEAR", "number";
    lat: F = "PLANE LATITUDE", "degrees";
    lon: F = "PLANE LONGITUDE", "degrees";
    alt_msl: F = "PLANE ALTITUDE", "feet";
    alt_agl: F = "PLANE ALT ABOVE GROUND", "feet";
    alt_ind: F = "INDICATED ALTITUDE", "feet";
    ias: F = "AIRSPEED INDICATED", "knots";
    tas: F = "AIRSPEED TRUE", "knots";
    gs: F = "GROUND VELOCITY", "knots";
    mach: F = "AIRSPEED MACH", "mach";
    vs: F = "VERTICAL SPEED", "feet per minute";
    pitch: F = "PLANE PITCH DEGREES", "degrees";
    bank: F = "PLANE BANK DEGREES", "degrees";
    hdg_true: F = "PLANE HEADING DEGREES TRUE", "degrees";
    hdg_mag: F = "PLANE HEADING DEGREES MAGNETIC", "degrees";
    track: F = "GPS GROUND TRUE TRACK", "degrees";
    ap_master: F = "AUTOPILOT MASTER", "bool";
    ap_fd: F = "AUTOPILOT FLIGHT DIRECTOR ACTIVE", "bool";
    ap_athr_arm: F = "AUTOPILOT THROTTLE ARM", "bool";
    ap_athr_active: F = "AUTOTHROTTLE ACTIVE", "bool";
    ap_hdg: F = "AUTOPILOT HEADING LOCK DIR", "degrees";
    ap_alt: F = "AUTOPILOT ALTITUDE LOCK VAR", "feet";
    ap_vs: F = "AUTOPILOT VERTICAL HOLD VAR", "feet per minute";
    ap_spd: F = "AUTOPILOT AIRSPEED HOLD VAR", "knots";
    ap_hdg_mode: F = "AUTOPILOT HEADING LOCK", "bool";
    ap_nav_mode: F = "AUTOPILOT NAV1 LOCK", "bool";
    ap_apr_mode: F = "AUTOPILOT APPROACH HOLD", "bool";
    ap_apr_is_loc: F = "AUTOPILOT APPROACH IS LOCALIZER", "bool";
    ap_alt_mode: F = "AUTOPILOT ALTITUDE LOCK", "bool";
    ap_vs_mode: F = "AUTOPILOT VERTICAL HOLD", "bool";
    ap_flc_mode: F = "AUTOPILOT FLIGHT LEVEL CHANGE", "bool";
    com1_active: F = "COM ACTIVE FREQUENCY:1", "MHz";
    com1_standby: F = "COM STANDBY FREQUENCY:1", "MHz";
    com2_active: F = "COM ACTIVE FREQUENCY:2", "MHz";
    com2_standby: F = "COM STANDBY FREQUENCY:2", "MHz";
    nav1_active: F = "NAV ACTIVE FREQUENCY:1", "MHz";
    nav1_standby: F = "NAV STANDBY FREQUENCY:1", "MHz";
    nav2_active: F = "NAV ACTIVE FREQUENCY:2", "MHz";
    nav2_standby: F = "NAV STANDBY FREQUENCY:2", "MHz";
    xpdr_code: F = "TRANSPONDER CODE:1", "Bco16";
    xpdr_state: F = "TRANSPONDER STATE:1", "Enum";
    xpdr_ident: F = "TRANSPONDER IDENT:1", "bool";
    gear_handle: F = "GEAR HANDLE POSITION", "bool";
    gear_pct: F = "GEAR TOTAL PCT EXTENDED", "percent";
    flaps_index: F = "FLAPS HANDLE INDEX", "number";
    flaps_positions: F = "FLAPS NUM HANDLE POSITIONS", "number";
    flaps_pct: F = "FLAPS HANDLE PERCENT", "percent";
    spoilers_pct: F = "SPOILERS HANDLE POSITION", "percent";
    spoilers_armed: F = "SPOILERS ARMED", "bool";
    parking_brake: F = "BRAKE PARKING POSITION", "bool";
    elev_trim_pct: F = "ELEVATOR TRIM PCT", "percent";
    throttle1: F = "GENERAL ENG THROTTLE LEVER POSITION:1", "percent";
    mixture1: F = "GENERAL ENG MIXTURE LEVER POSITION:1", "percent";
    prop1: F = "GENERAL ENG PROPELLER LEVER POSITION:1", "percent";
    camera_state: F = "CAMERA STATE", "Enum";
    cam_pitch_rad: F = "CAMERA GAMEPLAY PITCH YAW:0", "radians";
    cam_yaw_rad: F = "CAMERA GAMEPLAY PITCH YAW:1", "radians";
    light_beacon: F = "LIGHT BEACON", "bool";
    light_strobe: F = "LIGHT STROBE", "bool";
    light_nav: F = "LIGHT NAV", "bool";
    light_landing: F = "LIGHT LANDING", "bool";
    light_taxi: F = "LIGHT TAXI", "bool";
    light_logo: F = "LIGHT LOGO", "bool";
    light_wing: F = "LIGHT WING", "bool";
    light_panel: F = "LIGHT PANEL", "bool";
    light_cabin: F = "LIGHT CABIN", "bool";
    fuel_total_lbs: F = "FUEL TOTAL QUANTITY WEIGHT", "pounds";
    fuel_lbs_per_gal: F = "FUEL WEIGHT PER GALLON", "pounds";
    fuel_left_gal: F = "FUEL TANK LEFT MAIN QUANTITY", "gallons";
    fuel_center_gal: F = "FUEL TANK CENTER QUANTITY", "gallons";
    fuel_right_gal: F = "FUEL TANK RIGHT MAIN QUANTITY", "gallons";
    engine_count: F = "NUMBER OF ENGINES", "number";
    n1_1: F = "TURB ENG N1:1", "percent";
    n2_1: F = "TURB ENG N2:1", "percent";
    rpm1: F = "GENERAL ENG RPM:1", "rpm";
    egt1: F = "GENERAL ENG EXHAUST GAS TEMPERATURE:1", "celsius";
    ff1: F = "ENG FUEL FLOW PPH:1", "pounds per hour";
    n1_2: F = "TURB ENG N1:2", "percent";
    n2_2: F = "TURB ENG N2:2", "percent";
    rpm2: F = "GENERAL ENG RPM:2", "rpm";
    egt2: F = "GENERAL ENG EXHAUST GAS TEMPERATURE:2", "celsius";
    ff2: F = "ENG FUEL FLOW PPH:2", "pounds per hour";
    n1_3: F = "TURB ENG N1:3", "percent";
    n2_3: F = "TURB ENG N2:3", "percent";
    rpm3: F = "GENERAL ENG RPM:3", "rpm";
    egt3: F = "GENERAL ENG EXHAUST GAS TEMPERATURE:3", "celsius";
    ff3: F = "ENG FUEL FLOW PPH:3", "pounds per hour";
    n1_4: F = "TURB ENG N1:4", "percent";
    n2_4: F = "TURB ENG N2:4", "percent";
    rpm4: F = "GENERAL ENG RPM:4", "rpm";
    egt4: F = "GENERAL ENG EXHAUST GAS TEMPERATURE:4", "celsius";
    ff4: F = "ENG FUEL FLOW PPH:4", "pounds per hour";
    throttle2: F = "GENERAL ENG THROTTLE LEVER POSITION:2", "percent";
    throttle3: F = "GENERAL ENG THROTTLE LEVER POSITION:3", "percent";
    throttle4: F = "GENERAL ENG THROTTLE LEVER POSITION:4", "percent";
    wind_dir: F = "AMBIENT WIND DIRECTION", "degrees";
    wind_spd: F = "AMBIENT WIND VELOCITY", "knots";
    oat: F = "AMBIENT TEMPERATURE", "celsius";
    qnh: F = "KOHLSMAN SETTING MB:1", "millibars";
    engine_type: F = "ENGINE TYPE", "Enum";
    max_gross_weight_lbs: F = "MAX GROSS WEIGHT", "pounds";
    atc_model: S64 = "ATC MODEL", "";
    atc_type: S64 = "ATC TYPE", "";
    sim_category: S64 = "CATEGORY", "";
    g_force: F = "G FORCE", "GForce";
    td_vs: F = "PLANE TOUCHDOWN NORMAL VELOCITY", "feet per minute";
    td_pitch: F = "PLANE TOUCHDOWN PITCH DEGREES", "degrees";
    td_bank: F = "PLANE TOUCHDOWN BANK DEGREES", "degrees";
    td_lat: F = "PLANE TOUCHDOWN LATITUDE", "degrees";
    td_lon: F = "PLANE TOUCHDOWN LONGITUDE", "degrees";
    baro_std: F = "KOHLSMAN SETTING STD:1", "bool";
    pressure_alt: F = "PRESSURE ALTITUDE", "feet";
    radio_alt: F = "RADIO HEIGHT", "feet";
    decision_height: F = "DECISION HEIGHT", "feet";
    slip_ball: F = "TURN COORDINATOR BALL", "position 128";
    turn_rate: F = "TURN INDICATOR RATE", "degrees per second";
    aoa: F = "INCIDENCE ALPHA", "degrees";
    att_ind_pitch: F = "ATTITUDE INDICATOR PITCH DEGREES", "degrees";
    att_ind_bank: F = "ATTITUDE INDICATOR BANK DEGREES", "degrees";
    hdg_ind: F = "HEADING INDICATOR", "degrees";
    suction: F = "SUCTION PRESSURE", "inHg";
    mag_var: F = "MAGVAR", "degrees";
    tat: F = "TOTAL AIR TEMPERATURE", "celsius";
    vs0: F = "DESIGN SPEED VS0", "knots";
    vs1: F = "DESIGN SPEED VS1", "knots";
    vc: F = "DESIGN SPEED VC", "knots";
    barber_pole: F = "AIRSPEED BARBER POLE", "knots";
    stall_warning: F = "STALL WARNING", "bool";
    overspeed_warning: F = "OVERSPEED WARNING", "bool";
    marker: F = "MARKER BEACON STATE", "Enum";
    nav1_has: F = "NAV HAS NAV:1", "bool";
    nav1_has_loc: F = "NAV HAS LOCALIZER:1", "bool";
    nav1_has_gs: F = "NAV HAS GLIDE SLOPE:1", "bool";
    nav1_has_dme: F = "NAV HAS DME:1", "bool";
    nav1_ident: S64 = "NAV IDENT:1", "";
    nav1_obs: F = "NAV OBS:1", "degrees";
    nav1_loc_course: F = "NAV LOCALIZER:1", "degrees";
    nav1_radial: F = "NAV RADIAL:1", "degrees";
    nav1_cdi: F = "NAV CDI:1", "number";
    nav1_gsi: F = "NAV GSI:1", "number";
    nav1_dme: F = "NAV DME:1", "nautical miles";
    nav1_to_from: F = "NAV TOFROM:1", "Enum";
    nav2_has: F = "NAV HAS NAV:2", "bool";
    nav2_has_loc: F = "NAV HAS LOCALIZER:2", "bool";
    nav2_has_dme: F = "NAV HAS DME:2", "bool";
    nav2_ident: S64 = "NAV IDENT:2", "";
    nav2_obs: F = "NAV OBS:2", "degrees";
    nav2_radial: F = "NAV RADIAL:2", "degrees";
    nav2_cdi: F = "NAV CDI:2", "number";
    nav2_dme: F = "NAV DME:2", "nautical miles";
    nav2_to_from: F = "NAV TOFROM:2", "Enum";
    adf_signal: F = "ADF SIGNAL:1", "number";
    adf_radial: F = "ADF RADIAL:1", "degrees";
    gps_active_plan: F = "GPS IS ACTIVE FLIGHT PLAN", "bool";
    gps_approach: F = "GPS IS APPROACH ACTIVE", "bool";
    gps_drives_nav1: F = "GPS DRIVES NAV1", "bool";
    gps_next_id: S64 = "GPS WP NEXT ID", "";
    gps_prev_id: S64 = "GPS WP PREV ID", "";
    gps_next_dist: F = "GPS WP DISTANCE", "nautical miles";
    gps_next_bearing_true: F = "GPS WP BEARING", "degrees";
    gps_dtk_true: F = "GPS WP DESIRED TRACK", "degrees";
    gps_xtk: F = "GPS WP CROSS TRK", "nautical miles";
    gps_next_ete: F = "GPS WP ETE", "seconds";
    gps_dest_ete: F = "GPS ETE", "seconds";
    gps_next_lat: F = "GPS WP NEXT LAT", "degrees";
    gps_next_lon: F = "GPS WP NEXT LON", "degrees";
    gps_prev_lat: F = "GPS WP PREV LAT", "degrees";
    gps_prev_lon: F = "GPS WP PREV LON", "degrees";
    gps_wp_index: F = "GPS FLIGHT PLAN WP INDEX", "number";
    gps_wp_count: F = "GPS FLIGHT PLAN WP COUNT", "number";
    fd_pitch: F = "AUTOPILOT FLIGHT DIRECTOR PITCH", "degrees";
    fd_bank: F = "AUTOPILOT FLIGHT DIRECTOR BANK", "degrees";
    ap_alt_arm: F = "AUTOPILOT ALTITUDE ARM", "bool";
    ap_gs_arm: F = "AUTOPILOT GLIDESLOPE ARM", "bool";
    ap_gs_hold: F = "AUTOPILOT GLIDESLOPE HOLD", "bool";
    ap_bc: F = "AUTOPILOT BACKCOURSE HOLD", "bool";
    ap_wing_leveler: F = "AUTOPILOT WING LEVELER", "bool";
    ap_pitch_hold: F = "AUTOPILOT PITCH HOLD", "bool";
    ap_spd_hold: F = "AUTOPILOT AIRSPEED HOLD", "bool";
    ap_mach_hold: F = "AUTOPILOT MACH HOLD", "bool";
    ap_toga: F = "AUTOPILOT TAKEOFF POWER ACTIVE", "bool";
    ap_yaw_damper: F = "AUTOPILOT YAW DAMPER", "bool";
    ap_mach_var: F = "AUTOPILOT MACH HOLD VAR", "number";
    brake_left: F = "BRAKE LEFT POSITION", "percent";
    brake_right: F = "BRAKE RIGHT POSITION", "percent";
}

const LBS_TO_KG: f64 = 0.45359237;

fn b(v: f64) -> bool {
    v != 0.0
}

fn mag(true_deg: f64, mag_var: f64) -> f64 {
    r((((true_deg - mag_var) % 360.0) + 360.0) % 360.0, 1)
}

fn dots(raw: f64, full: f64) -> f64 {
    r((raw / full * 2.5).clamp(-2.5, 2.5), 2)
}

fn to_from(v: f64) -> String {
    match v as i64 {
        1 => "to",
        2 => "from",
        _ => "off",
    }
    .into()
}

/// Czas symulatora (UTC) z ZULU YEAR / DAY OF YEAR / TIME.
fn sim_time(d: &MsfsData) -> Option<String> {
    if d.zulu_year < 1900.0 || d.zulu_day_of_year < 1.0 {
        return None;
    }
    let base = chrono_like(d.zulu_year as i32, d.zulu_day_of_year as i64, d.zulu_time.floor() as i64)?;
    Some(base)
}

fn chrono_like(year: i32, day: i64, secs: i64) -> Option<String> {
    // Dni od 1970-01-01 do 1 stycznia roku `year`
    let days_before = |y: i32| -> i64 {
        let y1 = (y - 1) as i64;
        y1 * 365 + y1 / 4 - y1 / 100 + y1 / 400 - (1969 * 365 + 1969 / 4 - 1969 / 100 + 1969 / 400)
    };
    let total = (days_before(year) + day - 1) * 86_400 + secs;
    let s = total;
    let (days, rem) = (s.div_euclid(86_400), s.rem_euclid(86_400));
    // Kalendarz gregoriański z liczby dni (algorytm H. Hinnanta)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dd = doy - (153 * mp + 2) / 5 + 1;
    let mm = if mp < 10 { mp + 3 } else { mp - 9 };
    let yy = if mm <= 2 { y + 1 } else { y };
    Some(format!("{yy:04}-{mm:02}-{dd:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60))
}

pub fn map(d: &MsfsData, paused: bool, axes_active: bool) -> FlightState {
    let lbs_per_gal = if d.fuel_lbs_per_gal > 0.0 { d.fuel_lbs_per_gal } else { 6.7 };
    let count = (d.engine_count as i64).clamp(0, 4) as usize;
    let raw = [
        (d.n1_1, d.n2_1, d.rpm1, d.egt1, d.ff1, d.throttle1),
        (d.n1_2, d.n2_2, d.rpm2, d.egt2, d.ff2, d.throttle2),
        (d.n1_3, d.n2_3, d.rpm3, d.egt3, d.ff3, d.throttle3),
        (d.n1_4, d.n2_4, d.rpm4, d.egt4, d.ff4, d.throttle4),
    ];
    let engines = raw[..count]
        .iter()
        .map(|&(n1, n2, rpm, egt, ff, thr)| EngineData { n1_pct: r(n1, 1), n2_pct: r(n2, 1), rpm: r(rpm, 0), egt_c: r(egt, 0), fuel_flow_kg_h: r(ff * LBS_TO_KG, 0), throttle_pct: r(thr, 0) })
        .collect();
    let engine_type = engine_type_name(d.engine_type as i64).to_string();
    let (atc_model, atc_type) = (clean_atc_string(&d.atc_model), clean_atc_string(&d.atc_type));
    let category = classify(&engine_type, d.engine_count as i64, d.max_gross_weight_lbs, &format!("{} {atc_model} {atc_type}", d.title), &d.sim_category).to_string();
    FlightState {
        source: "msfs".into(),
        aircraft: AircraftInfo { title: d.title.clone(), on_ground: b(d.on_ground), paused, engine_type, atc_model, atc_type, category },
        sim_time_utc: sim_time(d),
        position: PositionData { lat: d.lat, lon: d.lon, alt_msl_ft: r(d.alt_msl, 0), alt_agl_ft: r(d.alt_agl, 0), alt_ind_ft: r(d.alt_ind, 0) },
        speed: SpeedData { ias_kt: r(d.ias, 1), tas_kt: r(d.tas, 1), gs_kt: r(d.gs, 1), mach: r(d.mach, 3), vs_fpm: r(d.vs, 0) },
        // MSFS: pochylenie dodatnie = nos w dół, przechylenie dodatnie = w lewo → odwracamy.
        attitude: AttitudeData { pitch_deg: r(-d.pitch, 1), bank_deg: r(-d.bank, 1), hdg_true_deg: r(d.hdg_true, 1), hdg_mag_deg: r(d.hdg_mag, 1), track_true_deg: r(d.track, 1) },
        autopilot: AutopilotData {
            master: b(d.ap_master),
            fd: b(d.ap_fd),
            athr: b(d.ap_athr_arm) || b(d.ap_athr_active),
            hdg_deg: r(d.ap_hdg, 0),
            alt_ft: r(d.ap_alt, 0),
            vs_fpm: r(d.ap_vs, 0),
            spd_kt: r(d.ap_spd, 0),
            modes: AutopilotModes {
                hdg: b(d.ap_hdg_mode),
                nav: b(d.ap_nav_mode),
                apr: b(d.ap_apr_mode) && !b(d.ap_apr_is_loc),
                loc: b(d.ap_apr_mode) && b(d.ap_apr_is_loc),
                alt: b(d.ap_alt_mode),
                vs: b(d.ap_vs_mode),
                flc: b(d.ap_flc_mode),
            },
        },
        radios: RadiosData {
            com1: RadioPair { active_mhz: r(d.com1_active, 3), standby_mhz: r(d.com1_standby, 3) },
            com2: RadioPair { active_mhz: r(d.com2_active, 3), standby_mhz: r(d.com2_standby, 3) },
            nav1: RadioPair { active_mhz: r(d.nav1_active, 3), standby_mhz: r(d.nav1_standby, 3) },
            nav2: RadioPair { active_mhz: r(d.nav2_active, 3), standby_mhz: r(d.nav2_standby, 3) },
            transponder: TransponderData { code: decode_squawk(d.xpdr_code as u32), mode: transponder_mode_name(d.xpdr_state as i64).into(), ident: b(d.xpdr_ident) },
        },
        controls: ControlsData {
            gear_down: b(d.gear_handle),
            gear_pct: r(d.gear_pct, 0),
            flaps_index: d.flaps_index as i32,
            flaps_positions: d.flaps_positions as i32 + 1,
            flaps_pct: r(d.flaps_pct, 0),
            spoilers_pct: r(d.spoilers_pct, 0),
            spoilers_armed: b(d.spoilers_armed),
            parking_brake: b(d.parking_brake),
            elevator_trim_pct: r(d.elev_trim_pct, 1),
            throttle_pct: r(d.throttle1, 0),
            mixture_pct: r(d.mixture1, 0),
            prop_pct: r(d.prop1, 0),
            camera_state: d.camera_state as i32,
            camera_pitch_deg: r(-d.cam_pitch_rad.to_degrees(), 0),
            camera_yaw_deg: r(d.cam_yaw_rad.to_degrees(), 0),
        },
        lights: LightsData {
            beacon: b(d.light_beacon),
            strobe: b(d.light_strobe),
            nav: b(d.light_nav),
            landing: b(d.light_landing),
            taxi: b(d.light_taxi),
            logo: b(d.light_logo),
            wing: b(d.light_wing),
            panel: b(d.light_panel),
            cabin: b(d.light_cabin),
        },
        fuel: FuelData {
            total_kg: r(d.fuel_total_lbs * LBS_TO_KG, 0),
            tanks: FuelTanks {
                left_kg: r(d.fuel_left_gal * lbs_per_gal * LBS_TO_KG, 0),
                center_kg: r(d.fuel_center_gal * lbs_per_gal * LBS_TO_KG, 0),
                right_kg: r(d.fuel_right_gal * lbs_per_gal * LBS_TO_KG, 0),
            },
        },
        engines,
        environment: EnvironmentData { wind_dir_deg: r(d.wind_dir, 0), wind_spd_kt: r(d.wind_spd, 0), oat_c: r(d.oat, 1), qnh_hpa: r(d.qnh, 1) },
        axes: AxesData { active: axes_active },
        avionics: Some(avionics(d)),
        touchdown: None,
    }
}

fn avionics(d: &MsfsData) -> AvionicsData {
    let mag_var = if d.mag_var > 180.0 { d.mag_var - 360.0 } else { d.mag_var };
    AvionicsData {
        baro_hpa: r(d.qnh, 1),
        baro_std: b(d.baro_std),
        pressure_alt_ft: r(d.pressure_alt, 0),
        radio_alt_ft: r(d.radio_alt, 0),
        decision_height_ft: r(d.decision_height, 0),
        slip_ball: r((d.slip_ball / 128.0).clamp(-1.0, 1.0), 3),
        turn_rate_dps: r(d.turn_rate, 2),
        g_force: r(d.g_force, 2),
        aoa_deg: r(d.aoa, 1),
        att_ind_pitch_deg: r(-d.att_ind_pitch, 1),
        att_ind_bank_deg: r(-d.att_ind_bank, 1),
        hdg_ind_deg: r(d.hdg_ind, 1),
        suction_in_hg: r(d.suction, 2),
        mag_var_deg: r(mag_var, 1),
        tat_c: r(d.tat, 1),
        speeds: SpeedLimits { vs0_kt: r(d.vs0, 0), vs1_kt: r(d.vs1, 0), vc_kt: r(d.vc, 0), vmo_kt: r(d.barber_pole, 0) },
        stall_warning: b(d.stall_warning),
        overspeed_warning: b(d.overspeed_warning),
        marker: match d.marker as i64 {
            1 => "outer",
            2 => "middle",
            3 => "inner",
            _ => "none",
        }
        .into(),
        nav1: NavReceiver {
            has_nav: b(d.nav1_has),
            has_loc: b(d.nav1_has_loc),
            has_gs: b(d.nav1_has_gs),
            has_dme: b(d.nav1_has_dme),
            ident: clean_atc_string(&d.nav1_ident),
            obs_deg: r(d.nav1_obs, 0),
            loc_course_deg: r(d.nav1_loc_course, 0),
            radial_deg: r(d.nav1_radial, 0),
            cdi_dots: dots(d.nav1_cdi, 127.0),
            gs_dots: dots(d.nav1_gsi, 119.0),
            dme_nm: r(d.nav1_dme, 1),
            to_from: to_from(d.nav1_to_from),
        },
        nav2: NavReceiver {
            has_nav: b(d.nav2_has),
            has_loc: b(d.nav2_has_loc),
            has_gs: false,
            has_dme: b(d.nav2_has_dme),
            ident: clean_atc_string(&d.nav2_ident),
            obs_deg: r(d.nav2_obs, 0),
            loc_course_deg: 0.0,
            radial_deg: r(d.nav2_radial, 0),
            cdi_dots: dots(d.nav2_cdi, 127.0),
            gs_dots: 0.0,
            dme_nm: r(d.nav2_dme, 1),
            to_from: to_from(d.nav2_to_from),
        },
        adf: AdfReceiver { has_signal: d.adf_signal > 0.0, rel_bearing_deg: r(d.adf_radial, 0) },
        gps: GpsData {
            active_flight_plan: b(d.gps_active_plan),
            approach_active: b(d.gps_approach),
            drives_nav1: b(d.gps_drives_nav1),
            next_id: clean_atc_string(&d.gps_next_id),
            prev_id: clean_atc_string(&d.gps_prev_id),
            next_dist_nm: r(d.gps_next_dist, 1),
            next_bearing_mag_deg: mag(d.gps_next_bearing_true, mag_var),
            desired_track_mag_deg: mag(d.gps_dtk_true, mag_var),
            cross_track_nm: r(d.gps_xtk, 2),
            next_ete_sec: r(d.gps_next_ete, 0),
            dest_ete_sec: r(d.gps_dest_ete, 0),
            next_lat: r(d.gps_next_lat, 6),
            next_lon: r(d.gps_next_lon, 6),
            prev_lat: r(d.gps_prev_lat, 6),
            prev_lon: r(d.gps_prev_lon, 6),
            wp_index: d.gps_wp_index as i32,
            wp_count: d.gps_wp_count as i32,
        },
        fd: FlightDirector { active: b(d.ap_fd), pitch_deg: r(-d.fd_pitch, 1), bank_deg: r(-d.fd_bank, 1) },
        ap: ApModesExtra {
            alt_armed: b(d.ap_alt_arm),
            gs_armed: b(d.ap_gs_arm),
            gs_active: b(d.ap_gs_hold),
            back_course: b(d.ap_bc),
            wing_leveler: b(d.ap_wing_leveler),
            pitch_hold: b(d.ap_pitch_hold),
            airspeed_hold: b(d.ap_spd_hold),
            mach_hold: b(d.ap_mach_hold),
            toga: b(d.ap_toga),
            yaw_damper: b(d.ap_yaw_damper),
            mach_var: r(d.ap_mach_var, 3),
        },
        brake_left_pct: r(d.brake_left, 0),
        brake_right_pct: r(d.brake_right, 0),
    }
}

/// Rozmiar bloku danych (do sprawdzenia, czy SimConnect przysłał całą definicję).
pub fn block_size() -> usize {
    VARS.iter().map(|v| match v.2 {
        Kind::F => 8,
        Kind::S256 => 256,
        Kind::S64 => 64,
    })
    .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_packed_block() {
        let mut bytes = vec![0u8; block_size()];
        bytes[..5].copy_from_slice(b"A320\0");
        // SIM ON GROUND = 1.0
        bytes[256..264].copy_from_slice(&1.0f64.to_le_bytes());
        let d = MsfsData::parse(&bytes);
        assert_eq!(d.title, "A320");
        assert_eq!(d.on_ground, 1.0);
        let s = map(&d, false, false);
        assert!(s.aircraft.on_ground);
        assert_eq!(s.radios.transponder.code, "0000");
        assert!(s.sim_time_utc.is_none());
    }

    #[test]
    fn sim_time_format() {
        assert_eq!(chrono_like(2026, 290, 12 * 3600 + 34 * 60 + 56).unwrap(), "2026-10-17T12:34:56Z");
        assert_eq!(chrono_like(2024, 60, 0).unwrap(), "2024-02-29T00:00:00Z");
    }
}
