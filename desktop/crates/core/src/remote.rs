//! Drugi komputer: OpenTakeoff łączy się z OpenTakeoff na komputerze z symulatorem tak jak telefon
//! (protokół Link v1 – kod parowania, potem zapamiętany token). Stan lotu przychodzi strumieniem `state`,
//! komendy, osie i zapytania navdata idą dalej do tamtego komputera. Telefony podłączone do tego
//! komputera widzą wszystko tak, jakby symulator działał tutaj.
//!
//! Dziennik jest jeden – na komputerze z symulatorem. Ten komputer nie nagrywa lotów sam, tylko
//! synchronizuje swój dziennik z tamtym (`sync_logbook`, to samo HTTP co telefon), więc loty, usunięcia
//! i loty z telefonów podłączonych tutaj schodzą się w jednym miejscu.

use std::collections::HashMap;
use std::io::{ErrorKind, Read};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use parking_lot::Mutex;
use serde_json::{json, Value};
use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

use crate::axes::AxisController;
use crate::commands::Validated;
use crate::lang::tr;
use crate::logbook::archive;
use crate::logbook::sync::{DownloadRequest, SyncRequest, SyncResponse};
use crate::logbook::LogbookStore;
use crate::navdata::{AirportInfo, AirportProcedures, FixResult, NavDataProvider, NavError, NavResult};
use crate::proto::{codes, FlightState, VERSION, WEBSOCKET_PORT};
use crate::source::{tick_ms, CommandResult, FlightSource};
use crate::trf;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY: Duration = Duration::from_secs(3);
const STALE: Duration = Duration::from_secs(3);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(6);
const NAVDATA_TIMEOUT: Duration = Duration::from_secs(12);

/// Adres „host” albo „host:port” (bez portu – 47810).
pub fn parse_address(text: &str) -> Option<(String, u16)> {
    let t = text.trim().trim_start_matches("ws://").trim_end_matches('/');
    if t.is_empty() {
        return None;
    }
    if let Some(rest) = t.strip_prefix('[') {
        // [IPv6]:port
        let (h, p) = rest.split_once(']')?;
        let port = match p.strip_prefix(':') {
            Some(p) => p.parse().ok()?,
            None => WEBSOCKET_PORT,
        };
        return Some((h.to_string(), port));
    }
    match t.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => Some((h.to_string(), p.trim().parse().ok().filter(|p| *p != 0)?)),
        _ => Some((t.to_string(), WEBSOCKET_PORT)),
    }
}

fn static_code(code: &str) -> &'static str {
    match code {
        codes::NOT_PAIRED => codes::NOT_PAIRED,
        codes::RATE_LIMITED => codes::RATE_LIMITED,
        codes::NOT_ALLOWED => codes::NOT_ALLOWED,
        codes::INVALID_VALUE => codes::INVALID_VALUE,
        codes::SIM_UNAVAILABLE => codes::SIM_UNAVAILABLE,
        codes::NOT_FOUND => codes::NOT_FOUND,
        _ => codes::SIM_ERROR,
    }
}

/// Wywoływane, gdy tamten komputer wyda token (zapis w ustawieniach) – pusty tekst: token nieważny.
pub type TokenFn = Arc<dyn Fn(&str) + Send + Sync>;

struct Shared {
    host: String,
    port: u16,
    client_name: String,
    code: Mutex<String>,
    token: Mutex<String>,
    on_token: TokenFn,
    stop: AtomicBool,
    running: AtomicBool,
    paired: AtomicBool,
    state: Mutex<Option<(FlightState, Instant)>>,
    /// Źródło po tamtej stronie („msfs”, „xplane”, „simulation”, „none”) i jego opis.
    remote_source: Mutex<String>,
    remote_text: Mutex<String>,
    remote_version: Mutex<String>,
    problem: Mutex<String>,
    out: Mutex<Option<Sender<String>>>,
    pending: Mutex<HashMap<String, Sender<Value>>>,
    next_id: AtomicU64,
}

impl Shared {
    fn request(&self, mut msg: Value, timeout: Duration) -> Option<Value> {
        if !self.paired.load(Ordering::SeqCst) {
            return None;
        }
        let id = format!("r{}", self.next_id.fetch_add(1, Ordering::SeqCst));
        msg["v"] = VERSION.into();
        msg["id"] = id.clone().into();
        let (tx, rx) = bounded(1);
        self.pending.lock().insert(id.clone(), tx);
        let sent = self.out.lock().as_ref().is_some_and(|o| o.send(msg.to_string()).is_ok());
        let res = if sent { rx.recv_timeout(timeout).ok() } else { None };
        self.pending.lock().remove(&id);
        res
    }

