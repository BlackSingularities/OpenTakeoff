//! Tryb symulacji: uproszczony lot testowy EPWA RWY33 → EPGD RWY29 bez MSFS. To nie model fizyczny –
//! daje spójne liczby dla ekranów telefonu i komputera oraz reaguje na komendy.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::axes::{AxisController, AxisOutput};
use crate::commands::{decode_squawk, Validated};
use crate::lang::tr;
use crate::navdata::{math, NavDataProvider};
use crate::proto::{codes, transponder_mode_name, *};
use crate::simnav::SimulationNavData;
use crate::source::{tick_ms, unix_ms, CommandResult, FlightSource};
use crate::trf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Parked,
    TakeoffRoll,
    Climb,
    Cruise,
    Descent,
    Approach,
    Rollout,
    Stopped,
}

const MAG_VAR: f64 = 6.0;
const NM_PER_DEG_LAT: f64 = 60.0;
pub const DEPARTURE: (f64, f64, f64, f64) = (52.1550, 20.9740, 362.0, 331.0);
pub const ARRIVAL: (f64, f64, f64, f64) = (54.3790, 18.4880, 489.0, 292.0);

fn norm(d: f64) -> f64 {
    math::norm_deg(d)
}
fn approach(value: f64, target: f64, step: f64) -> f64 {
    if value < target {
        (value + step).min(target)
    } else {
        (value - step).max(target)
    }
}
fn angle_diff(target: f64, current: f64) -> f64 {
    ((target - current + 540.0) % 360.0) - 180.0
}
fn round(v: f64, d: i32) -> f64 {
    r(v, d)
}

pub struct FlightSimulation {
    s: FlightState,
    pub phase: Phase,
    phase_time: f64,
    stopped_time: f64,
    target_cruise_ft: f64,
    target_spd_kt: f64,
    manual_speed: bool,
    manual_throttle: bool,
    touchdown_seq: i64,
}

impl Default for FlightSimulation {
    fn default() -> Self {
        Self::new()
    }
}

impl FlightSimulation {
    pub fn new() -> Self {
        let mut f = Self {
            s: FlightState::default(),
            phase: Phase::Parked,
            phase_time: 0.0,
            stopped_time: 0.0,
            target_cruise_ft: 24000.0,
            target_spd_kt: 290.0,
            manual_speed: false,
            manual_throttle: false,
            touchdown_seq: 0,
        };
        f.reset();
        f
    }

    pub fn state(&self) -> &FlightState {
        &self.s
    }

    pub fn state_mut(&mut self) -> &mut FlightState {
        &mut self.s
    }

    pub fn distance_to_arrival_nm(&self) -> f64 {
        math::distance_nm(self.s.position.lat, self.s.position.lon, ARRIVAL.0, ARRIVAL.1)
    }

    pub fn reset(&mut self) {
        let touchdown = self.s.touchdown.take();
        self.phase = Phase::Parked;
        self.phase_time = 0.0;
        self.stopped_time = 0.0;
        self.target_cruise_ft = 24000.0;
        self.target_spd_kt = 290.0;
        self.manual_speed = false;
        self.manual_throttle = false;
        let s = &mut self.s;
        s.source = "simulation".into();
        s.aircraft = AircraftInfo {
            title: "OpenTakeoff Sim A320 (symulacja)".into(),
            on_ground: true,
            paused: false,
            engine_type: "jet".into(),
            atc_model: "A320".into(),
            atc_type: "AIRBUS".into(),
            category: "airliner".into(),
        };
        s.position = PositionData { lat: DEPARTURE.0, lon: DEPARTURE.1, alt_msl_ft: DEPARTURE.2, alt_agl_ft: 0.0, alt_ind_ft: DEPARTURE.2 };
        s.speed = SpeedData::default();
        s.attitude = AttitudeData { hdg_true_deg: DEPARTURE.3, hdg_mag_deg: norm(DEPARTURE.3 - MAG_VAR), track_true_deg: DEPARTURE.3, ..Default::default() };
        s.autopilot = AutopilotData { fd: true, hdg_deg: norm(DEPARTURE.3 - MAG_VAR).round(), alt_ft: 24000.0, spd_kt: 250.0, ..Default::default() };
        s.radios = RadiosData {
            com1: RadioPair { active_mhz: 121.900, standby_mhz: 118.300 },
            com2: RadioPair { active_mhz: 121.500, standby_mhz: 122.800 },
            nav1: RadioPair { active_mhz: 110.300, standby_mhz: 113.450 },
            nav2: RadioPair { active_mhz: 113.450, standby_mhz: 110.300 },
            transponder: TransponderData { code: "2000".into(), mode: "alt".into(), ident: false },
        };
        s.controls = ControlsData { gear_down: true, gear_pct: 100.0, flaps_index: 1, flaps_positions: 5, flaps_pct: 25.0, spoilers_armed: true, mixture_pct: 100.0, prop_pct: 100.0, camera_state: 2, ..Default::default() };
        s.lights = LightsData { beacon: true, strobe: true, nav: true, landing: true, taxi: false, logo: true, wing: false, panel: true, cabin: true };
        s.fuel = FuelData { total_kg: 6400.0, tanks: FuelTanks { left_kg: 2700.0, center_kg: 1000.0, right_kg: 2700.0 } };
        s.engines = vec![EngineData::default(), EngineData::default()];
        s.environment = EnvironmentData { wind_dir_deg: 270.0, wind_spd_kt: 18.0, oat_c: 15.0, qnh_hpa: 1013.2 };
        s.touchdown = touchdown;
        self.update_engines(20.0, 0.0);
    }

