//! Integracja z Windows: pasek tytułu w kolorze motywu (DWM), okno bez ramki, jedna kopia programu,
//! zasobnik systemowy i uruchamianie razem z Windows.

use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE};

use crate::AppWindow;

pub fn hwnd(ui: &AppWindow) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    ui.window()
        .with_winit_window(|w| match w.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
            _ => 0,
        })
        .unwrap_or(0)
}

/// Pasek tytułu: ciemny (AMOLED – czarny, grafit – grafitowy) albo jasny.
pub fn dark_title(ui: &AppWindow, dark: bool) {
    let h = hwnd(ui);
    if h == 0 {
        return;
    }
    unsafe {
        use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE};
        let hw = HWND(h as *mut _);
        let on: i32 = if dark { 1 } else { 0 };
        let _ = DwmSetWindowAttribute(hw, DWMWA_USE_IMMERSIVE_DARK_MODE, &on as *const _ as *const _, 4);
        let mode = ui.global::<crate::Theme>().get_mode();
        let caption: u32 = match mode {
            0 => 0x0000_0000,
            1 => 0x0028_2423,
            3 => 0x002E_1B12,
            4 => 0x0010_140D,
            5 => 0x00EC_F5FA,
            _ => 0x00FF_FFFF,
        };
        let text: u32 = if dark { 0x00FF_FFFF } else { 0x001C_1818 };
        let _ = DwmSetWindowAttribute(hw, DWMWA_CAPTION_COLOR, &caption as *const _ as *const _, 4);
        let _ = DwmSetWindowAttribute(hw, DWMWA_TEXT_COLOR, &text as *const _ as *const _, 4);
    }
}

/// Polecenia okna bez ramki: przeciąganie, minimalizacja, maksymalizacja, zamknięcie, zmiana rozmiaru.
/// Wąskie okno: zmniejsz cały interfejs (współczynnik skali okna, jak zoom), zamiast go przycinać.
/// Liczone z rozmiaru fizycznego i skali systemu, więc zmiana skali nie zapętla przeliczania.
pub fn fit_scale(ui: &AppWindow) {
    const DESIGN_W: f32 = 1320.0;
    let w = ui.window();
    let size = w.size();
    let os = w.with_winit_window(|ww| ww.scale_factor() as f32).unwrap_or(1.0).max(0.5);
    let k = (size.width as f32 / os / DESIGN_W).clamp(0.72, 1.0);
    let want = os * k;
    if size.width > 0 && (w.scale_factor() - want).abs() > 0.01 {
        w.dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor: want });
    }
}

pub fn action(ui: &AppWindow, a: i32) {
    use slint::winit_030::winit::window::ResizeDirection;
    match a {
        100 => fit_scale(ui),
        0 => {
            ui.window().with_winit_window(|w| {
                let _ = w.drag_window();
            });
        }
        1 => ui.window().set_minimized(true),
        2 => {
            let m = !ui.window().is_maximized();
            ui.window().set_maximized(m);
            ui.global::<crate::App>().set_maximized(m);
        }
        3 => ui.window().dispatch_event(slint::platform::WindowEvent::CloseRequested),
        11..=18 => {
            let dir = match a {
                11 => ResizeDirection::North,
                12 => ResizeDirection::South,
                13 => ResizeDirection::West,
                14 => ResizeDirection::East,
                15 => ResizeDirection::NorthWest,
                16 => ResizeDirection::NorthEast,
                17 => ResizeDirection::SouthWest,
                _ => ResizeDirection::SouthEast,
            };
            ui.window().with_winit_window(|w| {
                let _ = w.drag_resize_window(dir);
            });
        }
        _ => {}
    }
}

pub fn bring_to_front(ui: &AppWindow) {
    ui.window().set_minimized(false);
    let h = hwnd(ui);
    if h != 0 {
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_SHOW};
            let _ = ShowWindow(HWND(h as *mut _), SW_SHOW);
            let _ = SetForegroundWindow(HWND(h as *mut _));
        }
    }
}

pub const MUTEX: &str = "Local\\OpenTakeoff-Windows-single-instance";
const SHOW_EVENT: &str = "Local\\OpenTakeoff-Windows-show";

struct SendHandle(HANDLE);
unsafe impl Send for SendHandle {}
unsafe impl Sync for SendHandle {}