    fn send(&self, msg: Value) {
        if let Some(o) = self.out.lock().as_ref() {
            let _ = o.send(msg.to_string());
        }
    }
}

pub struct RemoteSource {
    sh: Arc<Shared>,
    axes: Arc<AxisController>,
}

impl RemoteSource {
    /// `address` – „host” albo „host:port”; `code` – kod parowania z tamtego komputera (potrzebny tylko
    /// za pierwszym razem); `token` – zapamiętany po poprzednim parowaniu.
    pub fn new(axes: Arc<AxisController>, address: &str, code: &str, token: &str, on_token: TokenFn) -> Self {
        let (host, port) = parse_address(address).unwrap_or_default();
        let pc = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into());
        Self {
            sh: Arc::new(Shared {
                host,
                port,
                client_name: format!("OpenTakeoff {pc}"),
                code: Mutex::new(code.trim().to_string()),
                token: Mutex::new(token.trim().to_string()),
                on_token,
                stop: AtomicBool::new(false),
                running: AtomicBool::new(false),
                paired: AtomicBool::new(false),
                state: Mutex::new(None),
                remote_source: Mutex::new("none".into()),
                remote_text: Mutex::new(String::new()),
                remote_version: Mutex::new(String::new()),
                problem: Mutex::new(String::new()),
                out: Mutex::new(None),
                pending: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(1),
            }),
            axes,
        }
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.sh.host, self.sh.port)
    }

    /// (adres, port, token), gdy tamten komputer przyjął parowanie – do synchronizacji dziennika.
    pub fn link(&self) -> Option<(String, u16, String)> {
        let token = self.sh.token.lock().clone();
        (self.sh.paired.load(Ordering::SeqCst) && !token.is_empty()).then(|| (self.sh.host.clone(), self.sh.port, token))
    }
}

/// Wynik synchronizacji dziennika z komputerem z symulatorem.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncOutcome {
    pub received: usize,
    pub sent: usize,
    pub removed: usize,
}

