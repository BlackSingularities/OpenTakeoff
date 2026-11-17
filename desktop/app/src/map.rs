//! Mapa rysowana w Rust: kafelki rastrowe (CARTO na podkładzie OpenStreetMap albo OSM) w odwzorowaniu
//! Mercatora z pamięcią podręczną na dysku i w pamięci, a na nich ślad lotu w kolorach wysokości, samolot,
//! lotniska, pasy i znaczniki. Etykiety rysuje Slint (ostry tekst), stąd lista etykiet obok obrazu.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use tiny_skia::{Color, FillRule, FilterQuality, LineCap, LineJoin, Paint, PathBuilder, Pattern, Pixmap, PixmapPaint, SpreadMode, Stroke, Transform};

const TILE: f64 = 256.0;
const MAX_ZOOM: f64 = 18.0;
const MEMORY_TILES: usize = 600;
/// Największe pochylenie mapy (°) i połowa kąta widzenia kamery w pionie.
pub const MAX_TILT: f64 = 60.0;
const HALF_FOV: f64 = 25.0;

/// Pochylanie map włączone w ustawieniach; wyłączone – każda mapa jest płaska (widok z góry).
static TILT_ON: AtomicBool = AtomicBool::new(true);

pub fn set_tilt_enabled(on: bool) {
    TILT_ON.store(on, Ordering::SeqCst);
}

pub fn tilt_enabled() -> bool {
    TILT_ON.load(Ordering::SeqCst)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    Dark,
    Light,
    Osm,
    Satellite,
    /// Opady z radaru RainViewer (przezroczyste kafelki na podkład, bez zapisu na dysk).
    Radar,
}

/// Bieżąca klatka radaru: (adres serwera, ścieżka klatki).
static RADAR: Mutex<Option<(String, String)>> = Mutex::new(None);

/// Ustawia klatkę radaru; przy zmianie usuwa stare kafelki radaru z pamięci.
pub fn set_radar_frame(host: &str, path: &str) {
    let mut r = RADAR.lock();
    if r.as_ref().is_some_and(|(h, p)| h == host && p == path) {
        return;
    }
    *r = Some((host.to_string(), path.to_string()));
    drop(r);
    let t = tiles();
    t.mem.lock().map.retain(|k, _| k.0 != Style::Radar);
    t.failed.lock().retain(|k, _| k.0 != Style::Radar);
    TILES_SERIAL.fetch_add(1, Ordering::SeqCst);
}

impl Style {
    fn key(self) -> &'static str {
        match self {
            Style::Dark => "esri-dark",
            Style::Light => "esri-light",
            Style::Osm => "osm",
            Style::Satellite => "esri-sat",
            Style::Radar => "radar",
        }
    }
    fn url(self, z: u32, x: u32, y: u32) -> String {
        const ESRI: &str = "https://services.arcgisonline.com/ArcGIS/rest/services";
        match self {
            Style::Dark => format!("{ESRI}/Canvas/World_Dark_Gray_Base/MapServer/tile/{z}/{y}/{x}"),
            Style::Light => format!("{ESRI}/Canvas/World_Light_Gray_Base/MapServer/tile/{z}/{y}/{x}"),
            Style::Satellite => format!("{ESRI}/World_Imagery/MapServer/tile/{z}/{y}/{x}"),
            Style::Osm => format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png"),
            Style::Radar => match RADAR.lock().as_ref() {
                Some((host, path)) => format!("{host}{path}/256/{z}/{x}/{y}/2/1_1.png"),
                None => String::new(),
            },
        }
    }
    /// Najwyższy poziom kafelków danego podkładu (wyżej – powiększony kafelek).
    fn max_zoom(self) -> u32 {
        match self {
            Style::Dark | Style::Light => 16,
            Style::Satellite => 18,
            Style::Osm => 19,
            // darmowy dostęp RainViewer kończy się na poziomie 7 – wyżej kafelek jest powiększany
            Style::Radar => 7,
        }
    }
    pub fn attribution(self) -> &'static str {
        match self {
            Style::Osm => "© OpenStreetMap",
            Style::Radar => "RainViewer",
            Style::Satellite => "© Esri, Maxar, Earthstar Geographics",
            _ => "© Esri, HERE, Garmin, © OpenStreetMap",
        }
    }
}

type Key = (Style, u32, u32, u32);

/// Kafelki w pamięci z „zegarem” ostatniego użycia – przy przepełnieniu wypada najdawniej używany
/// (nie najdawniej wczytany: pochylona mapa trzyma naraz kafelki wielu poziomów i FIFO wyrzucało
/// te, które właśnie są na ekranie, przez co znikały i wczytywały się w kółko).
struct Mem {
    map: HashMap<Key, (Arc<Pixmap>, u64)>,
    clock: u64,
}

struct Tiles {
    mem: Mutex<Mem>,
    pending: Mutex<HashSet<Key>>,
    /// Nieudane pobrania z czasem – po [`RETRY_FAILED`] próba jest ponawiana.
    failed: Mutex<HashMap<Key, Instant>>,
    /// Zlecenia: najnowsze na końcu i obsługiwane najpierw (to, co jest teraz na ekranie), najstarsze
    /// odpadają ponad [`MAX_QUEUE`] – przy przesuwaniu i pochylaniu nie narasta zaległa kolejka.
    queue: Mutex<VecDeque<Key>>,
    ready: Condvar,
}

const MAX_QUEUE: usize = 256;
const RETRY_FAILED: Duration = Duration::from_secs(30);

/// Rośnie, gdy dotrze nowy kafelek – widoki rysują się wtedy od nowa.
pub static TILES_SERIAL: AtomicU64 = AtomicU64::new(0);

/// PNG albo JPEG → obraz tiny-skia.
fn decode(b: &[u8]) -> Option<Pixmap> {
    if let Ok(p) = Pixmap::decode_png(b) {
        return Some(p);
    }
    let img = image::load_from_memory(b).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let mut p = Pixmap::new(w, h)?;
    p.data_mut().copy_from_slice(&img.into_raw());
    Some(p)
}

fn tile_path(k: &Key) -> PathBuf {
    opentakeoff_core::paths::cache().join("tiles").join(k.0.key()).join(k.1.to_string()).join(k.2.to_string()).join(format!("{}.png", k.3))
}

