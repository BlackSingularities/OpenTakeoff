//! Źródło danych X-Plane 11/12 – wbudowany protokół UDP X-Plane (port 49000), bez wtyczek:
//! subskrypcje zmiennych (RREF), zapis zmiennych (DREF) i komendy (CMND). Zastępuje osobny most
//! `bridge-xplane/xplane_link.py`: telefon łączy się z OpenTakeoff na Windows tak samo jak przy MSFS.
//!
//! W X-Plane: Settings → Network → „Accept incoming connections” (UDP 49000; domyślnie włączone).

use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::json;

use crate::axes::AxisController;
use crate::commands::Validated;
use crate::lang::tr;
use crate::navdata::NavDataProvider;
use crate::proto::FlightState;
use crate::source::{tick_ms, unix_ms, CommandResult, FlightSource};
use crate::touchdown::{Sample, TouchdownLatch};

pub const XPLANE_PORT: u16 = 49000;
const RATE_HZ: i32 = 20;
const STALE_MS: i64 = 3000;
const RESUBSCRIBE_MS: i64 = 10_000;
const MAX_ENGINES: usize = 4;
const TITLE_BYTES: usize = 64;

const FT_PER_M: f64 = 3.28084;
const KT_PER_MS: f64 = 1.94384;
const HPA_PER_INHG: f64 = 33.8639;

const D: &[(&str, &str)] = &[
    ("lat", "sim/flightmodel/position/latitude"),
    ("lon", "sim/flightmodel/position/longitude"),
    ("elev", "sim/flightmodel/position/elevation"),
    ("agl", "sim/flightmodel/position/y_agl"),
    ("alt_ind", "sim/cockpit2/gauges/indicators/altitude_ft_pilot"),
    ("pitch", "sim/flightmodel/position/theta"),
    ("bank", "sim/flightmodel/position/phi"),
    ("hdg_true", "sim/flightmodel/position/psi"),
    ("hdg_mag", "sim/flightmodel/position/mag_psi"),
    ("track", "sim/flightmodel/position/hpath"),
    ("ias", "sim/flightmodel/position/indicated_airspeed"),
    ("tas", "sim/flightmodel/position/true_airspeed"),
    ("gs", "sim/flightmodel/position/groundspeed"),
    ("vs", "sim/flightmodel/position/vh_ind_fpm"),
    ("aoa", "sim/flightmodel/position/alpha"),
    ("magvar", "sim/flightmodel/position/magnetic_variation"),
    ("mach", "sim/flightmodel/misc/machno"),
    ("g", "sim/flightmodel/forces/g_nrml"),
    ("on_ground", "sim/flightmodel/failures/onground_any"),
    ("paused", "sim/time/paused"),
    ("zulu", "sim/time/zulu_time_sec"),
    ("radio_alt", "sim/cockpit2/gauges/indicators/radio_altimeter_height_ft_pilot"),
    ("baro", "sim/cockpit2/gauges/actuators/barometer_setting_in_hg_pilot"),
    ("slip", "sim/cockpit2/gauges/indicators/slip_deg"),
    ("turn", "sim/cockpit2/gauges/indicators/turn_rate_heading_deg_pilot"),
    ("stall", "sim/cockpit2/annunciators/stall_warning"),
    ("servos", "sim/cockpit2/autopilot/servos_on"),
    ("fd", "sim/cockpit2/autopilot/flight_director_mode"),
    ("athr", "sim/cockpit2/autopilot/autothrottle_enabled"),
    ("ap_hdg", "sim/cockpit2/autopilot/heading_dial_deg_mag_pilot"),
    ("ap_alt", "sim/cockpit2/autopilot/altitude_dial_ft"),
    ("ap_vs", "sim/cockpit2/autopilot/vvi_dial_fpm"),
    ("ap_spd", "sim/cockpit2/autopilot/airspeed_dial_kts"),
    ("st_hdg", "sim/cockpit2/autopilot/heading_status"),
    ("st_nav", "sim/cockpit2/autopilot/nav_status"),
    ("st_apr", "sim/cockpit2/autopilot/approach_status"),
    ("st_alt", "sim/cockpit2/autopilot/altitude_hold_status"),
    ("st_vs", "sim/cockpit2/autopilot/vvi_status"),
    ("st_spd", "sim/cockpit2/autopilot/speed_status"),
    ("st_gs", "sim/cockpit2/autopilot/glideslope_status"),
    ("com1", "sim/cockpit2/radios/actuators/com1_frequency_hz_833"),
    ("com1s", "sim/cockpit2/radios/actuators/com1_standby_frequency_hz_833"),
    ("com2", "sim/cockpit2/radios/actuators/com2_frequency_hz_833"),
    ("com2s", "sim/cockpit2/radios/actuators/com2_standby_frequency_hz_833"),
    ("nav1", "sim/cockpit2/radios/actuators/nav1_frequency_hz"),
    ("nav1s", "sim/cockpit2/radios/actuators/nav1_standby_frequency_hz"),
    ("nav2", "sim/cockpit2/radios/actuators/nav2_frequency_hz"),
    ("nav2s", "sim/cockpit2/radios/actuators/nav2_standby_frequency_hz"),
    ("xpdr", "sim/cockpit2/radios/actuators/transponder_code"),
    ("xpdr_mode", "sim/cockpit2/radios/actuators/transponder_mode"),
    ("gear_handle", "sim/cockpit2/controls/gear_handle_down"),
    ("gear", "sim/flightmodel2/gear/deploy_ratio[0]"),
    ("flaps", "sim/flightmodel/controls/flaprqst"),
    ("flap_detents", "sim/aircraft/controls/acf_flap_detents"),
    ("speedbrake", "sim/cockpit2/controls/speedbrake_ratio"),
    ("parking", "sim/cockpit2/controls/parking_brake_ratio"),
    ("trim", "sim/cockpit2/controls/elevator_trim"),
    ("mixture", "sim/cockpit2/engine/actuators/mixture_ratio_all"),
    ("prop", "sim/cockpit2/engine/actuators/prop_ratio_all"),
    ("beacon", "sim/cockpit2/switches/beacon_on"),
    ("strobe", "sim/cockpit2/switches/strobe_lights_on"),
    ("nav_lights", "sim/cockpit2/switches/navigation_lights_on"),
    ("landing", "sim/cockpit2/switches/landing_lights_on"),
    ("taxi", "sim/cockpit2/switches/taxi_light_on"),
    ("fuel", "sim/flightmodel/weight/m_fuel_total"),
    ("engines", "sim/aircraft/engine/acf_num_engines"),
    ("en_type", "sim/aircraft/prop/acf_en_type[0]"),
    ("wind_dir", "sim/weather/wind_direction_degt"),
    // mimo nazwy: m/s
    ("wind_spd", "sim/weather/wind_speed_kt"),
    ("oat", "sim/cockpit2/temperature/outside_air_temp_degc"),
    ("qnh", "sim/weather/barometer_sealevel_inhg"),
];