/// Synchronizacja dziennika z komputerem z symulatorem (jak telefon, docs/link-protocol.md p. 6):
/// loty usunięte tutaj od `since_ms` idą do kosza tam, brakujące loty w obie strony, a usunięte tam –
/// do kosza tutaj. `device` – nazwa tego komputera, `source` – opis tamtego (pochodzenie odebranych lotów).
pub fn sync_logbook(host: &str, port: u16, token: &str, store: &LogbookStore, device: &str, source: &str, since_ms: i64) -> Result<SyncOutcome, String> {
    let h = if host.contains(':') { format!("[{host}]") } else { host.to_string() };
    let base = format!("http://{h}:{port}/v1/logbook");
    let auth = format!("Bearer {token}");
    let post = |path: &str, content_type: &str, body: &[u8]| -> Result<Vec<u8>, String> {
        let resp = ureq::post(&format!("{base}/{path}"))
            .set("Authorization", &auth)
            .set("Content-Type", content_type)
            .timeout(Duration::from_secs(120))
            .send_bytes(body)
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        resp.into_reader().take(256 * 1024 * 1024).read_to_end(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    };
    let req = SyncRequest { v: 1, device: device.to_string(), flights: store.sync_list(), deleted: store.deleted_since(since_ms) };
    let body = serde_json::to_vec(&req).map_err(|e| e.to_string())?;
    let plan: SyncResponse = serde_json::from_slice(&post("sync", "application/json", &body)?).map_err(|e| e.to_string())?;
    let mut out = SyncOutcome::default();
    if !plan.remove.is_empty() {
        out.removed = store.delete(&plan.remove);
    }
    if !plan.want.is_empty() {
        post("upload", "application/octet-stream", &store.export(&plan.want))?;
        out.sent = plan.want.len();
    }
    if !plan.offer.is_empty() {
        let body = serde_json::to_vec(&DownloadRequest { ids: plan.offer.clone() }).map_err(|e| e.to_string())?;
        let flights = archive::decode(&post("download", "application/json", &body)?)?;
        out.received = store.import(flights, source).added;
    }
    Ok(out)
}

impl FlightSource for RemoteSource {
    fn kind(&self) -> &'static str {
        match self.sh.remote_source.lock().as_str() {
            "msfs" => "msfs",
            "xplane" => "xplane",
            "simulation" => "simulation",
            _ => "remote",
        }
    }

    fn is_connected(&self) -> bool {
        self.sh.paired.load(Ordering::SeqCst) && self.sh.state.lock().as_ref().is_some_and(|(_, t)| t.elapsed() < STALE)
    }

    fn status_text(&self) -> String {
        let addr = self.address();
        if self.sh.host.is_empty() {
            return tr("Wpisz adres komputera z symulatorem i OpenTakeoff").to_string();
        }
        let problem = self.sh.problem.lock().clone();
        if !problem.is_empty() {
            return trf!("Drugi komputer {addr}: {problem}");
        }
        if !self.sh.paired.load(Ordering::SeqCst) {
            return trf!("Łączenie z komputerem {addr}…");
        }
        let text = self.sh.remote_text.lock().clone();
        if self.is_connected() {
            trf!("Połączono z komputerem {addr}: {text}")
        } else {
            trf!("Połączono z komputerem {addr}, brak danych lotu: {text}")
        }
    }

    fn snapshot(&self) -> Option<FlightState> {
        self.sh.state.lock().as_ref().filter(|(_, t)| t.elapsed() < STALE).map(|(s, _)| s.clone())
    }

    fn execute(&self, cmd: &Validated) -> CommandResult {
        let mut msg = json!({ "type": "command", "name": cmd.spec.name });
        if cmd.spec.needs_value() {
            msg["value"] = cmd.value.into();
        }
        match self.sh.request(msg, COMMAND_TIMEOUT) {
            None => CommandResult::fail(codes::SIM_UNAVAILABLE, tr("Brak odpowiedzi drugiego komputera")),
            Some(r) if r["type"] == "ack" => CommandResult::success(),
            Some(r) => CommandResult::fail(static_code(r["code"].as_str().unwrap_or("")), r["message"].as_str().unwrap_or("").to_string()),
        }
    }

    fn axes(&self) -> Arc<AxisController> {
        self.axes.clone()
    }

    fn navdata(&self) -> Option<Arc<dyn NavDataProvider>> {
        Some(Arc::new(RemoteNav { sh: self.sh.clone() }))
    }

    fn start(&self) {
        if self.sh.host.is_empty() || self.sh.running.swap(true, Ordering::SeqCst) {
            return;
        }
        self.sh.stop.store(false, Ordering::SeqCst);
        let (sh, axes) = (self.sh.clone(), self.axes.clone());
        let _ = std::thread::Builder::new().name("remote-pc".into()).spawn(move || {
            while !sh.stop.load(Ordering::SeqCst) {
                session(&sh, &axes);
                sh.paired.store(false, Ordering::SeqCst);
                *sh.out.lock() = None;
                sh.pending.lock().clear();
                let until = Instant::now() + RETRY;
                while Instant::now() < until && !sh.stop.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            sh.running.store(false, Ordering::SeqCst);
        });
    }

    fn stop(&self) {
        self.sh.stop.store(true, Ordering::SeqCst);
    }

    fn sim_name(&self) -> String {
        let v = self.sh.remote_version.lock().clone();
        if v.is_empty() {
            tr("Drugi komputer").to_string()
        } else {
            trf!("Drugi komputer (OpenTakeoff {v})")
        }
    }
}

fn connect(sh: &Shared) -> Result<WebSocket<TcpStream>, String> {
    let addr = (sh.host.as_str(), sh.port).to_socket_addrs().map_err(|e| e.to_string())?.next().ok_or_else(|| tr("Nieznany adres").to_string())?;
    let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|e| e.to_string())?;
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(CONNECT_TIMEOUT));
    let req = format!("ws://{}:{}/", if sh.host.contains(':') { format!("[{}]", sh.host) } else { sh.host.clone() }, sh.port).into_client_request().map_err(|e| e.to_string())?;
    let (ws, _) = tungstenite::client(req, stream).map_err(|e| match e {
        tungstenite::HandshakeError::Failure(e) => e.to_string(),
        tungstenite::HandshakeError::Interrupted(_) => tr("Przerwane uzgadnianie połączenia").to_string(),
    })?;
    ws.get_ref().set_nonblocking(true).map_err(|e| e.to_string())?;
    Ok(ws)
}

