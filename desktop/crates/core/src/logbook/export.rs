//! Eksport lotów do formatów wymiany: GPX 1.1 (ślad z wysokością i czasem), KML (ślad 3D do Google Earth,
//! z punktami lądowań) i CSV (pełny tor z parametrami – do arkusza).

use super::model::{FlightLog, TrackPoint};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).unwrap_or_default().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub fn title(f: &FlightLog) -> String {
    let route = format!("{} – {}", f.from.as_deref().unwrap_or("?"), f.to.as_deref().unwrap_or("?"));
    match &f.aircraft {
        Some(a) => format!("{route} ({a})"),
        None => route,
    }
}

pub fn gpx(f: &FlightLog) -> String {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str("<gpx version=\"1.1\" creator=\"OpenTakeoff\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n");
    s.push_str(&format!("  <metadata><name>{}</name><time>{}</time></metadata>\n", esc(&title(f)), iso(f.start)));
    for (i, l) in f.all_landings().iter().enumerate() {
        if let Some(at) = l.at {
            if let Some(p) = nearest(&f.track, at) {
                s.push_str(&format!(
                    "  <wpt lat=\"{:.6}\" lon=\"{:.6}\"><ele>{:.1}</ele><time>{}</time><name>{}</name><desc>{} {} ft/min</desc></wpt>\n",
                    p.lat,
                    p.lon,
                    p.alt_ft as f64 * 0.3048,
                    iso(at),
                    esc(&format!("Lądowanie {}", i + 1)),
                    l.score,
                    l.vs_fpm
                ));
            }
        }
    }
    s.push_str(&format!("  <trk><name>{}</name><trkseg>\n", esc(&title(f))));
    for p in &f.track {
        s.push_str(&format!("    <trkpt lat=\"{:.6}\" lon=\"{:.6}\"><ele>{:.1}</ele><time>{}</time></trkpt>\n", p.lat, p.lon, p.alt_ft as f64 * 0.3048, iso(p.t)));
    }
    s.push_str("  </trkseg></trk>\n</gpx>\n");
    s
}

fn nearest(track: &[TrackPoint], t: i64) -> Option<&TrackPoint> {
    track.iter().min_by_key(|p| (p.t - t).abs())
}

pub fn kml(f: &FlightLog) -> String {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\" xmlns:gx=\"http://www.google.com/kml/ext/2.2\">\n<Document>\n");
    s.push_str(&format!("<name>{}</name>\n", esc(&title(f))));
    s.push_str("<Style id=\"track\"><LineStyle><color>ff3c8cff</color><width>3</width></LineStyle><PolyStyle><color>403c8cff</color></PolyStyle></Style>\n");
    s.push_str("<Placemark><name>Tor lotu</name><styleUrl>#track</styleUrl><gx:Track><altitudeMode>absolute</altitudeMode><extrude>1</extrude>\n");
    for p in &f.track {
        s.push_str(&format!("<when>{}</when>\n", iso(p.t)));
    }
    for p in &f.track {
        s.push_str(&format!("<gx:coord>{:.6} {:.6} {:.1}</gx:coord>\n", p.lon, p.lat, p.alt_ft as f64 * 0.3048));
    }
    s.push_str("</gx:Track></Placemark>\n");
    for (i, l) in f.all_landings().iter().enumerate() {
        if let Some(p) = l.at.and_then(|at| nearest(&f.track, at)) {
            s.push_str(&format!(
                "<Placemark><name>{}</name><description>{} · {} ft/min{}</description><Point><coordinates>{:.6},{:.6},0</coordinates></Point></Placemark>\n",
                esc(&format!("Lądowanie {} – {}", i + 1, l.score)),
                esc(&l.grade),
                l.vs_fpm,
                l.runway.as_ref().map(|r| format!(" · RWY {}", esc(r))).unwrap_or_default(),
                p.lon,
                p.lat
            ));
        }
    }
    s.push_str("</Document>\n</kml>\n");
    s
}

pub fn csv(f: &FlightLog) -> String {
    let mut s = String::from("time_utc,elapsed_s,lat,lon,alt_ft,agl_ft,gs_kt,ias_kt,vs_fpm,hdg_deg,bank_deg,pitch_deg,g,on_ground\n");
    let opt = |v: Option<i32>| v.map(|x| x.to_string()).unwrap_or_default();
    let optf = |v: Option<f32>| v.map(|x| format!("{x:.1}")).unwrap_or_default();
    for p in &f.track {
        s.push_str(&format!(
            "{},{},{:.6},{:.6},{},{},{},{},{},{},{},{},{},{}\n",
            iso(p.t),
            (p.t - f.start) / 1000,
            p.lat,
            p.lon,
            p.alt_ft,
            opt(p.agl_ft),
            p.gs_kt,
            opt(p.ias_kt),
            opt(p.vs_fpm),
            opt(p.hdg),
            optf(p.bank_deg),
            optf(p.pitch_deg),
            p.g_load.map(|g| format!("{g:.2}")).unwrap_or_default(),
            p.on_ground.map(|g| if g { "1" } else { "0" }).unwrap_or_default()
        ));
    }
    s
}

