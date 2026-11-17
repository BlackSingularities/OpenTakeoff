//! Karta Lotniska: wyszukiwanie, briefing z pogodą, pasy, procedury i obserwowane lotniska.
//! Część modułu `app` – wspólne pomocnicze funkcje i importy z `super`.

use super::*;

// ---------- Lotniska ----------

pub(super) fn apt_v(a: &Airport, extra: String) -> AptV {
    let place = [a.city.as_str(), a.country.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(", ");
    AptV { ident: ss(&a.ident), name: ss(&a.name), place: ss(place), kind: ss(a.kind_name()), extra: ss(extra), cat: ss("") }
}

pub fn search_airports(q: &str) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let Some(db) = airports::db() else {
        app.set_airport_results(model(Vec::new()));
        return;
    };
    let res: Vec<AptV> = db.search(q, 60).into_iter().map(|a| apt_v(a, String::new())).collect();
    app.set_airport_results(model(res));
}

pub fn open_airport(ident: &str, fetch: bool) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let Some(db) = airports::db() else {
        toast(tr("Baza lotnisk jeszcze się wczytuje"));
        return;
    };
    let Some(a) = db.get(ident).cloned() else {
        toast(trf!("Nie znaleziono lotniska {ident}"));
        return;
    };
    let st = state();
    let mut st = st.borrow_mut();
    let u = units(&st.s);
    st.airport = Some(a.ident.clone());
    st.s.last_airport = a.ident.clone();
    if st.procs.as_ref().is_some_and(|(i, _)| i != &a.ident) {
        st.procs = None;
        st.legs.clear();
        app.set_procedures(model(Vec::new()));
        app.set_procedure_legs(model(Vec::new()));
        app.set_procedure_title(ss(""));
        app.set_procedures_status(ss(""));
    }
    app.set_has_airport(true);
    app.set_airport(apt_v(&a, String::new()));
    app.set_airport_watched(st.s.watchlist.iter().any(|w| w == &a.ident));
    let mut info = vec![
        KV { k: ss(tr("Kod ICAO / IATA")), v: ss(format!("{}{}", if a.icao.is_empty() { &a.ident } else { &a.icao }, if a.iata.is_empty() { String::new() } else { format!(" / {}", a.iata) })) },
        KV { k: ss(tr("Elewacja")), v: ss(a.elev_ft.map(|e| u.alt_full(e as f64)).unwrap_or_default()) },
        KV { k: ss(tr("Położenie")), v: ss(fmt::latlon(a.lat, a.lon)) },
        KV { k: ss(tr("Region")), v: ss(&a.region) },
        KV { k: ss(tr("Ruch rozkładowy")), v: ss(if a.scheduled { tr("tak") } else { tr("nie") }) },
        KV { k: ss(tr("Najdłuższy pas")), v: ss(format!("{} ft", a.longest_runway_ft())) },
    ];
    if let Some(snap) = st.host.snapshot() {
        let d = math::distance_nm(snap.position.lat, snap.position.lon, a.lat, a.lon);
        let b = math::bearing_deg(snap.position.lat, snap.position.lon, a.lat, a.lon);
        info.push(KV { k: ss(tr("Od samolotu")), v: ss(format!("{} · {:03.0}°", u.dist(d), b)) });
    }
    let visits = st.host.logbook.flights().iter().filter(|f| f.from.as_deref() == Some(&a.ident) || f.to.as_deref() == Some(&a.ident)).count();
    info.push(KV { k: ss(tr("Twoje loty")), v: ss(fmt::flights(visits)) });
    app.set_airport_info(model(info));
    let mut freqs: Vec<FreqV> = a.freqs.iter().map(|f| FreqV { kind: ss(&f.kind), name: ss(&f.desc), mhz: ss(format!("{:.3}", f.mhz)) }).collect();
    freqs.sort_by(|x, y| x.kind.cmp(&y.kind));
    app.set_airport_freqs(model(freqs));
    let nearby: Vec<AptV> = db.within(a.lat, a.lon, 40.0, false).into_iter().filter(|(_, x)| x.ident != a.ident).take(25).map(|(d, x)| apt_v(x, format!("{:.0} NM", d))).collect();
    app.set_airport_nearby(model(nearby));
    // Mapa lotniska
    let span = 0.08;
    st.views[2].fit((a.lat - span, a.lon - span * 1.6, a.lat + span, a.lon + span * 1.6));
    let cached = st.weather.get(&a.ident).cloned();
    drop(st);
    set_runways(&a, cached.as_ref().and_then(|c| c.0.first()));
    render_airport_map();
    match cached {
        Some(c) if !fetch => show_weather(&a.ident, &c.0, &c.1, &c.2),
        _ => fetch_weather(&a.ident),
    }
}

