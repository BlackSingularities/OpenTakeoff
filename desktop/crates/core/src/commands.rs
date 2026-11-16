//! Jedyne komendy, jakie telefon może wysłać do symulatora. Wszystko spoza listy jest odrzucane.

use serde_json::Value;

use crate::proto::codes;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Zdarzenie SimConnect (SimConnect_TransmitClientEvent).
    Event,
    /// Zapis TRANSPONDER STATE (XPNDR_MODE).
    TransponderState,
    /// Zapis CAMERA STATE (kabina / widok zewnętrzny / dron).
    CameraState,
    /// Rozglądanie się: kąty w stopniach od początku przytrzymania.
    CameraPitch,
    CameraYaw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Autopilot,
    Radio,
    Transponder,
    Gear,
    Flaps,
    Spoilers,
    Brakes,
    Lights,
    Trim,
    Engines,
    View,
}

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub name: &'static str,
    pub group: Group,
    pub target: Target,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub squawk: bool,
}

impl Spec {
    pub fn needs_value(&self) -> bool {
        self.min.is_some()
    }
}

/// Komenda po walidacji: nazwa z listy, wartość i gotowy DWORD dla SimConnect.
#[derive(Clone, Copy, Debug)]
pub struct Validated {
    pub spec: Spec,
    pub value: i64,
    pub data: u32,
}

pub const COM_MIN_HZ: i64 = 118_000_000;
pub const COM_MAX_HZ: i64 = 136_990_000;
pub const NAV_MIN_HZ: i64 = 108_000_000;
pub const NAV_MAX_HZ: i64 = 117_950_000;

const fn s(name: &'static str, group: Group) -> Spec {
    Spec { name, group, target: Target::Event, min: None, max: None, squawk: false }
}
const fn v(name: &'static str, group: Group, min: i64, max: i64) -> Spec {
    Spec { name, group, target: Target::Event, min: Some(min), max: Some(max), squawk: false }
}
const fn t(name: &'static str, group: Group, target: Target, min: i64, max: i64) -> Spec {
    Spec { name, group, target, min: Some(min), max: Some(max), squawk: false }
}

use Group::*;