/// Lista lotów (bez torów) do arkusza.
pub fn logbook_csv(flights: &[FlightLog]) -> String {
    let mut s = String::from("date_utc,from,to,aircraft,category,duration_min,distance_nm,max_alt_ft,landings,last_landing_score,last_landing_vs_fpm,id\n");
    let q = |v: &str| if v.contains([',', '"', '\n']) { format!("\"{}\"", v.replace('"', "\"\"")) } else { v.to_string() };
    for f in flights {
        let l = f.last_landing();
        s.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            iso(f.start),
            q(f.from.as_deref().unwrap_or("")),
            q(f.to.as_deref().unwrap_or("")),
            q(f.aircraft.as_deref().unwrap_or("")),
            q(f.category.as_deref().unwrap_or("")),
            f.duration_min(),
            f.distance_nm,
            f.max_alt_ft,
            f.all_landings().len(),
            l.as_ref().map(|l| l.score.to_string()).unwrap_or_default(),
            l.as_ref().map(|l| l.vs_fpm.to_string()).unwrap_or_default(),
            f.id
        ));
    }
    s
}

/// Import śladu GPX jako lotu (np. z innego programu). Prędkość i kurs liczone z kolejnych punktów.
pub fn from_gpx(text: &str) -> Result<FlightLog, String> {
    let mut points: Vec<TrackPoint> = Vec::new();
    for chunk in text.split("<trkpt").skip(1) {
        let attr = |name: &str| -> Option<f64> {
            let k = format!("{name}=\"");
            let i = chunk.find(&k)? + k.len();
            chunk[i..].split('"').next()?.parse().ok()
        };
        let tag = |name: &str| -> Option<&str> {
            let o = format!("<{name}>");
            let i = chunk.find(&o)? + o.len();
            chunk[i..].split("</").next()
        };
        let (Some(lat), Some(lon)) = (attr("lat"), attr("lon")) else { continue };
        let ele_m: f64 = tag("ele").and_then(|e| e.trim().parse().ok()).unwrap_or(0.0);
        let t = tag("time").and_then(|t| chrono::DateTime::parse_from_rfc3339(t.trim()).ok()).map(|d| d.timestamp_millis());
        let t = t.unwrap_or_else(|| points.last().map(|p| p.t + 1000).unwrap_or(0));
        points.push(TrackPoint { t, lat, lon, alt_ft: (ele_m / 0.3048).round() as i32, ..Default::default() });
    }
    if points.len() < 2 {
        return Err("Plik GPX nie zawiera śladu".into());
    }
    for i in 1..points.len() {
        let (a, b) = (points[i - 1].clone(), points[i].clone());
        let dt_h = ((b.t - a.t).max(1)) as f64 / 3_600_000.0;
        let d = crate::navdata::math::distance_nm(a.lat, a.lon, b.lat, b.lon);
        points[i].gs_kt = (d / dt_h).round() as i32;
        points[i].vs_fpm = Some(((b.alt_ft - a.alt_ft) as f64 / (dt_h * 60.0)).round() as i32);
        points[i].hdg = Some(crate::navdata::math::bearing_deg(a.lat, a.lon, b.lat, b.lon).round() as i32);
    }
    let dist: f64 = points.windows(2).map(|w| crate::navdata::math::distance_nm(w[0].lat, w[0].lon, w[1].lat, w[1].lon)).sum();
    Ok(FlightLog {
        id: super::model::new_id(),
        start: points[0].t,
        end: points[points.len() - 1].t,
        max_alt_ft: points.iter().map(|p| p.alt_ft).max().unwrap_or(0),
        distance_nm: dist as i32,
        track: points,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f() -> FlightLog {
        FlightLog {
            id: "x".into(),
            start: 0,
            end: 120_000,
            from: Some("EPWA".into()),
            to: Some("EPGD".into()),
            track: vec![
                TrackPoint { t: 0, lat: 52.0, lon: 21.0, alt_ft: 1000, gs_kt: 100, ..Default::default() },
                TrackPoint { t: 60_000, lat: 52.02, lon: 21.0, alt_ft: 1500, gs_kt: 100, ..Default::default() },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn gpx_roundtrip() {
        let g = gpx(&f());
        assert!(g.contains("EPWA – EPGD"));
        let back = from_gpx(&g).unwrap();
        assert_eq!(back.track.len(), 2);
        assert!((back.track[1].alt_ft - 1500).abs() <= 1);
        assert!((back.track[1].gs_kt - 72).abs() <= 2);
    }

    #[test]
    fn kml_and_csv() {
        assert!(kml(&f()).contains("<gx:coord>21.000000 52.020000 457.2</gx:coord>"));
        let c = csv(&f());
        assert_eq!(c.lines().count(), 3);
        assert!(logbook_csv(&[f()]).contains("EPWA,EPGD"));
    }
}