fn tiles() -> &'static Tiles {
    static T: OnceLock<Tiles> = OnceLock::new();
    T.get_or_init(|| {
        for i in 0..4 {
            let _ = std::thread::Builder::new().name(format!("tiles-{i}")).spawn(move || {
                let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).user_agent(&format!("OpenTakeoff-Windows/{} (+https://redzikowski.com)", opentakeoff_core::version())).build();
                let t = tiles();
                loop {
                    let k = {
                        let mut q = t.queue.lock();
                        loop {
                            if let Some(k) = q.pop_back() {
                                break k;
                            }
                            t.ready.wait(&mut q);
                        }
                    };
                    let path = tile_path(&k);
                    let disk = k.0 != Style::Radar;
                    let bytes = std::fs::read(&path).ok().filter(|_| disk).or_else(|| {
                        let url = k.0.url(k.1, k.2, k.3);
                        if url.is_empty() {
                            return None;
                        }
                        let resp = agent.get(&url).call().ok()?;
                        if !resp.content_type().starts_with("image/") {
                            return None;
                        }
                        let mut b = Vec::new();
                        std::io::Read::read_to_end(&mut resp.into_reader(), &mut b).ok()?;
                        if disk {
                            if let Some(d) = path.parent() {
                                let _ = std::fs::create_dir_all(d);
                            }
                            let _ = std::fs::write(&path, &b);
                        }
                        Some(b)
                    });
                    match bytes.and_then(|b| decode(&b)) {
                        Some(p) => {
                            let mut m = t.mem.lock();
                            m.clock += 1;
                            let now = m.clock;
                            m.map.insert(k, (Arc::new(p), now));
                            while m.map.len() > MEMORY_TILES {
                                let Some(old) = m.map.iter().min_by_key(|(_, v)| v.1).map(|(k, _)| *k) else { break };
                                m.map.remove(&old);
                            }
                        }
                        None => {
                            t.failed.lock().insert(k, Instant::now());
                        }
                    }
                    t.pending.lock().remove(&k);
                    TILES_SERIAL.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
        Tiles {
            mem: Mutex::new(Mem { map: HashMap::new(), clock: 0 }),
            pending: Mutex::new(HashSet::new()),
            failed: Mutex::new(HashMap::new()),
            queue: Mutex::new(VecDeque::new()),
            ready: Condvar::new(),
        }
    })
}

/// Zlecenie pobrania na szczyt kolejki (także ponowione – przesuwa się wtedy na szczyt).
fn request(k: Key) {
    let t = tiles();
    let fresh = t.pending.lock().insert(k);
    let mut dropped = None;
    {
        let mut q = t.queue.lock();
        if !fresh {
            match q.iter().rposition(|x| *x == k) {
                // już pobierany przez wątek – nic do zrobienia
                None => return,
                Some(i) if i + 1 == q.len() => return,
                Some(i) => {
                    q.remove(i);
                }
            }
        }
        q.push_back(k);
        if q.len() > MAX_QUEUE {
            dropped = q.pop_front();
        }
    }
    if let Some(old) = dropped {
        t.pending.lock().remove(&old);
    }
    t.ready.notify_one();
}

/// Kafelek z pamięci; brak – zlecenie pobrania (z dysku albo z sieci) i None.
fn tile(k: Key) -> Option<Arc<Pixmap>> {
    if let Some(p) = cached(k) {
        return Some(p);
    }
    let t = tiles();
    {
        let mut f = t.failed.lock();
        match f.get(&k) {
            Some(at) if at.elapsed() < RETRY_FAILED => return None,
            Some(_) => {
                f.remove(&k);
            }
            None => {}
        }
    }
    request(k);
    None
}

/// Kafelek z pamięci (bez zlecania pobrania); odświeża jego czas użycia.
fn cached(k: Key) -> Option<Arc<Pixmap>> {
    let mut m = tiles().mem.lock();
    m.clock += 1;
    let now = m.clock;
    m.map.get_mut(&k).map(|v| {
        v.1 = now;
        v.0.clone()
    })
}

/// Kafelek do teksturowania (widok 3D): obraz i fragment (x, y, rozmiar w pikselach obrazu). Gdy kafelka
/// jeszcze nie ma, zleca pobranie i zwraca powiększony fragment kafelka z niższego poziomu – szukanego
/// aż do samej góry drzewa, a gdy żadnego nie ma, zleca też ogólniejszy kafelek (wczytuje się szybciej
/// i od razu wypełnia dziurę, zanim dotrze szczegółowy).
pub fn texture(style: Style, z: u32, x: u32, y: u32) -> Option<(Arc<Pixmap>, f32, f32, f32)> {
    let z = z.min(style.max_zoom());
    if let Some(t) = tile((style, z, x, y)) {
        return Some((t, 0.0, 0.0, TILE as f32));
    }
    for up in 1..=z {
        let (pz, px, py) = (z - up, x >> up, y >> up);
        if let Some(t) = cached((style, pz, px, py)) {
            let f = (1u32 << up) as f32;
            let size = TILE as f32 / f;
            return Some((t, (x - (px << up)) as f32 * size, (y - (py << up)) as f32 * size, size));
        }
    }
    let up = z.min(3);
    if up > 0 {
        let _ = tile((style, z - up, x >> up, y >> up));
    }
    None
}

// ---------- odwzorowanie ----------

fn world(z: f64) -> f64 {
    TILE * 2f64.powf(z)
}

/// Współrzędne świata (piksele przy zoomie z).
pub fn project(lat: f64, lon: f64, z: f64) -> (f64, f64) {
    let lat = lat.clamp(-85.05, 85.05).to_radians();
    let w = world(z);
    ((lon + 180.0) / 360.0 * w, (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * w)
}

pub fn unproject(x: f64, y: f64, z: f64) -> (f64, f64) {
    let w = world(z);
    let lon = x / w * 360.0 - 180.0;
    let n = std::f64::consts::PI * (1.0 - 2.0 * y / w);
    (n.sinh().atan().to_degrees(), lon)
}

#[derive(Clone, Debug)]
pub struct View {
    pub lat: f64,
    pub lon: f64,
    pub zoom: f64,
    /// Rozmiar w pikselach logicznych.
    pub w: f64,
    pub h: f64,
    pub follow: bool,
    pub user_moved: bool,
    /// Pochylenie (°): 0 – widok z góry, do [`MAX_TILT`] – mapa oglądana pod kątem.
    pub tilt: f64,
    /// Obrót mapy (°, 0–360): kierunek geograficzny, który jest u góry ekranu (0 – północ).
    pub bearing: f64,
}

impl Default for View {
    fn default() -> Self {
        Self { lat: 52.0, lon: 19.0, zoom: 5.5, w: 600.0, h: 400.0, follow: true, user_moved: false, tilt: 0.0, bearing: 0.0 }
    }
}

/// Przewyższenie wysokości trasy nad mapą (w skali 1:1 wysokość przelotowa ginie wobec długości trasy).
const ALT_EXAGGERATION: f64 = 4.0;

impl View {
    /// Mapa obrócona (północ nie u góry).
    pub fn rotated(&self) -> bool {
        let b = self.bearing.rem_euclid(360.0);
        b > 0.05 && b < 359.95
    }

    /// Mapa pochylona albo obrócona – kafelki i rysunki „na ziemi” trzeba rzutować, nie wystarczy przesunięcie.
    pub fn projected(&self) -> bool {
        self.tilted() || self.rotated()
    }

    /// Obrót przeciąganiem w poziomie (prawy przycisk). Zwraca, czy się zmienił.
    pub fn rotate_by(&mut self, dx: f64) -> bool {
        if dx.abs() < 1e-9 {
            return false;
        }
        self.bearing = (self.bearing - dx * 0.4).rem_euclid(360.0);
        true
    }

    /// Kompas: z powrotem północą do góry.
    pub fn reset_bearing(&mut self) {
        self.bearing = 0.0;
    }

    /// Przesunięcie na płaskiej mapie (wschód, południe) → układ obróconej mapy (w prawo, w dół ekranu).
    fn rot(&self, fx: f64, fy: f64) -> (f64, f64) {
        if !self.rotated() {
            return (fx, fy);
        }
        let (s, c) = self.bearing.to_radians().sin_cos();
        (fx * c + fy * s, -fx * s + fy * c)
    }

    fn unrot(&self, rx: f64, ry: f64) -> (f64, f64) {
        if !self.rotated() {
            return (rx, ry);
        }
        let (s, c) = self.bearing.to_radians().sin_cos();
        (rx * c - ry * s, rx * s + ry * c)
    }

    /// Kąt na ekranie dla kierunku geograficznego `deg` (do obracania symboli).
    pub fn screen_deg(&self, deg: f64) -> f64 {
        deg - self.bearing
    }

    /// Wysokość (ft) w pikselach logicznych mapy przy bieżącym zoomie, z przewyższeniem.
    pub fn alt_px(&self, alt_ft: f64) -> f64 {
        let m_per_px = 156_543.03392 * self.lat.to_radians().cos().max(0.05) / 2f64.powf(self.zoom);
        alt_ft.max(0.0) * 0.3048 / m_per_px * ALT_EXAGGERATION
    }
    /// Pochylenie użyte do rysowania: 0, gdy pochylanie jest wyłączone w ustawieniach.
    pub fn tilt_deg(&self) -> f64 {
        if tilt_enabled() && self.tilt >= 0.5 {
            self.tilt.min(MAX_TILT)
        } else {
            0.0
        }
    }

    pub fn tilted(&self) -> bool {
        self.tilt_deg() > 0.0
    }

    /// Ogniskowa kamery w pikselach logicznych; w środku ekranu skala jest taka sama jak bez pochylenia.
    fn focal(&self) -> f64 {
        (self.h / 2.0) / HALF_FOV.to_radians().tan()
    }

    /// Przesunięcie na płaskiej mapie względem środka (px logiczne, y w dół) → punkt ekranu.
    /// Kamera patrzy na środek mapy z odległości równej ogniskowej, pochylona o `tilt` od pionu.
    pub fn flat_to_screen(&self, fx: f64, fy: f64) -> (f64, f64) {
        self.flat_to_screen_h(fx, fy, 0.0)
    }

    /// Jak [`Self::flat_to_screen`], ale punkt wisi `hp` pikseli logicznych nad ziemią (widać to tylko
    /// na pochylonej mapie – z góry wysokość nie przesuwa punktu).
    pub fn flat_to_screen_h(&self, fx: f64, fy: f64, hp: f64) -> (f64, f64) {
        let (fx, fy) = self.rot(fx, fy);
        let t = self.tilt_deg();
        if t == 0.0 {
            return (self.w / 2.0 + fx, self.h / 2.0 + fy);
        }
        let (s, c) = t.to_radians().sin_cos();
        let d = self.focal();
        let gy = -fy;
        // punkty za kamerą lądują daleko pod ekranem (nie zawijają się na drugą stronę)
        let z = (gy * s - hp * c + d).max(d * 0.04);
        (self.w / 2.0 + d * fx / z, self.h / 2.0 - d * (gy * c + hp * s) / z)
    }

    /// Punkt ekranu → przesunięcie na płaskiej mapie względem środka. Nad horyzontem (i tuż pod nim)
    /// wynik jest ograniczony do najdalszego sensownego miejsca.
    pub fn screen_to_flat(&self, x: f64, y: f64) -> (f64, f64) {
        let t = self.tilt_deg();
        if t == 0.0 {
            return self.unrot(x - self.w / 2.0, y - self.h / 2.0);
        }
        let (s, c) = t.to_radians().sin_cos();
        let d = self.focal();
        let xx = (x - self.w / 2.0) / d;
        let yy = ((self.h / 2.0 - y) / d).min((c - 0.08) / s);
        let gy = yy * d / (c - yy * s);
        self.unrot(xx * (gy * s + d), -gy)
    }

    /// Szerokość i długość geograficzna punktu ekranu (także na pochylonej mapie).
    pub fn from_screen(&self, x: f64, y: f64) -> (f64, f64) {
        let (fx, fy) = self.screen_to_flat(x, y);
        let (cx, cy) = project(self.lat, self.lon, self.zoom);
        let (lat, lon) = unproject(cx + fx, cy + fy, self.zoom);
        (lat, ((lon + 540.0) % 360.0) - 180.0)
    }

    pub fn to_screen(&self, lat: f64, lon: f64) -> (f64, f64) {
        self.to_screen_alt(lat, lon, 0.0)
    }

    /// Punkt na wysokości `alt_ft` nad ziemią (z przewyższeniem [`ALT_EXAGGERATION`]).
    pub fn to_screen_alt(&self, lat: f64, lon: f64, alt_ft: f64) -> (f64, f64) {
        let (cx, cy) = project(self.lat, self.lon, self.zoom);
        let (x, y) = project(lat, lon, self.zoom);
        let w = world(self.zoom);
        let mut dx = x - cx;
        if dx > w / 2.0 {
            dx -= w;
        } else if dx < -w / 2.0 {
            dx += w;
        }
        let hp = if alt_ft > 0.0 && self.tilted() { self.alt_px(alt_ft) } else { 0.0 };
        self.flat_to_screen_h(dx, y - cy, hp)
    }

    /// Przeciągnięcie myszą z (x - dx, y - dy) do (x, y): punkt mapy pod wskaźnikiem idzie za nim,
    /// także na pochylonej i obróconej mapie.
    pub fn drag(&mut self, x: f64, y: f64, dx: f64, dy: f64) {
        if !self.projected() {
            self.pan(dx, dy);
            return;
        }
        let a = self.screen_to_flat(x - dx, y - dy);
        let b = self.screen_to_flat(x, y);
        let lim = (self.w + self.h) * 2.0;
        self.pan((b.0 - a.0).clamp(-lim, lim), (b.1 - a.1).clamp(-lim, lim));
    }

    /// Zmiana pochylenia przeciąganiem w pionie (w górę – bardziej pochylona). Zwraca, czy się zmieniło.
    pub fn tilt_by(&mut self, dy: f64) -> bool {
        if !tilt_enabled() {
            return false;
        }
        let old = self.tilt;
        let t = (self.tilt - dy * 0.3).clamp(0.0, MAX_TILT);
        self.tilt = if t < 2.0 { 0.0 } else { t };
        (self.tilt - old).abs() > 1e-6
    }

    /// Przycisk pochylenia: płasko ↔ 45°.
    pub fn toggle_tilt(&mut self) {
        self.tilt = if self.tilted() { 0.0 } else { 45.0 };
    }

    pub fn pan(&mut self, dx: f64, dy: f64) {
        let (cx, cy) = project(self.lat, self.lon, self.zoom);
        let (lat, lon) = unproject(cx - dx, cy - dy, self.zoom);
        self.lat = lat;
        self.lon = ((lon + 540.0) % 360.0) - 180.0;
        self.user_moved = true;
    }

    /// Przybliżenie kółkiem: punkt pod wskaźnikiem zostaje na miejscu.
    pub fn zoom_at(&mut self, delta: f64, mx: f64, my: f64) {
        let (gx, gy) = self.screen_to_flat(mx, my);
        let (lat, lon) = {
            let (cx, cy) = project(self.lat, self.lon, self.zoom);
            unproject(cx + gx, cy + gy, self.zoom)
        };
        let nz = (self.zoom + delta / 120.0 * 0.5).clamp(1.5, MAX_ZOOM);
        self.zoom = nz;
        let (px, py) = project(lat, lon, nz);
        let (lat2, lon2) = unproject(px - gx, py - gy, nz);
        self.lat = lat2;
        self.lon = lon2;
        self.user_moved = true;
    }

    /// Dopasowanie do prostokąta (lat0, lon0, lat1, lon1) z marginesem.
    pub fn fit(&mut self, b: (f64, f64, f64, f64)) {
        let (la0, lo0, la1, lo1) = b;
        self.lat = (la0 + la1) / 2.0;
        self.lon = (lo0 + lo1) / 2.0;
        let mut z = MAX_ZOOM - 3.0;
        while z > 1.5 {
            let (x0, y0) = project(la1.max(la0), lo0.min(lo1), z);
            let (x1, y1) = project(la0.min(la1), lo1.max(lo0), z);
            if (x1 - x0).abs() < self.w * 0.82 && (y1 - y0).abs() < self.h * 0.82 {
                break;
            }
            z -= 0.25;
        }
        self.zoom = z;
        self.user_moved = false;
        // Środek w odwzorowaniu (nie średnia szerokości)
        let (x0, y0) = project(la0, lo0, z);
        let (x1, y1) = project(la1, lo1, z);
        let (lat, lon) = unproject((x0 + x1) / 2.0, (y0 + y1) / 2.0, z);
        self.lat = lat;
        self.lon = lon;
    }

    /// Skala: długość odcinka podziałki w NM i jego szerokość w px (do podpisu).
    pub fn scale_text(&self) -> String {
        let m_per_px = 156_543.03392 * self.lat.to_radians().cos() / 2f64.powf(self.zoom);
        let nm = m_per_px * 100.0 / 1852.0;
        let nice = [0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0];
        let v = nice.iter().copied().find(|n| *n >= nm).unwrap_or(1000.0);
        format!("{} NM ≈ {:.0} px", v, v * 1852.0 / m_per_px)
    }
}

// ---------- nakładki ----------

#[derive(Clone, Debug, Default)]
pub struct Track {
    pub pts: Vec<(f64, f64, i32)>,
    /// None – kolory wysokości; Some – jeden kolor.
    pub color: Option<[u8; 4]>,
    pub width: f32,
    /// Plan lotu: na pochylonej mapie linia wisi na swojej wysokości, a pod nią półprzezroczysta ściana
    /// do ziemi z cieniem trasy na gruncie i pionowymi kreskami przy punktach.
    pub wall: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
    pub label: String,
    /// 0 lotnisko, 1 punkt nawigacyjny, 2 wyróżniony (znacznik/przyziemienie), 3 lotnisko małe, 4 zrzut ekranu
    pub kind: i32,
    /// Wysokość punktu planu (ft) – na pochylonej mapie punkt wisi razem z trasą; None – na ziemi.
    pub alt: Option<i32>,
}

#[derive(Clone, Debug, Default)]
pub struct Overlay {
    pub tracks: Vec<Track>,
    pub points: Vec<Point>,
    pub runways: Vec<(f64, f64, f64, f64)>,
    pub aircraft: Option<(f64, f64, f64)>,
    pub cursor: Option<(f64, f64)>,
    pub route: Vec<(f64, f64)>,
    /// Krycie warstwy radaru opadów (0 – bez radaru).
    pub radar: f32,
}

/// Kolor wysokości: nisko zielony → żółty → pomarańczowy → czerwony → fiolet (wysoko).
pub fn alt_color(alt: i32, max: i32) -> [u8; 4] {
    let t = (alt.max(0) as f64 / (max.max(1000) as f64)).clamp(0.0, 1.0);
    let stops: [(f64, [f64; 3]); 5] = [(0.0, [60.0, 207.0, 122.0]), (0.3, [255.0, 214.0, 70.0]), (0.55, [255.0, 140.0, 60.0]), (0.8, [255.0, 80.0, 95.0]), (1.0, [190.0, 92.0, 255.0])];
    for w in stops.windows(2) {
        if t <= w[1].0 {
            let f = (t - w[0].0) / (w[1].0 - w[0].0);
            let c: Vec<u8> = (0..3).map(|i| (w[0].1[i] + (w[1].1[i] - w[0].1[i]) * f) as u8).collect();
            return [c[0], c[1], c[2], 255];
        }
    }
    [190, 92, 255, 255]
}

pub struct Rendered {
    pub image: slint::Image,
    pub labels: Vec<(f32, f32, String, i32)>,
}

fn color(c: [u8; 4]) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], c[3])
}