pub static ALL: &[Spec] = &[
    // Autopilot
    s("AP_MASTER", Autopilot),
    s("AUTOPILOT_ON", Autopilot),
    s("AUTOPILOT_OFF", Autopilot),
    s("TOGGLE_FLIGHT_DIRECTOR", Autopilot),
    s("AUTO_THROTTLE_ARM", Autopilot),
    s("YAW_DAMPER_TOGGLE", Autopilot),
    s("AP_HDG_HOLD", Autopilot),
    s("AP_NAV1_HOLD", Autopilot),
    s("AP_APR_HOLD", Autopilot),
    s("AP_LOC_HOLD", Autopilot),
    s("AP_BC_HOLD", Autopilot),
    s("AP_ALT_HOLD", Autopilot),
    s("AP_VS_HOLD", Autopilot),
    s("FLIGHT_LEVEL_CHANGE", Autopilot),
    s("AP_PANEL_SPEED_HOLD", Autopilot),
    v("HEADING_BUG_SET", Autopilot, 0, 359),
    s("HEADING_BUG_INC", Autopilot),
    s("HEADING_BUG_DEC", Autopilot),
    v("AP_ALT_VAR_SET_ENGLISH", Autopilot, 0, 50_000),
    s("AP_ALT_VAR_INC", Autopilot),
    s("AP_ALT_VAR_DEC", Autopilot),
    v("AP_VS_VAR_SET_ENGLISH", Autopilot, -8_000, 8_000),
    s("AP_VS_VAR_INC", Autopilot),
    s("AP_VS_VAR_DEC", Autopilot),
    v("AP_SPD_VAR_SET", Autopilot, 0, 500),
    s("AP_SPD_VAR_INC", Autopilot),
    s("AP_SPD_VAR_DEC", Autopilot),
    s("AP_AIRSPEED_HOLD", Autopilot),
    s("AP_N1_HOLD", Autopilot),
    s("AP_PANEL_MACH_HOLD", Autopilot),
    v("AP_MACH_VAR_SET", Autopilot, 10, 99),
    s("AP_BANK_HOLD", Autopilot),
    s("AP_WING_LEVELER", Autopilot),
    // Radia
    v("COM_RADIO_SET_HZ", Radio, COM_MIN_HZ, COM_MAX_HZ),
    v("COM_STBY_RADIO_SET_HZ", Radio, COM_MIN_HZ, COM_MAX_HZ),
    v("COM2_RADIO_SET_HZ", Radio, COM_MIN_HZ, COM_MAX_HZ),
    v("COM2_STBY_RADIO_SET_HZ", Radio, COM_MIN_HZ, COM_MAX_HZ),
    v("NAV1_RADIO_SET_HZ", Radio, NAV_MIN_HZ, NAV_MAX_HZ),
    v("NAV1_STBY_SET_HZ", Radio, NAV_MIN_HZ, NAV_MAX_HZ),
    v("NAV2_RADIO_SET_HZ", Radio, NAV_MIN_HZ, NAV_MAX_HZ),
    v("NAV2_STBY_SET_HZ", Radio, NAV_MIN_HZ, NAV_MAX_HZ),
    s("COM_STBY_RADIO_SWAP", Radio),
    s("COM2_RADIO_SWAP", Radio),
    s("NAV1_RADIO_SWAP", Radio),
    s("NAV2_RADIO_SWAP", Radio),
    // Transponder
    Spec { name: "XPNDR_SET", group: Transponder, target: Target::Event, min: Some(0), max: Some(7777), squawk: true },
    s("XPNDR_IDENT_ON", Transponder),
    t("XPNDR_MODE", Transponder, Target::TransponderState, 0, 5),
    // Podwozie, klapy, spoilery, hamulec
    s("GEAR_UP", Gear),
    s("GEAR_DOWN", Gear),
    s("GEAR_TOGGLE", Gear),
    s("FLAPS_UP", Flaps),
    s("FLAPS_DOWN", Flaps),
    s("FLAPS_INCR", Flaps),
    s("FLAPS_DECR", Flaps),
    v("FLAPS_SET", Flaps, 0, 16383),
    s("SPOILERS_ON", Spoilers),
    s("SPOILERS_OFF", Spoilers),
    s("SPOILERS_TOGGLE", Spoilers),
    s("SPOILERS_ARM_TOGGLE", Spoilers),
    v("SPOILERS_SET", Spoilers, 0, 16383),
    s("PARKING_BRAKES", Brakes),
    v("PARKING_BRAKE_SET", Brakes, 0, 1),
    // Światła
    s("TOGGLE_BEACON_LIGHTS", Lights),
    s("STROBES_TOGGLE", Lights),
    s("TOGGLE_NAV_LIGHTS", Lights),
    s("LANDING_LIGHTS_TOGGLE", Lights),
    s("TOGGLE_TAXI_LIGHTS", Lights),
    s("TOGGLE_LOGO_LIGHTS", Lights),
    s("TOGGLE_WING_LIGHTS", Lights),
    s("PANEL_LIGHTS_TOGGLE", Lights),
    s("TOGGLE_CABIN_LIGHTS", Lights),
    // Trymer
    s("ELEV_TRIM_UP", Trim),
    s("ELEV_TRIM_DN", Trim),
    v("ELEVATOR_TRIM_SET", Trim, -16383, 16383),
    s("AILERON_TRIM_LEFT", Trim),
    s("AILERON_TRIM_RIGHT", Trim),
    s("RUDDER_TRIM_LEFT", Trim),
    s("RUDDER_TRIM_RIGHT", Trim),
    // Silniki
    v("THROTTLE_SET", Engines, 0, 16383),
    v("THROTTLE1_SET", Engines, 0, 16383),
    v("THROTTLE2_SET", Engines, 0, 16383),
    v("THROTTLE3_SET", Engines, 0, 16383),
    v("THROTTLE4_SET", Engines, 0, 16383),
    s("THROTTLE_CUT", Engines),
    s("THROTTLE_FULL", Engines),
    s("THROTTLE_REVERSE_THRUST_TOGGLE", Engines),
    s("AUTO_THROTTLE_TO_GA", Engines),
    v("MIXTURE_SET", Engines, 0, 16383),
    v("PROPELLER_SET", Engines, 0, 16383),
    // Kamera
    t("CAMERA_STATE", View, Target::CameraState, 2, 4),
    t("CAMERA_PITCH_SET", View, Target::CameraPitch, -90, 90),
    t("CAMERA_YAW_SET", View, Target::CameraYaw, -180, 180),
];

