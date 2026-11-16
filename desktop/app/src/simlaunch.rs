//! Start razem z symulatorem, dla każdej wersji i instalacji:
//! - MSFS 2020 i 2024 (Steam i Microsoft Store, dowolny folder gry): wpis OpenTakeoff w `exe.xml` – symulator
//!   sam uruchamia programy z tego pliku. Wpisy innych dodatków zostają nietknięte, przed pierwszą zmianą
//!   powstaje kopia `exe.xml.opentakeoff.bak`.
//! - X-Plane 11/12 (i MSFS, gdy `exe.xml` się nie da): mały proces czuwania w autostarcie Windows
//!   (`--sim-watch`, bez okna) sprawdza co kilka sekund procesy i uruchamia OpenTakeoff w zasobniku,
//!   gdy pojawi się symulator.

use std::path::{Path, PathBuf};

const NAME: &str = "OpenTakeoff";

/// Foldery symulatorów, w których leży (albo powinien leżeć) `exe.xml`: (nazwa, folder).
/// Zwraca tylko te, które istnieją – czyli zainstalowane i uruchomione choć raz.
fn sim_dirs() -> Vec<(&'static str, PathBuf)> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let mut out = Vec::new();
    if let Some(local) = env("LOCALAPPDATA") {
        out.push(("MSFS 2020", local.join("Packages").join("Microsoft.FlightSimulator_8wekyb3d8bbwe").join("LocalCache")));
        out.push(("MSFS 2024", local.join("Packages").join("Microsoft.Limitless_8wekyb3d8bbwe").join("LocalCache")));
    }
    if let Some(roaming) = env("APPDATA") {
        out.push(("MSFS 2020", roaming.join("Microsoft Flight Simulator")));
        out.push(("MSFS 2024", roaming.join("Microsoft Flight Simulator 2024")));
    }
    out.into_iter().filter(|(_, d)| d.is_dir()).collect()
}

/// Nazwy symulatorów, w których OpenTakeoff jest wpisany do `exe.xml`.
pub fn registered() -> Vec<&'static str> {
    let mut v: Vec<&str> = sim_dirs().into_iter().filter(|(_, d)| std::fs::read(d.join("exe.xml")).is_ok_and(|b| find_block(&b).is_some())).map(|(n, _)| n).collect();
    v.dedup();
    v
}

/// Procesy symulatorów, na które czeka proces czuwania (małe litery).
const SIM_EXES: &[&str] = &["x-plane.exe", "flightsimulator.exe", "flightsimulator2024.exe"];
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const WATCH_NAME: &str = "OpenTakeoff-SimWatch";
const WATCH_MUTEX: &str = "Local\\OpenTakeoff-SimWatch";

/// Włącza albo wyłącza start z symulatorem: `exe.xml` znalezionych MSFS i proces czuwania (X-Plane, inne instalacje).
/// Ok: nazwy symulatorów, z którymi program wystartuje.
pub fn set(on: bool) -> Result<Vec<&'static str>, String> {
    let mut sims = set_exe_xml(on)?;
    set_watch(on)?;
    if on {
        if !opentakeoff_core::xpnav::detect().is_empty() {
            sims.push("X-Plane");
        }
        sims.dedup();
    }
    Ok(sims)
}

/// Wpis procesu czuwania w autostarcie i uruchomienie go od razu (bez ponownego logowania).
fn set_watch(on: bool) -> Result<(), String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let key = HSTRING::from(RUN_KEY);
    let name = HSTRING::from(WATCH_NAME);
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    unsafe {
        if on {
            let value = format!("\"{}\" --sim-watch", exe.display());
            let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
            let r = RegSetKeyValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(name.as_ptr()), REG_SZ.0, Some(wide.as_ptr() as *const _), (wide.len() * 2) as u32);
            if r.is_err() {
                return Err(format!("{r:?}"));
            }
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(name.as_ptr()));
        }
    }
    if on && !mutex_exists(WATCH_MUTEX) {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new(&exe).arg("--sim-watch").creation_flags(0x0800_0008).spawn();
    }
    Ok(())
}

fn watch_registered() -> bool {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let key = HSTRING::from(RUN_KEY);
    let name = HSTRING::from(WATCH_NAME);
    let mut size = 0u32;
    unsafe { RegGetValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(name.as_ptr()), RRF_RT_REG_SZ, None, None, Some(&mut size)).is_ok() }
}

fn mutex_exists(name: &str) -> bool {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
    let n = HSTRING::from(name);
    unsafe {
        match OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, PCWSTR(n.as_ptr())) {
            Ok(h) => {
                let _ = CloseHandle(h);
                true
            }
            Err(_) => false,
        }
    }
}