/// Rysuje mapę. `scale` – współczynnik ekranu (piksele fizyczne na logiczny).
pub fn render(v: &View, style: Style, ov: &Overlay, scale: f32, dark: bool) -> Rendered {
    let (pm, labels) = render_pm(v, style, ov, scale, dark);
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(pm.data(), pm.width(), pm.height());
    Rendered { image: slint::Image::from_rgba8_premultiplied(buf), labels }
}

/// Jak [`render`], ale zwraca obraz tiny-skia (do dorysowania kolejnych warstw).
pub fn render_pm(v: &View, style: Style, ov: &Overlay, scale: f32, dark: bool) -> (Pixmap, Vec<(f32, f32, String, i32)>) {
    let pw = ((v.w * scale as f64).round() as u32).clamp(16, 6000);
    let ph = ((v.h * scale as f64).round() as u32).clamp(16, 6000);
    let mut pm = Pixmap::new(pw, ph).unwrap();
    pm.fill(if dark { Color::from_rgba8(14, 17, 22, 255) } else { Color::from_rgba8(232, 235, 239, 255) });
    let s = scale as f64;
    draw_tiles(&mut pm, v, style, scale, 1.0);
    if ov.radar > 0.0 {
        draw_tiles(&mut pm, v, Style::Radar, scale, ov.radar);
    }
    draw_haze(&mut pm, v, dark);
    let to = |lat: f64, lon: f64| -> (f32, f32) {
        let (x, y) = v.to_screen(lat, lon);
        ((x * s) as f32, (y * s) as f32)
    };
    let sc = scale;
    // Trasa z GPS (przerywana)
    if ov.route.len() > 1 {
        let mut pb = PathBuilder::new();
        for (i, (la, lo)) in ov.route.iter().enumerate() {
            let (x, y) = to(*la, *lo);
            if i == 0 {
                pb.move_to(x, y)
            } else {
                pb.line_to(x, y)
            }
        }
        if let Some(p) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color(Color::from_rgba8(255, 0, 220, 200));
            paint.anti_alias = true;
            let stroke = Stroke { width: 2.0 * sc, dash: tiny_skia::StrokeDash::new(vec![8.0 * sc, 6.0 * sc], 0.0), ..Default::default() };
            pm.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
        }
    }
    // Pasy
    for (la0, lo0, la1, lo1) in &ov.runways {
        let (a, b) = (to(*la0, *lo0), to(*la1, *lo1));
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        if let Some(p) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color(if dark { Color::from_rgba8(230, 230, 235, 230) } else { Color::from_rgba8(60, 60, 70, 230) });
            paint.anti_alias = true;
            let width = (v.zoom - 10.0).clamp(1.0, 6.0) as f32 * sc;
            pm.stroke_path(&p, &paint, &Stroke { width, line_cap: LineCap::Butt, ..Default::default() }, Transform::identity(), None);
        }
    }
    // Ślady
    for t in &ov.tracks {
        if t.pts.len() < 2 {
            continue;
        }
        let max_alt = t.pts.iter().map(|p| p.2).max().unwrap_or(1000);
        let step = (t.pts.len() / 4000).max(1);
        let pts: Vec<&(f64, f64, i32)> = t.pts.iter().step_by(step).chain(t.pts.last()).collect();
        let stroke = Stroke { width: t.width * sc, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
        let lift = t.wall && v.tilted();
        let pos = |p: &(f64, f64, i32)| -> (f32, f32) {
            if lift {
                let (x, y) = v.to_screen_alt(p.0, p.1, p.2 as f64);
                ((x * s) as f32, (y * s) as f32)
            } else {
                to(p.0, p.1)
            }
        };
        if lift {
            draw_wall(&mut pm, v, &pts, t.color, max_alt, s, dark);
        }
        // Obrys dla czytelności na podkładzie
        let mut outline = PathBuilder::new();
        for (i, p) in pts.iter().enumerate() {
            let (x, y) = pos(*p);
            if i == 0 {
                outline.move_to(x, y)
            } else {
                outline.line_to(x, y)
            }
        }
        if let Some(p) = outline.finish() {
            let mut paint = Paint::default();
            paint.set_color(if dark { Color::from_rgba8(0, 0, 0, 150) } else { Color::from_rgba8(255, 255, 255, 190) });
            paint.anti_alias = true;
            let mut st = stroke.clone();
            st.width += 2.5 * sc;
            pm.stroke_path(&p, &paint, &st, Transform::identity(), None);
            if let Some(c) = t.color {
                let mut paint = Paint::default();
                paint.set_color(color(c));
                paint.anti_alias = true;
                pm.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
                continue;
            }
        }
        for w in pts.windows(2) {
            let (a, b) = (pos(w[0]), pos(w[1]));
            if (a.0 - b.0).abs() > pw as f32 && (a.1 - b.1).abs() > ph as f32 {
                continue;
            }
            let mut pb = PathBuilder::new();
            pb.move_to(a.0, a.1);
            pb.line_to(b.0, b.1);
            if let Some(p) = pb.finish() {
                let mut paint = Paint::default();
                paint.set_color(color(alt_color((w[0].2 + w[1].2) / 2, max_alt)));
                paint.anti_alias = true;
                pm.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
            }
        }
    }
    // Punkty i etykiety
    let mut labels = Vec::new();
    for p in &ov.points {
        let (x, y) = match p.alt {
            Some(a) if v.tilted() => {
                let (x, y) = v.to_screen_alt(p.lat, p.lon, a as f64);
                ((x * s) as f32, (y * s) as f32)
            }
            _ => to(p.lat, p.lon),
        };
        if x < -20.0 || y < -20.0 || x > pw as f32 + 20.0 || y > ph as f32 + 20.0 {
            continue;
        }
        let (r, c) = match p.kind {
            0 => (4.5, [91, 140, 255, 255]),
            1 => (3.0, [180, 180, 190, 255]),
            2 => (5.0, [255, 181, 71, 255]),
            4 => (5.0, [79, 216, 235, 255]),
            _ => (3.0, [91, 140, 255, 200]),
        };
        if let Some(circle) = PathBuilder::from_circle(x, y, r * sc) {
            let mut paint = Paint::default();
            paint.anti_alias = true;
            paint.set_color(if dark { Color::from_rgba8(0, 0, 0, 200) } else { Color::from_rgba8(255, 255, 255, 220) });
            pm.stroke_path(&circle, &paint, &Stroke { width: 2.0 * sc, ..Default::default() }, Transform::identity(), None);
            paint.set_color(color(c));
            pm.fill_path(&circle, &paint, FillRule::Winding, Transform::identity(), None);
        }
        if !p.label.is_empty() {
            labels.push((x / sc, y / sc, p.label.clone(), if p.kind == 2 { 2 } else if p.kind == 0 { 0 } else { 1 }));
        }
    }
    // Kursor (odtwarzanie lotu)
    if let Some((la, lo)) = ov.cursor {
        let (x, y) = to(la, lo);
        if let Some(c) = PathBuilder::from_circle(x, y, 6.0 * sc) {
            let mut paint = Paint::default();
            paint.anti_alias = true;
            paint.set_color(Color::from_rgba8(255, 255, 255, 255));
            pm.fill_path(&c, &paint, FillRule::Winding, Transform::identity(), None);
            paint.set_color(Color::from_rgba8(91, 140, 255, 255));
            pm.stroke_path(&c, &paint, &Stroke { width: 3.0 * sc, ..Default::default() }, Transform::identity(), None);
        }
    }
    // Samolot
    if let Some((la, lo, hdg)) = ov.aircraft {
        let (x, y) = to(la, lo);
        let mut pb = PathBuilder::new();
        let shape: [(f32, f32); 11] = [(0.0, -12.0), (2.0, -4.0), (11.0, 2.0), (11.0, 4.0), (2.0, 1.5), (1.5, 8.0), (5.0, 11.0), (-5.0, 11.0), (-1.5, 8.0), (-2.0, 1.5), (-11.0, 4.0)];
        for (i, (px, py)) in shape.iter().chain([(-11.0f32, 2.0f32), (-2.0, -4.0)].iter()).enumerate() {
            if i == 0 {
                pb.move_to(*px, *py)
            } else {
                pb.line_to(*px, *py)
            }
        }
        pb.close();
        if let Some(p) = pb.finish() {
            let t = Transform::from_rotate(v.screen_deg(hdg) as f32).post_scale(1.25 * sc, 1.25 * sc).post_translate(x, y);
            let mut paint = Paint::default();
            paint.anti_alias = true;
            paint.set_color(Color::from_rgba8(0, 0, 0, 200));
            pm.stroke_path(&p, &paint, &Stroke { width: 2.5, line_join: LineJoin::Round, ..Default::default() }, t, None);
            paint.set_color(Color::from_rgba8(255, 214, 70, 255));
            pm.fill_path(&p, &paint, FillRule::Winding, t, None);
        }
    }
    (pm, labels)
}

