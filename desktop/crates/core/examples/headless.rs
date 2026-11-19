//! OpenTakeoff bez okna: serwer dla telefonów, lot pokazowy (tryb symulacji) i dziennik do synchronizacji.
//! Do zrzutów ekranu aplikacji na Androida w CI (Linux, emulator łączy się pod 10.0.2.2) – telefon widzi
//! go tak samo jak program na Windows w trybie symulacji.
//!
//! `cargo run --release -p opentakeoff-core --example headless -- <folder danych>`
//! W folderze: `devices.json` (sparowane telefony), `logbook/` (dziennik), `code.txt` (kod parowania, zapisywany
//! po starcie). Program działa, dopóki się go nie zatrzyma.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use opentakeoff_core::axes::AxisController;
use opentakeoff_core::discovery::{local_addresses, Discovery};
use opentakeoff_core::logbook::LogbookStore;
use opentakeoff_core::navdata::NavDataService;
use opentakeoff_core::pairing::Pairing;
use opentakeoff_core::proto::{self, WEBSOCKET_PORT};
use opentakeoff_core::server::LinkServer;
use opentakeoff_core::simulation::SimulationSource;
use opentakeoff_core::source::FlightSource;

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "headless-data".into()));
    std::fs::create_dir_all(dir.join("logbook")).expect("folder danych");
    let pairing = Arc::new(Pairing::new(Some(dir.join("devices.json"))));
    let axes = Arc::new(AxisController::default());
    let nav = Arc::new(NavDataService::new());
    let logbook = LogbookStore::open(dir.join("logbook"));
    let version = opentakeoff_core::version();
    let server = LinkServer::new(pairing.clone(), axes.clone(), nav, version, Arc::new(|s: String| println!("[link] {s}")));
    server.set_sync(logbook.clone());
    let src: Arc<dyn FlightSource> = Arc::new(SimulationSource::new(axes));
    src.start();
    server.set_source(Some(src.clone()));
    let port = server.start(WEBSOCKET_PORT).expect("port serwera");
    let _discovery = Discovery::start(move || proto::announce(version, "CI", port, &local_addresses(), "simulation"));
    let code = pairing.code();
    std::fs::write(dir.join("code.txt"), &code).expect("code.txt");
    println!("OpenTakeoff {version} bez okna: port {port}, kod {code}, lotów w dzienniku {}", logbook.len());
    let mut last = String::new();
    loop {
        std::thread::sleep(Duration::from_millis(500));
        // Zmiana stanu źródła → `status` do telefonów (jak pętla programu na Windows)
        let key = format!("{}|{}", src.is_connected(), src.status_text());
        if key != last {
            println!("[sim] {}", src.status_text());
            last = key;
            server.broadcast_status();
        }
    }
}