const ENGINE_REFS: &[(&str, &str)] = &[
    ("throttle", "sim/cockpit2/engine/actuators/throttle_ratio"),
    ("n1", "sim/flightmodel/engine/ENGN_N1_"),
    ("n2", "sim/flightmodel/engine/ENGN_N2_"),
    ("rpm", "sim/cockpit2/engine/indicators/engine_speed_rpm"),
    ("egt", "sim/cockpit2/engine/indicators/EGT_deg_C"),
    ("ff", "sim/cockpit2/engine/indicators/fuel_flow_kg_sec"),
];

const TITLE_REF: &str = "sim/aircraft/view/acf_ui_name";
const OVERRIDE_JOYSTICK: &str = "sim/operation/override/override_joystick";
const YOKE_ROLL: &str = "sim/joystick/yoke_roll_ratio";
const YOKE_PITCH: &str = "sim/joystick/yoke_pitch_ratio";
const YOKE_YAW: &str = "sim/joystick/yoke_heading_ratio";
const LEFT_BRAKE: &str = "sim/cockpit2/controls/left_brake_ratio";
const RIGHT_BRAKE: &str = "sim/cockpit2/controls/right_brake_ratio";
const THROTTLE_ALL: &str = "sim/cockpit2/engine/actuators/throttle_ratio_all";

/// Komendy protokołu bez wartości → komenda X-Plane.
fn command_path(name: &str) -> Option<&'static str> {
    Some(match name {
        "AP_MASTER" => "sim/autopilot/servos_toggle",
        "AUTOPILOT_ON" => "sim/autopilot/servos_on",
        "AUTOPILOT_OFF" => "sim/autopilot/servos_off_any",
        "TOGGLE_FLIGHT_DIRECTOR" => "sim/autopilot/fdir_toggle",
        "AUTO_THROTTLE_ARM" => "sim/autopilot/autothrottle_toggle",
        "YAW_DAMPER_TOGGLE" => "sim/systems/yaw_damper_toggle",
        "AP_HDG_HOLD" => "sim/autopilot/heading",
        "AP_NAV1_HOLD" | "AP_LOC_HOLD" => "sim/autopilot/NAV",
        "AP_APR_HOLD" => "sim/autopilot/approach",
        "AP_BC_HOLD" => "sim/autopilot/back_course",
        "AP_ALT_HOLD" => "sim/autopilot/altitude_hold",
        "AP_VS_HOLD" => "sim/autopilot/vertical_speed",
        "FLIGHT_LEVEL_CHANGE" | "AP_AIRSPEED_HOLD" => "sim/autopilot/level_change",
        "AP_PANEL_SPEED_HOLD" => "sim/autopilot/autothrottle_toggle",
        "AP_WING_LEVELER" => "sim/autopilot/wing_leveler",
        "HEADING_BUG_INC" => "sim/autopilot/heading_up",
        "HEADING_BUG_DEC" => "sim/autopilot/heading_down",
        "AP_ALT_VAR_INC" => "sim/autopilot/altitude_up",
        "AP_ALT_VAR_DEC" => "sim/autopilot/altitude_down",
        "AP_VS_VAR_INC" => "sim/autopilot/vertical_speed_up",
        "AP_VS_VAR_DEC" => "sim/autopilot/vertical_speed_down",
        "AP_SPD_VAR_INC" => "sim/autopilot/airspeed_up",
        "AP_SPD_VAR_DEC" => "sim/autopilot/airspeed_down",
        "COM_STBY_RADIO_SWAP" => "sim/radios/com1_standy_flip",
        "COM2_RADIO_SWAP" => "sim/radios/com2_standy_flip",
        "NAV1_RADIO_SWAP" => "sim/radios/nav1_standy_flip",
        "NAV2_RADIO_SWAP" => "sim/radios/nav2_standy_flip",
        "XPNDR_IDENT_ON" => "sim/transponder/transponder_ident",
        "GEAR_UP" => "sim/flight_controls/landing_gear_up",
        "GEAR_DOWN" => "sim/flight_controls/landing_gear_down",
        "GEAR_TOGGLE" => "sim/flight_controls/landing_gear_toggle",
        "FLAPS_INCR" => "sim/flight_controls/flaps_down",
        "FLAPS_DECR" => "sim/flight_controls/flaps_up",
        "TOGGLE_BEACON_LIGHTS" => "sim/lights/beacon_lights_toggle",
        "STROBES_TOGGLE" => "sim/lights/strobe_lights_toggle",
        "TOGGLE_NAV_LIGHTS" => "sim/lights/nav_lights_toggle",
        "LANDING_LIGHTS_TOGGLE" => "sim/lights/landing_lights_toggle",
        "TOGGLE_TAXI_LIGHTS" => "sim/lights/taxi_lights_toggle",
        "ELEV_TRIM_UP" => "sim/flight_controls/pitch_trim_up",
        "ELEV_TRIM_DN" => "sim/flight_controls/pitch_trim_down",
        "AILERON_TRIM_LEFT" => "sim/flight_controls/aileron_trim_left",
        "AILERON_TRIM_RIGHT" => "sim/flight_controls/aileron_trim_right",
        "RUDDER_TRIM_LEFT" => "sim/flight_controls/rudder_trim_left",
        "RUDDER_TRIM_RIGHT" => "sim/flight_controls/rudder_trim_right",
        "THROTTLE_REVERSE_THRUST_TOGGLE" => "sim/engines/thrust_reverse_toggle",
        "AUTO_THROTTLE_TO_GA" => "sim/engines/TOGA_power",
        _ => return None,
    })
}