/// Ściana pod trasą planu na pochylonej mapie: półprzezroczyste pasy od linii w powietrzu do ziemi,
/// cień trasy na gruncie i pionowe kreski przy punktach (łatwo ocenić, gdzie i jak wysoko leci samolot).
fn draw_wall(pm: &mut Pixmap, v: &View, pts: &[&(f64, f64, i32)], col: Option<[u8; 4]>, max_alt: i32, s: f64, dark: bool) {
    let sp = |p: (f64, f64)| ((p.0 * s) as f32, (p.1 * s) as f32);
    let ground: Vec<(f32, f32)> = pts.iter().map(|p| sp(v.to_screen(p.0, p.1))).collect();
    let air: Vec<(f32, f32)> = pts.iter().map(|p| sp(v.to_screen_alt(p.0, p.1, p.2 as f64))).collect();
    let tone = |i: usize| col.unwrap_or_else(|| alt_color(pts[i].2, max_alt));
    let mut paint = Paint::default();
    paint.anti_alias = true;
    for i in 0..pts.len().saturating_sub(1) {
        if pts[i].2 <= 0 && pts[i + 1].2 <= 0 {
            continue;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(ground[i].0, ground[i].1);
        pb.line_to(ground[i + 1].0, ground[i + 1].1);
        pb.line_to(air[i + 1].0, air[i + 1].1);
        pb.line_to(air[i].0, air[i].1);
        pb.close();
        if let Some(path) = pb.finish() {
            let c = tone(i);
            paint.set_color(Color::from_rgba8(c[0], c[1], c[2], 48));
            pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }
    // cień trasy na ziemi
    let mut pb = PathBuilder::new();
    for (i, g) in ground.iter().enumerate() {
        if i == 0 {
            pb.move_to(g.0, g.1)
        } else {
            pb.line_to(g.0, g.1)
        }
    }
    if let Some(path) = pb.finish() {
        paint.set_color(if dark { Color::from_rgba8(0, 0, 0, 150) } else { Color::from_rgba8(40, 46, 58, 110) });
        let st = Stroke { width: 1.4 * s as f32, line_join: LineJoin::Round, dash: tiny_skia::StrokeDash::new(vec![6.0 * s as f32, 4.0 * s as f32], 0.0), ..Default::default() };
        pm.stroke_path(&path, &paint, &st, Transform::identity(), None);
    }
    // pionowe kreski od punktu do ziemi
    for i in 0..pts.len() {
        if pts[i].2 <= 0 {
            continue;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(ground[i].0, ground[i].1);
        pb.line_to(air[i].0, air[i].1);
        if let Some(path) = pb.finish() {
            let c = tone(i);
            paint.set_color(Color::from_rgba8(c[0], c[1], c[2], 150));
            pm.stroke_path(&path, &paint, &Stroke { width: 1.0 * s as f32, ..Default::default() }, Transform::identity(), None);
        }
    }
}

/// Kafelki stylu `style` na obrazie (podkład albo nakładka z przezroczystością `opacity`).
pub fn draw_tiles(pm: &mut Pixmap, v: &View, style: Style, scale: f32, opacity: f32) {
    if v.projected() {
        draw_tiles_tilted(pm, v, style, scale, opacity);
        return;
    }
    let (pw, ph) = (pm.width(), pm.height());
    let s = scale as f64;
    // Kafelki: poziom dopasowany do gęstości ekranu
    let tz = ((v.zoom + s.log2()).round().clamp(0.0, MAX_ZOOM) as u32).min(style.max_zoom());
    let n = 1u32 << tz;
    let k = world(v.zoom) * s / world(tz as f64);
    let (cx, cy) = project(v.lat, v.lon, tz as f64);
    let (left, top) = (cx - pw as f64 / 2.0 / k, cy - ph as f64 / 2.0 / k);
    let tx0 = (left / TILE).floor() as i64;
    let ty0 = (top / TILE).floor().max(0.0) as i64;
    let tx1 = ((left + pw as f64 / k) / TILE).floor() as i64;
    let ty1 = (((top + ph as f64 / k) / TILE).floor() as i64).min(n as i64 - 1);
    let paint = PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, opacity, ..Default::default() };
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let wx = tx.rem_euclid(n as i64) as u32;
            let sx = ((tx as f64 * TILE - left) * k) as f32;
            let sy = ((ty as f64 * TILE - top) * k) as f32;
            match tile((style, tz, wx, ty as u32)) {
                Some(t) => pm.draw_pixmap(0, 0, t.as_ref().as_ref(), &paint, Transform::from_row(k as f32, 0.0, 0.0, k as f32, sx, sy), None),
                None => {
                    // Zastępczo powiększony kafelek z niższego poziomu (płynne ładowanie)
                    for up in 1..=4u32 {
                        if tz < up {
                            break;
                        }
                        let (pz, px, py) = (tz - up, wx >> up, ty as u32 >> up);
                        if let Some(t) = cached((style, pz, px, py)) {
                            let f = (1u32 << up) as f32;
                            let ox = (wx - (px << up)) as f32 * TILE as f32 / f;
                            let oy = (ty as u32 - (py << up)) as f32 * TILE as f32 / f;
                            let kk = k as f32 * f;
                            let mut clip = tiny_skia::Mask::new(pw, ph).unwrap();
                            if let Some(r) = tiny_skia::Rect::from_xywh(sx, sy, (TILE * k) as f32 + 1.0, (TILE * k) as f32 + 1.0) {
                                clip.fill_path(&PathBuilder::from_rect(r), FillRule::Winding, false, Transform::identity());
                            }
                            pm.draw_pixmap(0, 0, t.as_ref().as_ref(), &paint, Transform::from_row(kk, 0.0, 0.0, kk, sx - ox * kk, sy - oy * kk), Some(&clip));
                            break;
                        }
                    }
                }
            }
        }
    }
}