    pub fn step(&mut self, dt: f64, axes: AxisOutput, utc_now_ms: i64) {
        self.phase_time += dt;
        let dist = self.distance_to_arrival_nm();
        let brg = math::bearing_deg(self.s.position.lat, self.s.position.lon, ARRIVAL.0, ARRIVAL.1);
        let manual = !axes.is_final;
        let mut target_hdg = self.s.attitude.hdg_true_deg;
        let mut target_vs = 0.0;
        let mut target_ias = self.s.speed.ias_kt;
        let mut n1 = 20.0;
        let alt = self.s.position.alt_msl_ft;
        match self.phase {
            Phase::Parked => {
                if self.phase_time > 5.0 && !self.s.controls.parking_brake {
                    self.advance(Phase::TakeoffRoll);
                }
            }
            Phase::TakeoffRoll => {
                n1 = 92.0;
                target_hdg = DEPARTURE.3;
                target_ias = 160.0;
                if self.s.speed.ias_kt >= 145.0 {
                    self.advance(Phase::Climb);
                }
            }
            Phase::Climb => {
                n1 = 90.0;
                target_hdg = self.heading_target(brg);
                target_ias = if alt < 10000.0 { 250.0 } else { self.target_spd_kt };
                target_vs = if alt < 10000.0 { 2500.0 } else { 1800.0 };
                self.s.autopilot.modes.flc = true;
                if self.s.position.alt_agl_ft > 400.0 && self.s.controls.gear_down {
                    self.set_gear(false);
                }
                if self.s.position.alt_agl_ft > 3000.0 && self.s.controls.flaps_index > 0 {
                    self.set_flaps(0);
                }
                if self.s.position.alt_agl_ft > 1000.0 && !self.s.autopilot.master {
                    let ap = &mut self.s.autopilot;
                    ap.master = true;
                    ap.athr = true;
                    ap.modes.nav = !ap.modes.hdg;
                }
                if alt > 10000.0 && self.s.lights.landing {
                    self.s.lights.landing = false;
                }
                if alt >= self.target_cruise_ft - 50.0 {
                    self.advance(Phase::Cruise);
                }
                if self.top_of_descent(dist) {
                    self.advance(Phase::Descent);
                }
            }
            Phase::Cruise => {
                n1 = 82.0;
                target_hdg = self.heading_target(brg);
                target_ias = self.target_spd_kt;
                target_vs = ((self.target_cruise_ft - alt) * 2.0).clamp(-1500.0, 1500.0);
                let ap = &mut self.s.autopilot;
                ap.modes.flc = false;
                ap.modes.alt = (self.target_cruise_ft - alt).abs() < 100.0;
                ap.modes.vs = !ap.modes.alt;
                if self.top_of_descent(dist) {
                    self.advance(Phase::Descent);
                }
            }
            Phase::Descent => {
                n1 = 35.0;
                target_hdg = self.heading_target(brg);
                target_ias = if alt < 10000.0 { 220.0 } else { self.target_spd_kt.min(280.0) };
                target_vs = -1800.0;
                let ap = &mut self.s.autopilot;
                ap.modes.alt = false;
                ap.modes.vs = true;
                ap.alt_ft = 3000.0;
                if alt < 10000.0 && !self.s.lights.landing {
                    self.s.lights.landing = true;
                }
                if dist < 12.0 || alt < 3500.0 {
                    self.advance(Phase::Approach);
                }
            }
            Phase::Approach => {
                n1 = 55.0;
                target_hdg = self.heading_target(brg);
                target_ias = if dist > 6.0 { 170.0 } else { 140.0 };
                let path_alt = ARRIVAL.2 + 50.0 + dist * 318.0;
                target_vs = ((path_alt - alt) * 3.0 - self.s.speed.gs_kt * 5.3).clamp(-1500.0, 0.0);
                let ap = &mut self.s.autopilot;
                ap.modes.vs = false;
                ap.modes.nav = false;
                ap.modes.apr = true;
                ap.modes.loc = true;
                if dist < 8.0 && !self.s.controls.gear_down {
                    self.set_gear(true);
                }
                if dist < 10.0 && self.s.controls.flaps_index < 2 {
                    self.set_flaps(2);
                }
                if dist < 5.0 && self.s.controls.flaps_index < 4 {
                    self.set_flaps(4);
                }
                if dist < 1.0 && self.s.autopilot.master {
                    self.s.autopilot.master = false;
                    self.s.autopilot.athr = false;
                }
                if self.s.position.alt_agl_ft < 5.0 || dist < 0.2 {
                    self.advance(Phase::Rollout);
                    self.latch_touchdown(utc_now_ms);
                }
            }
            Phase::Rollout => {
                n1 = if self.phase_time < 15.0 { 70.0 } else { 20.0 };
                target_hdg = self.s.attitude.hdg_true_deg;
                target_ias = 0.0;
                if self.s.speed.ias_kt < 20.0 {
                    self.advance(Phase::Stopped);
                }
            }
            Phase::Stopped => {
                n1 = 20.0;
                target_ias = 0.0;
                self.stopped_time += dt;
                if self.stopped_time > 30.0 {
                    self.reset();
                    return;
                }
            }
        }

        let airborne = matches!(self.phase, Phase::Climb | Phase::Cruise | Phase::Descent | Phase::Approach);
        let target_bank;
        if airborne && manual {
            target_bank = axes.aileron * 35.0;
            target_vs = (self.s.speed.vs_fpm + axes.elevator * 3000.0 * dt).clamp(-4000.0, 4000.0);
        } else if airborne {
            target_bank = (angle_diff(target_hdg, self.s.attitude.hdg_true_deg) * 2.0).clamp(-25.0, 25.0);
        } else {
            target_bank = 0.0;
            target_vs = 0.0;
        }

        let s = &mut self.s;
        s.attitude.bank_deg = approach(s.attitude.bank_deg, target_bank, 8.0 * dt);
        let tas_ms = s.speed.tas_kt.max(1.0) * 0.5144;
        if airborne {
            let turn = 9.81 * s.attitude.bank_deg.to_radians().tan() / tas_ms * 180.0 / std::f64::consts::PI;
            s.attitude.hdg_true_deg = norm(s.attitude.hdg_true_deg + turn * dt);
        } else if self.phase == Phase::TakeoffRoll {
            s.attitude.hdg_true_deg = DEPARTURE.3;
        }
        s.attitude.hdg_mag_deg = norm(s.attitude.hdg_true_deg - MAG_VAR);
        let accel = match self.phase {
            Phase::TakeoffRoll => 3.5,
            Phase::Rollout => 4.0 + if s.controls.spoilers_pct > 50.0 { 1.0 } else { 0.0 },
            _ => 1.5,
        };
        if s.controls.parking_brake && !airborne {
            target_ias = 0.0;
        }
        s.speed.ias_kt = approach(s.speed.ias_kt, target_ias, accel * dt).max(0.0);
        s.speed.vs_fpm = if airborne { approach(s.speed.vs_fpm, target_vs, 600.0 * dt) } else { 0.0 };
        s.position.alt_msl_ft += s.speed.vs_fpm / 60.0 * dt;
        if self.phase == Phase::Climb && s.position.alt_msl_ft > self.target_cruise_ft {
            s.position.alt_msl_ft = self.target_cruise_ft;
        }
        let total = math::distance_nm(DEPARTURE.0, DEPARTURE.1, ARRIVAL.0, ARRIVAL.1);
        let f = (1.0 - dist / total).clamp(0.0, 1.0);
        let ground = DEPARTURE.2 + (ARRIVAL.2 - DEPARTURE.2) * f;
        if !airborne || s.position.alt_msl_ft < ground {
            s.position.alt_msl_ft = s.position.alt_msl_ft.max(ground);
        }
        if !airborne {
            s.position.alt_msl_ft = ground;
        }
        s.position.alt_agl_ft = (s.position.alt_msl_ft - ground).max(0.0);
        s.position.alt_ind_ft = s.position.alt_msl_ft + (s.environment.qnh_hpa - 1013.25) * 27.0;
        s.speed.tas_kt = s.speed.ias_kt * (1.0 + 0.02 * s.position.alt_msl_ft / 1000.0);
        let oat = 15.0 - 1.98 * s.position.alt_msl_ft / 1000.0 + 3.0;
        s.environment.oat_c = round(oat, 1);
        s.speed.mach = s.speed.tas_kt / (661.47 * ((oat + 273.15) / 288.15).sqrt());
        let wind_to = (s.environment.wind_dir_deg + 180.0).to_radians();
        let wind = if airborne { s.environment.wind_spd_kt } else { 0.0 };
        let h = s.attitude.hdg_true_deg.to_radians();
        let vn = s.speed.tas_kt * h.cos() + wind * wind_to.cos();
        let ve = s.speed.tas_kt * h.sin() + wind * wind_to.sin();
        s.speed.gs_kt = (vn * vn + ve * ve).sqrt();
        s.attitude.track_true_deg = if s.speed.gs_kt > 1.0 { norm(ve.atan2(vn).to_degrees()) } else { s.attitude.hdg_true_deg };
        let dnm = s.speed.gs_kt * dt / 3600.0;
        let trk = s.attitude.track_true_deg.to_radians();
        s.position.lat += dnm * trk.cos() / NM_PER_DEG_LAT;
        s.position.lon += dnm * trk.sin() / (NM_PER_DEG_LAT * s.position.lat.to_radians().cos());
        let gamma = if s.speed.tas_kt > 30.0 { (s.speed.vs_fpm / (s.speed.tas_kt * 101.27)).atan().to_degrees() } else { 0.0 };
        s.attitude.pitch_deg = if airborne {
            gamma + 2.5
        } else if self.phase == Phase::TakeoffRoll && s.speed.ias_kt > 140.0 {
            8.0
        } else {
            0.0
        };
        s.aircraft.on_ground = !airborne;
        s.sim_time_utc = Some(chrono::DateTime::from_timestamp_millis(utc_now_ms).unwrap_or_default().format("%Y-%m-%dT%H:%M:%SZ").to_string());
        s.axes.active = manual;
        if !self.manual_speed {
            s.autopilot.spd_kt = target_ias.round();
        }
        s.autopilot.vs_fpm = (target_vs / 100.0).round() * 100.0;
        if matches!(self.phase, Phase::Climb | Phase::Cruise) {
            s.autopilot.alt_ft = self.target_cruise_ft;
        }
        if self.phase == Phase::Rollout && s.controls.spoilers_armed {
            s.controls.spoilers_pct = 100.0;
            s.controls.spoilers_armed = false;
        }
        if self.manual_throttle && !s.autopilot.athr {
            n1 = 20.0 + s.controls.throttle_pct * 0.8;
        } else {
            s.controls.throttle_pct = ((n1 - 20.0) / 0.8).clamp(0.0, 100.0).round();
        }
        let n1 = if s.controls.parking_brake && !airborne { 20.0 } else { n1 };
        self.update_engines(n1, dt);
    }

