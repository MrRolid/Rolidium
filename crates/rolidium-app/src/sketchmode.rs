// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Interaction state of the sketch mode: tools, in-progress drawing with dimension fields, snapping.

use rolidium_doc::rolidium_sketch::{DVec2, Entity, PlaneDef, SnapKind};
use rolidium_doc::{BodyId, SketchId};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SkTool {
    None,
    Line,
    Rect,
    Circle,
    Arc,
}

pub struct Field {
    pub label: &'static str,
    pub text: String,
}

pub struct Drawing {
    pub start: DVec2,
    pub fields: Vec<Field>,
    pub active: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SnapLabel {
    Endpoint,
    Midpoint,
    Center,
    FaceVertex,
    FaceEdge,
    Grid,
    Free,
    /// Points of 3D features, projected into the plane.
    Vertex3D,
    Mid3D,
    Center3D,
    Edge3D,
    Intersection,
    Alignment,
    Extension,
    Parallel,
    Perpendicular,
    Symmetric,
    Tangent,
}

impl SnapLabel {
    pub fn text(self) -> &'static str {
        match self {
            SnapLabel::Endpoint => "endpoint",
            SnapLabel::Midpoint => "midpoint",
            SnapLabel::Center => "center",
            SnapLabel::FaceVertex => "face vertex",
            SnapLabel::FaceEdge => "on face edge",
            SnapLabel::Grid => "grid",
            SnapLabel::Free => "",
            SnapLabel::Vertex3D => "vertex (3D)",
            SnapLabel::Mid3D => "edge midpoint (3D)",
            SnapLabel::Center3D => "circle centre (3D)",
            SnapLabel::Edge3D => "on edge (3D)",
            SnapLabel::Intersection => "intersection",
            SnapLabel::Alignment => "aligned",
            SnapLabel::Extension => "extension",
            SnapLabel::Parallel => "parallel",
            SnapLabel::Perpendicular => "perpendicular",
            SnapLabel::Symmetric => "symmetric",
            SnapLabel::Tangent => "tangent",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Snap {
    pub pos: DVec2,
    pub label: SnapLabel,
    pub horizontal: bool,
    pub vertical: bool,
}

/// Typed distances (in the sketch axes) from the reference point to the point being placed.
pub struct Place {
    pub fields: [String; 2],
    pub active: usize,
}

pub struct SketchMode {
    pub plane: PlaneDef,
    /// The plane before the offset was applied, and the offset (mm) along its normal.
    pub base_plane: PlaneDef,
    pub offset: f64,
    pub offset_text: String,
    pub body: Option<BodyId>,
    pub sketch: Option<SketchId>,
    pub tool: SkTool,
    pub drawing: Option<Drawing>,
    /// Boundary of the face being drawn on, in sketch coordinates.
    pub outline: Vec<(DVec2, DVec2)>,
    pub corners: Vec<DVec2>,
    pub snap: Option<Snap>,
    /// Dotted guide lines of the current snap and the real 3D point it was projected from.
    pub guides: Vec<(DVec2, DVec2)>,
    pub real3d: Option<glam::Vec3>,
    /// Weakly drawn helper points (mirrored copies).
    pub helpers: Vec<DVec2>,
    /// Reference of the distance fields shown while placing a point, and the typed distances.
    pub ref_point: Option<DVec2>,
    pub place: Option<Place>,
    /// Arc tool: the end point once it has been placed (the next click gives the point on the arc).
    pub arc_end: Option<DVec2>,
    /// Point the camera centres on when the view is turned to the sketch.
    pub focus: glam::Vec3,
}

fn closest_on_segment(a: DVec2, b: DVec2, p: DVec2) -> DVec2 {
    let d = b - a;
    let l2 = d.length_squared();
    if l2 < 1e-18 {
        return a;
    }
    a + d * ((p - a).dot(d) / l2).clamp(0.0, 1.0)
}

fn parse(f: &Field) -> Option<f64> {
    f.text.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

impl SketchMode {
    pub fn new(plane: PlaneDef, body: Option<BodyId>, sketch: Option<SketchId>) -> Self {
        SketchMode {
            plane,
            base_plane: plane,
            offset: 0.0,
            offset_text: "0".into(),
            body,
            sketch,
            tool: SkTool::None,
            drawing: None,
            outline: Vec::new(),
            corners: Vec::new(),
            snap: None,
            guides: Vec::new(),
            real3d: None,
            helpers: Vec::new(),
            ref_point: None,
            place: None,
            arc_end: None,
            focus: plane.origin.as_vec3(),
        }
    }

    /// Moves the sketch plane along its normal (only meaningful while nothing is drawn on it).
    pub fn set_offset(&mut self, offset: f64) {
        self.offset = offset;
        self.plane = PlaneDef {
            origin: self.base_plane.origin + self.base_plane.normal.normalize() * offset,
            ..self.base_plane
        };
        self.focus = self.plane.origin.as_vec3();
    }

    pub fn set_tool(&mut self, t: SkTool) {
        self.tool = t;
        self.drawing = None;
    }

    pub fn begin(&mut self, start: DVec2) {
        let labels: &[&'static str] = match self.tool {
            SkTool::Line => &["Length", "Angle"],
            SkTool::Rect => &["Width", "Height"],
            SkTool::Circle => &["Diameter"],
            SkTool::Arc => &["Radius"],
            SkTool::None => return,
        };
        self.arc_end = None;
        self.drawing = Some(Drawing {
            start,
            fields: labels
                .iter()
                .map(|l| Field {
                    label: l,
                    text: String::new(),
                })
                .collect(),
            active: 0,
        });
    }

    /// Snapping of a raw plane position. `mm_per_px` converts the pixel radii; `grid` is the minor grid step.
    pub fn compute_snap(&self, raw: DVec2, mm_per_px: f64, grid: f64, points: &[(DVec2, SnapKind)]) -> Snap {
        let r_pt = 12.0 * mm_per_px;
        let mut best: Option<(f64, DVec2, SnapLabel)> = None;
        let mut consider = |d: f64, p: DVec2, l: SnapLabel| {
            if d <= r_pt && best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, p, l));
            }
        };
        for (p, k) in points {
            let l = match k {
                SnapKind::Endpoint => SnapLabel::Endpoint,
                SnapKind::Midpoint => SnapLabel::Midpoint,
                SnapKind::Center => SnapLabel::Center,
            };
            consider(p.distance(raw), *p, l);
        }
        for c in &self.corners {
            consider(c.distance(raw), *c, SnapLabel::FaceVertex);
        }
        if let Some((_, p, l)) = best {
            return Snap {
                pos: p,
                label: l,
                horizontal: false,
                vertical: false,
            };
        }
        let mut edge: Option<(f64, DVec2)> = None;
        for (a, b) in &self.outline {
            let q = closest_on_segment(*a, *b, raw);
            let d = q.distance(raw);
            if d <= r_pt && edge.is_none_or(|(bd, _)| d < bd) {
                edge = Some((d, q));
            }
        }
        if let Some((_, p)) = edge {
            return Snap {
                pos: p,
                label: SnapLabel::FaceEdge,
                horizontal: false,
                vertical: false,
            };
        }
        let mut pos = raw;
        let (mut h, mut v) = (false, false);
        if let Some(d) = &self.drawing {
            if (raw.y - d.start.y).abs() <= 6.0 * mm_per_px {
                pos.y = d.start.y;
                h = true;
            }
            if (raw.x - d.start.x).abs() <= 6.0 * mm_per_px {
                pos.x = d.start.x;
                v = true;
            }
        }
        let grid_on_screen = grid / mm_per_px;
        let mut label = SnapLabel::Free;
        if grid_on_screen >= 4.0 {
            if !v {
                pos.x = (pos.x / grid).round() * grid;
            }
            if !h {
                pos.y = (pos.y / grid).round() * grid;
            }
            label = SnapLabel::Grid;
        }
        Snap {
            pos,
            label,
            horizontal: h,
            vertical: v,
        }
    }

