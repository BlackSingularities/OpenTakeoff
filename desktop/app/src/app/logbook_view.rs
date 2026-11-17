//! Karta Dziennik: lista lotów, szczegóły lotu, analiza lądowania, mapa i odtwarzanie.
//! Część modułu `app` – wspólne pomocnicze funkcje i importy z `super`.

use super::*;

// ---------- Dziennik ----------

pub(super) fn matches(f: &FlightLog, meta: &opentakeoff_core::logbook::FlightMeta, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    let hay = airports::fold(&format!(
        "{} {} {} {} {} {} {}",
        f.from.as_deref().unwrap_or(""),
        f.to.as_deref().unwrap_or(""),
        f.aircraft.as_deref().unwrap_or(""),
        meta.tags.join(" "),
        meta.notes,
        fmt::date(f.start),
        meta.source
    ));
    airports::fold(q).split_whitespace().all(|w| hay.contains(w))
}

pub fn refresh_logbook() {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let mut st = st.borrow_mut();
    let u = units(&st.s);
    let store = st.host.logbook.clone();
    st.logbook_rev = store.revision();
    let trash = app.get_show_trash();
    let q = app.get_filter_text().to_string();
    let period = app.get_filter_period();
    let sort = app.get_sort_by();
    let metas = store.all_meta();
    let now = Local::now();
    let mut flights: Vec<FlightLog> = (if trash { store.trash() } else { store.flights() })
        .iter()
        .map(|f| store.effective(f))
        .filter(|f| {
            let m = metas.get(&f.id).cloned().unwrap_or_default();
            let in_period = match period {
                1 => unix_ms() - f.start < 30 * 86_400_000,
                2 => Local.timestamp_millis_opt(f.start).single().is_some_and(|d| d.year() == now.year()),
                3 => m.favorite,
                _ => true,
            };
            in_period && matches(f, &m, &q)
        })
        .collect();
    match sort {
        1 => flights.sort_by(|a, b| (b.end - b.start).cmp(&(a.end - a.start))),
        2 => flights.sort_by(|a, b| b.last_landing().map(|l| l.score).unwrap_or(-1).cmp(&a.last_landing().map(|l| l.score).unwrap_or(-1))),
        _ => flights.sort_by(|a, b| b.start.cmp(&a.start)),
    }
    let mut rows = Vec::new();
    let mut last_month = String::new();
    st.list = flights.iter().map(|f| f.id.clone()).collect();
    let selected = st.selected.clone();
    for f in &flights {
        if sort == 0 {
            if let Some(d) = Local.timestamp_millis_opt(f.start).single() {
                let key = format!("{} {}", fmt::month_name(d.month()), d.year()).to_uppercase();
                if key != last_month {
                    last_month = key.clone();
                    rows.push(FlightV { header: true, month: ss(key), score: -1, ..Default::default() });
                }
            }
        }
        let m = metas.get(&f.id).cloned().unwrap_or_default();
        let l = f.last_landing();
        rows.push(FlightV {
            id: ss(&f.id),
            date: ss(fmt::date(f.start)),
            month: ss(""),
            time: ss(fmt::time(f.start)),
            from: ss(f.from.clone().unwrap_or_else(|| "?".into())),
            to: ss(f.to.clone().unwrap_or_else(|| "?".into())),
            aircraft: ss(f.aircraft.clone().unwrap_or_else(|| tr("nieznany samolot").into())),
            category: ss(f.category.clone().unwrap_or_default()),
            duration: ss(fmt::duration_min(f.duration_min())),
            distance: ss(u.dist(f.distance_nm as f64)),
            max_alt: ss(u.alt_full(f.max_alt_ft as f64)),
            score: l.as_ref().map(|l| l.score).unwrap_or(-1),
            grade: ss(l.map(|l| landing::grade_of(l.score).to_string()).unwrap_or_default()),
            landings: f.all_landings().len() as i32,
            source: ss(if m.source != "pc" && !m.source.is_empty() && m.source != "import" { m.source.clone() } else { String::new() }),
            favorite: m.favorite,
            tags: ss(m.tags.join(", ")),
            notes: !m.notes.trim().is_empty(),
            selected: selected.contains(&f.id),
            header: false,
        });
    }
    let total_min: i64 = flights.iter().map(|f| f.duration_min()).sum();
    app.set_flights(model(rows));
    app.set_flights_summary(ss(format!("{} · {}", fmt::flights(flights.len()), fmt::hours(total_min))));
    app.set_trash_count(store.trash().len() as i32);
    app.set_selected_count(st.selected.len() as i32);
}