    pub fn execute(&mut self, cmd: &Validated) -> CommandResult {
        let v = cmd.value;
        let pct = |v: i64| (v as f64 / 16383.0 * 100.0).round();
        let mhz = |v: i64| round(v as f64 / 1_000_000.0, 3);
        match cmd.spec.name {
            "AP_MASTER" => self.s.autopilot.master = !self.s.autopilot.master,
            "AUTOPILOT_ON" => self.s.autopilot.master = true,
            "AUTOPILOT_OFF" => self.s.autopilot.master = false,
            "TOGGLE_FLIGHT_DIRECTOR" => self.s.autopilot.fd = !self.s.autopilot.fd,
            "AUTO_THROTTLE_ARM" | "AP_N1_HOLD" => self.s.autopilot.athr = !self.s.autopilot.athr,
            "YAW_DAMPER_TOGGLE" | "AP_BC_HOLD" | "AP_PANEL_MACH_HOLD" | "AP_BANK_HOLD" | "THROTTLE_REVERSE_THRUST_TOGGLE" | "AILERON_TRIM_LEFT" | "AILERON_TRIM_RIGHT" | "RUDDER_TRIM_LEFT"
            | "RUDDER_TRIM_RIGHT" => {}
            "AP_HDG_HOLD" => {
                let m = &mut self.s.autopilot.modes;
                m.hdg = !m.hdg;
                if m.hdg {
                    m.nav = false;
                }
            }
            "AP_NAV1_HOLD" => {
                let m = &mut self.s.autopilot.modes;
                m.nav = !m.nav;
                if m.nav {
                    m.hdg = false;
                }
            }
            "AP_APR_HOLD" => self.s.autopilot.modes.apr = !self.s.autopilot.modes.apr,
            "AP_LOC_HOLD" => self.s.autopilot.modes.loc = !self.s.autopilot.modes.loc,
            "AP_ALT_HOLD" => self.s.autopilot.modes.alt = !self.s.autopilot.modes.alt,
            "AP_VS_HOLD" => self.s.autopilot.modes.vs = !self.s.autopilot.modes.vs,
            "FLIGHT_LEVEL_CHANGE" => self.s.autopilot.modes.flc = !self.s.autopilot.modes.flc,
            "AP_PANEL_SPEED_HOLD" | "AP_AIRSPEED_HOLD" => self.manual_speed = !self.manual_speed,
            "AP_MACH_VAR_SET" => {
                let ias = self.mach_to_ias(v as f64 / 100.0);
                self.set_speed(ias)
            }
            "AP_WING_LEVELER" => {
                self.s.autopilot.modes.hdg = false;
                self.s.autopilot.modes.nav = false;
            }
            "HEADING_BUG_SET" => self.s.autopilot.hdg_deg = v as f64,
            "HEADING_BUG_INC" => self.s.autopilot.hdg_deg = norm(self.s.autopilot.hdg_deg + 1.0),
            "HEADING_BUG_DEC" => self.s.autopilot.hdg_deg = norm(self.s.autopilot.hdg_deg - 1.0),
            "AP_ALT_VAR_SET_ENGLISH" => self.set_cruise(v as f64),
            "AP_ALT_VAR_INC" => self.set_cruise(self.target_cruise_ft + 100.0),
            "AP_ALT_VAR_DEC" => self.set_cruise(self.target_cruise_ft - 100.0),
            "AP_VS_VAR_SET_ENGLISH" => self.s.autopilot.vs_fpm = v as f64,
            "AP_VS_VAR_INC" => self.s.autopilot.vs_fpm += 100.0,
            "AP_VS_VAR_DEC" => self.s.autopilot.vs_fpm -= 100.0,
            "AP_SPD_VAR_SET" => self.set_speed(v as f64),
            "AP_SPD_VAR_INC" => self.set_speed(self.target_spd_kt + 1.0),
            "AP_SPD_VAR_DEC" => self.set_speed(self.target_spd_kt - 1.0),
            "COM_RADIO_SET_HZ" => self.s.radios.com1.active_mhz = mhz(v),
            "COM_STBY_RADIO_SET_HZ" => self.s.radios.com1.standby_mhz = mhz(v),
            "COM2_RADIO_SET_HZ" => self.s.radios.com2.active_mhz = mhz(v),
            "COM2_STBY_RADIO_SET_HZ" => self.s.radios.com2.standby_mhz = mhz(v),
            "NAV1_RADIO_SET_HZ" => self.s.radios.nav1.active_mhz = mhz(v),
            "NAV1_STBY_SET_HZ" => self.s.radios.nav1.standby_mhz = mhz(v),
            "NAV2_RADIO_SET_HZ" => self.s.radios.nav2.active_mhz = mhz(v),
            "NAV2_STBY_SET_HZ" => self.s.radios.nav2.standby_mhz = mhz(v),
            "COM_STBY_RADIO_SWAP" => swap(&mut self.s.radios.com1),
            "COM2_RADIO_SWAP" => swap(&mut self.s.radios.com2),
            "NAV1_RADIO_SWAP" => swap(&mut self.s.radios.nav1),
            "NAV2_RADIO_SWAP" => swap(&mut self.s.radios.nav2),
            "XPNDR_SET" => self.s.radios.transponder.code = decode_squawk(cmd.data),
            "XPNDR_IDENT_ON" => self.s.radios.transponder.ident = true,
            "XPNDR_MODE" => self.s.radios.transponder.mode = transponder_mode_name(v).into(),
            "GEAR_UP" => self.set_gear(false),
            "GEAR_DOWN" => self.set_gear(true),
            "GEAR_TOGGLE" => self.set_gear(!self.s.controls.gear_down),
            "FLAPS_UP" => self.set_flaps(0),
            "FLAPS_DOWN" => self.set_flaps(self.s.controls.flaps_positions - 1),
            "FLAPS_INCR" => self.set_flaps(self.s.controls.flaps_index + 1),
            "FLAPS_DECR" => self.set_flaps(self.s.controls.flaps_index - 1),
            "FLAPS_SET" => {
                let n = (v as f64 / 16383.0 * (self.s.controls.flaps_positions - 1) as f64).round() as i32;
                self.set_flaps(n)
            }
            "SPOILERS_ON" => self.s.controls.spoilers_pct = 100.0,
            "SPOILERS_OFF" => self.s.controls.spoilers_pct = 0.0,
            "SPOILERS_TOGGLE" => self.s.controls.spoilers_pct = if self.s.controls.spoilers_pct > 0.0 { 0.0 } else { 100.0 },
            "SPOILERS_ARM_TOGGLE" => self.s.controls.spoilers_armed = !self.s.controls.spoilers_armed,
            "SPOILERS_SET" => self.s.controls.spoilers_pct = pct(v),
            "PARKING_BRAKES" => self.s.controls.parking_brake = !self.s.controls.parking_brake,
            "PARKING_BRAKE_SET" => self.s.controls.parking_brake = v == 1,
            "TOGGLE_BEACON_LIGHTS" => self.s.lights.beacon = !self.s.lights.beacon,
            "STROBES_TOGGLE" => self.s.lights.strobe = !self.s.lights.strobe,
            "TOGGLE_NAV_LIGHTS" => self.s.lights.nav = !self.s.lights.nav,
            "LANDING_LIGHTS_TOGGLE" => self.s.lights.landing = !self.s.lights.landing,
            "TOGGLE_TAXI_LIGHTS" => self.s.lights.taxi = !self.s.lights.taxi,
            "TOGGLE_LOGO_LIGHTS" => self.s.lights.logo = !self.s.lights.logo,
            "TOGGLE_WING_LIGHTS" => self.s.lights.wing = !self.s.lights.wing,
            "PANEL_LIGHTS_TOGGLE" => self.s.lights.panel = !self.s.lights.panel,
            "TOGGLE_CABIN_LIGHTS" => self.s.lights.cabin = !self.s.lights.cabin,
            "ELEV_TRIM_UP" => self.s.controls.elevator_trim_pct = (self.s.controls.elevator_trim_pct + 1.0).clamp(-100.0, 100.0),
            "ELEV_TRIM_DN" => self.s.controls.elevator_trim_pct = (self.s.controls.elevator_trim_pct - 1.0).clamp(-100.0, 100.0),
            "ELEVATOR_TRIM_SET" => self.s.controls.elevator_trim_pct = round(v as f64 / 16383.0 * 100.0, 1),
            "THROTTLE_SET" | "THROTTLE1_SET" | "THROTTLE2_SET" | "THROTTLE3_SET" | "THROTTLE4_SET" => {
                self.s.controls.throttle_pct = pct(v);
                self.manual_throttle = true;
            }
            "THROTTLE_CUT" => {
                self.s.controls.throttle_pct = 0.0;
                self.manual_throttle = true;
            }
            "THROTTLE_FULL" => {
                self.s.controls.throttle_pct = 100.0;
                self.manual_throttle = true;
            }
            "AUTO_THROTTLE_TO_GA" => {
                self.s.controls.throttle_pct = 100.0;
                self.manual_throttle = false;
            }
            "MIXTURE_SET" => self.s.controls.mixture_pct = pct(v),
            "PROPELLER_SET" => self.s.controls.prop_pct = pct(v),
            "CAMERA_STATE" => self.s.controls.camera_state = v as i32,
            "CAMERA_PITCH_SET" => self.s.controls.camera_pitch_deg = v as f64,
            "CAMERA_YAW_SET" => self.s.controls.camera_yaw_deg = v as f64,
            other => return CommandResult::fail(codes::NOT_ALLOWED, format!("Symulacja nie obsługuje {other}")),
        }
        CommandResult::success()
    }

