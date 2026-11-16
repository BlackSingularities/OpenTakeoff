//! Karta Przyrządy: PFD w stylu kabiny, ND/MFD, zegary analogowe i zegary silnika – przeniesione
//! z aplikacji na telefon. Tylko podgląd (sterowanie samolotem zostaje w telefonie).

use std::cell::RefCell;
use std::collections::HashMap;

use opentakeoff_core::aero::{airports, navaids};
use opentakeoff_core::proto::FlightState;
use slint::{ComponentHandle, SharedString};

use crate::app::{state, ui};
use crate::avionics::{self, gauges, nd, DisplayStyle, FlightData};
use crate::settings::ND_RANGES;
use crate::*;

/// Identyfikatory obszarów (App.map-event): 10 PFD, 11 ND, 12 zegary, 13 silniki, 14 PFD na karcie Na żywo.
pub const PFD: i32 = 10;
pub const ND: i32 = 11;
pub const PANEL: i32 = 12;
pub const ENGINES: i32 = 13;
pub const LIVE_PFD: i32 = 14;

thread_local! {
    static SIZES: RefCell<HashMap<i32, (f32, f32)>> = RefCell::new(HashMap::new());
}

pub fn size(id: i32, w: f32, h: f32) {
    let changed = SIZES.with(|s| {
        let mut s = s.borrow_mut();
        let old = s.insert(id, (w, h));
        old.is_none_or(|(a, b)| (a - w).abs() > 0.5 || (b - h).abs() > 0.5)
    });
    if changed {
        refresh();
    }
}

fn px(id: i32) -> Option<(u32, u32)> {
    let scale = state().borrow().scale;
    px_at(id, scale)
}

fn px_at(id: i32, scale: f32) -> Option<(u32, u32)> {
    SIZES.with(|s| s.borrow().get(&id).copied()).filter(|(w, h)| *w > 4.0 && *h > 4.0).map(|(w, h)| ((w * scale) as u32, (h * scale) as u32))
}

/// Styl wyświetlaczy: wybrany w ustawieniach albo rozpoznany z samolotu.
pub fn style_for(s: Option<&FlightState>) -> DisplayStyle {
    let key = state().borrow().s.instr_style.clone();
    style_key(s, &key)
}

fn style_key(s: Option<&FlightState>, key: &str) -> DisplayStyle {
    DisplayStyle::from_key(key).unwrap_or_else(|| s.map(DisplayStyle::detect).unwrap_or(DisplayStyle::Garmin))
}

pub(crate) fn data(s: Option<&FlightState>) -> FlightData {
    match s {
        Some(s) => {
            let tr = avionics::speed_trend(s.speed.ias_kt as f32);
            FlightData::from_state(s, tr)
        }
        None => FlightData::default(),
    }
}

/// PFD na karcie Na żywo (wołane przy pożyczonym stanie – bez sięgania do niego).
pub fn live_pfd(s: Option<&FlightState>, scale: f32, key: &str) -> Option<slint::Image> {
    let (w, h) = px_at(LIVE_PFD, scale)?;
    let d = data(s);
    Some(avionics::render_pfd(style_key(s, key), &d, w, h))
}

pub(crate) fn nd_inputs(s: &FlightState, range: f32) -> (Vec<nd::RoutePt>, Vec<nd::NdFix>) {
    let mut route = Vec::new();
    if let Some(av) = &s.avionics {
        let g = &av.gps;
        if g.active_flight_plan {
            if g.prev_lat != 0.0 || g.prev_lon != 0.0 {
                route.push(nd::RoutePt { ident: g.prev_id.clone(), lat: g.prev_lat, lon: g.prev_lon, constraint: None });
            }
            if g.next_lat != 0.0 || g.next_lon != 0.0 {
                route.push(nd::RoutePt { ident: g.next_id.clone(), lat: g.next_lat, lon: g.next_lon, constraint: None });
            }
        }
    }
    let (lat, lon) = (s.position.lat, s.position.lon);
    let mut fixes = Vec::new();
    if let Some(db) = airports::db() {
        for (_, a) in db.within(lat, lon, range as f64 * 1.1, false).into_iter().filter(|(_, a)| a.kind <= 1 || range <= 20.0).take(60) {
            fixes.push(nd::NdFix { ident: a.ident.clone(), kind: "airport", lat: a.lat, lon: a.lon });
        }
    }
    if let Some(db) = navaids::db() {
        for (_, a) in db.within(lat, lon, range as f64 * 1.1).into_iter().take(80) {
            let kind = if a.kind.is_ndb() {
                "ndb"
            } else if a.kind.is_vor() {
                "vor"
            } else {
                continue;
            };
            fixes.push(nd::NdFix { ident: a.ident.clone(), kind, lat: a.lat, lon: a.lon });
        }
    }
    (route, fixes)
}