fn session(sh: &Arc<Shared>, axes: &Arc<AxisController>) {
    let mut ws = match connect(sh) {
        Ok(ws) => ws,
        Err(e) => {
            *sh.problem.lock() = trf!("brak połączenia ({e})");
            return;
        }
    };
    let (tx, rx): (Sender<String>, Receiver<String>) = unbounded();
    *sh.out.lock() = Some(tx);
    let token = sh.token.lock().clone();
    let code = sh.code.lock().clone();
    let mut hello = json!({ "v": VERSION, "type": "hello", "id": "hello", "client": { "name": sh.client_name, "platform": "windows" }, "rateHz": 20 });
    if !token.is_empty() {
        hello["token"] = token.clone().into();
    }
    if !code.is_empty() {
        hello["code"] = code.into();
    }
    if token.is_empty() && hello.get("code").is_none() {
        *sh.problem.lock() = tr("wpisz kod parowania z tamtego komputera").to_string();
        let _ = ws.close(None);
        return;
    }
    let _ = ws.send(Message::Text(hello.to_string()));
    let mut last_ping = Instant::now();
    let mut last_msg = Instant::now();
    let mut last_axes = Instant::now();
    let mut axes_held = false;
    loop {
        if sh.stop.load(Ordering::SeqCst) {
            let _ = ws.close(None);
            let _ = ws.flush();
            return;
        }
        let mut busy = false;
        match ws.read() {
            Ok(Message::Text(t)) => {
                busy = true;
                last_msg = Instant::now();
                if !incoming(sh, &t) {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return;
                }
            }
            Ok(Message::Close(_)) => {
                *sh.problem.lock() = tr("tamten komputer zakończył połączenie").to_string();
                return;
            }
            Ok(_) => {
                busy = true;
                last_msg = Instant::now();
            }
            Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => {}
            Err(e) => {
                *sh.problem.lock() = trf!("połączenie przerwane ({e})");
                return;
            }
        }
        if last_msg.elapsed() > Duration::from_secs(30) {
            *sh.problem.lock() = tr("tamten komputer nie odpowiada").to_string();
            return;
        }
        // Osie z telefonów tego komputera → tamten komputer (20 razy na sekundę)
        if sh.paired.load(Ordering::SeqCst) && last_axes.elapsed() >= Duration::from_millis(50) {
            last_axes = Instant::now();
            let now = tick_ms();
            match axes.tick(now) {
                Some(o) if !o.is_final => {
                    axes_held = true;
                    let _ = ws.write(Message::Text(json!({ "v": VERSION, "type": "axis", "aileron": o.aileron, "elevator": o.elevator, "rudder": o.rudder }).to_string()));
                }
                Some(_) if axes_held => {
                    axes_held = false;
                    let _ = ws.write(Message::Text(json!({ "v": VERSION, "type": "axis", "release": true }).to_string()));
                }
                _ => {}
            }
            if let Some(a) = axes.tick_aux(now) {
                let mut m = json!({ "v": VERSION, "type": "axis" });
                if let Some(sb) = a.speedbrake {
                    m["speedbrake"] = sb.into();
                }
                if let Some(b) = a.brake {
                    m["brake"] = b.into();
                }
                let _ = ws.write(Message::Text(m.to_string()));
            }
        }
        while let Ok(m) = rx.try_recv() {
            busy = true;
            if ws.write(Message::Text(m)).is_err() {
                break;
            }
        }
        if last_ping.elapsed() > Duration::from_secs(10) {
            last_ping = Instant::now();
            let _ = ws.write(Message::Text(json!({ "v": VERSION, "type": "ping", "t": tick_ms() }).to_string()));
        }
        match ws.flush() {
            Ok(()) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => {}
            Err(e) => {
                *sh.problem.lock() = trf!("połączenie przerwane ({e})");
                return;
            }
        }
        if !busy {
            std::thread::sleep(Duration::from_millis(4));
        }
    }
}

