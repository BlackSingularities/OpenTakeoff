//! Serwer dla telefonów na porcie 47810: WebSocket (parowanie, strumień `state`, komendy, osie, navdata)
//! i HTTP (synchronizacja dziennika lotów, p. 6 protokołu). Każde połączenie ma własny wątek; komendy
//! wykonuje osobny wątek sesji (zmiana kamery potrafi trwać sekundę), zapytania navdata – wątki robocze
//! (najwyżej 4 naraz na telefon).

use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU16, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::{Condvar, Mutex, RwLock};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use tungstenite::protocol::{CloseFrame, Role, WebSocketConfig};
use tungstenite::{Message, WebSocket};

use crate::axes::{AxisController, UpdateResult};
use crate::commands::{self, Target, Validated};
use crate::lang::tr;
use crate::logbook::sync::{DownloadRequest, SyncHandler, SyncRequest};
use crate::navdata::{NavDataService, Query, TokenBucket};
use crate::pairing::{PairResult, Pairing};
use crate::proto::{self, codes, Incoming};
use crate::source::{tick_ms, unix_ms, FlightSource};
use crate::trf;

pub const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
pub const MAX_COMMANDS_PER_SECOND: f64 = 50.0;
pub const MAX_NAVDATA_IN_FLIGHT: usize = 4;
const MAX_HTTP_BODY: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ClientInfo {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub connected_at: i64,
    pub paired: bool,
    pub rate_hz: u32,
    pub controlling: bool,
}

/// Zdarzenia dla okna programu (np. komunikat po synchronizacji).
#[derive(Clone, Debug)]
pub enum ServerEvent {
    Synced { device: String, received: usize, sent: usize, removed: usize },
}

struct Session {
    id: String,
    ip: String,
    name: Mutex<String>,
    token: Mutex<Option<String>>,
    connected_at: i64,
    paired: AtomicBool,
    rate_hz: AtomicU32,
    out: Sender<String>,
    closed: AtomicBool,
    commands: TokenBucket,
    nav_bucket: TokenBucket,
    nav_slots: Mutex<usize>,
    nav_free: Condvar,
}

type LogFn = Arc<dyn Fn(String) + Send + Sync>;

