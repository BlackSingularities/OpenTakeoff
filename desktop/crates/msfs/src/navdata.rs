//! Dane nawigacyjne z bazy MSFS 2024 przez SimConnect Facilities API (AddToFacilityDefinition +
//! RequestFacilityData[_EX1]). Odpowiedzi przychodzą jako osobne wiadomości FACILITY_DATA dla każdego
//! obiektu drzewa (z ID rodzica), zakończone FACILITY_DATA_END. Pola są upakowane w kolejności definicji.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use opentakeoff_core::navdata::{
    fix_types as ft, math, AirportInfo, AirportProcedures, ApproachProcedure, FixResult, FrequencyInfo, IlsInfo, LegInfo, NamedLegs, NavDataProvider, NavError, NavResult, RunwayInfo,
    TerminalProcedure,
};
use parking_lot::{Condvar, Mutex};

use crate::data::{ascii, Cursor};
use crate::ffi::{self, Conn, Msg};

const DEF_AIRPORT: u32 = 0x4F41_0001;
const DEF_PROCEDURES: u32 = 0x4F41_0002;
const DEF_VOR: u32 = 0x4F41_0003;
const DEF_NDB: u32 = 0x4F41_0004;
const DEF_WAYPOINT: u32 = 0x4F41_0005;
const DEF_AIRPORT_POS: u32 = 0x4F41_0006;
const FIRST_REQUEST: u32 = 0x4F42_0000;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(4);
const LIST_TIMEOUT: Duration = Duration::from_secs(5);

// SIMCONNECT_FACILITY_DATA_TYPE
const T_AIRPORT: u32 = 0;
const T_RUNWAY: u32 = 1;
const T_FREQUENCY: u32 = 3;
const T_APPROACH: u32 = 5;
const T_APPROACH_TRANSITION: u32 = 6;
const T_APPROACH_LEG: u32 = 7;
const T_FINAL_APPROACH_LEG: u32 = 8;
const T_MISSED_APPROACH_LEG: u32 = 9;
const T_DEPARTURE: u32 = 10;
const T_ARRIVAL: u32 = 11;
const T_RUNWAY_TRANSITION: u32 = 12;
const T_ENROUTE_TRANSITION: u32 = 13;
const T_VOR: u32 = 19;
const T_NDB: u32 = 20;
const T_WAYPOINT: u32 = 21;

#[derive(Clone)]
struct Node {
    kind: u32,
    id: u32,
    parent: u32,
    index: u32,
    data: Vec<u8>,
}

#[derive(Clone)]
struct Candidate {
    kind: char,
    region: String,
    lat: f64,
    lon: f64,
}

#[derive(Default)]
struct Pending {
    nodes: Vec<Node>,
    candidates: Vec<Candidate>,
    list: Vec<FixResult>,
    list_type: String,
    done: Option<Result<(), NavError>>,
}

struct Reply {
    nodes: Vec<Node>,
    candidates: Vec<Candidate>,
}

struct Tree<'a> {
    nodes: &'a [Node],
}

impl<'a> Tree<'a> {
    fn root(&self, kind: u32) -> Option<&'a Node> {
        let ids: HashSet<u32> = self.nodes.iter().map(|n| n.id).collect();
        self.nodes.iter().find(|n| n.kind == kind && (n.parent == 0 || !ids.contains(&n.parent)))
    }
    fn children(&self, parent: &Node, kind: u32) -> Vec<&'a Node> {
        let mut v: Vec<&Node> = self.nodes.iter().filter(|n| n.parent == parent.id && n.kind == kind && n.id != parent.id).collect();
        v.sort_by_key(|n| n.index);
        v
    }
}

pub struct MsfsNavData {
    conn: Arc<Mutex<Option<Conn>>>,
    pending: Mutex<HashMap<u32, Pending>>,
    ready: Condvar,
    packets: Mutex<HashMap<u32, u32>>,
    early: Mutex<HashMap<u32, u32>>,
    next: Mutex<u32>,
}

fn not_found(m: impl Into<String>) -> NavError {
    NavError::not_found(m)
}