static SHOW: std::sync::OnceLock<SendHandle> = std::sync::OnceLock::new();

/// true – to pierwsza kopia programu (i od teraz trzyma blokadę).
pub fn single_instance() -> bool {
    unsafe {
        let name = HSTRING::from(MUTEX);
        let Ok(m) = CreateMutexW(None, true, PCWSTR(name.as_ptr())) else { return true };
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return false;
        }
        std::mem::forget(m);
        let ev = HSTRING::from(SHOW_EVENT);
        if let Ok(e) = CreateEventW(None, false, false, PCWSTR(ev.as_ptr())) {
            let _ = SHOW.set(SendHandle(e));
        }
        true
    }
}

/// Druga kopia: poproś pierwszą o pokazanie okna.
pub fn signal_show() {
    unsafe {
        let ev = HSTRING::from(SHOW_EVENT);
        if let Ok(e) = OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(ev.as_ptr())) {
            let _ = SetEvent(e);
        }
    }
}

pub fn show_requested() -> bool {
    SHOW.get().is_some_and(|h| unsafe { WaitForSingleObject(h.0, 0) } == WAIT_OBJECT_0)
}

/// Czy działa dawny OpenTakeoff Link (zajmuje port telefonów).
pub fn legacy_link_running() -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq OpenTakeoff-Link.exe", "/NH"])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("opentakeoff-link.exe"))
        .unwrap_or(false)
}

/// Wpis w HKCU\…\Run: start razem z Windows (w zasobniku).
pub fn set_autostart(on: bool) -> Result<(), String> {
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let key = HSTRING::from("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let name = HSTRING::from("OpenTakeoff");
    unsafe {
        if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let value = format!("\"{}\" --minimized", exe.display());
            let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
            let r = RegSetKeyValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(name.as_ptr()), REG_SZ.0, Some(wide.as_ptr() as *const _), (wide.len() * 2) as u32);
            if r.is_err() {
                return Err(format!("{r:?}"));
            }
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(name.as_ptr()));
        }
    }
    Ok(())
}

/// Ikona w zasobniku: klik – pokaż okno; menu: pokaż, zakończ.
pub struct Tray {
    _icon: Option<tray_icon::TrayIcon>,
    show_id: Option<tray_icon::menu::MenuId>,
    quit_id: Option<tray_icon::menu::MenuId>,
}

impl Tray {
    pub fn new() -> Tray {
        use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
        let png = include_bytes!("../icon.png");
        let icon = image::load_from_memory(png).ok().map(|i| i.resize(32, 32, image::imageops::FilterType::Lanczos3).to_rgba8());
        let Some(rgba) = icon else { return Tray { _icon: None, show_id: None, quit_id: None } };
        let (w, h) = rgba.dimensions();
        let Ok(icon) = tray_icon::Icon::from_rgba(rgba.into_raw(), w, h) else { return Tray { _icon: None, show_id: None, quit_id: None } };
        let menu = Menu::new();
        let show = MenuItem::new(opentakeoff_core::lang::tr("Pokaż OpenTakeoff"), true, None);
        let quit = MenuItem::new(opentakeoff_core::lang::tr("Zakończ (rozłącza telefony)"), true, None);
        let _ = menu.append_items(&[&show, &PredefinedMenuItem::separator(), &quit]);
        let tray = tray_icon::TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("OpenTakeoff").with_icon(icon).build().ok();
        Tray { _icon: tray, show_id: Some(show.id().clone()), quit_id: Some(quit.id().clone()) }
    }

    pub fn take_show(&self) -> bool {
        let mut show = false;
        while let Ok(e) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::Click { button: tray_icon::MouseButton::Left, button_state: tray_icon::MouseButtonState::Up, .. } = e {
                show = true;
            }
            if let tray_icon::TrayIconEvent::DoubleClick { .. } = e {
                show = true;
            }
        }
        if let Ok(e) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if Some(&e.id) == self.show_id.as_ref() {
                show = true;
            } else if Some(&e.id) == self.quit_id.as_ref() {
                QUIT.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        show
    }

    pub fn take_quit(&self) -> bool {
        QUIT.swap(false, std::sync::atomic::Ordering::SeqCst)
    }
}

static QUIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
