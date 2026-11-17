//! Zrzuty ekranu symulatora z miejscem samolotu: skrót klawiszowy działa także, gdy aktywny jest
//! symulator; obraz (JPG) i miniatura trafiają do folderu programu, a do spisu – pozycja, wysokość,
//! kurs, prędkość, samolot i najbliższe lotnisko. Zrzuty widać w galerii i na mapie.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use opentakeoff_core::aero::airports;
use opentakeoff_core::lang::tr;
use opentakeoff_core::source::unix_ms;
use opentakeoff_core::{log, trf};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::app::{self, state, ui};
use crate::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Shot {
    pub id: String,
    pub file: String,
    pub thumb: String,
    pub time: i64,
    pub has_pos: bool,
    pub lat: f64,
    pub lon: f64,
    pub alt_ft: f64,
    pub agl_ft: f64,
    pub hdg: f64,
    pub gs_kt: f64,
    pub ias_kt: f64,
    pub aircraft: String,
    pub phase: String,
    pub nearest: String,
    /// Lot z dziennika nagrywany w chwili zrzutu (puste – starsze zrzuty; wtedy dopasowanie po czasie).
    pub flight: String,
}

thread_local! {
    static SHOTS: RefCell<Option<Vec<Shot>>> = const { RefCell::new(None) };
    static SELECTED: RefCell<String> = const { RefCell::new(String::new()) };
    static KEY_WAS_DOWN: Cell<bool> = const { Cell::new(false) };
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

pub fn dir() -> PathBuf {
    opentakeoff_core::paths::config().join("screenshots")
}

fn index_file() -> PathBuf {
    dir().join("screenshots.json")
}

fn with_list<R>(f: impl FnOnce(&mut Vec<Shot>) -> R) -> R {
    SHOTS.with(|s| {
        let mut s = s.borrow_mut();
        if s.is_none() {
            let list: Vec<Shot> = std::fs::read_to_string(index_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            *s = Some(list);
        }
        f(s.as_mut().unwrap())
    })
}

fn save_index() {
    let json = with_list(|l| serde_json::to_string_pretty(l).unwrap_or_default());
    let _ = std::fs::create_dir_all(dir());
    let _ = opentakeoff_core::paths::write_atomic(&index_file(), json.as_bytes());
}

/// Punkty na mapę: (szer., dł., podpis).
pub fn points() -> Vec<(f64, f64, String)> {
    with_list(|l| l.iter().filter(|s| s.has_pos).map(|s| (s.lat, s.lon, crate::fmt::time(s.time))).collect())
}

/// Zrzuty z danego lotu: zapisane z jego id albo zrobione w czasie jego trwania.
pub fn for_flight(f: &opentakeoff_core::logbook::FlightLog) -> Vec<Shot> {
    with_list(|l| {
        let mut v: Vec<Shot> = l.iter().filter(|s| s.flight == f.id || (s.flight.is_empty() && s.time >= f.start - 60_000 && s.time <= f.end + 60_000)).cloned().collect();
        v.sort_by_key(|s| s.time);
        v
    })
}

/// Lot z dziennika, do którego należy zrzut (id albo czas).
fn flight_of(s: &Shot) -> Option<std::sync::Arc<opentakeoff_core::logbook::FlightLog>> {
    let store = state().borrow().host.logbook.clone();
    if !s.flight.is_empty() {
        if let Some(f) = store.get(&s.flight) {
            return Some(f);
        }
    }
    store.flights().into_iter().find(|f| s.time >= f.start - 60_000 && s.time <= f.end + 60_000)
}

pub fn file_path(s: &Shot) -> PathBuf {
    dir().join(&s.file)
}

pub fn thumb_image(s: &Shot) -> slint::Image {
    slint::Image::load_from_path(&dir().join(&s.thumb)).unwrap_or_default()
}

/// Skrót (ustawienia): 1–4 F9–F12, 5 Ctrl+Shift+S, 6 Ctrl+F12.
fn hotkey_down(key: i32) -> bool {
    use crate::screen::key_down;
    const CTRL: u16 = 0x11;
    const SHIFT: u16 = 0x10;
    match key {
        1 => key_down(0x78),
        2 => key_down(0x79),
        3 => key_down(0x7A),
        4 => key_down(0x7B) && !key_down(CTRL),
        5 => key_down(CTRL) && key_down(SHIFT) && key_down(0x53),
        6 => key_down(CTRL) && key_down(0x7B),
        _ => false,
    }
}

/// Sprawdzane co 50 ms: wciśnięty skrót – zrzut.
pub fn poll_key() {
    let key = state().borrow().s.shot_key;
    if key == 0 {
        return;
    }
    let down = hotkey_down(key);
    let was = KEY_WAS_DOWN.with(|k| k.replace(down));
    if down && !was {
        take();
    }
}

/// Zrzut ekranu wybranego w ustawieniach z bieżącym miejscem samolotu.
pub fn take() {
    if BUSY.with(|b| b.get()) {
        return;
    }
    let Some(u) = ui() else { return };
    let mons = crate::screen::monitors(u.window());
    let (idx, snap, flight) = {
        let st = state();
        let b = st.borrow();
        (b.s.shot_monitor, b.host.snapshot(), b.host.recording_id())
    };
    let Some(m) = mons.get(idx.max(0) as usize).or(mons.first()).cloned() else {
        app::toast(tr("Nie znaleziono ekranu do zrzutu"));
        return;
    };
    let live = u.global::<App>().get_live();
    let now = unix_ms();
    let mut shot = Shot { id: now.to_string(), time: now, flight: flight.unwrap_or_default(), ..Default::default() };
    if let Some(s) = &snap {
        shot.has_pos = true;
        shot.lat = s.position.lat;
        shot.lon = s.position.lon;
        shot.alt_ft = s.position.alt_ind_ft;
        shot.agl_ft = s.position.alt_agl_ft;
        shot.hdg = s.attitude.hdg_mag_deg.rem_euclid(360.0);
        shot.gs_kt = s.speed.gs_kt;
        shot.ias_kt = s.speed.ias_kt;
        shot.aircraft = s.aircraft.title.clone();
        shot.phase = live.phase.to_string();
        shot.nearest = airports::db()
            .and_then(|d| d.within(s.position.lat, s.position.lon, 60.0, false).into_iter().next().map(|(dist, a)| format!("{} {} · {:.0} NM", a.ident, a.name, dist)))
            .unwrap_or_default();
    }
    BUSY.with(|b| b.set(true));
    std::thread::spawn(move || {
        let res = (|| -> Result<Shot, String> {
            let img = crate::screen::capture(&m).ok_or_else(|| tr("Nie udało się odczytać obrazu ekranu").to_string())?;
            let d = dir();
            std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
            let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
            let name = format!("OpenTakeoff_{stamp}.jpg");
            let thumb = format!("OpenTakeoff_{stamp}_t.jpg");
            let mut f = std::io::BufWriter::new(std::fs::File::create(d.join(&name)).map_err(|e| e.to_string())?);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, 92).encode_image(&img).map_err(|e| e.to_string())?;
            drop(f);
            let tw = 360u32;
            let th = (img.height() as u64 * tw as u64 / img.width().max(1) as u64) as u32;
            let small = image::imageops::resize(&img, tw, th.max(1), image::imageops::FilterType::Triangle);
            let mut f = std::io::BufWriter::new(std::fs::File::create(d.join(&thumb)).map_err(|e| e.to_string())?);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, 85).encode_image(&small).map_err(|e| e.to_string())?;
            drop(f);
            shot.file = name;
            shot.thumb = thumb;
            Ok(shot)
        })();
        app::later(move || {
            BUSY.with(|b| b.set(false));
            match res {
                Ok(s) => {
                    let id = s.id.clone();
                    let place = if s.nearest.is_empty() { String::new() } else { format!(" – {}", s.nearest) };
                    with_list(|l| l.insert(0, s));
                    save_index();
                    SELECTED.with(|x| *x.borrow_mut() = id);
                    refresh();
                    crate::navmap::render(true);
                    app::toast(trf!("Zapisano zrzut ekranu{place}"));
                }
                Err(e) => {
                    log::warn(&format!("Zrzut ekranu: {e}"));
                    app::toast(trf!("Zrzut ekranu nie powiódł się: {e}"));
                }
            }
        });
    });
}

