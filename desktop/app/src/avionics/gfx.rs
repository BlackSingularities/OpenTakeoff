//! Płótno do rysowania przyrządów: odpowiednik android.graphics.Canvas + Pen z aplikacji na telefon
//! (save/restore, obroty, przycinanie, linie, wielokąty, łuki, gradienty i tekst). Dzięki temu renderery
//! PFD, zegarów i ND są przeniesione z Kotlina prawie linijka w linijkę. Tekst to obrysy glifów
//! (IBM Plex Sans Condensed, B612 Mono), więc obraca się i przycina razem z resztą rysunku.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ab_glyph::{Font, FontRef, GlyphId, OutlineCurve};
use tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Mask, Paint, Path, PathBuilder, Pixmap, Point, RadialGradient, Rect, Shader, SpreadMode, Stroke,
    StrokeDash, Transform,
};

pub type Argb = u32;

pub const WHITE: Argb = 0xFFFFFFFF;
pub const BLACK: Argb = 0xFF000000;
pub const RED: Argb = 0xFFFF2A2A;
pub const AMBER: Argb = 0xFFFFA200;
pub const YELLOW: Argb = 0xFFFFE000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Cond,
    CondBold,
    Mono,
    MonoBold,
}

static PLEX: &[u8] = include_bytes!("../../fonts/plexcond_regular.ttf");
static PLEX_B: &[u8] = include_bytes!("../../fonts/plexcond_bold.ttf");
static MONO: &[u8] = include_bytes!("../../fonts/b612mono_regular.ttf");
static MONO_B: &[u8] = include_bytes!("../../fonts/b612mono_bold.ttf");

struct Fonts {
    faces: [FontRef<'static>; 4],
    glyphs: HashMap<(Face, GlyphId), Option<Path>>,
}

thread_local! {
    static FONTS: RefCell<Fonts> = RefCell::new(Fonts {
        faces: [
            FontRef::try_from_slice(PLEX).unwrap(),
            FontRef::try_from_slice(PLEX_B).unwrap(),
            FontRef::try_from_slice(MONO).unwrap(),
            FontRef::try_from_slice(MONO_B).unwrap(),
        ],
        glyphs: HashMap::new(),
    });
}

fn face_ix(f: Face) -> usize {
    match f {
        Face::Cond => 0,
        Face::CondBold => 1,
        Face::Mono => 2,
        Face::MonoBold => 3,
    }
}

/// Obrys glifu w jednostkach czcionki (oś y w dół, linia bazowa na 0).
fn glyph_path(fonts: &mut Fonts, face: Face, id: GlyphId) -> Option<Path> {
    if let Some(p) = fonts.glyphs.get(&(face, id)) {
        return p.clone();
    }
    let font = &fonts.faces[face_ix(face)];
    let path = font.outline(id).and_then(|o| {
        let mut pb = PathBuilder::new();
        let mut last: Option<(f32, f32)> = None;
        let mut start: Option<(f32, f32)> = None;
        for c in &o.curves {
            let (p0, p_end) = match c {
                OutlineCurve::Line(a, b) => (*a, *b),
                OutlineCurve::Quad(a, _, b) => (*a, *b),
                OutlineCurve::Cubic(a, _, _, b) => (*a, *b),
            };
            let a = (p0.x, -p0.y);
            if last.map(|l| (l.0 - a.0).abs() > 0.01 || (l.1 - a.1).abs() > 0.01).unwrap_or(true) {
                if start.is_some() {
                    pb.close();
                }
                pb.move_to(a.0, a.1);
                start = Some(a);
            }
            match c {
                OutlineCurve::Line(_, b) => pb.line_to(b.x, -b.y),
                OutlineCurve::Quad(_, q, b) => pb.quad_to(q.x, -q.y, b.x, -b.y),
                OutlineCurve::Cubic(_, q1, q2, b) => pb.cubic_to(q1.x, -q1.y, q2.x, -q2.y, b.x, -b.y),
            }
            last = Some((p_end.x, -p_end.y));
        }
        if start.is_some() {
            pb.close();
        }
        pb.finish()
    });
    fonts.glyphs.insert((face, id), path.clone());
    path
}

/// Szerokość napisu w pikselach przy rozmiarze `size` (jak Paint.measureText).
pub fn text_width(s: &str, size: f32, face: Face) -> f32 {
    FONTS.with(|f| {
        let f = f.borrow();
        let font = &f.faces[face_ix(face)];
        let k = size / font.units_per_em().unwrap_or(1000.0);
        let mut w = 0.0;
        let mut prev: Option<GlyphId> = None;
        for ch in s.chars() {
            let id = font.glyph_id(ch);
            if let Some(p) = prev {
                w += font.kern_unscaled(p, id);
            }
            w += font.h_advance_unscaled(id);
            prev = Some(id);
        }
        w * k
    })
}

pub fn color(c: Argb) -> Color {
    Color::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8)
}

