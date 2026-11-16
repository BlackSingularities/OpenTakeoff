//! Mapa rysowana w Rust: kafelki rastrowe (CARTO na podkładzie OpenStreetMap albo OSM) w odwzorowaniu
//! Mercatora z pamięcią podręczną na dysku i w pamięci, a na nich ślad lotu w kolorach wysokości, samolot,
//! lotniska, pasy i znaczniki. Etykiety rysuje Slint (ostry tekst), stąd lista etykiet obok obrazu.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crossbeam_channel::{unbounded, Sender};
use parking_lot::Mutex;
use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

const TILE: f64 = 256.0;
const MAX_ZOOM: f64 = 18.0;
const MEMORY_TILES: usize = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    Dark,
    Light,
    Osm,
    Satellite,
}

impl Style {
    fn key(self) -> &'static str {
        match self {
            Style::Dark => "esri-dark",
            Style::Light => "esri-light",
            Style::Osm => "osm",
            Style::Satellite => "esri-sat",
        }
    }
    fn url(self, z: u32, x: u32, y: u32) -> String {
        const ESRI: &str = "https://services.arcgisonline.com/ArcGIS/rest/services";
        match self {
            Style::Dark => format!("{ESRI}/Canvas/World_Dark_Gray_Base/MapServer/tile/{z}/{y}/{x}"),
            Style::Light => format!("{ESRI}/Canvas/World_Light_Gray_Base/MapServer/tile/{z}/{y}/{x}"),
            Style::Satellite => format!("{ESRI}/World_Imagery/MapServer/tile/{z}/{y}/{x}"),
            Style::Osm => format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png"),
        }
    }
    /// Najwyższy poziom kafelków danego podkładu (wyżej – powiększony kafelek).
    fn max_zoom(self) -> u32 {
        match self {
            Style::Dark | Style::Light => 16,
            Style::Satellite => 18,
            Style::Osm => 19,
        }
    }
    pub fn attribution(self) -> &'static str {
        match self {
            Style::Osm => "© OpenStreetMap",
            Style::Satellite => "© Esri, Maxar, Earthstar Geographics",
            _ => "© Esri, HERE, Garmin, © OpenStreetMap",
        }
    }
}

type Key = (Style, u32, u32, u32);

struct Tiles {
    mem: Mutex<(HashMap<Key, Arc<Pixmap>>, VecDeque<Key>)>,
    pending: Mutex<HashSet<Key>>,
    failed: Mutex<HashSet<Key>>,
    queue: Sender<Key>,
}

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
        let (tx, rx) = unbounded::<Key>();
        for i in 0..4 {
            let rx = rx.clone();
            let _ = std::thread::Builder::new().name(format!("tiles-{i}")).spawn(move || {
                let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).user_agent(&format!("OpenTakeoff-Windows/{} (+https://redzikowski.com)", opentakeoff_core::version())).build();
                while let Ok(k) = rx.recv() {
                    let t = tiles();
                    let path = tile_path(&k);
                    let bytes = std::fs::read(&path).ok().or_else(|| {
                        let resp = agent.get(&k.0.url(k.1, k.2, k.3)).call().ok()?;
                        if !resp.content_type().starts_with("image/") {
                            return None;
                        }
                        let mut b = Vec::new();
                        std::io::Read::read_to_end(&mut resp.into_reader(), &mut b).ok()?;
                        if let Some(d) = path.parent() {
                            let _ = std::fs::create_dir_all(d);
                        }
                        let _ = std::fs::write(&path, &b);
                        Some(b)
                    });
                    match bytes.and_then(|b| decode(&b)) {
                        Some(p) => {
                            let mut m = t.mem.lock();
                            m.0.insert(k, Arc::new(p));
                            m.1.push_back(k);
                            while m.1.len() > MEMORY_TILES {
                                if let Some(old) = m.1.pop_front() {
                                    m.0.remove(&old);
                                }
                            }
                        }
                        None => {
                            t.failed.lock().insert(k);
                        }
                    }
                    t.pending.lock().remove(&k);
                    TILES_SERIAL.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
        Tiles { mem: Mutex::new((HashMap::new(), VecDeque::new())), pending: Mutex::new(HashSet::new()), failed: Mutex::new(HashSet::new()), queue: tx }
    })
}

