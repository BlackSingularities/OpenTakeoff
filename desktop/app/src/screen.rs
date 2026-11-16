//! Ekrany i okna nakładek: lista monitorów, ustawianie okna w wybranym miejscu wybranego ekranu,
//! okna „zawsze na wierzchu”, które nie zabierają fokusu symulatorowi, przepuszczanie kliknięć
//! oraz zrzut ekranu (GDI).

use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;
use windows::Win32::Foundation::HWND;

/// Monitor w pikselach fizycznych.
#[derive(Clone, Debug)]
pub struct Monitor {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub primary: bool,
}

/// Monitory (główny pierwszy) odczytane przez okno programu.
pub fn monitors(win: &slint::Window) -> Vec<Monitor> {
    win.with_winit_window(|w| {
        let primary = w.primary_monitor();
        let mut out: Vec<Monitor> = w
            .available_monitors()
            .map(|m| {
                let p = m.position();
                let s = m.size();
                let is_primary = primary.as_ref().is_some_and(|pm| pm == &m);
                Monitor { name: m.name().unwrap_or_default(), x: p.x, y: p.y, w: s.width, h: s.height, primary: is_primary }
            })
            .collect();
        out.sort_by_key(|m| (!m.primary, m.x, m.y));
        out
    })
    .filter(|l| !l.is_empty())
    .unwrap_or_else(primary_only)
}

/// Gdy okno programu jeszcze nie powstało (start w zasobniku) – sam ekran główny.
fn primary_only() -> Vec<Monitor> {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    if w <= 0 || h <= 0 {
        return Vec::new();
    }
    vec![Monitor { name: String::new(), x: 0, y: 0, w: w as u32, h: h as u32, primary: true }]
}

/// Nazwy ekranów do listy wyboru: „Ekran 1 (główny) · 2560 × 1440”.
pub fn monitor_labels(list: &[Monitor]) -> Vec<String> {
    list.iter()
        .enumerate()
        .map(|(i, m)| {
            let main = if m.primary { format!(" ({})", opentakeoff_core::lang::tr("główny")) } else { String::new() };
            format!("{} {}{} · {} × {}", opentakeoff_core::lang::tr("Ekran"), i + 1, main, m.w, m.h)
        })
        .collect()
}

pub fn hwnd_of(win: &slint::Window) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    win.with_winit_window(|w| match w.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
        _ => 0,
    })
    .unwrap_or(0)
}

/// Pozycja lewego górnego rogu okna `w`×`h` na monitorze: `pos` jak w ustawieniach (0 środek,
/// 1 góra-środek, 2 dół-środek, 3 lewy górny, 4 prawy górny, 5 lewy dolny, 6 prawy dolny).
pub fn place(m: &Monitor, pos: i32, w: u32, h: u32, margin: i32) -> (i32, i32) {
    let (mw, mh) = (m.w as i32, m.h as i32);
    let (w, h) = (w as i32, h as i32);
    let cx = m.x + (mw - w) / 2;
    let cy = m.y + (mh - h) / 2;
    let left = m.x + margin;
    let right = m.x + mw - w - margin;
    let top = m.y + margin;
    let bottom = m.y + mh - h - margin;
    match pos {
        1 => (cx, top),
        2 => (cx, bottom),
        3 => (left, top),
        4 => (right, top),
        5 => (left, bottom),
        6 => (right, bottom),
        _ => (cx, cy),
    }
}

/// Okno nakładki: zawsze na wierzchu, bez przycisku na pasku zadań, nie aktywuje się po kliknięciu
/// (symulator nie traci fokusu), zaokrąglone rogi w Windows 11; opcjonalnie przepuszcza kliknięcia.
pub fn make_overlay(win: &slint::Window, click_through: bool) {
    let h = hwnd_of(win);
    if h != 0 {
        unsafe {
            use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
            use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST};
            let hw = HWND(h as *mut _);
            let ex = GetWindowLongPtrW(hw, GWL_EXSTYLE);
            let want = ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0) as isize;
            if want != ex {
                SetWindowLongPtrW(hw, GWL_EXSTYLE, want);
            }
            let pref = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(hw, DWMWA_WINDOW_CORNER_PREFERENCE, &pref as *const _ as *const _, 4);
            let _ = SetWindowPos(hw, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
    win.with_winit_window(|w| {
        let _ = w.set_cursor_hittest(!click_through);
    });
}

/// Okno, które było aktywne przed pokazaniem nakładki (żeby oddać mu fokus).
pub fn foreground() -> isize {
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow().0 as isize }
}

/// Oddaje fokus oknu `h` (np. symulatorowi), jeśli nakładka go przejęła.
pub fn restore_foreground(h: isize, overlay: &slint::Window) {
    if h == 0 {
        return;
    }
    let mine = hwnd_of(overlay);
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};
        if GetForegroundWindow().0 as isize == mine && mine != 0 {
            let _ = SetForegroundWindow(HWND(h as *mut _));
        }
    }
}

/// Ustawia okno na monitorze `m` w położeniu `pos` (rozmiar fizyczny bierze z okna).
pub fn position<T: ComponentHandle>(c: &T, m: &Monitor, pos: i32, margin_logical: i32) {
    let win = c.window();
    let size = win.size();
    let sf = win.scale_factor();
    let margin = (margin_logical as f32 * sf).round() as i32;
    let (x, y) = place(m, pos, size.width, size.height, margin);
    win.set_position(slint::PhysicalPosition::new(x, y));
}

/// Zrzut prostokąta ekranu (piksele fizyczne pulpitu) jako RGB.
pub fn capture(m: &Monitor) -> Option<image::RgbImage> {
    use windows::Win32::Graphics::Gdi::*;
    let (w, h) = (m.w as i32, m.h as i32);
    if w <= 0 || h <= 0 {
        return None;
    }
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp.into());
        let ok = BitBlt(mem, 0, 0, w, h, Some(screen), m.x, m.y, SRCCOPY | CAPTUREBLT).is_ok();
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = w;
        info.bmiHeader.biHeight = -h;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let lines = if ok { GetDIBits(mem, bmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS) } else { 0 };
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if lines <= 0 {
            return None;
        }
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for px in buf.chunks_exact(4) {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        image::RgbImage::from_raw(w as u32, h as u32, rgb)
    }
}

/// Stan klawisza (wciśnięty teraz) – do skrótów działających także w symulatorze.
pub fn key_down(vk: u16) -> bool {
    unsafe { (windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}