fn view(s: &Shot, units: &crate::fmt::Units) -> ShotV {
    let d = dir();
    ShotV {
        id: ss(&s.id),
        thumb: slint::Image::load_from_path(&d.join(&s.thumb)).unwrap_or_default(),
        time: ss(crate::fmt::clock(s.time)),
        date: ss(crate::fmt::date(s.time)),
        place: ss(if s.nearest.is_empty() { tr("Poza zasięgiem lotnisk").to_string() } else { s.nearest.clone() }),
        alt: ss(if s.has_pos { format!("{} {}", units.alt(s.alt_ft), units.alt_unit()) } else { String::new() }),
        info: ss(if s.has_pos { format!("HDG {:03.0}° · GS {:.0} kt · IAS {:.0} kt · AGL {}", s.hdg, s.gs_kt, s.ias_kt, units.alt_full(s.agl_ft)) } else { tr("Bez danych lotu (symulator nie był połączony)").to_string() }),
        coords: ss(if s.has_pos { crate::fmt::latlon(s.lat, s.lon) } else { String::new() }),
        aircraft: ss(if s.phase.is_empty() { s.aircraft.clone() } else { format!("{} · {}", s.aircraft, s.phase) }),
        has_pos: s.has_pos,
        flight: ss(flight_of(s).map(|f| format!("{} → {} · {}", f.from.as_deref().unwrap_or("?"), f.to.as_deref().unwrap_or("?"), crate::fmt::date(f.start))).unwrap_or_default()),
    }
}