fn freq_mhz(f: u32) -> Option<f64> {
    match f {
        0 => None,
        f if f >= 100_000 => Some(math_round(f as f64 / 1_000_000.0, 3)),
        f => Some(math_round(f as f64 / 1000.0, 3)),
    }
}

fn math_round(v: f64, d: i32) -> f64 {
    opentakeoff_core::proto::r(v, d)
}

const LEG_TYPES: [&str; 24] = ["UNKNOWN", "AF", "CA", "CD", "CF", "CI", "CR", "DF", "FA", "FC", "FD", "FM", "HA", "HF", "HM", "IF", "PI", "RF", "TF", "VA", "VD", "VI", "VM", "VR"];

fn has_course(t: &str) -> bool {
    matches!(t, "CA" | "CD" | "CF" | "CI" | "CR" | "FA" | "FC" | "FD" | "FM" | "HA" | "HF" | "HM" | "PI" | "VA" | "VD" | "VI" | "VM" | "VR")
}

fn fix_type_name(t: char) -> &'static str {
    match t {
        'V' => ft::VOR,
        'N' => ft::NDB,
        'A' => ft::AIRPORT,
        _ => ft::WAYPOINT,
    }
}

fn frequency_type(t: i32) -> &'static str {
    match t {
        1 => "ATIS",
        7 | 14 | 15 => "CLR",
        5 => "GND",
        6 => "TWR",
        8 => "APP",
        9 => "DEP",
        10 => "CTR",
        2..=4 => "UNICOM",
        _ => "OTHER",
    }
}

fn approach_type(t: i32) -> &'static str {
    match t {
        1 => "GPS",
        2 | 8 => "VOR",
        3 | 9 => "NDB",
        4 => "ILS",
        5 | 11 => "LOC",
        10 => "RNAV",
        _ => "OTHER",
    }
}

fn approach_label(t: i32) -> &'static str {
    match t {
        1 => "GPS",
        2 => "VOR",
        3 => "NDB",
        4 => "ILS",
        5 => "LOC",
        6 => "SDF",
        7 => "LDA",
        8 => "VORDME",
        9 => "NDBDME",
        10 => "RNAV",
        11 => "LOC BC",
        _ => "APP",
    }
}

impl MsfsNavData {
    pub fn new(conn: Arc<Mutex<Option<Conn>>>) -> Self {
        Self { conn, pending: Mutex::new(HashMap::new()), ready: Condvar::new(), packets: Mutex::new(HashMap::new()), early: Mutex::new(HashMap::new()), next: Mutex::new(FIRST_REQUEST) }
    }