    /// Ustaw samolot na prostej do EPGD RWY 29 (ścieżka 3°, podwozie i klapy wypuszczone).
    pub fn position_on_final(&mut self, dist_nm: f64) {
        self.reset();
        self.phase = Phase::Approach;
        let back = norm(ARRIVAL.3 + 180.0).to_radians();
        let s = &mut self.s;
        s.position.lat = ARRIVAL.0 + dist_nm * back.cos() / NM_PER_DEG_LAT;
        s.position.lon = ARRIVAL.1 + dist_nm * back.sin() / (NM_PER_DEG_LAT * s.position.lat.to_radians().cos());
        s.position.alt_msl_ft = ARRIVAL.2 + 50.0 + dist_nm * 318.0;
        s.position.alt_agl_ft = s.position.alt_msl_ft - ARRIVAL.2;
        s.position.alt_ind_ft = s.position.alt_msl_ft;
        s.speed = SpeedData { ias_kt: 145.0, tas_kt: 147.0, gs_kt: 140.0, mach: 0.0, vs_fpm: -750.0 };
        s.attitude = AttitudeData { hdg_true_deg: ARRIVAL.3, hdg_mag_deg: norm(ARRIVAL.3 - MAG_VAR), track_true_deg: ARRIVAL.3, pitch_deg: 2.5, bank_deg: 0.0 };
        s.autopilot.master = true;
        s.autopilot.athr = true;
        s.autopilot.alt_ft = 3000.0;
        s.aircraft.on_ground = false;
        s.lights.landing = true;
        s.radios.nav1 = RadioPair { active_mhz: 109.300, standby_mhz: 110.900 };
        self.set_gear(true);
        self.set_flaps(4);
        self.update_engines(55.0, 0.0);
    }

