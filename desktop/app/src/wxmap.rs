//! Pogoda na karcie Mapa: radar opadów z RainViewer (bez klucza, kafelki do poziomu 7) i METAR-y stacji
//! z widocznego obszaru z NOAA. Pobieranie w tle; po nadejściu danych mapa rysuje się od nowa
//! (licznik [`crate::map::TILES_SERIAL`]).

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use opentakeoff_core::aero::weather::{self, Metar};
use parking_lot::Mutex;

const RADAR_TTL: Duration = Duration::from_secs(300);
const METAR_TTL: Duration = Duration::from_secs(600);
/// Największy obszar (w stopniach szerokości), dla którego pobieramy METAR-y – dalej trzeba przybliżyć.
const METAR_MAX_SPAN: f64 = 24.0;

struct RadarState {
    fetched: Option<Instant>,
    busy: bool,
    time: i64,
    error: bool,
}

static RADAR: Mutex<RadarState> = Mutex::new(RadarState { fetched: None, busy: false, time: 0, error: false });

struct MetarState {
    /// Obszar pobranych danych (lat0, lon0, lat1, lon1).
    area: Option<(f64, f64, f64, f64)>,
    at: Option<Instant>,
    busy: bool,
    list: Option<Arc<Vec<Metar>>>,
    error: Option<String>,
    too_wide: bool,
}

static METARS: Mutex<MetarState> = Mutex::new(MetarState { area: None, at: None, busy: false, list: None, error: None, too_wide: false });

fn bump() {
    crate::map::TILES_SERIAL.fetch_add(1, Ordering::SeqCst);
}

/// Pilnuje świeżości klatki radaru (najnowsza z przeszłych, co 5 minut).
pub fn ensure_radar() {
    {
        let mut r = RADAR.lock();
        if r.busy || r.fetched.is_some_and(|t| t.elapsed() < RADAR_TTL) {
            return;
        }
        r.busy = true;
    }
    std::thread::spawn(|| {
        let res = (|| -> Result<(String, String, i64), String> {
            let body = ureq::get("https://api.rainviewer.com/public/weather-maps.json")
                .set("User-Agent", "OpenTakeoff-Windows")
                .timeout(Duration::from_secs(15))
                .call()
                .map_err(|e| e.to_string())?
                .into_string()
                .map_err(|e| e.to_string())?;
            let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
            let host = v["host"].as_str().ok_or("host")?.to_string();
            let last = v["radar"]["past"].as_array().and_then(|a| a.last()).ok_or("past")?;
            Ok((host, last["path"].as_str().ok_or("path")?.to_string(), last["time"].as_i64().unwrap_or(0)))
        })();
        let mut r = RADAR.lock();
        r.busy = false;
        r.fetched = Some(Instant::now());
        match res {
            Ok((host, path, time)) => {
                r.time = time;
                r.error = false;
                drop(r);
                crate::map::set_radar_frame(&host, &path);
            }
            Err(e) => {
                // ponowna próba po minucie zamiast po pięciu
                r.fetched = Some(Instant::now() - RADAR_TTL + Duration::from_secs(60));
                r.error = true;
                drop(r);
                opentakeoff_core::log::warn(&format!("radar: {e}"));
            }
        }
        bump();
    });
}

/// Czas klatki radaru (Unix, s) albo None, gdy jeszcze jej nie ma.
pub fn radar_time() -> Option<i64> {
    let r = RADAR.lock();
    (r.time > 0).then_some(r.time)
}

pub fn radar_error() -> bool {
    RADAR.lock().error
}

/// Pilnuje METAR-ów dla widocznego obszaru: pobiera, gdy obszar wyjdzie poza pobrany albo dane się zestarzeją.
pub fn ensure_metars(lat0: f64, lon0: f64, lat1: f64, lon1: f64) {
    let (la0, la1) = (lat0.min(lat1), lat0.max(lat1));
    let (lo0, lo1) = (lon0.min(lon1), lon0.max(lon1));
    let mut m = METARS.lock();
    m.too_wide = la1 - la0 > METAR_MAX_SPAN || lo1 - lo0 > METAR_MAX_SPAN * 1.6;
    if m.busy || m.too_wide {
        return;
    }
    let inside = m.area.is_some_and(|(a, b, c, d)| la0 >= a && lo0 >= b && la1 <= c && lo1 <= d);
    let fresh = m.at.is_some_and(|t| t.elapsed() < METAR_TTL);
    let failed_recently = m.error.is_some() && m.at.is_some_and(|t| t.elapsed() < Duration::from_secs(60));
    if (inside && fresh) || failed_recently {
        return;
    }
    // zapas wokół widoku, zaokrąglony do pół stopnia – mniej zapytań przy przesuwaniu
    let pad_la = ((la1 - la0) * 0.3).max(0.5);
    let pad_lo = ((lo1 - lo0) * 0.3).max(0.5);
    let snap = |v: f64, up: bool| if up { (v * 2.0).ceil() / 2.0 } else { (v * 2.0).floor() / 2.0 };
    let area = (snap((la0 - pad_la).max(-89.0), false), snap(lo0 - pad_lo, false), snap((la1 + pad_la).min(89.0), true), snap(lo1 + pad_lo, true));
    m.busy = true;
    drop(m);
    std::thread::spawn(move || {
        let res = weather::metars_in(area.0, area.1.max(-180.0), area.2, area.3.min(180.0));
        let mut m = METARS.lock();
        m.busy = false;
        m.at = Some(Instant::now());
        match res {
            Ok(list) => {
                m.area = Some(area);
                m.list = Some(Arc::new(list));
                m.error = None;
            }
            Err(e) => {
                opentakeoff_core::log::warn(&format!("metar (obszar): {e}"));
                m.error = Some(e);
            }
        }
        drop(m);
        bump();
    });
}

/// Pobrane METAR-y (mogą obejmować więcej niż widok).
pub fn metars() -> Arc<Vec<Metar>> {
    METARS.lock().list.clone().unwrap_or_default()
}

/// Stan warstwy METAR do podpisu: (liczba stacji, trwa pobieranie, błąd, za duży obszar).
pub fn metar_status() -> (usize, bool, bool, bool) {
    let m = METARS.lock();
    (m.list.as_ref().map_or(0, |l| l.len()), m.busy, m.error.is_some(), m.too_wide)
}

/// Kolor kategorii lotu (ARGB) – jak na mapach lotniczych: VFR zielony, MVFR niebieski, IFR czerwony, LIFR fiolet.
pub fn category_color(cat: &str) -> u32 {
    match cat {
        "VFR" => 0xFF3CCF7A,
        "MVFR" => 0xFF4C8DFF,
        "IFR" => 0xFFFF4D5A,
        "LIFR" => 0xFFD25CFF,
        _ => 0xFF9AA4B2,
    }
}