    /// Definicje pól – po każdym nowym połączeniu.
    pub fn define(c: &Conn) -> Result<(), String> {
        if !c.has_facilities() {
            return Err("SimConnect bez Facilities API".into());
        }
        let def = |d: u32, fields: &[&str]| -> Result<(), String> {
            for f in fields {
                let hr = c.add_facility(d, f);
                if hr < 0 {
                    return Err(format!("AddToFacilityDefinition({f}) = 0x{hr:08X}"));
                }
            }
            Ok(())
        };
        def(
            DEF_AIRPORT,
            &[
                "OPEN AIRPORT", "LATITUDE", "LONGITUDE", "ALTITUDE", "NAME64", "ICAO", "OPEN RUNWAY", "LATITUDE", "LONGITUDE", "ALTITUDE", "HEADING", "LENGTH", "WIDTH", "PRIMARY_ILS_ICAO",
                "PRIMARY_ILS_REGION", "PRIMARY_NUMBER", "PRIMARY_DESIGNATOR", "SECONDARY_ILS_ICAO", "SECONDARY_ILS_REGION", "SECONDARY_NUMBER", "SECONDARY_DESIGNATOR", "CLOSE RUNWAY",
                "OPEN FREQUENCY", "TYPE", "FREQUENCY", "NAME", "CLOSE FREQUENCY", "CLOSE AIRPORT",
            ],
        )?;
        let leg = ["TYPE", "FIX_ICAO", "FIX_REGION", "FIX_TYPE", "FIX_LATITUDE", "FIX_LONGITUDE", "FLY_OVER", "DISTANCE_MINUTE", "TRUE_DEGREE", "COURSE", "ROUTE_DISTANCE", "APPROACH_ALT_DESC", "ALTITUDE1", "ALTITUDE2", "SPEED_LIMIT"];
        let wrap = |name: &str| -> Vec<String> {
            let mut v = vec![format!("OPEN {name}")];
            v.extend(leg.iter().map(|s| s.to_string()));
            v.push(format!("CLOSE {name}"));
            v
        };
        let terminal = |name: &str| -> Vec<String> {
            let mut v = vec![format!("OPEN {name}"), "NAME".into(), "OPEN RUNWAY_TRANSITION".into(), "RUNWAY_NUMBER".into(), "RUNWAY_DESIGNATOR".into()];
            v.extend(wrap("APPROACH_LEG"));
            v.push("CLOSE RUNWAY_TRANSITION".into());
            v.push("OPEN ENROUTE_TRANSITION".into());
            v.push("NAME".into());
            v.extend(wrap("APPROACH_LEG"));
            v.push("CLOSE ENROUTE_TRANSITION".into());
            v.extend(wrap("APPROACH_LEG"));
            v.push(format!("CLOSE {name}"));
            v
        };
        let mut p: Vec<String> = vec!["OPEN AIRPORT".into(), "ICAO".into()];
        p.extend(terminal("DEPARTURE"));
        p.extend(terminal("ARRIVAL"));
        p.extend(["OPEN APPROACH", "TYPE", "SUFFIX", "RUNWAY_NUMBER", "RUNWAY_DESIGNATOR", "OPEN APPROACH_TRANSITION", "NAME", "IAF_ICAO"].iter().map(|s| s.to_string()));
        p.extend(wrap("APPROACH_LEG"));
        p.push("CLOSE APPROACH_TRANSITION".into());
        p.extend(wrap("FINAL_APPROACH_LEG"));
        p.extend(wrap("MISSED_APPROACH_LEG"));
        p.push("CLOSE APPROACH".into());
        p.push("CLOSE AIRPORT".into());
        let refs: Vec<&str> = p.iter().map(|s| s.as_str()).collect();
        def(DEF_PROCEDURES, &refs)?;
        def(DEF_VOR, &["OPEN VOR", "VOR_LATITUDE", "VOR_LONGITUDE", "FREQUENCY", "LOCALIZER", "GLIDE_SLOPE", "HAS_GLIDE_SLOPE", "IS_NAV", "CLOSE VOR"])?;
        def(DEF_NDB, &["OPEN NDB", "LATITUDE", "LONGITUDE", "FREQUENCY", "CLOSE NDB"])?;
        def(DEF_WAYPOINT, &["OPEN WAYPOINT", "LATITUDE", "LONGITUDE", "ICAO", "REGION", "CLOSE WAYPOINT"])?;
        def(DEF_AIRPORT_POS, &["OPEN AIRPORT", "LATITUDE", "LONGITUDE", "ICAO", "REGION", "CLOSE AIRPORT"])?;
        Ok(())
    }

    /// Rozłączenie: oczekujące zapytania kończą się błędem.
    pub fn detach(&self) {
        let mut p = self.pending.lock();
        for v in p.values_mut() {
            v.done = Some(Err(NavError::unavailable()));
        }
        self.ready.notify_all();
        self.packets.lock().clear();
    }

    fn alloc(&self) -> u32 {
        let mut n = self.next.lock();
        let id = *n;
        *n = n.wrapping_add(1).max(FIRST_REQUEST);
        id
    }

    fn wait(&self, id: u32, timeout: Duration) -> Option<Pending> {
        let deadline = Instant::now() + timeout;
        let mut p = self.pending.lock();
        loop {
            if p.get(&id).is_some_and(|x| x.done.is_some()) {
                return p.remove(&id);
            }
            if self.ready.wait_until(&mut p, deadline).timed_out() {
                return p.remove(&id);
            }
        }
    }

