//! Pakiet briefingu planu lotu: strona HTML z trasą, paliwem, tabelą punktów, lotniskami (pasy,
//! częstotliwości) oraz METAR i TAF startu, celu i zapasowego. Otwiera się w przeglądarce z oknem
//! drukowania – „Microsoft Print to PDF” zapisuje ją jako PDF. Pliki zostają w folderze danych `briefing`.
//! Gdy pobrany OFP SimBrief ma ten sam start i cel, briefing dostaje też jego wagi, paliwo, navlog z
//! wiatrem i temperaturą, prędkości TLR i plan ATC; METAR/TAF z OFP zastępują pogodę, gdy sieć zawiedzie.

use chrono::Utc;
use opentakeoff_core::aero::airports;
use opentakeoff_core::aero::weather::WeatherClient;
use opentakeoff_core::lang::tr;
use opentakeoff_core::{log, paths, trf};

use crate::app::{later, toast};
use crate::flightplan::{fmt_alt, fuel_plan, hm, route_string, FPlan, Leg};
use crate::simbrief::{self, OfpApt, Plan as Sb};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Buduje briefing w tle (pogoda z sieci) i otwiera go w przeglądarce.
pub fn make(plan: FPlan, legs: Vec<Leg>) {
    if plan.origin.is_empty() || plan.dest.is_empty() {
        toast(tr("Wpisz lotnisko startu i celu"));
        return;
    }
    toast(tr("Przygotowuję briefing – pobieram pogodę…"));
    let sb = simbrief::plan().filter(|s| s.origin == plan.origin && s.dest == plan.dest);
    std::thread::spawn(move || {
        let html = build(&plan, &legs, sb.as_ref());
        let dir = paths::config().join("briefing");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join(format!("{}-{}-{}.html", plan.origin, plan.dest, Utc::now().format("%Y%m%d-%H%M")));
        let res = std::fs::write(&file, html);
        later(move || match res {
            Ok(()) => {
                if open::that(&file).is_err() {
                    toast(trf!("Briefing zapisany: {f}", f = file.display()));
                }
            }
            Err(e) => {
                log::warn(&format!("Briefing: {e}"));
                toast(trf!("Nie udało się zapisać briefingu: {e}"));
            }
        });
    });
}

fn airport_html(icao: &str, role: &str, wx: &WeatherClient, ofp: Option<&OfpApt>) -> String {
    let mut out = format!("<section><h2>{} <span class=role>{}</span></h2>", esc(icao), esc(role));
    if let Some(a) = airports::db().and_then(|d| d.get(icao).cloned()) {
        out += &format!(
            "<p class=sub>{} · {} · {} {} ft</p>",
            esc(&a.name),
            esc(&a.city),
            esc(tr("elewacja")),
            a.elev_ft.map(|e| e.to_string()).unwrap_or_else(|| "–".into())
        );
        let rwys: Vec<String> = a
            .runways
            .iter()
            .filter(|r| !r.closed)
            .map(|r| format!("<tr><td>{}/{}</td><td>{:.0} m</td><td>{:.0} m</td><td>{}</td><td>{}</td></tr>", esc(&r.le), esc(&r.he), r.length_ft as f64 * 0.3048, r.width_ft as f64 * 0.3048, esc(&r.surface), if r.lighted { "✓" } else { "" }))
            .collect();
        if !rwys.is_empty() {
            out += &format!(
                "<table><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr>{}</table>",
                esc(tr("Pas")),
                esc(tr("Długość")),
                esc(tr("Szerokość")),
                esc(tr("Nawierzchnia")),
                esc(tr("Światła")),
                rwys.join("")
            );
        }
        let freqs: Vec<String> = a.freqs.iter().filter(|f| f.mhz > 0.0).map(|f| format!("<span class=freq><b>{}</b> {:.3}</span>", esc(&f.kind), f.mhz)).collect();
        if !freqs.is_empty() {
            out += &format!("<p>{}</p>", freqs.join(" "));
        }
    }
    let ofp_metar = ofp.map(|a| a.metar.clone()).filter(|m| !m.is_empty());
    let ofp_taf = ofp.map(|a| a.taf.clone()).filter(|m| !m.is_empty());
    match wx.metars(icao, 3) {
        Ok(m) if !m.is_empty() => {
            out += "<h3>METAR</h3><pre>";
            for x in m.iter().take(3) {
                out += &esc(&x.raw);
                out += "\n";
            }
            out += "</pre>";
        }
        _ if ofp_metar.is_some() => out += &format!("<h3>METAR (OFP)</h3><pre>{}</pre>", esc(ofp_metar.as_deref().unwrap_or(""))),
        Ok(_) => out += &format!("<p class=muted>{}</p>", esc(tr("Brak METAR"))),
        Err(e) => out += &format!("<p class=muted>METAR: {}</p>", esc(&e)),
    }
    match wx.taf(icao) {
        Ok(Some(t)) => out += &format!("<h3>TAF</h3><pre>{}</pre>", esc(&t.raw)),
        _ if ofp_taf.is_some() => out += &format!("<h3>TAF (OFP)</h3><pre>{}</pre>", esc(ofp_taf.as_deref().unwrap_or(""))),
        Ok(None) => out += &format!("<p class=muted>{}</p>", esc(tr("Brak TAF"))),
        Err(e) => out += &format!("<p class=muted>TAF: {}</p>", esc(&e)),
    }
    out + "</section>"
}