/// Wiadomość od tamtego komputera. false – zakończyć połączenie (nieudane parowanie).
fn incoming(sh: &Arc<Shared>, text: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return true };
    match v["type"].as_str().unwrap_or("") {
        "state" => {
            if let Ok(s) = serde_json::from_value::<FlightState>(v["data"].clone()) {
                *sh.state.lock() = Some((s, Instant::now()));
            }
        }
        "status" => {
            *sh.remote_source.lock() = v["source"].as_str().unwrap_or("none").to_string();
            *sh.remote_text.lock() = v["message"].as_str().unwrap_or("").to_string();
        }
        "hello_ok" => {
            let token = v["token"].as_str().unwrap_or("").to_string();
            *sh.remote_source.lock() = v["source"].as_str().unwrap_or("none").to_string();
            *sh.remote_version.lock() = v["version"].as_str().unwrap_or("").to_string();
            sh.problem.lock().clear();
            sh.code.lock().clear();
            if !token.is_empty() && *sh.token.lock() != token {
                *sh.token.lock() = token.clone();
                (sh.on_token)(&token);
            }
            sh.paired.store(true, Ordering::SeqCst);
        }
        "error" if v["id"] == "hello" || v["code"] == codes::PAIRING_FAILED => {
            if v["code"] == codes::RATE_LIMITED {
                *sh.problem.lock() = tr("za dużo prób parowania – odczekaj minutę").to_string();
            } else {
                // Token unieważniony na tamtym komputerze albo zły kod: potrzebny nowy kod parowania
                let had_token = !sh.token.lock().is_empty();
                sh.token.lock().clear();
                if had_token {
                    (sh.on_token)("");
                }
                sh.code.lock().clear();
                *sh.problem.lock() = tr("zły kod parowania – wpisz kod widoczny na tamtym komputerze").to_string();
            }
            return false;
        }
        _ => {
            if let Some(id) = v["id"].as_str() {
                if let Some(tx) = sh.pending.lock().remove(id) {
                    let _ = tx.send(v);
                }
            }
        }
    }
    true
}

/// Dane nawigacyjne z symulatora na tamtym komputerze.
struct RemoteNav {
    sh: Arc<Shared>,
}

impl RemoteNav {
    fn query(&self, q: Value) -> NavResult<Value> {
        let mut msg = q;
        msg["type"] = "navdata_request".into();
        match self.sh.request(msg, NAVDATA_TIMEOUT) {
            None => Err(NavError::unavailable()),
            Some(r) if r["type"] == "navdata" => Ok(r["data"].clone()),
            Some(r) => Err(NavError { code: static_code(r["code"].as_str().unwrap_or("")), message: r["message"].as_str().unwrap_or("").to_string() }),
        }
    }

    fn fixes(&self, q: Value) -> NavResult<Vec<FixResult>> {
        let data = match self.query(q) {
            Err(e) if e.code == codes::NOT_FOUND => return Ok(Vec::new()),
            r => r?,
        };
        serde_json::from_value(data["results"].clone()).map_err(|e| NavError { code: codes::SIM_ERROR, message: e.to_string() })
    }
}

impl NavDataProvider for RemoteNav {
    fn airport(&self, icao: &str) -> NavResult<AirportInfo> {
        let data = self.query(json!({ "kind": "airport", "icao": icao }))?;
        serde_json::from_value(data).map_err(|e| NavError { code: codes::SIM_ERROR, message: e.to_string() })
    }

    fn procedures(&self, icao: &str) -> NavResult<AirportProcedures> {
        let data = self.query(json!({ "kind": "procedures_full", "icao": icao }))?;
        serde_json::from_value(data).map_err(|e| NavError { code: codes::SIM_ERROR, message: e.to_string() })
    }

    fn find_fix(&self, ident: &str) -> NavResult<Vec<FixResult>> {
        self.fixes(json!({ "kind": "fix", "ident": ident }))
    }

