//! Strony dla OBS (źródło „Przeglądarka”): pasek do transmisji i karta lądowania pod
//! `http://127.0.0.1:47810/obs/…`. Działają też wtedy, gdy symulator jest w wyłącznym pełnym
//! ekranie, i mają przezroczyste tło bez przechwytywania okna.
//!
//! Strony co pół sekundy pobierają JSON z bieżącymi danymi; program odświeża je z głównego wątku
//! tylko wtedy, gdy jakaś strona była niedawno otwarta.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use opentakeoff_core::server::WebPages;
use opentakeoff_core::source::unix_ms;
use parking_lot::Mutex;
use serde_json::{json, Value};

const INDEX: &str = include_str!("../web/index.html");
const BAR: &str = include_str!("../web/bar.html");
const LANDING: &str = include_str!("../web/landing.html");
const LOGO: &[u8] = include_bytes!("../icon.png");
const MONO: &[u8] = include_bytes!("../fonts/b612mono_bold.ttf");
const MONO_R: &[u8] = include_bytes!("../fonts/b612mono_regular.ttf");
const COND: &[u8] = include_bytes!("../fonts/plexcond_bold.ttf");
const COND_R: &[u8] = include_bytes!("../fonts/plexcond_regular.ttf");

pub struct Pages {
    bar: Mutex<String>,
    landing: Mutex<Value>,
    demo: Mutex<Value>,
    index: Mutex<Value>,
    seq: AtomicU64,
    lan: AtomicBool,
    polled: AtomicI64,
}

static PAGES: OnceLock<Arc<Pages>> = OnceLock::new();

pub fn pages() -> Arc<Pages> {
    PAGES
        .get_or_init(|| {
            Arc::new(Pages {
                bar: Mutex::new("{\"cells\":[]}".into()),
                landing: Mutex::new(json!({ "seq": 0 })),
                demo: Mutex::new(json!({ "seq": 0 })),
                index: Mutex::new(json!({})),
                seq: AtomicU64::new(0),
                lan: AtomicBool::new(false),
                polled: AtomicI64::new(0),
            })
        })
        .clone()
}

impl WebPages for Pages {
    fn get(&self, path: &str, query: &str) -> Option<(&'static str, Vec<u8>)> {
        let html = "text/html; charset=utf-8";
        let js = "application/json; charset=utf-8";
        match path.trim_end_matches('/') {
            "/obs" => Some((html, INDEX.replace("/*DATA*/{}", &self.index.lock().to_string()).into_bytes())),
            "/obs/bar" => Some((html, BAR.as_bytes().to_vec())),
            "/obs/landing" => Some((html, LANDING.as_bytes().to_vec())),
            "/obs/bar.json" => {
                self.polled.store(unix_ms(), Ordering::Relaxed);
                Some((js, self.bar.lock().clone().into_bytes()))
            }
            "/obs/landing.json" => {
                self.polled.store(unix_ms(), Ordering::Relaxed);
                let v = if query.split('&').any(|q| q == "demo=1") { self.demo.lock().clone() } else { self.landing.lock().clone() };
                Some((js, v.to_string().into_bytes()))
            }
            "/obs/logo.png" => Some(("image/png", LOGO.to_vec())),
            "/obs/fonts/mono-bold.ttf" => Some(("font/ttf", MONO.to_vec())),
            "/obs/fonts/mono.ttf" => Some(("font/ttf", MONO_R.to_vec())),
            "/obs/fonts/cond-bold.ttf" => Some(("font/ttf", COND.to_vec())),
            "/obs/fonts/cond.ttf" => Some(("font/ttf", COND_R.to_vec())),
            _ => None,
        }
    }

    fn allow_lan(&self) -> bool {
        self.lan.load(Ordering::Relaxed)
    }
}

pub fn set_lan(on: bool) {
    pages().lan.store(on, Ordering::Relaxed);
}

/// Czy jakaś strona OBS pobierała dane w ostatnich 15 s (wtedy warto liczyć pasek).
pub fn wanted() -> bool {
    unix_ms() - pages().polled.load(Ordering::Relaxed) < 15_000
}

pub fn publish_bar(v: Value) {
    *pages().bar.lock() = v.to_string();
}

/// Nowe lądowanie (albo podgląd z ustawień): strona karty pokaże je, gdy zobaczy nowy numer.
pub fn publish_landing(mut v: Value) {
    let p = pages();
    let seq = p.seq.fetch_add(1, Ordering::Relaxed) + 1;
    v["seq"] = json!(seq);
    v["at"] = json!(unix_ms());
    *p.landing.lock() = v;
}

/// Przykładowa karta na stałe (`?demo=1`) – do ustawienia źródła w OBS.
pub fn set_demo(mut v: Value) {
    v["seq"] = json!(1);
    v["at"] = json!(unix_ms());
    *pages().demo.lock() = v;
}

/// Teksty strony z adresami (język programu).
pub fn set_index(v: Value) {
    *pages().index.lock() = v;
}

pub fn color(c: slint::Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue())
}