    /// The entity the current drawing would create, from the mouse position and the typed values.
    pub fn effective(&self, mouse: DVec2) -> Option<Entity> {
        let d = self.drawing.as_ref()?;
        let delta = mouse - d.start;
        let f = |i: usize| d.fields.get(i).and_then(parse);
        match self.tool {
            SkTool::Line => {
                let len = f(0).map(f64::abs).unwrap_or(delta.length());
                let ang = f(1)
                    .map(f64::to_radians)
                    .unwrap_or_else(|| delta.y.atan2(delta.x));
                Some(Entity::Line {
                    a: d.start,
                    b: d.start + DVec2::new(ang.cos(), ang.sin()) * len,
                })
            }
            SkTool::Rect => {
                let sign = |v: f64| if v < 0.0 { -1.0 } else { 1.0 };
                let dim = |typed: Option<f64>, mouse: f64| match typed {
                    Some(t) => sign(mouse) * t.abs() * sign(t),
                    None => mouse,
                };
                Some(Entity::Rect {
                    a: d.start,
                    b: d.start + DVec2::new(dim(f(0), delta.x), dim(f(1), delta.y)),
                })
            }
            SkTool::Circle => Some(Entity::Circle {
                center: d.start,
                radius: f(0).map(|v| v.abs() / 2.0).unwrap_or(delta.length()),
            }),
            SkTool::Arc => match self.arc_end {
                None => Some(Entity::Line { a: d.start, b: mouse }),
                Some(end) => match f(0).map(f64::abs) {
                    Some(r) => Entity::arc_chord_radius(d.start, end, r, mouse),
                    None => Entity::arc_through(d.start, mouse, end),
                },
            },
            SkTool::None => None,
        }
    }

