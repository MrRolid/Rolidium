// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The Measure tool: click two elements (a vertex, a point of a sketch or curve, an edge, a face) and the distance, its
//! X/Y/Z parts, an angle or the distance of cylinder axes appear in the view and in a panel. The measurements are
//! temporary (not saved); permanent dimensions will be built from the same elements (`MElem`) later.

use super::pick::dist_to_segment;
use super::properties::{fmt_area, fmt_length, Prop};
use super::*;
use crate::i18n::{tr, trf};
use rolidium_kernel::{measure, EdgeId, EdgeKind, Elem, Measurement};
use std::sync::Arc;

/// What is measured: the stable description of an element (it can become a permanent dimension later).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MElem {
    Point(DVec3),
    Edge(BodyId, u32),
    Face(BodyId, u32),
}

pub struct MResult {
    pub a: MElem,
    pub b: MElem,
    pub m: Measurement,
}

#[derive(Default)]
pub struct MeasureUi {
    pub first: Option<MElem>,
    /// Under the pointer now (a point; edges and faces are highlighted by the renderer).
    pub hover: Option<MElem>,
    pub results: Vec<MResult>,
    /// The document state the results belong to; when it changes the results are dropped.
    pub state: Option<Arc<rolidium_doc::State>>,
}

const PICK_RADIUS: f32 = 9.0;

fn color_line() -> Color32 {
    crate::theme::rgb(255, 214, 90)
}

impl App {
    pub(super) fn measure_active(&self) -> bool {
        self.tool == Tool::Measure && self.sk.is_none()
    }

    pub(super) fn activate_measure(&mut self) {
        self.leave_sketch();
        self.tool = Tool::Measure;
        self.measure.first = None;
        self.clear_selection();
        self.layers_changed = true;
    }

    /// Esc: forget the first click, or else all measurements. Returns true when it did something.
    pub(super) fn measure_escape(&mut self) -> bool {
        if self.tool != Tool::Measure {
            return false;
        }
        if self.measure.first.take().is_some() {
            self.clear_selection();
            self.layers_changed = true;
            return true;
        }
        if !self.measure.results.is_empty() {
            self.measure.results.clear();
            return true;
        }
        false
    }