    fn request(&self, def: u32, icao: &str, region: &str, kind: Option<char>, timeout: Duration) -> NavResult<Reply> {
        let id = self.alloc();
        {
            let g = self.conn.lock();
            let Some(c) = g.as_ref() else { return Err(NavError::unavailable()) };
            self.pending.lock().insert(id, Pending::default());
            let hr = c.request_facility(def, id, icao, region, kind.map(|k| k as u8));
            if hr < 0 {
                self.pending.lock().remove(&id);
                return Err(not_found(format!("MSFS odrzucił zapytanie o {icao} (0x{hr:08X})")));
            }
            if let Some(packet) = c.last_packet() {
                self.packets.lock().insert(packet, id);
                if let Some(code) = self.early.lock().remove(&packet) {
                    if let Some(p) = self.pending.lock().get_mut(&id) {
                        p.done = Some(Err(not_found(format!("Brak {icao} w bazie MSFS (wyjątek {code})"))));
                    }
                }
            }
        }
        match self.wait(id, timeout) {
            Some(Pending { done: Some(Ok(())), nodes, candidates, .. }) => Ok(Reply { nodes, candidates }),
            Some(Pending { done: Some(Err(e)), .. }) => Err(e),
            _ => Err(not_found(format!("MSFS nie zwrócił danych dla {icao} w {} s", timeout.as_secs()))),
        }
    }

    fn request_list(&self, list_type: u32, fix_type: &str) -> Vec<FixResult> {
        let id = self.alloc();
        {
            let g = self.conn.lock();
            let Some(c) = g.as_ref() else { return Vec::new() };
            self.pending.lock().insert(id, Pending { list_type: fix_type.to_string(), ..Default::default() });
            if c.request_list(list_type, id) < 0 {
                self.pending.lock().remove(&id);
                return Vec::new();
            }
        }
        self.wait(id, LIST_TIMEOUT).map(|p| p.list).unwrap_or_default()
    }

    /// Wiadomość z kolejki SimConnect (wątek połączenia).
    pub fn on_message(&self, m: &Msg) {
        let size = m.data.len();
        match m.id {
            ffi::RECV_FACILITY_DATA if size >= 40 => {
                let req = m.u32(12);
                if let Some(p) = self.pending.lock().get_mut(&req) {
                    p.nodes.push(Node { kind: m.u32(24), id: m.u32(16), parent: m.u32(20), index: m.u32(32), data: m.data[40..].to_vec() });
                }
            }
            ffi::RECV_FACILITY_DATA_END if size >= 16 => {
                let req = m.u32(12);
                if let Some(p) = self.pending.lock().get_mut(&req) {
                    p.done.get_or_insert(Ok(()));
                }
                self.ready.notify_all();
            }
            ffi::RECV_FACILITY_MINIMAL_LIST if size >= 28 => {
                let (req, count, entry, out_of) = (m.u32(12), m.u32(16) as usize, m.u32(20), m.u32(24));
                let mut g = self.pending.lock();
                if let Some(p) = g.get_mut(&req) {
                    if count > 0 {
                        p.candidates.extend(parse_minimal(&m.data[28..], count));
                    }
                    if entry + 1 >= out_of {
                        p.done.get_or_insert(Ok(()));
                        drop(g);
                        self.ready.notify_all();
                    }
                }
            }
            ffi::RECV_AIRPORT_LIST | ffi::RECV_WAYPOINT_LIST | ffi::RECV_NDB_LIST | ffi::RECV_VOR_LIST if size >= 28 => {
                let (req, count, entry, out_of) = (m.u32(12), m.u32(16) as usize, m.u32(20), m.u32(24));
                let mut g = self.pending.lock();
                if let Some(p) = g.get_mut(&req) {
                    if count > 0 {
                        let t = p.list_type.clone();
                        p.list.extend(parse_list(&m.data[28..], count, &t));
                    }
                    if entry + 1 >= out_of {
                        p.done.get_or_insert(Ok(()));
                        drop(g);
                        self.ready.notify_all();
                    }
                }
            }
            ffi::RECV_EXCEPTION if size >= 24 => {
                let (exception, send_id) = (m.u32(12), m.u32(16));
                if let Some(req) = self.packets.lock().remove(&send_id) {
                    if let Some(p) = self.pending.lock().get_mut(&req) {
                        p.done = Some(Err(not_found(format!("Brak obiektu w bazie MSFS (wyjątek SimConnect {exception})"))));
                    }
                    self.ready.notify_all();
                } else if !self.pending.lock().is_empty() {
                    let mut e = self.early.lock();
                    e.insert(send_id, exception);
                    if e.len() > 64 {
                        e.clear();
                    }
                }
            }
            _ => {}
        }
    }

