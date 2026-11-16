//! Archiwum lotów .oab (wersja 2): gzip z JSON-em, tor w kolumnach różnic – pozycja w 1e-5° (~1 m), czas
//! i wysokość jako różnice względem poprzedniego punktu. Ten sam zapis co `FlightArchive` w telefonie.

use std::io::{Read, Write};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

use super::model::{FlightLog, TrackPoint};

const SCALE: f64 = 100_000.0;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TrackColumns {
    pub t0: i64,
    pub dt: Vec<i64>,
    pub d_lat: Vec<i64>,
    pub d_lon: Vec<i64>,
    pub d_alt: Vec<i32>,
    pub gs: Vec<i32>,
    pub vs: Vec<Option<i32>>,
    pub ias: Vec<Option<i32>>,
    pub hdg: Vec<Option<i32>>,
    pub bank: Vec<Option<i32>>,
    pub pitch: Vec<Option<i32>>,
    pub agl: Vec<Option<i32>>,
    pub g: Vec<Option<i32>>,
    pub gnd: Vec<Option<i32>>,
}

#[derive(Serialize, Deserialize)]
struct StoredFlight {
    meta: FlightLog,
    cols: TrackColumns,
}

#[derive(Serialize, Deserialize)]
struct ArchiveFile {
    #[serde(default = "app_name")]
    app: String,
    #[serde(default = "format_name")]
    format: String,
    #[serde(default = "two")]
    version: i32,
    flights: Vec<StoredFlight>,
}

fn app_name() -> String {
    "OpenTakeoff".into()
}
fn format_name() -> String {
    "flights".into()
}
fn two() -> i32 {
    2
}

#[derive(Deserialize, Default)]
struct LegacyFile {
    #[serde(default)]
    flights: Vec<FlightLog>,
}

pub fn columns(track: &[TrackPoint]) -> TrackColumns {
    let Some(first) = track.first() else { return TrackColumns::default() };
    let mut c = TrackColumns { t0: first.t, ..Default::default() };
    let (mut pt, mut plat, mut plon, mut palt) = (first.t, 0i64, 0i64, 0i32);
    for p in track {
        let la = (p.lat * SCALE).round() as i64;
        let lo = (p.lon * SCALE).round() as i64;
        c.dt.push(p.t - pt);
        c.d_lat.push(la - plat);
        c.d_lon.push(lo - plon);
        c.d_alt.push(p.alt_ft - palt);
        pt = p.t;
        plat = la;
        plon = lo;
        palt = p.alt_ft;
    }
    c.gs = track.iter().map(|p| p.gs_kt).collect();
    c.vs = track.iter().map(|p| p.vs_fpm).collect();
    c.ias = track.iter().map(|p| p.ias_kt).collect();
    c.hdg = track.iter().map(|p| p.hdg).collect();
    // Kolumny analizy tylko, gdy lot je ma – starsze wpisy nie rosną o listy pełne null
    fn opt(track: &[TrackPoint], f: impl Fn(&TrackPoint) -> Option<i32>) -> Vec<Option<i32>> {
        if track.iter().any(|p| f(p).is_some()) {
            track.iter().map(f).collect()
        } else {
            Vec::new()
        }
    }
    c.bank = opt(track, |p| p.bank_deg.map(|v| (v * 10.0).round() as i32));
    c.pitch = opt(track, |p| p.pitch_deg.map(|v| (v * 10.0).round() as i32));
    c.agl = opt(track, |p| p.agl_ft);
    c.g = opt(track, |p| p.g_load.map(|v| (v * 100.0).round() as i32));
    c.gnd = opt(track, |p| p.on_ground.map(|v| v as i32));
    c
}

pub fn track(c: &TrackColumns) -> Vec<TrackPoint> {
    let (mut t, mut la, mut lo, mut alt) = (c.t0, 0i64, 0i64, 0i32);
    let n = c.dt.len().min(c.d_lat.len()).min(c.d_lon.len()).min(c.d_alt.len());
    let get = |v: &Vec<Option<i32>>, i: usize| v.get(i).copied().flatten();
    (0..n)
        .map(|i| {
            t += c.dt[i];
            la += c.d_lat[i];
            lo += c.d_lon[i];
            alt += c.d_alt[i];
            TrackPoint {
                t,
                lat: la as f64 / SCALE,
                lon: lo as f64 / SCALE,
                alt_ft: alt,
                gs_kt: c.gs.get(i).copied().unwrap_or(0),
                vs_fpm: get(&c.vs, i),
                ias_kt: get(&c.ias, i),
                hdg: get(&c.hdg, i),
                bank_deg: get(&c.bank, i).map(|v| v as f32 / 10.0),
                pitch_deg: get(&c.pitch, i).map(|v| v as f32 / 10.0),
                agl_ft: get(&c.agl, i),
                g_load: get(&c.g, i).map(|v| v as f32 / 100.0),
                on_ground: get(&c.gnd, i).map(|v| v == 1),
            }
        })
        .collect()
}

