//! Protokół OpenTakeoff Link v1 (docs/link-protocol.md): stałe, kody błędów, stan lotu i wiadomości.
//! Kształt JSON jest taki sam jak w dawnym programie w C#, więc telefon nie widzi różnicy.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const VERSION: i64 = 1;
pub const WEBSOCKET_PORT: u16 = 47810;
pub const DISCOVERY_PORT: u16 = 47811;
pub const MAX_CLIENT_MESSAGE_BYTES: usize = 16 * 1024;
pub const DEFAULT_RATE_HZ: u32 = 10;
pub const MIN_RATE_HZ: u32 = 1;
pub const MAX_RATE_HZ: u32 = 20;
pub const MAX_NAVDATA_PER_SECOND: u32 = 10;
/// Nazwa serwera w `hello_ok` i `announce`.
pub const SERVER_NAME: &str = "OpenTakeoff";
/// Funkcje dodane do protokołu v1 po pierwszym wydaniu. `logbook_sync` – synchronizacja dziennika lotów
/// przez HTTP na tym samym porcie (docs/link-protocol.md, p. 6).
pub const FEATURES: &[&str] = &["navdata", "touchdown", "aircraft_category", "logbook_sync"];

pub mod codes {
    pub const BAD_JSON: &str = "bad_json";
    pub const UNSUPPORTED_VERSION: &str = "unsupported_version";
    pub const UNKNOWN_TYPE: &str = "unknown_type";
    pub const NOT_PAIRED: &str = "not_paired";
    pub const PAIRING_FAILED: &str = "pairing_failed";
    pub const RATE_LIMITED: &str = "rate_limited";
    pub const NOT_ALLOWED: &str = "not_allowed";
    pub const INVALID_VALUE: &str = "invalid_value";
    pub const SIM_UNAVAILABLE: &str = "sim_unavailable";
    pub const SIM_ERROR: &str = "sim_error";
    pub const NOT_FOUND: &str = "not_found";
}

/// Zaokrąglenie do `digits` miejsc; NaN i nieskończoność → 0 (protokół nie zna takich liczb).
pub fn r(v: f64, digits: i32) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    let p = 10f64.powi(digits);
    let x = (v * p).round() / p;
    if x == 0.0 {
        0.0
    } else {
        x
    }
}

// ---------- stan lotu (`state.data`) ----------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FlightState {
    pub source: String,
    pub aircraft: AircraftInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sim_time_utc: Option<String>,
    pub position: PositionData,
    pub speed: SpeedData,
    pub attitude: AttitudeData,
    pub autopilot: AutopilotData,
    pub radios: RadiosData,
    pub controls: ControlsData,
    pub lights: LightsData,
    pub fuel: FuelData,
    pub engines: Vec<EngineData>,
    pub environment: EnvironmentData,
    pub axes: AxesData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avionics: Option<AvionicsData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touchdown: Option<TouchdownData>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AircraftInfo {
    pub title: String,
    pub on_ground: bool,
    pub paused: bool,
    pub engine_type: String,
    pub atc_model: String,
    pub atc_type: String,
    pub category: String,
}

