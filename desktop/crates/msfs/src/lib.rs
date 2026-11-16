//! Połączenie z MSFS 2020/2024 przez SimConnect. Samo łączy się ponownie, gdy symulator zostanie zamknięty
//! lub uruchomiony później. Dane samolotu przychodzą co klatkę; komendy i osie z telefonu idą jako zdarzenia.

mod data;
mod ffi;
mod look;
mod navdata;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use opentakeoff_core::axes::AxisController;
use opentakeoff_core::commands::{self, Target, Validated};
use opentakeoff_core::lang::tr;
use opentakeoff_core::navdata::NavDataProvider;
use opentakeoff_core::proto::{codes, r, FlightState, TouchdownData};
use opentakeoff_core::source::{tick_ms, unix_ms, CommandResult, FlightSource};
use opentakeoff_core::touchdown::{Sample, TouchdownLatch};
use opentakeoff_core::trf;
use parking_lot::Mutex;

use data::MsfsData;
use ffi::Conn;
use look::MouseLook;
use navdata::MsfsNavData;

const DEF_STATE: u32 = 1;
const DEF_XPDR: u32 = 2;
const DEF_CAMERA: u32 = 3;
const REQ_STATE: u32 = 1;
const PAUSE_EVENT: u32 = 9001;
const FIRST_EVENT: u32 = 10_000;
const AXIS_RANGE: f64 = 16383.0;
const FAIL_LIMIT: u32 = 8;
const CAM_COCKPIT: i32 = 2;
const CAM_EXTERNAL: i32 = 3;
const LOOK_RELEASE_MS: u128 = 400;
const LOOK_CENTERED_HOLD_MS: u128 = 300;

// Kierunek osi w zdarzeniach AXIS_*: ujemny aileron = w prawo, ujemny elevator = nos w górę.
const AILERON_SIGN: f64 = -1.0;
const ELEVATOR_SIGN: f64 = -1.0;
const RUDDER_SIGN: f64 = 1.0;

type LogFn = Arc<dyn Fn(String) + Send + Sync>;

struct St {
    last: Option<MsfsData>,
    last_at: Option<Instant>,
    paused: bool,
    latch: TouchdownLatch,
    touchdown: Option<TouchdownData>,
    connected: bool,
    status: String,
    sim_name: String,
}

struct Look {
    mouse: MouseLook,
    yaw: f64,
    pitch: f64,
    last_cmd: Instant,
    last_off_center: Instant,
    refused: bool,
    pump: bool,
}

struct Inner {
    axes: Arc<AxisController>,
    conn: Arc<Mutex<Option<Conn>>>,
    nav: Arc<MsfsNavData>,
    st: Mutex<St>,
    fails: AtomicU32,
    running: AtomicBool,
    look: Mutex<Look>,
    events: HashMap<&'static str, u32>,
    log: LogFn,
}

pub struct MsfsSource {
    inner: Arc<Inner>,
}

fn to_axis(v: f64) -> u32 {
    ((v.clamp(-1.0, 1.0) * AXIS_RANGE).round() as i32) as u32
}

/// Dźwignia/pedał 0…1 na pełny zakres osi: −16383 = schowane / puszczone, 16383 = do końca.
fn to_lever(v: f64) -> u32 {
    (((v.clamp(0.0, 1.0) * 2.0 - 1.0) * AXIS_RANGE).round() as i32) as u32
}

impl MsfsSource {
    pub fn new(axes: Arc<AxisController>, log: LogFn) -> Self {
        let mut events = HashMap::new();
        let mut id = FIRST_EVENT;
        for s in commands::ALL.iter().filter(|s| s.target == Target::Event) {
            events.insert(s.name, id);
            id += 1;
        }
        for a in commands::AXIS_EVENTS.iter().chain(["VIEW_MODE"].iter()) {
            events.insert(*a, id);
            id += 1;
        }
        let conn = Arc::new(Mutex::new(None));
        Self {
            inner: Arc::new(Inner {
                axes,
                nav: Arc::new(MsfsNavData::new(conn.clone())),
                conn,
                st: Mutex::new(St {
                    last: None,
                    last_at: None,
                    paused: false,
                    latch: TouchdownLatch::new(),
                    touchdown: None,
                    connected: false,
                    status: tr("Czekam na MSFS").into(),
                    sim_name: String::new(),
                }),
                fails: AtomicU32::new(0),
                running: AtomicBool::new(false),
                look: Mutex::new(Look { mouse: MouseLook::default(), yaw: 0.0, pitch: 0.0, last_cmd: Instant::now(), last_off_center: Instant::now(), refused: false, pump: false }),
                events,
                log,
            }),
        }
    }