pub(super) fn flight_title(f: &FlightLog) -> String {
    format!("{}  →  {}", f.from.as_deref().unwrap_or("?"), f.to.as_deref().unwrap_or("?"))
}

pub fn open_flight(id: &str) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let mut st = st.borrow_mut();
    let store = st.host.logbook.clone();
    let Some(raw) = store.get(id) else {
        app.set_has_detail(false);
        return;
    };
    let in_trash = store.trash().iter().any(|f| f.id == id);
    let f = Arc::new(store.effective(&raw));
    let meta = store.meta(id);
    let u = units(&st.s);
    st.detail = Some(f.clone());
    st.replay = 0.0;
    st.playing = false;
    st.anchor = Some(id.to_string());
    let t = &f.track;
    let gs: Vec<f64> = t.iter().filter(|p| !p.on_ground.unwrap_or(false) && p.gs_kt > 30).map(|p| p.gs_kt as f64).collect();
    let avg = if gs.is_empty() { 0.0 } else { gs.iter().sum::<f64>() / gs.len() as f64 };
    let maxgs = gs.iter().cloned().fold(0.0, f64::max);
    let maxvs = t.iter().filter_map(|p| p.vs_fpm).max().unwrap_or(0);
    let minvs = t.iter().filter_map(|p| p.vs_fpm).min().unwrap_or(0);
    let l = f.last_landing();
    let source = match meta.source.as_str() {
        "pc" => tr("Nagrany na tym komputerze").to_string(),
        "import" | "" => tr("Zaimportowany").to_string(),
        s => trf!("Z telefonu: {s}"),
    };
    let detail = DetailV {
        id: ss(id),
        title: ss(flight_title(&f)),
        route: ss(flight_title(&f)),
        subtitle: ss(format!(
            "{} · {} – {} · {}",
            fmt::date(f.start),
            fmt::time(f.start),
            fmt::time(f.end),
            f.aircraft.clone().unwrap_or_else(|| tr("nieznany samolot").into())
        )),
        aircraft: ss(f.aircraft.clone().unwrap_or_default()),
        category: ss(f.category.clone().unwrap_or_default()),
        from: ss(f.from.clone().unwrap_or_default()),
        to: ss(f.to.clone().unwrap_or_default()),
        duration: ss(fmt::duration_min(f.duration_min())),
        distance: ss(u.dist(f.distance_nm as f64)),
        max_alt: ss(u.alt_full(f.max_alt_ft as f64)),
        avg_gs: ss(u.spd(avg)),
        max_gs: ss(u.spd(maxgs)),
        max_vs: ss(format!("+{maxvs} ft/min")),
        min_vs: ss(format!("{minvs} ft/min")),
        points: ss(t.len().to_string()),
        source: ss(format!("{source} · {} {}", t.len(), tr("punktów ścieżki"))),
        notes: ss(&meta.notes),
        tags: ss(meta.tags.join(", ")),
        favorite: meta.favorite,
        in_trash,
        score: l.as_ref().map(|l| l.score).unwrap_or(-1),
        grade: ss(l.map(|l| landing::grade_of(l.score).to_string()).unwrap_or_default()),
        cursor_info: ss(""),
        cursor_pos: 0.0,
    };
    app.set_detail(detail);
    let landings: Vec<LandingV> = f
        .all_landings()
        .iter()
        .enumerate()
        .map(|(i, l)| LandingV {
            title: ss(format!("{} {}{}", tr("Lądowanie"), i + 1, l.airport.as_ref().map(|a| format!(" · {a}")).unwrap_or_default())),
            score: l.score,
            grade: ss(landing::grade_of(l.score)),
            vs: ss(format!("{} ft/min", l.vs_fpm)),
            g: ss(l.g_force.map(|g| format!("{g:.2} g")).unwrap_or_default()),
            runway: ss(l.runway.as_ref().map(|r| format!("RWY {r}")).unwrap_or_default()),
            along: ss(l.along_ft.map(|a| trf!("{a} ft za progiem")).unwrap_or_default()),
            notes: ss(landing::notes_now(l).join(" ")),
            at: ss(l.at.map(fmt::time).unwrap_or_default()),
            has_profile: l.profile.len() >= 10,
        })
        .collect();
    app.set_detail_landings(model(landings));
    // Analiza: ostatnie lądowanie z zapisanym przebiegiem
    let all = f.all_landings();
    let pick = all.iter().rposition(|l| l.profile.len() >= 10).or(if all.is_empty() { None } else { Some(all.len() - 1) });
    show_analysis(&app, &f, pick);
    let span = (f.end - f.start).max(1) as f32;
    let markers: Vec<MarkerV> =
        f.markers.iter().map(|m| MarkerV { t: (m.t / 1000) as i32, time: ss(fmt::clock(m.t)), label: ss(&m.label), pos: ((m.t - t.first().map(|p| p.t).unwrap_or(f.start)) as f32 / span).clamp(0.0, 1.0) }).collect();
    app.set_detail_markers(model(markers));
    let shots: Vec<ShotV> = crate::shots::for_flight(&f)
        .iter()
        .map(|s| ShotV {
            id: ss(&s.id),
            thumb: crate::shots::thumb_image(s),
            time: ss(fmt::clock(s.time)),
            date: ss(fmt::date(s.time)),
            place: ss(&s.nearest),
            alt: ss(if s.has_pos { u.alt_full(s.alt_ft) } else { String::new() }),
            has_pos: s.has_pos,
            ..Default::default()
        })
        .collect();
    app.set_detail_shots(model(shots));
    set_chart(&app, &f, &st.s);
    app.set_has_detail(true);
    app.set_replay_playing(false);
    // Mapa dopasowana do toru
    if let Some(b) = map::bounds(t.iter().map(|p| (p.lat, p.lon))) {
        st.views[1].fit(b);
    }
    drop(st);
    render_flight_map();
    update_cursor();
}