/// Komendy z wartością → (zmienna X-Plane, przeliczenie wartości protokołu).
fn setter(name: &str) -> Option<(String, fn(i64) -> f32)> {
    fn same(v: i64) -> f32 {
        v as f32
    }
    fn ratio(v: i64) -> f32 {
        v as f32 / 16383.0
    }
    fn khz(v: i64) -> f32 {
        (v / 1000) as f32
    }
    fn ten_khz(v: i64) -> f32 {
        (v / 10_000) as f32
    }
    let p = |k: &str| path(k).to_string();
    Some(match name {
        "HEADING_BUG_SET" => (p("ap_hdg"), same),
        "AP_ALT_VAR_SET_ENGLISH" => (p("ap_alt"), same),
        "AP_VS_VAR_SET_ENGLISH" => (p("ap_vs"), same),
        "AP_SPD_VAR_SET" => (p("ap_spd"), same),
        "COM_RADIO_SET_HZ" => (p("com1"), khz),
        "COM_STBY_RADIO_SET_HZ" => (p("com1s"), khz),
        "COM2_RADIO_SET_HZ" => (p("com2"), khz),
        "COM2_STBY_RADIO_SET_HZ" => (p("com2s"), khz),
        "NAV1_RADIO_SET_HZ" => (p("nav1"), ten_khz),
        "NAV1_STBY_SET_HZ" => (p("nav1s"), ten_khz),
        "NAV2_RADIO_SET_HZ" => (p("nav2"), ten_khz),
        "NAV2_STBY_SET_HZ" => (p("nav2s"), ten_khz),
        "FLAPS_SET" => (p("flaps"), ratio),
        "SPOILERS_SET" => (p("speedbrake"), ratio),
        "PARKING_BRAKE_SET" => (p("parking"), same),
        "ELEVATOR_TRIM_SET" => (p("trim"), ratio),
        "THROTTLE_SET" => (THROTTLE_ALL.to_string(), ratio),
        "THROTTLE1_SET" => ("sim/cockpit2/engine/actuators/throttle_ratio[0]".to_string(), ratio),
        "THROTTLE2_SET" => ("sim/cockpit2/engine/actuators/throttle_ratio[1]".to_string(), ratio),
        "THROTTLE3_SET" => ("sim/cockpit2/engine/actuators/throttle_ratio[2]".to_string(), ratio),
        "THROTTLE4_SET" => ("sim/cockpit2/engine/actuators/throttle_ratio[3]".to_string(), ratio),
        "MIXTURE_SET" => (p("mixture"), ratio),
        "PROPELLER_SET" => (p("prop"), ratio),
        _ => return None,
    })
}

fn path(key: &str) -> &'static str {
    D.iter().find(|(k, _)| *k == key).map(|(_, p)| *p).unwrap_or("")
}

/// Numeracja protokołu: off, standby, test, on, alt, ground. X-Plane: off, standby, on, alt, test, ground.
fn xpdr_to_xplane(n: i64) -> f32 {
    match n {
        2 => 4.0,
        3 => 2.0,
        4 => 3.0,
        x => x as f32,
    }
}

fn xpdr_name(n: i64) -> &'static str {
    match n {
        1 => "standby",
        2 => "on",
        3 | 6 | 7 => "alt",
        4 => "test",
        5 => "ground",
        _ => "off",
    }
}

fn engine_type_name(t: i64) -> &'static str {
    match t {
        0 | 1 | 3 => "piston",
        9 | 10 => "turboprop",
        5 | 7 => "jet",
        _ => "none",
    }
}