/// Strony dla OBS (`/obs…`): pasek i karta lądowania jako źródło przeglądarki.
pub trait WebPages: Send + Sync {
    /// Treść strony albo danych: (typ, bajty); None – nie ma takiej strony.
    fn get(&self, path: &str, query: &str) -> Option<(&'static str, Vec<u8>)>;
    /// Czy strony mogą otwierać inne komputery w sieci (drugi komputer do streamu).
    fn allow_lan(&self) -> bool;
}

struct Inner {
    pairing: Arc<Pairing>,
    axes: Arc<AxisController>,
    version: String,
    sessions: Mutex<Vec<Arc<Session>>>,
    source: RwLock<Option<Arc<dyn FlightSource>>>,
    seq: AtomicI64,
    nav: Arc<NavDataService>,
    sync: RwLock<Option<Arc<dyn SyncHandler>>>,
    web: RwLock<Option<Arc<dyn WebPages>>>,
    stop: AtomicBool,
    port: AtomicU16,
    log: LogFn,
    events: Mutex<Vec<ServerEvent>>,
}

#[derive(Clone)]
pub struct LinkServer {
    inner: Arc<Inner>,
}

impl LinkServer {
    pub fn new(pairing: Arc<Pairing>, axes: Arc<AxisController>, nav: Arc<NavDataService>, version: &str, log: LogFn) -> Self {
        Self {
            inner: Arc::new(Inner {
                pairing,
                axes,
                version: version.to_string(),
                sessions: Mutex::new(Vec::new()),
                source: RwLock::new(None),
                seq: AtomicI64::new(0),
                nav,
                sync: RwLock::new(None),
                web: RwLock::new(None),
                stop: AtomicBool::new(false),
                port: AtomicU16::new(0),
                log,
                events: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Otwiera port i zaczyna przyjmować połączenia. Błąd = port zajęty (np. działa drugi program).
    pub fn start(&self, port: u16) -> std::io::Result<u16> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        let port = listener.local_addr()?.port();
        self.inner.port.store(port, Ordering::SeqCst);
        let inner = self.inner.clone();
        std::thread::Builder::new().name("link-accept".into()).spawn(move || {
            for stream in listener.incoming() {
                if inner.stop.load(Ordering::SeqCst) {
                    break;
                }
                match stream {
                    Ok(s) => {
                        let i = inner.clone();
                        let _ = std::thread::Builder::new().name("link-conn".into()).spawn(move || handle(i, s));
                    }
                    Err(e) => (inner.log)(trf!("Błąd przyjmowania połączenia: {e}")),
                }
            }
        })?;
        (self.inner.log)(trf!("Serwer telefonów nasłuchuje na porcie {port}"));
        Ok(port)
    }

    pub fn port(&self) -> u16 {
        self.inner.port.load(Ordering::SeqCst)
    }

    pub fn set_source(&self, source: Option<Arc<dyn FlightSource>>) {
        *self.inner.source.write() = source;
        self.broadcast_status();
    }

    pub fn source(&self) -> Option<Arc<dyn FlightSource>> {
        self.inner.source.read().clone()
    }

    pub fn set_sync(&self, handler: Arc<dyn SyncHandler>) {
        *self.inner.sync.write() = Some(handler);
    }

    pub fn set_web(&self, pages: Arc<dyn WebPages>) {
        *self.inner.web.write() = Some(pages);
    }

    pub fn source_name(&self) -> &'static str {
        source_name(&self.inner)
    }

    pub fn clients(&self) -> Vec<ClientInfo> {
        let owner = self.inner.axes.owner();
        self.inner
            .sessions
            .lock()
            .iter()
            .map(|s| ClientInfo {
                id: s.id.clone(),
                name: s.name.lock().clone(),
                ip: s.ip.clone(),
                connected_at: s.connected_at,
                paired: s.paired.load(Ordering::SeqCst),
                rate_hz: s.rate_hz.load(Ordering::SeqCst),
                controlling: owner.as_deref() == Some(s.id.as_str()),
            })
            .collect()
    }

    /// Rozłącza telefon (np. z listy urządzeń).
    pub fn disconnect(&self, id: &str) {
        if let Some(s) = self.inner.sessions.lock().iter().find(|s| s.id == id) {
            s.closed.store(true, Ordering::SeqCst);
        }
    }

    pub fn take_events(&self) -> Vec<ServerEvent> {
        std::mem::take(&mut *self.inner.events.lock())
    }

    /// `status` do wszystkich sparowanych (zmiana źródła danych albo połączenia z MSFS).
    pub fn broadcast_status(&self) {
        let msg = status_message(&self.inner);
        for s in self.inner.sessions.lock().iter().filter(|s| s.paired.load(Ordering::SeqCst)) {
            let _ = s.out.send(msg.clone());
        }
    }

    pub fn stop(&self) {
        self.inner.stop.store(true, Ordering::SeqCst);
        for s in self.inner.sessions.lock().iter() {
            s.closed.store(true, Ordering::SeqCst);
        }
        // Budzi wątek akceptujący, żeby zauważył koniec.
        let _ = TcpStream::connect_timeout(&([127, 0, 0, 1], self.port()).into(), Duration::from_millis(200));
    }
}

fn source_name(inner: &Inner) -> &'static str {
    match inner.source.read().as_ref() {
        Some(s) if s.is_connected() => s.kind(),
        _ => "none",
    }
}

fn status_message(inner: &Inner) -> String {
    let src = inner.source.read().clone();
    let sim_connected = src.as_ref().is_some_and(|s| matches!(s.kind(), "msfs" | "xplane") && s.is_connected());
    let text = src.as_ref().map(|s| s.status_text()).unwrap_or_else(|| tr("Brak źródła danych").to_string());
    proto::status(source_name(inner), sim_connected, &text)
}

// ---------- HTTP i uzgadnianie WebSocket ----------

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    rest: Vec<u8>,
}

fn read_head(stream: &mut TcpStream) -> Option<Request> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    let deadline = Instant::now() + HELLO_TIMEOUT;
    let end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        if buf.len() > 16 * 1024 || Instant::now() > deadline {
            return None;
        }
        match stream.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => continue,
            Err(_) => return None,
        }
    };
    let head = String::from_utf8_lossy(&buf[..end]).to_string();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_string();
    let path = first.next()?.to_string();
    let headers = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string())).collect();
    Some(Request { method, path, headers, rest: buf[end + 4..].to_vec() })
}