    /// Why the tool cannot be used now.
    pub(super) fn measure_unavailable(&self) -> Option<&'static str> {
        if self.doc.bodies().is_empty()
            && self.doc.meshes().is_empty()
            && self.doc.curves().is_empty()
            && self.doc.sketches().is_empty()
        {
            Some("Measuring needs a body, a curve or a sketch")
        } else {
            None
        }
    }

    fn nearest_on_lines(&self, lines: &[Vec<Vec3>], pointer: Pos2, rect: Rect) -> Option<(f32, DVec3)> {
        let mut best: Option<(f32, DVec3)> = None;
        for pts in lines {
            for w in pts.windows(2) {
                if let (Some(a), Some(b)) = (self.project(w[0], rect), self.project(w[1], rect)) {
                    let (d, t) = dist_to_segment(pointer, a, b);
                    if d <= PICK_RADIUS && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, w[0].lerp(w[1], t).as_dvec3()));
                    }
                }
            }
        }
        best
    }

    /// The element under the pointer: a vertex first, then a point of a sketch line or curve, an edge, a face.
    pub(super) fn measure_pick(
        &self,
        pointer: Pos2,
        rect: Rect,
        face: Option<(BodyId, u32)>,
    ) -> Option<MElem> {
        // vertices: the ends of edges, sketch shapes and curves
        let mut ends: Vec<Vec3> = Vec::new();
        for m in self.meshes.iter() {
            for e in &m.mesh.edges {
                if let (Some(a), Some(b)) = (e.points.first(), e.points.last()) {
                    ends.push(*a);
                    ends.push(*b);
                }
            }
        }
        let mut lines: Vec<Vec<Vec3>> = Vec::new();
        for s in self.doc.sketches().iter().filter(|s| s.visible) {
            for (_, e) in s.sketch.entities() {
                for pl in e.polylines() {
                    let pts: Vec<Vec3> = pl.iter().map(|p| s.plane.to_3d(*p).as_vec3()).collect();
                    if let (Some(a), Some(b)) = (pts.first(), pts.last()) {
                        ends.push(*a);
                        ends.push(*b);
                    }
                    lines.push(pts);
                }
            }
        }
        for (id, pts) in &self.curve_cache.lines {
            if self.doc.curve(*id).is_some_and(|c| c.visible) {
                if let (Some(a), Some(b)) = (pts.first(), pts.last()) {
                    ends.push(*a);
                    ends.push(*b);
                }
                lines.push(pts.clone());
            }
        }
        let vertex = ends
            .iter()
            .filter_map(|p| self.project(*p, rect).map(|s| ((s - pointer).length(), *p)))
            .filter(|(d, _)| *d <= PICK_RADIUS)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, p)) = vertex {
            return Some(MElem::Point(p.as_dvec3()));
        }
        // an edge of a body wins over sketch lines and curves, which give the nearest point on them
        let obj = self.pick_object_d(pointer, rect, true, false);
        if let Some((Obj::Edge(b, e), d)) = obj {
            // a narrow round or chamfer face must stay reachable: the edge only wins close to its line
            if d <= 2.5 || face.is_none_or(|(fb, ff)| !self.is_blend_face(fb, ff)) {
                return Some(MElem::Edge(b, e));
            }
        }
        if let Some((_, p)) = self.nearest_on_lines(&lines, pointer, rect) {
            return Some(MElem::Point(p));
        }
        face.map(|(b, f)| MElem::Face(b, f))
    }

    /// Ctrl+click: a point of the model under the pointer. It snaps to the middle of an edge and to the centre of a circle; else
    /// it is the nearest point of an edge, or the point of the face under the pointer.
    pub(super) fn measure_point_pick(
        &self,
        pointer: Pos2,
        rect: Rect,
        face: Option<(BodyId, u32)>,
    ) -> Option<MElem> {
        let mut snaps: Vec<Vec3> = Vec::new();
        let mut nearest: Option<(f32, Vec3)> = None;
        for m in self.meshes.iter() {
            for e in &m.mesh.edges {
                let circle = self
                    .doc
                    .body(m.id)
                    .and_then(|b| b.solid.edge_info(EdgeId(e.edge.0)).ok())
                    .is_some_and(|i| i.kind == EdgeKind::Circle);
                let (mid, centre) = edge_snaps(&e.points, circle);
                snaps.extend(mid);
                snaps.extend(centre);
                for w in e.points.windows(2) {
                    if let (Some(a), Some(b)) = (self.project(w[0], rect), self.project(w[1], rect)) {
                        let (d, t) = dist_to_segment(pointer, a, b);
                        if d <= PICK_RADIUS && nearest.is_none_or(|(bd, _)| d < bd) {
                            nearest = Some((d, w[0].lerp(w[1], t)));
                        }
                    }
                }
            }
        }
        let snap = snaps
            .iter()
            .filter_map(|p| self.project(*p, rect).map(|s| ((s - pointer).length(), *p)))
            .filter(|(d, _)| *d <= PICK_RADIUS + 3.0)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, p)) = snap {
            return Some(MElem::Point(p.as_dvec3()));
        }
        if let Some((_, p)) = nearest {
            return Some(MElem::Point(p.as_dvec3()));
        }
        let (b, f) = face?;
        let aspect = rect.width() / rect.height().max(1.0);
        let (o, d) = self.camera.ray(Self::ndc(pointer, rect), aspect);
        self.ray_face_hit(b, f, o, d).map(|(p, _)| MElem::Point(p))
    }

    pub(super) fn measure_input(
        &mut self,
        resp: &egui::Response,
        pointer: Option<Pos2>,
        rect: Rect,
        face: Option<(BodyId, u32)>,
    ) {
        // results belong to one version of the document
        if self
            .measure
            .state
            .as_ref()
            .is_none_or(|s| !Arc::ptr_eq(s, self.doc.state()))
        {
            self.measure.results.clear();
            self.measure.first = None;
            self.measure.state = Some(self.doc.state().clone());
        }
        let ctrl = resp.ctx.input(|i| i.modifiers.command);
        let pick = match (resp.hovered(), pointer) {
            (true, Some(p)) if ctrl => self.measure_point_pick(p, rect, face),
            (true, Some(p)) => self.measure_pick(p, rect, face),
            _ => None,
        };
        self.measure.hover = pick;
        if !resp.clicked_by(PointerButton::Primary) {
            return;
        }
        let Some(el) = pick else { return };
        match self.measure.first {
            None => {
                self.measure.first = Some(el);
                // the first element is shown as selected, and the Selection panel lists its properties
                self.clear_selection();
                match el {
                    MElem::Face(b, f) => self.selection.push((b, f)),
                    MElem::Edge(b, e) => self.edge_sel.push((b, e)),
                    MElem::Point(_) => {}
                }
                self.layers_changed = true;
            }
            Some(first) => {
                if first == el {
                    return;
                }
                match self.measure_two(first, el) {
                    Ok(m) => self.measure.results.push(MResult { a: first, b: el, m }),
                    Err(e) => self.set_status(e, true),
                }
                self.measure.first = None;
                self.clear_selection();
                self.layers_changed = true;
            }
        }
    }

    fn measure_two(&self, a: MElem, b: MElem) -> Result<Measurement, String> {
        let get = |e: &MElem| -> Result<Elem<'_>, String> {
            match *e {
                MElem::Point(p) => Ok(Elem::Point(p)),
                MElem::Edge(body, i) => self
                    .doc
                    .body(body)
                    .map(|bd| Elem::Edge(&bd.solid, EdgeId(i)))
                    .ok_or_else(|| tr("The body no longer exists")),
                MElem::Face(body, i) => self
                    .doc
                    .body(body)
                    .map(|bd| Elem::Face(&bd.solid, FaceId(i)))
                    .ok_or_else(|| tr("The body no longer exists")),
            }
        };
        measure(&get(&a)?, &get(&b)?).map_err(|e| e.to_string())
    }

    fn melem_name(&self, e: &MElem) -> String {
        match e {
            MElem::Point(p) => trf("Point ({}, {}, {})", &[fmt_n(p.x), fmt_n(p.y), fmt_n(p.z)]),
            MElem::Edge(b, _) => trf(
                "Edge of {}",
                &[self.doc.body(*b).map_or("?".into(), |b| b.name.clone())],
            ),
            MElem::Face(b, f) => {
                let kind = self
                    .doc
                    .body(*b)
                    .and_then(|bd| bd.solid.face_info(FaceId(*f)).ok())
                    .map(|i| i.kind);
                let what = if kind == Some(rolidium_kernel::FaceKind::Cylinder) {
                    "Cylindrical face of {}"
                } else {
                    "Face of {}"
                };
                trf(what, &[self.doc.body(*b).map_or("?".into(), |b| b.name.clone())])
            }
        }
    }

    /// The rows of one measurement (label, value); the click on a value copies it.
    fn measure_rows(&self, r: &MResult) -> Vec<Prop> {
        let m = &r.m;
        let row = |l: &str, v: String| Prop {
            label: tr(l),
            value: v,
        };
        let mut rows = vec![
            row("Distance", fmt_length(m.distance)),
            row("Δ X", fmt_length(m.delta.x)),
            row("Δ Y", fmt_length(m.delta.y)),
            row("Δ Z", fmt_length(m.delta.z)),
        ];
        if let Some(d) = m.parallel_distance {
            rows.push(row("Distance of the parallel faces", fmt_length(d)));
        }
        if let Some((d, _, _)) = m.axes_distance {
            rows.push(row("Distance of the axes", fmt_length(d)));
        }
        if let Some(a) = &m.angle {
            rows.push(row("Angle", format!("{}°", fmt_n(a.degrees))));
        }
        rows
    }

    pub(super) fn measure_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        if !self.measure_active() {
            return;
        }
        let mut clear = false;
        let mut docks = std::mem::take(&mut self.docks);
        docks.panel(
            ctx,
            "measure",
            Pos2::new(rect.max.x - 290.0, rect.min.y + 10.0),
            None,
            270.0,
            &tr("Measure"),
            false,
            |ui| {
                if self.measure.results.is_empty() {
                    ui.label(if self.measure.first.is_some() {
                        tr("Click the second element")
                    } else {
                        tr("Click two elements: a vertex, a point of a sketch or curve, an edge or a face")
                    });
                }
                for (i, r) in self.measure.results.iter().enumerate().rev() {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!(
                            "{}  ↔  {}",
                            self.melem_name(&r.a),
                            self.melem_name(&r.b)
                        ))
                        .strong(),
                    );
                    let _ = i;
                    for p in self.measure_rows(r) {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("{}:", p.label)).weak());
                            let resp = ui
                                .add(
                                    egui::Label::new(egui::RichText::new(&p.value).monospace())
                                        .sense(Sense::click()),
                                )
                                .on_hover_text(tr("Click to copy the value"));
                            if resp.clicked() {
                                let v = p.value.trim_end_matches('°').trim_end_matches(" mm").to_string();
                                ui.ctx().copy_text(v);
                            }
                        });
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.measure.results.is_empty() || self.measure.first.is_some(),
                            egui::Button::new(tr("Clear")),
                        )
                        .on_hover_text(tr("Remove all measurements (Esc)"))
                        .clicked()
                    {
                        clear = true;
                    }
                });
            },
        );
        self.docks = docks;
        if clear {
            self.measure.results.clear();
            self.measure.first = None;
            self.clear_selection();
            self.layers_changed = true;
        }
    }

    fn arrow(painter: &egui::Painter, from: Pos2, to: Pos2, col: Color32) {
        painter.line_segment([from, to], Stroke::new(1.8, col));
        let d = (from - to).normalized();
        if d.length() > 0.0 {
            let n = Vec2::new(-d.y, d.x);
            for tip in [to, from] {
                let dir = if tip == to { d } else { -d };
                painter.add(egui::Shape::convex_polygon(
                    vec![tip, tip + dir * 9.0 + n * 3.5, tip + dir * 9.0 - n * 3.5],
                    col,
                    Stroke::NONE,
                ));
            }
        }
    }

    fn label_at(painter: &egui::Painter, at: Pos2, text: String, col: Color32) {
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(14.0), col);
        let r = Rect::from_center_size(at, galley.size() + Vec2::new(10.0, 4.0));
        painter.rect_filled(r, 4.0, Color32::from_black_alpha(190));
        painter.galley(r.min + Vec2::new(5.0, 2.0), galley, col);
    }

    /// The dimension lines, the angle arcs and the marks of the elements under the pointer.
    pub(super) fn measure_paint(&self, ui: &egui::Ui, rect: Rect) {
        if !self.measure_active() {
            return;
        }
        let painter = ui.painter().with_clip_rect(rect);
        let col = color_line();
        let ring = |p: DVec3, c: Color32| {
            if let Some(s) = self.project(p.as_vec3(), rect) {
                painter.circle_stroke(s, 7.0, Stroke::new(2.0, c));
            }
        };
        if let Some(MElem::Point(p)) = self.measure.first {
            ring(p, col);
        }
        if let Some(MElem::Point(p)) = self.measure.hover {
            ring(p, crate::theme::rgb(120, 255, 160));
        }
        for r in &self.measure.results {
            let m = &r.m;
            if m.distance > 1e-9 {
                if let (Some(a), Some(b)) = (
                    self.project(m.from.as_vec3(), rect),
                    self.project(m.to.as_vec3(), rect),
                ) {
                    Self::arrow(&painter, a, b, col);
                    Self::label_at(
                        &painter,
                        a.lerp(b, 0.5) + Vec2::new(0.0, -12.0),
                        fmt_length(m.distance),
                        col,
                    );
                }
            }
            if let Some((d, p, q)) = m.axes_distance {
                if d > 1e-9 {
                    if let (Some(a), Some(b)) =
                        (self.project(p.as_vec3(), rect), self.project(q.as_vec3(), rect))
                    {
                        let c2 = crate::theme::rgb(150, 210, 255);
                        painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.8, c2), 7.0, 4.0));
                        Self::label_at(&painter, a.lerp(b, 0.5) + Vec2::new(0.0, 12.0), fmt_length(d), c2);
                    }
                }
            }
            if let Some(an) = &m.angle {
                if an.degrees > 1e-6 {
                    self.paint_angle(&painter, rect, an);
                }
            }
        }
    }

    fn paint_angle(&self, painter: &egui::Painter, rect: Rect, an: &rolidium_kernel::Angle) {
        let col = crate::theme::rgb(255, 160, 120);
        // an arc about 45 px in radius, drawn in the plane of the two arms
        let Some(c) = self.project(an.apex.as_vec3(), rect) else {
            return;
        };
        let probe = self.project((an.apex + an.dir_a).as_vec3(), rect);
        let px_per_mm = probe.map_or(1.0, |p| (p - c).length()).max(0.05);
        let radius = (45.0 / px_per_mm as f64).min(self.camera.distance as f64 * 0.5);
        let n = an.dir_a.cross(an.dir_b);
        let (u, v) = if n.length() < 1e-9 {
            (an.dir_a, an.dir_a)
        } else {
            (an.dir_a, n.cross(an.dir_a).normalize())
        };
        let total = an.degrees.to_radians();
        let pts: Vec<Pos2> = (0..=24)
            .filter_map(|i| {
                let t = total * i as f64 / 24.0;
                let p = an.apex + (u * t.cos() + v * t.sin()) * radius;
                self.project(p.as_vec3(), rect)
            })
            .collect();
        if pts.len() > 1 {
            painter.add(egui::Shape::line(pts.clone(), Stroke::new(2.0, col)));
            let mid = pts[pts.len() / 2];
            Self::label_at(
                painter,
                c + (mid - c) * 1.45,
                format!("{}°", fmt_n(an.degrees)),
                col,
            );
        }
    }
}