pub(super) fn show_analysis(app: &App, f: &FlightLog, idx: Option<usize>) {
    let all = f.all_landings();
    match idx.and_then(|i| all.get(i).map(|l| (i, l))) {
        Some((i, l)) => {
            let title = format!("{} {}{}", tr("Lądowanie"), i + 1, l.at.map(|t| format!(" · {}", fmt::time(t))).unwrap_or_default());
            app.set_analysis(crate::analysis::build(l, title));
            app.set_analysis_index(i as i32);
        }
        None => {
            app.set_analysis(AnalysisV::default());
            app.set_analysis_index(-1);
        }
    }
}

/// Analiza wybranego lądowania bieżącego lotu.
pub fn select_landing(i: i32) {
    let Some(ui) = ui() else { return };
    let f = state().borrow().detail.clone();
    if let Some(f) = f {
        show_analysis(&ui.global::<App>(), &f, usize::try_from(i).ok());
    }
}

pub(super) fn set_chart(app: &App, f: &FlightLog, s: &Settings) {
    let c = crate::chart::build(f, s.units_alt == 1, s.units_speed == 1);
    let lv = |v: Vec<crate::chart::Label>| model(v.into_iter().map(|l| LabelV { x: l.x, y: l.y, text: ss(l.text), kind: l.kind }).collect());
    let y2 = if app.get_chart_mode() == 2 { crate::chart::vs_labels(f) } else { c.y2_labels };
    let y1 = if app.get_chart_mode() == 0 { c.y_labels } else { Vec::new() };
    app.set_chart(ChartV {
        alt: ss(c.alt),
        spd: ss(c.spd),
        gs: ss(c.gs),
        vs: ss(c.vs),
        ground: ss(c.ground),
        grid: ss(c.grid),
        y_labels: lv(y1),
        x_labels: lv(c.x_labels),
        y2_labels: lv(y2),
        markers: lv(c.markers),
        has_ias: c.has_ias,
        has_vs: c.has_vs,
    });
}