/// Czy działa któryś symulator (lista procesów przez ToolHelp, bez uruchamiania poleceń).
fn sim_running() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = false;
        if Process32FirstW(snap, &mut e).is_ok() {
            loop {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                let name = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
                if SIM_EXES.contains(&name.as_str()) {
                    found = true;
                    break;
                }
                if Process32NextW(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        found
    }
}

/// Odświeżenie przy starcie programu: ścieżka w `exe.xml` (np. po reinstalacji) i proces czuwania.
pub fn set_refresh() -> Result<(), String> {
    set_exe_xml(true)?;
    set_watch(true)
}

/// Proces czuwania (`--sim-watch`): bez okna, jedna kopia; gdy pojawi się symulator, a OpenTakeoff nie działa,
/// uruchamia go w zasobniku. Kończy się, gdy użytkownik wyłączy start z symulatorem.
pub fn watch_main() {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    let n = HSTRING::from(WATCH_MUTEX);
    let _guard = unsafe { CreateMutexW(None, true, PCWSTR(n.as_ptr())) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let mut was_running = sim_running();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(4));
        if !watch_registered() {
            return;
        }
        let running = sim_running();
        if running && !was_running && !mutex_exists(crate::win::MUTEX) {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new(&exe).args(["--minimized", "--from-sim"]).creation_flags(0x0000_0008).spawn();
        }
        was_running = running;
    }
}

/// `exe.xml` wszystkich znalezionych MSFS (2020 / 2024, Steam / Store).
fn set_exe_xml(on: bool) -> Result<Vec<&'static str>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut done = Vec::new();
    let mut errors = Vec::new();
    for (name, dir) in sim_dirs() {
        match update(&dir.join("exe.xml"), on.then_some(exe.as_path())) {
            Ok(true) => done.push(name),
            Ok(false) => {}
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }
    done.dedup();
    if errors.is_empty() {
        Ok(done)
    } else {
        Err(errors.join("; "))
    }
}

