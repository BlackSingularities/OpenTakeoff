//! Dziennik programu: `logs/opentakeoff.log` w pamięci podręcznej (po 1 MB przechodzi w `opentakeoff.1.log`)
//! oraz ostatnie wpisy w pamięci – do podglądu „Aktywność” w oknie programu.

use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;

use parking_lot::Mutex;

const LIMIT: u64 = 1024 * 1024;
const RECENT: usize = 400;

static LOCK: Mutex<()> = Mutex::new(());
static RECENT_LINES: Mutex<VecDeque<(i64, String)>> = Mutex::new(VecDeque::new());
static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn file() -> PathBuf {
    let dir = crate::paths::cache().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("opentakeoff.log")
}

fn write(level: &str, msg: &str) {
    let now = chrono::Local::now();
    {
        let mut r = RECENT_LINES.lock();
        r.push_back((crate::source::unix_ms(), msg.to_string()));
        while r.len() > RECENT {
            r.pop_front();
        }
        SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    if cfg!(test) {
        return;
    }
    let _g = LOCK.lock();
    let path = file();
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LIMIT) {
        let _ = std::fs::rename(&path, path.with_file_name("opentakeoff.1.log"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} [{level}] {msg}", now.format("%Y-%m-%d %H:%M:%S%.3f"));
    }
}

pub fn info(msg: &str) {
    write("INFO", msg);
}

pub fn warn(msg: &str) {
    write("WARN", msg);
}

pub fn error(msg: &str) {
    write("ERROR", msg);
}

/// Numer zmiany listy ostatnich wpisów (do odświeżania widoku).
pub fn serial() -> u64 {
    SERIAL.load(std::sync::atomic::Ordering::Relaxed)
}

/// Ostatnie wpisy: (czas ms, tekst), najstarsze pierwsze.
pub fn recent() -> Vec<(i64, String)> {
    RECENT_LINES.lock().iter().cloned().collect()
}

/// Każda awaria trafia do dziennika (miejsce, komunikat, przebieg wywołań).
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info.location().map(|l| l.to_string()).unwrap_or_else(|| "?".into());
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(brak komunikatu)".into());
        let trace = std::backtrace::Backtrace::force_capture();
        let thread = std::thread::current().name().unwrap_or("?").to_string();
        error(&format!("awaria w wątku {thread} ({location}): {message}\n{trace}"));
        previous(info);
    }));
}
