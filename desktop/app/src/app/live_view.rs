//! Karta Na żywo: odczyt lotu, zegary, lampki, profil wysokości i mapa bieżącego lotu.
//! Część modułu `app` – wspólne pomocnicze funkcje i importy z `super`.

use super::*;

// ---------- Na żywo ----------

pub(super) fn phase(s: &opentakeoff_core::proto::FlightState) -> String {
    if s.aircraft.on_ground {
        if s.speed.gs_kt > 30.0 {
            tr("rozbieg / dobieg").into()
        } else if s.speed.gs_kt > 2.0 {
            tr("kołowanie").into()
        } else {
            tr("na ziemi").into()
        }
    } else if s.speed.vs_fpm > 400.0 {
        tr("wznoszenie").into()
    } else if s.speed.vs_fpm < -400.0 {
        if s.position.alt_agl_ft < 3000.0 {
            tr("podejście").into()
        } else {
            tr("zniżanie").into()
        }
    } else {
        tr("przelot").into()
    }
}

/// Etap lotu na osi karty Na żywo: 0 kołowanie, 1 start, 2 wznoszenie, 3 przelot, 4 zniżanie,
/// 5 podejście, 6 po lądowaniu. Nagrywanie zaczyna się dopiero w powietrzu, więc rozbieg z trwającym
/// nagraniem to dobieg po lądowaniu.
pub(super) fn phase_index(s: &opentakeoff_core::proto::FlightState, recording: bool) -> i32 {
    if s.aircraft.on_ground {
        match (recording, s.speed.gs_kt > 30.0) {
            (false, false) => 0,
            (false, true) => 1,
            (true, _) => 6,
        }
    } else if s.speed.vs_fpm > 400.0 {
        2
    } else if s.speed.vs_fpm < -400.0 {
        if s.position.alt_agl_ft < 3000.0 {
            5
        } else {
            4
        }
    } else if s.position.alt_agl_ft < 1500.0 && s.speed.vs_fpm < -100.0 {
        5
    } else {
        3
    }
}

pub(super) fn parse_sim_utc(t: Option<&str>) -> Option<chrono::DateTime<chrono::Utc>> {
    let t = t?;
    chrono::DateTime::parse_from_rfc3339(t).ok().map(|d| d.with_timezone(&chrono::Utc)).or_else(|| chrono::NaiveDateTime::parse_from_str(t.trim_end_matches('Z'), "%Y-%m-%dT%H:%M:%S").ok().map(|n| n.and_utc()))
}

/// Godzina symulatora „HH:MM:SSZ”.
pub(super) fn utc_clock(t: Option<&str>) -> String {
    parse_sim_utc(t).map(|d| d.format("%H:%MZ").to_string()).unwrap_or_default()
}

/// Przewidywana godzina przylotu (czas symulatora + pozostały czas).
pub(super) fn utc_plus(t: Option<&str>, sec: f64) -> String {
    parse_sim_utc(t).map(|d| (d + chrono::Duration::seconds(sec as i64)).format("%H:%MZ").to_string()).unwrap_or_default()
}

pub(super) fn lamp(k: &str, v: &str, on: bool, tone: i32) -> LampV {
    LampV { k: ss(k), v: ss(v), on, tone }
}

// Zegar silnika jak na E/WD: łuk 210° od lewego dołu (0 %) do prawej (100 %), środek (50, 56), promień 38,
// w polu 100×100. Kąt liczony od góry zgodnie z ruchem wskazówek.
pub(super) const GAUGE_START: f64 = -120.0;
pub(super) const GAUGE_SWEEP: f64 = 210.0;

pub(super) fn gauge_pt(frac: f64, r: f64) -> (f64, f64) {
    let a = (GAUGE_START + GAUGE_SWEEP * frac).to_radians();
    (50.0 + r * a.sin(), 56.0 - r * a.cos())
}

pub(super) fn gauge_arc(frac: f64) -> String {
    let f = frac.clamp(0.0, 1.1);
    if f < 0.005 {
        return String::new();
    }
    let (x0, y0) = gauge_pt(0.0, 38.0);
    let (x1, y1) = gauge_pt(f, 38.0);
    let large = if GAUGE_SWEEP * f > 180.0 { 1 } else { 0 };
    format!("M {x0:.2} {y0:.2} A 38 38 0 {large} 1 {x1:.2} {y1:.2}")
}