    /// Czułość rozglądania: zliczenia myszy na stopień obrotu kamery.
    pub fn set_look_counts_per_degree(&self, v: f64) {
        self.inner.look.lock().mouse.counts_per_degree = v.clamp(1.0, 60.0);
    }

    /// Czy biblioteka SimConnect daje się wczytać (do diagnostyki w programie).
    pub fn library_status() -> Result<(), String> {
        ffi::api().map(|_| ())
    }
}

impl Inner {
    fn set_status(&self, connected: bool, text: String) {
        let mut s = self.st.lock();
        s.connected = connected;
        s.status = text;
    }

    fn connect(&self) -> Result<(), String> {
        let c = Conn::open("OpenTakeoff")?;
        for (name, unit, kind) in data::VARS {
            let k = match kind {
                data::Kind::F => ffi::DATATYPE_FLOAT64,
                data::Kind::S256 => ffi::DATATYPE_STRING256,
                data::Kind::S64 => ffi::DATATYPE_STRING64,
            };
            let unit = if unit.is_empty() { None } else { Some(*unit) };
            let hr = c.add_data(DEF_STATE, name, unit, k);
            if hr < 0 {
                (self.log)(format!("AddToDataDefinition({name}) = 0x{hr:08X}"));
            }
        }
        c.add_data(DEF_XPDR, "TRANSPONDER STATE:1", Some("Enum"), ffi::DATATYPE_FLOAT64);
        c.add_data(DEF_CAMERA, "CAMERA STATE", Some("Enum"), ffi::DATATYPE_FLOAT64);
        for (name, id) in &self.events {
            let hr = c.map_event(*id, name);
            if hr < 0 {
                (self.log)(trf!("Nie udało się przypisać zdarzenia {name} (0x{code})", code = format!("{hr:08X}")));
            }
        }
        c.subscribe(PAUSE_EVENT, "Pause");
        let hr = c.request_data(REQ_STATE, DEF_STATE, ffi::PERIOD_VISUAL_FRAME);
        if hr < 0 {
            return Err(format!("RequestDataOnSimObject 0x{hr:08X}"));
        }
        if let Err(e) = MsfsNavData::define(&c) {
            (self.log)(trf!("Baza nawigacyjna MSFS niedostępna: {e}"));
        }
        self.fails.store(0, Ordering::SeqCst);
        *self.conn.lock() = Some(c);
        {
            let mut s = self.st.lock();
            s.last = None;
            s.last_at = None;
            s.connected = true;
            s.status = tr("Połączono z MSFS – czekam na dane samolotu").into();
        }
        Ok(())
    }

    fn disconnect(&self) {
        self.stop_look();
        *self.conn.lock() = None;
        self.nav.detach();
        let mut s = self.st.lock();
        s.last = None;
        s.last_at = None;
        if s.connected {
            s.connected = false;
            s.status = tr("Czekam na MSFS").into();
        }
    }

    fn transmit(&self, name: &str, data: u32) -> Result<(), String> {
        let Some(&id) = self.events.get(name) else { return Err(format!("Nieznane zdarzenie {name}")) };
        let g = self.conn.lock();
        let Some(c) = g.as_ref() else { return Err(tr("Brak połączenia z MSFS").into()) };
        let hr = c.transmit(id, data);
        if hr < 0 {
            self.fails.fetch_add(1, Ordering::SeqCst);
            return Err(format!("SimConnect 0x{hr:08X}"));
        }
        self.fails.store(0, Ordering::SeqCst);
        Ok(())
    }

    fn set_var(&self, def: u32, v: f64) -> Result<(), String> {
        let g = self.conn.lock();
        let Some(c) = g.as_ref() else { return Err(tr("Brak połączenia z MSFS").into()) };
        let hr = c.set_f64(def, v);
        if hr < 0 {
            self.fails.fetch_add(1, Ordering::SeqCst);
            return Err(format!("SimConnect 0x{hr:08X}"));
        }
        Ok(())
    }

