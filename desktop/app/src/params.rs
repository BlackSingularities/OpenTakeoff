//! Układ „Parametry” na karcie Przyrządy: wszystkie dane z symulatora pogrupowane w karty.
//! Skróty jak w kabinie (IAS, AGL, N1…); kolor wartości: 0 zwykły, 1 włączone (zielony),
//! 2 ostrzeżenie (bursztyn), 3 wyłączone / brak (przygaszony), 4 nastawa (cyjan).

use opentakeoff_core::lang::tr;
use opentakeoff_core::proto::FlightState;
use opentakeoff_core::trf;

use crate::avionics::gfx::*;

pub struct Group {
    pub title: &'static str,
    pub items: Vec<(String, String, i32)>,
}

const NORMAL: i32 = 0;
const ON: i32 = 1;
const WARN: i32 = 2;
const OFF: i32 = 3;
const SET: i32 = 4;

struct G(Vec<(String, String, i32)>);

impl G {
    fn kv(&mut self, k: &str, v: impl Into<String>) -> &mut Self {
        self.0.push((k.to_string(), v.into(), NORMAL));
        self
    }
    fn tone(&mut self, k: &str, v: impl Into<String>, t: i32) -> &mut Self {
        self.0.push((k.to_string(), v.into(), t));
        self
    }
    fn flag(&mut self, k: &str, on: bool) -> &mut Self {
        self.0.push((k.to_string(), if on { "ON".into() } else { "OFF".into() }, if on { ON } else { OFF }));
        self
    }
}

fn deg(v: f64) -> String {
    format!("{:03.0}°", v.rem_euclid(360.0))
}

fn sgn(v: f64, d: usize, unit: &str) -> String {
    format!("{v:+.d$}{unit}")
}

fn dms(v: f64, pos: char, neg: char) -> String {
    let h = if v >= 0.0 { pos } else { neg };
    let a = v.abs();
    let d = a.floor();
    let m = (a - d) * 60.0;
    format!("{h}{d:.0}°{m:06.3}'")
}

fn mhz(v: f64) -> String {
    if v > 0.0 {
        format!("{v:.3}")
    } else {
        "–".into()
    }
}