/// Pochylona mapa: kafelki rozpięte w perspektywie. Bliżej kamery szczegółowe, dalej coraz ogólniejsze
/// (drzewo czwórkowe: kafelek dzieli się, dopóki na ekranie jest większy niż ok. 1,4 kafelka).
fn draw_tiles_tilted(pm: &mut Pixmap, v: &View, style: Style, scale: f32, opacity: f32) {
    let s = scale as f64;
    let zt = ((v.zoom + s.log2()).round().clamp(0.0, MAX_ZOOM) as u32).min(style.max_zoom());
    let z0 = zt.saturating_sub(7);
    // Widoczny obszar płaskiej mapy (względem środka, px logiczne przy v.zoom)
    let corners = [(0.0, 0.0), (v.w, 0.0), (0.0, v.h), (v.w, v.h), (v.w / 2.0, 0.0)];
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in corners.iter().map(|&(x, y)| v.screen_to_flat(x, y)) {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let (cx, cy) = project(v.lat, v.lon, v.zoom);
    let k0 = 2f64.powf(v.zoom - z0 as f64);
    let n0 = 1i64 << z0;
    let tx0 = ((cx + x0) / k0 / TILE).floor() as i64;
    let tx1 = ((cx + x1) / k0 / TILE).floor() as i64;
    let ty0 = (((cy + y0) / k0 / TILE).floor() as i64).max(0);
    let ty1 = (((cy + y1) / k0 / TILE).floor() as i64).min(n0 - 1);
    let t = Tilted { v, style, scale, opacity, zt, c: (cx, cy), vis: (x0, y0, x1, y1) };
    let mut budget = 900usize;
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            t.tile(pm, z0, tx, ty, &mut budget);
        }
    }
}