    fn latch_touchdown(&mut self, utc_now_ms: i64) {
        self.touchdown_seq += 1;
        let s = &mut self.s;
        s.touchdown = Some(TouchdownData {
            seq: self.touchdown_seq,
            t: utc_now_ms,
            vs_fpm: -180.0,
            g_force: 1.2,
            pitch_deg: 3.5,
            bank_deg: -0.4,
            heading_true_deg: round(s.attitude.hdg_true_deg, 1),
            lat: round(s.position.lat, 6),
            lon: round(s.position.lon, 6),
            ias_kt: round(s.speed.ias_kt, 1),
            gs_kt: round(s.speed.gs_kt, 1),
        });
    }

    fn mach_to_ias(&self, mach: f64) -> f64 {
        let tas = mach * 661.47 * ((self.s.environment.oat_c + 273.15) / 288.15).sqrt();
        tas / (1.0 + 0.02 * self.s.position.alt_msl_ft / 1000.0)
    }

    fn advance(&mut self, next: Phase) {
        self.phase = next;
        self.phase_time = 0.0;
    }

    fn heading_target(&self, route: f64) -> f64 {
        if self.s.autopilot.modes.hdg {
            norm(self.s.autopilot.hdg_deg + MAG_VAR)
        } else {
            route
        }
    }

