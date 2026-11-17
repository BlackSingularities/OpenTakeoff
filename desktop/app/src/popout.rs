//! Przyrządy w osobnych oknach (np. na drugi monitor): PFD, ND, silniki, systemy i zegary analogowe.
//! Zwykłe okna Windows – przesuwa się je i zmienia rozmiar jak każde inne; obraz rysowany w rozmiarze
//! okna. Otwarte okna, ich miejsce i rozmiar pamiętają ustawienia (`popouts`), więc wracają po
//! ponownym uruchomieniu programu. Opcjonalnie zawsze na wierzchu.

use std::cell::RefCell;
use std::time::Instant;

use opentakeoff_core::lang::tr;
use slint::{ComponentHandle, PhysicalPosition, PhysicalSize};

use crate::app::{state, ui};
use crate::avionics::{self, gauges, nd, DisplayStyle};
use crate::settings::{PopoutCfg, ND_RANGES};
use crate::*;

/// Rodzaje okien: 0 PFD, 1 ND, 2 silniki, 3 systemy, 4 zegary analogowe.
pub const KINDS: i32 = 5;

pub fn name(kind: i32) -> &'static str {
    match kind {
        0 => "PFD",
        1 => "ND",
        2 => tr("Silniki"),
        3 => tr("Systemy"),
        _ => tr("Zegary"),
    }
}

struct Pop {
    kind: i32,
    win: InstrWindow,
    geom: (i32, i32, u32, u32),
}

struct St {
    pops: Vec<Pop>,
    saved: Instant,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St { pops: Vec::new(), saved: Instant::now() });
}

fn geometry(w: &InstrWindow) -> (i32, i32, u32, u32) {
    let p = w.window().position();
    let s = w.window().size();
    (p.x, p.y, s.width, s.height)
}

/// Stan okien do interfejsu: które są otwarte i „zawsze na wierzchu”.
fn publish() {
    let Some(u) = ui() else { return };
    let open: Vec<bool> = ST.with(|s| {
        let s = s.borrow();
        (0..KINDS).map(|k| s.pops.iter().any(|p| p.kind == k)).collect()
    });
    let app = u.global::<App>();
    app.set_popouts(slint::ModelRc::new(slint::VecModel::from(open)));
    app.set_popout_top(state().borrow().s.popout_top);
}

/// Zapis listy otwartych okien z ich położeniem.
fn remember() {
    let list: Vec<PopoutCfg> = ST.with(|s| s.borrow().pops.iter().map(|p| PopoutCfg { kind: p.kind, x: p.geom.0, y: p.geom.1, w: p.geom.2, h: p.geom.3 }).collect());
    let st = state();
    let mut b = st.borrow_mut();
    b.s.popouts = list;
    b.s.save();
}

fn open(kind: i32, cfg: Option<&PopoutCfg>) {
    if let Some(w) = ST.with(|s| s.borrow().pops.iter().find(|p| p.kind == kind).map(|p| p.win.clone_strong())) {
        let _ = w.show();
        return;
    }
    let Ok(w) = InstrWindow::new() else { return };
    w.set_title_text(format!("OpenTakeoff – {}", name(kind)).into());
    w.set_on_top(state().borrow().s.popout_top);
    match cfg {
        // Miejsce sprzed zamknięcia (rozsądne wartości – monitor mógł zniknąć)
        Some(c) if c.w >= 120 && c.h >= 120 && c.x > -20_000 && c.y > -20_000 => {
            w.window().set_position(PhysicalPosition::new(c.x, c.y));
            w.window().set_size(PhysicalSize::new(c.w, c.h));
        }
        _ => {
            let (lw, lh) = match kind {
                2 | 3 => (520.0, 420.0),
                4 => (760.0, 460.0),
                _ => (520.0, 520.0),
            };
            w.window().set_size(slint::LogicalSize::new(lw, lh));
        }
    }
    {
        let k = kind;
        w.window().on_close_requested(move || {
            ST.with(|s| s.borrow_mut().pops.retain(|p| p.kind != k));
            remember();
            publish();
            slint::CloseRequestResponse::HideWindow
        });
    }
    let _ = w.show();
    let geom = geometry(&w);
    ST.with(|s| s.borrow_mut().pops.push(Pop { kind, win: w, geom }));
    render_all();
    remember();
    publish();
}

