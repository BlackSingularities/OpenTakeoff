//! Formatowanie wartości do wyświetlenia (jednostki z ustawień, czasy, daty).

use chrono::{Local, TimeZone};
use opentakeoff_core::lang::tr;

#[derive(Clone, Copy)]
pub struct Units {
    pub meters: bool,
    pub kmh: bool,
}

impl Units {
    pub fn alt(&self, ft: f64) -> String {
        if self.meters {
            format!("{:.0}", ft * 0.3048)
        } else {
            format!("{:.0}", ft)
        }
    }
    pub fn alt_unit(&self) -> &'static str {
        if self.meters {
            "m"
        } else {
            "ft"
        }
    }
    pub fn alt_full(&self, ft: f64) -> String {
        format!("{} {}", self.alt(ft), self.alt_unit())
    }
    pub fn spd(&self, kt: f64) -> String {
        if self.kmh {
            format!("{:.0} km/h", kt * 1.852)
        } else {
            format!("{:.0} kt", kt)
        }
    }
    pub fn dist(&self, nm: f64) -> String {
        if self.kmh {
            format!("{:.0} km", nm * 1.852)
        } else {
            format!("{:.0} NM", nm)
        }
    }
}

pub fn duration_min(min: i64) -> String {
    if min < 60 {
        format!("{min} min")
    } else {
        format!("{} h {:02} min", min / 60, min % 60)
    }
}

pub fn hours(min: i64) -> String {
    format!("{} h {:02} min", min / 60, min % 60)
}

pub fn date(ms: i64) -> String {
    Local.timestamp_millis_opt(ms).single().map(|d| d.format("%d.%m.%Y").to_string()).unwrap_or_default()
}

pub fn time(ms: i64) -> String {
    Local.timestamp_millis_opt(ms).single().map(|d| d.format("%H:%M").to_string()).unwrap_or_default()
}

pub fn date_time(ms: i64) -> String {
    Local.timestamp_millis_opt(ms).single().map(|d| d.format("%d.%m.%Y %H:%M").to_string()).unwrap_or_default()
}

pub fn clock(ms: i64) -> String {
    Local.timestamp_millis_opt(ms).single().map(|d| d.format("%H:%M:%S").to_string()).unwrap_or_default()
}

pub fn month_name(m: u32) -> &'static str {
    match m {
        1 => tr("styczeń"),
        2 => tr("luty"),
        3 => tr("marzec"),
        4 => tr("kwiecień"),
        5 => tr("maj"),
        6 => tr("czerwiec"),
        7 => tr("lipiec"),
        8 => tr("sierpień"),
        9 => tr("wrzesień"),
        10 => tr("październik"),
        11 => tr("listopad"),
        _ => tr("grudzień"),
    }
}

pub fn month_short(m: u32) -> &'static str {
    match m {
        1 => tr("sty"),
        2 => tr("lut"),
        3 => tr("mar"),
        4 => tr("kwi"),
        5 => tr("maj"),
        6 => tr("cze"),
        7 => tr("lip"),
        8 => tr("sie"),
        9 => tr("wrz"),
        10 => tr("paź"),
        11 => tr("lis"),
        _ => tr("gru"),
    }
}

/// „przed chwilą”, „5 min temu”, „2 h temu”, data.
pub fn ago(ms: i64) -> String {
    if ms <= 0 {
        return tr("nigdy").to_string();
    }
    let s = (opentakeoff_core::source::unix_ms() - ms) / 1000;
    if s < 60 {
        tr("przed chwilą").to_string()
    } else if s < 3600 {
        opentakeoff_core::trf!("{m} min temu", m = s / 60)
    } else if s < 86_400 {
        opentakeoff_core::trf!("{h} h temu", h = s / 3600)
    } else {
        date_time(ms)
    }
}

pub fn ete(sec: f64) -> String {
    if sec <= 0.0 || !sec.is_finite() {
        return String::new();
    }
    let s = sec as i64;
    if s >= 3600 {
        format!("{}:{:02} h", s / 3600, s % 3600 / 60)
    } else {
        format!("{}:{:02} min", s / 60, s % 60)
    }
}

pub fn latlon(lat: f64, lon: f64) -> String {
    let ns = if lat >= 0.0 { "N" } else { "S" };
    let ew = if lon >= 0.0 { "E" } else { "W" };
    format!("{:.4}° {ns}  {:.4}° {ew}", lat.abs(), lon.abs())
}

pub fn bytes(n: u64) -> String {
    if n > 1024 * 1024 {
        format!("{:.1} MB", n as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:.0} kB", n as f64 / 1024.0)
    }
}

/// Liczba lotów z odmianą (1 lot, 2 loty, 5 lotów).
pub fn flights(n: usize) -> String {
    let word = if opentakeoff_core::lang::current() != "pl" {
        if n == 1 {
            tr("lot")
        } else {
            tr("lotów")
        }
    } else if n == 1 {
        "lot"
    } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
        "loty"
    } else {
        "lotów"
    };
    format!("{n} {word}")
}
