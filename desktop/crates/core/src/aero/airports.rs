//! Baza lotnisk OurAirports (domena publiczna): lotniska, pasy i częstotliwości. Pobierana raz przy pierwszym
//! uruchomieniu (ok. 18 MB CSV), zapisywana w pamięci podręcznej jako skompresowany JSON i odświeżana co 30 dni.

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::logbook::landing::RunwayGeom;
use crate::navdata::math::{bearing_deg, distance_nm};

const BASE: &str = "https://davidmegginson.github.io/ourairports-data";
const REFRESH_DAYS: u64 = 30;
const CACHE_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Runway {
    pub le: String,
    pub he: String,
    pub length_ft: i32,
    pub width_ft: i32,
    pub surface: String,
    pub lighted: bool,
    pub closed: bool,
    pub le_lat: f64,
    pub le_lon: f64,
    pub le_hdg: Option<f64>,
    pub le_elev: Option<i32>,
    pub le_displaced: i32,
    pub he_lat: f64,
    pub he_lon: f64,
    pub he_hdg: Option<f64>,
    pub he_elev: Option<i32>,
    pub he_displaced: i32,
}

impl Runway {
    pub fn has_coords(&self) -> bool {
        self.le_lat != 0.0 && self.he_lat != 0.0
    }

    /// Kurs rzeczywisty końca `le` (z bazy albo z położenia obu końców).
    pub fn le_heading(&self) -> Option<f64> {
        self.le_hdg.or_else(|| self.has_coords().then(|| bearing_deg(self.le_lat, self.le_lon, self.he_lat, self.he_lon)))
    }

    pub fn he_heading(&self) -> Option<f64> {
        self.he_hdg.or_else(|| self.le_heading().map(|h| (h + 180.0) % 360.0))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Frequency {
    pub kind: String,
    pub desc: String,
    pub mhz: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Airport {
    pub ident: String,
    /// 0 duże, 1 średnie, 2 małe, 3 lądowisko dla śmigłowców, 4 wodne.
    pub kind: u8,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub elev_ft: Option<i32>,
    pub country: String,
    pub region: String,
    pub city: String,
    pub iata: String,
    pub icao: String,
    pub scheduled: bool,
    pub wiki: String,
    pub home: String,
    pub runways: Vec<Runway>,
    pub freqs: Vec<Frequency>,
}

impl Airport {
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            0 => crate::lang::tr("duże lotnisko"),
            1 => crate::lang::tr("średnie lotnisko"),
            2 => crate::lang::tr("małe lotnisko"),
            3 => crate::lang::tr("lądowisko dla śmigłowców"),
            _ => crate::lang::tr("lotnisko wodne"),
        }
    }

    /// Najdłuższy czynny pas (ft).
    pub fn longest_runway_ft(&self) -> i32 {
        self.runways.iter().filter(|r| !r.closed).map(|r| r.length_ft).max().unwrap_or(0)
    }

    /// Oba końce każdego czynnego pasa – do oceny lądowania.
    pub fn runway_geoms(&self) -> Vec<RunwayGeom> {
        let mut out = Vec::new();
        for r in self.runways.iter().filter(|r| !r.closed && r.has_coords()) {
            let len = (r.length_ft > 0).then_some(r.length_ft as f64);
            if let Some(h) = r.le_heading() {
                out.push(RunwayGeom { ident: r.le.clone(), threshold_lat: r.le_lat, threshold_lon: r.le_lon, heading_true_deg: h, length_ft: len });
            }
            if let Some(h) = r.he_heading() {
                out.push(RunwayGeom { ident: r.he.clone(), threshold_lat: r.he_lat, threshold_lon: r.he_lon, heading_true_deg: h, length_ft: len });
            }
        }
        out
    }
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    fetched_at: i64,
    airports: Vec<Airport>,
}

pub struct AirportDb {
    pub airports: Vec<Airport>,
    pub fetched_at: i64,
    by_ident: HashMap<String, usize>,
    grid: HashMap<(i32, i32), Vec<usize>>,
    folded: Vec<String>,
}

/// Tekst bez polskich i innych znaków diakrytycznych, małymi literami – do wyszukiwania.
pub fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ą' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'Ą' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'a',
            'ć' | 'ç' | 'č' | 'Ć' | 'Ç' | 'Č' => 'c',
            'ę' | 'è' | 'é' | 'ê' | 'ë' | 'ě' | 'Ę' | 'È' | 'É' | 'Ê' | 'Ë' | 'Ě' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
            'ł' | 'Ł' => 'l',
            'ń' | 'ñ' | 'ň' | 'Ń' | 'Ñ' | 'Ň' => 'n',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'ø' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => 'o',
            'ś' | 'š' | 'ş' | 'Ś' | 'Š' | 'Ş' => 's',
            'ù' | 'ú' | 'û' | 'ü' | 'ů' | 'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ů' => 'u',
            'ý' | 'ÿ' | 'Ý' => 'y',
            'ź' | 'ż' | 'ž' | 'Ź' | 'Ż' | 'Ž' => 'z',
            'ř' | 'Ř' => 'r',
            'ß' => 's',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

fn cell(lat: f64, lon: f64) -> (i32, i32) {
    (lat.floor() as i32, lon.floor() as i32)
}

impl AirportDb {
    pub fn new(airports: Vec<Airport>, fetched_at: i64) -> Self {
        let mut by_ident = HashMap::new();
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        let mut folded = Vec::with_capacity(airports.len());
        for (i, a) in airports.iter().enumerate() {
            by_ident.insert(a.ident.to_uppercase(), i);
            if !a.icao.is_empty() {
                by_ident.entry(a.icao.to_uppercase()).or_insert(i);
            }
            grid.entry(cell(a.lat, a.lon)).or_default().push(i);
            folded.push(fold(&format!("{} {} {} {} {}", a.ident, a.iata, a.name, a.city, a.icao)));
        }
        Self { airports, fetched_at, by_ident, grid, folded }
    }