fn build(p: &FPlan, legs: &[Leg], sb: Option<&Sb>) -> String {
    let total = legs.last().map(|l| l.cum).unwrap_or(0.0);
    let tas = p.tas_kt.max(60) as f64;
    let fuel = fuel_plan(p, total);
    let wx = WeatherClient::new();
    let kg = |v: f64| format!("{v:.0} kg");
    let fuel_rows = [
        (tr("Kołowanie").to_string(), kg(fuel.taxi)),
        (tr("Trasa").to_string(), kg(fuel.trip)),
        (trf!("Rezerwa trasy {p}%", p = format!("{:.0}", p.cont_pct)), kg(fuel.cont)),
        (if p.altn.is_empty() { tr("Zapasowe").to_string() } else { format!("{} {} · {:.0} NM", tr("Zapasowe"), p.altn, fuel.altn_nm) }, kg(fuel.altn)),
        (trf!("Rezerwa końcowa {m} min", m = p.reserve_min), kg(fuel.reserve)),
        (tr("Dodatkowe").to_string(), kg(fuel.extra)),
        (tr("Minimum do startu").to_string(), kg(fuel.takeoff())),
        (tr("Blok").to_string(), kg(fuel.block())),
    ];
    let fuel_html: String = fuel_rows.iter().map(|(k, v)| format!("<tr><td>{}</td><td class=num>{}</td></tr>", esc(k), esc(v))).collect();
    let leg_html: String = legs
        .iter()
        .enumerate()
        .map(|(i, l)| {
            format!(
                "<tr><td><b>{}</b></td><td>{}</td><td>{}</td><td class=num>{}</td><td class=num>{}</td><td class=num>{}</td><td class=num>{:.0}</td><td class=num>{}</td></tr>",
                esc(&l.ident),
                esc(&l.via),
                esc(&l.constraint),
                fmt_alt(l.alt_plan),
                if i == 0 { String::new() } else { format!("{:03.0}°", l.crs) },
                if i == 0 { String::new() } else { format!("{:.0}", l.dist) },
                l.cum,
                hm(l.cum / tas)
            )
        })
        .collect();
    let o = sb.map(|s| &s.ofp);
    let mut airports_html = airport_html(&p.origin, tr("start"), &wx, o.map(|o| &o.origin));
    airports_html += &airport_html(&p.dest, tr("cel"), &wx, o.map(|o| &o.dest));
    if !p.altn.is_empty() {
        airports_html += &airport_html(&p.altn, tr("zapasowe"), &wx, o.map(|o| &o.alternate).filter(|a| a.icao == p.altn));
    }
    let ofp_html = sb.map(ofp_html).unwrap_or_default();
    let procs = [("SID", &p.sid), ("STAR", &p.star), (tr("Podejście"), &p.app)]
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k} {}", esc(v)))
        .collect::<Vec<_>>()
        .join(" · ");
    format!(
        r#"<!doctype html>
<html lang="{lang}"><head><meta charset="utf-8"><title>{title}</title>
<style>
body{{font:13px/1.45 "Segoe UI",Arial,sans-serif;color:#111;margin:24px auto;max-width:960px;padding:0 16px}}
h1{{font-size:24px;margin:0}} h2{{font-size:17px;margin:18px 0 4px;padding-top:10px;border-top:2px solid #111}}
h3{{font-size:12px;margin:10px 0 2px;text-transform:uppercase;letter-spacing:.06em;color:#444}}
.sub,.muted{{color:#555}} .role{{font-weight:400;color:#666;font-size:13px}}
table{{border-collapse:collapse;width:100%;margin:6px 0}} th,td{{border-bottom:1px solid #ddd;padding:3px 6px;text-align:left}}
th{{font-size:11px;text-transform:uppercase;color:#555}} td.num{{text-align:right;font-variant-numeric:tabular-nums}}
pre{{font:12px/1.4 Consolas,monospace;background:#f3f3f3;padding:8px;white-space:pre-wrap;margin:2px 0}}
.grid{{display:grid;grid-template-columns:1fr 1fr;gap:20px}} .freq{{display:inline-block;margin-right:12px}}
.bar{{display:flex;gap:8px;align-items:center;margin:10px 0}} button{{font:inherit;padding:6px 14px}}
.note{{font-size:11px;color:#666;margin-top:24px}}
@media print{{.bar{{display:none}} body{{margin:0}} section{{break-inside:avoid}}}}
</style></head><body>
<div class=bar><button onclick="window.print()">{print}</button><span class=muted>{print_hint}</span></div>
<h1>{orig} → {dest}</h1>
<p class=sub>{generated} {now} UTC · {cruise_k} FL{fl:03} · TAS {tas:.0} kt · {dist:.0} NM · {ete}</p>
<p><b>{route_k}:</b> <code>{route}</code>{procs}</p>
<div class=grid>
<section><h2>{fuel_k}</h2><table>{fuel_html}</table><p class=muted>{burn_k}: {burn:.0} kg/h</p></section>
<section><h2>NOTAM</h2><p>{notam_text}</p><p><a href="https://notams.aim.faa.gov/notamSearch/nsapp.html#/">FAA NOTAM Search</a> – {ids}</p></section>
</div>
<section><h2>{legs_k}</h2><table><tr><th>{pt}</th><th>{via}</th><th>{cons}</th><th>ALT</th><th>CRS</th><th>NM</th><th>Σ NM</th><th>ETE</th></tr>{leg_html}</table></section>
{ofp_html}
{airports_html}
<p class=note>{disclaimer}</p>
<script>window.addEventListener("load",function(){{setTimeout(function(){{window.print()}},600)}})</script>
</body></html>"#,
        lang = opentakeoff_core::lang::current(),
        title = esc(&format!("Briefing {} → {}", p.origin, p.dest)),
        print = esc(tr("Drukuj / zapisz jako PDF")),
        print_hint = esc(tr("W oknie drukowania wybierz „Microsoft Print to PDF”, żeby zapisać plik PDF.")),
        orig = esc(&p.origin),
        dest = esc(&p.dest),
        generated = esc(tr("Przygotowano")),
        now = Utc::now().format("%Y-%m-%d %H:%M"),
        cruise_k = esc(tr("przelot")),
        fl = p.cruise_ft / 100,
        tas = tas,
        dist = total,
        ete = hm(total / tas),
        route_k = esc(tr("Trasa")),
        route = esc(&route_string(p)),
        procs = if procs.is_empty() { String::new() } else { format!(" · {procs}") },
        fuel_k = esc(tr("Paliwo")),
        fuel_html = fuel_html,
        burn_k = esc(tr("Spalanie")),
        burn = p.burn_kgh,
        notam_text = esc(tr("NOTAM-y sprawdź przed lotem w wyszukiwarce FAA albo w AIS kraju startu i celu.")),
        ids = esc(&[p.origin.as_str(), p.dest.as_str(), p.altn.as_str()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(" ")),
        legs_k = esc(tr("Punkty trasy")),
        pt = esc(tr("Punkt")),
        via = esc(tr("Droga")),
        cons = esc(tr("Ograniczenie")),
        leg_html = leg_html,
        airports_html = airports_html,
        ofp_html = ofp_html,
        disclaimer = esc(tr("OpenTakeoff – tylko do symulacji lotów. Nie używać do rzeczywistej nawigacji.")),
    )
}

/// Sekcje OFP SimBrief: czasy, wagi wobec limitów, paliwo, prędkości TLR, navlog i plan ATC.
fn ofp_html(sb: &Sb) -> String {
    let o = &sb.ofp;
    let unit = if sb.lbs { "lb" } else { "kg" };
    let w = |v: f64| if v > 0.0 { format!("{v:.0} {unit}") } else { "–".to_string() };
    let utc = |t: i64| if t > 0 { chrono::DateTime::from_timestamp(t, 0).map(|d| d.format("%H:%MZ").to_string()).unwrap_or_default() } else { "–".to_string() };
    let mut h = format!(
        "<section><h2>OFP SimBrief <span class=role>{} · {} · CI {}</span></h2><p class=sub>OUT {} · OFF {} · ON {} · IN {} · ETE {} · BLOCK {} · {} {:+.0} kt · ISA {:+.0}</p>",
        esc(&sb.callsign),
        esc(&sb.aircraft),
        esc(&sb.ci),
        utc(o.sched_out),
        utc(o.sched_off),
        utc(o.sched_on),
        utc(o.sched_in),
        crate::fmt::ete(sb.ete_s),
        crate::fmt::ete(o.block_s),
        esc(tr("średni wiatr")),
        o.avg_wind_comp,
        o.avg_isa
    );
    let row = |k: &str, est: f64, max: f64| {
        format!(
            "<tr><td>{}</td><td class=num>{}</td><td class=num>{}</td><td class=num>{}</td></tr>",
            esc(k),
            w(est),
            w(max),
            if max > 0.0 { format!("{:+.0}", max - est) } else { String::new() }
        )
    };
    h += &format!(
        "<div class=grid><div><h3>{}</h3><table><tr><th></th><th>{}</th><th>max</th><th>{}</th></tr>{}{}{}</table><p class=muted>OEW {} · {} {} · {} {}</p></div>",
        esc(tr("Wagi")),
        esc(tr("plan")),
        esc(tr("zapas")),
        row("ZFW", o.zfw, o.max_zfw),
        row("TOW", o.tow, o.max_tow),
        row("LDW", o.ldw, o.max_ldw),
        w(o.oew),
        esc(tr("ładunek")),
        w(o.payload),
        esc(tr("pasażerowie")),
        o.pax
    );
    let fuel = [
        (tr("Kołowanie"), o.taxi),
        (tr("Trasa"), o.trip),
        (tr("Rezerwa trasy"), o.cont),
        (tr("Zapasowe"), o.altn),
        (tr("Rezerwa końcowa"), o.reserve),
        ("ETOPS", o.etops),
        (tr("Dodatkowe"), o.extra),
        (tr("Minimum do startu"), o.min_to),
        (tr("Blok"), o.ramp),
        (tr("Na lądowaniu"), o.plan_land),
    ];
    let fuel_rows: String = fuel.iter().filter(|x| x.1 > 0.0).map(|(k, v)| format!("<tr><td>{}</td><td class=num>{}</td></tr>", esc(k), w(*v))).collect();
    h += &format!("<div><h3>{} (SimBrief)</h3><table>{}</table></div></div>", esc(tr("Paliwo")), fuel_rows);
    let tlr: Vec<String> = [
        o.tlr_to.as_ref().map(|t| format!("{} {}: {}", tr("Start"), t.rwy, simbrief::tlr_text(t, true))),
        o.tlr_ldg.as_ref().map(|t| format!("{} {}: {}", tr("Lądowanie"), t.rwy, simbrief::tlr_text(t, false))),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !tlr.is_empty() {
        h += &format!("<h3>TLR</h3><p><b>{}</b></p>", esc(&tlr.join(" · ")));
    }
    let nav: String = sb
        .fixes
        .iter()
        .map(|f| {
            format!(
                "<tr><td><b>{}</b></td><td>{}</td><td>{}</td><td class=num>{}</td><td class=num>{}</td><td class=num>{}</td><td class=num>{}</td><td class=num>{}</td><td class=num>{:.0}</td><td class=num>{}</td><td class=num>{}</td></tr>",
                esc(&f.ident),
                esc(&f.via),
                esc(&f.stage),
                fmt_alt(f.alt_ft as f64),
                esc(&f.wind),
                f.oat.map(|t| format!("{t:.0}")).unwrap_or_default(),
                f.isa_dev.map(|t| format!("{t:+.0}")).unwrap_or_default(),
                f.gs.map(|g| format!("{g:.0}")).unwrap_or_default(),
                f.cum_nm,
                f.time_s.map(|t| format!("{}:{:02}", (t as i64) / 3600, (t as i64) % 3600 / 60)).unwrap_or_default(),
                f.fuel.map(|x| format!("{x:.0}")).unwrap_or_default()
            )
        })
        .collect();
    h += &format!(
        "<h3>Navlog</h3><table><tr><th>{}</th><th>{}</th><th></th><th>ALT</th><th>{}</th><th>OAT</th><th>ISA</th><th>GS</th><th>Σ NM</th><th>{}</th><th>{} ({})</th></tr>{}</table>",
        esc(tr("Punkt")),
        esc(tr("Droga")),
        esc(tr("Wiatr")),
        esc(tr("Czas")),
        esc(tr("Paliwo")),
        unit,
        nav
    );
    if !o.fpl.is_empty() {
        h += &format!("<h3>{}</h3><pre>{}</pre>", esc(tr("Plan lotu ATC (ICAO)")), esc(&o.fpl));
    }
    h + "</section>"
}