/// Kafelek z pamięci; brak – zlecenie pobrania (z dysku albo z sieci) i None.
fn tile(k: Key) -> Option<Arc<Pixmap>> {
    let t = tiles();
    if let Some(p) = t.mem.lock().0.get(&k) {
        return Some(p.clone());
    }
    if t.failed.lock().contains(&k) {
        return None;
    }
    if t.pending.lock().insert(k) {
        let _ = t.queue.send(k);
    }
    None
}

fn cached(k: Key) -> Option<Arc<Pixmap>> {
    tiles().mem.lock().0.get(&k).cloned()
}

/// Kafelek do teksturowania (widok 3D): obraz i fragment (x, y, rozmiar w pikselach obrazu). Gdy kafelka
/// jeszcze nie ma, zleca pobranie i zwraca powiększony fragment kafelka z niższego poziomu.
pub fn texture(style: Style, z: u32, x: u32, y: u32) -> Option<(Arc<Pixmap>, f32, f32, f32)> {
    let z = z.min(style.max_zoom());
    if let Some(t) = tile((style, z, x, y)) {
        return Some((t, 0.0, 0.0, TILE as f32));
    }
    for up in 1..=6u32 {
        if z < up {
            break;
        }
        let (pz, px, py) = (z - up, x >> up, y >> up);
        if let Some(t) = cached((style, pz, px, py)) {
            let f = (1u32 << up) as f32;
            let size = TILE as f32 / f;
            return Some((t, (x - (px << up)) as f32 * size, (y - (py << up)) as f32 * size, size));
        }
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
}

impl Default for View {
    fn default() -> Self {
        Self { lat: 52.0, lon: 19.0, zoom: 5.5, w: 600.0, h: 400.0, follow: true, user_moved: false }
    }
}

impl View {
    pub fn to_screen(&self, lat: f64, lon: f64) -> (f64, f64) {
        let (cx, cy) = project(self.lat, self.lon, self.zoom);
        let (x, y) = project(lat, lon, self.zoom);
        let w = world(self.zoom);
        let mut dx = x - cx;
        if dx > w / 2.0 {
            dx -= w;
        } else if dx < -w / 2.0 {
            dx += w;
        }
        (self.w / 2.0 + dx, self.h / 2.0 + (y - cy))
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
        let (lat, lon) = {
            let (cx, cy) = project(self.lat, self.lon, self.zoom);
            unproject(cx + mx - self.w / 2.0, cy + my - self.h / 2.0, self.zoom)
        };
        let nz = (self.zoom + delta / 120.0 * 0.5).clamp(1.5, MAX_ZOOM);
        self.zoom = nz;
        let (px, py) = project(lat, lon, nz);
        let (lat2, lon2) = unproject(px - (mx - self.w / 2.0), py - (my - self.h / 2.0), nz);
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
}

#[derive(Clone, Debug)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
    pub label: String,
    /// 0 lotnisko, 1 punkt nawigacyjny, 2 wyróżniony (znacznik/przyziemienie), 3 lotnisko małe, 4 zrzut ekranu
    pub kind: i32,
}

#[derive(Clone, Debug, Default)]
pub struct Overlay {
    pub tracks: Vec<Track>,
    pub points: Vec<Point>,
    pub runways: Vec<(f64, f64, f64, f64)>,
    pub aircraft: Option<(f64, f64, f64)>,
    pub cursor: Option<(f64, f64)>,
    pub route: Vec<(f64, f64)>,
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
    let paint = PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
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
        // Obrys dla czytelności na podkładzie
        let mut outline = PathBuilder::new();
        for (i, p) in pts.iter().enumerate() {
            let (x, y) = to(p.0, p.1);
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
            let (a, b) = (to(w[0].0, w[0].1), to(w[1].0, w[1].1));
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
        let (x, y) = to(p.lat, p.lon);
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
            let t = Transform::from_rotate(hdg as f32).post_scale(1.25 * sc, 1.25 * sc).post_translate(x, y);
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