    fn top_of_descent(&self, dist: f64) -> bool {
        let needed = (self.s.position.alt_msl_ft - ARRIVAL.2 - 3000.0) / 1000.0 * 3.2 + 12.0;
        dist <= needed
    }

    fn set_cruise(&mut self, ft: f64) {
        self.target_cruise_ft = ft.clamp(3000.0, 41000.0);
        self.s.autopilot.alt_ft = self.target_cruise_ft;
        if self.phase == Phase::Cruise && self.target_cruise_ft > self.s.position.alt_msl_ft + 100.0 {
            self.advance(Phase::Climb);
        }
    }

    fn set_speed(&mut self, kt: f64) {
        self.manual_speed = true;
        self.target_spd_kt = kt.clamp(120.0, 340.0);
        self.s.autopilot.spd_kt = self.target_spd_kt;
    }

    fn set_gear(&mut self, down: bool) {
        self.s.controls.gear_down = down;
        self.s.controls.gear_pct = if down { 100.0 } else { 0.0 };
    }

    fn set_flaps(&mut self, index: i32) {
        let c = &mut self.s.controls;
        c.flaps_index = index.clamp(0, c.flaps_positions - 1);
        c.flaps_pct = (100.0 * c.flaps_index as f64 / (c.flaps_positions - 1) as f64).round();
    }