fn classify(engine_type: &str, engines: i64, title: &str) -> &'static str {
    const BIZ: &[&str] = &["CITATION", "LEARJET", "PHENOM", "GULFSTREAM", "CHALLENGER", "FALCON", "HONDAJET", "CJ4", "SF50", "PC-24"];
    const AIRLINER: &[&str] = &[
        "A300", "A310", "A318", "A319", "A320", "A321", "A330", "A340", "A350", "A380", "737", "747", "757", "767", "777", "787", "CRJ", "E170", "E175", "E190", "E195", "MD-11", "MD-80", "MD-82", "DC-10", "ATR", "DASH 8",
        "DHC-8", "Q400", "CONCORDE",
    ];
    const HELI: &[&str] = &["HELICOPTER", "R22", "R44", "R66", "S-76", "EC135", "BELL "];
    let t = title.to_uppercase();
    if HELI.iter().any(|w| t.contains(w)) {
        return "helicopter";
    }
    let airliner = AIRLINER.iter().any(|w| t.contains(w));
    let biz = BIZ.iter().any(|w| t.contains(w));
    match engine_type {
        "jet" => {
            if biz && !airliner {
                "bizjet"
            } else if airliner || engines >= 3 {
                "airliner"
            } else {
                "bizjet"
            }
        }
        "turboprop" => {
            if airliner {
                "airliner"
            } else {
                "turboprop"
            }
        }
        _ => "ga",
    }
}

struct Inner {
    sock: Mutex<Option<Arc<UdpSocket>>>,
    target: Mutex<SocketAddr>,
    paths: Vec<String>,
    index: HashMap<&'static str, usize>,
    values: Mutex<Vec<f32>>,
    last_data: AtomicI64,
    running: AtomicBool,
    snapshot: Mutex<Option<FlightState>>,
    latch: Mutex<TouchdownLatch>,
    axes_engaged: AtomicBool,
    last_speedbrake: Mutex<Option<f64>>,
    brake_on: AtomicBool,
    generation: AtomicI64,
}

/// Źródło X-Plane. `defer` – gdy podane i połączone (MSFS w trybie automatycznym), X-Plane nie
/// steruje osiami, żeby dwa źródła nie pisały jednocześnie.
pub struct XPlaneSource {
    axes: Arc<AxisController>,
    inner: Arc<Inner>,
    defer: Mutex<Option<Arc<dyn FlightSource>>>,
}

fn resolve(host: &str) -> SocketAddr {
    let h = if host.trim().is_empty() { "127.0.0.1" } else { host.trim() };
    // „adres:port” – inny port niż 49000 (np. testy albo przekierowanie)
    let addr = if h.contains(':') { h.to_string() } else { format!("{h}:{XPLANE_PORT}") };
    addr.to_socket_addrs().ok().and_then(|mut a| a.next()).unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], XPLANE_PORT)))
}

impl XPlaneSource {
    pub fn new(axes: Arc<AxisController>, host: &str) -> Self {
        let mut paths: Vec<String> = D.iter().map(|(_, p)| p.to_string()).collect();
        let mut index: HashMap<&'static str, usize> = D.iter().enumerate().map(|(i, (k, _))| (*k, i)).collect();
        for (k, p) in ENGINE_REFS {
            for i in 0..MAX_ENGINES {
                let key: &'static str = Box::leak(format!("{k}{i}").into_boxed_str());
                index.insert(key, paths.len());
                paths.push(format!("{p}[{i}]"));
            }
        }
        for i in 0..TITLE_BYTES {
            paths.push(format!("{TITLE_REF}[{i}]"));
        }
        let n = paths.len();
        Self {
            axes,
            inner: Arc::new(Inner {
                sock: Mutex::new(None),
                target: Mutex::new(resolve(host)),
                paths,
                index,
                values: Mutex::new(vec![0.0; n]),
                last_data: AtomicI64::new(i64::MIN / 2),
                running: AtomicBool::new(false),
                snapshot: Mutex::new(None),
                latch: Mutex::new(TouchdownLatch::new()),
                axes_engaged: AtomicBool::new(false),
                last_speedbrake: Mutex::new(None),
                brake_on: AtomicBool::new(false),
                generation: AtomicI64::new(0),
            }),
            defer: Mutex::new(None),
        }
    }

    /// Źródło, któremu X-Plane ustępuje osi (tryb automatyczny).
    pub fn defer_to(&self, src: Arc<dyn FlightSource>) {
        *self.defer.lock() = Some(src);
    }

    /// Adres komputera z X-Plane (pusty – ten komputer).
    pub fn set_host(&self, host: &str) {
        *self.inner.target.lock() = resolve(host);
    }
}

impl Inner {
    fn send(&self, data: &[u8]) {
        let sock = self.sock.lock().clone();
        if let Some(s) = sock {
            let target = *self.target.lock();
            let _ = s.send_to(data, target);
        }
    }

    fn rref(&self, freq: i32, idx: i32, path: &str) {
        let mut b = Vec::with_capacity(413);
        b.extend_from_slice(b"RREF\0");
        b.extend_from_slice(&freq.to_le_bytes());
        b.extend_from_slice(&idx.to_le_bytes());
        let mut p = [0u8; 400];
        let bytes = path.as_bytes();
        let n = bytes.len().min(399);
        p[..n].copy_from_slice(&bytes[..n]);
        b.extend_from_slice(&p);
        self.send(&b);
    }