/// The snap points of an edge given by its polyline: the middle (by length) and, for a circle or an arc, the centre.
pub fn edge_snaps(points: &[Vec3], circle: bool) -> (Option<Vec3>, Option<Vec3>) {
    if points.len() < 2 {
        return (None, None);
    }
    let total: f32 = points.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    let mut left = total * 0.5;
    let mut mid = None;
    for w in points.windows(2) {
        let l = (w[1] - w[0]).length();
        if left <= l && l > 0.0 {
            mid = Some(w[0].lerp(w[1], left / l));
            break;
        }
        left -= l;
    }
    let centre = if circle && points.len() >= 4 {
        let n = points.len() - 1;
        let (a, b, c) = (points[0], points[n / 3], points[2 * n / 3]);
        let (u, v) = (b - a, c - a);
        let w = u.cross(v);
        (w.length_squared() > 1e-12).then(|| {
            a + (w.cross(u) * v.length_squared() + v.cross(w) * u.length_squared())
                / (2.0 * w.length_squared())
        })
    } else {
        None
    };
    (mid, centre)
}

/// Numbers with up to three decimals (trailing zeros removed).
fn fmt_n(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" || s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// The properties of a single element, the same ones the Selection panel gives.
#[allow(dead_code)]
pub fn single_rows(solid: &rolidium_kernel::Solid, face: Option<u32>, edge: Option<u32>) -> Vec<Prop> {
    let mut out = Vec::new();
    if let Some(f) = face {
        if let Ok(i) = solid.face_info(FaceId(f)) {
            out.push(Prop {
                label: tr("Area"),
                value: fmt_area(i.area),
            });
            if i.kind == rolidium_kernel::FaceKind::Cylinder {
                out.push(Prop {
                    label: tr("Diameter"),
                    value: fmt_length(i.radius * 2.0),
                });
            }
        }
    }
    if let Some(e) = edge {
        if let Ok(i) = solid.edge_info(EdgeId(e)) {
            out.push(Prop {
                label: tr("Length"),
                value: fmt_length(i.length),
            });
            if i.kind == EdgeKind::Circle && i.radius > 0.0 {
                out.push(Prop {
                    label: tr("Diameter"),
                    value: fmt_length(i.radius * 2.0),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_snaps_to_its_middle_and_a_circle_to_its_centre() {
        let line = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
        ];
        let (mid, centre) = edge_snaps(&line, false);
        assert!((mid.unwrap() - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-5 && centre.is_none());
        // a circle of radius 5 round (1, 2, 3), in a tilted plane
        let (cx, n) = (Vec3::new(1.0, 2.0, 3.0), Vec3::new(1.0, 1.0, 1.0).normalize());
        let u = n.cross(Vec3::Z).normalize();
        let v = n.cross(u);
        let ring: Vec<Vec3> = (0..=48)
            .map(|i| {
                let a = i as f32 / 48.0 * std::f32::consts::TAU;
                cx + (u * a.cos() + v * a.sin()) * 5.0
            })
            .collect();
        let (_, centre) = edge_snaps(&ring, true);
        assert!((centre.unwrap() - cx).length() < 1e-3, "{centre:?}");
        // an arc works too
        let (_, centre) = edge_snaps(&ring[..20], true);
        assert!((centre.unwrap() - cx).length() < 1e-3);
    }
}