struct Tilted<'a> {
    v: &'a View,
    style: Style,
    scale: f32,
    opacity: f32,
    /// Najszczegółowszy poziom kafelków (przy środku ekranu).
    zt: u32,
    /// Środek widoku we współrzędnych świata przy v.zoom.
    c: (f64, f64),
    /// Widoczny prostokąt płaskiej mapy względem środka.
    vis: (f64, f64, f64, f64),
}

impl Tilted<'_> {
    fn screen(&self, fx: f64, fy: f64) -> (f32, f32) {
        let (x, y) = self.v.flat_to_screen(fx, fy);
        let s = self.scale as f64;
        ((x * s) as f32, (y * s) as f32)
    }

    fn tile(&self, pm: &mut Pixmap, z: u32, tx: i64, ty: i64, budget: &mut usize) {
        if *budget == 0 {
            return;
        }
        let k = 2f64.powf(self.v.zoom - z as f64);
        let size = TILE * k;
        let (fx0, fy0) = (tx as f64 * size - self.c.0, ty as f64 * size - self.c.1);
        let vis = self.vis;
        if fx0 > vis.2 || fx0 + size < vis.0 || fy0 > vis.3 || fy0 + size < vis.1 {
            return;
        }
        let q = [self.screen(fx0, fy0), self.screen(fx0 + size, fy0), self.screen(fx0 + size, fy0 + size), self.screen(fx0, fy0 + size)];
        let edge = |a: (f32, f32), b: (f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
        let big = edge(q[0], q[1]).max(edge(q[3], q[2])).max(edge(q[0], q[3])).max(edge(q[1], q[2]));
        if z < self.zt && big > (TILE * 1.4) as f32 {
            for (ix, iy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                self.tile(pm, z + 1, tx * 2 + ix, ty * 2 + iy, budget);
            }
            return;
        }
        *budget -= 1;
        let n = 1i64 << z;
        let Some((tex, ox, oy, tsize)) = texture(self.style, z, tx.rem_euclid(n) as u32, ty as u32) else { return };
        // siatka, żeby perspektywa była gładka (pojedyncze trójkąty są afiniczne)
        let g = if big > 600.0 { 8 } else { 4 };
        let mut grid = Vec::with_capacity((g + 1) * (g + 1));
        for j in 0..=g {
            for i in 0..=g {
                grid.push(self.screen(fx0 + size * i as f64 / g as f64, fy0 + size * j as f64 / g as f64));
            }
        }
        let (pw, ph) = (pm.width() as f32, pm.height() as f32);
        let uv = |i: usize, j: usize| (ox + tsize * i as f32 / g as f32, oy + tsize * j as f32 / g as f32);
        let ix = |i: usize, j: usize| j * (g + 1) + i;
        for j in 0..g {
            for i in 0..g {
                let (a, b, cc, d) = (grid[ix(i, j)], grid[ix(i + 1, j)], grid[ix(i + 1, j + 1)], grid[ix(i, j + 1)]);
                let minx = a.0.min(b.0).min(cc.0).min(d.0);
                let maxx = a.0.max(b.0).max(cc.0).max(d.0);
                let miny = a.1.min(b.1).min(cc.1).min(d.1);
                let maxy = a.1.max(b.1).max(cc.1).max(d.1);
                if maxx < 0.0 || minx > pw || maxy < 0.0 || miny > ph {
                    continue;
                }
                let (ta, tb, tc, td) = (uv(i, j), uv(i + 1, j), uv(i + 1, j + 1), uv(i, j + 1));
                fill_tex(pm, &tex, [ta, tb, tc], [a, b, cc], self.opacity);
                fill_tex(pm, &tex, [ta, tc, td], [a, cc, d], self.opacity);
            }
        }
    }
}