    fn subscribe(&self, on: bool) {
        let title_start = self.paths.len() - TITLE_BYTES;
        for (i, p) in self.paths.iter().enumerate() {
            let freq = if !on {
                0
            } else if i >= title_start {
                1
            } else {
                RATE_HZ
            };
            self.rref(freq, i as i32, p);
            // Paczkami – seria kilkuset datagramów przepełnia bufor UDP X-Plane
            if i % 16 == 15 {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    fn write(&self, path: &str, value: f32) {
        let mut b = Vec::with_capacity(509);
        b.extend_from_slice(b"DREF\0");
        b.extend_from_slice(&value.to_le_bytes());
        let mut p = [0u8; 500];
        let bytes = path.as_bytes();
        let n = bytes.len().min(499);
        p[..n].copy_from_slice(&bytes[..n]);
        b.extend_from_slice(&p);
        self.send(&b);
    }

    fn command(&self, path: &str) {
        let mut b = Vec::with_capacity(path.len() + 6);
        b.extend_from_slice(b"CMND\0");
        b.extend_from_slice(path.as_bytes());
        b.push(0);
        self.send(&b);
    }

    fn connected(&self) -> bool {
        tick_ms() - self.last_data.load(Ordering::SeqCst) < STALE_MS
    }

    fn v(&self, vals: &[f32], key: &str) -> f64 {
        self.index.get(key).and_then(|i| vals.get(*i)).map(|x| *x as f64).unwrap_or(0.0)
    }

    fn title(&self, vals: &[f32]) -> String {
        let start = self.paths.len() - TITLE_BYTES;
        let mut raw = Vec::new();
        for i in 0..TITLE_BYTES {
            let b = vals.get(start + i).copied().unwrap_or(0.0) as i64 as u8;
            if b == 0 {
                break;
            }
            raw.push(b);
        }
        String::from_utf8_lossy(&raw).trim().to_string()
    }

    fn receive(&self, data: &[u8]) {
        if data.len() < 5 || &data[..4] != b"RREF" {
            return;
        }
        let mut vals = self.values.lock();
        let mut off = 5;
        while off + 8 <= data.len() {
            let idx = i32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]);
            let val = f32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]);
            if idx >= 0 && (idx as usize) < vals.len() {
                vals[idx as usize] = val;
            }
            off += 8;
        }
        self.last_data.store(tick_ms(), Ordering::SeqCst);
    }

    fn build(&self) -> FlightState {
        let vals = self.values.lock().clone();
        let v = |k: &str| self.v(&vals, k);
        let title = self.title(&vals);
        let on_ground = v("on_ground") >= 0.5;
        let (lat, lon) = (v("lat"), v("lon"));
        let (ias, gs) = (v("ias"), v("gs") * KT_PER_MS);
        let (vs, g) = (v("vs"), if self.index.contains_key("g") { v("g") } else { 1.0 });
        let (pitch, bank, hdg) = (v("pitch"), v("bank"), v("hdg_true"));
        let touchdown = self.latch.lock().update(&Sample {
            time_ms: tick_ms(),
            unix_ms: unix_ms(),
            on_ground,
            vs_fpm: vs,
            g_force: g,
            pitch_deg: pitch,
            bank_deg: bank,
            heading_true_deg: hdg,
            lat,
            lon,
            ias_kt: ias,
            gs_kt: gs,
            ..Default::default()
        });
        let count = (v("engines") as i64).clamp(1, MAX_ENGINES as i64) as usize;
        let engines: Vec<serde_json::Value> = (0..count)
            .map(|i| {
                json!({
                    "n1Pct": round(v(&format!("n1{i}")), 1), "n2Pct": round(v(&format!("n2{i}")), 1), "rpm": v(&format!("rpm{i}")).round(),
                    "egtC": v(&format!("egt{i}")).round(), "fuelFlowKgH": round(v(&format!("ff{i}")) * 3600.0, 1),
                    "throttlePct": round(v(&format!("throttle{i}")) * 100.0, 1),
                })
            })
            .collect();
        let engine_type = engine_type_name(v("en_type") as i64);
        let detents = v("flap_detents") as i64;
        let flaps = v("flaps");
        let speedbrake = v("speedbrake");
        let zulu = v("zulu").max(0.0) as i64;
        let midnight = chrono::Utc::now().date_naive().and_hms_opt(0, 0, 0).map(|d| d.and_utc()).unwrap_or_else(chrono::Utc::now);
        let sim_time = midnight + chrono::Duration::seconds(zulu);
        let radio = |a: &str, s: &str, scale: f64| json!({"activeMhz": round(v(a) / scale, 3), "standbyMhz": round(v(s) / scale, 3)});
        let st_gs = v("st_gs");
        let state = json!({
            "source": "xplane",
            "aircraft": {"title": if title.is_empty() { "X-Plane".to_string() } else { title.clone() }, "onGround": on_ground, "paused": v("paused") >= 0.5,
                         "engineType": engine_type, "category": classify(engine_type, count as i64, &title)},
            "simTimeUtc": sim_time.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "position": {"lat": lat, "lon": lon, "altMslFt": round(v("elev") * FT_PER_M, 1), "altAglFt": round(v("agl") * FT_PER_M, 1), "altIndFt": round(v("alt_ind"), 1)},
            "speed": {"iasKt": round(ias, 1), "tasKt": round(v("tas") * KT_PER_MS, 1), "gsKt": round(gs, 1), "mach": round(v("mach"), 3), "vsFpm": vs.round()},
            "attitude": {"pitchDeg": round(pitch, 2), "bankDeg": round(bank, 2), "hdgTrueDeg": round(hdg, 1), "hdgMagDeg": round(v("hdg_mag"), 1), "trackTrueDeg": round(v("track"), 1)},
            "autopilot": {
                "master": v("servos") >= 0.5, "fd": v("fd") >= 0.5, "athr": v("athr") >= 0.5,
                "hdgDeg": v("ap_hdg").round(), "altFt": v("ap_alt").round(), "vsFpm": v("ap_vs").round(), "spdKt": v("ap_spd").round(),
                "modes": {"hdg": v("st_hdg") >= 1.0, "nav": v("st_nav") >= 1.0, "apr": v("st_apr") >= 1.0, "loc": false,
                          "alt": v("st_alt") >= 2.0, "vs": v("st_vs") >= 2.0, "flc": v("st_spd") >= 2.0},
            },
            "radios": {
                "com1": radio("com1", "com1s", 1000.0), "com2": radio("com2", "com2s", 1000.0),
                "nav1": radio("nav1", "nav1s", 100.0), "nav2": radio("nav2", "nav2s", 100.0),
                "transponder": {"code": format!("{:04}", v("xpdr") as i64), "mode": xpdr_name(v("xpdr_mode") as i64), "ident": false},
            },
            "controls": {
                "gearDown": v("gear_handle") >= 0.5, "gearPct": round(v("gear") * 100.0, 1),
                "flapsIndex": (flaps * detents as f64).round() as i32, "flapsPositions": (detents + 1) as i32, "flapsPct": round(flaps * 100.0, 1),
                "spoilersPct": round(speedbrake.max(0.0) * 100.0, 1), "spoilersArmed": speedbrake < -0.25,
                "parkingBrake": v("parking") >= 0.5, "elevatorTrimPct": round(v("trim") * 100.0, 1),
                "throttlePct": round(v("throttle0") * 100.0, 1), "mixturePct": round(v("mixture") * 100.0, 1), "propPct": round(v("prop") * 100.0, 1),
            },
            "lights": {"beacon": v("beacon") >= 0.5, "strobe": v("strobe") >= 0.5, "nav": v("nav_lights") >= 0.5, "landing": v("landing") >= 0.5, "taxi": v("taxi") >= 0.5},
            "fuel": {"totalKg": round(v("fuel"), 1)},
            "engines": engines,
            "environment": {"windDirDeg": v("wind_dir").round(), "windSpdKt": round(v("wind_spd") * KT_PER_MS, 1), "oatC": round(v("oat"), 1), "qnhHpa": round(v("qnh") * HPA_PER_INHG, 1)},
            "axes": {"active": false},
            "avionics": {
                "baroHpa": round(v("baro") * HPA_PER_INHG, 1), "radioAltFt": round(v("radio_alt"), 1),
                "slipBall": round((v("slip") / 8.0).clamp(-1.0, 1.0), 2), "turnRateDps": round(v("turn"), 2),
                "gForce": round(g, 2), "aoaDeg": round(v("aoa"), 1), "magVarDeg": round(v("magvar"), 1),
                "stallWarning": v("stall") >= 0.5,
                "ap": {"gsArmed": st_gs == 1.0, "gsActive": st_gs >= 2.0},
            },
        });
        let mut s: FlightState = match serde_json::from_value(state) {
            Ok(s) => s,
            Err(e) => {
                crate::log::warn(&format!("X-Plane: niezgodny stan: {e}"));
                if cfg!(test) {
                    panic!("X-Plane: niezgodny stan: {e}");
                }
                FlightState::default()
            }
        };
        s.touchdown = touchdown;
        s
    }
}