    /// Live values shown next to the cursor: (label, text, typed).
    pub fn field_texts(&self, mouse: DVec2) -> Vec<(&'static str, String, bool)> {
        let Some(d) = &self.drawing else { return Vec::new() };
        if self.tool == SkTool::Arc && self.arc_end.is_none() {
            return Vec::new();
        }
        let live: Vec<f64> = match self.effective(mouse) {
            Some(Entity::Line { a, b }) => vec![a.distance(b), (b - a).to_angle().to_degrees()],
            Some(Entity::Rect { a, b }) => vec![(b.x - a.x).abs(), (b.y - a.y).abs()],
            Some(Entity::Circle { radius, .. }) => vec![radius * 2.0],
            Some(Entity::Arc { radius, .. }) if self.arc_end.is_some() => vec![radius],
            Some(Entity::Arc { .. }) => vec![],
            None => vec![],
        };
        d.fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                if f.text.is_empty() {
                    (
                        f.label,
                        format!("{:.2}", live.get(i).copied().unwrap_or(0.0)),
                        false,
                    )
                } else {
                    (f.label, f.text.clone(), true)
                }
            })
            .collect()
    }

    pub fn type_char(&mut self, ch: char) {
        let ch = if ch == ',' { '.' } else { ch };
        if self.drawing.is_none() {
            if let Some(p) = &mut self.place {
                if ch.is_ascii_digit() || ch == '.' || ch == '-' {
                    let f = &mut p.fields[p.active];
                    if (ch == '-' && !f.is_empty()) || (ch == '.' && f.contains('.')) {
                        return;
                    }
                    f.push(ch);
                }
            }
            return;
        }
        let Some(d) = &mut self.drawing else { return };
        if !(ch.is_ascii_digit() || ch == '.' || ch == '-') {
            return;
        }
        let f = &mut d.fields[d.active];
        if ch == '-' && !f.text.is_empty() {
            return;
        }
        if ch == '.' && f.text.contains('.') {
            return;
        }
        f.text.push(ch);
    }

    /// Where the typed distances from the reference put the point (the sign follows the mouse if not typed).
    pub fn place_target(&self, mouse: DVec2) -> Option<DVec2> {
        let (r, p) = (self.ref_point?, self.place.as_ref()?);
        let num = |i: usize, side: f64| -> Option<f64> {
            let t = p.fields[i].trim();
            if t.is_empty() {
                None
            } else {
                t.parse::<f64>().ok().map(|v| {
                    if t.starts_with('-') {
                        v
                    } else {
                        v.abs() * if side < 0.0 { -1.0 } else { 1.0 }
                    }
                })
            }
        };
        let d = mouse - r;
        Some(DVec2::new(
            num(0, d.x).map_or(mouse.x, |v| r.x + v),
            num(1, d.y).map_or(mouse.y, |v| r.y + v),
        ))
    }

    pub fn backspace(&mut self) {
        if let (None, Some(p)) = (&self.drawing, &mut self.place) {
            p.fields[p.active].pop();
        }
        if let Some(d) = &mut self.drawing {
            d.fields[d.active].text.pop();
        }
    }

    pub fn tab(&mut self) {
        if self.drawing.is_none() && self.tool != SkTool::None {
            match &mut self.place {
                None => {
                    self.place = Some(Place {
                        fields: [String::new(), String::new()],
                        active: 0,
                    });
                }
                Some(p) => p.active = 1 - p.active,
            }
            return;
        }
        if let Some(d) = &mut self.drawing {
            d.active = (d.active + 1) % d.fields.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(tool: SkTool) -> SketchMode {
        let mut m = SketchMode::new(PlaneDef::base(), None, None);
        m.set_tool(tool);
        m.begin(DVec2::new(10.0, 10.0));
        m
    }

    #[test]
    fn typed_line_length_keeps_mouse_direction() {
        let mut m = mode(SkTool::Line);
        for c in "25".chars() {
            m.type_char(c);
        }
        let e = m.effective(DVec2::new(10.0, 50.0)).unwrap();
        match e {
            Entity::Line { a, b } => {
                assert!((b - a).length() - 25.0 < 1e-9);
                assert!((b.x - 10.0).abs() < 1e-9 && (b.y - 35.0).abs() < 1e-9);
            }
            _ => panic!(),
        }
        m.tab();
        for c in "90".chars() {
            m.type_char(c);
        }
        assert!(
            matches!(m.effective(DVec2::new(99.0, 10.0)), Some(Entity::Line { b, .. }) if (b.x - 10.0).abs() < 1e-9 && (b.y - 35.0).abs() < 1e-9)
        );
    }

    #[test]
    fn rect_and_circle_fields() {
        let mut m = mode(SkTool::Rect);
        m.type_char('3');
        m.type_char('0');
        m.tab();
        m.type_char('2');
        m.type_char('0');
        match m.effective(DVec2::new(0.0, 0.0)).unwrap() {
            // mouse is down-left of the start, typed sizes keep that direction
            Entity::Rect { a, b } => assert_eq!((b - a), DVec2::new(-30.0, -20.0)),
            _ => panic!(),
        }
        let mut c = mode(SkTool::Circle);
        c.type_char('1');
        c.type_char('0');
        assert!(
            matches!(c.effective(DVec2::new(11.0, 10.0)), Some(Entity::Circle { radius, .. }) if (radius - 5.0).abs() < 1e-12)
        );
    }

    #[test]
    fn snapping_priorities() {
        let m = mode(SkTool::Line);
        let pts = [(DVec2::new(30.0, 30.0), SnapKind::Endpoint)];
        // near an endpoint: endpoint wins over grid
        let s = m.compute_snap(DVec2::new(30.4, 29.8), 0.1, 10.0, &pts);
        assert_eq!(s.label, SnapLabel::Endpoint);
        assert_eq!(s.pos, DVec2::new(30.0, 30.0));
        // horizontal to the start point
        let s = m.compute_snap(DVec2::new(47.3, 10.2), 0.1, 10.0, &[]);
        assert!(s.horizontal && (s.pos.y - 10.0).abs() < 1e-12);
        // grid
        let s = m.compute_snap(DVec2::new(61.9, 80.3), 0.1, 10.0, &[]);
        assert_eq!(s.label, SnapLabel::Grid);
        assert_eq!(s.pos, DVec2::new(60.0, 80.0));
        // far from everything with a tiny grid: free
        let s = m.compute_snap(DVec2::new(61.9, 80.3), 1.0, 1.0, &[]);
        assert_eq!(s.label, SnapLabel::Free);
    }
}