pub(super) fn gauge_needle(frac: f64) -> String {
    let f = frac.clamp(0.0, 1.1);
    let (x1, y1) = gauge_pt(f, 42.0);
    let (x0, y0) = gauge_pt(f, 8.0);
    format!("M {x0:.2} {y0:.2} L {x1:.2} {y1:.2}")
}

/// Znacznik położenia dźwigni na zewnątrz łuku.
pub(super) fn gauge_tick(frac: f64) -> String {
    let (x0, y0) = gauge_pt(frac, 43.0);
    let (x1, y1) = gauge_pt(frac, 49.0);
    format!("M {x0:.2} {y0:.2} L {x1:.2} {y1:.2}")
}

/// Profil wysokości z bieżącego śladu: linia i wypełnienie w polu 200×60 (do 160 próbek).
pub(super) fn alt_profile(track: &[crate::host::LivePoint]) -> (String, String, i32) {
    if track.len() < 2 {
        return (String::new(), String::new(), 0);
    }
    let n = track.len().min(160);
    let pts: Vec<i32> = (0..n).map(|i| track[i * (track.len() - 1) / (n - 1).max(1)].alt).collect();
    let top = pts.iter().copied().max().unwrap_or(0);
    let lo = pts.iter().copied().min().unwrap_or(0).min(0);
    let span = ((top - lo) as f64).max(500.0);
    let mut line = String::new();
    for (i, a) in pts.iter().enumerate() {
        let x = i as f64 * 200.0 / (n - 1) as f64;
        let y = 58.0 - (*a - lo) as f64 / span * 54.0;
        line.push_str(&format!("{} {x:.1} {y:.1} ", if i == 0 { "M" } else { "L" }));
    }
    let area = format!("{line}L 200 60 L 0 60 Z");
    (line, area, top)
}