fn round(v: f64, d: i32) -> f64 {
    let k = 10f64.powi(d);
    (v * k).round() / k
}

impl FlightSource for XPlaneSource {
    fn kind(&self) -> &'static str {
        "xplane"
    }

    fn is_connected(&self) -> bool {
        self.inner.connected()
    }

    fn status_text(&self) -> String {
        if self.inner.connected() {
            tr("Połączono z X-Plane").to_string()
        } else {
            tr("Czekam na X-Plane (Settings → Network: UDP 49000)").to_string()
        }
    }

    fn snapshot(&self) -> Option<FlightState> {
        if !self.inner.connected() {
            return None;
        }
        let mut s = self.inner.snapshot.lock().clone()?;
        s.axes.active = self.axes.is_active();
        Some(s)
    }

    fn execute(&self, cmd: &Validated) -> CommandResult {
        if !self.inner.connected() {
            return CommandResult::fail("sim_unavailable", tr("Brak połączenia z X-Plane"));
        }
        let name = cmd.spec.name;
        let v = cmd.value;
        let x = &self.inner;
        let vals = x.values.lock().clone();
        if let Some(p) = command_path(name) {
            x.command(p);
            return CommandResult::success();
        }
        if let Some((p, conv)) = setter(name) {
            x.write(&p, conv(v));
            return CommandResult::success();
        }
        match name {
            "XPNDR_SET" => x.write(path("xpdr"), v as f32),
            "XPNDR_MODE" => x.write(path("xpdr_mode"), xpdr_to_xplane(v)),
            "CAMERA_STATE" => match v {
                2 => x.command("sim/view/3d_cockpit_cmnd_look"),
                3 => x.command("sim/view/chase"),
                4 => x.command("sim/view/free_camera"),
                _ => return CommandResult::fail("invalid_value", "CAMERA_STATE 2…4"),
            },
            "FLAPS_UP" => x.write(path("flaps"), 0.0),
            "FLAPS_DOWN" => x.write(path("flaps"), 1.0),
            "SPOILERS_ON" => x.write(path("speedbrake"), 1.0),
            "SPOILERS_OFF" => x.write(path("speedbrake"), 0.0),
            "THROTTLE_CUT" => x.write(THROTTLE_ALL, 0.0),
            "THROTTLE_FULL" => x.write(THROTTLE_ALL, 1.0),
            "SPOILERS_TOGGLE" => x.write(path("speedbrake"), if x.v(&vals, "speedbrake") > 0.0 { 0.0 } else { 1.0 }),
            "SPOILERS_ARM_TOGGLE" => x.write(path("speedbrake"), if x.v(&vals, "speedbrake") < -0.25 { 0.0 } else { -0.5 }),
            "PARKING_BRAKES" => x.write(path("parking"), if x.v(&vals, "parking") >= 0.5 { 0.0 } else { 1.0 }),
            "CAMERA_PITCH_SET" | "CAMERA_YAW_SET" => return CommandResult::fail("sim_error", tr("Rozglądanie się z telefonu działa tylko w MSFS")),
            _ => return CommandResult::fail("sim_error", format!("{} {name}", tr("X-Plane nie obsługuje komendy"))),
        }
        CommandResult::success()
    }

    fn axes(&self) -> Arc<AxisController> {
        self.axes.clone()
    }

    fn navdata(&self) -> Option<Arc<dyn NavDataProvider>> {
        None
    }

    fn start(&self) {
        if self.inner.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let generation = self.inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let sock = match UdpSocket::bind("0.0.0.0:0") {
            Ok(s) => Arc::new(s),
            Err(e) => {
                crate::log::warn(&format!("X-Plane: nie można otworzyć gniazda UDP: {e}"));
                return;
            }
        };
        let _ = sock.set_read_timeout(Some(Duration::from_millis(250)));
        *self.inner.sock.lock() = Some(sock.clone());
        // Odbiór
        {
            let inner = self.inner.clone();
            let _ = std::thread::Builder::new().name("xplane-rx".into()).spawn(move || {
                let mut buf = [0u8; 2048];
                while inner.running.load(Ordering::SeqCst) && inner.generation.load(Ordering::SeqCst) == generation {
                    if let Ok((n, _)) = sock.recv_from(&mut buf) {
                        inner.receive(&buf[..n]);
                    }
                }
            });
        }
        // Subskrypcje, migawki i osie (20 Hz)
        let inner = self.inner.clone();
        let axes = self.axes.clone();
        let defer = self.defer.lock().clone();
        let _ = std::thread::Builder::new().name("xplane".into()).spawn(move || {
            let mut last_sub = i64::MIN / 2;
            while inner.running.load(Ordering::SeqCst) && inner.generation.load(Ordering::SeqCst) == generation {
                let now = tick_ms();
                // Ponawiamy też przy połączeniu: UDP może zgubić pojedynczą subskrypcję
                if (!inner.connected() && now - last_sub > 2000) || now - last_sub > RESUBSCRIBE_MS {
                    inner.subscribe(true);
                    last_sub = tick_ms();
                }
                if inner.connected() {
                    *inner.snapshot.lock() = Some(inner.build());
                    let drive = defer.as_ref().is_none_or(|d| !d.is_connected());
                    if drive {
                        let out = axes.peek(now);
                        if !out.is_final {
                            inner.write(OVERRIDE_JOYSTICK, 1.0);
                            inner.write(YOKE_ROLL, out.aileron as f32);
                            inner.write(YOKE_PITCH, out.elevator as f32);
                            inner.write(YOKE_YAW, out.rudder as f32);
                            inner.axes_engaged.store(true, Ordering::SeqCst);
                        } else if inner.axes_engaged.swap(false, Ordering::SeqCst) {
                            inner.write(YOKE_ROLL, 0.0);
                            inner.write(YOKE_PITCH, 0.0);
                            inner.write(YOKE_YAW, 0.0);
                            inner.write(OVERRIDE_JOYSTICK, 0.0);
                        }
                        let (sb, brake) = axes.peek_aux(now);
                        {
                            let mut last = inner.last_speedbrake.lock();
                            if sb.is_some() && sb != *last {
                                inner.write(path("speedbrake"), sb.unwrap_or(0.0) as f32);
                            }
                            *last = sb;
                        }
                        if brake > 0.0 {
                            inner.write(LEFT_BRAKE, brake as f32);
                            inner.write(RIGHT_BRAKE, brake as f32);
                            inner.brake_on.store(true, Ordering::SeqCst);
                        } else if inner.brake_on.swap(false, Ordering::SeqCst) {
                            inner.write(LEFT_BRAKE, 0.0);
                            inner.write(RIGHT_BRAKE, 0.0);
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(1000 / RATE_HZ as u64));
            }
        });
    }

    fn stop(&self) {
        if !self.inner.running.swap(false, Ordering::SeqCst) {
            return;
        }
        if self.inner.axes_engaged.swap(false, Ordering::SeqCst) {
            self.inner.write(OVERRIDE_JOYSTICK, 0.0);
        }
        self.inner.subscribe(false);
        *self.inner.sock.lock() = None;
        *self.inner.snapshot.lock() = None;
        self.inner.last_data.store(i64::MIN / 2, Ordering::SeqCst);
    }

    fn sim_name(&self) -> String {
        if self.inner.connected() {
            "X-Plane".to_string()
        } else {
            String::new()
        }
    }
}

/// Tryb automatyczny: MSFS i X-Plane naraz – dane i komendy idą do tego, który działa (MSFS ma
/// pierwszeństwo, gdy działają oba).
pub struct AutoSource {
    msfs: Arc<dyn FlightSource>,
    xplane: Arc<XPlaneSource>,
}

impl AutoSource {
    pub fn new(msfs: Arc<dyn FlightSource>, xplane: Arc<XPlaneSource>) -> Self {
        xplane.defer_to(msfs.clone());
        Self { msfs, xplane }
    }

    fn active(&self) -> &dyn FlightSource {
        if !self.msfs.is_connected() && self.xplane.is_connected() {
            self.xplane.as_ref()
        } else {
            self.msfs.as_ref()
        }
    }
}

impl FlightSource for AutoSource {
    fn kind(&self) -> &'static str {
        self.active().kind()
    }
    fn is_connected(&self) -> bool {
        self.msfs.is_connected() || self.xplane.is_connected()
    }
    fn status_text(&self) -> String {
        if self.is_connected() {
            self.active().status_text()
        } else {
            tr("Czekam na MSFS albo X-Plane").to_string()
        }
    }
    fn snapshot(&self) -> Option<FlightState> {
        self.active().snapshot()
    }
    fn execute(&self, cmd: &Validated) -> CommandResult {
        self.active().execute(cmd)
    }
    fn axes(&self) -> Arc<AxisController> {
        self.msfs.axes()
    }
    fn navdata(&self) -> Option<Arc<dyn NavDataProvider>> {
        if self.msfs.is_connected() {
            self.msfs.navdata()
        } else {
            None
        }
    }
    fn start(&self) {
        self.msfs.start();
        self.xplane.start();
    }
    fn stop(&self) {
        self.msfs.stop();
        self.xplane.stop();
    }
    fn sim_name(&self) -> String {
        self.active().sim_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::validate;

    /// Atrapa X-Plane: zapamiętuje subskrypcje, odsyła wartości i zbiera komendy.
    #[test]
    fn talks_xplane_udp() {
        let fake = UdpSocket::bind("127.0.0.1:0").unwrap();
        fake.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
        let port = fake.local_addr().unwrap().port();
        let src = XPlaneSource::new(Arc::new(AxisController::default()), &format!("127.0.0.1:{port}"));
        src.start();
        let mut subs: HashMap<String, i32> = HashMap::new();
        let mut client = None;
        let mut buf = [0u8; 1024];
        let t0 = std::time::Instant::now();
        while subs.len() < src.inner.paths.len() && t0.elapsed() < Duration::from_secs(5) {
            if let Ok((n, from)) = fake.recv_from(&mut buf) {
                client = Some(from);
                if n == 413 && &buf[..5] == b"RREF " {
                    let idx = i32::from_le_bytes([buf[9], buf[10], buf[11], buf[12]]);
                    let path = String::from_utf8_lossy(&buf[13..413]).trim_end_matches(' ').to_string();
                    subs.insert(path, idx);
                }
            }
        }
        assert_eq!(subs.len(), src.inner.paths.len(), "subskrypcje");
        let client = client.unwrap();
        let mut pkt = b"RREF ".to_vec();
        for (key, val) in [("lat", 54.38f32), ("lon", 18.47), ("ias", 142.0), ("vs", -650.0), ("servos", 1.0), ("on_ground", 0.0), ("engines", 2.0)] {
            pkt.extend_from_slice(&subs[path(key)].to_le_bytes());
            pkt.extend_from_slice(&val.to_le_bytes());
        }
        let title = subs[&format!("{TITLE_REF}[0]")];
        for (i, ch) in b"A320".iter().enumerate() {
            pkt.extend_from_slice(&(title + i as i32).to_le_bytes());
            pkt.extend_from_slice(&(*ch as f32).to_le_bytes());
        }
        fake.send_to(&pkt, client).unwrap();
        let t0 = std::time::Instant::now();
        while src.snapshot().is_none() && t0.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let s = src.snapshot().expect("migawka X-Plane");
        assert_eq!(s.source, "xplane");
        assert!((s.position.lat - 54.38).abs() < 1e-3);
        assert_eq!(s.speed.ias_kt.round(), 142.0);
        assert!(s.autopilot.master);
        assert_eq!(s.aircraft.title, "A320");
        assert_eq!(s.engines.len(), 2);
        assert!(src.is_connected());
        // Komenda z telefonu → CMND
        let cmd = validate(Some("GEAR_DOWN"), None).unwrap();
        assert!(src.execute(&cmd).ok);
        let t0 = std::time::Instant::now();
        let mut got = false;
        while !got && t0.elapsed() < Duration::from_secs(3) {
            if let Ok((n, _)) = fake.recv_from(&mut buf) {
                got = n > 5 && &buf[..5] == b"CMND " && buf[5..n].starts_with(b"sim/flight_controls/landing_gear_down");
            }
        }
        assert!(got, "brak CMND");
        src.stop();
    }
}