pub fn encode(flights: &[FlightLog]) -> Vec<u8> {
    let file = ArchiveFile {
        app: app_name(),
        format: format_name(),
        version: 2,
        flights: flights.iter().map(|f| StoredFlight { meta: FlightLog { track: Vec::new(), ..f.clone() }, cols: columns(&f.track) }).collect(),
    };
    let json = serde_json::to_vec(&file).unwrap_or_default();
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    let _ = gz.write_all(&json);
    gz.finish().unwrap_or_default()
}

/// Czyta archiwum (gzip) albo zwykły JSON – nowy lub stary format.
pub fn decode(bytes: &[u8]) -> Result<Vec<FlightLog>, String> {
    let text = if bytes.len() > 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut s = String::new();
        GzDecoder::new(bytes).read_to_string(&mut s).map_err(|e| e.to_string())?;
        s
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?
    };
    if text.contains("\"cols\"") {
        let f: ArchiveFile = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        Ok(f.flights.into_iter().map(|s| FlightLog { track: track(&s.cols), ..s.meta }).collect())
    } else {
        let f: LegacyFile = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        Ok(f.flights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FlightLog {
        let track = (0..50)
            .map(|i| TrackPoint {
                t: 1_000_000 + i * 1000,
                lat: 52.1 + i as f64 * 0.001,
                lon: 20.9 - i as f64 * 0.0005,
                alt_ft: 300 + i as i32 * 50,
                gs_kt: 140,
                vs_fpm: Some(1500),
                ias_kt: Some(150),
                hdg: Some(331),
                bank_deg: if i == 3 { Some(-2.5) } else { None },
                ..Default::default()
            })
            .collect();
        FlightLog { id: "abc".into(), start: 1_000_000, end: 1_049_000, aircraft: Some("A320".into()), max_alt_ft: 2750, distance_nm: 3, track, ..Default::default() }
    }

    #[test]
    fn roundtrip() {
        let f = sample();
        let back = decode(&encode(&[f.clone()])).unwrap();
        assert_eq!(back.len(), 1);
        let b = &back[0];
        assert_eq!(b.id, "abc");
        assert_eq!(b.track.len(), 50);
        assert!((b.track[10].lat - f.track[10].lat).abs() < 1e-9);
        assert_eq!(b.track[49].alt_ft, f.track[49].alt_ft);
        assert_eq!(b.track[3].bank_deg, Some(-2.5));
        assert_eq!(b.track[4].bank_deg, None);
        assert_eq!(b.track[4].agl_ft, None);
    }

    #[test]
    fn phone_archive_json() {
        // Zapis telefonu (kotlinx.serialization, explicitNulls = false)
        let json = r#"{"app":"OpenTakeoff","format":"flights","version":2,"flights":[{"meta":{"id":"x1","start":10,"end":200000,"aircraft":"C172","from":"EPGD","maxAltFt":3000,"distanceNm":40,"track":[],"landings":[{"score":88,"grade":"Dobre","vsFpm":-150,"bounces":0,"notes":["a"]}],"markers":[]},"cols":{"t0":10,"dt":[0,1000],"dLat":[5000000,10],"dLon":[1800000,-10],"dAlt":[100,20],"gs":[90,91],"vs":[null,300],"ias":[95,96],"hdg":[10,11]}}]}"#;
        let f = decode(json.as_bytes()).unwrap();
        assert_eq!(f[0].from.as_deref(), Some("EPGD"));
        assert_eq!(f[0].to, None);
        assert_eq!(f[0].track.len(), 2);
        assert_eq!(f[0].track[1].alt_ft, 120);
        assert_eq!(f[0].track[0].vs_fpm, None);
        assert_eq!(f[0].all_landings()[0].score, 88);
    }

    #[test]
    fn meta_always_has_track_field() {
        let bytes = encode(&[sample()]);
        let mut gz = GzDecoder::new(&bytes[..]);
        let mut s = String::new();
        gz.read_to_string(&mut s).unwrap();
        assert!(s.contains("\"track\":[]"));
        assert!(!s.contains("\"to\":null"));
    }

    #[test]
    fn legacy_json() {
        let json = r#"{"flights":[{"id":"o","start":1,"end":2,"maxAltFt":1,"distanceNm":0,"track":[{"t":1,"lat":1.0,"lon":2.0,"altFt":3,"gsKt":4}]}]}"#;
        let f = decode(json.as_bytes()).unwrap();
        assert_eq!(f[0].track[0].gs_kt, 4);
    }
}
