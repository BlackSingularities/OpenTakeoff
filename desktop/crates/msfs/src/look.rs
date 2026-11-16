//! Rozglądanie się tak jak myszą w MSFS: przytrzymany prawy przycisk i ruch myszy (SendInput) w oknie MSFS
//! na pierwszym planie. MSFS 2024 nie pozwala zapisać kąta kamery, a CameraSetRelative6DOF przełącza w kamerę
//! 6DOF, z której symulator często nie wraca. Po puszczeniu mysz wraca o tyle samo – kamera wraca na wprost.

use opentakeoff_core::lang::tr;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT, MOUSE_EVENT_FLAGS};
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId, SetCursorPos};

const SIM_PROCESSES: [&str; 2] = ["flightsimulator2024.exe", "flightsimulator.exe"];
const MAX_STEP: i32 = 160;
const PARK_FROM_TOP: f64 = 0.14;

pub struct MouseLook {
    held: bool,
    window: isize,
    sent_x: i32,
    sent_y: i32,
    cursor: POINT,
    pub counts_per_degree: f64,
}

impl Default for MouseLook {
    fn default() -> Self {
        Self { held: false, window: 0, sent_x: 0, sent_y: 0, cursor: POINT::default(), counts_per_degree: 8.0 }
    }
}

fn send(dx: i32, dy: i32, flags: MOUSE_EVENT_FLAGS) -> u32 {
    let input = INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } };
    unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) }
}

fn process_name(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

fn is_elevated(pid: u32) -> bool {
    unsafe {
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut token = HANDLE::default();
        let mut elevated = false;
        if OpenProcessToken(p, TOKEN_QUERY, &mut token).is_ok() {
            let mut e = TOKEN_ELEVATION::default();
            let mut ret = 0u32;
            if GetTokenInformation(token, TokenElevation, Some(&mut e as *mut _ as *mut _), std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut ret).is_ok() {
                elevated = e.TokenIsElevated != 0;
            }
            let _ = CloseHandle(token);
        }
        let _ = CloseHandle(p);
        elevated
    }
}

/// Okno MSFS na pierwszym planie albo powód, dla którego nie można zacząć.
fn sim_window() -> Result<HWND, String> {
    unsafe {
        let w = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(w, Some(&mut pid));
        let name = process_name(pid).unwrap_or_default().to_lowercase();
        let file = name.rsplit(['\\', '/']).next().unwrap_or("");
        if !SIM_PROCESSES.contains(&file) {
            return Err(tr("Rozglądanie działa tylko, gdy okno MSFS jest aktywne – kliknij w okno symulatora").into());
        }
        if is_elevated(pid) && !is_elevated(std::process::id()) {
            return Err(tr("MSFS działa jako administrator – uruchom OpenTakeoff także jako administrator, żeby rozglądać się z telefonu").into());
        }
        Ok(w)
    }
}

impl MouseLook {
    pub fn active(&self) -> bool {
        self.held
    }

    /// Wciska prawy przycisk nad oknem MSFS. Err – powód odmowy.
    pub fn begin(&mut self) -> Result<(), String> {
        if self.held {
            return Ok(());
        }
        let w = sim_window()?;
        unsafe {
            let mut rc = RECT::default();
            if GetClientRect(w, &mut rc).is_err() || rc.right - rc.left < 50 || rc.bottom - rc.top < 50 {
                return Err(tr("Nie znaleziono okna MSFS").into());
            }
            let mut park = POINT { x: (rc.right - rc.left) / 2, y: ((rc.bottom - rc.top) as f64 * PARK_FROM_TOP) as i32 };
            if !ClientToScreen(w, &mut park).as_bool() {
                return Err(tr("Nie znaleziono okna MSFS").into());
            }
            let _ = GetCursorPos(&mut self.cursor);
            let _ = SetCursorPos(park.x, park.y);
            if send(0, 0, MOUSEEVENTF_RIGHTDOWN) == 0 {
                let _ = SetCursorPos(self.cursor.x, self.cursor.y);
                return Err(tr("Windows zablokował symulowanie myszy").into());
            }
        }
        self.window = w.0 as isize;
        self.held = true;
        self.sent_x = 0;
        self.sent_y = 0;
        Ok(())
    }

    fn counts(&self, yaw: f64, pitch: f64) -> (i32, i32) {
        ((yaw * self.counts_per_degree).round() as i32, (-pitch * self.counts_per_degree).round() as i32)
    }

    /// Krok w stronę kątów od początku rozglądania. false – MSFS przestał być aktywny (przycisk już puszczony).
    pub fn step(&mut self, yaw: f64, pitch: f64) -> bool {
        if !self.held {
            return false;
        }
        if unsafe { GetForegroundWindow() }.0 as isize != self.window {
            self.release();
            return false;
        }
        let (tx, ty) = self.counts(yaw, pitch);
        let dx = (tx - self.sent_x).clamp(-MAX_STEP, MAX_STEP);
        let dy = (ty - self.sent_y).clamp(-MAX_STEP, MAX_STEP);
        if (dx != 0 || dy != 0) && send(dx, dy, MOUSEEVENTF_MOVE) != 0 {
            self.sent_x += dx;
            self.sent_y += dy;
        }
        true
    }

    pub fn reached(&self, yaw: f64, pitch: f64) -> bool {
        self.counts(yaw, pitch) == (self.sent_x, self.sent_y)
    }

    pub fn release(&mut self) {
        if !self.held {
            return;
        }
        self.held = false;
        send(0, 0, MOUSEEVENTF_RIGHTUP);
        unsafe {
            let _ = SetCursorPos(self.cursor.x, self.cursor.y);
        }
    }
}