    fn ils(&self, icao: &str, region: &str) -> Option<IlsInfo> {
        let reply = self.request(DEF_VOR, icao, region, Some('V'), LOOKUP_TIMEOUT).ok()?;
        let t = Tree { nodes: &reply.nodes };
        let vor = t.root(T_VOR)?;
        let mut c = Cursor::new(&vor.data);
        c.f64();
        c.f64();
        let freq = c.u32();
        let loc = c.f32() as f64;
        let gs = c.f32() as f64;
        let has_gs = c.i32() != 0;
        Some(IlsInfo {
            ident: icao.to_string(),
            freq_mhz: freq_mhz(freq).unwrap_or(0.0),
            loc_course_deg: math_round(math::norm_deg(loc), 1),
            gs_angle_deg: (has_gs && gs > 0.0).then(|| math_round(gs, 2)),
        })
    }

    fn lookup(&self, ident: &str, region: Option<&str>, kind: char) -> Vec<FixResult> {
        let def = match kind {
            'V' => DEF_VOR,
            'N' => DEF_NDB,
            _ => DEF_WAYPOINT,
        };
        let Ok(reply) = self.request(def, ident, region.unwrap_or(""), Some(kind), LOOKUP_TIMEOUT) else { return Vec::new() };
        if !reply.candidates.is_empty() && region.is_none() {
            return reply
                .candidates
                .iter()
                .filter(|c| c.kind == kind || c.kind == '\0')
                .take(8)
                .flat_map(|c| {
                    let found = self.lookup(ident, Some(&c.region), kind);
                    if found.is_empty() {
                        vec![FixResult { ident: ident.into(), kind: fix_type_name(kind).into(), lat: c.lat, lon: c.lon, freq_mhz: None, region: Some(c.region.clone()) }]
                    } else {
                        found
                    }
                })
                .collect();
        }
        let root_type = match kind {
            'V' => T_VOR,
            'N' => T_NDB,
            _ => T_WAYPOINT,
        };
        let t = Tree { nodes: &reply.nodes };
        let Some(root) = t.root(root_type) else { return Vec::new() };
        let mut c = Cursor::new(&root.data);
        let (lat, lon) = (c.f64(), c.f64());
        let mut reg = region.filter(|r| !r.is_empty()).map(str::to_string);
        let mhz = if kind == 'V' || kind == 'N' {
            freq_mhz(c.u32())
        } else {
            c.str(8);
            let r = c.str(8);
            if !r.is_empty() {
                reg = Some(r);
            }
            None
        };
        if lat == 0.0 && lon == 0.0 {
            return Vec::new();
        }
        vec![FixResult { ident: ident.into(), kind: fix_type_name(kind).into(), lat: math_round(lat, 6), lon: math_round(lon, 6), freq_mhz: mhz, region: reg }]
    }

    fn lookup_airport(&self, ident: &str) -> Vec<FixResult> {
        if ident.len() < 3 {
            return Vec::new();
        }
        let Ok(reply) = self.request(DEF_AIRPORT_POS, ident, "", None, LOOKUP_TIMEOUT) else { return Vec::new() };
        let t = Tree { nodes: &reply.nodes };
        let Some(root) = t.root(T_AIRPORT) else { return Vec::new() };
        let mut c = Cursor::new(&root.data);
        let (lat, lon) = (c.f64(), c.f64());
        let icao = c.str(8);
        let region = c.str(8);
        vec![FixResult {
            ident: if icao.is_empty() { ident.into() } else { icao },
            kind: ft::AIRPORT.into(),
            lat: math_round(lat, 6),
            lon: math_round(lon, 6),
            freq_mhz: None,
            region: (!region.is_empty()).then_some(region),
        }]
    }