impl Default for AircraftInfo {
    fn default() -> Self {
        Self { title: String::new(), on_ground: false, paused: false, engine_type: "none".into(), atc_model: String::new(), atc_type: String::new(), category: "ga".into() }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TouchdownData {
    pub seq: i64,
    pub t: i64,
    pub vs_fpm: f64,
    pub g_force: f64,
    pub pitch_deg: f64,
    pub bank_deg: f64,
    pub heading_true_deg: f64,
    pub lat: f64,
    pub lon: f64,
    pub ias_kt: f64,
    pub gs_kt: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PositionData {
    pub lat: f64,
    pub lon: f64,
    pub alt_msl_ft: f64,
    pub alt_agl_ft: f64,
    pub alt_ind_ft: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpeedData {
    pub ias_kt: f64,
    pub tas_kt: f64,
    pub gs_kt: f64,
    pub mach: f64,
    pub vs_fpm: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AttitudeData {
    pub pitch_deg: f64,
    pub bank_deg: f64,
    pub hdg_true_deg: f64,
    pub hdg_mag_deg: f64,
    pub track_true_deg: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutopilotData {
    pub master: bool,
    pub fd: bool,
    pub athr: bool,
    pub hdg_deg: f64,
    pub alt_ft: f64,
    pub vs_fpm: f64,
    pub spd_kt: f64,
    pub modes: AutopilotModes,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AutopilotModes {
    pub hdg: bool,
    pub nav: bool,
    pub apr: bool,
    pub loc: bool,
    pub alt: bool,
    pub vs: bool,
    pub flc: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RadiosData {
    pub com1: RadioPair,
    pub com2: RadioPair,
    pub nav1: RadioPair,
    pub nav2: RadioPair,
    pub transponder: TransponderData,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RadioPair {
    pub active_mhz: f64,
    pub standby_mhz: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TransponderData {
    pub code: String,
    pub mode: String,
    pub ident: bool,
}

impl Default for TransponderData {
    fn default() -> Self {
        Self { code: "2000".into(), mode: "standby".into(), ident: false }
    }
}

pub const TRANSPONDER_MODES: [&str; 6] = ["off", "standby", "test", "on", "alt", "ground"];

pub fn transponder_mode_name(state: i64) -> &'static str {
    if (0..6).contains(&state) {
        TRANSPONDER_MODES[state as usize]
    } else {
        "off"
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ControlsData {
    pub gear_down: bool,
    pub gear_pct: f64,
    pub flaps_index: i32,
    pub flaps_positions: i32,
    pub flaps_pct: f64,
    pub spoilers_pct: f64,
    pub spoilers_armed: bool,
    pub parking_brake: bool,
    pub elevator_trim_pct: f64,
    pub throttle_pct: f64,
    pub mixture_pct: f64,
    pub prop_pct: f64,
    pub camera_state: i32,
    pub camera_pitch_deg: f64,
    pub camera_yaw_deg: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LightsData {
    pub beacon: bool,
    pub strobe: bool,
    pub nav: bool,
    pub landing: bool,
    pub taxi: bool,
    pub logo: bool,
    pub wing: bool,
    pub panel: bool,
    pub cabin: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FuelData {
    pub total_kg: f64,
    pub tanks: FuelTanks,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FuelTanks {
    pub left_kg: f64,
    pub center_kg: f64,
    pub right_kg: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineData {
    pub n1_pct: f64,
    pub n2_pct: f64,
    pub rpm: f64,
    pub egt_c: f64,
    pub fuel_flow_kg_h: f64,
    pub throttle_pct: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EnvironmentData {
    pub wind_dir_deg: f64,
    pub wind_spd_kt: f64,
    pub oat_c: f64,
    pub qnh_hpa: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AxesData {
    pub active: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AvionicsData {
    pub baro_hpa: f64,
    pub baro_std: bool,
    pub pressure_alt_ft: f64,
    pub radio_alt_ft: f64,
    pub decision_height_ft: f64,
    pub slip_ball: f64,
    pub turn_rate_dps: f64,
    pub g_force: f64,
    pub aoa_deg: f64,
    pub att_ind_pitch_deg: f64,
    pub att_ind_bank_deg: f64,
    pub hdg_ind_deg: f64,
    pub suction_in_hg: f64,
    pub mag_var_deg: f64,
    pub tat_c: f64,
    pub speeds: SpeedLimits,
    pub stall_warning: bool,
    pub overspeed_warning: bool,
    pub marker: String,
    pub nav1: NavReceiver,
    pub nav2: NavReceiver,
    pub adf: AdfReceiver,
    pub gps: GpsData,
    pub fd: FlightDirector,
    pub ap: ApModesExtra,
    pub brake_left_pct: f64,
    pub brake_right_pct: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpeedLimits {
    pub vs0_kt: f64,
    pub vs1_kt: f64,
    pub vc_kt: f64,
    pub vmo_kt: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NavReceiver {
    pub has_nav: bool,
    pub has_loc: bool,
    pub has_gs: bool,
    pub has_dme: bool,
    pub ident: String,
    pub obs_deg: f64,
    pub loc_course_deg: f64,
    pub radial_deg: f64,
    pub cdi_dots: f64,
    pub gs_dots: f64,
    pub dme_nm: f64,
    pub to_from: String,
}

impl Default for NavReceiver {
    fn default() -> Self {
        Self {
            has_nav: false,
            has_loc: false,
            has_gs: false,
            has_dme: false,
            ident: String::new(),
            obs_deg: 0.0,
            loc_course_deg: 0.0,
            radial_deg: 0.0,
            cdi_dots: 0.0,
            gs_dots: 0.0,
            dme_nm: 0.0,
            to_from: "off".into(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AdfReceiver {
    pub has_signal: bool,
    pub rel_bearing_deg: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GpsData {
    pub active_flight_plan: bool,
    pub approach_active: bool,
    pub drives_nav1: bool,
    pub next_id: String,
    pub prev_id: String,
    pub next_dist_nm: f64,
    pub next_bearing_mag_deg: f64,
    pub desired_track_mag_deg: f64,
    pub cross_track_nm: f64,
    pub next_ete_sec: f64,
    pub dest_ete_sec: f64,
    pub next_lat: f64,
    pub next_lon: f64,
    pub prev_lat: f64,
    pub prev_lon: f64,
    pub wp_index: i32,
    pub wp_count: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FlightDirector {
    pub active: bool,
    pub pitch_deg: f64,
    pub bank_deg: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ApModesExtra {
    pub alt_armed: bool,
    pub gs_armed: bool,
    pub gs_active: bool,
    pub back_course: bool,
    pub wing_leveler: bool,
    pub pitch_hold: bool,
    pub airspeed_hold: bool,
    pub mach_hold: bool,
    pub toga: bool,
    pub yaw_damper: bool,
    pub mach_var: f64,
}

// ---------- wiadomości serwer → telefon ----------

pub fn state(seq: i64, t: i64, data: &FlightState) -> String {
    json!({ "v": VERSION, "type": "state", "seq": seq, "t": t, "data": data }).to_string()
}

pub fn status(source: &str, sim_connected: bool, message: &str) -> String {
    json!({ "v": VERSION, "type": "status", "source": source, "simConnected": sim_connected, "message": message }).to_string()
}

pub fn ack(id: Option<&str>, name: &str) -> String {
    let mut m = json!({ "v": VERSION, "type": "ack", "name": name });
    if let Some(id) = id {
        m["id"] = id.into();
    }
    m.to_string()
}

/// `error` zawsze z polem `id` (także `null`).
pub fn error(id: Option<&str>, code: &str, message: &str) -> String {
    json!({ "v": VERSION, "type": "error", "id": id, "code": code, "message": message }).to_string()
}

pub fn pong(id: Option<&str>, t: Option<i64>, server_t: i64) -> String {
    let mut m = json!({ "v": VERSION, "type": "pong", "serverT": server_t });
    if let Some(id) = id {
        m["id"] = id.into();
    }
    if let Some(t) = t {
        m["t"] = t.into();
    }
    m.to_string()
}

pub struct HelloOk<'a> {
    pub id: Option<&'a str>,
    pub token: &'a str,
    pub version: &'a str,
    pub source: &'a str,
    pub rate_hz: u32,
    pub max_deflection: f64,
    pub axis_timeout_ms: i64,
}

pub fn hello_ok(h: &HelloOk) -> String {
    let mut m = json!({
        "v": VERSION, "type": "hello_ok", "token": h.token, "server": SERVER_NAME, "version": h.version,
        "source": h.source, "rateHz": h.rate_hz,
        "limits": { "maxDeflection": h.max_deflection, "axisTimeoutMs": h.axis_timeout_ms, "navdataPerSecond": MAX_NAVDATA_PER_SECOND },
        "features": FEATURES,
    });
    if let Some(id) = h.id {
        m["id"] = id.into();
    }
    m.to_string()
}

pub fn announce(version: &str, host: &str, port: u16, ips: &[String], source: &str) -> String {
    json!({ "v": VERSION, "type": "announce", "name": SERVER_NAME, "version": version, "host": host, "port": port, "ips": ips, "source": source })
        .to_string()
}

pub fn navdata(id: Option<&str>, kind: &str, data: Value) -> String {
    json!({ "v": VERSION, "type": "navdata", "id": id, "kind": kind, "data": data }).to_string()
}

// ---------- wiadomości od telefonu ----------

/// Wiadomość od telefonu po wstępnym sprawdzeniu `v` i `type`.
#[derive(Debug)]
pub struct Incoming {
    pub kind: String,
    pub id: Option<String>,
    pub root: serde_json::Map<String, Value>,
}

impl Incoming {
    /// Ok – wiadomość; Err – gotowa wiadomość `error` do odesłania.
    pub fn parse(text: &str) -> Result<Incoming, String> {
        let v: Value = serde_json::from_str(text).map_err(|_| error(None, codes::BAD_JSON, "Niepoprawny JSON"))?;
        let Value::Object(root) = v else { return Err(error(None, codes::BAD_JSON, "Oczekiwano obiektu JSON")) };
        let id = match root.get("id") {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Number(n)) => Some(n.to_string()),
            _ => None,
        };
        let version_ok = root.get("v").and_then(|v| v.as_f64()).is_some_and(|v| v == VERSION as f64);
        if !version_ok {
            return Err(error(id.as_deref(), codes::UNSUPPORTED_VERSION, &format!("Obsługiwana wersja protokołu: {VERSION}")));
        }
        let kind = match root.get("type") {
            Some(Value::String(s)) if !s.is_empty() => s.clone(),
            _ => return Err(error(id.as_deref(), codes::BAD_JSON, "Brak pola type")),
        };
        Ok(Incoming { kind, id, root })
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        self.root.get(name).and_then(|v| v.as_str())
    }

    pub fn bool(&self, name: &str) -> bool {
        self.root.get(name).and_then(|v| v.as_bool()).unwrap_or(false)
    }

    pub fn int(&self, name: &str) -> Option<i64> {
        self.root.get(name).and_then(|v| v.as_i64())
    }

    pub fn has(&self, name: &str) -> bool {
        self.root.get(name).is_some_and(|v| !v.is_null())
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.root.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_json_shape() {
        let mut s = FlightState { source: "msfs".into(), ..Default::default() };
        s.engines.push(EngineData { n1_pct: 85.1, ..Default::default() });
        let j: Value = serde_json::from_str(&state(5, 10, &s)).unwrap();
        assert_eq!(j["type"], "state");
        assert_eq!(j["data"]["aircraft"]["onGround"], false);
        assert_eq!(j["data"]["engines"][0]["n1Pct"], 85.1);
        assert!(j["data"].get("avionics").is_none());
        assert!(j["data"].get("touchdown").is_none());
        assert_eq!(j["data"]["radios"]["transponder"]["code"], "2000");
        assert_eq!(j["data"]["controls"]["elevatorTrimPct"], 0.0);
        assert_eq!(j["data"]["fuel"]["tanks"]["leftKg"], 0.0);
        assert_eq!(j["data"]["environment"]["qnhHpa"], 0.0);
    }

    #[test]
    fn parse_errors() {
        assert!(Incoming::parse("nope").unwrap_err().contains("bad_json"));
        assert!(Incoming::parse("[1]").unwrap_err().contains("bad_json"));
        assert!(Incoming::parse(r#"{"v":2,"type":"ping","id":"x"}"#).unwrap_err().contains("unsupported_version"));
        let e = Incoming::parse(r#"{"v":1,"id":7}"#).unwrap_err();
        assert!(e.contains("\"id\":\"7\""));
        let m = Incoming::parse(r#"{"v":1,"type":"ping","id":"p1","t":5}"#).unwrap();
        assert_eq!(m.kind, "ping");
        assert_eq!(m.id.as_deref(), Some("p1"));
        assert_eq!(m.int("t"), Some(5));
    }

    #[test]
    fn error_has_null_id() {
        let j: Value = serde_json::from_str(&error(None, "x", "y")).unwrap();
        assert!(j.get("id").unwrap().is_null());
        let a: Value = serde_json::from_str(&ack(None, "AP_MASTER")).unwrap();
        assert!(a.get("id").is_none());
    }

    #[test]
    fn rounding() {
        assert_eq!(r(1.256, 2), 1.26);
        assert_eq!(r(f64::NAN, 2), 0.0);
        assert_eq!(r(-0.0001, 1), 0.0);
    }
}
