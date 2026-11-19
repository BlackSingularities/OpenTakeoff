//! Składa wszystko razem: serwer dla telefonów, rozgłaszanie, źródło danych (MSFS albo symulacja),
//! zapis lotów na komputerze i dziennik z synchronizacją. Działa w tle niezależnie od okna.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use opentakeoff_core::aero::airports;
use opentakeoff_core::aero::weather::WeatherClient;
use opentakeoff_core::axes::AxisController;
use opentakeoff_core::discovery::{local_addresses, Discovery};
use opentakeoff_core::lang::tr;
use opentakeoff_core::logbook::recorder::Recorder;
use opentakeoff_core::logbook::{FlightLog, LogbookStore};
use opentakeoff_core::navdata::NavDataService;
use opentakeoff_core::pairing::Pairing;
use opentakeoff_core::proto::{self, FlightState, WEBSOCKET_PORT};
use opentakeoff_core::server::LinkServer;
use opentakeoff_core::simulation::SimulationSource;
use opentakeoff_core::source::{unix_ms, FlightSource};
use opentakeoff_core::xplane::{AutoSource, XPlaneSource};
use opentakeoff_core::{log, paths, trf};
use opentakeoff_msfs::MsfsSource;
use parking_lot::{Mutex, RwLock};

/// Zdarzenia dla okna (komunikaty).
#[derive(Clone, Debug)]
pub enum HostEvent {
    FlightSaved(String, String),
    FlightDiscarded,
}

/// Punkt śladu na mapie „Na żywo” (także bez nagrywania).
#[derive(Clone, Copy, Debug)]
pub struct LivePoint {
    pub lat: f64,
    pub lon: f64,
    pub alt: i32,
}

pub struct Host {
    pub pairing: Arc<Pairing>,
    pub axes: Arc<AxisController>,
    pub server: LinkServer,
    pub nav: Arc<NavDataService>,
    pub logbook: Arc<LogbookStore>,
    pub weather: Arc<WeatherClient>,
    source: RwLock<Arc<dyn FlightSource>>,
    msfs: Mutex<Option<Arc<MsfsSource>>>,
    pub simulation: AtomicBool,
    /// Symulator: 0 automatycznie (MSFS albo X-Plane), 1 tylko MSFS, 2 tylko X-Plane.
    pub sim_kind: AtomicI64,
    xplane_host: Mutex<String>,
    _discovery: Mutex<Option<Discovery>>,
    recorder: Mutex<Recorder>,
    pub record_enabled: AtomicBool,
    pub link_ok: AtomicBool,
    look: AtomicI64,
    events: Mutex<Vec<HostEvent>>,
    live: Mutex<Vec<LivePoint>>,
    last: Mutex<Option<FlightState>>,
    status_key: Mutex<String>,
}

fn version() -> &'static str {
    opentakeoff_core::version()
}

fn log_line(s: String) {
    log::info(&s);
}