/// Odświeża obrazy widocznej karty (Przyrządy albo PFD na karcie Na żywo).
pub fn refresh() {
    let Some(u) = ui() else { return };
    let app = u.global::<App>();
    let tab = app.get_tab();
    let snap = state().borrow().host.snapshot();
    if tab == 1 {
        let (scale, key) = {
            let st = state();
            let b = st.borrow();
            (b.scale, b.s.instr_style.clone())
        };
        if let Some(img) = live_pfd(snap.as_ref(), scale, &key) {
            app.set_pfd(img);
        }
        return;
    }
    if tab != 8 {
        return;
    }
    let set = state().borrow().s.clone();
    let style = style_for(snap.as_ref());
    let d = data(snap.as_ref());
    let layout = set.instr_layout;
    let range = ND_RANGES[set.nd_range.clamp(0, ND_RANGES.len() as i32 - 1) as usize];
    let detected = snap.as_ref().map(DisplayStyle::detect);
    app.set_instr(InstrV {
        style: DisplayStyle::from_key(&set.instr_style).map(|s| DisplayStyle::ALL.iter().position(|x| *x == s).unwrap_or(0) as i32 + 1).unwrap_or(0),
        style_name: SharedString::from(match (DisplayStyle::from_key(&set.instr_style), detected) {
            (Some(s), _) => s.label().to_string(),
            (None, Some(d)) => format!("{} (auto)", d.label()),
            (None, None) => "auto".to_string(),
        }),
        layout,
        range: SharedString::from(if range.fract() == 0.0 { format!("{} NM", range as i32) } else { format!("{range:.1} NM") }),
        rose: set.nd_rose,
        apt: set.nd_layers & 1 != 0,
        vor: set.nd_layers & 2 != 0,
        ndb: set.nd_layers & 4 != 0,
        has_data: snap.is_some(),
        title: SharedString::from(snap.as_ref().map(|s| s.aircraft.title.clone()).unwrap_or_default()),
    });
    // Kokpit: PFD + ND + silniki; Zegary: panel analogowy + silniki; Wszystko: PFD, ND, panel, silniki
    if layout != 1 {
        if let Some((w, h)) = px(PFD) {
            app.set_instr_pfd(avionics::render_pfd(style, &d, w, h));
        }
        if let Some((w, h)) = px(ND) {
            let (route, fixes) = snap.as_ref().map(|s| nd_inputs(s, range)).unwrap_or_default();
            let layers = nd::Layers { airports: set.nd_layers & 1 != 0, vor: set.nd_layers & 2 != 0, ndb: set.nd_layers & 4 != 0 };
            app.set_instr_nd(avionics::render_nd(style, &d, w, h, range, set.nd_rose, &route, &fixes, layers));
        }
    }
    if layout != 0 {
        if let Some((w, h)) = px(PANEL) {
            let kind = match style {
                DisplayStyle::Heli => gauges::Kind::Heli,
                DisplayStyle::Glider => gauges::Kind::Glider,
                _ => gauges::Kind::Cessna,
            };
            app.set_instr_panel(avionics::render_panel(kind, &d, w, h));
        }
    }
    if let Some((w, h)) = px(ENGINES) {
        app.set_instr_engines(avionics::render_engines(style, &d, w, h));
    }
}

pub fn cmd(id: &str, arg: &str) -> bool {
    let st = state();
    {
        let mut s = st.borrow_mut();
        match id {
            "instr-style" => {
                let i: usize = arg.parse().unwrap_or(0);
                s.s.instr_style = if i == 0 { String::new() } else { DisplayStyle::ALL.get(i - 1).map(|d| d.key().to_string()).unwrap_or_default() };
            }
            "instr-layout" => s.s.instr_layout = arg.parse().unwrap_or(0),
            "nd-range" => {
                let d: i32 = arg.parse().unwrap_or(0);
                s.s.nd_range = (s.s.nd_range + d).clamp(0, ND_RANGES.len() as i32 - 1);
            }
            "nd-rose" => s.s.nd_rose = !s.s.nd_rose,
            "nd-layer" => {
                let bit: u32 = match arg {
                    "apt" => 1,
                    "vor" => 2,
                    _ => 4,
                };
                s.s.nd_layers ^= bit;
            }
            _ => return false,
        }
        s.s.save();
    }
    refresh();
    true
}