/// Dopisuje (exe = Some) albo usuwa (None) wpis w jednym pliku. Ok(true), gdy plik się zmienił.
fn update(path: &Path, exe: Option<&Path>) -> Result<bool, String> {
    let old = match std::fs::read(path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    let new = match (&old, exe) {
        (None, None) => return Ok(false),
        (None, Some(exe)) => new_file(&block(exe, true)?),
        (Some(b), _) => edit(b, exe)?,
    };
    if old.as_deref() == Some(new.as_slice()) {
        return Ok(false);
    }
    if let Some(b) = &old {
        let bak = path.with_extension("xml.opentakeoff.bak");
        if !bak.exists() {
            std::fs::write(&bak, b).map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(path, new).map_err(|e| e.to_string())?;
    Ok(true)
}

fn new_file(block: &[u8]) -> Vec<u8> {
    let mut v = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<SimBase.Document Type=\"Launch\" version=\"1,0\">\r\n  <Descr>Launch</Descr>\r\n  <Filename>exe.xml</Filename>\r\n  <Disabled>False</Disabled>\r\n  <Launch.ManualLoad>False</Launch.ManualLoad>\r\n".to_vec();
    v.extend_from_slice(block);
    v.extend_from_slice(b"</SimBase.Document>\r\n");
    v
}

/// Zmiana istniejącego pliku na poziomie bajtów (pliki MSFS bywają w Windows-1252, więc bez przekodowania):
/// usuwa nasz poprzedni wpis i – przy włączaniu – dopisuje nowy przed `</SimBase.Document>`.
fn edit(b: &[u8], exe: Option<&Path>) -> Result<Vec<u8>, String> {
    let mut v = b.to_vec();
    while let Some((s, e)) = find_block(&v) {
        v.drain(s..e);
    }
    let Some(exe) = exe else { return Ok(v) };
    let utf8 = is_utf8_decl(&v) || v.is_ascii();
    let blk = block(exe, utf8)?;
    if utf8 && !is_utf8_decl(&v) {
        // Plik czysto ASCII: można bezpiecznie ogłosić UTF-8 (polskie litery w ścieżce)
        v = set_utf8_decl(&v);
    }
    let close = find(&v, b"</SimBase.Document>").ok_or("exe.xml bez </SimBase.Document>")?;
    let mut out = v[..close].to_vec();
    out.extend_from_slice(&blk);
    out.extend_from_slice(&v[close..]);
    Ok(out)
}

fn block(exe: &Path, utf8: bool) -> Result<Vec<u8>, String> {
    let p = exe.display().to_string().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let s = format!("  <Launch.Addon>\r\n    <Name>{NAME}</Name>\r\n    <Disabled>False</Disabled>\r\n    <Path>{p}</Path>\r\n    <CommandLine>--minimized --from-sim</CommandLine>\r\n  </Launch.Addon>\r\n");
    if utf8 {
        return Ok(s.into_bytes());
    }
    // Windows-1252: znaki do U+00FF zapisujemy jako jeden bajt; dalszych (np. „ł”) nie da się zapisać
    s.chars().map(|c| u8::try_from(c as u32).map_err(|_| format!("ścieżki programu ({}) nie da się zapisać w kodowaniu exe.xml", exe.display()))).collect()
}

/// Zakres bajtów naszego wpisu `<Launch.Addon>…<Name>OpenTakeoff</Name>…</Launch.Addon>` (z wcięciem i końcem linii).
fn find_block(b: &[u8]) -> Option<(usize, usize)> {
    let open = b"<Launch.Addon>";
    let close = b"</Launch.Addon>";
    let mut from = 0;
    while let Some(i) = find(&b[from..], open) {
        let s = from + i;
        let e = s + find(&b[s..], close)? + close.len();
        let body = &b[s..e];
        if find(body, format!("<Name>{NAME}</Name>").as_bytes()).is_some() {
            let mut s2 = s;
            while s2 > 0 && (b[s2 - 1] == b' ' || b[s2 - 1] == b'\t') {
                s2 -= 1;
            }
            let mut e2 = e;
            if b.get(e2) == Some(&b'\r') {
                e2 += 1;
            }
            if b.get(e2) == Some(&b'\n') {
                e2 += 1;
            }
            return Some((s2, e2));
        }
        from = e;
    }
    None
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

fn decl(b: &[u8]) -> Option<(usize, usize)> {
    let s = find(b, b"<?xml")?;
    let e = s + find(&b[s..], b"?>")? + 2;
    Some((s, e))
}

fn is_utf8_decl(b: &[u8]) -> bool {
    decl(b).is_some_and(|(s, e)| {
        let d = String::from_utf8_lossy(&b[s..e]).to_ascii_lowercase();
        d.contains("utf-8") || !d.contains("encoding")
    })
}

fn set_utf8_decl(b: &[u8]) -> Vec<u8> {
    let new = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>";
    match decl(b) {
        Some((s, e)) => [&b[..s], &new[..], &b[e..]].concat(),
        None => [&new[..], b"\r\n", b].concat(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OTHER: &str = "<?xml version=\"1.0\" encoding=\"Windows-1252\"?>\r\n<SimBase.Document Type=\"Launch\" version=\"1,0\">\r\n  <Descr>Launch</Descr>\r\n  <Launch.Addon>\r\n    <Name>FSUIPC7</Name>\r\n    <Path>C:\\FSUIPC7\\FSUIPC7.exe</Path>\r\n  </Launch.Addon>\r\n</SimBase.Document>\r\n";

    #[test]
    fn add_and_remove_keeps_other_addons() {
        let exe = Path::new(r"C:\Program Files\OpenTakeoff\OpenTakeoff.exe");
        let added = edit(OTHER.as_bytes(), Some(exe)).unwrap();
        let s = String::from_utf8(added.clone()).unwrap();
        assert!(s.contains("FSUIPC7") && s.contains("<Name>OpenTakeoff</Name>") && s.contains(r"<Path>C:\Program Files\OpenTakeoff\OpenTakeoff.exe</Path>"));
        assert!(s.find("OpenTakeoff</Name>").unwrap() < s.find("</SimBase.Document>").unwrap());
        // ponowne włączenie nie dubluje wpisu
        let again = edit(&added, Some(exe)).unwrap();
        assert_eq!(String::from_utf8(again).unwrap().matches("<Name>OpenTakeoff</Name>").count(), 1);
        // wyłączenie przywraca treść z innymi dodatkami (deklaracja zmieniona na UTF-8 – plik był ASCII)
        let removed = String::from_utf8(edit(&added, None).unwrap()).unwrap();
        assert_eq!(removed, OTHER.replace("encoding=\"Windows-1252\"", "encoding=\"UTF-8\""));
    }

    #[test]
    fn cp1252_with_accents_and_polish_path() {
        let mut b = OTHER.as_bytes().to_vec();
        let i = find(&b, b"FSUIPC7</Name>").unwrap();
        b.insert(i, 0xE9); // „é” w Windows-1252
        assert!(edit(&b, Some(Path::new(r"C:\Gry\OpenTakeoff.exe"))).is_ok());
        assert!(edit(&b, Some(Path::new(r"C:\Użytkownik\OpenTakeoff.exe"))).is_err());
    }

    #[test]
    fn new_file_has_addon() {
        let s = String::from_utf8(new_file(&block(Path::new(r"C:\OT\OpenTakeoff.exe"), true).unwrap())).unwrap();
        assert!(s.starts_with("<?xml") && s.contains("<Launch.Addon>") && s.ends_with("</SimBase.Document>\r\n"));
        assert!(find_block(s.as_bytes()).is_some());
    }
}
