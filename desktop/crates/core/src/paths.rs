//! Foldery programu. Wszystkie dane leżą domyślnie obok programu, w folderze `data`:
//! `data\` – ustawienia, sparowane telefony, plan lotu, zrzuty ekranu, `data\logbook\` – dziennik lotów,
//! `data\cache\` – pamięć podręczna (mapy, baza lotnisk, logi, aktualizacje). Instalator nadaje
//! użytkownikom prawo zapisu do `data` w folderze instalacji.
//!
//! Gdy do folderu programu nie da się pisać (np. kopia w chronionym folderze bez uprawnień), dane trafiają
//! do %APPDATA%\OpenTakeoff i %LOCALAPPDATA%\OpenTakeoff jak w wersjach do 3.4. Zmienna środowiskowa
//! `OPENTAKEOFF_DATA` wskazuje inny folder (testy, kilka profili).
//!
//! Przy pierwszym uruchomieniu z folderem `data` dane z dawnego miejsca (%APPDATA%\OpenTakeoff) są do niego
//! przenoszone, więc aktualizacja z 3.4 nie gubi dziennika ani ustawień.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const APP_DIR: &str = "OpenTakeoff";

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|p| p.to_path_buf())
}

/// Dawny znacznik wersji przenośnej – dziś dane i tak leżą obok programu; zostaje dla zgodności.
pub fn portable() -> bool {
    exe_dir().is_some_and(|d| d.join("portable.ini").exists())
}

fn ensure(p: PathBuf) -> PathBuf {
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Czy do folderu da się zapisać (tworzy go, jeśli trzeba).
fn writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-test");
    let ok = std::fs::write(&probe, b"ok").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// (folder danych, folder pamięci podręcznej) – ustalane raz na uruchomienie.
fn roots() -> &'static (PathBuf, PathBuf) {
    static ROOTS: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();
    ROOTS.get_or_init(|| {
        if let Ok(dir) = std::env::var("OPENTAKEOFF_DATA") {
            let d = PathBuf::from(dir);
            return (d.clone(), d.join("cache"));
        }
        if let Some(d) = exe_dir().map(|e| e.join("data")).filter(|d| writable(d)) {
            migrate_legacy(&d);
            return (d.clone(), d.join("cache"));
        }
        (legacy_config(), dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join(APP_DIR))
    })
}

fn legacy_config() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join(APP_DIR)
}

/// Jednorazowe przeniesienie danych z %APPDATA%\OpenTakeoff do nowego folderu `data`, gdy ten jest pusty.
/// Pamięć podręczna (%LOCALAPPDATA%) nie jest przenoszona – program pobierze ją ponownie.
fn migrate_legacy(target: &Path) {
    let old = legacy_config();
    if !old.join("settings.json").exists() && !old.join("logbook").exists() {
        return;
    }
    if target.join("settings.json").exists() || target.join("logbook").exists() {
        return;
    }
    if copy_dir(&old, target).is_ok() {
        // Stary folder zostaje pod inną nazwą jako kopia bezpieczeństwa (nie jest już czytany).
        let _ = std::fs::rename(&old, old.with_file_name(format!("{APP_DIR} (przeniesione)")));
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dst = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dst)?;
        } else {
            std::fs::copy(e.path(), dst)?;
        }
    }
    Ok(())
}

/// Ustawienia, tokeny sparowanych telefonów, plan lotu, zrzuty ekranu.
pub fn config() -> PathBuf {
    ensure(roots().0.clone())
}

/// Dziennik lotów (archiwum .oab i dane dodatkowe).
pub fn logbook() -> PathBuf {
    ensure(config().join("logbook"))
}

/// Pamięć podręczna: kafelki mapy, baza lotnisk, pogoda, logi, aktualizacje.
pub fn cache() -> PathBuf {
    ensure(roots().1.clone())
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