    pub fn len(&self) -> usize {
        self.airports.len()
    }

    pub fn is_empty(&self) -> bool {
        self.airports.is_empty()
    }

    pub fn get(&self, ident: &str) -> Option<&Airport> {
        let u = ident.trim().to_uppercase();
        self.by_ident.get(&u).map(|&i| &self.airports[i]).or_else(|| {
            if u.len() == 3 {
                self.airports.iter().find(|a| a.iata == u)
            } else {
                None
            }
        })
    }

    /// Wyszukiwanie po kodzie ICAO/IATA, nazwie i mieście (bez znaczenia wielkość liter i diakrytyki).
    pub fn search(&self, query: &str, limit: usize) -> Vec<&Airport> {
        let q = fold(query.trim());
        if q.is_empty() {
            return Vec::new();
        }
        let up = query.trim().to_uppercase();
        let words: Vec<&str> = q.split_whitespace().collect();
        let mut scored: Vec<(i32, usize)> = Vec::new();
        for (i, a) in self.airports.iter().enumerate() {
            let s = if a.ident == up || a.icao == up {
                0
            } else if a.iata == up {
                1
            } else if a.ident.starts_with(&up) {
                2
            } else if words.iter().all(|w| self.folded[i].contains(w)) {
                3 + if fold(&a.name).starts_with(&q) { 0 } else { 1 }
            } else {
                continue;
            };
            scored.push((s * 10 + a.kind as i32, i));
        }
        scored.sort();
        scored.into_iter().take(limit).map(|(_, i)| &self.airports[i]).collect()
    }

