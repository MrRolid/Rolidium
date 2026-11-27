// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Text as outlines. A line of text is laid out with one of the bundled fonts (Liberation Sans and Serif, Hack, DejaVu Sans: see `fonts/LICENSES.txt`) and
//! comes out as closed polylines in millimetres: the outlines of the letters and the holes inside them (the inside of an "O", "e", "ě"). The outlines can
//! then be grouped into shapes (an outer outline with its holes) for extruding.

use glam::DVec2;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, MetadataProvider};

#[derive(Debug, thiserror::Error)]
pub enum TextError {
    #[error("the font {0} is not known")]
    UnknownFont(String),
    #[error("the font file cannot be read")]
    BadFont,
    #[error("the text is empty or has no letters that make a shape")]
    Empty,
    #[error("the height of the letters must be a positive number")]
    BadHeight,
}

/// A bundled family of four faces.
pub struct Family {
    pub id: &'static str,
    pub name: &'static str,
    /// What it is good for (shown next to the name).
    pub note: &'static str,
    regular: &'static [u8],
    bold: &'static [u8],
    italic: &'static [u8],
    bold_italic: &'static [u8],
}

macro_rules! face {
    ($f:literal) => {
        include_bytes!(concat!("../fonts/", $f, ".ttf"))
    };
}

pub static FAMILIES: [Family; 4] = [
    Family {
        id: "sans",
        name: "Liberation Sans",
        note: "sans serif",
        regular: face!("LiberationSans-Regular"),
        bold: face!("LiberationSans-Bold"),
        italic: face!("LiberationSans-Italic"),
        bold_italic: face!("LiberationSans-BoldItalic"),
    },
    Family {
        id: "serif",
        name: "Liberation Serif",
        note: "serif",
        regular: face!("LiberationSerif-Regular"),
        bold: face!("LiberationSerif-Bold"),
        italic: face!("LiberationSerif-Italic"),
        bold_italic: face!("LiberationSerif-BoldItalic"),
    },
    Family {
        id: "mono",
        name: "Hack",
        note: "technical, one width",
        regular: face!("Hack-Regular"),
        bold: face!("Hack-Bold"),
        italic: face!("Hack-Italic"),
        bold_italic: face!("Hack-BoldItalic"),
    },
    Family {
        id: "wide",
        name: "DejaVu Sans",
        note: "wide and sturdy, good for 3D printing",
        regular: face!("DejaVuSans-Regular"),
        bold: face!("DejaVuSans-Bold"),
        italic: face!("DejaVuSans-Italic"),
        bold_italic: face!("DejaVuSans-BoldItalic"),
    },
];

pub fn family(id: &str) -> Result<&'static Family, TextError> {
    FAMILIES
        .iter()
        .find(|f| f.id == id)
        .ok_or_else(|| TextError::UnknownFont(id.to_string()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Centre,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    /// The height of the capital letters, mm.
    pub height: f64,
    pub align: Align,
    /// Added after every letter, mm.
    pub spacing: f64,
    /// The distance of lines as a multiple of the height of the capitals (1.5: a comfortable line).
    pub line_spacing: f64,
    /// Turns the text about its origin, degrees.
    pub rotation: f64,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            family: "sans".into(),
            bold: false,
            italic: false,
            height: 5.0,
            align: Align::Left,
            spacing: 0.0,
            line_spacing: 1.6,
            rotation: 0.0,
        }
    }
}

/// The laid out text.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    /// Closed polylines (the last point is not repeated), in mm, the origin at the start (left), the middle or the end (right) of the baseline of the first line.
    pub contours: Vec<Vec<DVec2>>,
    /// Characters the font has no letter for (they are left out).
    pub missing: Vec<char>,
    pub min: DVec2,
    pub max: DVec2,
}

/// A shape of the text: an outer outline and the holes inside it.
#[derive(Debug, Clone)]
pub struct Shape {
    pub outer: Vec<DVec2>,
    pub holes: Vec<Vec<DVec2>>,
}

struct Pen {
    scale: f64,
    dx: f64,
    dy: f64,
    tol_units: f64,
    done: Vec<Vec<DVec2>>,
    cur: Vec<DVec2>,
    last: DVec2,
}

impl Pen {
    fn raw(&self, x: f32, y: f32) -> DVec2 {
        DVec2::new(x as f64, y as f64)
    }
    fn flush(&mut self) {
        if self.cur.len() > 1 && self.cur.first() == self.cur.last() {
            self.cur.pop();
        }
        if self.cur.len() >= 3 {
            let c = std::mem::take(&mut self.cur);
            self.done.push(c);
        } else {
            self.cur.clear();
        }
    }
    fn push(&mut self, p: DVec2) {
        if self.cur.last().is_none_or(|l| l.distance_squared(p) > 1e-18) {
            self.cur.push(DVec2::new(self.dx + p.x * self.scale, self.dy + p.y * self.scale));
        }
        self.last = p;
    }
}

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush();
        let p = self.raw(x, y);
        self.push(p);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.raw(x, y);
        self.push(p);
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (p0, c, p1) = (self.last, self.raw(cx, cy), self.raw(x, y));
        let dev = (p0 - 2.0 * c + p1).length() / 4.0;
        let n = ((dev / self.tol_units).sqrt().ceil() as usize).clamp(1, 64);
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            self.push(p0 * (u * u) + c * (2.0 * u * t) + p1 * (t * t));
        }
    }
    fn curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        let (p0, c1, c2, p1) = (self.last, self.raw(c1x, c1y), self.raw(c2x, c2y), self.raw(x, y));
        let dev = ((p0 - 2.0 * c1 + c2).length().max((c1 - 2.0 * c2 + p1).length())) * 0.75;
        let n = ((dev / self.tol_units).sqrt().ceil() as usize).clamp(1, 64);
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            self.push(p0 * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + p1 * (t * t * t));
        }
    }
    fn close(&mut self) {
        self.flush();
    }
}