impl Host {
    pub fn start(simulation: bool, sim_kind: i64, xplane_host: &str, look_sensitivity: i32, record: bool, interval_ms: i64) -> Arc<Host> {
        let pairing = Arc::new(Pairing::new(Some(paths::config().join("devices.json"))));
        let axes = Arc::new(AxisController::default());
        let nav = Arc::new(NavDataService::new());
        let logbook = LogbookStore::open(paths::logbook());
        let server = LinkServer::new(pairing.clone(), axes.clone(), nav.clone(), version(), Arc::new(log_line));
        server.set_sync(logbook.clone());
        server.set_web(crate::web::pages());
        let mut rec = Recorder::new(
            Box::new(|lat, lon| airports::db().map(|d| d.runways_near(lat, lon)).unwrap_or_default()),
            Box::new(|lat, lon| airports::db().and_then(|d| d.nearest(lat, lon).map(|a| a.ident.clone()))),
        );
        rec.interval_ms = interval_ms;
        let placeholder: Arc<dyn FlightSource> = Arc::new(SimulationSource::new(axes.clone()));
        let host = Arc::new(Host {
            pairing,
            axes,
            server,
            nav,
            logbook,
            weather: Arc::new(WeatherClient::new()),
            source: RwLock::new(placeholder),
            msfs: Mutex::new(None),
            simulation: AtomicBool::new(simulation),
            sim_kind: AtomicI64::new(sim_kind.clamp(0, 2)),
            xplane_host: Mutex::new(xplane_host.to_string()),
            _discovery: Mutex::new(None),
            recorder: Mutex::new(rec),
            record_enabled: AtomicBool::new(record),
            link_ok: AtomicBool::new(true),
            look: AtomicI64::new(look_sensitivity as i64),
            events: Mutex::new(Vec::new()),
            live: Mutex::new(Vec::new()),
            last: Mutex::new(None),
            status_key: Mutex::new(String::new()),
        });
        // Lot przerwany zamknięciem programu – szkic trafia do dziennika
        if let Some(draft) = host.logbook.take_draft() {
            if draft.track.len() > 1 {
                log::info("Odzyskano lot przerwany zamknięciem programu");
                host.logbook.put(draft, "pc");
            }
        }
        match host.server.start(WEBSOCKET_PORT) {
            Ok(_) => {}
            Err(e) => {
                host.link_ok.store(false, Ordering::SeqCst);
                log::error(&trf!("Port {port} zajęty – telefony nie połączą się: {e}", port = WEBSOCKET_PORT));
            }
        }
        let weak = Arc::downgrade(&host);
        *host._discovery.lock() = Some(Discovery::start(move || {
            let (src, port) = match weak.upgrade() {
                Some(h) => (h.server.source_name(), h.server.port()),
                None => ("none", WEBSOCKET_PORT),
            };
            // Port 0 = serwer nie wystartował (port zajęty): nie zapraszaj telefonów pod martwy adres
            if port == 0 {
                return None;
            }
            let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into());
            Some(proto::announce(version(), &host, port, &local_addresses(), src))
        }));
        host.switch_source(simulation);
        log::info(&trf!("Rozgłaszanie na UDP {port}", port = proto::DISCOVERY_PORT));
        let h = host.clone();
        let _ = std::thread::Builder::new().name("host".into()).spawn(move || h.run());
        host
    }

    pub fn source(&self) -> Arc<dyn FlightSource> {
        self.source.read().clone()
    }

    /// MSFS albo tryb symulacji.
    pub fn switch_source(&self, simulation: bool) {
        let old = self.source.read().clone();
        old.stop();
        self.simulation.store(simulation, Ordering::SeqCst);
        let kind = self.sim_kind.load(Ordering::SeqCst);
        let msfs = || {
            let m = Arc::new(MsfsSource::new(self.axes.clone(), Arc::new(log_line)));
            m.set_look_counts_per_degree(8.0 * self.look.load(Ordering::SeqCst) as f64 / 100.0);
            *self.msfs.lock() = Some(m.clone());
            m
        };
        let src: Arc<dyn FlightSource> = if simulation {
            *self.msfs.lock() = None;
            Arc::new(SimulationSource::new(self.axes.clone()))
        } else if kind == 2 {
            *self.msfs.lock() = None;
            Arc::new(XPlaneSource::new(self.axes.clone(), &self.xplane_host.lock()))
        } else if kind == 1 {
            msfs()
        } else {
            let m: Arc<dyn FlightSource> = msfs();
            let x = Arc::new(XPlaneSource::new(self.axes.clone(), &self.xplane_host.lock()));
            Arc::new(AutoSource::new(m, x))
        };
        src.start();
        *self.source.write() = src.clone();
        self.server.set_source(Some(src));
        self.nav.clear();
        self.live.lock().clear();
        log::info(if simulation {
            tr("Tryb symulacji włączony")
        } else {
            match kind {
                1 => tr("Źródło danych: MSFS"),
                2 => tr("Źródło danych: X-Plane"),
                _ => tr("Źródło danych: MSFS albo X-Plane (automatycznie)"),
            }
        });
    }

    /// Wybór symulatora (0 auto, 1 MSFS, 2 X-Plane) i adres komputera z X-Plane; przełącza źródło.
    pub fn set_sim(&self, kind: i64, xplane_host: &str) {
        self.sim_kind.store(kind.clamp(0, 2), Ordering::SeqCst);
        *self.xplane_host.lock() = xplane_host.to_string();
        if !self.simulation.load(Ordering::SeqCst) {
            self.switch_source(false);
        }
    }

    pub fn set_look_sensitivity(&self, percent: i32) {
        self.look.store(percent as i64, Ordering::SeqCst);
        if let Some(m) = self.msfs.lock().as_ref() {
            m.set_look_counts_per_degree(8.0 * percent as f64 / 100.0);
        }
    }

    pub fn set_interval(&self, ms: i64) {
        self.recorder.lock().interval_ms = ms;
    }

    pub fn take_events(&self) -> Vec<HostEvent> {
        std::mem::take(&mut *self.events.lock())
    }

    pub fn snapshot(&self) -> Option<FlightState> {
        self.last.lock().clone()
    }

    pub fn live_track(&self) -> Vec<LivePoint> {
        self.live.lock().clone()
    }

    pub fn clear_live(&self) {
        self.live.lock().clear();
    }

    /// (nagrywa, od kiedy ms, wstrzymany po ręcznym zakończeniu, paliwo na starcie, ostatnie lądowanie)
    pub fn recording(&self) -> (bool, Option<i64>, bool, Option<f64>, Option<opentakeoff_core::logbook::LandingReport>) {
        let r = self.recorder.lock();
        (r.recording, r.started_at(), r.hold_until_ground, r.start_fuel_kg, r.last_landing.clone())
    }

    /// Id lotu, który właśnie się nagrywa.
    pub fn recording_id(&self) -> Option<String> {
        self.recorder.lock().recording_id().map(str::to_string)
    }

    pub fn current_track(&self) -> Vec<opentakeoff_core::logbook::TrackPoint> {
        self.recorder.lock().current_track().to_vec()
    }

    pub fn mark(&self) -> bool {
        let mut r = self.recorder.lock();
        let label = match r.started_at() {
            Some(s) => {
                let sec = (unix_ms() - s) / 1000;
                format!("{} {}:{:02}", tr("Znacznik"), sec / 60, sec % 60)
            }
            None => tr("Znacznik").to_string(),
        };
        r.mark(&label, unix_ms())
    }

    pub fn stop_recording(&self) {
        let log = self.recorder.lock().stop_by_user(unix_ms());
        self.finished(log);
    }

    pub fn resume_recording(&self) {
        self.recorder.lock().resume();
    }

    fn finished(&self, log: Option<FlightLog>) {
        match log {
            Some(f) => {
                let route = format!("{} → {}", f.from.as_deref().unwrap_or("?"), f.to.as_deref().unwrap_or("?"));
                let id = f.id.clone();
                self.logbook.put(f, "pc");
                self.logbook.clear_draft();
                log::info(&trf!("Zapisano lot {route}"));
                self.events.lock().push(HostEvent::FlightSaved(id, route));
            }
            None => {
                self.logbook.clear_draft();
                self.events.lock().push(HostEvent::FlightDiscarded);
            }
        }
    }

    fn run(self: Arc<Self>) {
        let mut last_draft = Instant::now();
        let mut offline_since: Option<Instant> = None;
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let src = self.source();
            // Zmiana stanu źródła → `status` do telefonów
            let key = format!("{}|{}|{}", src.kind(), src.is_connected(), src.status_text());
            {
                let mut k = self.status_key.lock();
                if *k != key {
                    *k = key;
                    self.server.broadcast_status();
                }
            }
            let snap = if src.is_connected() { src.snapshot() } else { None };
            *self.last.lock() = snap.clone();
            let now = unix_ms();
            match &snap {
                Some(s) => {
                    offline_since = None;
                    if s.position.lat != 0.0 || s.position.lon != 0.0 {
                        let mut l = self.live.lock();
                        let moved = l.last().is_none_or(|p| (p.lat - s.position.lat).abs() > 0.0005 || (p.lon - s.position.lon).abs() > 0.0005 || (p.alt - s.position.alt_msl_ft as i32).abs() > 100);
                        // Teleport (nowy lot w symulatorze) – nowy ślad
                        if l.last().is_some_and(|p| opentakeoff_core::navdata::math::distance_nm(p.lat, p.lon, s.position.lat, s.position.lon) > 50.0) {
                            l.clear();
                        }
                        if moved && !s.aircraft.on_ground || l.is_empty() {
                            l.push(LivePoint { lat: s.position.lat, lon: s.position.lon, alt: s.position.alt_msl_ft as i32 });
                            if l.len() > 30_000 {
                                l.drain(..5_000);
                            }
                        }
                    }
                    if self.record_enabled.load(Ordering::SeqCst) {
                        let done = self.recorder.lock().on_state(s, now);
                        if done.is_some() {
                            self.finished(done);
                        }
                        if last_draft.elapsed() > Duration::from_secs(30) {
                            last_draft = Instant::now();
                            if let Some(d) = self.recorder.lock().snapshot(now) {
                                self.logbook.save_draft(&d);
                            }
                        }
                    }
                }
                None => {
                    // Symulator zamknięty w trakcie lotu: po minucie bez danych lot trafia do dziennika
                    let since = *offline_since.get_or_insert_with(Instant::now);
                    if since.elapsed() > Duration::from_secs(60) && self.recorder.lock().recording {
                        let log = self.recorder.lock().finish(now);
                        self.finished(log);
                    }
                }
            }
        }
    }

    pub fn shutdown(&self) {
        let now = unix_ms();
        if let Some(d) = self.recorder.lock().snapshot(now) {
            self.logbook.save_draft(&d);
        }
        self.source().stop();
        self.server.stop();
        self.logbook.save_now();
    }

    pub fn addresses(&self) -> Vec<String> {
        local_addresses()
    }

    /// Synchronizacja dziennika z telefonami włączona albo wyłączona (ustawienie).
    pub fn set_sync_enabled(&self, on: bool) {
        self.server.set_sync(Arc::new(SyncGate { store: self.logbook.clone(), on }));
    }
}

/// Synchronizacja z możliwością wyłączenia: wyłączona odpowiada pustym planem (telefon nic nie wysyła).
struct SyncGate {
    store: Arc<LogbookStore>,
    on: bool,
}

impl opentakeoff_core::logbook::sync::SyncHandler for SyncGate {
    // Wywołania magazynu przez cechę SyncHandler (plan/upload/download/list)
    fn plan(&self, req: &opentakeoff_core::logbook::sync::SyncRequest, device: &str) -> opentakeoff_core::logbook::sync::SyncResponse {
        if self.on {
            self.store.plan(req, device)
        } else {
            opentakeoff_core::logbook::sync::SyncResponse { v: 1, flights: self.store.len(), ..Default::default() }
        }
    }
    fn upload(&self, bytes: &[u8], device: &str) -> Result<opentakeoff_core::logbook::ImportResult, String> {
        if !self.on {
            return Err(tr("Synchronizacja wyłączona w programie na komputerze").into());
        }
        self.store.upload(bytes, device)
    }
    fn download(&self, ids: &[String]) -> Vec<u8> {
        self.store.download(if self.on { ids } else { &[] })
    }
    fn list(&self) -> Vec<opentakeoff_core::logbook::sync::SyncFlight> {
        self.store.list()
    }
}