    fn legs(&self, t: &Tree, parent: &Node, kind: u32, cache: &mut HashMap<String, Option<(f64, f64)>>) -> Vec<LegInfo> {
        let mut out = Vec::new();
        for n in t.children(parent, kind) {
            let mut c = Cursor::new(&n.data);
            let leg_type = c.i32();
            let fix = c.str(8);
            let fix_region = c.str(8);
            let fix_type = (c.i32() as u8) as char;
            let (lat, lon) = (c.f64(), c.f64());
            let fly_over = c.i32() != 0;
            let minutes = c.i32() != 0;
            c.i32();
            let course = c.f32() as f64;
            let route_distance = c.f32() as f64;
            let alt_desc = c.i32();
            let alt1 = c.f32() as f64;
            let alt2 = c.f32() as f64;
            let speed = c.f32() as f64;
            let (mut flat, mut flon) = (None, None);
            if !fix.is_empty() {
                if lat != 0.0 || lon != 0.0 {
                    flat = Some(math_round(lat, 6));
                    flon = Some(math_round(lon, 6));
                } else {
                    let key = format!("{fix_type}|{fix}|{fix_region}");
                    let pos = *cache.entry(key).or_insert_with(|| {
                        if matches!(fix_type, 'W' | 'V' | 'N') {
                            self.lookup(&fix, Some(&fix_region), fix_type).first().map(|f| (f.lat, f.lon))
                        } else {
                            None
                        }
                    });
                    if let Some((a, b)) = pos {
                        flat = Some(math_round(a, 6));
                        flon = Some(math_round(b, 6));
                    }
                }
            }
            let desc = match alt_desc {
                1 => Some("AT"),
                2 => Some("ABOVE"),
                3 => Some("BELOW"),
                4 => Some("BETWEEN"),
                _ => None,
            };
            let lt = LEG_TYPES.get(leg_type as usize).copied().unwrap_or("UNKNOWN");
            out.push(LegInfo {
                fix,
                lat: flat,
                lon: flon,
                leg_type: lt.into(),
                alt_desc: desc.map(str::to_string),
                alt1_ft: desc.map(|_| math_round(alt1 * math::FT_PER_M, 0)),
                alt2_ft: (desc == Some("BETWEEN")).then(|| math_round(alt2 * math::FT_PER_M, 0)),
                speed_kt: (speed > 0.0).then(|| math_round(speed, 0)),
                course_deg: (has_course(lt) || course != 0.0).then(|| math_round(math::norm_deg(course), 1)),
                dist_nm: (!minutes && route_distance > 0.0).then(|| math_round(route_distance / math::M_PER_NM, 1)),
                fly_over,
            });
        }
        out
    }
}

fn parse_minimal(data: &[u8], count: usize) -> Vec<Candidate> {
    let entry = data.len() / count.max(1);
    let icao_size = entry.saturating_sub(24);
    if icao_size < 10 {
        return Vec::new();
    }
    let ident_len = (icao_size - 4) / 2;
    (0..count)
        .filter_map(|i| {
            let o = i * entry;
            let e = data.get(o..o + entry)?;
            let kind = e[0] as char;
            let region = ascii(&e[1 + ident_len..1 + ident_len + 3]);
            let lat = f64::from_le_bytes(e[icao_size..icao_size + 8].try_into().ok()?);
            let lon = f64::from_le_bytes(e[icao_size + 8..icao_size + 16].try_into().ok()?);
            Some(Candidate { kind, region, lat, lon })
        })
        .collect()
}

fn parse_list(data: &[u8], count: usize, fix_type: &str) -> Vec<FixResult> {
    let entry = data.len() / count.max(1);
    let tail = match fix_type {
        ft::AIRPORT => 0,
        ft::WAYPOINT => 4,
        ft::NDB => 8,
        _ => 44,
    };
    let ident_len = entry as i64 - 3 - 24 - tail;
    if !(4..=16).contains(&ident_len) {
        return Vec::new();
    }
    let il = ident_len as usize;
    (0..count)
        .filter_map(|i| {
            let o = i * entry;
            let e = data.get(o..o + entry)?;
            let ident = ascii(&e[..il]);
            let region = ascii(&e[il..il + 3]);
            let lat = f64::from_le_bytes(e[il + 3..il + 11].try_into().ok()?);
            let lon = f64::from_le_bytes(e[il + 11..il + 19].try_into().ok()?);
            let mhz = if fix_type == ft::NDB || fix_type == ft::VOR {
                e.get(il + 3 + 24 + 4..il + 3 + 24 + 8).and_then(|b| freq_mhz(u32::from_le_bytes(b.try_into().ok()?)))
            } else {
                None
            };
            if ident.is_empty() || (lat == 0.0 && lon == 0.0) {
                return None;
            }
            Some(FixResult { ident, kind: fix_type.into(), lat: math_round(lat, 6), lon: math_round(lon, 6), freq_mhz: mhz, region: (!region.is_empty()).then_some(region) })
        })
        .collect()
}