    fn update_engines(&mut self, n1_target: f64, dt: f64) {
        let throttle = self.s.controls.throttle_pct;
        let mut flow = 0.0;
        for e in &mut self.s.engines {
            e.n1_pct = round(approach(e.n1_pct, n1_target, 6.0 * dt + if dt == 0.0 { 100.0 } else { 0.0 }), 1);
            e.n2_pct = round(58.0 + e.n1_pct * 0.42, 1);
            e.egt_c = (380.0 + e.n1_pct * 5.2).round();
            e.fuel_flow_kg_h = (200.0 + (e.n1_pct - 20.0).max(0.0) * 15.0).round();
            e.rpm = 0.0;
            e.throttle_pct = throttle;
            flow += e.fuel_flow_kg_h;
        }
        let burn = flow * dt / 3600.0;
        let t = &mut self.s.fuel.tanks;
        if t.center_kg > burn {
            t.center_kg -= burn;
        } else {
            t.left_kg = (t.left_kg - burn / 2.0).max(0.0);
            t.right_kg = (t.right_kg - burn / 2.0).max(0.0);
        }
        self.s.fuel.total_kg = round(t.left_kg + t.center_kg + t.right_kg, 1);
    }
}

fn swap(p: &mut RadioPair) {
    std::mem::swap(&mut p.active_mhz, &mut p.standby_mhz);
}

pub fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::Parked => tr("postój przed startem"),
        Phase::TakeoffRoll => tr("rozbieg"),
        Phase::Climb => tr("wznoszenie"),
        Phase::Cruise => tr("przelot"),
        Phase::Descent => tr("zniżanie"),
        Phase::Approach => tr("podejście"),
        Phase::Rollout => tr("dobieg"),
        Phase::Stopped => tr("na miejscu"),
    }
}