pub(super) fn set_runways(a: &Airport, m: Option<&Metar>) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let wind = m.and_then(|m| Some((m.wind_dir? as f64, m.wind_kt? as f64, m.gust_kt)));
    let mut rows: Vec<RwyV> = Vec::new();
    let mut best: Option<(usize, f64)> = None;
    for r in a.runways.iter().filter(|r| !r.closed) {
        for (ident, hdg) in [(r.le.clone(), r.le_heading()), (r.he.clone(), r.he_heading())] {
            if ident.is_empty() {
                continue;
            }
            let (head, cross, wtext) = match (wind, hdg) {
                (Some((d, s, g)), Some(h)) => {
                    let (hw, cw) = landing::wind_components(d, s, h);
                    let gust = g.map(|g| format!(" ({} G{})", tr("porywy"), g)).unwrap_or_default();
                    (
                        hw,
                        cw,
                        format!("{} {:.0} kt · {} {:.0} kt {}{gust}", if hw >= 0.0 { tr("czołowy") } else { tr("tylny") }, hw.abs(), tr("boczny"), cw.abs(), if cw >= 0.0 { tr("z prawej") } else { tr("z lewej") }),
                    )
                }
                _ => (0.0, 0.0, String::new()),
            };
            if wind.is_some() && best.is_none_or(|b| head > b.1) {
                best = Some((rows.len(), head));
            }
            rows.push(RwyV {
                ident: ss(&ident),
                length: ss(format!("{} × {} ft", r.length_ft, r.width_ft)),
                surface: ss(if r.surface.is_empty() { "—".to_string() } else { r.surface.clone() }),
                heading: ss(hdg.map(|h| format!("{:03.0}° {}", h, tr("rzecz."))).unwrap_or_default()),
                wind: ss(wtext),
                head: head as f32,
                cross: cross as f32,
                best: false,
                ils: ss(a.freqs.iter().find(|f| f.desc.to_uppercase().contains(&format!("ILS {ident}")) || f.desc.to_uppercase().contains(&format!("RWY {ident}"))).map(|f| format!("{} {:.2}", f.desc, f.mhz)).unwrap_or_default()),
            });
        }
    }
    if let Some((i, _)) = best {
        rows[i].best = true;
    }
    app.set_airport_runways(model(rows));
}

pub(super) fn fetch_weather(icao: &str) {
    let st = state();
    let client = st.borrow().host.weather.clone();
    if let Some(ui) = ui() {
        ui.global::<App>().set_weather_status(ss(tr("Pobieranie pogody…")));
    }
    let icao = icao.to_string();
    std::thread::spawn(move || {
        let metars = client.metars(&icao, 4);
        let taf = client.taf(&icao);
        later(move || {
            let (m, t, status) = match (metars, taf) {
                (Ok(m), Ok(t)) => {
                    let s = if m.is_empty() { tr("Lotnisko nie nadaje METAR-u").to_string() } else { trf!("Pobrano {t}", t = fmt::clock(unix_ms())) };
                    (m, t, s)
                }
                (Err(e), _) | (_, Err(e)) => (Vec::new(), None, trf!("Pogoda niedostępna: {e}")),
            };
            state().borrow_mut().weather.insert(icao.clone(), (m.clone(), t.clone(), status.clone()));
            show_weather(&icao, &m, &t, &status);
        });
    });
}