/// Trójkąt tekstury (u, v) rozpięty na trójkącie ekranu.
fn fill_tex(pm: &mut Pixmap, tex: &Arc<Pixmap>, t: [(f32, f32); 3], s: [(f32, f32); 3], opacity: f32) {
    let (du1, dv1) = (t[1].0 - t[0].0, t[1].1 - t[0].1);
    let (du2, dv2) = (t[2].0 - t[0].0, t[2].1 - t[0].1);
    let det = du1 * dv2 - du2 * dv1;
    if det.abs() < 1e-9 {
        return;
    }
    let (dx1, dy1) = (s[1].0 - s[0].0, s[1].1 - s[0].1);
    let (dx2, dy2) = (s[2].0 - s[0].0, s[2].1 - s[0].1);
    let a = (dx1 * dv2 - dx2 * dv1) / det;
    let c = (dx2 * du1 - dx1 * du2) / det;
    let b = (dy1 * dv2 - dy2 * dv1) / det;
    let d = (dy2 * du1 - dy1 * du2) / det;
    let e = s[0].0 - a * t[0].0 - c * t[0].1;
    let f = s[0].1 - b * t[0].0 - d * t[0].1;
    let mut pb = PathBuilder::new();
    pb.move_to(s[0].0, s[0].1);
    pb.line_to(s[1].0, s[1].1);
    pb.line_to(s[2].0, s[2].1);
    pb.close();
    let Some(path) = pb.finish() else { return };
    let mut paint = Paint::default();
    // bez wygładzania krawędzi – inaczej między trójkątami widać szwy
    paint.anti_alias = false;
    paint.shader = Pattern::new(tex.as_ref().as_ref(), SpreadMode::Pad, FilterQuality::Bilinear, opacity, Transform::from_row(a, b, c, d, e, f));
    pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
}