/// Kolor z innym kanałem alfa (0–255).
pub fn alpha(c: Argb, a: u8) -> Argb {
    (c & 0x00FF_FFFF) | ((a as u32) << 24)
}

#[derive(Clone)]
struct State {
    tf: Transform,
    clip: Option<Rc<Mask>>,
}

pub struct Canvas {
    pub pm: Pixmap,
    st: State,
    stack: Vec<State>,
    /// Domyślna czcionka tekstu (wąska jak sans-serif-condensed w Androidzie).
    pub face: Face,
}

fn paint_of(c: Argb) -> Paint<'static> {
    let mut p = Paint::default();
    p.anti_alias = true;
    p.set_color(color(c));
    p
}

fn stroke_of(w: f32) -> Stroke {
    Stroke { width: w.max(0.01), line_cap: LineCap::Butt, line_join: LineJoin::Miter, miter_limit: 4.0, dash: None }
}

/// Łuk jak w Android `Path.arcTo(oval, start, sweep)`: kąty w stopniach, 0 = godzina 3, dodatnie zgodnie z zegarem.
pub fn arc_points(pb: &mut PathBuilder, cx: f32, cy: f32, rx: f32, ry: f32, start: f32, sweep: f32, move_first: bool) {
    let n = ((sweep.abs() / 4.0).ceil() as usize).max(2);
    for i in 0..=n {
        let a = (start + sweep * i as f32 / n as f32).to_radians();
        let (x, y) = (cx + rx * a.cos(), cy + ry * a.sin());
        if i == 0 && move_first {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
}

pub fn poly_path(pts: &[f32], close: bool) -> Option<Path> {
    if pts.len() < 4 {
        return None;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0], pts[1]);
    let mut i = 2;
    while i + 1 < pts.len() {
        pb.line_to(pts[i], pts[i + 1]);
        i += 2;
    }
    if close {
        pb.close();
    }
    pb.finish()
}

pub fn rect_path(l: f32, t: f32, r: f32, b: f32) -> Option<Path> {
    Rect::from_ltrb(l.min(r), t.min(b), l.max(r), b.max(t)).map(PathBuilder::from_rect)
}

pub fn round_rect_path(l: f32, t: f32, r: f32, b: f32, rad: f32) -> Option<Path> {
    let rad = rad.min((r - l) / 2.0).min((b - t) / 2.0).max(0.0);
    let mut pb = PathBuilder::new();
    pb.move_to(l + rad, t);
    pb.line_to(r - rad, t);
    arc_points(&mut pb, r - rad, t + rad, rad, rad, -90.0, 90.0, false);
    pb.line_to(r, b - rad);
    arc_points(&mut pb, r - rad, b - rad, rad, rad, 0.0, 90.0, false);
    pb.line_to(l + rad, b);
    arc_points(&mut pb, l + rad, b - rad, rad, rad, 90.0, 90.0, false);
    pb.line_to(l, t + rad);
    arc_points(&mut pb, l + rad, t + rad, rad, rad, 180.0, 90.0, false);
    pb.close();
    pb.finish()
}

pub fn circle_path(x: f32, y: f32, r: f32) -> Option<Path> {
    PathBuilder::from_circle(x, y, r.max(0.01))
}

impl Canvas {
    pub fn new(w: u32, h: u32) -> Self {
        Self { pm: Pixmap::new(w.max(8), h.max(8)).unwrap(), st: State { tf: Transform::identity(), clip: None }, stack: Vec::new(), face: Face::Cond }
    }

    pub fn from_pixmap(pm: Pixmap) -> Self {
        Self { pm, st: State { tf: Transform::identity(), clip: None }, stack: Vec::new(), face: Face::Cond }
    }

    /// Tekst z obwódką (czytelny na mapie): najpierw obrys `halo`, potem wypełnienie.
    pub fn text_halo(&mut self, s: &str, x: f32, y: f32, size: f32, c: Argb, halo: Argb, align: Align, bold: bool) {
        let face = if bold { Face::CondBold } else { Face::Cond };
        let o = (size * 0.09).max(1.0);
        for (dx, dy) in [(-o, 0.0), (o, 0.0), (0.0, -o), (0.0, o), (-o * 0.7, -o * 0.7), (o * 0.7, o * 0.7), (-o * 0.7, o * 0.7), (o * 0.7, -o * 0.7)] {
            self.text_f(s, x + dx, y + dy, size, halo, align, face);
        }
        self.text_f(s, x, y, size, c, align, face);
    }

    pub fn width(&self) -> f32 {
        self.pm.width() as f32
    }
    pub fn height(&self) -> f32 {
        self.pm.height() as f32
    }

    pub fn into_image(self) -> slint::Image {
        let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(self.pm.data(), self.pm.width(), self.pm.height());
        slint::Image::from_rgba8_premultiplied(buf)
    }

    // ---------- stan ----------

    pub fn save(&mut self) {
        self.stack.push(self.st.clone());
    }
    pub fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.st = s;
        }
    }
    pub fn translate(&mut self, x: f32, y: f32) {
        self.st.tf = self.st.tf.pre_translate(x, y);
    }
    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.st.tf = self.st.tf.pre_scale(sx, sy);
    }
    /// Obrót o `deg` stopni (zgodnie z zegarem, jak w Androidzie) wokół (cx, cy).
    pub fn rotate(&mut self, deg: f32, cx: f32, cy: f32) {
        self.st.tf = self.st.tf.pre_concat(Transform::from_rotate_at(deg, cx, cy));
    }
    pub fn transform(&self) -> Transform {
        self.st.tf
    }

    pub fn clip_path(&mut self, p: &Path) {
        let tf = self.st.tf;
        let m = match &self.st.clip {
            Some(m) => {
                let mut m = (**m).clone();
                m.intersect_path(p, FillRule::Winding, true, tf);
                m
            }
            None => {
                let Some(mut m) = Mask::new(self.pm.width(), self.pm.height()) else { return };
                m.fill_path(p, FillRule::Winding, true, tf);
                m
            }
        };
        self.st.clip = Some(Rc::new(m));
    }
    pub fn clip_rect(&mut self, l: f32, t: f32, r: f32, b: f32) {
        match rect_path(l, t, r, b) {
            Some(p) => self.clip_path(&p),
            None => {
                if let Some(m) = Mask::new(self.pm.width(), self.pm.height()) {
                    self.st.clip = Some(Rc::new(m));
                }
            }
        }
    }
    pub fn clip_circle(&mut self, x: f32, y: f32, r: f32) {
        if let Some(p) = circle_path(x, y, r) {
            self.clip_path(&p);
        }
    }

    // ---------- rysowanie ścieżek ----------

    pub fn fill_path(&mut self, p: &Path, c: Argb) {
        let paint = paint_of(c);
        let clip = self.st.clip.clone();
        self.pm.fill_path(p, &paint, FillRule::Winding, self.st.tf, clip.as_deref());
    }
    pub fn fill_path_eo(&mut self, p: &Path, c: Argb) {
        let paint = paint_of(c);
        let clip = self.st.clip.clone();
        self.pm.fill_path(p, &paint, FillRule::EvenOdd, self.st.tf, clip.as_deref());
    }
    pub fn fill_path_paint(&mut self, p: &Path, paint: &Paint) {
        let clip = self.st.clip.clone();
        self.pm.fill_path(p, paint, FillRule::Winding, self.st.tf, clip.as_deref());
    }
    pub fn stroke_path(&mut self, p: &Path, c: Argb, w: f32) {
        let paint = paint_of(c);
        let clip = self.st.clip.clone();
        self.pm.stroke_path(p, &paint, &stroke_of(w), self.st.tf, clip.as_deref());
    }
    pub fn stroke_path_round(&mut self, p: &Path, c: Argb, w: f32) {
        let paint = paint_of(c);
        let clip = self.st.clip.clone();
        let s = Stroke { line_cap: LineCap::Round, line_join: LineJoin::Round, ..stroke_of(w) };
        self.pm.stroke_path(p, &paint, &s, self.st.tf, clip.as_deref());
    }
    pub fn stroke_path_dash(&mut self, p: &Path, c: Argb, w: f32, on: f32, off: f32) {
        let paint = paint_of(c);
        let clip = self.st.clip.clone();
        let s = Stroke { dash: StrokeDash::new(vec![on, off], 0.0), ..stroke_of(w) };
        self.pm.stroke_path(p, &paint, &s, self.st.tf, clip.as_deref());
    }

    // ---------- Pen ----------

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, c: Argb, w: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(x1, y1);
        pb.line_to(x2, y2);
        if let Some(p) = pb.finish() {
            self.stroke_path(&p, c, w);
        }
    }
    pub fn line_round(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, c: Argb, w: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(x1, y1);
        pb.line_to(x2, y2);
        if let Some(p) = pb.finish() {
            self.stroke_path_round(&p, c, w);
        }
    }
    pub fn dash_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, c: Argb, w: f32, on: f32, off: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(x1, y1);
        pb.line_to(x2, y2);
        if let Some(p) = pb.finish() {
            self.stroke_path_dash(&p, c, w, on, off);
        }
    }
    pub fn rect(&mut self, l: f32, t: f32, r: f32, b: f32, c: Argb) {
        if let Some(p) = rect_path(l, t, r, b) {
            self.fill_path(&p, c);
        }
    }
    pub fn frame(&mut self, l: f32, t: f32, r: f32, b: f32, c: Argb, w: f32) {
        if let Some(p) = rect_path(l, t, r, b) {
            self.stroke_path(&p, c, w);
        }
    }
    pub fn round_rect(&mut self, l: f32, t: f32, r: f32, b: f32, rad: f32, fill: Argb, stroke: Argb, w: f32) {
        if let Some(p) = round_rect_path(l, t, r, b, rad) {
            if fill != 0 {
                self.fill_path(&p, fill);
            }
            if w > 0.0 {
                self.stroke_path(&p, stroke, w);
            }
        }
    }
    /// Koło: wypełnione (w <= 0) albo okrąg o grubości w.
    pub fn circle(&mut self, x: f32, y: f32, r: f32, c: Argb, w: f32) {
        if let Some(p) = circle_path(x, y, r) {
            if w <= 0.0 {
                self.fill_path(&p, c);
            } else {
                self.stroke_path(&p, c, w);
            }
        }
    }
    pub fn dash_circle(&mut self, x: f32, y: f32, r: f32, c: Argb, w: f32, on: f32, off: f32) {
        if let Some(p) = circle_path(x, y, r) {
            self.stroke_path_dash(&p, c, w, on, off);
        }
    }
    /// Wielokąt: wypełnienie (fill != 0) i/lub obrys (w > 0).
    pub fn poly(&mut self, fill: Argb, stroke: Argb, w: f32, pts: &[f32]) {
        if let Some(p) = poly_path(pts, true) {
            if fill != 0 {
                self.fill_path(&p, fill);
            }
            if w > 0.0 {
                self.stroke_path(&p, stroke, w);
            }
        }
    }
    /// Łamana otwarta.
    pub fn polyline(&mut self, c: Argb, w: f32, pts: &[f32]) {
        if let Some(p) = poly_path(pts, false) {
            self.stroke_path(&p, c, w);
        }
    }
    /// Łuk okręgu w prostokącie (l, t, r, b) jak Canvas.drawArc(useCenter = false), obrys.
    pub fn arc(&mut self, l: f32, t: f32, r: f32, b: f32, start: f32, sweep: f32, c: Argb, w: f32) {
        let mut pb = PathBuilder::new();
        arc_points(&mut pb, (l + r) / 2.0, (t + b) / 2.0, (r - l) / 2.0, (b - t) / 2.0, start, sweep, true);
        if let Some(p) = pb.finish() {
            self.stroke_path(&p, c, w);
        }
    }
    pub fn dash_arc(&mut self, l: f32, t: f32, r: f32, b: f32, start: f32, sweep: f32, c: Argb, w: f32, on: f32, off: f32) {
        let mut pb = PathBuilder::new();
        arc_points(&mut pb, (l + r) / 2.0, (t + b) / 2.0, (r - l) / 2.0, (b - t) / 2.0, start, sweep, true);
        if let Some(p) = pb.finish() {
            self.stroke_path_dash(&p, c, w, on, off);
        }
    }
    /// Wycinek pierścienia (kolorowe łuki zegarów): kąty jak w drawArc.
    pub fn arc_band(&mut self, cx: f32, cy: f32, r: f32, width: f32, start: f32, sweep: f32, c: Argb) {
        let mut pb = PathBuilder::new();
        arc_points(&mut pb, cx, cy, r, r, start, sweep, true);
        if let Some(p) = pb.finish() {
            self.stroke_path(&p, c, width);
        }
    }
    /// Odcinek promieniowy wokół (cx, cy) pod kątem a (0 = do góry, + w prawo).
    pub fn radial(&mut self, cx: f32, cy: f32, a: f32, r1: f32, r2: f32, c: Argb, w: f32) {
        let rad = a.to_radians();
        let (s, k) = (rad.sin(), rad.cos());
        self.line(cx + r1 * s, cy - r1 * k, cx + r2 * s, cy - r2 * k, c, w);
    }
    /// Prostokąt z pionowym gradientem (y0 → c0, y1 → c1).
    pub fn rect_vgrad(&mut self, l: f32, t: f32, r: f32, b: f32, y0: f32, c0: Argb, y1: f32, c1: Argb) {
        let Some(p) = rect_path(l, t, r, b) else { return };
        let Some(sh) = LinearGradient::new(Point::from_xy(0.0, y0), Point::from_xy(0.0, y1), vec![GradientStop::new(0.0, color(c0)), GradientStop::new(1.0, color(c1))], SpreadMode::Pad, Transform::identity()) else {
            self.fill_path(&p, c0);
            return;
        };
        let mut paint = Paint::default();
        paint.anti_alias = true;
        paint.shader = sh;
        self.fill_path_paint(&p, &paint);
    }
    /// Wypełnienie ścieżki gradientem promieniowym.
    pub fn fill_radial(&mut self, p: &Path, cx: f32, cy: f32, r: f32, c0: Argb, c1: Argb) {
        let Some(sh) = RadialGradient::new(Point::from_xy(cx, cy), Point::from_xy(cx, cy), r.max(1.0), vec![GradientStop::new(0.0, color(c0)), GradientStop::new(1.0, color(c1))], SpreadMode::Pad, Transform::identity()) else {
            self.fill_path(p, c0);
            return;
        };
        let mut paint = Paint::default();
        paint.anti_alias = true;
        paint.shader = sh;
        self.fill_path_paint(p, &paint);
    }
    pub fn fill_shader(&mut self, p: &Path, sh: Shader<'static>) {
        let mut paint = Paint::default();
        paint.anti_alias = true;
        paint.shader = sh;
        self.fill_path_paint(p, &paint);
    }

    // ---------- tekst ----------

    /// Tekst wyśrodkowany pionowo względem y, wyrównany w poziomie wg `align`.
    pub fn text_f(&mut self, s: &str, x: f32, y: f32, size: f32, c: Argb, align: Align, face: Face) {
        if s.is_empty() || size <= 0.5 {
            return;
        }
        let w = text_width(s, size, face);
        let x0 = match align {
            Align::Left => x,
            Align::Center => x - w / 2.0,
            Align::Right => x - w,
        };
        let base = y + size * 0.36;
        let paint = paint_of(c);
        let clip = self.st.clip.clone();
        let tf0 = self.st.tf;
        FONTS.with(|f| {
            let mut f = f.borrow_mut();
            let upm = f.faces[face_ix(face)].units_per_em().unwrap_or(1000.0);
            let k = size / upm;
            let mut pen = 0.0f32;
            let mut prev: Option<GlyphId> = None;
            for ch in s.chars() {
                let (id, kern, adv) = {
                    let font = &f.faces[face_ix(face)];
                    let id = font.glyph_id(ch);
                    (id, prev.map(|p| font.kern_unscaled(p, id)).unwrap_or(0.0), font.h_advance_unscaled(id))
                };
                pen += kern;
                if let Some(p) = glyph_path(&mut f, face, id) {
                    let tf = tf0.pre_translate(x0 + pen * k, base).pre_scale(k, k);
                    self.pm.fill_path(&p, &paint, FillRule::Winding, tf, clip.as_deref());
                }
                pen += adv;
                prev = Some(id);
            }
        });
    }
    pub fn text_a(&mut self, s: &str, x: f32, y: f32, size: f32, c: Argb, align: Align) {
        let f = self.face;
        self.text_f(s, x, y, size, c, align, f);
    }
    pub fn text(&mut self, s: &str, x: f32, y: f32, size: f32, c: Argb) {
        self.text_a(s, x, y, size, c, Align::Center);
    }
    pub fn text_l(&mut self, s: &str, x: f32, y: f32, size: f32, c: Argb) {
        self.text_a(s, x, y, size, c, Align::Left);
    }
    pub fn text_r(&mut self, s: &str, x: f32, y: f32, size: f32, c: Argb) {
        self.text_a(s, x, y, size, c, Align::Right);
    }
    pub fn text_b(&mut self, s: &str, x: f32, y: f32, size: f32, c: Argb) {
        let f = if self.face == Face::Mono { Face::MonoBold } else { Face::CondBold };
        self.text_f(s, x, y, size, c, Align::Center, f);
    }
    pub fn text_ab(&mut self, s: &str, x: f32, y: f32, size: f32, c: Argb, align: Align) {
        let f = if self.face == Face::Mono { Face::MonoBold } else { Face::CondBold };
        self.text_f(s, x, y, size, c, align, f);
    }
    pub fn tw(&self, s: &str, size: f32) -> f32 {
        text_width(s, size, self.face)
    }

    /// Bęben z cyframi przewijanymi pionowo (jak w prawdziwych PFD).
    /// `value` – pozycja w krokach bębna, `label(k)` – etykieta k-tego kroku.
    pub fn drum(&mut self, l: f32, t: f32, r: f32, b: f32, value: f32, spacing: f32, size: f32, c: Argb, x: f32, align: Align, label: impl Fn(i32) -> String) {
        self.save();
        self.clip_rect(l, t, r, b);
        let cy = (t + b) / 2.0;
        let base = value.floor() as i32;
        for k in base - 3..=base + 3 {
            let y = cy + (value - k as f32) * spacing;
            if y < t - size || y > b + size {
                continue;
            }
            self.text_a(&label(k), x, y, size, c, align);
        }
        self.restore();
    }
}

// ---------- pomocnicze (jak w Gfx.kt / FlightData.kt) ----------

pub fn floor_to(v: f32, step: i32) -> i32 {
    ((v / step as f32).floor() * step as f32) as i32
}

pub fn imod(a: i32, m: i32) -> i32 {
    ((a % m) + m) % m
}

pub fn wrap360(a: f32) -> f32 {
    let x = a % 360.0;
    if x < 0.0 {
        x + 360.0
    } else {
        x
    }
}

pub fn wrap180(a: f32) -> f32 {
    let x = wrap360(a);
    if x > 180.0 {
        x - 360.0
    } else {
        x
    }
}

pub fn ang_diff(a: f32, b: f32) -> f32 {
    wrap180(a - b)
}

pub fn roundi(v: f32) -> i32 {
    v.round() as i32
}

/// Odpowiednik Kotlinowego `Float.sign` (0 dla zera).
pub fn sign(v: f32) -> f32 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        0.0
    }
}