pub(super) fn show_weather(icao: &str, metars: &[Metar], taf: &Option<Taf>, status: &str) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    if state().borrow().airport.as_deref() != Some(icao) {
        return;
    }
    let groups = |raw: &str| -> ModelRc<GroupV> { model(metar_dec::decode(raw).into_iter().map(|g| GroupV { token: ss(g.token), meaning: ss(g.meaning) }).collect()) };
    let m = metars.first();
    app.set_weather_status(ss(status));
    app.set_metar_raw(ss(m.map(|m| m.raw.clone()).unwrap_or_default()));
    app.set_metar_groups(groups(m.map(|m| m.raw.as_str()).unwrap_or("")));
    app.set_taf_raw(ss(taf.as_ref().map(|t| t.raw.clone()).unwrap_or_default()));
    app.set_taf_groups(groups(taf.as_ref().map(|t| t.raw.as_str()).unwrap_or("")));
    app.set_metar_history(model(metars.iter().skip(1).take(6).map(|m| ss(&m.raw)).collect()));
    let mut apt = app.get_airport();
    apt.cat = ss(m.map(flight_category).unwrap_or_default());
    app.set_airport(apt);
    let summary = match m {
        Some(m) => vec![
            KV {
                k: ss(tr("Wiatr")),
                v: ss(match (m.wind_variable, m.wind_dir, m.wind_kt) {
                    (true, _, Some(s)) => format!("VRB {s} kt"),
                    (_, Some(d), Some(s)) => format!("{d:03}° {s} kt{}", m.gust_kt.map(|g| format!(" G{g}")).unwrap_or_default()),
                    _ => "—".into(),
                }),
            },
            KV { k: ss(tr("Widzialność")), v: ss(m.visibility.clone().map(|v| format!("{v} SM")).unwrap_or_default()) },
            KV { k: ss(tr("Temperatura")), v: ss(m.temp_c.map(|t| format!("{t:.0} °C / {:.0}", m.dewpoint_c.unwrap_or(t))).unwrap_or_default()) },
            KV { k: "QNH".into(), v: ss(m.altim_hpa.map(|q| format!("{q:.0} hPa")).unwrap_or_default()) },
            KV { k: ss(tr("Kategoria")), v: ss(flight_category(m)) },
        ],
        None => Vec::new(),
    };
    app.set_metar_summary(model(summary));
    if let Some(a) = airports::db().and_then(|d| d.get(icao).cloned()) {
        set_runways(&a, m);
        crate::briefing::set(&a, metars, taf);
    }
}

pub fn render_airport_map() {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let st = st.borrow();
    let Some(ident) = st.airport.clone() else { return };
    let Some(db) = airports::db() else { return };
    let Some(a) = db.get(&ident) else { return };
    let dark = ui.global::<Theme>().get_dark();
    let mut ov = Overlay::default();
    for (_, x) in db.within(a.lat, a.lon, 25.0, false).into_iter().take(40) {
        ov.points.push(Point { lat: x.lat, lon: x.lon, label: x.ident.clone(), kind: if x.ident == a.ident { 2 } else if x.kind <= 1 { 0 } else { 3 } });
        for r in x.runways.iter().filter(|r| r.has_coords() && !r.closed) {
            ov.runways.push((r.le_lat, r.le_lon, r.he_lat, r.he_lon));
        }
    }
    if !st.legs.is_empty() {
        let pts: Vec<(f64, f64, i32)> = st.legs.iter().filter_map(|l| Some((l.lat?, l.lon?, l.alt1_ft.unwrap_or(0.0) as i32))).collect();
        for l in &st.legs {
            if let (Some(la), Some(lo)) = (l.lat, l.lon) {
                ov.points.push(Point { lat: la, lon: lo, label: l.fix.clone(), kind: 1 });
            }
        }
        ov.tracks.push(Track { pts, color: Some([255, 0, 220, 230]), width: 2.5 });
    }
    if let Some(s) = st.host.snapshot() {
        ov.aircraft = Some((s.position.lat, s.position.lon, s.attitude.hdg_true_deg));
    }
    let r = map::render(&st.views[2], style(&st, dark), &ov, st.scale, dark);
    app.set_map_airport(r.image);
    app.set_map_airport_labels(labels(r.labels));
}