pub fn render_flight_map() {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let st = st.borrow();
    let Some(f) = st.detail.clone() else { return };
    let dark = ui.global::<Theme>().get_dark();
    let mut ov = Overlay::default();
    ov.tracks.push(Track { pts: f.track.iter().map(|p| (p.lat, p.lon, p.alt_ft)).collect(), color: None, width: 3.0 });
    let db = airports::db();
    for code in [f.from.as_deref(), f.to.as_deref()].into_iter().flatten() {
        if let Some(a) = db.as_ref().and_then(|d| d.get(code)) {
            ov.points.push(Point { lat: a.lat, lon: a.lon, label: a.ident.clone(), kind: 0 });
            for r in a.runways.iter().filter(|r| r.has_coords() && !r.closed) {
                ov.runways.push((r.le_lat, r.le_lon, r.he_lat, r.he_lon));
            }
        }
    }
    for (i, l) in f.all_landings().iter().enumerate() {
        if let Some(p) = l.at.and_then(|at| f.track.iter().min_by_key(|p| (p.t - at).abs())) {
            ov.points.push(Point { lat: p.lat, lon: p.lon, label: format!("{} {} · {}", tr("Lądowanie"), i + 1, l.score), kind: 2 });
        }
    }
    for m in &f.markers {
        if let Some(p) = f.track.iter().min_by_key(|p| (p.t - m.t).abs()) {
            ov.points.push(Point { lat: p.lat, lon: p.lon, label: m.label.clone(), kind: 2 });
        }
    }
    for s in crate::shots::for_flight(&f).iter().filter(|s| s.has_pos) {
        ov.points.push(Point { lat: s.lat, lon: s.lon, label: format!("{} {}", tr("Zrzut"), fmt::time(s.time)), kind: 4 });
    }
    if let Some(p) = replay_point(&f, st.replay) {
        ov.aircraft = Some((p.lat, p.lon, p.hdg.unwrap_or(0) as f64));
    }
    if st.replay_3d {
        let img = render_flight_3d(&f, &st, dark);
        app.set_map_flight(img);
        app.set_map_flight_labels(model(Vec::new()));
        return;
    }
    let r = map::render(&st.views[1], style(&st, dark), &ov, st.scale, dark);
    app.set_map_flight(r.image);
    app.set_map_flight_labels(labels(r.labels));
}

/// Odtwarzanie lotu w 3D: kamera za samolotem w interpolowanej pozycji, cały ślad i pasy lotnisk.
pub(super) fn render_flight_3d(f: &FlightLog, st: &State, dark: bool) -> slint::Image {
    use crate::scene3d::{self, Label3};
    let v = &st.views[1];
    let w = ((v.w * st.scale as f64) as u32).max(16);
    let h = ((v.h * st.scale as f64) as u32).max(16);
    let Some((p, alt)) = replay_interp(f, st.replay) else {
        return slint::Image::default();
    };
    // Teren pod samolotem: z AGL, a bez niego najniższy punkt na ziemi (albo całego śladu)
    let ground = match p.agl_ft {
        Some(agl) => alt - agl as f64,
        None => f.track.iter().filter(|t| t.on_ground == Some(true)).map(|t| t.alt_ft).min().or_else(|| f.track.iter().map(|t| t.alt_ft).min()).unwrap_or(0) as f64,
    };
    let mut runways = Vec::new();
    let mut points = Vec::new();
    let db = airports::db();
    for code in [f.from.as_deref(), f.to.as_deref()].into_iter().flatten() {
        if let Some(a) = db.as_ref().and_then(|d| d.get(code)) {
            points.push(Label3 { lat: a.lat, lon: a.lon, text: a.ident.clone(), kind: 0 });
            for r in a.runways.iter().filter(|r| r.has_coords() && !r.closed) {
                runways.push((r.le_lat, r.le_lon, r.he_lat, r.he_lon, r.width_ft.max(60) as f64));
            }
        }
    }
    let track: Vec<(f64, f64, i32)> = f.track.iter().map(|t| (t.lat, t.lon, t.alt_ft)).collect();
    let u = units(&st.s);
    let el = (p.t - f.track[0].t).max(0) / 1000;
    let hud = vec![
        format!("{} · ×{}", f.aircraft.clone().filter(|a| !a.is_empty()).unwrap_or_else(|| "OpenTakeoff".into()), st.replay_speed as i64),
        format!("{}:{:02}:{:02} · ALT {} · GS {}", el / 3600, el % 3600 / 60, el % 60, u.alt_full(alt), u.spd(p.gs_kt as f64)),
        format!("HDG {:03}° · V/S {:+} ft/min", p.hdg.unwrap_or(0).rem_euclid(360), p.vs_fpm.unwrap_or(0)),
    ];
    let sc = scene3d::Scene {
        style: style(st, dark),
        dark,
        w,
        h,
        scale: st.scale,
        lat: p.lat,
        lon: p.lon,
        alt_ft: alt,
        hdg: p.hdg.unwrap_or(0) as f64,
        pitch: p.pitch_deg.unwrap_or(0.0) as f64,
        bank: p.bank_deg.unwrap_or(0.0) as f64,
        ground_ft: ground,
        mag_var: 0.0,
        show_aircraft: true,
        track: &track,
        runways: &runways,
        points: &points,
        route: &[],
        plan: &[],
        hud,
        compass: false,
        curtain: true,
        lines: &[],
        sun: None,
    };
    scene3d::render(&sc, &st.replay_cam)
}