fn close(kind: i32) {
    let w = ST.with(|s| {
        let mut s = s.borrow_mut();
        let i = s.pops.iter().position(|p| p.kind == kind)?;
        Some(s.pops.remove(i).win)
    });
    if let Some(w) = w {
        let _ = w.hide();
    }
    remember();
    publish();
}

/// Start programu: okna otwarte przy poprzednim zamknięciu.
pub fn restore() {
    let list = state().borrow().s.popouts.clone();
    for c in &list {
        if (0..KINDS).contains(&c.kind) {
            open(c.kind, Some(c));
        }
    }
    publish();
}

/// Zamknięcie programu: chowa okna bez zmiany zapamiętanej listy.
pub fn close_all() {
    let wins: Vec<InstrWindow> = ST.with(|s| s.borrow().pops.iter().map(|p| p.win.clone_strong()).collect());
    for w in wins {
        let _ = w.hide();
    }
}

/// Co 100 ms: obraz w każdym otwartym oknie; zmiana położenia zapisuje się najwyżej co 2 s.
pub fn tick() {
    if ST.with(|s| s.borrow().pops.is_empty()) {
        return;
    }
    render_all();
    let moved = ST.with(|s| {
        let mut s = s.borrow_mut();
        let mut moved = false;
        for p in s.pops.iter_mut() {
            let g = geometry(&p.win);
            if g != p.geom {
                p.geom = g;
                moved = true;
            }
        }
        if moved && s.saved.elapsed().as_secs() >= 2 {
            s.saved = Instant::now();
            true
        } else {
            false
        }
    });
    if moved {
        remember();
    }
}

fn render_all() {
    let wins: Vec<(i32, InstrWindow)> = ST.with(|s| s.borrow().pops.iter().map(|p| (p.kind, p.win.clone_strong())).collect());
    if wins.is_empty() {
        return;
    }
    let snap = state().borrow().host.snapshot();
    let style = crate::instruments::style_for(snap.as_ref());
    let d = crate::instruments::data(snap.as_ref());
    let set = state().borrow().s.clone();
    for (kind, w) in wins {
        let size = w.window().size();
        let (pw, ph) = (size.width.max(1), size.height.max(1));
        if pw < 20 || ph < 20 {
            continue;
        }
        let img = match kind {
            0 => avionics::render_pfd(style, &d, pw, ph),
            1 => {
                let range = ND_RANGES[set.nd_range.clamp(0, ND_RANGES.len() as i32 - 1) as usize];
                let (route, fixes) = snap.as_ref().map(|s| crate::instruments::nd_inputs(s, range)).unwrap_or_default();
                let layers = nd::Layers { airports: set.nd_layers & 1 != 0, vor: set.nd_layers & 2 != 0, ndb: set.nd_layers & 4 != 0 };
                avionics::render_nd(style, &d, pw, ph, range, set.nd_rose, &route, &fixes, layers)
            }
            2 => avionics::render_engines(style, &d, pw, ph),
            3 => avionics::render_sd(style, &d, pw, ph),
            _ => {
                let kind = match style {
                    DisplayStyle::Heli => gauges::Kind::Heli,
                    DisplayStyle::Glider => gauges::Kind::Glider,
                    _ => gauges::Kind::Cessna,
                };
                avionics::render_panel(kind, &d, pw, ph)
            }
        };
        w.set_image(img);
    }
}

pub fn cmd(id: &str, arg: &str) -> bool {
    match id {
        // „popout” 0–4: otwiera albo zamyka okno
        "popout" => {
            let kind: i32 = arg.parse().unwrap_or(-1);
            if !(0..KINDS).contains(&kind) {
                return true;
            }
            if ST.with(|s| s.borrow().pops.iter().any(|p| p.kind == kind)) {
                close(kind);
            } else {
                open(kind, None);
            }
        }
        "popout-top" => {
            let on = {
                let st = state();
                let mut b = st.borrow_mut();
                b.s.popout_top = !b.s.popout_top;
                b.s.save();
                b.s.popout_top
            };
            let wins: Vec<InstrWindow> = ST.with(|s| s.borrow().pops.iter().map(|p| p.win.clone_strong()).collect());
            for w in wins {
                w.set_on_top(on);
            }
            publish();
        }
        _ => return false,
    }
    true
}