/// Tryb symulacji jako źródło danych: krok co 50 ms w osobnym wątku.
pub struct SimulationSource {
    sim: Arc<Mutex<FlightSimulation>>,
    axes: Arc<AxisController>,
    nav: Arc<dyn NavDataProvider>,
    running: Arc<AtomicBool>,
    time_scale: f64,
}

impl SimulationSource {
    pub fn new(axes: Arc<AxisController>) -> Self {
        Self::with(axes, 1.0, None)
    }

    pub fn with(axes: Arc<AxisController>, time_scale: f64, start_on_final_nm: Option<f64>) -> Self {
        let mut sim = FlightSimulation::new();
        if let Some(nm) = start_on_final_nm {
            sim.position_on_final(nm);
        }
        Self { sim: Arc::new(Mutex::new(sim)), axes, nav: Arc::new(SimulationNavData), running: Arc::new(AtomicBool::new(false)), time_scale }
    }
}

impl FlightSource for SimulationSource {
    fn kind(&self) -> &'static str {
        "simulation"
    }

    fn is_connected(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn status_text(&self) -> String {
        let s = self.sim.lock();
        trf!("Symulacja: {phase}, do EPGD {dist} NM", phase = phase_name(s.phase), dist = s.distance_to_arrival_nm().round())
    }

    fn snapshot(&self) -> Option<FlightState> {
        if !self.is_connected() {
            return None;
        }
        Some(self.sim.lock().state().clone())
    }

    fn execute(&self, cmd: &Validated) -> CommandResult {
        self.sim.lock().execute(cmd)
    }

    fn axes(&self) -> Arc<AxisController> {
        self.axes.clone()
    }

    fn navdata(&self) -> Option<Arc<dyn NavDataProvider>> {
        Some(self.nav.clone())
    }

    fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let (sim, axes, running, scale) = (self.sim.clone(), self.axes.clone(), self.running.clone(), self.time_scale);
        std::thread::Builder::new()
            .name("simulation".into())
            .spawn(move || {
                let mut last = Instant::now();
                while running.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(50));
                    let now = Instant::now();
                    let dt = (now - last).as_secs_f64().min(0.5) * scale;
                    last = now;
                    let t = tick_ms();
                    let out = axes.peek(t);
                    axes.tick(t);
                    let aux = axes.tick_aux(t);
                    let mut s = sim.lock();
                    if let Some(sb) = aux.and_then(|a| a.speedbrake) {
                        s.state_mut().controls.spoilers_pct = (sb * 100.0).round();
                    }
                    s.step(dt, out, unix_ms());
                }
            })
            .ok();
    }

    fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn sim_name(&self) -> String {
        tr("Symulacja").to_string()
    }
}

impl Drop for SimulationSource {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::validate;
    use serde_json::json;

    #[test]
    fn full_flight_lands_at_epgd() {
        let mut f = FlightSimulation::new();
        let mut t = 0i64;
        let mut landed = false;
        for _ in 0..(3 * 3600 * 2) {
            f.step(0.5, AxisOutput::NEUTRAL, t);
            t += 500;
            if f.state().touchdown.is_some() {
                landed = true;
                break;
            }
        }
        assert!(landed, "phase {:?}", f.phase);
        assert!(f.distance_to_arrival_nm() < 3.0);
        assert_eq!(f.state().touchdown.as_ref().unwrap().vs_fpm, -180.0);
    }

    #[test]
    fn commands_change_state() {
        let mut f = FlightSimulation::new();
        f.execute(&validate(Some("HEADING_BUG_SET"), Some(&json!(90))).unwrap());
        assert_eq!(f.state().autopilot.hdg_deg, 90.0);
        f.execute(&validate(Some("XPNDR_SET"), Some(&json!(7700))).unwrap());
        assert_eq!(f.state().radios.transponder.code, "7700");
        f.execute(&validate(Some("COM_STBY_RADIO_SWAP"), None).unwrap());
        assert_eq!(f.state().radios.com1.active_mhz, 118.3);
        f.execute(&validate(Some("FLAPS_DOWN"), None).unwrap());
        assert_eq!(f.state().controls.flaps_index, 4);
        assert_eq!(f.state().controls.flaps_pct, 100.0);
    }

    #[test]
    fn short_final_touchdown() {
        let mut f = FlightSimulation::new();
        f.position_on_final(2.0);
        let mut t = 0;
        while f.state().touchdown.is_none() && t < 600_000 {
            f.step(0.25, AxisOutput::NEUTRAL, t);
            t += 250;
        }
        assert!(f.state().touchdown.is_some());
        assert!(f.state().aircraft.on_ground);
    }
}