fn ete(sec: f64) -> String {
    if sec <= 0.0 || !sec.is_finite() {
        return "–".into();
    }
    let s = sec.round() as i64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn per_engine(s: &FlightState, f: impl Fn(&opentakeoff_core::proto::EngineData) -> String) -> String {
    s.engines.iter().map(f).collect::<Vec<_>>().join("  ")
}

pub fn groups(s: &FlightState) -> Vec<Group> {
    let mut out = Vec::new();
    let mut push = |title: &'static str, g: G| {
        if !g.0.is_empty() {
            out.push(Group { title, items: g.0 });
        }
    };
    let av = s.avionics.as_ref();

    // Samolot
    let mut g = G(Vec::new());
    g.kv(tr("Samolot"), s.aircraft.title.clone());
    if !s.aircraft.atc_type.is_empty() || !s.aircraft.atc_model.is_empty() {
        g.kv("ATC", format!("{} {}", s.aircraft.atc_type, s.aircraft.atc_model).trim().to_string());
    }
    g.kv(tr("Silnik"), s.aircraft.engine_type.clone());
    if !s.aircraft.category.is_empty() {
        g.kv(tr("Kategoria"), s.aircraft.category.clone());
    }
    g.kv(tr("Źródło"), s.source.to_uppercase());
    g.tone(tr("Na ziemi"), if s.aircraft.on_ground { tr("TAK") } else { tr("NIE") }, if s.aircraft.on_ground { ON } else { OFF });
    g.tone(tr("Pauza"), if s.aircraft.paused { "ON" } else { "OFF" }, if s.aircraft.paused { WARN } else { OFF });
    if let Some(t) = &s.sim_time_utc {
        g.kv("UTC", t.clone());
    }
    if let Some(a) = av {
        g.tone("STALL", if a.stall_warning { "WARN" } else { "–" }, if a.stall_warning { WARN } else { OFF });
        g.tone("OVERSPEED", if a.overspeed_warning { "WARN" } else { "–" }, if a.overspeed_warning { WARN } else { OFF });
    }
    push("Samolot", g);

    // Pozycja
    let p = &s.position;
    let mut g = G(Vec::new());
    g.kv("LAT", dms(p.lat, 'N', 'S')).kv("LON", dms(p.lon, 'E', 'W'));
    g.kv(tr("Dziesiętnie"), format!("{:.5} {:.5}", p.lat, p.lon));
    g.kv("ALT MSL", format!("{:.0} ft", p.alt_msl_ft)).kv("ALT IND", format!("{:.0} ft", p.alt_ind_ft)).kv("AGL", format!("{:.0} ft", p.alt_agl_ft));
    g.kv(tr("Teren"), format!("{:.0} ft", p.alt_msl_ft - p.alt_agl_ft));
    if let Some(a) = av {
        g.kv("PRESS ALT", format!("{:.0} ft", a.pressure_alt_ft));
        if a.radio_alt_ft > 0.0 && a.radio_alt_ft < 2600.0 {
            g.kv("RA", format!("{:.0} ft", a.radio_alt_ft));
        }
        if a.decision_height_ft > 0.0 {
            g.tone("DH", format!("{:.0} ft", a.decision_height_ft), SET);
        }
    }
    push("Pozycja", g);

    // Prędkości
    let v = &s.speed;
    let mut g = G(Vec::new());
    g.kv("IAS", format!("{:.0} kt", v.ias_kt)).kv("TAS", format!("{:.0} kt", v.tas_kt)).kv("GS", format!("{:.0} kt", v.gs_kt));
    g.kv("MACH", format!("{:.3}", v.mach)).kv("V/S", format!("{:+.0} fpm", v.vs_fpm));
    if v.gs_kt > 30.0 && v.vs_fpm.abs() > 50.0 {
        // kąt ścieżki z GS i V/S
        let fpa = (v.vs_fpm / (v.gs_kt * 101.27)).atan().to_degrees();
        g.kv("FPA", sgn(fpa, 1, "°"));
    }
    if let Some(a) = av {
        let sp = &a.speeds;
        for (k, x) in [("VS0", sp.vs0_kt), ("VS1", sp.vs1_kt), ("VC", sp.vc_kt), ("VMO", sp.vmo_kt)] {
            if x > 0.0 {
                g.kv(k, format!("{x:.0} kt"));
            }
        }
    }
    push("Prędkości", g);

    // Położenie
    let at = &s.attitude;
    let mut g = G(Vec::new());
    g.kv("PITCH", sgn(at.pitch_deg, 1, "°")).kv("BANK", sgn(at.bank_deg, 1, "°"));
    g.kv("HDG MAG", deg(at.hdg_mag_deg)).kv("HDG TRUE", deg(at.hdg_true_deg)).kv("TRK TRUE", deg(at.track_true_deg));
    if v.gs_kt > 20.0 {
        let drift = (at.track_true_deg - at.hdg_true_deg + 540.0).rem_euclid(360.0) - 180.0;
        g.kv("DRIFT", sgn(drift, 1, "°"));
    }
    if let Some(a) = av {
        g.tone("G", format!("{:.2}", a.g_force), if a.g_force > 2.0 || a.g_force < 0.0 { WARN } else { NORMAL });
        g.kv("AOA", sgn(a.aoa_deg, 1, "°"));
        g.kv(tr("Zakręt"), format!("{:+.1}°/s", a.turn_rate_dps));
        g.kv(tr("Kulka"), sgn(a.slip_ball, 2, ""));
        g.kv("MAG VAR", sgn(a.mag_var_deg, 1, "°"));
    }
    push("Położenie", g);

    // Autopilot
    let ap = &s.autopilot;
    let mut g = G(Vec::new());
    g.flag("AP", ap.master).flag("FD", ap.fd).flag("A/THR", ap.athr);
    g.tone("SPD", format!("{:.0} kt", ap.spd_kt), SET).tone("HDG", deg(ap.hdg_deg), SET).tone("ALT", format!("{:.0} ft", ap.alt_ft), SET).tone("V/S", format!("{:+.0} fpm", ap.vs_fpm), SET);
    let m = &ap.modes;
    let mut modes: Vec<&str> = [("HDG", m.hdg), ("NAV", m.nav), ("APR", m.apr), ("LOC", m.loc), ("ALT", m.alt), ("VS", m.vs), ("FLC", m.flc)].iter().filter(|x| x.1).map(|x| x.0).collect();
    if let Some(a) = av {
        let x = &a.ap;
        for (k, on) in [("ALT ARM", x.alt_armed), ("GS ARM", x.gs_armed), ("GS", x.gs_active), ("BC", x.back_course), ("LVL", x.wing_leveler), ("PIT", x.pitch_hold), ("SPD", x.airspeed_hold), ("MACH", x.mach_hold), ("TOGA", x.toga), ("YD", x.yaw_damper)] {
            if on {
                modes.push(k);
            }
        }
        if a.fd.active {
            g.tone("FD CMD", format!("{:+.1}° / {:+.1}°", a.fd.pitch_deg, a.fd.bank_deg), SET);
        }
    }
    g.tone(tr("Tryby"), if modes.is_empty() { "–".to_string() } else { modes.join(" ") }, if modes.is_empty() { OFF } else { ON });
    push("Autopilot", g);

    // GPS
    if let Some(a) = av {
        let gp = &a.gps;
        let mut g = G(Vec::new());
        g.flag(tr("Plan"), gp.active_flight_plan).flag("APPR", gp.approach_active).flag("GPS → NAV1", gp.drives_nav1);
        if gp.active_flight_plan {
            g.kv(tr("Odcinek"), format!("{} → {}", if gp.prev_id.is_empty() { "–" } else { &gp.prev_id }, if gp.next_id.is_empty() { "–" } else { &gp.next_id }));
            g.kv(tr("Punkt"), format!("{} / {}", gp.wp_index, gp.wp_count));
            g.kv("DIST", format!("{:.1} NM", gp.next_dist_nm)).kv("BRG", deg(gp.next_bearing_mag_deg)).tone("DTK", deg(gp.desired_track_mag_deg), SET);
            g.tone("XTK", format!("{:+.2} NM", gp.cross_track_nm), if gp.cross_track_nm.abs() > 1.0 { WARN } else { NORMAL });
            g.kv("ETE WPT", ete(gp.next_ete_sec)).kv("ETE DEST", ete(gp.dest_ete_sec));
        }
        push("GPS", g);
    }

    // Radio
    let r = &s.radios;
    let mut g = G(Vec::new());
    g.kv("COM1", mhz(r.com1.active_mhz)).tone("COM1 STBY", mhz(r.com1.standby_mhz), SET);
    g.kv("COM2", mhz(r.com2.active_mhz)).tone("COM2 STBY", mhz(r.com2.standby_mhz), SET);
    g.kv("NAV1", mhz(r.nav1.active_mhz)).tone("NAV1 STBY", mhz(r.nav1.standby_mhz), SET);
    g.kv("NAV2", mhz(r.nav2.active_mhz)).tone("NAV2 STBY", mhz(r.nav2.standby_mhz), SET);
    let x = &r.transponder;
    g.kv("XPDR", format!("{} {}", x.code, x.mode).trim().to_string());
    if x.ident {
        g.tone("IDENT", "ON", ON);
    }
    push("Radio", g);

    // Odbiorniki NAV i ADF
    if let Some(a) = av {
        let mut g = G(Vec::new());
        for (n, rx) in [("NAV1", &a.nav1), ("NAV2", &a.nav2)] {
            if !rx.has_nav {
                g.tone(n, tr("brak sygnału"), OFF);
                continue;
            }
            let id = if rx.ident.is_empty() { n.to_string() } else { format!("{n} {}", rx.ident) };
            g.tone(&id, format!("OBS {}  R{}", deg(rx.obs_deg), deg(rx.radial_deg)), ON);
            g.kv(&format!("{n} CDI"), format!("{:+.1} {}", rx.cdi_dots, rx.to_from));
            if rx.has_gs {
                g.kv(&format!("{n} GS"), format!("{:+.1}", rx.gs_dots));
            }
            if rx.has_dme {
                g.kv(&format!("{n} DME"), format!("{:.1} NM", rx.dme_nm));
            }
        }
        g.tone("ADF", if a.adf.has_signal { format!("{} REL", deg(a.adf.rel_bearing_deg)) } else { tr("brak sygnału").to_string() }, if a.adf.has_signal { ON } else { OFF });
        if !a.marker.is_empty() {
            g.tone("MKR", a.marker.clone(), ON);
        }
        push("Odbiorniki", g);
    }

    // Silniki
    if !s.engines.is_empty() {
        let mut g = G(Vec::new());
        let any = |f: fn(&opentakeoff_core::proto::EngineData) -> f64| s.engines.iter().any(|e| f(e) > 0.0);
        if any(|e| e.n1_pct) {
            g.kv("N1 %", per_engine(s, |e| format!("{:.1}", e.n1_pct)));
        }
        if any(|e| e.n2_pct) {
            g.kv("N2 %", per_engine(s, |e| format!("{:.1}", e.n2_pct)));
        }
        if any(|e| e.rpm) {
            g.kv("RPM", per_engine(s, |e| format!("{:.0}", e.rpm)));
        }
        if any(|e| e.torque_pct) {
            g.kv("TQ %", per_engine(s, |e| format!("{:.1}", e.torque_pct)));
        }
        if any(|e| e.prop_rpm) {
            g.kv("PROP RPM", per_engine(s, |e| format!("{:.0}", e.prop_rpm)));
        }
        g.kv("EGT °C", per_engine(s, |e| format!("{:.0}", e.egt_c)));
        g.kv("FF kg/h", per_engine(s, |e| format!("{:.0}", e.fuel_flow_kg_h)));
        g.kv("THR %", per_engine(s, |e| format!("{:.0}", e.throttle_pct)));
        push("Silniki", g);
    }

    // Paliwo
    let f = &s.fuel;
    let mut g = G(Vec::new());
    g.kv(tr("Razem"), format!("{:.0} kg", f.total_kg));
    let t = &f.tanks;
    if t.left_kg + t.center_kg + t.right_kg > 0.0 {
        g.kv(tr("Lewy"), format!("{:.0} kg", t.left_kg)).kv(tr("Środkowy"), format!("{:.0} kg", t.center_kg)).kv(tr("Prawy"), format!("{:.0} kg", t.right_kg));
        let imb = (t.left_kg - t.right_kg).abs();
        g.tone(tr("Różnica L/P"), format!("{imb:.0} kg"), if imb > 500.0 { WARN } else { NORMAL });
    }
    let ff: f64 = s.engines.iter().map(|e| e.fuel_flow_kg_h).sum();
    if ff > 1.0 {
        g.kv(tr("Przepływ"), format!("{ff:.0} kg/h"));
        let h = f.total_kg / ff;
        g.tone(tr("Wystarczy na"), ete(h * 3600.0), if h < 0.75 { WARN } else { NORMAL });
    }
    push("Paliwo", g);

    // Sterowanie
    let c = &s.controls;
    let mut g = G(Vec::new());
    g.tone(tr("Podwozie"), if c.gear_pct >= 99.0 { "DOWN".to_string() } else if c.gear_pct <= 1.0 { "UP".to_string() } else { format!("{:.0}%", c.gear_pct) }, if c.gear_down { ON } else { NORMAL });
    g.kv(tr("Klapy"), format!("{} / {}  ({:.0}%)", c.flaps_index, (c.flaps_positions - 1).max(0), c.flaps_pct));
    g.kv(tr("Spojlery"), format!("{:.0}%", c.spoilers_pct));
    g.flag(tr("Spojlery uzbr."), c.spoilers_armed);
    g.tone(tr("Hamulec postojowy"), if c.parking_brake { "SET" } else { "OFF" }, if c.parking_brake { WARN } else { OFF });
    if let Some(a) = av {
        g.kv(tr("Hamulce L/P"), format!("{:.0}% / {:.0}%", a.brake_left_pct, a.brake_right_pct));
    }
    g.kv(tr("Trym"), format!("{:+.0}%", c.elevator_trim_pct));
    g.kv(tr("Przepustnica"), format!("{:.0}%", c.throttle_pct));
    if c.mixture_pct > 0.0 {
        g.kv(tr("Mieszanka"), format!("{:.0}%", c.mixture_pct));
    }
    if c.prop_pct > 0.0 {
        g.kv(tr("Śmigło"), format!("{:.0}%", c.prop_pct));
    }
    push("Sterowanie", g);

    // Światła
    let l = &s.lights;
    let mut g = G(Vec::new());
    for (k, on) in [("BEACON", l.beacon), ("STROBE", l.strobe), ("NAV", l.nav), ("LANDING", l.landing), ("TAXI", l.taxi), ("LOGO", l.logo), ("WING", l.wing), ("PANEL", l.panel), ("CABIN", l.cabin)] {
        g.flag(k, on);
    }
    push("Światła", g);

    // Środowisko
    let e = &s.environment;
    let mut g = G(Vec::new());
    g.kv(tr("Wiatr"), format!("{} / {:.0} kt", deg(e.wind_dir_deg), e.wind_spd_kt));
    if e.wind_spd_kt > 0.5 {
        let rel = (e.wind_dir_deg - at.hdg_true_deg).to_radians();
        g.kv(tr("Czołowy / boczny"), format!("{:+.0} / {:+.0} kt", e.wind_spd_kt * rel.cos(), e.wind_spd_kt * rel.sin()));
    }
    g.kv("OAT", format!("{:+.0} °C", e.oat_c));
    if let Some(a) = av {
        g.kv("TAT", format!("{:+.0} °C", a.tat_c));
        g.tone("BARO", if a.baro_std { "STD".to_string() } else { format!("{:.0} hPa", a.baro_hpa) }, SET);
        if a.suction_in_hg > 0.0 {
            g.kv(tr("Podciśnienie"), format!("{:.1} inHg", a.suction_in_hg));
        }
    }
    g.kv("QNH", format!("{:.0} hPa", e.qnh_hpa));
    // ISA: odchyłka temperatury od atmosfery wzorcowej na wysokości ciśnieniowej
    let pa = av.map(|a| a.pressure_alt_ft).unwrap_or(p.alt_msl_ft);
    g.kv("ISA", format!("{:+.0} °C", e.oat_c - (15.0 - 1.98 * pa / 1000.0)));
    push("Środowisko", g);

    out
}