/// Galeria do interfejsu.
pub fn refresh() {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    let units = {
        let st = state();
        let b = st.borrow();
        crate::fmt::Units { meters: b.s.units_alt == 1, kmh: b.s.units_speed == 1 }
    };
    let list = with_list(|l| l.clone());
    let sel = SELECTED.with(|x| x.borrow().clone());
    app.set_shots(ModelRc::new(VecModel::from(list.iter().map(|s| view(s, &units)).collect::<Vec<_>>())));
    app.set_shots_dir(ss(dir().display().to_string()));
    match list.iter().find(|s| s.id == sel).or(list.first()) {
        Some(s) => {
            SELECTED.with(|x| *x.borrow_mut() = s.id.clone());
            app.set_shot(view(s, &units));
            app.set_shot_image(slint::Image::load_from_path(&dir().join(&s.file)).unwrap_or_default());
            app.set_has_shot(true);
        }
        None => {
            app.set_has_shot(false);
            app.set_shot(ShotV::default());
            app.set_shot_image(slint::Image::default());
        }
    }
}

fn selected() -> Option<Shot> {
    let sel = SELECTED.with(|x| x.borrow().clone());
    with_list(|l| l.iter().find(|s| s.id == sel).cloned())
}

/// Usuwa zrzuty (pliki i wpisy).
pub fn delete(ids: &[String]) {
    let d = dir();
    with_list(|l| {
        for s in l.iter().filter(|s| ids.contains(&s.id)) {
            let _ = std::fs::remove_file(d.join(&s.file));
            let _ = std::fs::remove_file(d.join(&s.thumb));
        }
        l.retain(|s| !ids.contains(&s.id));
    });
    save_index();
    refresh();
    crate::navmap::render(true);
}

pub fn selected_id() -> Option<String> {
    selected().map(|s| s.id)
}

pub fn cmd(id: &str, arg: &str) -> bool {
    match id {
        "shot-take" => take(),
        "shot-select" => {
            SELECTED.with(|x| *x.borrow_mut() = arg.to_string());
            refresh();
        }
        "shot-flight" => {
            if let Some(f) = selected().as_ref().and_then(flight_of) {
                if let Some(u) = ui() {
                    u.global::<App>().set_tab(2);
                }
                crate::app::refresh_logbook();
                crate::app::open_flight(&f.id);
            }
        }
        "shot-show" => {
            // Z dziennika: zrzut w galerii
            SELECTED.with(|x| *x.borrow_mut() = arg.to_string());
            if let Some(u) = ui() {
                u.global::<App>().set_tab(11);
            }
            refresh();
        }
        "shot-open" => {
            if let Some(s) = selected() {
                let _ = open::that(dir().join(&s.file));
            }
        }
        "shot-folder" => {
            let _ = std::fs::create_dir_all(dir());
            let _ = open::that(dir());
        }
        "shot-copy-pos" => {
            if let Some(s) = selected().filter(|s| s.has_pos) {
                let text = format!("{:.6}, {:.6}", s.lat, s.lon);
                if let Ok(mut c) = arboard::Clipboard::new() {
                    let _ = c.set_text(text.clone());
                }
                app::toast(trf!("Skopiowano {text}"));
            }
        }
        "shot-map" => {
            if let Some(s) = selected().filter(|s| s.has_pos) {
                {
                    let st = state();
                    let mut b = st.borrow_mut();
                    b.s.nav_shots = true;
                }
                crate::navmap::show_at(s.lat, s.lon);
                if let Some(u) = ui() {
                    u.global::<App>().set_tab(9);
                }
                crate::navmap::render(true);
            }
        }
        "shot-monitor" | "shot-key" => {
            let v: i32 = arg.parse().unwrap_or(0);
            {
                let st = state();
                let mut b = st.borrow_mut();
                if id == "shot-monitor" {
                    b.s.shot_monitor = v.max(0);
                } else {
                    b.s.shot_key = v.clamp(0, 6);
                }
                b.s.save();
            }
            if let Some(u) = ui() {
                let a = u.global::<App>();
                a.set_shot_monitor(state().borrow().s.shot_monitor);
                a.set_shot_key(state().borrow().s.shot_key);
            }
        }
        _ => return false,
    }
    true
}