pub fn refresh_live(force_map: bool) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let tab = app.get_tab();
    // Dane lotu trafiają do paska narzędzi i panelu bocznego na każdej karcie; mapa i PFD tylko
    // na Pulpicie i karcie Na żywo.
    let with_map = tab == 0 || tab == 1;
    let st = state();
    let mut st = st.borrow_mut();
    let u = units(&st.s);
    let snap = st.host.snapshot();
    let mut live = app.get_live();
    live.has_data = snap.is_some();
    if let Some(s) = &snap {
        let db = airports::db();
        let near = db.as_ref().and_then(|d| d.within(s.position.lat, s.position.lon, 60.0, false).into_iter().next().map(|(dist, a)| format!("{} {} · {:.0} NM", a.ident, a.name, dist)));
        let av = s.avionics.clone().unwrap_or_default();
        live.title = ss(&s.aircraft.title);
        live.category = ss(&s.aircraft.category);
        live.source = ss(&s.source);
        live.phase = ss(phase(s));
        live.on_ground = s.aircraft.on_ground;
        live.paused = s.aircraft.paused;
        live.alt = ss(u.alt(s.position.alt_ind_ft));
        live.alt_unit = ss(u.alt_unit());
        live.agl = ss(format!("{}", u.alt_full(if av.radio_alt_ft > 0.0 { av.radio_alt_ft } else { s.position.alt_agl_ft })));
        live.ias = ss(format!("{:.0}", s.speed.ias_kt));
        live.tas = ss(format!("{:.0}", s.speed.tas_kt));
        live.gs = ss(format!("{:.0}", s.speed.gs_kt));
        live.mach = ss(if s.speed.mach > 0.3 { format!("{:.2}", s.speed.mach) } else { String::new() });
        live.vs = ss(format!("{:+.0}", s.speed.vs_fpm));
        live.vs_value = s.speed.vs_fpm as f32;
        live.hdg = ss(format!("{:03.0}", s.attitude.hdg_mag_deg.rem_euclid(360.0)));
        live.trk = ss(format!("{:03.0}", s.attitude.track_true_deg.rem_euclid(360.0)));
        live.pitch = s.attitude.pitch_deg as f32;
        live.bank = s.attitude.bank_deg as f32;
        live.position = ss(fmt::latlon(s.position.lat, s.position.lon));
        live.nearest = ss(near.unwrap_or_default());
        live.wind = ss(format!("{:03.0}° / {:.0} kt", s.environment.wind_dir_deg, s.environment.wind_spd_kt));
        live.oat = ss(format!("{:.0} °C", s.environment.oat_c));
        live.qnh = ss(if av.baro_std { "STD".to_string() } else { format!("{:.0} hPa", s.environment.qnh_hpa) });
        live.fuel = ss(format!("{:.0} kg", s.fuel.total_kg));
        let (_, rec_since, _, fuel0, last) = st.host.recording();
        live.fuel_level = fuel0.map(|f| (s.fuel.total_kg / f.max(1.0)) as f32).unwrap_or(1.0);
        let ap = &s.autopilot;
        let mut modes = Vec::new();
        for (on, n) in [(ap.modes.hdg, "HDG"), (ap.modes.nav, "NAV"), (ap.modes.loc, "LOC"), (ap.modes.apr, "APR"), (ap.modes.alt, "ALT"), (ap.modes.vs, "V/S"), (ap.modes.flc, "FLC")] {
            if on {
                modes.push(n);
            }
        }
        if ap.athr {
            modes.push("A/THR");
        }
        live.ap_line = ss(modes.join("  "));
        live.ap_master = ap.master;
        live.ap_alt = ss(format!("{:.0}", ap.alt_ft));
        live.ap_hdg = ss(format!("{:03.0}°", ap.hdg_deg));
        live.ap_spd = ss(if av.ap.mach_hold { format!("M{:.2}", av.ap.mach_var) } else { format!("{:.0} kt", ap.spd_kt) });
        live.ap_vs = ss(format!("{:+.0} ft/min", ap.vs_fpm));
        let r = &s.radios;
        live.com1 = ss(format!("{:.3}  ⇄  {:.3}", r.com1.active_mhz, r.com1.standby_mhz));
        live.com2 = ss(format!("{:.3}  ⇄  {:.3}", r.com2.active_mhz, r.com2.standby_mhz));
        live.nav1 = ss(format!("{:.2}  ⇄  {:.2}{}", r.nav1.active_mhz, r.nav1.standby_mhz, if av.nav1.ident.is_empty() { String::new() } else { format!("  {}", av.nav1.ident) }));
        live.nav2 = ss(format!("{:.2}  ⇄  {:.2}", r.nav2.active_mhz, r.nav2.standby_mhz));
        live.xpdr = ss(format!("{}  {}", r.transponder.code, r.transponder.mode.to_uppercase()));
        let c = &s.controls;
        live.gear = ss(if c.gear_pct >= 99.0 { tr("wypuszczone").to_string() } else if c.gear_pct <= 1.0 { tr("schowane").to_string() } else { format!("{:.0} %", c.gear_pct) });
        live.flaps = ss(format!("{} / {} ({:.0} %)", c.flaps_index, (c.flaps_positions - 1).max(0), c.flaps_pct));
        live.spoilers = ss(if c.spoilers_armed { tr("uzbrojone").to_string() } else { format!("{:.0} %", c.spoilers_pct) });
        live.brake = c.parking_brake;
        live.gear_short = ss(if c.gear_pct >= 99.0 { "DN".to_string() } else if c.gear_pct <= 1.0 { "UP".to_string() } else { format!("{:.0}%", c.gear_pct) });
        live.gear_on = c.gear_pct > 1.0;
        live.flaps_short = ss(if c.flaps_index <= 0 { "UP".to_string() } else { c.flaps_index.to_string() });
        live.spoilers_short = ss(if c.spoilers_armed { "ARM".to_string() } else if c.spoilers_pct > 1.0 { format!("{:.0}%", c.spoilers_pct) } else { "RET".to_string() });
        live.spoilers_on = c.spoilers_armed || c.spoilers_pct > 1.0;
        let l = &s.lights;
        let lights: Vec<&str> = [(l.beacon, "BCN"), (l.strobe, "STRB"), (l.nav, "NAV"), (l.landing, "LDG"), (l.taxi, "TAXI"), (l.logo, "LOGO")].into_iter().filter(|x| x.0).map(|x| x.1).collect();
        live.lights = ss(if lights.is_empty() { tr("wyłączone").to_string() } else { lights.join(" ") });
        let g = &av.gps;
        live.next_wp = ss(if g.next_id.is_empty() { tr("brak planu lotu w symulatorze").to_string() } else { format!("{} ({:03.0}°)", g.next_id, g.next_bearing_mag_deg) });
        live.next_dist = ss(if g.next_id.is_empty() { String::new() } else { u.dist(g.next_dist_nm) });
        live.next_ete = ss(fmt::ete(g.next_ete_sec));
        live.dest_ete = ss(fmt::ete(g.dest_ete_sec));
        live.wp_progress = if g.wp_count > 1 { (g.wp_index as f32 / (g.wp_count - 1) as f32).clamp(0.0, 1.0) } else { 0.0 };
        live.sim_time = ss(s.sim_time_utc.clone().unwrap_or_default().replace('T', " ").replace('Z', " UTC"));
        live.touchdown = ss(last.map(|l| format!("{} · {} · {} ft/min{}\n{}", l.score, landing::grade_of(l.score), l.vs_fpm, l.runway.as_ref().map(|r| format!(" · RWY {r}")).unwrap_or_default(), landing::notes_now(&l).join(" "))).unwrap_or_default());
        // ---- Karta Na żywo ----
        live.phase_index = phase_index(s, rec_since.is_some());
        live.utc = ss(utc_clock(s.sim_time_utc.as_deref()));
        live.flight_time = ss(rec_since.map(|t| fmt::duration_min((unix_ms() - t) / 60_000)).unwrap_or_default());
        live.eta = ss(if g.dest_ete_sec > 0.0 { utc_plus(s.sim_time_utc.as_deref(), g.dest_ete_sec) } else { String::new() });
        // FMA
        let m = &ap.modes;
        live.fma_thr = ss(if ap.athr { if av.ap.mach_hold { "MACH" } else { "SPEED" } } else { "" });
        live.fma_vert = ss(if m.apr && av.nav1.has_gs { "G/S" } else if m.flc { if ap.alt_ft > s.position.alt_ind_ft { "OP CLB" } else { "OP DES" } } else if m.vs { "V/S" } else if m.alt { "ALT" } else { "" });
        live.fma_lat = ss(if m.apr || m.loc { "LOC" } else if m.nav { if g.active_flight_plan { "NAV" } else { "VOR" } } else if m.hdg { "HDG" } else { "" });
        live.ap_fd = ap.fd;
        live.ap_athr = ap.athr;
        // Radia
        live.com1_a = ss(format!("{:.3}", r.com1.active_mhz));
        live.com1_s = ss(format!("{:.3}", r.com1.standby_mhz));
        live.com2_a = ss(format!("{:.3}", r.com2.active_mhz));
        live.com2_s = ss(format!("{:.3}", r.com2.standby_mhz));
        live.nav1_a = ss(format!("{:.2}", r.nav1.active_mhz));
        live.nav1_s = ss(format!("{:.2}", r.nav1.standby_mhz));
        live.nav1_id = ss(&av.nav1.ident);
        live.nav1_dme = ss(if av.nav1.has_dme && av.nav1.dme_nm > 0.0 { format!("{:.1} NM", av.nav1.dme_nm) } else { String::new() });
        live.nav2_a = ss(format!("{:.2}", r.nav2.active_mhz));
        live.nav2_s = ss(format!("{:.2}", r.nav2.standby_mhz));
        live.xpdr_code = ss(&r.transponder.code);
        live.xpdr_mode = ss(r.transponder.mode.to_uppercase());
        // Wiatr względem dziobu: składowa czołowa (+) / tylna (−) i boczna
        let rel_deg = (s.environment.wind_dir_deg - s.attitude.hdg_true_deg).rem_euclid(360.0);
        let ws = s.environment.wind_spd_kt;
        let (head, cross) = (ws * rel_deg.to_radians().cos(), ws * rel_deg.to_radians().sin());
        live.wind_rel = rel_deg as f32;
        live.wind_spd = ss(format!("{:.0}", ws));
        live.wind_head = ss(if ws < 1.0 { String::new() } else if head >= 0.0 { trf!("czołowy {kt} kt", kt = head.round()) } else { trf!("tylny {kt} kt", kt = (-head).round()) });
        live.wind_cross = ss(if cross.abs() < 1.0 { String::new() } else if cross > 0.0 { trf!("z prawej {kt} kt", kt = cross.round()) } else { trf!("z lewej {kt} kt", kt = (-cross).round()) });
        // Paliwo: przepływ, zasięg w czasie, zapas na celu
        let ff: f64 = s.engines.iter().map(|e| e.fuel_flow_kg_h).sum();
        live.fuel_flow = ss(if ff > 1.0 { format!("{:.0} kg/h", ff) } else { String::new() });
        live.fuel_endurance = ss(if ff > 1.0 { fmt::ete(s.fuel.total_kg / ff * 3600.0) } else { String::new() });
        let at_dest = if ff > 1.0 && g.dest_ete_sec > 0.0 { Some(s.fuel.total_kg - ff * g.dest_ete_sec / 3600.0) } else { None };
        live.fuel_dest = ss(at_dest.map(|f| format!("{:.0} kg", f.max(0.0))).unwrap_or_default());
        live.fuel_dest_low = at_dest.is_some_and(|f| f < ff * 0.75);
        let t = &s.fuel.tanks;
        let tmax = t.left_kg.max(t.right_kg).max(t.center_kg).max(1.0);
        live.tank_l = ss(format!("{:.0}", t.left_kg));
        live.tank_c = ss(format!("{:.0}", t.center_kg));
        live.tank_r = ss(format!("{:.0}", t.right_kg));
        live.tank_l_level = (t.left_kg / tmax) as f32;
        live.tank_c_level = (t.center_kg / tmax) as f32;
        live.tank_r_level = (t.right_kg / tmax) as f32;
        live.has_center = t.center_kg > 0.5;
        // Parametry lotu
        live.g_force = ss(if av.g_force != 0.0 { format!("{:.2}", av.g_force) } else { String::new() });
        live.aoa = ss(if av.aoa_deg != 0.0 { format!("{:.1}°", av.aoa_deg) } else { String::new() });
        live.tat = ss(if av.tat_c != 0.0 { format!("{:.0} °C", av.tat_c) } else { String::new() });
        live.trim = ss(format!("{:+.0} %", c.elevator_trim_pct));
        live.stall = av.stall_warning;
        live.overspeed = av.overspeed_warning;
        live.has_route = !g.next_id.is_empty();
        if tab == 1 {
            let (line, area, top) = alt_profile(&st.host.live_track());
            live.alt_line = ss(line);
            live.alt_area = ss(area);
            live.alt_top = ss(if top > 0 { u.alt_full(top as f64) } else { String::new() });
        }
        // Lampki systemów i świateł
        let gear_tone = if c.gear_pct >= 99.0 { 0 } else { 1 };
        let marker = av.marker.to_uppercase();
        let lamps = vec![
            lamp("GEAR", &live.gear_short, c.gear_pct > 1.0, gear_tone),
            lamp("FLAPS", &live.flaps_short, c.flaps_index > 0, 3),
            lamp("SPLR", &live.spoilers_short, live.spoilers_on, if c.spoilers_pct > 1.0 { 1 } else { 0 }),
            lamp("BRAKE", if c.parking_brake { "SET" } else { "OFF" }, c.parking_brake, 1),
            lamp("STALL", if av.stall_warning { "WARN" } else { "" }, av.stall_warning, 2),
            lamp("OVSPD", if av.overspeed_warning { "WARN" } else { "" }, av.overspeed_warning, 2),
            lamp("MKR", &marker, !marker.is_empty() && marker != "NONE", 3),
            lamp("SIM", if s.aircraft.paused { "PAUSE" } else { "RUN" }, s.aircraft.paused, 1),
        ];
        app.set_live_lamps(model(lamps));
        let lt = &s.lights;
        let lights: Vec<LampV> = [(lt.beacon, "BCN"), (lt.strobe, "STRB"), (lt.nav, "NAV"), (lt.landing, "LDG"), (lt.taxi, "TAXI"), (lt.logo, "LOGO"), (lt.wing, "WING")]
            .into_iter()
            .map(|(on, k)| lamp(k, "", on, 0))
            .collect();
        app.set_live_lights(model(lights));
        let engines: Vec<EngineV> = s
            .engines
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let turbine = e.n1_pct > 0.5;
                let frac = if turbine { e.n1_pct / 100.0 } else { e.rpm / 2700.0 };
                let thr = (e.throttle_pct / 100.0).clamp(0.0, 1.0);
                EngineV {
                    label: ss(trf!("Silnik {n}", n = i + 1)),
                    main: ss(if turbine { format!("N1 {:.1} %", e.n1_pct) } else { format!("{:.0} RPM", e.rpm) }),
                    sub: ss(format!("EGT {:.0} °C · {:.0} kg/h · {} {:.0} %", e.egt_c, e.fuel_flow_kg_h, tr("dźwignia"), e.throttle_pct)),
                    level: frac.clamp(0.0, 1.0) as f32,
                    kind: ss(if turbine { "N1" } else { "RPM" }),
                    value: ss(if turbine { format!("{:.1}", e.n1_pct) } else { format!("{:.0}", e.rpm) }),
                    arc: ss(gauge_arc(frac)),
                    needle: ss(gauge_needle(frac)),
                    thr_tick: ss(gauge_tick(thr)),
                    egt: ss(format!("{:.0}", e.egt_c)),
                    egt_level: (e.egt_c / if turbine { 950.0 } else { 900.0 }).clamp(0.0, 1.0) as f32,
                    ff: ss(format!("{:.0}", e.fuel_flow_kg_h)),
                    n2: ss(if turbine && e.n2_pct > 0.5 { format!("{:.1}", e.n2_pct) } else if e.prop_rpm > 1.0 { format!("{:.0}", e.prop_rpm) } else { String::new() }),
                    warn: frac > 1.0,
                }
            })
            .collect();
        app.set_engines(model(engines));
    }
    app.set_live(live);
    if tab == 1 {
        if let Some(img) = crate::instruments::live_pfd(snap.as_ref(), st.scale, &st.s.instr_style) {
            app.set_pfd(img);
        }
    }
    // Mapa: co 0,5 s albo od razu po zmianie widoku
    if with_map && (force_map || st.live_render.elapsed().as_millis() > 500) {
        st.live_render = Instant::now();
        let dark = ui.global::<Theme>().get_dark();
        let track = st.host.live_track();
        let mut ov = Overlay::default();
        if track.len() > 1 {
            ov.tracks.push(Track { pts: track.iter().map(|p| (p.lat, p.lon, p.alt)).collect(), color: None, width: 3.0, wall: false });
        }
        if let Some(s) = &snap {
            ov.aircraft = Some((s.position.lat, s.position.lon, s.attitude.hdg_true_deg));
            if let Some(av) = &s.avionics {
                if av.gps.next_lat != 0.0 && av.gps.prev_lat != 0.0 {
                    ov.route = vec![(av.gps.prev_lat, av.gps.prev_lon), (av.gps.next_lat, av.gps.next_lon)];
                }
            }
            if st.views[0].follow {
                st.views[0].lat = s.position.lat;
                st.views[0].lon = s.position.lon;
                if !st.views[0].user_moved && st.views[0].zoom < 8.0 {
                    st.views[0].zoom = 9.0;
                }
            }
            if let Some(db) = airports::db() {
                let v = &st.views[0];
                if v.zoom > 7.0 {
                    let radius = (v.w.max(v.h) * 156_543.0 * v.lat.to_radians().cos() / 2f64.powf(v.zoom) / 1852.0).min(200.0);
                    for (_, a) in db.within(v.lat, v.lon, radius, false).into_iter().filter(|(_, a)| a.kind <= 1 || v.zoom > 9.5).take(80) {
                        ov.points.push(Point { lat: a.lat, lon: a.lon, label: a.ident.clone(), kind: if a.kind <= 1 { 0 } else { 3 }, alt: None });
                        if v.zoom > 11.0 {
                            for r in a.runways.iter().filter(|r| r.has_coords() && !r.closed) {
                                ov.runways.push((r.le_lat, r.le_lon, r.he_lat, r.he_lon));
                            }
                        }
                    }
                }
            }
        }
        app.set_map_attribution(style(&st, dark).attribution().into());
        let r = map::render(&st.views[0], style(&st, dark), &ov, st.scale, dark);
        app.set_map_live(r.image);
        app.set_map_live_labels(labels(r.labels));
        app.set_map_live_scale(ss(st.views[0].scale_text()));
    }
}