/// Tytuły grup do tłumaczenia (Slint dostaje już przetłumaczone).
pub fn title(t: &'static str) -> &'static str {
    match t {
        "Samolot" => tr("Samolot"),
        "Pozycja" => tr("Pozycja"),
        "Prędkości" => tr("Prędkości"),
        "Położenie" => tr("Położenie"),
        "Autopilot" => tr("Autopilot"),
        "Radio" => tr("Radio"),
        "Odbiorniki" => tr("Odbiorniki"),
        "Silniki" => tr("Silniki"),
        "Paliwo" => tr("Paliwo"),
        "Sterowanie" => tr("Sterowanie"),
        "Światła" => tr("Światła"),
        "Środowisko" => tr("Środowisko"),
        x => x,
    }
}

// ---------- historia i wykresy (tryb Dane) ----------

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    t: f64,
    alt: f32,
    ias: f32,
    gs: f32,
    vs: f32,
    pitch: f32,
    bank: f32,
    n1: f32,
    rpm: f32,
    ff: f32,
    fuel: f32,
}

/// Ostatnie 10 minut, próbka co sekundę.
const KEEP_S: f64 = 600.0;

thread_local! {
    static HIST: std::cell::RefCell<std::collections::VecDeque<Sample>> = std::cell::RefCell::new(std::collections::VecDeque::new());
    static GEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Dopisuje próbkę do historii (wołane co sekundę z głównego zegara, także gdy karta jest zamknięta).
pub fn sample(s: &FlightState) {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let n = s.engines.len().max(1) as f64;
    let smp = Sample {
        t,
        alt: s.position.alt_ind_ft as f32,
        ias: s.speed.ias_kt as f32,
        gs: s.speed.gs_kt as f32,
        vs: s.speed.vs_fpm as f32,
        pitch: s.attitude.pitch_deg as f32,
        bank: s.attitude.bank_deg as f32,
        n1: (s.engines.iter().map(|e| e.n1_pct).sum::<f64>() / n) as f32,
        rpm: (s.engines.iter().map(|e| e.rpm).sum::<f64>() / n) as f32,
        ff: s.engines.iter().map(|e| e.fuel_flow_kg_h).sum::<f64>() as f32,
        fuel: s.fuel.total_kg as f32,
    };
    HIST.with(|h| {
        let mut h = h.borrow_mut();
        // przerwa (np. nowy lot) – zacznij od nowa
        if h.back().is_some_and(|b| t - b.t > 30.0) {
            h.clear();
        }
        h.push_back(smp);
        while h.front().is_some_and(|f| t - f.t > KEEP_S) {
            h.pop_front();
        }
    });
    GEN.with(|g| g.set(g.get() + 1));
}

/// Numer kolejnej próbki – wykres przerysowujemy tylko, gdy przybyło danych.
pub fn generation() -> u64 {
    GEN.with(|g| g.get())
}

struct Series<'a> {
    name: &'a str,
    color: Argb,
    get: fn(&Sample) -> f32,
}