pub fn load_procedures(icao: &str) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let host = st.borrow().host.clone();
    if st.borrow().procs.as_ref().is_some_and(|(i, _)| i == icao) {
        return;
    }
    let src = host.source();
    let Some(provider) = src.navdata().filter(|_| src.is_connected()) else {
        app.set_procedures_status(ss(tr("Procedury pochodzą z bazy MSFS – uruchom symulator (albo tryb symulacji: EPWA, EPGD).")));
        return;
    };
    app.set_procedures_status(ss(tr("Pobieranie procedur z MSFS…")));
    let icao = icao.to_string();
    let kind = src.kind();
    let nav = host.nav.clone();
    std::thread::spawn(move || {
        let res = nav.procedures(provider.as_ref(), kind, &icao);
        later(move || {
            let Some(ui) = self::ui() else { return };
            let app = ui.global::<App>();
            match res {
                Ok(p) => {
                    let mut rows = Vec::new();
                    let head = |t: &str| ProcV { name: ss(t), info: ss(""), kind: -1 };
                    if !p.departures.is_empty() {
                        rows.push(head(&tr("ODLOTY (SID)").to_string()));
                        rows.extend(p.departures.iter().map(|d| ProcV { name: ss(&d.name), info: ss(d.runway_transitions.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(" ")), kind: 0 }));
                    }
                    if !p.arrivals.is_empty() {
                        rows.push(head(&tr("PRZYLOTY (STAR)").to_string()));
                        rows.extend(p.arrivals.iter().map(|d| ProcV { name: ss(&d.name), info: ss(d.runway_transitions.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(" ")), kind: 1 }));
                    }
                    if !p.approaches.is_empty() {
                        rows.push(head(&tr("PODEJŚCIA").to_string()));
                        rows.extend(p.approaches.iter().map(|a| ProcV { name: ss(&a.name), info: ss(a.transitions.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(" ")), kind: 2 }));
                    }
                    app.set_procedures_status(ss(trf!("{n} procedur z bazy symulatora", n = p.departures.len() + p.arrivals.len() + p.approaches.len())));
                    app.set_procedures(model(rows));
                    state().borrow_mut().procs = Some((icao, p));
                }
                Err(e) => app.set_procedures_status(ss(e.message)),
            }
        });
    });
}

