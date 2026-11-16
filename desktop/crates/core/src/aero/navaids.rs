//! Pomoce nawigacyjne OurAirports (domena publiczna): VOR, VOR-DME, VORTAC, NDB, DME, TACAN.
//! Pobierane raz (ok. 2 MB CSV) razem z bazą lotnisk, trzymane w pamięci podręcznej i odświeżane co 30 dni.
//! Służą do warstwy radiowej na mapie (róże VOR z radialami) i na ND.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::navdata::math::distance_nm;

const URL: &str = "https://davidmegginson.github.io/ourairports-data/navaids.csv";
const REFRESH_DAYS: u64 = 30;
const CACHE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Vor,
    VorDme,
    Vortac,
    Tacan,
    Ndb,
    NdbDme,
    Dme,
}

impl Kind {
    pub fn is_vor(self) -> bool {
        matches!(self, Kind::Vor | Kind::VorDme | Kind::Vortac)
    }
    pub fn is_ndb(self) -> bool {
        matches!(self, Kind::Ndb | Kind::NdbDme)
    }
    pub fn label(self) -> &'static str {
        match self {
            Kind::Vor => "VOR",
            Kind::VorDme => "VOR-DME",
            Kind::Vortac => "VORTAC",
            Kind::Tacan => "TACAN",
            Kind::Ndb => "NDB",
            Kind::NdbDme => "NDB-DME",
            Kind::Dme => "DME",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Navaid {
    pub ident: String,
    pub name: String,
    pub kind: Kind,
    /// VOR/DME w MHz, NDB w kHz.
    pub freq: f64,
    pub lat: f64,
    pub lon: f64,
    pub elev_ft: Option<i32>,
    /// Deklinacja magnetyczna stacji (E +) – radiale VOR są względem niej.
    pub mag_var: f64,
    pub country: String,
    pub airport: String,
}

impl Navaid {
    pub fn freq_text(&self) -> String {
        if self.kind.is_ndb() {
            format!("{:.0} kHz", self.freq)
        } else {
            format!("{:.2} MHz", self.freq)
        }
    }
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    fetched_at: i64,
    navaids: Vec<Navaid>,
}

pub struct NavaidDb {
    pub navaids: Vec<Navaid>,
    grid: HashMap<(i32, i32), Vec<usize>>,
}

fn cell(lat: f64, lon: f64) -> (i32, i32) {
    (lat.floor() as i32, lon.floor() as i32)
}

impl NavaidDb {
    pub fn new(navaids: Vec<Navaid>) -> Self {
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, n) in navaids.iter().enumerate() {
            grid.entry(cell(n.lat, n.lon)).or_default().push(i);
        }
        Self { navaids, grid }
    }

    /// Pomoce w promieniu (NM), najbliższe pierwsze.
    pub fn within(&self, lat: f64, lon: f64, radius_nm: f64) -> Vec<(f64, &Navaid)> {
        let deg_lat = (radius_nm / 60.0).ceil() as i32 + 1;
        let deg_lon = (radius_nm / (60.0 * lat.to_radians().cos().max(0.05))).ceil() as i32 + 1;
        let (cy, cx) = cell(lat, lon);
        let mut out = Vec::new();
        for y in cy - deg_lat..=cy + deg_lat {
            for x in cx - deg_lon..=cx + deg_lon {
                let xx = ((x + 180).rem_euclid(360)) - 180;
                if let Some(v) = self.grid.get(&(y, xx)) {
                    for &i in v {
                        let n = &self.navaids[i];
                        let d = distance_nm(lat, lon, n.lat, n.lon);
                        if d <= radius_nm {
                            out.push((d, n));
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }

    pub fn find(&self, ident: &str) -> Vec<&Navaid> {
        let id = ident.trim().to_uppercase();
        self.navaids.iter().filter(|n| n.ident == id).collect()
    }

    /// Wyszukiwanie po identyfikatorze lub nazwie.
    pub fn search(&self, q: &str, limit: usize) -> Vec<&Navaid> {
        let q = super::airports::fold(q.trim());
        if q.is_empty() {
            return Vec::new();
        }
        let mut exact: Vec<&Navaid> = Vec::new();
        let mut other: Vec<&Navaid> = Vec::new();
        for n in &self.navaids {
            if n.ident.eq_ignore_ascii_case(&q) {
                exact.push(n);
            } else if super::airports::fold(&n.name).contains(&q) {
                other.push(n);
            }
            if exact.len() + other.len() > limit * 4 {
                break;
            }
        }
        exact.extend(other);
        exact.truncate(limit);
        exact
    }
}

pub fn parse(csv_text: &str) -> Result<Vec<Navaid>, String> {
    let mut rd = csv::ReaderBuilder::new().flexible(true).from_reader(csv_text.as_bytes());
    let h: HashMap<String, usize> = rd.headers().map_err(|e| e.to_string())?.iter().enumerate().map(|(i, h)| (h.to_string(), i)).collect();
    let mut out = Vec::new();
    for r in rd.records().filter_map(|r| r.ok()) {
        let col = |name: &str| h.get(name).and_then(|&i| r.get(i)).unwrap_or("").trim().to_string();
        let kind = match col("type").as_str() {
            "VOR" => Kind::Vor,
            "VOR-DME" => Kind::VorDme,
            "VORTAC" => Kind::Vortac,
            "TACAN" => Kind::Tacan,
            "NDB" => Kind::Ndb,
            "NDB-DME" => Kind::NdbDme,
            "DME" => Kind::Dme,
            _ => continue,
        };
        let (Ok(lat), Ok(lon)) = (col("latitude_deg").parse::<f64>(), col("longitude_deg").parse::<f64>()) else { continue };
        let khz: f64 = col("frequency_khz").parse().unwrap_or(0.0);
        let freq = if kind.is_ndb() { khz } else { khz / 1000.0 };
        out.push(Navaid {
            ident: col("ident").to_uppercase(),
            name: col("name"),
            kind,
            freq,
            lat,
            lon,
            elev_ft: col("elevation_ft").parse().ok(),
            mag_var: col("magnetic_variation_deg").parse().unwrap_or(0.0),
            country: col("iso_country"),
            airport: col("associated_airport").to_uppercase(),
        });
    }
    Ok(out)
}

static DB: RwLock<Option<Arc<NavaidDb>>> = RwLock::new(None);

pub fn db() -> Option<Arc<NavaidDb>> {
    DB.read().clone()
}

pub fn install(list: Vec<Navaid>) {
    *DB.write() = Some(Arc::new(NavaidDb::new(list)));
}

fn cache_file() -> std::path::PathBuf {
    crate::paths::cache().join("navaids.json.gz")
}

fn load_cache() -> Option<CacheFile> {
    let bytes = std::fs::read(cache_file()).ok()?;
    let mut s = String::new();
    GzDecoder::new(&bytes[..]).read_to_string(&mut s).ok()?;
    let c: CacheFile = serde_json::from_str(&s).ok()?;
    (c.version == CACHE_VERSION && !c.navaids.is_empty()).then_some(c)
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

/// Wczytuje bazę z pamięci podręcznej albo ją pobiera (blokuje – wołać w tle).
pub fn load(force: bool) -> Result<usize, String> {
    let now = crate::source::unix_ms();
    let cached = load_cache();
    if let Some(c) = &cached {
        let age_days = ((now - c.fetched_at).max(0) / 86_400_000) as u64;
        install(c.navaids.clone());
        if !force && age_days < REFRESH_DAYS {
            return Ok(c.navaids.len());
        }
    }
    let fetched = (|| -> Result<Vec<Navaid>, String> {
        let resp = ureq::get(URL).set("User-Agent", "OpenTakeoff-Windows").timeout(Duration::from_secs(120)).call().map_err(|e| e.to_string())?;
        let mut s = String::new();
        resp.into_reader().take(40 * 1024 * 1024).read_to_string(&mut s).map_err(|e| e.to_string())?;
        parse(&s)
    })();
    match fetched {
        Ok(list) if !list.is_empty() => {
            let n = list.len();
            save_cache(&CacheFile { version: CACHE_VERSION, fetched_at: now, navaids: list.clone() });
            install(list);
            Ok(n)
        }
        Ok(_) => Err("Pusta baza pomocy nawigacyjnych".into()),
        Err(e) => match cached {
            Some(c) => Ok(c.navaids.len()),
            None => Err(e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = "\"id\",\"filename\",\"ident\",\"name\",\"type\",\"frequency_khz\",\"latitude_deg\",\"longitude_deg\",\"elevation_ft\",\"iso_country\",\"dme_frequency_khz\",\"dme_channel\",\"dme_latitude_deg\",\"dme_longitude_deg\",\"dme_elevation_ft\",\"slaved_variation_deg\",\"magnetic_variation_deg\",\"usageType\",\"power\",\"associated_airport\"\n\
1,\"x\",\"GDA\",\"Gdańsk\",\"VOR-DME\",114200,54.38,18.47,500,\"PL\",,,,,,,5.2,\"BOTH\",\"HIGH\",\"EPGD\"\n\
2,\"y\",\"GD\",\"Gdańsk NDB\",\"NDB\",400,54.40,18.40,,\"PL\",,,,,,,5.0,\"TERMINAL\",\"LOW\",\"EPGD\"\n";

    #[test]
    fn parse_and_query() {
        let l = parse(CSV).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].kind, Kind::VorDme);
        assert!((l[0].freq - 114.2).abs() < 1e-9);
        assert_eq!(l[1].freq_text(), "400 kHz");
        let db = NavaidDb::new(l);
        assert_eq!(db.within(54.38, 18.47, 10.0).len(), 2);
        assert_eq!(db.find("gda")[0].mag_var, 5.2);
        assert_eq!(db.search("gdansk", 5).len(), 2);
    }
}