impl NavDataProvider for MsfsNavData {
    fn airport(&self, icao: &str) -> NavResult<AirportInfo> {
        let reply = self.request(DEF_AIRPORT, icao, "", None, REQUEST_TIMEOUT)?;
        let t = Tree { nodes: &reply.nodes };
        let root = t.root(T_AIRPORT).ok_or_else(|| not_found(format!("Brak lotniska {icao} w bazie MSFS")))?;
        let mut c = Cursor::new(&root.data);
        let (lat, lon, alt_m) = (c.f64(), c.f64(), c.f64());
        let name = c.str(64);
        let ident = c.str(8);
        let mut runways = Vec::new();
        let mut ils_refs: Vec<(usize, String, String)> = Vec::new();
        for rw in t.children(root, T_RUNWAY) {
            let mut r = Cursor::new(&rw.data);
            let (clat, clon, calt) = (r.f64(), r.f64(), r.f64());
            let heading = r.f32() as f64;
            let length_m = r.f32() as f64;
            let width_m = r.f32() as f64;
            let (p_ils, p_reg) = (r.str(8), r.str(8));
            let (p_num, p_des) = (r.i32(), r.i32());
            let (s_ils, s_reg) = (r.str(8), r.str(8));
            let (s_num, s_des) = (r.i32(), r.i32());
            let mut add_end = |num: i32, des: i32, hdg: f64, ils: String, reg: String| {
                let id = math::runway_ident(num, des);
                if id.is_empty() {
                    return;
                }
                let (tlat, tlon) = math::offset(clat, clon, hdg + 180.0, length_m / 2.0);
                if !ils.is_empty() {
                    ils_refs.push((runways.len(), ils, reg));
                }
                runways.push(RunwayInfo {
                    ident: id,
                    heading_true_deg: math_round(math::norm_deg(hdg), 1),
                    length_ft: math_round(length_m * math::FT_PER_M, 0),
                    width_ft: math_round(width_m * math::FT_PER_M, 0),
                    threshold_lat: math_round(tlat, 6),
                    threshold_lon: math_round(tlon, 6),
                    elev_ft: math_round(calt * math::FT_PER_M, 0),
                    ils: None,
                });
            };
            add_end(p_num, p_des, heading, p_ils, p_reg);
            add_end(s_num, s_des, heading + 180.0, s_ils, s_reg);
        }
        let mut frequencies = Vec::new();
        for f in t.children(root, T_FREQUENCY) {
            let mut c = Cursor::new(&f.data);
            let kind = c.i32();
            let hz = c.i32();
            let fname = c.str(64);
            if hz > 0 {
                frequencies.push(FrequencyInfo { kind: frequency_type(kind).into(), name: fname, mhz: math_round(hz as f64 / 1_000_000.0, 3) });
            }
        }
        for (i, ils, reg) in ils_refs {
            runways[i].ils = self.ils(&ils, &reg);
        }
        Ok(AirportInfo {
            icao: if ident.is_empty() { icao.into() } else { ident },
            name,
            lat: math_round(lat, 6),
            lon: math_round(lon, 6),
            elev_ft: math_round(alt_m * math::FT_PER_M, 0),
            runways,
            frequencies,
        })
    }

