//! Kalkulatory lotnicze (narzędzia w programie): wiatr, wysokości, prędkości, zniżanie, paliwo, jednostki.

/// Składowe wiatru dla pasa: (czołowa +/tylna −, boczna + z prawej).
pub fn wind_components(wind_dir: f64, wind_kt: f64, runway_heading: f64) -> (f64, f64) {
    crate::logbook::landing::wind_components(wind_dir, wind_kt, runway_heading)
}

/// Wysokość ciśnieniowa (ft) z wysokości elewacji i QNH (hPa).
pub fn pressure_altitude(elev_ft: f64, qnh_hpa: f64) -> f64 {
    elev_ft + (1013.25 - qnh_hpa) * 27.0
}

/// Wysokość gęstościowa (ft) – przybliżenie 120 ft na każdy °C ponad ISA.
pub fn density_altitude(elev_ft: f64, qnh_hpa: f64, oat_c: f64) -> f64 {
    let pa = pressure_altitude(elev_ft, qnh_hpa);
    let isa = 15.0 - 1.98 * pa / 1000.0;
    pa + 120.0 * (oat_c - isa)
}

/// Prędkość rzeczywista z przyrządowej (przybliżenie 2 % na 1000 ft wysokości gęstościowej).
pub fn tas_from_ias(ias_kt: f64, density_alt_ft: f64) -> f64 {
    ias_kt * (1.0 + 0.02 * density_alt_ft / 1000.0)
}

/// Mach z prędkości rzeczywistej i temperatury.
pub fn mach(tas_kt: f64, oat_c: f64) -> f64 {
    tas_kt / (661.47 * ((oat_c + 273.15) / 288.15).sqrt())
}

/// Kąt znoszenia i prędkość nad ziemią: (WCA °, GS kt, kurs do utrzymania °).
pub fn wind_triangle(course_deg: f64, tas_kt: f64, wind_dir: f64, wind_kt: f64) -> (f64, f64, f64) {
    let a = (wind_dir - course_deg).to_radians();
    let s = (wind_kt * a.sin() / tas_kt.max(1.0)).clamp(-1.0, 1.0);
    let wca = s.asin();
    let gs = tas_kt * wca.cos() - wind_kt * a.cos();
    let hdg = (course_deg + wca.to_degrees()).rem_euclid(360.0);
    (wca.to_degrees(), gs.max(0.0), hdg)
}

/// Plan zniżania: (odległość do początku zniżania NM, wymagana prędkość pionowa ft/min).
pub fn descent(from_ft: f64, to_ft: f64, gs_kt: f64, angle_deg: f64) -> (f64, f64) {
    let drop = (from_ft - to_ft).max(0.0);
    let nm = drop / (angle_deg.to_radians().tan() * 6076.12);
    let vs = gs_kt * 6076.12 / 60.0 * angle_deg.to_radians().tan();
    (nm, vs)
}

/// Paliwo potrzebne na lot: (minuty lotu, paliwo na przelot, z zapasem).
pub fn fuel(distance_nm: f64, gs_kt: f64, burn_per_hour: f64, reserve_min: f64) -> (f64, f64, f64) {
    let min = distance_nm / gs_kt.max(1.0) * 60.0;
    let trip = burn_per_hour * min / 60.0;
    let total = trip + burn_per_hour * reserve_min / 60.0;
    (min, trip, total)
}

/// Przeliczniki jednostek: (nazwa, mnożnik do jednostki bazowej) w grupach.
pub const UNITS: &[(&str, &[(&str, f64)])] = &[
    ("Długość", &[("m", 1.0), ("ft", 0.3048), ("km", 1000.0), ("NM", 1852.0), ("SM", 1609.344)]),
    ("Prędkość", &[("kt", 1.0), ("km/h", 0.539957), ("m/s", 1.943844), ("mph", 0.868976), ("ft/min", 0.00987473)]),
    ("Masa", &[("kg", 1.0), ("lb", 0.45359237)]),
    ("Objętość", &[("l", 1.0), ("US gal", 3.785411784), ("imp gal", 4.54609)]),
    ("Ciśnienie", &[("hPa", 1.0), ("inHg", 33.8639), ("mmHg", 1.333224)]),
];

pub fn convert(value: f64, group: usize, from: usize, to: usize) -> f64 {
    let Some((_, units)) = UNITS.get(group) else { return value };
    let (Some(a), Some(b)) = (units.get(from), units.get(to)) else { return value };
    value * a.1 / b.1
}

pub fn celsius_to_f(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

/// Wilgotność względna (%) z temperatury i punktu rosy (wzór Magnusa).
pub fn relative_humidity(t: f64, td: f64) -> f64 {
    let g = |x: f64| (17.625 * x) / (243.04 + x);
    (100.0 * (g(td) - g(t)).exp()).clamp(0.0, 100.0)
}

/// Przybliżona podstawa chmur kłębiastych (ft AGL) z rozkładu temperatury i punktu rosy.
pub fn cloud_base_ft(t: f64, td: f64) -> f64 {
    ((t - td) / 2.5 * 1000.0).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculators() {
        assert!((pressure_altitude(0.0, 1003.25) - 270.0).abs() < 1e-9);
        assert!((density_altitude(0.0, 1013.25, 25.0) - 1200.0).abs() < 1e-6);
        let (wca, gs, hdg) = wind_triangle(360.0, 100.0, 90.0, 20.0);
        assert!((wca - 11.54).abs() < 0.1, "{wca}");
        assert!((gs - 97.98).abs() < 0.1);
        assert!((hdg - 11.54).abs() < 0.1);
        let (nm, vs) = descent(35000.0, 5000.0, 420.0, 3.0);
        assert!((nm - 94.3).abs() < 0.5, "{nm}");
        assert!((vs - 2229.0).abs() < 10.0, "{vs}");
        assert!((convert(1.0, 0, 3, 1) - 6076.1).abs() < 0.1);
        assert!((relative_humidity(20.0, 20.0) - 100.0).abs() < 1e-6);
    }
}