/// Pochylona mapa: mgiełka u góry (daleki plan), mocniejsza przy większym pochyleniu.
pub fn draw_haze(pm: &mut Pixmap, v: &View, dark: bool) {
    let t = v.tilt_deg();
    if t == 0.0 {
        return;
    }
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    let k = (t / MAX_TILT) as f32;
    let a = (k * 235.0) as u8;
    let (r, g, b) = if dark { (14, 17, 22) } else { (214, 224, 236) };
    let band = h * (0.12 + 0.22 * k);
    let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, w, band) else { return };
    let Some(sh) = tiny_skia::LinearGradient::new(
        tiny_skia::Point::from_xy(0.0, 0.0),
        tiny_skia::Point::from_xy(0.0, band),
        vec![tiny_skia::GradientStop::new(0.0, Color::from_rgba8(r, g, b, a)), tiny_skia::GradientStop::new(1.0, Color::from_rgba8(r, g, b, 0))],
        SpreadMode::Pad,
        Transform::identity(),
    ) else {
        return;
    };
    let mut paint = Paint::default();
    paint.shader = sh;
    pm.fill_rect(rect, &paint, Transform::identity(), None);
}

/// Granice zbioru punktów (lat0, lon0, lat1, lon1) albo None.
pub fn bounds(pts: impl Iterator<Item = (f64, f64)>) -> Option<(f64, f64, f64, f64)> {
    let mut b: Option<(f64, f64, f64, f64)> = None;
    for (la, lo) in pts {
        b = Some(match b {
            None => (la, lo, la, lo),
            Some((a, c, d, e)) => (a.min(la), c.min(lo), d.max(la), e.max(lo)),
        });
    }
    b.map(|(a, c, d, e)| {
        // Minimalny rozmiar (lot po kręgu, jedno lotnisko)
        let pad = 0.02;
        (a - pad, c - pad, d + pad, e + pad)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_roundtrip() {
        let (x, y) = project(54.38, 18.47, 10.0);
        let (la, lo) = unproject(x, y, 10.0);
        assert!((la - 54.38).abs() < 1e-9 && (lo - 18.47).abs() < 1e-9);
    }

    #[test]
    fn tilted_view_roundtrip_and_zoom() {
        let mut v = View { lat: 52.0, lon: 21.0, zoom: 8.0, w: 800.0, h: 600.0, tilt: 45.0, ..Default::default() };
        // środek ekranu to nadal środek mapy
        let (x, y) = v.to_screen(52.0, 21.0);
        assert!((x - 400.0).abs() < 1e-6 && (y - 300.0).abs() < 1e-6);
        let (x, y) = v.to_screen(52.1, 21.2);
        let (la, lo) = v.from_screen(x, y);
        assert!((la - 52.1).abs() < 1e-6 && (lo - 21.2).abs() < 1e-6);
        v.zoom_at(120.0, x, y);
        let (x2, y2) = v.to_screen(52.1, 21.2);
        assert!((x - x2).abs() < 0.5 && (y - y2).abs() < 0.5);
        // przeciąganie: punkt pod wskaźnikiem idzie za myszą
        let (px, py) = v.to_screen(52.05, 21.1);
        v.drag(px + 30.0, py - 20.0, 30.0, -20.0);
        let (qx, qy) = v.to_screen(52.05, 21.1);
        assert!((qx - px - 30.0).abs() < 0.5 && (qy - py + 20.0).abs() < 0.5);
        // północ jest wyżej na ekranie
        let (_, yn) = v.to_screen(52.3, 21.0);
        assert!(yn < 300.0);
    }

    #[test]
    fn rotated_view_roundtrip_and_altitude() {
        let mut v = View { lat: 52.0, lon: 21.0, zoom: 8.0, w: 800.0, h: 600.0, bearing: 90.0, ..Default::default() };
        // obrót o 90°: wschód u góry ekranu
        let (_, ye) = v.to_screen(52.0, 21.3);
        assert!(ye < 300.0);
        for tilt in [0.0, 40.0] {
            v.tilt = tilt;
            let (x, y) = v.to_screen(52.1, 21.2);
            let (la, lo) = v.from_screen(x, y);
            assert!((la - 52.1).abs() < 1e-6 && (lo - 21.2).abs() < 1e-6);
            let (px, py) = v.to_screen(52.05, 21.1);
            v.drag(px + 25.0, py + 15.0, 25.0, 15.0);
            let (qx, qy) = v.to_screen(52.05, 21.1);
            assert!((qx - px - 25.0).abs() < 0.5 && (qy - py - 15.0).abs() < 0.5);
        }
        // na pochylonej mapie punkt w powietrzu jest wyżej na ekranie niż jego rzut na ziemię
        let (_, yg) = v.to_screen(52.0, 21.0);
        let (_, ya) = v.to_screen_alt(52.0, 21.0, 30_000.0);
        assert!(ya < yg - 1.0);
        v.rotate_by(30.0);
        v.reset_bearing();
        assert!(!v.rotated());
    }

    #[test]
    fn view_zoom_keeps_point() {
        let mut v = View { lat: 52.0, lon: 21.0, zoom: 8.0, w: 800.0, h: 600.0, ..Default::default() };
        let (x, y) = v.to_screen(52.2, 21.3);
        v.zoom_at(120.0, x, y);
        let (x2, y2) = v.to_screen(52.2, 21.3);
        assert!((x - x2).abs() < 0.5 && (y - y2).abs() < 0.5);
        v.fit((52.0, 20.0, 54.5, 21.0));
        let (a, b) = v.to_screen(54.5, 20.0);
        assert!(a > 0.0 && b > 0.0 && a < 800.0);
    }
}
