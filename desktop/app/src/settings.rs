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
    /// Panel boczny ze stanem lotu: widoczny i jego szerokość w pikselach logicznych.
    pub sidebar: bool,
    pub sidebar_w: i32,
    /// Wersja układu okna; przy przejściu na układ 3.1 (AMOLED, karty w pasku tytułu) motyw
    /// przestawia się raz na AMOLED. Brak pola w starszym pliku = 0.
    #[serde(default)]
    pub ui_version: i32,
    /// Karta lądowania na ekranie symulatora.
    pub landing_card: LandingCardCfg,
    /// Pasek do transmisji (stream) na wybranym ekranie.
    pub stream_bar: StreamBarCfg,
    /// Okno nakładki zawsze na wierzchu (PFD, ND, odczyt lotu albo mapa).
    pub overlay: OverlayCfg,
    /// Nazwa użytkownika albo numer (Pilot ID) w SimBrief.
    pub simbrief_user: String,
    /// Zrzuty ekranu: ekran do zrzutu (0 – główny), klawisz (0 brak, 1–4 F9–F12, 5 Ctrl+Shift+S, 6 Ctrl+F12).
    pub shot_monitor: i32,
    pub shot_key: i32,
    /// Warstwy mapy: plan z SimBrief i miejsca zrzutów ekranu.
    pub nav_plan: bool,
    pub nav_shots: bool,
    /// Symulator: 0 automatycznie (MSFS albo X-Plane), 1 tylko MSFS, 2 tylko X-Plane.
    pub sim_kind: i32,
    /// Adres komputera z X-Plane (pusty – ten komputer).
    pub xplane_host: String,
    /// Strony dla OBS (pasek, karta lądowania) także dla innych komputerów w sieci lokalnej.
    pub obs_lan: bool,
    /// Aktualizacje: sprawdzanie co kilka godzin i wersja, o której użytkownik nie chce przypominać.
    pub update_auto: bool,
    pub update_skip: String,
}

/// Położenie okna na ekranie: 0 środek, 1 u góry na środku, 2 u dołu na środku, 3 lewy górny róg,
/// 4 prawy górny, 5 lewy dolny, 6 prawy dolny.
pub const POSITIONS: i32 = 7;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LandingCardCfg {
    pub enabled: bool,
    pub monitor: i32,
    pub position: i32,
    /// 0 mała, 1 średnia, 2 duża, 3 bardzo duża.
    pub size: i32,
    /// 0 pełna karta, 1 kompaktowa, 2 pasek.
    pub layout: i32,
    /// Czas na ekranie w sekundach; 0 – do kliknięcia.
    pub seconds: i32,
    /// Krycie tła w procentach (40–100).
    pub opacity: i32,
    /// Odstęp od krawędzi ekranu (px).
    pub margin: i32,
    /// Kolor: 0 według oceny, 1 kolor akcentu, 2 biel.
    pub tone: i32,
    pub show_score: bool,
    pub show_vs: bool,
    pub show_g: bool,
    pub show_speed: bool,
    pub show_runway: bool,
    pub show_offsets: bool,
    pub show_bounces: bool,
    pub show_wind: bool,
    pub show_notes: bool,
    pub show_airport: bool,
    pub show_aircraft: bool,
    pub show_scale: bool,
    pub click_through: bool,
}

impl Default for LandingCardCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            monitor: 0,
            position: 0,
            size: 1,
            layout: 0,
            seconds: 15,
            opacity: 92,
            margin: 40,
            tone: 0,
            show_score: true,
            show_vs: true,
            show_g: true,
            show_speed: true,
            show_runway: true,
            show_offsets: true,
            show_bounces: true,
            show_wind: true,
            show_notes: true,
            show_airport: true,
            show_aircraft: true,
            show_scale: true,
            click_through: false,
        }
    }
}

/// Pola paska do transmisji (kolejność = kolejność na pasku).
pub const STREAM_FIELDS: [&str; 22] = [
    "route", "aircraft", "phase", "alt", "ias", "gs", "vs", "hdg", "wind", "oat", "qnh", "fuel", "next", "dest", "ap", "com1", "xpdr", "nearest", "utc", "flight", "landing", "title",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StreamBarCfg {
    pub enabled: bool,
    pub monitor: i32,
    /// 0 u góry, 1 u dołu.
    pub edge: i32,
    /// Szerokość w procentach szerokości ekranu (30–100); węższy pasek jest wyśrodkowany albo przy krawędzi.
    pub width: i32,
    /// 0 do lewej, 1 na środku, 2 do prawej.
    pub align: i32,
    /// 0 mały, 1 średni, 2 duży.
    pub size: i32,
    pub opacity: i32,
    /// 0 czarny, 1 grafitowy, 2 w kolorze akcentu.
    pub style: i32,
    pub show_labels: bool,
    pub show_logo: bool,
    pub accent_line: bool,
    pub click_through: bool,
    /// Własny tekst (np. nazwa kanału) – pole „title”.
    pub title: String,
    pub fields: Vec<String>,
}

impl Default for StreamBarCfg {
    fn default() -> Self {
        Self {
            enabled: false,
            monitor: 0,
            edge: 1,
            width: 100,
            align: 1,
            size: 1,
            opacity: 88,
            style: 0,
            show_labels: true,
            show_logo: true,
            accent_line: true,
            click_through: true,
            title: String::new(),
            fields: ["route", "aircraft", "phase", "alt", "gs", "vs", "hdg", "wind", "dest", "landing"].iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OverlayCfg {
    pub enabled: bool,
    pub monitor: i32,
    pub position: i32,
    /// 0 PFD, 1 ND, 2 PFD i ND, 3 odczyt lotu, 4 mapa.
    pub content: i32,
    /// 0 mała, 1 średnia, 2 duża, 3 bardzo duża.
    pub size: i32,
    pub opacity: i32,
    pub margin: i32,
    pub show_title: bool,
    pub click_through: bool,
}

impl Default for OverlayCfg {
    fn default() -> Self {
        Self { enabled: false, monitor: 0, position: 6, content: 0, size: 1, opacity: 100, margin: 24, show_title: true, click_through: false }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: 0,
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
            sidebar: true,
            sidebar_w: 276,
            ui_version: UI_VERSION,
            landing_card: LandingCardCfg::default(),
            stream_bar: StreamBarCfg::default(),
            overlay: OverlayCfg::default(),
            simbrief_user: String::new(),
            shot_monitor: 0,
            shot_key: 4,
            nav_plan: true,
            nav_shots: true,
            sim_kind: 0,
            xplane_host: String::new(),
            obs_lan: false,
            update_auto: true,
            update_skip: String::new(),
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

pub const UI_VERSION: i32 = 2;

pub const ACCENTS: [u32; 8] = [0x5b8cff, 0x3ccf7a, 0xffb547, 0xff5a5f, 0xd55cff, 0x2ec4d6, 0xf472b6, 0xa3a3a3];
pub const INTERVALS_MS: [i64; 5] = [250, 500, 1000, 2000, 5000];

fn path() -> std::path::PathBuf {
    opentakeoff_core::paths::config().join("settings.json")
}

impl Settings {
    pub fn load() -> Settings {
        let mut s: Settings = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if s.ui_version < UI_VERSION {
            // Nowy układ okna wygląda najlepiej w czerni AMOLED
            s.theme = 0;
            s.sidebar = true;
            s.ui_version = UI_VERSION;
            s.save();
        }
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
