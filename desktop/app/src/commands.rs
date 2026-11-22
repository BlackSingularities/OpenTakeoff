//! Polecenia interfejsu (App.cmd): przełączanie kart, ustawienia, dziennik, eksport, parowanie telefonów
//! i reszta – polecenia kart w osobnych modułach (instruments, flightplan, checklists, navmap, overlays, shots).

use crate::*;

pub(crate) fn cmd(ui: &AppWindow, id: &str, arg: &str) {
    let app = ui.global::<App>();
    let host = state().borrow().host.clone();
    match id {
        "tab" => {
            let t: i32 = arg.parse().unwrap_or(0);
            state().borrow_mut().s.tab = t;
            match t {
                0 | 1 => app::refresh_live(true),
                2 => {
                    app::refresh_logbook();
                    app::render_flight_map();
                }
                3 => app::refresh_stats(),
                4 => app::render_airport_map(),
                8 => instruments::refresh(),
                9 => navmap::render(true),
                10 => {
                    overlays::refresh_monitors();
                    overlays::push_settings();
                }
                11 => shots::refresh(),
                12 => flightplan::refresh(),
                13 => checklists::refresh(),
                _ => {}
            }
            if t == 0 {
                simbrief::refresh();
            }
        }
        // Ctrl+PgDn / Ctrl+PgUp: następna / poprzednia karta w kolejności paska kart
        "tab-step" => {
            const ORDER: [i32; 14] = [0, 1, 8, 9, 2, 3, 11, 12, 13, 4, 5, 10, 6, 7];
            let cur = ORDER.iter().position(|&t| t == app.get_tab()).unwrap_or(0) as i32;
            let step: i32 = arg.parse().unwrap_or(1);
            let next = ORDER[(cur + step).rem_euclid(ORDER.len() as i32) as usize];
            app.set_tab(next);
            cmd(ui, "tab", &next.to_string());
        }
        "welcome-done" => {
            let st = state();
            let mut b = st.borrow_mut();
            b.s.welcome_done = true;
            b.s.save();
        }
        "simbrief-fetch" => simbrief::fetch(arg),
        "simbrief-clear" => simbrief::clear(),
        "plan-map" => {
            let pts: Vec<(f64, f64)> = simbrief::route().into_iter().map(|p| (p.0, p.1)).collect();
            if pts.len() > 1 {
                {
                    let st = state();
                    let mut b = st.borrow_mut();
                    b.s.nav_plan = true;
                }
                navmap::fit_points(&pts);
                app.set_tab(9);
                navmap::render(true);
            }
        }
        "shot-delete" => {
            if let Some(id) = shots::selected_id() {
                confirm(tr("Usunąć ten zrzut ekranu? Plik zniknie z dysku.").into(), tr("Usuń"), "shot-delete", vec![id]);
            }
        }
        "pair-new-code" => {
            host.pairing.regenerate_code();
            app::refresh_status();
            toast(trf!("Nowy kod parowania: {c}", c = host.pairing.code()));
        }
        "pair-revoke-all" => confirm(tr("Unieważnić wszystkie sparowane telefony? Każdy będzie musiał połączyć się ponownie z nowym kodem.").into(), tr("Unieważnij"), "revoke-all", Vec::new()),
        "device-revoke" => confirm(tr("Unieważnić ten telefon? Przy następnym połączeniu będzie potrzebny kod parowania.").into(), tr("Unieważnij"), "revoke", vec![arg.into()]),
        "device-disconnect" => host.server.disconnect(arg),
        "copy-address" => {
            let text = format!("{}:{}", host.addresses().first().cloned().unwrap_or_default(), host.server.port());
            if let Ok(mut c) = arboard::Clipboard::new() {
                let _ = c.set_text(text.clone());
            }
            toast(trf!("Skopiowano adres {text}"));
        }
        "source" | "xplane-host" => {
            let (sim, kind, xp_host) = {
                let st = state();
                let mut b = st.borrow_mut();
                if id == "xplane-host" {
                    b.s.xplane_host = arg.trim().to_string();
                } else {
                    match arg {
                        "sim" => b.s.simulation = true,
                        other => {
                            b.s.simulation = false;
                            b.s.sim_kind = match other {
                                "msfs" => 1,
                                "xplane" => 2,
                                "remote" => 3,
                                _ => 0,
                            };
                        }
                    }
                }
                (b.s.simulation, b.s.sim_kind, b.s.xplane_host.clone())
            };
            save();
            if sim {
                if !host.simulation.load(Ordering::SeqCst) {
                    host.switch_source(true);
                }
            } else {
                host.simulation.store(false, Ordering::SeqCst);
                host.set_sim(kind as i64, &xp_host);
            }
            app.set_sim_kind(kind);
            app.set_xplane_host(xp_host.into());
            app::refresh_status();
        }
        // Drugi komputer: „adres|kod parowania” (kod pusty – zapamiętany token)
        "remote-connect" => {
            let (addr, code) = arg.split_once('|').unwrap_or((arg, ""));
            let addr = addr.trim().to_string();
            if opentakeoff_core::remote::parse_address(&addr).is_none() && !addr.is_empty() {
                toast(tr("Nieprawidłowy adres – wpisz np. 192.168.1.20 albo 192.168.1.20:47810").to_string());
                return;
            }
            {
                let st = state();
                let mut b = st.borrow_mut();
                if b.s.remote_host != addr {
                    b.s.remote_token.clear();
                }
                b.s.remote_host = addr.clone();
                b.s.simulation = false;
                b.s.sim_kind = 3;
            }
            save();
            host.simulation.store(false, Ordering::SeqCst);
            host.sim_kind.store(3, Ordering::SeqCst);
            host.set_remote(&addr, code);
            app.set_sim_kind(3);
            app.set_remote_host(addr.into());
            app.set_remote_code("".into());
            app::refresh_status();
        }
        "simulation" => {
            let sim = arg == "1";
            if sim != host.simulation.load(Ordering::SeqCst) {
                host.switch_source(sim);
                state().borrow_mut().s.simulation = sim;
                save();
            }
            app::refresh_status();
        }
        "look-sensitivity" => {
            let v: i32 = arg.parse::<f32>().map(|f| f.round() as i32).unwrap_or(100).clamp(20, 400);
            host.set_look_sensitivity(v);
            state().borrow_mut().s.look_sensitivity = v;
            app.set_look_sensitivity(v);
            save();
        }
        "record-mark" => {
            if host.mark() {
                toast(tr("Dodano znacznik"));
            }
        }
        "record-stop" => host.stop_recording(),
        "record-resume" => {
            host.resume_recording();
            toast(tr("Zapis wznowiony – zacznie się, gdy samolot będzie w powietrzu"));
        }
        "live-clear" => {
            host.clear_live();
            app::refresh_live(true);
        }
        "watch-refresh" => app::refresh_watchlist(true),
        "airport-search" => app::search_airports(arg),
        "airport-search-open" => {
            if let Some(a) = opentakeoff_core::aero::airports::db().and_then(|d| d.search(arg, 1).first().map(|a| a.ident.clone())) {
                app::open_airport(&a, false);
            }
        }
        "airport-open" => {
            app.set_tab(4);
            app.set_airport_view(0);
            app::open_airport(arg, false);
        }
        "airport-refresh" => app::open_airport(arg, true),
        "airport-near-aircraft" => {
            if let (Some(s), Some(db)) = (host.snapshot(), opentakeoff_core::aero::airports::db()) {
                if let Some((_, a)) = db.within(s.position.lat, s.position.lon, 100.0, false).into_iter().find(|(_, a)| a.kind <= 2) {
                    let id = a.ident.clone();
                    app::open_airport(&id, false);
                }
            }
        }
        "airport-watch" => {
            {
                let st = state();
                let mut s = st.borrow_mut();
                let list = &mut s.s.watchlist;
                if let Some(i) = list.iter().position(|w| w == arg) {
                    list.remove(i);
                } else if !arg.is_empty() {
                    list.push(arg.to_string());
                }
                let watched = list.iter().any(|w| w == arg);
                app.set_airport_watched(watched);
            }
            save();
            app::refresh_watchlist(true);
        }
        "procedures-load" => app::load_procedures(arg),
        "procedure-open" => app::open_procedure(arg),
        "filter" => app::refresh_logbook(),
        "flight-open" => {
            if app.get_tab() != 2 {
                app.set_tab(2);
                app::refresh_logbook();
            }
            state().borrow_mut().selected.clear();
            app::open_flight(arg);
            app::refresh_logbook();
        }
        "flight-toggle" => {
            {
                let st = state();
                let mut s = st.borrow_mut();
                if !s.selected.remove(arg) {
                    s.selected.insert(arg.to_string());
                }
                s.anchor = Some(arg.to_string());
            }
            app::refresh_logbook();
        }
        "flight-range" => {
            {
                let st = state();
                let mut s = st.borrow_mut();
                let anchor = s.anchor.clone().unwrap_or_else(|| arg.to_string());
                let a = s.list.iter().position(|x| *x == anchor);
                let b = s.list.iter().position(|x| x == arg);
                if let (Some(a), Some(b)) = (a, b) {
                    let range: Vec<String> = s.list[a.min(b)..=a.max(b)].to_vec();
                    s.selected.extend(range);
                }
            }
            app::refresh_logbook();
        }
        "select-all" => {
            {
                let st = state();
                let mut s = st.borrow_mut();
                let all = s.list.clone();
                s.selected.extend(all);
            }
            app::refresh_logbook();
        }
        "select-none" => {
            state().borrow_mut().selected.clear();
            app::refresh_logbook();
        }
        "flight-favorite" | "flight-from" | "flight-to" | "flight-aircraft" | "flight-tags" | "flight-notes" => {
            let Some(fid) = state().borrow().detail.as_ref().map(|f| f.id.clone()) else { return };
            let v = arg.trim().to_string();
            host.logbook.update_meta(&fid, |m| match id {
                "flight-favorite" => m.favorite = !m.favorite,
                "flight-from" => m.from = (!v.is_empty()).then(|| v.to_uppercase()),
                "flight-to" => m.to = (!v.is_empty()).then(|| v.to_uppercase()),
                "flight-aircraft" => m.aircraft = (!v.is_empty()).then(|| v.clone()),
                "flight-tags" => m.tags = v.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect(),
                _ => m.notes = arg.to_string(),
            });
            if id != "flight-notes" {
                app::open_flight(&fid);
                app::refresh_logbook();
            }
        }
        "marker-delete" => {
            let Some(f) = state().borrow().detail.clone() else { return };
            let t: i64 = arg.parse::<f64>().unwrap_or(0.0) as i64;
            if let Some(raw) = host.logbook.get(&f.id) {
                let mut nf = (*raw).clone();
                nf.markers.retain(|m| m.t / 1000 != t);
                host.logbook.replace(nf);
                app::open_flight(&f.id);
            }
        }
        "delete" => {
            let ids = targets();
            if !ids.is_empty() {
                confirm(trf!("Przenieść {n} do kosza? Usunięte loty nie wrócą przy synchronizacji, a telefon usunie je u siebie przy następnym połączeniu.", n = fmt::flights(ids.len())), tr("Do kosza"), "delete", ids);
            }
        }
        "restore" => {
            let n = host.logbook.restore(&targets());
            state().borrow_mut().selected.clear();
            toast(trf!("Przywrócono {n}", n = fmt::flights(n)));
            app.set_has_detail(false);
            app::refresh_logbook();
        }
        "empty-trash" => {
            let n = host.logbook.trash().len();
            if n > 0 {
                confirm(trf!("Usunąć trwale {n} z kosza? Tego nie da się cofnąć.", n = fmt::flights(n)), tr("Usuń trwale"), "empty-trash", Vec::new());
            }
        }
        "confirm-no" => {
            state().borrow_mut().confirm = None;
            app.set_confirm_text("".into());
        }
        "confirm-yes" => {
            app.set_confirm_text("".into());
            let Some((action, ids)) = state().borrow_mut().confirm.take() else { return };
            match action.as_str() {
                "delete" => {
                    let n = host.logbook.delete(&ids);
                    state().borrow_mut().selected.clear();
                    app.set_has_detail(false);
                    state().borrow_mut().detail = None;
                    toast(trf!("Przeniesiono do kosza: {n}", n = fmt::flights(n)));
                    app::refresh_logbook();
                }
                "empty-trash" => {
                    let n = host.logbook.empty_trash();
                    app.set_has_detail(false);
                    toast(trf!("Usunięto trwale: {n}", n = fmt::flights(n)));
                    app::refresh_logbook();
                }
                "revoke-all" => {
                    host.pairing.revoke_all();
                    toast(tr("Wszystkie telefony unieważnione"));
                }
                "revoke" => {
                    for t in ids {
                        host.pairing.revoke(&t);
                    }
                    toast(tr("Telefon unieważniony"));
                }
                "shot-delete" => {
                    shots::delete(&ids);
                    toast(tr("Usunięto zrzut ekranu"));
                }
                _ => {}
            }
            app::refresh_status();
        }
        "import" => app::import(),
        "export" => app::export(arg),
        "landing-analysis" => app::select_landing(arg.parse().unwrap_or(-1)),
        "export-all" => app::export("oab"),
        "replay-toggle" => {
            let st = state();
            let playing = {
                let mut s = st.borrow_mut();
                s.playing = !s.playing && s.detail.is_some();
                if s.playing && s.replay >= 0.999 {
                    s.replay = 0.0;
                }
                s.playing
            };
            app.set_replay_playing(playing);
        }
        "replay-speed" => {
            let v = arg.parse::<f64>().unwrap_or(60.0).clamp(1.0, 3600.0);
            state().borrow_mut().replay_speed = v;
        }
        "replay-seek" => {
            state().borrow_mut().replay = arg.parse::<f64>().unwrap_or(0.0).clamp(0.0, 1.0);
            app::update_cursor();
            app::render_flight_map();
        }
        "stats-refresh" => app::refresh_stats(),
        "open-logs" => {
            let _ = open::that(opentakeoff_core::log::file().parent().unwrap());
        }
        "open-data" => {
            let _ = open::that(opentakeoff_core::paths::config());
        }
        "open-url" => {
            let _ = open::that(arg);
        }
        "db-refresh" => load_airports(true),
        "theme" | "accent" | "scale" | "corners" | "map-style" => {
            let v: i32 = arg.parse().unwrap_or(0);
            {
                let st = state();
                let mut s = st.borrow_mut();
                match id {
                    "theme" => s.s.theme = v.clamp(0, 5),
                    "accent" => s.s.accent = v.clamp(0, ACCENTS.len() as i32 - 1) as usize,
                    "scale" => s.s.scale = [0.9, 1.0, 1.1, 1.25][v.clamp(0, 3) as usize],
                    "corners" => s.s.corners = v.clamp(0, 2),
                    _ => s.s.map_style = v.clamp(0, 4),
                }
            }
            let s = state().borrow().s.clone();
            apply_theme(ui, &s);
            apply_settings(ui, &s);
            save();
            app::rerender_maps();
        }
        "map-tilt" => {
            let on = arg == "1";
            state().borrow_mut().s.map_tilt = on;
            map::set_tilt_enabled(on);
            save();
            apply_settings(ui, &state().borrow().s.clone());
            app::rerender_maps();
            crate::flightplan::render_map();
        }
        "language" => {
            state().borrow_mut().s.language = arg.to_string();
            save();
            apply_settings(ui, &state().borrow().s.clone());
            toast(tr("Język zmieni się po ponownym uruchomieniu programu"));
        }
        "units-alt" | "units-speed" | "record-interval" => {
            let v: i32 = arg.parse().unwrap_or(0);
            {
                let st = state();
                let mut s = st.borrow_mut();
                match id {
                    "units-alt" => s.s.units_alt = v.clamp(0, 1),
                    "units-speed" => s.s.units_speed = v.clamp(0, 1),
                    _ => {
                        s.s.record_interval = v.clamp(0, 4);
                        host.set_interval(INTERVALS_MS[s.s.record_interval as usize]);
                    }
                }
            }
            save();
            apply_settings(ui, &state().borrow().s.clone());
            app::refresh_logbook();
        }
        "record-flights" | "auto-sync" | "autostart" | "start-minimized" | "close-to-tray" => {
            let on = arg == "1";
            {
                let st = state();
                let mut s = st.borrow_mut();
                match id {
                    "record-flights" => {
                        s.s.record_flights = on;
                        host.record_enabled.store(on, Ordering::SeqCst);
                    }
                    "auto-sync" => {
                        s.s.auto_sync = on;
                        host.set_sync_enabled(on);
                    }
                    "autostart" => {
                        s.s.autostart = on;
                        if let Err(e) = win::set_autostart(on) {
                            toast(trf!("Nie udało się zmienić autostartu: {e}"));
                        }
                    }
                    "start-minimized" => {
                        s.s.start_minimized = on;
                        if s.s.autostart {
                            let _ = win::set_autostart(true);
                        }
                    }
                    _ => s.s.close_to_tray = on,
                }
            }
            save();
            apply_settings(ui, &state().borrow().s.clone());
        }
        "sim-launch" => {
            let on = arg == "1";
            match simlaunch::set(on) {
                Ok(sims) => {
                    state().borrow_mut().s.sim_launch = on;
                    save();
                    let mut list: Vec<&str> = simlaunch::registered();
                    for s in sims {
                        if !list.contains(&s) {
                            list.push(s);
                        }
                    }
                    let sims = if list.is_empty() { tr("MSFS i X-Plane (czuwanie na proces symulatora)").to_string() } else { list.join(", ") };
                    toast(if on { trf!("OpenTakeoff uruchomi się razem z: {sims}") } else { tr("OpenTakeoff nie będzie się już uruchamiać z symulatorem").to_string() });
                }
                Err(e) => toast(trf!("Nie udało się zmienić pliku exe.xml symulatora: {e}")),
            }
            apply_settings(ui, &state().borrow().s.clone());
        }
        "update-check" => check_update(true),
        "update-install" => {
            app.set_update_dialog(false);
            install_update();
        }
        "update-later" => app.set_update_dialog(false),
        "update-skip" => {
            app.set_update_dialog(false);
            let v = app.get_update_version().to_string();
            let st = state();
            let mut b = st.borrow_mut();
            b.s.update_skip = v;
            b.s.save();
        }
        "update-auto" => {
            let on = arg == "1";
            app.set_update_auto(on);
            let st = state();
            let mut b = st.borrow_mut();
            b.s.update_auto = on;
            b.s.save();
        }
        "feedback" => feedback::open_form(),
        _ => {
            if !instruments::cmd(id, arg) && !flightplan::cmd(id, arg) && !checklists::cmd(id, arg) && !popout::cmd(id, arg) && !navmap::cmd(id, arg) && !overlays::cmd(id, arg) && !shots::cmd(id, arg) {
                log::warn(&format!("Nieznane polecenie: {id} {arg}"));
            }
        }
    }
}