/// How exactly the curves are followed, mm.
pub const TOLERANCE: f64 = 0.01;

/// Lays the text out. Lines are separated by `\n`.
pub fn layout(text: &str, style: &Style) -> Result<Layout, TextError> {
    if !(style.height.is_finite() && style.height > 0.0) {
        return Err(TextError::BadHeight);
    }
    let fam = family(&style.family)?;
    let data = match (style.bold, style.italic) {
        (false, false) => fam.regular,
        (true, false) => fam.bold,
        (false, true) => fam.italic,
        (true, true) => fam.bold_italic,
    };
    let font = FontRef::new(data).map_err(|_| TextError::BadFont)?;
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    let upem = metrics.units_per_em as f64;
    let cap = metrics.cap_height.map(|c| c as f64).filter(|c| *c > 1.0).unwrap_or(upem * 0.7);
    let scale = style.height / cap;
    let charmap = font.charmap();
    let gm = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let outlines = font.outline_glyphs();
    let mut out = Layout::default();
    let line_height = style.height * style.line_spacing.max(0.2);
    let rot = style.rotation.to_radians();
    let (sr, cr) = rot.sin_cos();
    // first the lines as outlines with their own widths, then the alignment
    struct Line {
        contours: Vec<Vec<DVec2>>,
        width: f64,
    }
    let mut lines: Vec<Line> = Vec::new();
    for raw in text.split('\n') {
        let mut x = 0.0;
        let mut contours = Vec::new();
        let mut last_visible = 0.0;
        for ch in raw.chars() {
            if ch == '\r' {
                continue;
            }
            let Some(gid) = charmap.map(ch) else {
                if !out.missing.contains(&ch) {
                    out.missing.push(ch);
                }
                continue;
            };
            let adv = gm.advance_width(gid).unwrap_or(0.0) as f64 * scale;
            if let Some(g) = outlines.get(gid) {
                let mut pen = Pen {
                    scale,
                    dx: x,
                    dy: 0.0,
                    tol_units: TOLERANCE / scale,
                    done: Vec::new(),
                    cur: Vec::new(),
                    last: DVec2::ZERO,
                };
                if g.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen).is_ok() {
                    pen.flush();
                    contours.extend(pen.done);
                }
            }
            x += adv + style.spacing;
            if !ch.is_whitespace() {
                last_visible = x - style.spacing;
            }
        }
        lines.push(Line {
            contours,
            width: last_visible,
        });
    }
    let mut lo = DVec2::splat(f64::MAX);
    let mut hi = DVec2::splat(f64::MIN);
    for (i, l) in lines.iter().enumerate() {
        let shift = match style.align {
            Align::Left => 0.0,
            Align::Centre => -l.width / 2.0,
            Align::Right => -l.width,
        };
        for c in &l.contours {
            let moved: Vec<DVec2> = c
                .iter()
                .map(|p| {
                    let q = DVec2::new(p.x + shift, p.y - line_height * i as f64);
                    DVec2::new(q.x * cr - q.y * sr, q.x * sr + q.y * cr)
                })
                .collect();
            for p in &moved {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
            out.contours.push(moved);
        }
    }
    if out.contours.is_empty() {
        return Err(TextError::Empty);
    }
    out.min = lo;
    out.max = hi;
    Ok(out)
}

fn area(c: &[DVec2]) -> f64 {
    let mut a = 0.0;
    for i in 0..c.len() {
        let (p, q) = (c[i], c[(i + 1) % c.len()]);
        a += p.x * q.y - q.x * p.y;
    }
    a / 2.0
}

fn inside(poly: &[DVec2], p: DVec2) -> bool {
    let mut c = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
        j = i;
    }
    c
}

/// A point that certainly lies on the outline `c` and is not a vertex shared with another outline: the middle of its longest side.
fn probe_point(c: &[DVec2]) -> DVec2 {
    let mut best = (0.0, c[0]);
    for i in 0..c.len() {
        let (p, q) = (c[i], c[(i + 1) % c.len()]);
        let l = p.distance_squared(q);
        if l > best.0 {
            best = (l, (p + q) / 2.0);
        }
    }
    best.1
}

