//! Ustawienia programu (settings.json w folderze danych). Brakujące pola dostają wartości domyślne,
//! więc nowsze wersje czytają starsze pliki.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 0 AMOLED, 1 grafitowy, 2 jasny.
    pub theme: i32,
    pub accent: usize,
    pub scale: f32,
    pub corners: i32,
    /// Pusty – język Windows.
    pub language: String,
    pub simulation: bool,
    pub look_sensitivity: i32,
    pub record_flights: bool,
    /// 0: 250 ms, 1: 500 ms, 2: 1 s, 3: 2 s, 4: 5 s.
    pub record_interval: i32,
    pub auto_sync: bool,
    pub autostart: bool,
    pub start_minimized: bool,
    pub close_to_tray: bool,
    /// 0 ft, 1 m.
    pub units_alt: i32,
    /// 0 kt, 1 km/h.
    pub units_speed: i32,
    /// 0 zgodny z motywem, 1 ciemny, 2 jasny, 3 OpenStreetMap.
    pub map_style: i32,
    pub watchlist: Vec<String>,
    pub last_airport: String,
    pub window: Option<(i32, i32, u32, u32, bool)>,
    pub tab: i32,
    pub migrated_link: bool,
    /// Styl przyrządów: pusty – rozpoznany z samolotu, inaczej klucz stylu (A320, B737…).
    pub instr_style: String,
    /// Układ karty Przyrządy: 0 kokpit, 1 zegary, 2 wszystko.
    pub instr_layout: i32,
    /// Zasięg ND (indeks w ND_RANGES) i tryb ROSE.
    pub nd_range: i32,
    pub nd_rose: bool,
    pub nd_layers: u32,
    /// Warstwy mapy nawigacyjnej (maska bitowa NAV_*).
    pub nav_layers: u32,
    pub nav_3d: bool,
    pub nav_cam: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: 1,
            accent: 0,
            scale: 1.0,
            corners: 1,
            language: String::new(),
            simulation: false,
            look_sensitivity: 100,
            record_flights: true,
            record_interval: 2,
            auto_sync: true,
            autostart: false,
            start_minimized: false,
            close_to_tray: true,
            units_alt: 0,
            units_speed: 0,
            map_style: 0,
            watchlist: vec!["EPWA".into(), "EPGD".into(), "EPKK".into()],
            last_airport: String::new(),
            window: None,
            tab: 0,
            migrated_link: false,
            instr_style: String::new(),
            instr_layout: 0,
            nd_range: 3,
            nd_rose: false,
            nd_layers: 0b111,
            nav_layers: NAV_DEFAULT,
            nav_3d: false,
            nav_cam: 0,
        }
    }
}

pub const ND_RANGES: [f32; 8] = [2.5, 5.0, 10.0, 20.0, 40.0, 80.0, 160.0, 320.0];

pub const NAV_APT: u32 = 1;
pub const NAV_VOR: u32 = 2;
pub const NAV_NDB: u32 = 4;
pub const NAV_VOR_ROSE: u32 = 8;
pub const NAV_ROSE: u32 = 16;
pub const NAV_RINGS: u32 = 32;
pub const NAV_VECTOR: u32 = 64;
pub const NAV_TRACK: u32 = 128;
pub const NAV_ROUTE: u32 = 256;
pub const NAV_CURTAIN: u32 = 512;
pub const NAV_RWY: u32 = 1024;
pub const NAV_DEFAULT: u32 = NAV_APT | NAV_VOR | NAV_NDB | NAV_VOR_ROSE | NAV_ROSE | NAV_VECTOR | NAV_TRACK | NAV_ROUTE | NAV_CURTAIN | NAV_RWY;

pub const ACCENTS: [u32; 8] = [0x5b8cff, 0x3ccf7a, 0xffb547, 0xff5a5f, 0xd55cff, 0x2ec4d6, 0xf472b6, 0xa3a3a3];
pub const INTERVALS_MS: [i64; 5] = [250, 500, 1000, 2000, 5000];

fn path() -> std::path::PathBuf {
    opentakeoff_core::paths::config().join("settings.json")
}

impl Settings {
    pub fn load() -> Settings {
        let mut s: Settings = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if !s.migrated_link {
            // Ustawienia dawnego OpenTakeoff Link (tryb symulacji, czułość rozglądania, język)
            if let Some(v) = std::fs::read_to_string(opentakeoff_core::paths::legacy_link().join("settings.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
                if let Some(b) = v.get("SimulationMode").and_then(|b| b.as_bool()) {
                    s.simulation = b;
                }
                if let Some(n) = v.get("LookSensitivity").and_then(|n| n.as_i64()) {
                    s.look_sensitivity = n.clamp(20, 400) as i32;
                }
            }
            s.migrated_link = true;
            s.save();
        }
        s
    }

    pub fn save(&self) {
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = opentakeoff_core::paths::write_atomic(&path(), t.as_bytes());
        }
    }

    pub fn interval_ms(&self) -> i64 {
        INTERVALS_MS[self.record_interval.clamp(0, 4) as usize]
    }
}