    fn nearby(&self, lat: f64, lon: f64, radius_nm: f64) -> NavResult<Vec<FixResult>> {
        self.fixes(json!({ "kind": "nearby", "lat": lat, "lon": lon, "radiusNm": radius_nm.clamp(1.0, 250.0) }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navdata::NavDataService;
    use crate::pairing::Pairing;
    use crate::server::LinkServer;
    use crate::simulation::SimulationSource;

    #[test]
    fn addresses() {
        assert_eq!(parse_address("192.168.1.5"), Some(("192.168.1.5".into(), WEBSOCKET_PORT)));
        assert_eq!(parse_address(" 192.168.1.5:47812 "), Some(("192.168.1.5".into(), 47812)));
        assert_eq!(parse_address("ws://pc-sim:47811/"), Some(("pc-sim".into(), 47811)));
        assert_eq!(parse_address("[fe80::1]:47813"), Some(("fe80::1".into(), 47813)));
        assert_eq!(parse_address(""), None);
        assert_eq!(parse_address("pc:abc"), None);
    }

    #[test]
    fn pair_stream_command_and_navdata() {
        let pairing = Arc::new(Pairing::new(None));
        let axes = Arc::new(AxisController::default());
        let srv = LinkServer::new(pairing.clone(), axes.clone(), Arc::new(NavDataService::new()), "3.0.0", Arc::new(|_| {}));
        let port = srv.start(0).unwrap();
        let sim: Arc<dyn FlightSource> = Arc::new(SimulationSource::new(axes));
        sim.start();
        srv.set_source(Some(sim));

        let saved = Arc::new(Mutex::new(String::new()));
        let s2 = saved.clone();
        let remote = RemoteSource::new(Arc::new(AxisController::default()), &format!("127.0.0.1:{port}"), &pairing.code(), "", Arc::new(move |t: &str| *s2.lock() = t.to_string()));
        remote.start();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !remote.is_connected() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(remote.is_connected(), "{}", remote.status_text());
        assert_eq!(remote.kind(), "simulation");
        assert!(!saved.lock().is_empty());
        assert_eq!(remote.snapshot().unwrap().source, "simulation");
        let cmd = crate::commands::validate(Some("HEADING_BUG_SET"), Some(&json!(123))).unwrap();
        assert!(remote.execute(&cmd).ok);
        let nav = remote.navdata().unwrap();
        assert_eq!(nav.airport("EPGD").unwrap().icao, "EPGD");
        assert!(!nav.procedures("EPGD").unwrap().approaches.is_empty());
        remote.stop();
        srv.stop();
    }

    fn flight(id: &str, start: i64) -> crate::logbook::FlightLog {
        use crate::logbook::TrackPoint;
        let track = vec![TrackPoint { t: start, lat: 52.0, lon: 21.0, ..Default::default() }, TrackPoint { t: start + 600_000, lat: 52.1, lon: 21.1, ..Default::default() }];
        crate::logbook::FlightLog { id: id.into(), start, end: start + 600_000, aircraft: Some("C172".into()), track, ..Default::default() }
    }

    fn temp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("otk-remote-{}", crate::logbook::model::new_id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    #[test]
    fn logbook_sync_both_ways() {
        let (main_dir, second_dir) = (temp(), temp());
        let main = LogbookStore::open(main_dir.clone());
        let second = LogbookStore::open(second_dir.clone());
        main.put(flight("main-only", 10_000_000), "pc");
        main.put(flight("main-deleted", 20_000_000), "pc");
        second.put(flight("main-deleted", 20_000_000), "pc");
        second.put(flight("second-only", 30_000_000), "pc");
        second.put(flight("second-deleted", 40_000_000), "pc");
        main.put(flight("second-deleted", 40_000_000), "pc");
        main.delete(&["main-deleted".into()]);
        second.delete(&["second-deleted".into()]);

        let pairing = Arc::new(Pairing::new(None));
        let axes = Arc::new(AxisController::default());
        let srv = LinkServer::new(pairing.clone(), axes, Arc::new(NavDataService::new()), "3.0.0", Arc::new(|_| {}));
        srv.set_sync(main.clone());
        let port = srv.start(0).unwrap();
        let token = match pairing.try_pair("127.0.0.1", Some(pairing.code().as_str()), None) {
            crate::pairing::PairResult::Ok(t) => t,
            r => panic!("{r:?}"),
        };
        let r = sync_logbook("127.0.0.1", port, &token, &second, "PC2", "PC1", 0).unwrap();
        assert_eq!(r, SyncOutcome { received: 1, sent: 1, removed: 1 });
        let ids = |s: &LogbookStore| {
            let mut v: Vec<String> = s.flights().iter().map(|f| f.id.clone()).collect();
            v.sort();
            v
        };
        assert_eq!(ids(&*main), vec!["main-only".to_string(), "second-only".to_string()]);
        assert_eq!(ids(&*second), ids(&*main));
        assert!(sync_logbook("127.0.0.1", port, "zly", &second, "PC2", "PC1", 0).is_err());
        srv.stop();
        let _ = std::fs::remove_dir_all(main_dir);
        let _ = std::fs::remove_dir_all(second_dir);
    }
}
