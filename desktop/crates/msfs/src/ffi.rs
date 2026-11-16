//! Wiązania SimConnect ładowane dynamicznie. SimConnect.dll (z MSFS SDK) jest wbudowany w program i przy
//! pierwszym użyciu trafia do pamięci podręcznej – program nie wymaga instalacji SDK.

use std::ffi::{c_char, c_void, CString};
use std::path::PathBuf;
use std::sync::OnceLock;

use sha2::{Digest, Sha256};

pub type Handle = *mut c_void;
pub type HResult = i32;

pub const DATATYPE_FLOAT64: u32 = 4;
pub const DATATYPE_STRING64: u32 = 7;
pub const DATATYPE_STRING256: u32 = 9;
pub const PERIOD_VISUAL_FRAME: u32 = 2;
pub const OBJECT_ID_USER: u32 = 0;
pub const UNUSED: u32 = 0xFFFF_FFFF;
pub const GROUP_PRIORITY_HIGHEST: u32 = 1;
pub const EVENT_FLAG_GROUPID_IS_PRIORITY: u32 = 0x10;

pub const RECV_EXCEPTION: u32 = 1;
pub const RECV_OPEN: u32 = 2;
pub const RECV_QUIT: u32 = 3;
pub const RECV_EVENT: u32 = 4;
pub const RECV_SIMOBJECT_DATA: u32 = 8;
pub const RECV_AIRPORT_LIST: u32 = 18;
pub const RECV_VOR_LIST: u32 = 19;
pub const RECV_NDB_LIST: u32 = 20;
pub const RECV_WAYPOINT_LIST: u32 = 21;
pub const RECV_FACILITY_DATA: u32 = 28;
pub const RECV_FACILITY_DATA_END: u32 = 29;
pub const RECV_FACILITY_MINIMAL_LIST: u32 = 30;

static DLL: &[u8] = include_bytes!("../../../vendor/SimConnect.dll");

type FnOpen = unsafe extern "system" fn(*mut Handle, *const c_char, *mut c_void, u32, Handle, u32) -> HResult;
type FnClose = unsafe extern "system" fn(Handle) -> HResult;
type FnNext = unsafe extern "system" fn(Handle, *mut *mut u8, *mut u32) -> HResult;
type FnAddData = unsafe extern "system" fn(Handle, u32, *const c_char, *const c_char, u32, f32, u32) -> HResult;
type FnRequestData = unsafe extern "system" fn(Handle, u32, u32, u32, u32, u32, u32, u32, u32) -> HResult;
type FnMapEvent = unsafe extern "system" fn(Handle, u32, *const c_char) -> HResult;
type FnTransmit = unsafe extern "system" fn(Handle, u32, u32, u32, u32, u32) -> HResult;
type FnSetData = unsafe extern "system" fn(Handle, u32, u32, u32, u32, u32, *mut c_void) -> HResult;
type FnSubscribe = unsafe extern "system" fn(Handle, u32, *const c_char) -> HResult;
type FnAddFacility = unsafe extern "system" fn(Handle, u32, *const c_char) -> HResult;
type FnRequestFacility = unsafe extern "system" fn(Handle, u32, u32, *const c_char, *const c_char) -> HResult;
type FnRequestFacilityEx1 = unsafe extern "system" fn(Handle, u32, u32, *const c_char, *const c_char, c_char) -> HResult;
type FnRequestList = unsafe extern "system" fn(Handle, u32, u32) -> HResult;
type FnLastPacket = unsafe extern "system" fn(Handle, *mut u32) -> HResult;

pub struct Api {
    _lib: libloading::Library,
    open: FnOpen,
    close: FnClose,
    next: FnNext,
    add_data: FnAddData,
    request_data: FnRequestData,
    map_event: FnMapEvent,
    transmit: FnTransmit,
    set_data: FnSetData,
    subscribe: FnSubscribe,
    add_facility: Option<FnAddFacility>,
    request_facility: Option<FnRequestFacility>,
    request_facility_ex1: Option<FnRequestFacilityEx1>,
    request_list: Option<FnRequestList>,
    last_packet: FnLastPacket,
}

// Wskaźniki funkcji z biblioteki są ważne przez cały czas życia `_lib`.
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

fn dll_path() -> Result<PathBuf, String> {
    // Najpierw obok programu (instalator), potem kopia w pamięci podręcznej.
    if let Some(p) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("SimConnect.dll"))) {
        if p.exists() {
            return Ok(p);
        }
    }
    let p = opentakeoff_core::paths::cache().join("SimConnect.dll");
    let want = Sha256::digest(DLL);
    let ok = std::fs::read(&p).map(|b| Sha256::digest(&b) == want).unwrap_or(false);
    if !ok {
        std::fs::write(&p, DLL).map_err(|e| format!("SimConnect.dll: {e}"))?;
    }
    Ok(p)
}

fn load() -> Result<Api, String> {
    let path = dll_path()?;
    unsafe {
        let lib = libloading::Library::new(&path).map_err(|e| e.to_string())?;
        macro_rules! req {
            ($n:literal) => {
                *lib.get(concat!($n, "\0").as_bytes()).map_err(|e| format!("{}: {e}", $n))?
            };
        }
        macro_rules! opt {
            ($n:literal) => {
                lib.get(concat!($n, "\0").as_bytes()).ok().map(|s| *s)
            };
        }
        Ok(Api {
            open: req!("SimConnect_Open"),
            close: req!("SimConnect_Close"),
            next: req!("SimConnect_GetNextDispatch"),
            add_data: req!("SimConnect_AddToDataDefinition"),
            request_data: req!("SimConnect_RequestDataOnSimObject"),
            map_event: req!("SimConnect_MapClientEventToSimEvent"),
            transmit: req!("SimConnect_TransmitClientEvent"),
            set_data: req!("SimConnect_SetDataOnSimObject"),
            subscribe: req!("SimConnect_SubscribeToSystemEvent"),
            add_facility: opt!("SimConnect_AddToFacilityDefinition"),
            request_facility: opt!("SimConnect_RequestFacilityData"),
            request_facility_ex1: opt!("SimConnect_RequestFacilityData_EX1"),
            request_list: opt!("SimConnect_RequestFacilitiesList_EX1"),
            last_packet: req!("SimConnect_GetLastSentPacketID"),
            _lib: lib,
        })
    }
}

