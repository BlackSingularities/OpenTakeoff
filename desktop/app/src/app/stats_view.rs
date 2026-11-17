//! Karta Statystyki: sumy, wykresy, osiągnięcia i mapa lotów.
//! Część modułu `app` – wspólne pomocnicze funkcje i importy z `super`.

use super::*;

// ---------- Statystyki ----------

/// Początek okresu statystyk (ms): 0 – całość, 1 – 12 miesięcy, 2 – 30 dni, 3 – bieżący rok.
pub(super) fn period_start(period: i32) -> i64 {
    let now = unix_ms();
    match period {
        1 => now - 365 * 86_400_000,
        2 => now - 30 * 86_400_000,
        3 => Local.with_ymd_and_hms(Local::now().year(), 1, 1, 0, 0, 0).single().map(|d| d.timestamp_millis()).unwrap_or(0),
        _ => i64::MIN,
    }
}

/// Stopnie pilota wg nalotu (godziny, nazwa, belki na pagonach).
pub(super) fn ranks() -> [(i64, &'static str, i32); 8] {
    [
        (0, tr("Uczeń pilot"), 1),
        (10, tr("Pilot turystyczny"), 1),
        (50, tr("Pilot zawodowy"), 2),
        (150, tr("Drugi pilot"), 2),
        (400, tr("Starszy drugi pilot"), 3),
        (800, tr("Kapitan"), 4),
        (1500, tr("Starszy kapitan"), 4),
        (3000, tr("Szef pilotów"), 4),
    ]
}

/// Czas w fazach lotu z zapisanego śladu (minuty): ziemia, wznoszenie, przelot, zniżanie.
pub(super) fn phase_minutes(f: &FlightLog) -> [f64; 4] {
    let mut out = [0.0; 4];
    for w in f.track.windows(2) {
        let dt = (w[1].t - w[0].t) as f64 / 60_000.0;
        if !(0.0..=10.0).contains(&dt) {
            continue;
        }
        let p = &w[0];
        let vs = p.vs_fpm.map(|v| v as f64).unwrap_or_else(|| (w[1].alt_ft - w[0].alt_ft) as f64 / dt.max(0.01));
        let i = if p.on_ground.unwrap_or(p.gs_kt < 40 && p.agl_ft.unwrap_or(100) < 20) {
            0
        } else if vs > 300.0 {
            1
        } else if vs < -300.0 {
            3
        } else {
            2
        };
        out[i] += dt;
    }
    out
}

/// Seria dni z lotami: (bieżąca, najdłuższa).
pub(super) fn streaks(flights: &[FlightLog]) -> (i64, i64) {
    let mut days: Vec<i64> = flights.iter().filter_map(|f| Local.timestamp_millis_opt(f.start).single()).map(|d| d.date_naive().num_days_from_ce() as i64).collect();
    days.sort_unstable();
    days.dedup();
    let (mut best, mut run) = (0i64, 0i64);
    for (i, d) in days.iter().enumerate() {
        run = if i > 0 && days[i - 1] + 1 == *d { run + 1 } else { 1 };
        best = best.max(run);
    }
    let today = Local::now().date_naive().num_days_from_ce() as i64;
    let current = match days.last() {
        Some(&l) if l == today || l + 1 == today => run,
        _ => 0,
    };
    (current, best)
}

pub(super) fn extra_stats(flights: &[FlightLog], s: &stats::Stats, u: &Units, period: i32) -> StatsV {
    let mut v = StatsV::default();
    // Stopień pilota liczony z całego nalotu w okresie
    let hours = s.minutes / 60;
    let r = ranks();
    let idx = r.iter().rposition(|x| hours >= x.0).unwrap_or(0);
    v.rank = ss(r[idx].1);
    v.rank_stripes = r[idx].2;
    if let Some(next) = r.get(idx + 1) {
        let span = (next.0 - r[idx].0).max(1) as f64 * 60.0;
        v.rank_progress = (((s.minutes - r[idx].0 * 60) as f64) / span).clamp(0.0, 1.0) as f32;
        v.rank_next = ss(next.1);
        v.rank_left = ss(trf!("jeszcze {h} do stopnia: {r}", h = fmt::hours(next.0 * 60 - s.minutes), r = next.1));
    } else {
        v.rank_progress = 1.0;
        v.rank_left = ss(tr("Najwyższy stopień"));
    }
    v.period = ss(match period {
        1 => tr("ostatnie 12 miesięcy"),
        2 => tr("ostatnie 30 dni"),
        3 => tr("bieżący rok"),
        _ => tr("cały dziennik"),
    });
    if s.flights > 0 {
        v.avg_flight = ss(fmt::duration_min(s.minutes / s.flights as i64));
        v.avg_dist = ss(u.dist(s.distance_nm as f64 / s.flights as f64));
    }
    let laps = s.distance_nm as f64 / 21_600.0;
    v.earth_laps = ss(if laps >= 0.01 { trf!("{x}× dookoła Ziemi", x = format!("{laps:.2}").replace('.', ",")) } else { String::new() });
    let (cur, best) = streaks(flights);
    v.streak = ss(if cur > 0 { trf!("{n} dni z rzędu", n = cur) } else { String::new() });
    v.streak_best = ss(if best > 0 { trf!("rekord: {n} dni", n = best) } else { String::new() });
    // Fazy lotu
    let mut ph = [0.0f64; 4];
    for f in flights {
        let m = phase_minutes(f);
        for i in 0..4 {
            ph[i] += m[i];
        }
    }
    let total: f64 = ph.iter().sum::<f64>().max(1.0);
    let t = |m: f64| ss(fmt::duration_min(m.round() as i64));
    v.t_ground = t(ph[0]);
    v.t_climb = t(ph[1]);
    v.t_cruise = t(ph[2]);
    v.t_descent = t(ph[3]);
    v.f_ground = (ph[0] / total) as f32;
    v.f_climb = (ph[1] / total) as f32;
    v.f_cruise = (ph[2] / total) as f32;
    v.f_descent = (ph[3] / total) as f32;
    // Lądowania
    let lands: Vec<LandingReportLite> = flights.iter().flat_map(|f| f.all_landings()).map(|l| LandingReportLite { score: l.score, vs: l.vs_fpm.abs(), g: l.g_force, bounces: l.bounces, cross: l.cross_ft, xwind: l.crosswind_kt }).collect();
    if let Some(avg) = s.avg_score {
        v.score_value = avg.round() as i32;
        v.grade = ss(landing::grade_of(avg.round() as i32));
    } else {
        v.score_value = -1;
    }
    if !lands.is_empty() {
        let n = lands.len() as f64;
        v.butter = ss(lands.iter().filter(|l| l.vs < 60).count().to_string());
        let gs: Vec<f64> = lands.iter().filter_map(|l| l.g).collect();
        v.avg_g = ss(if gs.is_empty() { String::new() } else { format!("{:.2} G", gs.iter().sum::<f64>() / gs.len() as f64) });
        v.bounces = ss(format!("{:.0} %", lands.iter().filter(|l| l.bounces > 0).count() as f64 / n * 100.0));
        let cr: Vec<f64> = lands.iter().filter_map(|l| l.cross.map(|c| c.abs() as f64)).collect();
        v.avg_cross = ss(if cr.is_empty() { String::new() } else { format!("{:.0} ft", cr.iter().sum::<f64>() / cr.len() as f64) });
        v.max_xwind = ss(lands.iter().filter_map(|l| l.xwind).map(|x| x.abs()).max().map(|x| format!("{x} kt")).unwrap_or_default());
    }
    v
}

pub(super) struct LandingReportLite {
    score: i32,
    vs: i32,
    g: Option<f64>,
    bounces: i32,
    cross: Option<i32>,
    xwind: Option<i32>,
}

pub(super) fn bars(vals: &[(String, f64, String)], accent_last: bool) -> ModelRc<BarV> {
    let max = vals.iter().map(|x| x.1).fold(0.0, f64::max).max(1e-9);
    let n = vals.len();
    model(vals.iter().enumerate().map(|(i, (l, v, t))| BarV { label: ss(l), value: (v / max) as f32, text: ss(t), accent: accent_last && i + 1 == n }).collect())
}

/// Wykresy karty Statystyki: kalendarz aktywności, godziny, dni tygodnia, poziomy przelotu,
/// opadanie przy przyziemieniu, oceny i ostatnie lądowania.
pub(super) fn set_stats_charts(app: &App, flights: &[FlightLog], all: &[FlightLog]) {
    // Kalendarz ostatnich 26 tygodni (zawsze z całego dziennika), kolumny = tygodnie od poniedziałku
    let today = Local::now().date_naive();
    let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
    let first = monday - chrono::Duration::weeks(25);
    let mut mins = vec![0i64; 26 * 7];
    let mut count = vec![0usize; 26 * 7];
    for f in all {
        if let Some(d) = Local.timestamp_millis_opt(f.start).single() {
            let idx = (d.date_naive() - first).num_days();
            if (0..26 * 7).contains(&idx) {
                mins[idx as usize] += f.duration_min().max(0);
                count[idx as usize] += 1;
            }
        }
    }
    let hmax = mins.iter().copied().max().unwrap_or(0).max(1) as f32;
    let heat: Vec<BarV> = (0..26 * 7)
        .map(|i| {
            let day = first + chrono::Duration::days(i as i64);
            let future = day > today;
            BarV {
                label: ss(format!("{} {}", day.day(), fmt::month_short(day.month()))),
                value: if future { -1.0 } else if mins[i] == 0 && count[i] == 0 { 0.0 } else { (mins[i] as f32 / hmax).max(0.12) },
                text: ss(if count[i] == 0 { tr("brak lotów").to_string() } else { format!("{} · {}", fmt::flights(count[i]), fmt::duration_min(mins[i])) }),
                accent: day == today,
            }
        })
        .collect();
    app.set_stats_heat(model(heat));
    let mut months = Vec::new();
    let mut last_m = 0;
    for w in 0..26 {
        let d = first + chrono::Duration::weeks(w);
        months.push(if d.month() != last_m { fmt::month_short(d.month()).to_string() } else { String::new() });
        last_m = d.month();
    }
    app.set_stats_heat_months(model_strings(months));
    // Godzina startu i dzień tygodnia
    let mut hours = [0usize; 24];
    let mut wd = [0usize; 7];
    for f in flights {
        if let Some(d) = Local.timestamp_millis_opt(f.start).single() {
            hours[d.hour() as usize] += 1;
            wd[d.weekday().num_days_from_monday() as usize] += 1;
        }
    }
    let cnt = |n: usize| if n == 0 { String::new() } else { n.to_string() };
    app.set_stats_hours(bars(&hours.iter().enumerate().map(|(h, &n)| (if h % 3 == 0 { format!("{h:02}") } else { String::new() }, n as f64, cnt(n))).collect::<Vec<_>>(), false));
    let names = [tr("Pn"), tr("Wt"), tr("Śr"), tr("Cz"), tr("Pt"), tr("Sb"), tr("Nd")];
    app.set_stats_weekdays(bars(&wd.iter().enumerate().map(|(i, &n)| (names[i].to_string(), n as f64, cnt(n))).collect::<Vec<_>>(), false));
    // Najwyższy poziom lotu
    let lv_edges = [(0, "<5k"), (5_000, "5–10k"), (10_000, "FL100"), (20_000, "FL200"), (30_000, "FL300"), (35_000, "FL350"), (40_000, "FL400+")];
    let mut lv = [0usize; 7];
    for f in flights {
        let i = lv_edges.iter().rposition(|e| f.max_alt_ft >= e.0).unwrap_or(0);
        lv[i] += 1;
    }
    app.set_stats_levels(bars(&lv_edges.iter().zip(lv).map(|(e, n)| (e.1.to_string(), n as f64, cnt(n))).collect::<Vec<_>>(), false));
    // Prędkość opadania przy przyziemieniu
    let mut lands: Vec<(i64, i32, i32, String)> = flights.iter().flat_map(|f| f.all_landings().into_iter().map(move |l| (l.at.unwrap_or(f.end), l.score, l.vs_fpm.abs(), l.airport.clone().or(f.to.clone()).unwrap_or_default()))).collect();
    let vs_edges = [(0, "<60"), (60, "60"), (120, "120"), (180, "180"), (240, "240"), (360, "360"), (600, "600+")];
    let mut vb = [0usize; 7];
    for l in &lands {
        vb[vs_edges.iter().rposition(|e| l.2 >= e.0).unwrap_or(0)] += 1;
    }
    app.set_stats_vs(bars(&vs_edges.iter().zip(vb).map(|(e, n)| (e.1.to_string(), n as f64, cnt(n))).collect::<Vec<_>>(), false));
    // Oceny wg stopni
    let grades = [(90, 101), (75, 90), (55, 75), (35, 55), (i32::MIN, 35)];
    let total = lands.len().max(1) as f32;
    let gmax = grades.iter().map(|g| lands.iter().filter(|l| l.1 >= g.0 && l.1 < g.1).count()).max().unwrap_or(1).max(1) as f32;
    app.set_stats_grades(model(
        grades
            .iter()
            .map(|g| {
                let n = lands.iter().filter(|l| l.1 >= g.0 && l.1 < g.1).count();
                RowV { name: ss(landing::grade_of(if g.0 == i32::MIN { 0 } else { g.0 })), a: ss(n.to_string()), b: ss(format!("{:.0} %", n as f32 / total * 100.0)), level: n as f32 / gmax, id: ss("") }
            })
            .collect(),
    ));
    // Ostatnie 30 lądowań
    lands.sort_by_key(|l| l.0);
    let recent: Vec<BarV> = lands.iter().rev().take(30).rev().map(|l| BarV { label: ss(&l.3), value: (l.1.max(0) as f32 / 100.0).max(0.04), text: ss(format!("{} · {} ft/min", l.1, l.2)), accent: false }).collect();
    app.set_stats_recent(model(recent));
}

pub fn refresh_stats() {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let mut st = st.borrow_mut();
    let u = units(&st.s);
    let store = st.host.logbook.clone();
    st.stats_rev = store.revision();
    let all_flights: Vec<FlightLog> = store.flights().iter().map(|f| store.effective(f)).collect();
    let period = app.get_stats_period();
    let from = period_start(period);
    let flights: Vec<FlightLog> = all_flights.iter().filter(|f| f.start >= from).cloned().collect();
    let s = stats::compute(&flights);
    let rec = |r: &Option<stats::Record>, f: &dyn Fn(f64) -> String| (r.as_ref().map(|r| f(r.value)).unwrap_or_default(), r.as_ref().map(|r| r.flight_id.clone()).unwrap_or_default());
    let (longest, longest_id) = rec(&s.longest, &|v| fmt::duration_min(v as i64));
    let (farthest, farthest_id) = rec(&s.farthest, &|v| u.dist(v));
    let (highest, highest_id) = rec(&s.highest, &|v| u.alt_full(v));
    let (softest, softest_id) = rec(&s.softest, &|v| format!("{v:.0} ft/min"));
    let (best, best_id) = rec(&s.best_landing, &|v| format!("{v:.0}"));
    app.set_stats(StatsV {
        flights: ss(s.flights.to_string()),
        hours: ss(fmt::hours(s.minutes)),
        distance: ss(u.dist(s.distance_nm as f64)),
        landings: ss(s.landings.to_string()),
        avg_score: ss(s.avg_score.map(|v| format!("{v:.0}")).unwrap_or_default()),
        avg_vs: ss(s.avg_vs_fpm.map(|v| trf!("średnio {v} ft/min", v = format!("{v:.0}"))).unwrap_or_default()),
        airports: ss(s.airports.to_string()),
        aircraft: ss(s.aircraft_types.to_string()),
        since: ss(s.first_flight.map(|t| trf!("od {d}", d = fmt::date(t))).unwrap_or_default()),
        longest: ss(longest),
        farthest: ss(farthest),
        highest: ss(highest),
        softest: ss(softest),
        best: ss(best),
        longest_id: ss(longest_id),
        farthest_id: ss(farthest_id),
        highest_id: ss(highest_id),
        softest_id: ss(softest_id),
        best_id: ss(best_id),
        ..extra_stats(&flights, &s, &u, period)
    });
    set_stats_charts(&app, &flights, &all_flights);
    // Osiągnięcia i wyzwania tygodnia
    let ach = achievements::compute(&all_flights, unix_ms());
    let icon = |i: achievements::Icon| i as i32;
    app.set_ach_badges(model(
        ach.badges
            .iter()
            .map(|b| BadgeV { id: ss(b.id), title: ss(&b.title), desc: ss(&b.desc), icon: icon(b.icon), level: b.level as i32, levels: b.levels as i32, progress: b.progress, value: ss(&b.value) })
            .collect(),
    ));
    app.set_ach_week(model(
        ach.week
            .iter()
            .map(|c| ChallengeV {
                title: ss(&c.title),
                desc: ss(&c.desc),
                icon: icon(c.icon),
                airport: ss(c.airport.clone().unwrap_or_default()),
                done: c.done,
                progress: c.progress,
                status: ss(&c.status),
                flight_id: ss(c.flight_id.clone().unwrap_or_default()),
            })
            .collect(),
    ));
    app.set_ach_week_title(ss(&ach.week_title));
    app.set_ach_week_left(ss(&ach.week_left));
    app.set_ach_count(ss(trf!("{a} z {b} poziomów", a = ach.unlocked, b = ach.total_levels)));
    // Powiadomienie o nowym poziomie albo zaliczonym wyzwaniu (pierwsze liczenie – bez powiadomień)
    let keys = ach.keys();
    if !all_flights.is_empty() || st.s.ach_init {
        let fresh: Vec<String> = if st.s.ach_init { keys.iter().filter(|k| !st.s.ach_seen.contains(k)).cloned().collect() } else { Vec::new() };
        if !st.s.ach_init || !fresh.is_empty() {
            st.s.ach_init = true;
            st.s.ach_seen = keys.clone();
            st.s.save();
        }
        if let Some(k) = fresh.last() {
            let text = match k.split_once(':') {
                Some((id, lvl)) => {
                    let title = ach.badges.iter().find(|b| b.id == id).map(|b| b.title.clone()).unwrap_or_default();
                    trf!("Nowe osiągnięcie: {t} – poziom {l}", t = title, l = lvl)
                }
                None => {
                    let title = ach.week.iter().find(|c| &c.key == k).map(|c| c.title.clone()).unwrap_or_default();
                    trf!("Wyzwanie tygodnia zaliczone: {t}", t = title)
                }
            };
            let more = fresh.len().saturating_sub(1);
            let text = if more > 0 { format!("{text} (+{more})") } else { text };
            later(move || toast(text));
        }
    }
    // Ostatnie 12 miesięcy (także puste)
    let metric = app.get_stats_metric();
    let now = Local::now();
    let mut months = Vec::new();
    for i in (0..12).rev() {
        let mut y = now.year();
        let mut m = now.month() as i32 - i;
        while m <= 0 {
            m += 12;
            y -= 1;
        }
        let key = format!("{y:04}-{m:02}");
        let found = s.months.iter().find(|x| x.key == key);
        let v = match metric {
            1 => found.map(|x| x.minutes as f64 / 60.0).unwrap_or(0.0),
            2 => found.map(|x| x.distance_nm as f64).unwrap_or(0.0),
            _ => found.map(|x| x.flights as f64).unwrap_or(0.0),
        };
        months.push((fmt::month_short(m as u32).to_string(), v, i == 0));
    }
    let max = months.iter().map(|m| m.1).fold(0.0, f64::max).max(1.0);
    app.set_stats_months(model(
        months
            .into_iter()
            .map(|(l, v, cur)| BarV {
                label: ss(l),
                value: (v / max) as f32,
                text: ss(if v == 0.0 { String::new() } else if metric == 1 { format!("{v:.1}") } else { format!("{v:.0}") }),
                accent: cur,
            })
            .collect(),
    ));
    let smax = s.score_buckets.iter().copied().max().unwrap_or(1).max(1) as f32;
    app.set_stats_scores(model(
        s.score_buckets
            .iter()
            .enumerate()
            .map(|(i, &n)| BarV { label: ss(format!("{}", i * 10)), value: n as f32 / smax, text: ss(if n == 0 { String::new() } else { n.to_string() }), accent: false })
            .collect(),
    ));
    let rows = |v: &[stats::Counted]| -> ModelRc<RowV> {
        let max = v.iter().map(|c| c.flights).max().unwrap_or(1).max(1) as f32;
        model(v.iter().map(|c| RowV { name: ss(&c.name), a: ss(fmt::flights(c.flights)), b: ss(fmt::hours(c.minutes)), level: c.flights as f32 / max, id: ss("") }).collect())
    };
    app.set_stats_aircraft(rows(&s.top_aircraft));
    app.set_stats_airports(rows(&s.top_airports));
    app.set_stats_routes(rows(&s.top_routes));
    // Mapa wszystkich lotów
    let all: Vec<(f64, f64)> = flights.iter().flat_map(|f| [f.track.first(), f.track.last()]).flatten().map(|p| (p.lat, p.lon)).collect();
    if !st.views[3].user_moved {
        if let Some(b) = map::bounds(all.into_iter()) {
            st.views[3].fit(b);
        }
    }
    drop(st);
    render_stats_map();
}

pub fn render_stats_map() {
    let Some(ui) = ui() else { return };
    let app = ui.global::<App>();
    let st = state();
    let st = st.borrow();
    let dark = ui.global::<Theme>().get_dark();
    let store = st.host.logbook.clone();
    let accent = ACCENTS[st.s.accent.min(ACCENTS.len() - 1)];
    let mut ov = Overlay::default();
    let from = period_start(app.get_stats_period());
    for f in store.flights().iter().filter(|f| f.start >= from).take(500) {
        let step = (f.track.len() / 300).max(1);
        ov.tracks.push(Track {
            pts: f.track.iter().step_by(step).map(|p| (p.lat, p.lon, p.alt_ft)).collect(),
            color: Some([(accent >> 16) as u8, (accent >> 8) as u8, accent as u8, 200]),
            width: 1.8,
        });
    }
    let r = map::render(&st.views[3], style(&st, dark), &ov, st.scale, dark);
    app.set_map_stats(r.image);
}