    fn handle(&self, m: &ffi::Msg) {
        match m.id {
            ffi::RECV_OPEN => {
                let major = m.u32(12 + 256);
                let name = if major >= 12 { "MSFS 2024" } else { "MSFS 2020" };
                self.st.lock().sim_name = name.into();
                (self.log)(trf!("Połączono z {name} przez SimConnect"));
            }
            ffi::RECV_QUIT => {
                (self.log)(tr("MSFS został zamknięty").into());
                self.fails.store(FAIL_LIMIT, Ordering::SeqCst);
            }
            ffi::RECV_EVENT => {
                if m.u32(16) == PAUSE_EVENT {
                    self.st.lock().paused = m.u32(20) != 0;
                }
            }
            ffi::RECV_SIMOBJECT_DATA if m.data.len() >= 40 => {
                if m.u32(12) != REQ_STATE || m.data.len() < 40 + data::block_size() {
                    return;
                }
                let d = MsfsData::parse(&m.data[40..]);
                let sample = Sample {
                    time_ms: tick_ms(),
                    unix_ms: unix_ms(),
                    on_ground: d.on_ground != 0.0,
                    vs_fpm: d.vs,
                    g_force: d.g_force,
                    pitch_deg: -d.pitch,
                    bank_deg: -d.bank,
                    heading_true_deg: d.hdg_true,
                    lat: d.lat,
                    lon: d.lon,
                    ias_kt: d.ias,
                    gs_kt: d.gs,
                    td_vs_fpm: Some(d.td_vs),
                    td_pitch_deg: Some(-d.td_pitch),
                    td_bank_deg: Some(-d.td_bank),
                    td_lat: Some(d.td_lat),
                    td_lon: Some(d.td_lon),
                };
                let mut s = self.st.lock();
                if s.last.is_none() {
                    s.status = trf!("Połączono z {sim}", sim = if s.sim_name.is_empty() { "MSFS".to_string() } else { s.sim_name.clone() });
                }
                s.touchdown = s.latch.update(&sample);
                s.last = Some(d);
                s.last_at = Some(Instant::now());
            }
            _ => self.nav.on_message(m),
        }
    }

    fn axes_tick(&self) {
        let now = tick_ms();
        let out = self.axes.tick(now);
        let aux = self.axes.tick_aux(now);
        if out.is_none() && aux.is_none() {
            return;
        }
        let mut ok = true;
        if let Some(o) = out {
            ok &= self.transmit("AXIS_AILERONS_SET", to_axis(o.aileron * AILERON_SIGN)).is_ok();
            ok &= self.transmit("AXIS_ELEVATOR_SET", to_axis(o.elevator * ELEVATOR_SIGN)).is_ok();
            ok &= self.transmit("AXIS_RUDDER_SET", to_axis(o.rudder * RUDDER_SIGN)).is_ok();
        }
        if let Some(a) = aux {
            if let Some(sb) = a.speedbrake {
                ok &= self.transmit("AXIS_SPOILER_SET", to_lever(sb)).is_ok();
            }
            if let Some(br) = a.brake {
                // Hamulce kół tylko na ziemi – w powietrzu zawsze puszczone
                let on_ground = self.st.lock().last.as_ref().is_some_and(|d| d.on_ground != 0.0);
                let v = to_lever(if on_ground { br } else { 0.0 });
                ok &= self.transmit("AXIS_LEFT_BRAKE_SET", v).is_ok();
                ok &= self.transmit("AXIS_RIGHT_BRAKE_SET", v).is_ok();
            }
        }
        if !ok {
            (self.log)(tr("Błąd wysyłania osi do MSFS").into());
        }
    }