    fn procedures(&self, icao: &str) -> NavResult<AirportProcedures> {
        let reply = self.request(DEF_PROCEDURES, icao, "", None, REQUEST_TIMEOUT)?;
        let t = Tree { nodes: &reply.nodes };
        let root = t.root(T_AIRPORT).ok_or_else(|| not_found(format!("Brak lotniska {icao} w bazie MSFS")))?;
        let id = Cursor::new(&root.data).str(8);
        let mut cache = HashMap::new();
        let mut result = AirportProcedures { icao: if id.is_empty() { icao.into() } else { id }, ..Default::default() };
        for (kind, is_dep) in [(T_DEPARTURE, true), (T_ARRIVAL, false)] {
            for p in t.children(root, kind) {
                let mut proc = TerminalProcedure { name: Cursor::new(&p.data).str(8), ..Default::default() };
                for rt in t.children(p, T_RUNWAY_TRANSITION) {
                    let mut c = Cursor::new(&rt.data);
                    let ident = math::runway_ident(c.i32(), c.i32());
                    let legs = self.legs(&t, rt, T_APPROACH_LEG, &mut cache);
                    proc.runway_transitions.push(NamedLegs::new(if ident.is_empty() { "ALL" } else { &ident }, legs));
                }
                for et in t.children(p, T_ENROUTE_TRANSITION) {
                    let name = Cursor::new(&et.data).str(8);
                    let legs = self.legs(&t, et, T_APPROACH_LEG, &mut cache);
                    proc.enroute_transitions.push(NamedLegs::new(&name, legs));
                }
                proc.common_legs = self.legs(&t, p, T_APPROACH_LEG, &mut cache);
                if is_dep {
                    result.departures.push(proc);
                } else {
                    result.arrivals.push(proc);
                }
            }
        }
        let mut names = HashSet::new();
        for a in t.children(root, T_APPROACH) {
            let mut c = Cursor::new(&a.data);
            let kind = c.i32();
            let suffix_code = c.i32();
            let runway = math::runway_ident(c.i32(), c.i32());
            let suffix = (33..127).contains(&suffix_code).then(|| ((suffix_code as u8) as char).to_string());
            let label = approach_label(kind);
            let base = format!("{label}{}{}", suffix.as_ref().map(|s| format!(" {s}")).unwrap_or_default(), if runway.is_empty() { String::new() } else { format!(" {runway}") });
            let mut name = base.clone();
            let mut n = 2;
            while !names.insert(name.clone()) {
                name = format!("{base} ({n})").replace("  ", " ");
                n += 1;
            }
            let mut ap = ApproachProcedure { name, kind: approach_type(kind).into(), runway: (!runway.is_empty()).then_some(runway), suffix, ..Default::default() };
            for tr in t.children(a, T_APPROACH_TRANSITION) {
                let mut c = Cursor::new(&tr.data);
                let tname = c.str(8);
                let iaf = c.str(8);
                let legs = self.legs(&t, tr, T_APPROACH_LEG, &mut cache);
                ap.transitions.push(NamedLegs::new(if tname.is_empty() { &iaf } else { &tname }, legs));
            }
            ap.final_legs = self.legs(&t, a, T_FINAL_APPROACH_LEG, &mut cache);
            ap.missed_legs = self.legs(&t, a, T_MISSED_APPROACH_LEG, &mut cache);
            result.approaches.push(ap);
        }
        Ok(result)
    }

    fn find_fix(&self, ident: &str) -> NavResult<Vec<FixResult>> {
        let mut all = Vec::new();
        for k in ['W', 'V', 'N'] {
            all.extend(self.lookup(ident, None, k));
        }
        all.extend(self.lookup_airport(ident));
        let mut seen = HashSet::new();
        all.retain(|f| seen.insert((f.kind.clone(), f.region.clone(), (f.lat * 1000.0).round() as i64, (f.lon * 1000.0).round() as i64)));
        Ok(all)
    }

    fn nearby(&self, _lat: f64, _lon: f64, _radius: f64) -> NavResult<Vec<FixResult>> {
        if self.conn.lock().is_none() {
            return Err(NavError::unavailable());
        }
        let mut out = Vec::new();
        for (t, kind) in [(0, ft::AIRPORT), (1, ft::WAYPOINT), (2, ft::NDB), (3, ft::VOR)] {
            out.extend(self.request_list(t, kind));
        }
        Ok(out)
    }
}