    /// Lotniska w promieniu (NM), najbliższe pierwsze. `heliports` – czy brać lądowiska śmigłowców.
    pub fn within(&self, lat: f64, lon: f64, radius_nm: f64, heliports: bool) -> Vec<(f64, &Airport)> {
        let deg_lat = (radius_nm / 60.0).ceil() as i32 + 1;
        let deg_lon = (radius_nm / (60.0 * lat.to_radians().cos().max(0.05))).ceil() as i32 + 1;
        let (cy, cx) = cell(lat, lon);
        let mut out = Vec::new();
        for y in cy - deg_lat..=cy + deg_lat {
            for x in cx - deg_lon..=cx + deg_lon {
                let xx = ((x + 180).rem_euclid(360)) - 180;
                if let Some(v) = self.grid.get(&(y, xx)) {
                    for &i in v {
                        let a = &self.airports[i];
                        if !heliports && a.kind == 3 {
                            continue;
                        }
                        let d = distance_nm(lat, lon, a.lat, a.lon);
                        if d <= radius_nm {
                            out.push((d, a));
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }

    /// Najbliższe lotnisko z pasem (do opisu startu i lądowania) w promieniu 10 NM.
    pub fn nearest(&self, lat: f64, lon: f64) -> Option<&Airport> {
        self.within(lat, lon, 10.0, true).into_iter().find(|(d, a)| a.kind != 3 || *d < 0.5).map(|(_, a)| a)
    }

    /// Pasy w pobliżu punktu (do oceny lądowania).
    pub fn runways_near(&self, lat: f64, lon: f64) -> Vec<RunwayGeom> {
        self.within(lat, lon, 5.0, false).into_iter().flat_map(|(_, a)| a.runway_geoms()).collect()
    }
}

// ---------- pobieranie i pamięć podręczna ----------

fn cache_file() -> PathBuf {
    crate::paths::cache().join("airports.json.gz")
}

fn download(name: &str, progress: &dyn Fn(String)) -> Result<String, String> {
    progress(crate::trf!("Pobieranie {name}…"));
    let resp = ureq::get(&format!("{BASE}/{name}")).set("User-Agent", "OpenTakeoff-Windows").timeout(Duration::from_secs(180)).call().map_err(|e| e.to_string())?;
    let mut s = String::new();
    resp.into_reader().take(80 * 1024 * 1024).read_to_string(&mut s).map_err(|e| e.to_string())?;
    Ok(s)
}

fn records(text: &str) -> Result<(HashMap<String, usize>, Vec<csv::StringRecord>), String> {
    let mut rd = csv::ReaderBuilder::new().flexible(true).from_reader(text.as_bytes());
    let headers: HashMap<String, usize> = rd.headers().map_err(|e| e.to_string())?.iter().enumerate().map(|(i, h)| (h.to_string(), i)).collect();
    Ok((headers, rd.records().filter_map(|r| r.ok()).collect()))
}

/// Zamiana trzech plików CSV OurAirports na listę lotnisk z pasami i częstotliwościami.
pub fn parse(airports_csv: &str, runways_csv: &str, freqs_csv: &str) -> Result<Vec<Airport>, String> {
    let (h, rows) = records(airports_csv)?;
    let col = |r: &csv::StringRecord, name: &str| h.get(name).and_then(|&i| r.get(i)).unwrap_or("").trim().to_string();
    let mut list: Vec<Airport> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for r in &rows {
        let kind = match col(r, "type").as_str() {
            "large_airport" => 0,
            "medium_airport" => 1,
            "small_airport" => 2,
            "heliport" => 3,
            "seaplane_base" => 4,
            _ => continue,
        };
        let (Ok(lat), Ok(lon)) = (col(r, "latitude_deg").parse::<f64>(), col(r, "longitude_deg").parse::<f64>()) else { continue };
        let ident = col(r, "ident").to_uppercase();
        let mut icao = col(r, "icao_code").to_uppercase();
        if icao.is_empty() {
            let g = col(r, "gps_code").to_uppercase();
            if g.len() == 4 && g.chars().all(|c| c.is_ascii_alphabetic()) {
                icao = g;
            }
        }
        index.insert(ident.clone(), list.len());
        list.push(Airport {
            ident,
            kind,
            name: col(r, "name"),
            lat,
            lon,
            elev_ft: col(r, "elevation_ft").parse().ok(),
            country: col(r, "iso_country"),
            region: col(r, "iso_region"),
            city: col(r, "municipality"),
            iata: col(r, "iata_code").to_uppercase(),
            icao,
            scheduled: col(r, "scheduled_service") == "yes",
            wiki: col(r, "wikipedia_link"),
            home: col(r, "home_link"),
            runways: Vec::new(),
            freqs: Vec::new(),
        });
    }
    let (h, rows) = records(runways_csv)?;
    let col = |r: &csv::StringRecord, name: &str| h.get(name).and_then(|&i| r.get(i)).unwrap_or("").trim().to_string();
    let f = |s: String| s.parse::<f64>().ok();
    for r in &rows {
        let Some(&i) = index.get(&col(r, "airport_ident").to_uppercase()) else { continue };
        list[i].runways.push(Runway {
            le: col(r, "le_ident"),
            he: col(r, "he_ident"),
            length_ft: col(r, "length_ft").parse().unwrap_or(0),
            width_ft: col(r, "width_ft").parse().unwrap_or(0),
            surface: col(r, "surface"),
            lighted: col(r, "lighted") == "1",
            closed: col(r, "closed") == "1",
            le_lat: f(col(r, "le_latitude_deg")).unwrap_or(0.0),
            le_lon: f(col(r, "le_longitude_deg")).unwrap_or(0.0),
            le_hdg: f(col(r, "le_heading_degT")),
            le_elev: col(r, "le_elevation_ft").parse().ok(),
            le_displaced: col(r, "le_displaced_threshold_ft").parse().unwrap_or(0),
            he_lat: f(col(r, "he_latitude_deg")).unwrap_or(0.0),
            he_lon: f(col(r, "he_longitude_deg")).unwrap_or(0.0),
            he_hdg: f(col(r, "he_heading_degT")),
            he_elev: col(r, "he_elevation_ft").parse().ok(),
            he_displaced: col(r, "he_displaced_threshold_ft").parse().unwrap_or(0),
        });
    }
    let (h, rows) = records(freqs_csv)?;
    let col = |r: &csv::StringRecord, name: &str| h.get(name).and_then(|&i| r.get(i)).unwrap_or("").trim().to_string();
    for r in &rows {
        let Some(&i) = index.get(&col(r, "airport_ident").to_uppercase()) else { continue };
        let Ok(mhz) = col(r, "frequency_mhz").parse::<f64>() else { continue };
        list[i].freqs.push(Frequency { kind: col(r, "type").to_uppercase(), desc: col(r, "description"), mhz });
    }
    for a in &mut list {
        a.runways.sort_by(|x, y| x.closed.cmp(&y.closed).then(y.length_ft.cmp(&x.length_ft)));
    }
    Ok(list)
}

fn load_cache() -> Option<CacheFile> {
    let bytes = std::fs::read(cache_file()).ok()?;
    let mut s = String::new();
    GzDecoder::new(&bytes[..]).read_to_string(&mut s).ok()?;
    let c: CacheFile = serde_json::from_str(&s).ok()?;
    (c.version == CACHE_VERSION && !c.airports.is_empty()).then_some(c)
}

fn save_cache(c: &CacheFile) {
    use std::io::Write;
    let mut gz = GzEncoder::new(Vec::new(), flate2::Compression::fast());
    if gz.write_all(&serde_json::to_vec(c).unwrap_or_default()).is_ok() {
        if let Ok(b) = gz.finish() {
            let _ = crate::paths::write_atomic(&cache_file(), &b);
        }
    }
}

static DB: RwLock<Option<Arc<AirportDb>>> = RwLock::new(None);

/// Załadowana baza (None, dopóki się wczytuje albo pobiera).
pub fn db() -> Option<Arc<AirportDb>> {
    DB.read().clone()
}

/// Wczytuje bazę z pamięci podręcznej albo ją pobiera (blokuje – wołać w tle). `force` – pobierz od nowa.
pub fn load(force: bool, progress: &dyn Fn(String)) -> Result<Arc<AirportDb>, String> {
    let now = crate::source::unix_ms();
    let cached = load_cache();
    if let Some(c) = &cached {
        let age_days = ((now - c.fetched_at).max(0) / 86_400_000) as u64;
        if !force && age_days < REFRESH_DAYS {
            let db = Arc::new(AirportDb::new(c.airports.clone(), c.fetched_at));
            *DB.write() = Some(db.clone());
            return Ok(db);
        }
        if DB.read().is_none() {
            *DB.write() = Some(Arc::new(AirportDb::new(c.airports.clone(), c.fetched_at)));
        }
    }
    let fetched = (|| -> Result<Vec<Airport>, String> {
        let a = download("airports.csv", progress)?;
        let r = download("runways.csv", progress)?;
        let f = download("airport-frequencies.csv", progress)?;
        progress(crate::lang::tr("Przygotowanie bazy lotnisk…").to_string());
        parse(&a, &r, &f)
    })();
    match fetched {
        Ok(list) if !list.is_empty() => {
            let c = CacheFile { version: CACHE_VERSION, fetched_at: now, airports: list };
            save_cache(&c);
            let db = Arc::new(AirportDb::new(c.airports, now));
            *DB.write() = Some(db.clone());
            Ok(db)
        }
        Ok(_) => Err("Pusta baza lotnisk".into()),
        Err(e) => match cached {
            Some(c) => {
                crate::log::warn(&format!("Baza lotnisk: odświeżenie nie powiodło się ({e}), używam zapisanej"));
                let db = Arc::new(AirportDb::new(c.airports, c.fetched_at));
                *DB.write() = Some(db.clone());
                Ok(db)
            }
            None => Err(e),
        },
    }
}

/// Baza do testów i trybu bez sieci.
pub fn install(db: AirportDb) {
    *DB.write() = Some(Arc::new(db));
}

#[cfg(test)]
mod tests {
    use super::*;

    const AIRPORTS: &str = "\"id\",\"ident\",\"type\",\"name\",\"latitude_deg\",\"longitude_deg\",\"elevation_ft\",\"continent\",\"iso_country\",\"iso_region\",\"municipality\",\"scheduled_service\",\"icao_code\",\"iata_code\",\"gps_code\",\"local_code\",\"home_link\",\"wikipedia_link\",\"keywords\"\n\
1,\"EPGD\",\"large_airport\",\"Gdańsk Lech Wałęsa Airport\",54.377602,18.4662,489,\"EU\",\"PL\",\"PL-22\",\"Gdańsk\",\"yes\",\"EPGD\",\"GDN\",\"EPGD\",,,,\n\
2,\"EPWA\",\"large_airport\",\"Warsaw Chopin Airport\",52.165699,20.967100,362,\"EU\",\"PL\",\"PL-14\",\"Warsaw\",\"yes\",\"EPWA\",\"WAW\",\"EPWA\",,,,\n\
3,\"PL-0001\",\"heliport\",\"Szpital\",54.38,18.47,10,\"EU\",\"PL\",\"PL-22\",\"Gdańsk\",\"no\",,,,,,,\n\
4,\"XX99\",\"closed\",\"Old\",54.0,18.0,10,\"EU\",\"PL\",\"PL-22\",\"\",\"no\",,,,,,,\n";
    const RUNWAYS: &str = "\"id\",\"airport_ref\",\"airport_ident\",\"length_ft\",\"width_ft\",\"surface\",\"lighted\",\"closed\",\"le_ident\",\"le_latitude_deg\",\"le_longitude_deg\",\"le_elevation_ft\",\"le_heading_degT\",\"le_displaced_threshold_ft\",\"he_ident\",\"he_latitude_deg\",\"he_longitude_deg\",\"he_elevation_ft\",\"he_heading_degT\",\"he_displaced_threshold_ft\"\n\
1,1,\"EPGD\",9186,148,\"ASP\",1,0,\"11\",54.3884,18.448,470,112,,\"29\",54.379,18.488,489,292,\n";
    const FREQS: &str = "\"id\",\"airport_ref\",\"airport_ident\",\"type\",\"description\",\"frequency_mhz\"\n1,1,\"EPGD\",\"TWR\",\"Rebiechowo Tower\",118.1\n";

    fn db() -> AirportDb {
        AirportDb::new(parse(AIRPORTS, RUNWAYS, FREQS).unwrap(), 0)
    }

    #[test]
    fn parse_and_search() {
        let d = db();
        assert_eq!(d.len(), 3);
        assert_eq!(d.get("epgd").unwrap().runways.len(), 1);
        assert_eq!(d.get("GDN").unwrap().ident, "EPGD");
        assert_eq!(d.search("gdansk", 5)[0].ident, "EPGD");
        assert_eq!(d.search("WAW", 5)[0].ident, "EPWA");
        assert_eq!(d.search("chopin", 5)[0].ident, "EPWA");
        assert_eq!(d.get("EPGD").unwrap().freqs[0].mhz, 118.1);
    }

    #[test]
    fn nearest_and_runways() {
        let d = db();
        assert_eq!(d.nearest(54.37, 18.47).unwrap().ident, "EPGD");
        assert!(d.nearest(50.0, 10.0).is_none());
        let rw = d.runways_near(54.38, 18.47);
        assert_eq!(rw.len(), 2);
        assert_eq!(rw[1].ident, "29");
        assert_eq!(fold("Łódź Śródmieście"), "lodz srodmiescie");
    }
}