fn handle(inner: Arc<Inner>, mut stream: TcpStream) {
    let ip = stream.peer_addr().map(|a| a.ip().to_canonical().to_string()).unwrap_or_else(|_| "?".into());
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let Some(req) = read_head(&mut stream) else { return };
    let upgrade = req.headers.get("upgrade").is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    if upgrade {
        let Some(key) = req.headers.get("sec-websocket-key") else {
            let _ = respond(&mut stream, 400, "text/plain", b"Bad Request");
            return;
        };
        let mut h = Sha1::new();
        h.update(key.trim().as_bytes());
        h.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
        let accept = base64::engine::general_purpose::STANDARD.encode(h.finalize());
        let resp = format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n");
        if stream.write_all(resp.as_bytes()).is_err() {
            return;
        }
        websocket(inner, stream, req.rest, ip);
    } else {
        http(&inner, stream, req, &ip);
    }
}

fn respond(stream: &mut TcpStream, code: u16, content_type: &str, body: &[u8]) -> std::io::Result<()> {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Payload Too Large",
        503 => "Service Unavailable",
        _ => "Error",
    };
    let head = format!("HTTP/1.1 {code} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len());
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn json_body(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap_or_default()
}

fn http(inner: &Arc<Inner>, mut stream: TcpStream, req: Request, ip: &str) {
    let path = req.path.split('?').next().unwrap_or("").to_string();
    if path == "/obs" || path.starts_with("/obs/") {
        let query = req.path.split_once('?').map(|(_, q)| q).unwrap_or("");
        let web = inner.web.read().clone();
        let local = ip.parse::<std::net::IpAddr>().is_ok_and(|a| a.is_loopback());
        let _ = match web {
            Some(w) if !local && !w.allow_lan() => respond(&mut stream, 403, "text/plain; charset=utf-8", tr("Strony dla OBS są dostępne tylko na tym komputerze. Włącz dostęp z sieci lokalnej na karcie Nakładki.").as_bytes()),
            Some(w) if req.method == "GET" => match w.get(&path, query) {
                Some((ct, body)) => respond(&mut stream, 200, ct, &body),
                None => respond(&mut stream, 404, "text/plain", b"Not Found"),
            },
            _ => respond(&mut stream, 404, "text/plain", b"Not Found"),
        };
        return;
    }
    if !path.starts_with("/v1/") {
        let body = format!("OpenTakeoff {} – {}\n", inner.version, tr("połącz się z tym komputerem z aplikacji OpenTakeoff na telefonie."));
        let _ = respond(&mut stream, 200, "text/plain; charset=utf-8", body.as_bytes());
        return;
    }
    let token = req.headers.get("authorization").and_then(|a| a.strip_prefix("Bearer ")).map(str::trim).unwrap_or("");
    if !inner.pairing.is_valid_token(token) {
        let _ = respond(&mut stream, 401, "application/json", &json_body(&json!({ "v": 1, "code": codes::PAIRING_FAILED, "message": "Nieważny token" })));
        return;
    }
    let Some(sync) = inner.sync.read().clone() else {
        let _ = respond(&mut stream, 503, "application/json", &json_body(&json!({ "v": 1, "code": "unavailable" })));
        return;
    };
    let len: usize = req.headers.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    if len > MAX_HTTP_BODY {
        let _ = respond(&mut stream, 413, "text/plain", b"too big");
        return;
    }
    let mut body = req.rest;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut chunk = vec![0u8; 64 * 1024];
    while body.len() < len {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    body.truncate(len);
    let device = inner.pairing.device_name(token).filter(|n| !n.is_empty()).unwrap_or_else(|| ip.to_string());
    let result: Result<(u16, &str, Vec<u8>), String> = match (req.method.as_str(), path.as_str()) {
        ("GET", "/v1/logbook/list") => Ok((200, "application/json", json_body(&json!({ "v": 1, "flights": sync.list() })))),
        ("POST", "/v1/logbook/sync") => serde_json::from_slice::<SyncRequest>(&body).map_err(|e| e.to_string()).map(|r| {
            let name = if r.device.trim().is_empty() { device.clone() } else { r.device.trim().to_string() };
            let plan = sync.plan(&r, &name);
            if !plan.remove.is_empty() || !plan.want.is_empty() || !plan.offer.is_empty() {
                (inner.log)(trf!("{name}: synchronizacja – do pobrania z telefonu {w}, do wysłania {o}, do usunięcia w telefonie {r}", w = plan.want.len(), o = plan.offer.len(), r = plan.remove.len()));
            }
            inner.events.lock().push(ServerEvent::Synced { device: name, received: 0, sent: 0, removed: plan.remove.len() });
            (200, "application/json", serde_json::to_vec(&plan).unwrap_or_default())
        }),
        ("POST", "/v1/logbook/upload") => sync.upload(&body, &device).map(|r| {
            if r.added > 0 {
                (inner.log)(trf!("{device}: odebrano {n} lot(y) z telefonu", n = r.added));
            }
            inner.events.lock().push(ServerEvent::Synced { device: device.clone(), received: r.added, sent: 0, removed: 0 });
            (200, "application/json", json_body(&json!({ "v": 1, "added": r.added, "skipped": r.skipped })))
        }),
        ("POST", "/v1/logbook/download") => serde_json::from_slice::<DownloadRequest>(&body).map_err(|e| e.to_string()).map(|r| {
            let bytes = sync.download(&r.ids);
            if !r.ids.is_empty() {
                (inner.log)(trf!("{device}: wysłano {n} lot(y) do telefonu", n = r.ids.len()));
            }
            inner.events.lock().push(ServerEvent::Synced { device: device.clone(), received: 0, sent: r.ids.len(), removed: 0 });
            inner.pairing.mark_synced(token);
            (200, "application/octet-stream", bytes)
        }),
        _ => Ok((404, "application/json", json_body(&json!({ "v": 1, "code": "not_found" })))),
    };
    match result {
        Ok((code, ct, body)) => {
            let _ = respond(&mut stream, code, ct, &body);
        }
        Err(e) => {
            let _ = respond(&mut stream, 400, "application/json", &json_body(&json!({ "v": 1, "code": codes::BAD_JSON, "message": e })));
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}

// ---------- sesja WebSocket ----------

enum Job {
    Command(Option<String>, Validated),
}

fn websocket(inner: Arc<Inner>, stream: TcpStream, rest: Vec<u8>, ip: String) {
    let mut cfg = WebSocketConfig::default();
    cfg.max_message_size = Some(proto::MAX_CLIENT_MESSAGE_BYTES);
    cfg.max_frame_size = Some(proto::MAX_CLIENT_MESSAGE_BYTES);
    cfg.max_write_buffer_size = 4 * 1024 * 1024;
    if stream.set_nonblocking(true).is_err() {
        return;
    }
    let mut ws = WebSocket::from_partially_read(stream, rest, Role::Server, Some(cfg));
    let (tx, rx): (Sender<String>, Receiver<String>) = unbounded();
    let session = Arc::new(Session {
        id: crate::logbook::model::new_id().replace('-', ""),
        ip: ip.clone(),
        name: Mutex::new(ip.clone()),
        token: Mutex::new(None),
        connected_at: unix_ms(),
        paired: AtomicBool::new(false),
        rate_hz: AtomicU32::new(proto::DEFAULT_RATE_HZ),
        out: tx,
        closed: AtomicBool::new(false),
        commands: TokenBucket::new(MAX_COMMANDS_PER_SECOND),
        nav_bucket: TokenBucket::new(proto::MAX_NAVDATA_PER_SECOND as f64),
        nav_slots: Mutex::new(0),
        nav_free: Condvar::new(),
    });
    inner.sessions.lock().push(session.clone());
    (inner.log)(trf!("Połączenie z {ip}"));

    // Wątek komend: wykonuje po kolei (zachowana kolejność ack), nie blokuje strumienia stanu.
    let (job_tx, job_rx) = unbounded::<Job>();
    {
        let (inner, session) = (inner.clone(), session.clone());
        let _ = std::thread::Builder::new().name("link-commands".into()).spawn(move || {
            while let Ok(Job::Command(id, cmd)) = job_rx.recv() {
                run_command(&inner, &session, id.as_deref(), &cmd);
            }
        });
    }

    let started = Instant::now();
    let mut last_msg = Instant::now();
    let mut last_ping = Instant::now();
    let mut next_tick = Instant::now();
    let mut credit = 0u32;
    let tick = Duration::from_millis(1000 / proto::MAX_RATE_HZ as u64);
    let close_reason: Option<(u16, &str)> = loop {
        if session.closed.load(Ordering::SeqCst) || inner.stop.load(Ordering::SeqCst) {
            break Some((1001, "bye"));
        }
        let mut busy = false;
        match ws.read() {
            Ok(Message::Text(t)) => {
                busy = true;
                last_msg = Instant::now();
                handle_message(&inner, &session, &t, &job_tx);
            }
            Ok(Message::Close(_)) => break None,
            Ok(_) => {
                busy = true;
                last_msg = Instant::now();
            }
            Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => {}
            Err(tungstenite::Error::Capacity(_)) => break Some((1009, "too big")),
            Err(_) => break None,
        }
        let paired = session.paired.load(Ordering::SeqCst);
        if !paired && started.elapsed() > HELLO_TIMEOUT {
            break Some((1008, "hello timeout"));
        }
        if paired && last_msg.elapsed() > IDLE_TIMEOUT {
            (inner.log)(trf!("{name} ({ip}) milczy od 30 s – rozłączam", name = session.name.lock().clone()));
            break Some((1001, "idle"));
        }
        // Strumień stanu: zegar tyka z maksymalną częstotliwością, wysyłamy średnio dokładnie rateHz.
        if Instant::now() >= next_tick {
            next_tick += tick;
            if Instant::now() > next_tick + tick * 4 {
                next_tick = Instant::now() + tick;
            }
            if paired {
                credit += session.rate_hz.load(Ordering::SeqCst);
                if credit >= proto::MAX_RATE_HZ {
                    credit -= proto::MAX_RATE_HZ;
                    let src = inner.source.read().clone();
                    if let Some(snap) = src.filter(|s| s.is_connected()).and_then(|s| s.snapshot()) {
                        let seq = inner.seq.fetch_add(1, Ordering::SeqCst) + 1;
                        let _ = ws.write(Message::Text(proto::state(seq, unix_ms(), &snap)));
                    }
                }
            }
        }
        while let Ok(m) = rx.try_recv() {
            busy = true;
            if ws.write(Message::Text(m)).is_err() {
                break;
            }
        }
        if last_ping.elapsed() > Duration::from_secs(15) {
            last_ping = Instant::now();
            let _ = ws.write(Message::Ping(Vec::new()));
        }
        match ws.flush() {
            Ok(()) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => {}
            Err(_) => break None,
        }
        if !busy {
            std::thread::sleep(Duration::from_millis(4));
        }
    };
    if let Some((code, reason)) = close_reason {
        let _ = ws.close(Some(CloseFrame { code: code.into(), reason: reason.into() }));
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            match ws.flush() {
                Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(5)),
                _ => break,
            }
        }
    }
    drop(job_tx);
    session.closed.store(true, Ordering::SeqCst);
    inner.axes.release(&session.id);
    inner.sessions.lock().retain(|s| !Arc::ptr_eq(s, &session));
    let name = session.name.lock().clone();
    (inner.log)(trf!("Rozłączono {name} ({ip})"));
}

fn send(s: &Session, msg: String) {
    let _ = s.out.send(msg);
}

fn handle_message(inner: &Arc<Inner>, s: &Arc<Session>, text: &str, jobs: &Sender<Job>) {
    let m = match Incoming::parse(text) {
        Ok(m) => m,
        Err(e) => return send(s, e),
    };
    let id = m.id.clone();
    match m.kind.as_str() {
        "ping" => return send(s, proto::pong(id.as_deref(), m.get("t").and_then(|v| v.as_i64()), unix_ms())),
        "hello" => return hello(inner, s, &m),
        _ => {}
    }
    if !s.paired.load(Ordering::SeqCst) {
        return send(s, proto::error(id.as_deref(), codes::NOT_PAIRED, "Najpierw wyślij hello z kodem parowania"));
    }
    match m.kind.as_str() {
        "command" => {
            let validation = commands::validate(m.str("name"), m.get("value"));
            let look = matches!(&validation, Ok(v) if matches!(v.spec.target, Target::CameraPitch | Target::CameraYaw));
            if !look && !s.commands.try_take() {
                return send(s, proto::error(id.as_deref(), codes::RATE_LIMITED, "Maksymalnie 50 komend na sekundę"));
            }
            match validation {
                Err((code, msg)) => {
                    (inner.log)(trf!("{name}: odrzucono {cmd} – {msg}", name = s.name.lock().clone(), cmd = m.str("name").unwrap_or("—")));
                    send(s, proto::error(id.as_deref(), code, &msg));
                }
                Ok(cmd) => {
                    let _ = jobs.send(Job::Command(id, cmd));
                }
            }
        }
        "axis" => axis(inner, s, &m),
        "navdata_request" => navdata(inner, s, &m),
        other => send(s, proto::error(id.as_deref(), codes::UNKNOWN_TYPE, &format!("Nieznany typ wiadomości: {other}"))),
    }
}

fn hello(inner: &Arc<Inner>, s: &Arc<Session>, m: &Incoming) {
    let id = m.id.as_deref();
    match inner.pairing.try_pair(&s.ip, m.str("code"), m.str("token")) {
        PairResult::RateLimited => {
            (inner.log)(trf!("{ip}: za dużo prób parowania – blokada na 60 s", ip = s.ip));
            send(s, proto::error(id, codes::RATE_LIMITED, "Za dużo prób — odczekaj minutę"));
        }
        PairResult::Failed => {
            (inner.log)(trf!("{ip}: nieudane parowanie (zły kod lub token)", ip = s.ip));
            send(s, proto::error(id, codes::PAIRING_FAILED, "Zły kod parowania lub nieważny token"));
        }
        PairResult::Ok(token) => {
            if let Some(n) = m.get("client").and_then(|c| c.get("name")).and_then(|n| n.as_str()).map(str::trim).filter(|n| !n.is_empty()) {
                *s.name.lock() = n.chars().take(40).collect();
            }
            let rate = m.int("rateHz").unwrap_or(proto::DEFAULT_RATE_HZ as i64).clamp(proto::MIN_RATE_HZ as i64, proto::MAX_RATE_HZ as i64) as u32;
            s.rate_hz.store(rate, Ordering::SeqCst);
            s.paired.store(true, Ordering::SeqCst);
            *s.token.lock() = Some(token.clone());
            let name = s.name.lock().clone();
            inner.pairing.touch(&token, &name, &s.ip);
            (inner.log)(trf!("Sparowano: {name} ({ip}), {rate} Hz", ip = s.ip));
            send(
                s,
                proto::hello_ok(&proto::HelloOk {
                    id,
                    token: &token,
                    version: &inner.version,
                    source: source_name(inner),
                    rate_hz: rate,
                    max_deflection: inner.axes.max_deflection,
                    axis_timeout_ms: inner.axes.timeout_ms,
                }),
            );
            send(s, status_message(inner));
        }
    }
}

fn run_command(inner: &Arc<Inner>, s: &Arc<Session>, id: Option<&str>, cmd: &Validated) {
    let Some(src) = inner.source.read().clone().filter(|s| s.is_connected()) else {
        return send(s, proto::error(id, codes::SIM_UNAVAILABLE, "Brak połączenia z MSFS (włącz symulator albo tryb symulacji)"));
    };
    let r = src.execute(cmd);
    let name = s.name.lock().clone();
    if r.ok {
        if !matches!(cmd.spec.target, Target::CameraPitch | Target::CameraYaw) {
            let v = if cmd.spec.needs_value() { format!(" = {}", cmd.value) } else { String::new() };
            (inner.log)(format!("{name}: {}{v}", cmd.spec.name));
        }
        send(s, proto::ack(id, cmd.spec.name));
    } else {
        let msg = r.message.unwrap_or_else(|| "Błąd symulatora".into());
        (inner.log)(trf!("{name}: {cmd} nie powiodło się – {msg}", cmd = cmd.spec.name));
        send(s, proto::error(id, r.code.unwrap_or(codes::SIM_ERROR), &msg));
    }
}

fn axis_value(m: &Incoming, name: &str) -> Result<Option<f64>, ()> {
    match m.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().filter(|d| d.is_finite()).map(Some).ok_or(()),
    }
}

fn axis(inner: &Arc<Inner>, s: &Arc<Session>, m: &Incoming) {
    let id = m.id.as_deref();
    if m.bool("release") {
        inner.axes.release(&s.id);
        return;
    }
    let (Ok(a), Ok(e), Ok(r)) = (axis_value(m, "aileron"), axis_value(m, "elevator"), axis_value(m, "rudder")) else {
        return send(s, proto::error(id, codes::INVALID_VALUE, "Osie muszą być liczbami −1…1"));
    };
    let (Ok(sb), Ok(br)) = (axis_value(m, "speedbrake"), axis_value(m, "brake")) else {
        return send(s, proto::error(id, codes::INVALID_VALUE, "Hamulce muszą być liczbami 0…1"));
    };
    let now = tick_ms();
    if sb.is_some() || br.is_some() {
        if inner.axes.update_aux(&s.id, sb, br, now) == UpdateResult::Busy {
            send(s, proto::error(id, codes::NOT_ALLOWED, "Hamulcami steruje już inny telefon"));
        }
        if a.is_none() && e.is_none() && r.is_none() {
            return;
        }
    }
    let was = inner.axes.owner().as_deref() == Some(s.id.as_str());
    match inner.axes.update(&s.id, a, e, r, now) {
        UpdateResult::Busy => send(s, proto::error(id, codes::NOT_ALLOWED, "Osiami steruje już inny telefon")),
        UpdateResult::Invalid => send(s, proto::error(id, codes::INVALID_VALUE, "Osie muszą być liczbami −1…1")),
        UpdateResult::Ok if !was => (inner.log)(trf!("{name} przejmuje stery", name = s.name.lock().clone())),
        UpdateResult::Ok => {}
    }
}

fn navdata(inner: &Arc<Inner>, s: &Arc<Session>, m: &Incoming) {
    let id = m.id.clone();
    if !s.nav_bucket.try_take() {
        return send(s, proto::error(id.as_deref(), codes::RATE_LIMITED, "Maksymalnie 10 zapytań navdata na sekundę"));
    }
    let q = match Query::parse(m) {
        Ok(q) => q,
        Err(e) => return send(s, proto::error(id.as_deref(), codes::INVALID_VALUE, &e)),
    };
    let Some(src) = inner.source.read().clone().filter(|s| s.is_connected()) else {
        return send(s, proto::error(id.as_deref(), codes::SIM_UNAVAILABLE, "Brak połączenia z MSFS (włącz symulator albo tryb symulacji)"));
    };
    let Some(provider) = src.navdata() else {
        return send(s, proto::error(id.as_deref(), codes::SIM_UNAVAILABLE, "Brak połączenia z MSFS (włącz symulator albo tryb symulacji)"));
    };
    let (inner, s) = (inner.clone(), s.clone());
    let kind = src.kind();
    let _ = std::thread::Builder::new().name("link-navdata".into()).spawn(move || {
        {
            let mut slots = s.nav_slots.lock();
            while *slots >= MAX_NAVDATA_IN_FLIGHT {
                if s.closed.load(Ordering::SeqCst) {
                    return;
                }
                s.nav_free.wait_for(&mut slots, Duration::from_millis(200));
            }
            *slots += 1;
        }
        let res = inner.nav.execute(&q, provider.as_ref(), kind);
        {
            *s.nav_slots.lock() -= 1;
            s.nav_free.notify_one();
        }
        if s.closed.load(Ordering::SeqCst) {
            return;
        }
        match res {
            Ok(data) => send(&s, proto::navdata(id.as_deref(), q.kind(), data)),
            Err(e) => send(&s, proto::error(id.as_deref(), e.code, &e.message)),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::SimulationSource;
    use tungstenite::client::IntoClientRequest;

    fn server() -> (LinkServer, Arc<Pairing>, u16) {
        let pairing = Arc::new(Pairing::new(None));
        let axes = Arc::new(AxisController::default());
        let srv = LinkServer::new(pairing.clone(), axes.clone(), Arc::new(NavDataService::new()), "3.0.0", Arc::new(|_| {}));
        let port = srv.start(0).unwrap();
        let src: Arc<dyn FlightSource> = Arc::new(SimulationSource::new(axes));
        src.start();
        srv.set_source(Some(src));
        (srv, pairing, port)
    }

    type Client = WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

    fn connect(port: u16) -> Client {
        let req = format!("ws://127.0.0.1:{port}/").into_client_request().unwrap();
        let (ws, _) = tungstenite::connect(req).unwrap();
        if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_ref() {
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        }
        ws
    }

    fn next(ws: &mut Client, kind: &str) -> Value {
        for _ in 0..200 {
            if let Ok(Message::Text(t)) = ws.read() {
                let v: Value = serde_json::from_str(&t).unwrap();
                if v["type"] == kind {
                    return v;
                }
            }
        }
        panic!("brak {kind}");
    }

    #[test]
    fn pair_stream_and_command() {
        let (srv, pairing, port) = server();
        let mut ws = connect(port);
        ws.send(Message::Text(r#"{"v":1,"type":"command","id":"c0","name":"AP_MASTER"}"#.into())).unwrap();
        assert_eq!(next(&mut ws, "error")["code"], "not_paired");
        ws.send(Message::Text(r#"{"v":1,"type":"hello","id":"1","code":"000000x"}"#.into())).unwrap();
        assert_eq!(next(&mut ws, "error")["code"], "pairing_failed");
        let hello = format!(r#"{{"v":1,"type":"hello","id":"1","code":"{}","client":{{"name":"Test"}},"rateHz":20}}"#, pairing.code());
        ws.send(Message::Text(hello)).unwrap();
        let ok = next(&mut ws, "hello_ok");
        assert_eq!(ok["rateHz"], 20);
        assert!(ok["features"].as_array().unwrap().iter().any(|f| f == "logbook_sync"));
        assert_eq!(next(&mut ws, "status")["source"], "simulation");
        assert_eq!(next(&mut ws, "state")["data"]["source"], "simulation");
        ws.send(Message::Text(r#"{"v":1,"type":"command","id":"c1","name":"HEADING_BUG_SET","value":123}"#.into())).unwrap();
        assert_eq!(next(&mut ws, "ack")["id"], "c1");
        ws.send(Message::Text(r#"{"v":1,"type":"navdata_request","id":"n1","kind":"airport","icao":"EPGD"}"#.into())).unwrap();
        assert_eq!(next(&mut ws, "navdata")["data"]["icao"], "EPGD");
        ws.send(Message::Text(r#"{"v":1,"type":"ping","id":"p1","t":5}"#.into())).unwrap();
        assert_eq!(next(&mut ws, "pong")["t"], 5);
        assert_eq!(srv.clients().len(), 1);
        assert_eq!(srv.clients()[0].name, "Test");
        srv.stop();
    }

    #[test]
    fn obs_pages_local_only() {
        struct Pages(bool);
        impl WebPages for Pages {
            fn get(&self, path: &str, _q: &str) -> Option<(&'static str, Vec<u8>)> {
                (path == "/obs/bar").then(|| ("text/html", b"<html>bar</html>".to_vec()))
            }
            fn allow_lan(&self) -> bool {
                self.0
            }
        }
        let (srv, _p, port) = server();
        srv.set_web(Arc::new(Pages(false)));
        let get = |path: &str| {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out);
            out
        };
        let ok = get("/obs/bar?x=1");
        assert!(ok.starts_with("HTTP/1.1 200") && ok.contains("<html>bar</html>") && ok.contains("no-store"), "{ok}");
        assert!(get("/obs/none").starts_with("HTTP/1.1 404"));
    }

    #[test]
    fn http_requires_token() {
        let (srv, _pairing, port) = server();
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(b"POST /v1/logbook/sync HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\n\r\n{}").unwrap();
        let mut resp = String::new();
        let _ = s.read_to_string(&mut resp);
        assert!(resp.starts_with("HTTP/1.1 401"), "{resp}");
        srv.stop();
    }
}