/// Zdarzenia osi – nie są komendami `command`, przychodzą przez wiadomość `axis`.
pub static AXIS_EVENTS: &[&str] = &["AXIS_AILERONS_SET", "AXIS_ELEVATOR_SET", "AXIS_RUDDER_SET", "AXIS_SPOILER_SET", "AXIS_LEFT_BRAKE_SET", "AXIS_RIGHT_BRAKE_SET"];

pub fn find(name: &str) -> Option<&'static Spec> {
    ALL.iter().find(|s| s.name == name)
}

/// Err((kod, opis)).
pub fn validate(name: Option<&str>, value: Option<&Value>) -> Result<Validated, (&'static str, String)> {
    let Some(spec) = name.and_then(find) else {
        return Err((codes::NOT_ALLOWED, format!("Komenda poza listą dozwolonych: {}", name.unwrap_or(""))));
    };
    if !spec.needs_value() {
        return Ok(Validated { spec: *spec, value: 0, data: 0 });
    }
    let (min, max) = (spec.min.unwrap(), spec.max.unwrap());
    let d = match value.and_then(|v| v.as_f64()) {
        Some(d) if d.is_finite() && d == d.floor() => d,
        _ => return Err((codes::INVALID_VALUE, format!("{} wymaga wartości całkowitej {min}…{max}", spec.name))),
    };
    if d < min as f64 || d > max as f64 {
        return Err((codes::INVALID_VALUE, format!("{}: wartość {d} poza zakresem {min}…{max}", spec.name)));
    }
    let n = d as i64;
    let data = if spec.squawk {
        encode_squawk(n).ok_or((codes::INVALID_VALUE, format!("{}: kod transpondera musi mieć cyfry 0–7", spec.name)))?
    } else {
        n as i32 as u32
    };
    Ok(Validated { spec: *spec, value: n, data })
}

/// 7000 → 0x7000 (BCD16, jak oczekuje XPNDR_SET).
pub fn encode_squawk(code: i64) -> Option<u32> {
    if !(0..=7777).contains(&code) {
        return None;
    }
    let mut bcd = 0u32;
    for c in format!("{code:04}").chars() {
        let d = c.to_digit(10)?;
        if d > 7 {
            return None;
        }
        bcd = (bcd << 4) | d;
    }
    Some(bcd)
}

/// 0x7000 → "7000".
pub fn decode_squawk(bcd: u32) -> String {
    format!("{}{}{}{}", (bcd >> 12) & 0xF, (bcd >> 8) & 0xF, (bcd >> 4) & 0xF, bcd & 0xF)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn allowlist() {
        assert!(validate(Some("AP_MASTER"), None).is_ok());
        assert_eq!(validate(Some("EXPLODE"), None).unwrap_err().0, codes::NOT_ALLOWED);
        assert_eq!(validate(None, None).unwrap_err().0, codes::NOT_ALLOWED);
        assert_eq!(validate(Some("HEADING_BUG_SET"), Some(&json!(360))).unwrap_err().0, codes::INVALID_VALUE);
        assert_eq!(validate(Some("HEADING_BUG_SET"), Some(&json!(1.5))).unwrap_err().0, codes::INVALID_VALUE);
        assert_eq!(validate(Some("HEADING_BUG_SET"), Some(&json!(270.0))).unwrap().value, 270);
        assert_eq!(validate(Some("AP_VS_VAR_SET_ENGLISH"), Some(&json!(-1500))).unwrap().data, (-1500i32) as u32);
        assert_eq!(validate(Some("XPNDR_SET"), Some(&json!(7000))).unwrap().data, 0x7000);
        assert_eq!(validate(Some("XPNDR_SET"), Some(&json!(7800))).unwrap_err().0, codes::INVALID_VALUE);
        assert_eq!(validate(Some("COM_RADIO_SET_HZ"), Some(&json!(121_500_000))).unwrap().data, 121_500_000);
        assert!(validate(Some("COM_RADIO_SET_HZ"), Some(&json!("121.5"))).is_err());
    }

    #[test]
    fn squawk() {
        assert_eq!(encode_squawk(1200), Some(0x1200));
        assert_eq!(encode_squawk(8), None);
        assert_eq!(decode_squawk(0x7700), "7700");
        assert_eq!(decode_squawk(0x0042), "0042");
    }

    #[test]
    fn names_unique() {
        for (i, a) in ALL.iter().enumerate() {
            assert!(ALL[i + 1..].iter().all(|b| b.name != a.name), "{}", a.name);
        }
    }
}