/// Groups outlines into shapes: an outline inside no other is an outer outline; one inside an odd number is a hole of the smallest outline round it.
/// Outer outlines come out counter-clockwise, holes clockwise.
pub fn shapes(contours: &[Vec<DVec2>]) -> Vec<Shape> {
    let n = contours.len();
    let probes: Vec<DVec2> = contours.iter().map(|c| probe_point(c)).collect();
    let areas: Vec<f64> = contours.iter().map(|c| area(c).abs()).collect();
    // for each outline the outlines round it
    let mut around: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        for j in 0..n {
            if i != j && areas[j] > areas[i] && inside(&contours[j], probes[i]) {
                around[i].push(j);
            }
        }
    }
    let mut shapes: Vec<Shape> = Vec::new();
    let mut index_of: Vec<Option<usize>> = vec![None; n];
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|i| around[*i].len());
    for i in &order {
        let depth = around[*i].len();
        if depth % 2 == 0 {
            let mut c = contours[*i].clone();
            if area(&c) < 0.0 {
                c.reverse();
            }
            index_of[*i] = Some(shapes.len());
            shapes.push(Shape {
                outer: c,
                holes: Vec::new(),
            });
        }
    }
    for i in &order {
        let depth = around[*i].len();
        if depth % 2 == 1 {
            // the nearest outline round it (the smallest) is its outer outline
            let parent = around[*i]
                .iter()
                .copied()
                .filter(|j| index_of[*j].is_some())
                .min_by(|a, b| areas[*a].total_cmp(&areas[*b]));
            if let Some(si) = parent.and_then(|p| index_of[p]) {
                let mut c = contours[*i].clone();
                if area(&c) > 0.0 {
                    c.reverse();
                }
                shapes[si].holes.push(c);
            }
        }
    }
    shapes
}

/// Lays the text out and groups it into shapes.
pub fn text_shapes(text: &str, style: &Style) -> Result<(Vec<Shape>, Layout), TextError> {
    let l = layout(text, style)?;
    let s = shapes(&l.contours);
    if s.is_empty() {
        return Err(TextError::Empty);
    }
    Ok((s, l))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sty() -> Style {
        Style {
            height: 10.0,
            ..Style::default()
        }
    }

    #[test]
    fn the_letter_o_is_an_outline_with_a_hole() {
        let (s, _) = text_shapes("O", &sty()).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].holes.len(), 1);
    }

    #[test]
    fn the_letter_i_has_a_dot_and_a_stem() {
        let (s, _) = text_shapes("i", &sty()).unwrap();
        assert_eq!(s.len(), 2);
        assert!(s.iter().all(|x| x.holes.is_empty()));
    }

    #[test]
    fn czech_letters_are_there_in_every_bundled_font() {
        for f in FAMILIES.iter() {
            for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                let st = Style {
                    family: f.id.into(),
                    bold,
                    italic,
                    ..sty()
                };
                let l = layout("ěščřžýáíéůúďťňĚŠČŘŽÝÁÍÉŮÚĎŤŇ ó ü ß", &st).unwrap();
                assert!(l.missing.is_empty(), "{} {bold} {italic}: {:?}", f.name, l.missing);
            }
        }
    }

    #[test]
    fn the_capitals_are_as_high_as_asked() {
        let l = layout("H", &sty()).unwrap();
        assert!((l.max.y - l.min.y - 10.0).abs() < 0.2, "{:?} {:?}", l.min, l.max);
        assert!(l.min.y.abs() < 0.1);
    }

    #[test]
    fn alignment_moves_the_text() {
        let a = layout("Rolidium", &Style { align: Align::Left, ..sty() }).unwrap();
        let c = layout("Rolidium", &Style { align: Align::Centre, ..sty() }).unwrap();
        let r = layout("Rolidium", &Style { align: Align::Right, ..sty() }).unwrap();
        let w = a.max.x - a.min.x;
        assert!(a.min.x >= -0.5 && a.min.x < 1.5);
        assert!((c.min.x + c.max.x).abs() < 1.5, "{} {}", c.min.x, c.max.x);
        assert!(r.max.x.abs() < 1.5 && r.max.x - r.min.x > w - 0.1);
    }

    #[test]
    fn a_second_line_goes_below_and_the_spacing_widens() {
        let one = layout("AB", &sty()).unwrap();
        let two = layout("AB\nAB", &sty()).unwrap();
        assert!(two.min.y < one.min.y - 10.0);
        let wide = layout("AB", &Style { spacing: 3.0, ..sty() }).unwrap();
        assert!(wide.max.x > one.max.x + 2.5);
    }

    #[test]
    fn rotation_turns_the_outline() {
        let l = layout("I", &Style { rotation: 90.0, ..sty() }).unwrap();
        assert!(l.max.x - l.min.x > 9.0 && l.max.y - l.min.y < 3.0, "{:?} {:?}", l.min, l.max);
    }

    #[test]
    fn nothing_to_draw_is_an_error() {
        assert!(matches!(layout("   ", &sty()), Err(TextError::Empty)));
        assert!(layout("A", &Style { height: 0.0, ..sty() }).is_err());
    }
}
