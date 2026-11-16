//! Foldery programu. Wersja przenośna (plik `portable.ini` obok programu) trzyma wszystko w folderze `data`
//! obok pliku .exe; zainstalowana – w %APPDATA%\OpenTakeoff (ustawienia, dziennik lotów) i
//! %LOCALAPPDATA%\OpenTakeoff (pamięć podręczna: mapy, baza lotnisk, aktualizacje).

use std::path::PathBuf;

pub const APP_DIR: &str = "OpenTakeoff";

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|p| p.to_path_buf())
}

pub fn portable() -> bool {
    exe_dir().is_some_and(|d| d.join("portable.ini").exists())
}

fn ensure(p: PathBuf) -> PathBuf {
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Ustawienia i tokeny sparowanych telefonów.
pub fn config() -> PathBuf {
    if let Ok(dir) = std::env::var("OPENTAKEOFF_DATA") {
        return ensure(PathBuf::from(dir));
    }
    if portable() {
        if let Some(d) = exe_dir() {
            return ensure(d.join("data"));
        }
    }
    ensure(dirs::config_dir().unwrap_or_else(std::env::temp_dir).join(APP_DIR))
}

/// Dziennik lotów (archiwum .oab i dane dodatkowe).
pub fn logbook() -> PathBuf {
    ensure(config().join("logbook"))
}

/// Pamięć podręczna: kafelki mapy, baza lotnisk, pogoda, aktualizacje.
pub fn cache() -> PathBuf {
    if let Ok(dir) = std::env::var("OPENTAKEOFF_DATA") {
        return ensure(PathBuf::from(dir).join("cache"));
    }
    if portable() {
        if let Some(d) = exe_dir() {
            return ensure(d.join("data").join("cache"));
        }
    }
    ensure(dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join(APP_DIR))
}

/// Folder dawnego programu OpenTakeoff Link (C#) – stąd przejmujemy tokeny sparowanych telefonów.
pub fn legacy_link() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("OpenAirBook Link")
}

/// Bezpieczny zapis: najpierw plik tymczasowy, potem zamiana (awaria w trakcie nie niszczy starego pliku).
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