pub fn open_procedure(name: &str) {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let Some((icao, p)) = st.borrow().procs.clone() else { return };
    let (kind, runway) = if p.departures.iter().any(|d| d.name == name) {
        ("departure", None)
    } else if p.arrivals.iter().any(|d| d.name == name) {
        ("arrival", None)
    } else {
        ("approach", None::<&str>)
    };
    let pick_rwy = |list: &[opentakeoff_core::navdata::TerminalProcedure]| list.iter().find(|d| d.name == name).and_then(|d| d.runway_transitions.first().map(|t| t.name.clone()));
    let rwy = match kind {
        "departure" => pick_rwy(&p.departures),
        "arrival" => pick_rwy(&p.arrivals),
        _ => runway.map(str::to_string),
    };
    let rwy = rwy.filter(|r| r != "ALL");
    match p.legs(kind, name, rwy.as_deref(), None) {
        Ok(info) => {
            let u = units(&st.borrow().s);
            let mut legs = info.legs.clone();
            legs.extend(info.missed.iter().cloned());
            let rows: Vec<LegV> = legs
                .iter()
                .map(|l| LegV {
                    fix: ss(if l.fix.is_empty() { "(—)".to_string() } else { l.fix.clone() }),
                    kind: ss(&l.leg_type),
                    alt: ss(match (l.alt_desc.as_deref(), l.alt1_ft, l.alt2_ft) {
                        (Some("AT"), Some(a), _) => u.alt_full(a),
                        (Some("ABOVE"), Some(a), _) => format!("≥ {}", u.alt_full(a)),
                        (Some("BELOW"), Some(a), _) => format!("≤ {}", u.alt_full(a)),
                        (Some("BETWEEN"), Some(a), Some(b)) => format!("{} – {}", u.alt(b), u.alt_full(a)),
                        _ => String::new(),
                    }),
                    speed: ss(l.speed_kt.map(|s| format!("≤ {s:.0} kt")).unwrap_or_default()),
                    course: ss(l.course_deg.map(|c| format!("{c:03.0}°")).unwrap_or_default()),
                    dist: ss(l.dist_nm.map(|d| format!("{d:.1} NM")).unwrap_or_default()),
                })
                .collect();
            app.set_procedure_legs(model(rows));
            app.set_procedure_title(ss(format!("{} · {}{}", icao, info.name, info.runway.map(|r| format!(" · RWY {r}")).unwrap_or_default())));
            st.borrow_mut().legs = info.legs;
            // Mapa dopasowana do procedury
            let b = map::bounds(st.borrow().legs.iter().filter_map(|l| Some((l.lat?, l.lon?))));
            if let Some(b) = b {
                st.borrow_mut().views[2].fit(b);
            }
            render_airport_map();
        }
        Err(e) => toast(e.message),
    }
}

pub fn refresh_watchlist(fetch: bool) {
    let st = state();
    let (list, client) = {
        let s = st.borrow();
        (s.s.watchlist.clone(), s.host.weather.clone())
    };
    if fetch {
        st.borrow_mut().watch_fetched = Some(Instant::now());
        let ids = list.clone();
        std::thread::spawn(move || {
            if let Ok(ms) = client.latest(&ids) {
                later(move || {
                    let st = state();
                    for m in ms {
                        st.borrow_mut().watch.insert(m.station.clone(), m);
                    }
                    refresh_watchlist(false);
                });
            }
        });
    }
    let Some(ui) = ui() else { return };
    let db = airports::db();
    let s = st.borrow();
    let rows: Vec<WxV> = list
        .iter()
        .map(|icao| {
            let m = s.watch.get(icao);
            WxV {
                icao: ss(icao),
                name: ss(db.as_ref().and_then(|d| d.get(icao).map(|a| a.name.clone())).unwrap_or_default()),
                cat: ss(m.map(flight_category).unwrap_or_default()),
                raw: ss(m.map(|m| m.raw.clone()).unwrap_or_else(|| tr("Pobieranie…").into())),
                wind: ss(m
                    .map(|m| match (m.wind_variable, m.wind_dir, m.wind_kt) {
                        (true, _, Some(k)) => format!("VRB{k:02}KT"),
                        (_, Some(d), Some(k)) => format!("{d:03}/{k}{}", m.gust_kt.map(|g| format!("G{g}")).unwrap_or_default()) + " kt",
                        _ => String::new(),
                    })
                    .unwrap_or_default()),
                vis: ss(m.and_then(|m| m.visibility.clone()).map(|v| format!("{v} SM")).unwrap_or_default()),
                temp: ss(m.and_then(|m| m.temp_c).map(|t| format!("{t:.0} °C")).unwrap_or_default()),
                qnh: ss(m.and_then(|m| m.altim_hpa).map(|q| format!("Q{q:.0}")).unwrap_or_default()),
                age: ss(m.map(|m| fmt::ago(m.obs_time * 1000)).unwrap_or_default()),
            }
        })
        .collect();
    ui.global::<App>().set_watchlist(model(rows));
}