pub(super) fn replay_point(f: &FlightLog, pos: f64) -> Option<opentakeoff_core::logbook::TrackPoint> {
    replay_interp(f, pos).map(|(p, _)| p)
}

/// Punkt śladu w chwili `pos` (0–1) interpolowany między próbkami – płynny ruch przy odtwarzaniu.
/// Zwraca też wysokość bez zaokrąglenia do pełnych stóp.
pub(super) fn replay_interp(f: &FlightLog, pos: f64) -> Option<(opentakeoff_core::logbook::TrackPoint, f64)> {
    let t = &f.track;
    if t.len() < 2 {
        return None;
    }
    let (t0, t1) = (t[0].t, t[t.len() - 1].t);
    let target = t0 + ((t1 - t0) as f64 * pos.clamp(0.0, 1.0)) as i64;
    let i = t.partition_point(|p| p.t < target);
    if i == 0 {
        return Some((t[0].clone(), t[0].alt_ft as f64));
    }
    if i >= t.len() {
        let l = t[t.len() - 1].clone();
        let a = l.alt_ft as f64;
        return Some((l, a));
    }
    let (a, b) = (&t[i - 1], &t[i]);
    let k = ((target - a.t) as f64 / (b.t - a.t).max(1) as f64).clamp(0.0, 1.0);
    let lerp = |x: f64, y: f64| x + (y - x) * k;
    let lerp_o = |x: Option<f64>, y: Option<f64>| match (x, y) {
        (Some(x), Some(y)) => Some(lerp(x, y)),
        (x, y) => y.or(x),
    };
    let mut p = b.clone();
    p.t = target;
    p.lat = lerp(a.lat, b.lat);
    p.lon = lerp(a.lon, b.lon);
    let alt = lerp(a.alt_ft as f64, b.alt_ft as f64);
    p.alt_ft = alt.round() as i32;
    p.gs_kt = lerp(a.gs_kt as f64, b.gs_kt as f64).round() as i32;
    p.ias_kt = lerp_o(a.ias_kt.map(f64::from), b.ias_kt.map(f64::from)).map(|v| v.round() as i32);
    p.vs_fpm = lerp_o(a.vs_fpm.map(f64::from), b.vs_fpm.map(f64::from)).map(|v| v.round() as i32);
    p.agl_ft = lerp_o(a.agl_ft.map(f64::from), b.agl_ft.map(f64::from)).map(|v| v.round() as i32);
    p.bank_deg = lerp_o(a.bank_deg.map(f64::from), b.bank_deg.map(f64::from)).map(|v| v as f32);
    p.pitch_deg = lerp_o(a.pitch_deg.map(f64::from), b.pitch_deg.map(f64::from)).map(|v| v as f32);
    // kurs po krótszym łuku (359° → 1°)
    p.hdg = match (a.hdg, b.hdg) {
        (Some(x), Some(y)) => {
            let d = ((y - x) as f64 + 540.0).rem_euclid(360.0) - 180.0;
            Some(((x as f64 + d * k).round() as i32).rem_euclid(360))
        }
        (x, y) => y.or(x),
    };
    Some((p, alt))
}

pub fn update_cursor() {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let st = st.borrow();
    let Some(f) = st.detail.clone() else { return };
    let u = units(&st.s);
    let mut d = app.get_detail();
    d.cursor_pos = st.replay as f32;
    if let Some(p) = replay_point(&f, st.replay) {
        let el = (p.t - f.track[0].t) / 1000;
        d.cursor_info = ss(format!(
            "{}:{:02}:{:02}  {}  {}  {} ft/min  {}",
            el / 3600,
            el % 3600 / 60,
            el % 60,
            u.alt_full(p.alt_ft as f64),
            u.spd(p.ias_kt.unwrap_or(p.gs_kt) as f64),
            p.vs_fpm.unwrap_or(0),
            fmt::clock(p.t)
        ));
    }
    app.set_detail(d);
}