    fn run(self: &Arc<Self>) {
        let mut last_try: Option<Instant> = None;
        let mut last_error = String::new();
        let mut last_axes = Instant::now();
        while self.running.load(Ordering::SeqCst) {
            if self.conn.lock().is_none() {
                if last_try.is_none_or(|t| t.elapsed() > Duration::from_secs(5)) {
                    last_try = Some(Instant::now());
                    match self.connect() {
                        Ok(()) => last_error.clear(),
                        Err(e) => {
                            let text = match ffi::api() {
                                Err(e) => trf!("Nie można wczytać SimConnect.dll: {e}"),
                                Ok(_) => tr("Czekam na MSFS").into(),
                            };
                            self.set_status(false, text);
                            if e != last_error {
                                (self.log)(trf!("MSFS niedostępny: {e}"));
                                last_error = e;
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            // Zbierz wiadomości (krótko trzymając połączenie), potem obsłuż bez blokady.
            let msgs: Vec<ffi::Msg> = {
                let g = self.conn.lock();
                match g.as_ref() {
                    Some(c) => std::iter::from_fn(|| c.next()).take(256).collect(),
                    None => Vec::new(),
                }
            };
            let idle = msgs.is_empty();
            for m in &msgs {
                self.handle(m);
            }
            if last_axes.elapsed() >= Duration::from_millis(50) {
                last_axes = Instant::now();
                self.axes_tick();
                // Cisza po połączeniu (menu MSFS, ładowanie lotu): dane przestają być aktualne
                let mut s = self.st.lock();
                if s.last_at.is_some_and(|t| t.elapsed() > Duration::from_secs(5)) {
                    s.last = None;
                    s.last_at = None;
                    s.status = tr("Połączono z MSFS – czekam na lot").into();
                }
            }
            if self.fails.load(Ordering::SeqCst) >= FAIL_LIMIT {
                (self.log)(tr("Połączenie z MSFS przestało odpowiadać – łączę ponownie").into());
                self.disconnect();
                last_try = None;
                continue;
            }
            if idle {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        self.disconnect();
    }

    fn camera_state(&self) -> i32 {
        self.st.lock().last.as_ref().map(|d| d.camera_state as i32).unwrap_or(-1)
    }

    fn wait_camera(&self, target: i32, attempts: u32) -> bool {
        for _ in 0..attempts {
            std::thread::sleep(Duration::from_millis(40));
            if self.camera_state() == target {
                return true;
            }
        }
        false
    }

    fn try_write_camera(&self, target: i32, attempts: u32) -> bool {
        match self.set_var(DEF_CAMERA, target as f64) {
            Ok(()) => self.wait_camera(target, attempts),
            Err(e) => {
                (self.log)(trf!("Zapis CAMERA STATE nie powiódł się: {e}"));
                false
            }
        }
    }

    fn set_camera(&self, target: i32) -> CommandResult {
        if self.look.lock().mouse.active() {
            {
                let mut l = self.look.lock();
                l.yaw = 0.0;
                l.pitch = 0.0;
                l.last_off_center = Instant::now() - Duration::from_secs(10);
            }
            for _ in 0..100 {
                if !self.look.lock().mouse.active() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        let raw = self.camera_state();
        if raw == target {
            return CommandResult::success();
        }
        if self.try_write_camera(target, 20) {
            return CommandResult::success();
        }
        let raw = self.camera_state();
        let ok = if raw == CAM_COCKPIT || raw == CAM_EXTERNAL {
            self.transmit("VIEW_MODE", 0).is_ok() && self.wait_camera(target, 20)
        } else {
            target == CAM_COCKPIT && self.try_write_camera(CAM_EXTERNAL, 40) && self.try_write_camera(CAM_COCKPIT, 40)
        };
        if ok {
            CommandResult::success()
        } else {
            CommandResult::fail(codes::SIM_ERROR, if target == CAM_COCKPIT { tr("MSFS nie przełączył na widok z kabiny") } else { tr("MSFS nie przełączył na widok zewnętrzny") })
        }
    }

    fn stop_look(&self) {
        let mut l = self.look.lock();
        l.mouse.release();
        l.yaw = 0.0;
        l.pitch = 0.0;
        l.refused = false;
    }

    fn look(self: &Arc<Self>, pitch_axis: bool, deg: i64) -> CommandResult {
        let mut l = self.look.lock();
        if pitch_axis {
            l.pitch = deg as f64;
        } else {
            l.yaw = deg as f64;
        }
        l.last_cmd = Instant::now();
        let centered = l.yaw == 0.0 && l.pitch == 0.0;
        if !centered {
            l.last_off_center = Instant::now();
        }
        if !l.mouse.active() {
            if centered {
                l.refused = false;
                return CommandResult::success();
            }
            if l.refused {
                return CommandResult::success();
            }
            if let Err(why) = l.mouse.begin() {
                l.refused = true;
                (self.log)(trf!("Rozglądanie odrzucone: {why}"));
                return CommandResult::fail(codes::NOT_ALLOWED, why);
            }
            if !l.pump {
                l.pump = true;
                let me = self.clone();
                let _ = std::thread::Builder::new().name("msfs-look".into()).spawn(move || me.look_pump());
            }
        }
        let (y, p) = (l.yaw, l.pitch);
        if l.mouse.step(y, p) {
            CommandResult::success()
        } else {
            l.refused = true;
            CommandResult::fail(codes::NOT_ALLOWED, tr("Okno MSFS przestało być aktywne — rozglądanie przerwane"))
        }
    }

    fn look_pump(&self) {
        loop {
            std::thread::sleep(Duration::from_millis(15));
            let mut l = self.look.lock();
            if !l.mouse.active() {
                l.pump = false;
                return;
            }
            if l.last_cmd.elapsed().as_millis() > LOOK_RELEASE_MS {
                l.yaw = 0.0;
                l.pitch = 0.0;
            }
            let (y, p) = (l.yaw, l.pitch);
            if !l.mouse.step(y, p) {
                l.refused = true;
                l.pump = false;
                return;
            }
            if y == 0.0 && p == 0.0 && l.mouse.reached(0.0, 0.0) && l.last_off_center.elapsed().as_millis() > LOOK_CENTERED_HOLD_MS {
                l.mouse.release();
                l.refused = false;
                l.pump = false;
                return;
            }
        }
    }
}

impl FlightSource for MsfsSource {
    fn kind(&self) -> &'static str {
        "msfs"
    }

    fn is_connected(&self) -> bool {
        let s = self.inner.st.lock();
        s.connected && s.last.is_some()
    }

    fn status_text(&self) -> String {
        self.inner.st.lock().status.clone()
    }

    fn snapshot(&self) -> Option<FlightState> {
        let (d, paused, td) = {
            let s = self.inner.st.lock();
            (s.last.clone()?, s.paused, s.touchdown.clone())
        };
        let mut state = data::map(&d, paused, self.inner.axes.is_active());
        state.touchdown = td.map(|mut t| {
            t.lat = r(t.lat, 6);
            t.lon = r(t.lon, 6);
            t
        });
        Some(state)
    }

    fn execute(&self, cmd: &Validated) -> CommandResult {
        if self.inner.conn.lock().is_none() {
            return CommandResult::fail(codes::SIM_UNAVAILABLE, tr("Brak połączenia z MSFS"));
        }
        let res = match cmd.spec.target {
            Target::CameraPitch => return self.inner.look(true, cmd.value),
            Target::CameraYaw => return self.inner.look(false, cmd.value),
            Target::CameraState => return self.inner.set_camera(cmd.value as i32),
            Target::TransponderState => self.inner.set_var(DEF_XPDR, cmd.value as f64),
            Target::Event => self.inner.transmit(cmd.spec.name, cmd.data),
        };
        match res {
            Ok(()) => CommandResult::success(),
            Err(e) => CommandResult::fail(codes::SIM_ERROR, e),
        }
    }

    fn axes(&self) -> Arc<AxisController> {
        self.inner.axes.clone()
    }

    fn navdata(&self) -> Option<Arc<dyn NavDataProvider>> {
        Some(self.inner.nav.clone())
    }

    fn start(&self) {
        if self.inner.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.inner.clone();
        let _ = std::thread::Builder::new().name("msfs".into()).spawn(move || me.run());
    }

    fn stop(&self) {
        self.inner.stop_look();
        self.inner.running.store(false, Ordering::SeqCst);
    }

    fn sim_name(&self) -> String {
        let s = self.inner.st.lock();
        if s.sim_name.is_empty() {
            "MSFS".into()
        } else {
            s.sim_name.clone()
        }
    }
}

impl Drop for MsfsSource {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn simconnect_library_loads() {
        let api = super::ffi::api().expect("SimConnect.dll");
        let _ = api;
        // Bez działającego MSFS otwarcie kończy się błędem, ale nie awarią.
        let _ = super::ffi::Conn::open("OpenTakeoff test");
    }
}