/// Biblioteka SimConnect (wczytywana raz). Err – brak biblioteki albo nieznana wersja.
pub fn api() -> Result<&'static Api, String> {
    static API: OnceLock<Result<Api, String>> = OnceLock::new();
    API.get_or_init(load).as_ref().map_err(|e| e.clone())
}

fn cs(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

/// Otwarte połączenie. Wywołania muszą być szeregowane (robi to MsfsSource przez Mutex).
pub struct Conn {
    pub h: Handle,
    pub api: &'static Api,
}

unsafe impl Send for Conn {}

/// Wiadomość z kolejki SimConnect (kopia – pamięć biblioteki jest ważna tylko do następnego wywołania).
pub struct Msg {
    pub id: u32,
    pub data: Vec<u8>,
}

impl Msg {
    pub fn u32(&self, offset: usize) -> u32 {
        self.data.get(offset..offset + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0)
    }
}

impl Conn {
    pub fn open(name: &str) -> Result<Conn, String> {
        let api = api()?;
        let mut h: Handle = std::ptr::null_mut();
        let n = cs(name);
        let hr = unsafe { (api.open)(&mut h, n.as_ptr(), std::ptr::null_mut(), 0, std::ptr::null_mut(), 0) };
        if hr < 0 || h.is_null() {
            return Err(format!("SimConnect_Open 0x{hr:08X}"));
        }
        Ok(Conn { h, api })
    }

    pub fn next(&self) -> Option<Msg> {
        let mut p: *mut u8 = std::ptr::null_mut();
        let mut n: u32 = 0;
        let hr = unsafe { (self.api.next)(self.h, &mut p, &mut n) };
        if hr < 0 || p.is_null() || n < 12 {
            return None;
        }
        let data = unsafe { std::slice::from_raw_parts(p, n as usize) }.to_vec();
        let id = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        Some(Msg { id, data })
    }

    pub fn add_data(&self, def: u32, name: &str, unit: Option<&str>, kind: u32) -> HResult {
        let n = cs(name);
        let u = unit.map(cs);
        unsafe { (self.api.add_data)(self.h, def, n.as_ptr(), u.as_ref().map(|u| u.as_ptr()).unwrap_or(std::ptr::null()), kind, 0.0, UNUSED) }
    }

    pub fn request_data(&self, req: u32, def: u32, period: u32) -> HResult {
        unsafe { (self.api.request_data)(self.h, req, def, OBJECT_ID_USER, period, 0, 0, 0, 0) }
    }

    pub fn map_event(&self, id: u32, name: &str) -> HResult {
        let n = cs(name);
        unsafe { (self.api.map_event)(self.h, id, n.as_ptr()) }
    }

    pub fn transmit(&self, event: u32, data: u32) -> HResult {
        unsafe { (self.api.transmit)(self.h, OBJECT_ID_USER, event, data, GROUP_PRIORITY_HIGHEST, EVENT_FLAG_GROUPID_IS_PRIORITY) }
    }

    pub fn set_f64(&self, def: u32, value: f64) -> HResult {
        let mut v = value;
        unsafe { (self.api.set_data)(self.h, def, OBJECT_ID_USER, 0, 0, 8, &mut v as *mut f64 as *mut c_void) }
    }

    pub fn subscribe(&self, id: u32, name: &str) -> HResult {
        let n = cs(name);
        unsafe { (self.api.subscribe)(self.h, id, n.as_ptr()) }
    }

    pub fn has_facilities(&self) -> bool {
        self.api.add_facility.is_some() && self.api.request_facility.is_some() && self.api.request_facility_ex1.is_some()
    }

    pub fn add_facility(&self, def: u32, field: &str) -> HResult {
        let Some(f) = self.api.add_facility else { return -1 };
        let n = cs(field);
        unsafe { f(self.h, def, n.as_ptr()) }
    }

    pub fn request_facility(&self, def: u32, req: u32, icao: &str, region: &str, kind: Option<u8>) -> HResult {
        let (i, r) = (cs(icao), cs(region));
        unsafe {
            match kind {
                Some(t) => match self.api.request_facility_ex1 {
                    Some(f) => f(self.h, def, req, i.as_ptr(), r.as_ptr(), t as c_char),
                    None => -1,
                },
                None => match self.api.request_facility {
                    Some(f) => f(self.h, def, req, i.as_ptr(), r.as_ptr()),
                    None => -1,
                },
            }
        }
    }

    pub fn request_list(&self, kind: u32, req: u32) -> HResult {
        let Some(f) = self.api.request_list else { return -1 };
        unsafe { f(self.h, kind, req) }
    }

    pub fn last_packet(&self) -> Option<u32> {
        let mut id = 0u32;
        let hr = unsafe { (self.api.last_packet)(self.h, &mut id) };
        (hr >= 0).then_some(id)
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        unsafe {
            (self.api.close)(self.h);
        }
    }
}