/// Siatka wykresów z ostatnich 10 minut: wysokość, prędkości, V/S, pochylenie i przechylenie, silniki, paliwo.
pub fn draw_charts(c: &mut Canvas, w: f32, h: f32, k: f32) {
    const BG: Argb = 0xFF0B1016;
    const CY: Argb = 0xFF4FD8EB;
    const GR: Argb = 0xFF3EE07A;
    const AM: Argb = 0xFFFFB547;
    const DIM: Argb = 0xFF6F7C8A;
    c.rect(0.0, 0.0, w, h, BG);
    let data: Vec<Sample> = HIST.with(|h| h.borrow().iter().copied().collect());
    if data.len() < 2 {
        c.text_f(tr("Wykresy pojawią się po kilku sekundach lotu"), w / 2.0, h / 2.0, 12.0 * k, DIM, Align::Center, Face::Cond);
        return;
    }
    let piston = data.iter().all(|s| s.n1 < 1.0);
    let charts: Vec<(&str, &str, Vec<Series>)> = vec![
        ("ALT", "ft", vec![Series { name: "ALT", color: CY, get: |s| s.alt }]),
        ("IAS / GS", "kt", vec![Series { name: "IAS", color: CY, get: |s| s.ias }, Series { name: "GS", color: GR, get: |s| s.gs }]),
        ("V/S", "fpm", vec![Series { name: "V/S", color: CY, get: |s| s.vs }]),
        ("PITCH / BANK", "°", vec![Series { name: "PITCH", color: CY, get: |s| s.pitch }, Series { name: "BANK", color: AM, get: |s| s.bank }]),
        if piston { ("RPM", "", vec![Series { name: "RPM", color: GR, get: |s| s.rpm }]) } else { ("N1", "%", vec![Series { name: "N1", color: GR, get: |s| s.n1 }]) },
        ("FUEL / FF", "kg", vec![Series { name: "FUEL", color: AM, get: |s| s.fuel }, Series { name: "FF", color: CY, get: |s| s.ff }]),
    ];
    let cols = if w / h > 4.0 { 6 } else if w / h > 1.6 { 3 } else { 2 };
    let rows = charts.len().div_ceil(cols);
    let gap = 8.0 * k;
    let cw = (w - gap * (cols as f32 + 1.0)) / cols as f32;
    let ch = (h - gap * (rows as f32 + 1.0)) / rows as f32;
    let t1 = data.last().map(|s| s.t).unwrap_or(0.0);
    let t0 = (t1 - KEEP_S).max(data[0].t);
    for (i, (title, unit, series)) in charts.iter().enumerate() {
        let (cx, cy) = (gap + (i % cols) as f32 * (cw + gap), gap + (i / cols) as f32 * (ch + gap));
        c.round_rect(cx, cy, cx + cw, cy + ch, 4.0 * k, 0xFF101820, 0x14FFFFFF, 1.0 * k);
        c.text_f(title, cx + 8.0 * k, cy + 11.0 * k, 10.0 * k, CY, Align::Left, Face::CondBold);
        // bieżące wartości po prawej
        let mut tx = cx + cw - 8.0 * k;
        for s in series.iter().rev() {
            let v = (s.get)(data.last().unwrap());
            let txt = if s.name == "V/S" || s.name.starts_with("PITCH") || s.name == "BANK" { format!("{v:+.0}") } else { format!("{v:.0}") };
            let txt = format!("{txt}{}", if series.len() == 1 && !unit.is_empty() { format!(" {unit}") } else { String::new() });
            c.text_f(&txt, tx, cy + 11.0 * k, 11.0 * k, s.color, Align::Right, Face::MonoBold);
            tx -= text_width(&txt, 11.0 * k, Face::MonoBold) + 10.0 * k;
        }
        let (pl, pr, pt, pb) = (cx + 8.0 * k, cx + cw - 8.0 * k, cy + 22.0 * k, cy + ch - 8.0 * k);
        if pb - pt < 10.0 {
            continue;
        }
        // zakres osi Y ze wszystkich serii
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for s in series {
            for d in &data {
                let v = (s.get)(d);
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        if *title == "V/S" || title.starts_with("PITCH") {
            let m = lo.abs().max(hi.abs()).max(if *title == "V/S" { 500.0 } else { 5.0 });
            lo = -m;
            hi = m;
        }
        let span = (hi - lo).max(1.0);
        lo -= span * 0.08;
        hi += span * 0.08;
        let y = |v: f32| pb - (v - lo) / (hi - lo) * (pb - pt);
        let x = |t: f64| pl + ((t - t0) / (t1 - t0).max(1.0)) as f32 * (pr - pl);
        if lo < 0.0 && hi > 0.0 {
            c.dash_line(pl, y(0.0), pr, y(0.0), 0x30FFFFFF, 1.0 * k, 3.0 * k, 4.0 * k);
        }
        c.text_f(&format!("{hi:.0}"), pl, pt + 4.0 * k, 8.5 * k, DIM, Align::Left, Face::Mono);
        c.text_f(&format!("{lo:.0}"), pl, pb - 4.0 * k, 8.5 * k, DIM, Align::Left, Face::Mono);
        for s in series {
            let pts: Vec<f32> = data.iter().filter(|d| d.t >= t0).flat_map(|d| [x(d.t), y((s.get)(d))]).collect();
            if pts.len() >= 4 {
                c.polyline(s.color, 1.6 * k, &pts);
            }
        }
    }
    let mins = ((t1 - t0) / 60.0).round().max(1.0);
    c.text_f(&trf!("ostatnie {m:.0} min", m = mins), w - gap - 4.0 * k, h - 3.0 * k, 8.5 * k, DIM, Align::Right, Face::Cond);
}
