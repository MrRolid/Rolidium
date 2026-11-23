// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The tool "Sketch plane" (key N): the pointer over a point (the end of a curve, a vertex of a body, the centre of a round edge, a point of an edge)
//! shows the plane that would be chosen there; the mouse wheel goes through the proposals, a click starts a sketch on the plane. The last proposal
//! is a free plane: three rings round the point turn it in steps, the panel shows its angles to XY, XZ and YZ.

use super::*;
use crate::i18n::{parse_num, tr};
use rolidium_doc::{
    angles_to_base, in_steps, turned, with_angle_to_base, CurveEnd, CurveId, EndFrame, PlaneKind,
    SketchAnchor,
};

/// How near (pixels) the pointer must be to a point to take it.
const REACH_END: f32 = 16.0;
const REACH_POINT: f32 = 11.0;
const REACH_EDGE: f32 = 7.0;
/// The wheel moves to the next proposal after this much scrolling.
const WHEEL_STEP: f32 = 30.0;

/// One plane that can be chosen at the point.
#[derive(Clone)]
pub struct Prop {
    pub name: &'static str,
    pub plane: PlaneDef,
    /// Kept with the sketch when the point is the end of a curve.
    pub kind: Option<PlaneKind>,
    pub free: bool,
}

/// The point under the pointer and the planes that can be chosen there.
#[derive(Clone)]
pub struct Hover {
    pub point: DVec3,
    pub anchor: Option<(CurveId, CurveEnd)>,
    pub props: Vec<Prop>,
}

/// The free plane that is being turned.
pub struct FreeEdit {
    pub plane: PlaneDef,
    pub anchor: Option<(CurveId, CurveEnd)>,
    pub step: f64,
    /// A ring is being dragged: which one (0 = round X, 1 = round Y, 2 = round Z), the parameter where the drag began, the plane then.
    drag: Option<(usize, f64, PlaneDef)>,
    fields: [String; 3],
    key: [f64; 3],
}

#[derive(Default)]
pub struct PlaneAt {
    pub hover: Option<Hover>,
    pub choice: usize,
    wheel: f32,
    pub free: Option<FreeEdit>,
}

fn props_for(kinds: &[(&'static str, PlaneDef)], point: DVec3, free_from: Option<PlaneDef>) -> Vec<Prop> {
    let mut v: Vec<Prop> = kinds
        .iter()
        .map(|(n, p)| Prop {
            name: n,
            plane: *p,
            kind: None,
            free: false,
        })
        .collect();
    for (name, base) in [
        ("Plane parallel to XY", PlaneDef::base()),
        ("Plane parallel to XZ", PlaneDef::xz()),
        ("Plane parallel to YZ", PlaneDef::yz()),
    ] {
        v.push(Prop {
            name,
            plane: PlaneDef {
                origin: point,
                ..base
            },
            kind: None,
            free: false,
        });
    }
    v.push(Prop {
        name: "Free plane",
        plane: PlaneDef {
            origin: point,
            ..free_from.unwrap_or(PlaneDef::base())
        },
        kind: None,
        free: true,
    });
    v
}

/// The circle (or arc) a polyline lies on: its centre and the axis, when all its points are on one circle.
fn circle_of(points: &[Vec3]) -> Option<(DVec3, DVec3)> {
    if points.len() < 6 {
        return None;
    }
    let (a, b, c) = (
        points[0].as_dvec3(),
        points[points.len() / 2].as_dvec3(),
        points[points.len() - 1].as_dvec3(),
    );
    let (ab, ac) = (b - a, c - a);
    let n = ab.cross(ac);
    if n.length() < 1e-9 {
        return None;
    }
    let centre = a
        + (n.cross(ab) * ac.length_squared() + ac.cross(n) * ab.length_squared())
            / (2.0 * n.length_squared());
    let r = (a - centre).length();
    let axis = n.normalize();
    let ok = points.iter().all(|p| {
        ((p.as_dvec3() - centre).length() - r).abs() < 0.02 * r
            && (p.as_dvec3() - centre).dot(axis).abs() < 0.02 * r
    });
    ok.then_some((centre, axis))
}

impl App {
    /// Starts the tool.
    pub(super) fn begin_plane_at(&mut self) {
        if !self.idle() {
            return;
        }
        self.cancel_pull();
        self.plane_move = None;
        self.plane_pick = false;
        self.plane_at = Some(PlaneAt::default());
        self.set_status(
            "Sketch plane: point at the end of a curve, a vertex, the centre of a round edge or a point of an edge; the wheel changes the plane, a click chooses it",
            false,
        );
        self.layers_changed = true;
    }

    pub(super) fn end_plane_at(&mut self) {
        if self.plane_at.take().is_some() {
            self.layers_changed = true;
        }
    }

    /// The point under the pointer (screen position) and what can be chosen there.
    pub(super) fn plane_hover_at(&self, p: Pos2, rect: Rect) -> Option<Hover> {
        let dist = |q: Vec3| self.project(q, rect).map(|s| s.distance(p));
        // 1. the ends of the curves
        let mut best: Option<(f32, Hover)> = None;
        for c in self.doc.curves().iter().filter(|c| c.visible) {
            for end in [CurveEnd::Start, CurveEnd::End] {
                let pts = self.curve_points(c.id);
                let q = match end {
                    CurveEnd::Start => pts.first(),
                    CurveEnd::End => pts.last(),
                };
                let Some(q) = q else { continue };
                let Some(d) = dist(*q).filter(|d| *d < REACH_END) else {
                    continue;
                };
                if best.as_ref().is_some_and(|(b, _)| *b <= d) {
                    continue;
                }
                let Ok(frame) = EndFrame::of(&c.def, end) else {
                    continue;
                };
                let props: Vec<Prop> = frame
                    .proposals(PlaneDef::base())
                    .into_iter()
                    .filter_map(|k| {
                        let plane = frame.plane(&k)?;
                        Some(Prop {
                            name: k.name(),
                            plane,
                            free: matches!(k, PlaneKind::Free { .. }),
                            kind: Some(k),
                        })
                    })
                    .collect();
                best = Some((
                    d,
                    Hover {
                        point: frame.point,
                        anchor: Some((c.id, end)),
                        props,
                    },
                ));
            }
        }
        if let Some((_, h)) = best {
            return Some(h);
        }
        // 2. vertices, centres of round edges and points of edges of the bodies
        let mut vertex: Option<(f32, DVec3, Option<DVec3>)> = None;
        let mut centre: Option<(f32, DVec3, DVec3)> = None;
        let mut on_edge: Option<(f32, DVec3, DVec3)> = None;
        for m in self.meshes.iter() {
            if !self.doc.body(m.id).is_some_and(|b| b.visible) {
                continue;
            }
            for e in &m.mesh.edges {
                let pts = &e.points;
                if pts.len() < 2 {
                    continue;
                }
                for (i, end) in [(0usize, pts[0]), (pts.len() - 1, pts[pts.len() - 1])] {
                    if let Some(d) = dist(end).filter(|d| *d < REACH_POINT) {
                        if vertex.as_ref().is_none_or(|v| d < v.0) {
                            let nb = if i == 0 { pts[1] } else { pts[pts.len() - 2] };
                            let t = (end - nb).as_dvec3().normalize_or_zero();
                            vertex = Some((d, end.as_dvec3(), (t.length() > 0.5).then_some(t)));
                        }
                    }
                }
                if let Some((c, axis)) = circle_of(pts) {
                    if let Some(d) = dist(c.as_vec3()).filter(|d| *d < REACH_POINT) {
                        if centre.as_ref().is_none_or(|v| d < v.0) {
                            centre = Some((d, c, axis));
                        }
                    }
                }
                for w in pts.windows(2) {
                    let (Some(a), Some(b)) = (self.project(w[0], rect), self.project(w[1], rect)) else {
                        continue;
                    };
                    let ab = b - a;
                    let t = if ab.length_sq() > 1e-6 {
                        ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let d = (a + ab * t).distance(p);
                    if d < REACH_EDGE && on_edge.as_ref().is_none_or(|v| d < v.0) {
                        let q = w[0] + (w[1] - w[0]) * t;
                        on_edge = Some((d, q.as_dvec3(), (w[1] - w[0]).as_dvec3().normalize_or_zero()));
                    }
                }
            }
        }
        let tangent_plane = |t: DVec3, o: DVec3| {
            let x = t.cross(DVec3::Z);
            PlaneDef {
                origin: o,
                normal: t,
                x_dir: if x.length() > 1e-6 {
                    x.normalize()
                } else {
                    DVec3::X
                },
            }
        };
        if let Some((_, o, t)) = vertex {
            let mut first = Vec::new();
            if let Some(t) = t {
                first.push(("Plane square to the edge", tangent_plane(t, o)));
            }
            return Some(Hover {
                point: o,
                anchor: None,
                props: props_for(&first, o, None),
            });
        }
        if let Some((_, c, axis)) = centre {
            let x = axis.cross(DVec3::Z);
            let plane = PlaneDef {
                origin: c,
                normal: axis,
                x_dir: if x.length() > 1e-6 {
                    x.normalize()
                } else {
                    DVec3::X
                },
            };
            return Some(Hover {
                point: c,
                anchor: None,
                props: props_for(&[("Plane of the round edge", plane)], c, None),
            });
        }
        if let Some((_, o, t)) = on_edge {
            let first: Vec<(&'static str, PlaneDef)> = if t.length() > 0.5 {
                vec![("Plane square to the edge", tangent_plane(t, o))]
            } else {
                Vec::new()
            };
            return Some(Hover {
                point: o,
                anchor: None,
                props: props_for(&first, o, None),
            });
        }
        None
    }

    /// The pointer, the wheel and the clicks while the tool is active.
    pub(super) fn plane_at_input(
        &mut self,
        resp: &egui::Response,
        pointer: Option<Pos2>,
        rect: Rect,
        scroll: f32,
    ) {
        let ctx = resp.ctx.clone();
        if self.plane_at.as_ref().is_some_and(|p| p.free.is_some()) {
            self.plane_free_input(resp, pointer, rect);
            return;
        }
        let hover = match (resp.hovered(), pointer) {
            (true, Some(p)) => self.plane_hover_at(p, rect),
            _ => None,
        };
        let Some(pa) = &mut self.plane_at else { return };
        // a new point starts again with the first proposal
        let same = match (&pa.hover, &hover) {
            (Some(a), Some(b)) => (a.point - b.point).length() < 1e-9 && a.props.len() == b.props.len(),
            (None, None) => true,
            _ => false,
        };
        if !same {
            pa.choice = 0;
            pa.wheel = 0.0;
        }
        pa.hover = hover;
        let mut status = None;
        if let Some(h) = &pa.hover {
            let n = h.props.len();
            pa.wheel += scroll;
            while pa.wheel >= WHEEL_STEP {
                pa.wheel -= WHEEL_STEP;
                pa.choice = (pa.choice + n - 1) % n;
            }
            while pa.wheel <= -WHEEL_STEP {
                pa.wheel += WHEEL_STEP;
                pa.choice = (pa.choice + 1) % n;
            }
            pa.choice = pa.choice.min(n - 1);
            status = Some(h.props[pa.choice].name);
        }
        if let Some(name) = status {
            self.set_status_quiet(trf(
                "Sketch plane: {} (the wheel changes it, a click chooses it)",
                &[tr(name)],
            ));
        }
        let click = resp.clicked_by(PointerButton::Primary);
        if click {
            let chosen = self
                .plane_at
                .as_ref()
                .and_then(|p| p.hover.as_ref().map(|h| (h.clone(), p.choice)));
            if let Some((h, i)) = chosen {
                let prop = h.props[i.min(h.props.len() - 1)].clone();
                if prop.free {
                    self.plane_at.as_mut().unwrap().free = Some(FreeEdit {
                        plane: prop.plane,
                        anchor: h.anchor,
                        step: 1.0,
                        drag: None,
                        fields: Default::default(),
                        key: [f64::NAN; 3],
                    });
                    self.set_status(
                        "Free plane: drag a ring to turn the plane (the step is in the panel), type the angles, Enter chooses it, Esc cancels",
                        false,
                    );
                } else {
                    self.start_sketch_on(prop.plane, h.anchor.map(|(c, e)| (c, e, prop.kind.clone())));
                }
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.end_plane_at();
            self.set_status("Cancelled", false);
        }
    }

    /// The three rings of the free plane: 360 points of the ring round the world axis `k` (0 = X, 1 = Y, 2 = Z) about `c`, radius `r`.
    fn free_ring(c: DVec3, r: f64, k: usize) -> Vec<DVec3> {
        (0..360)
            .map(|i| {
                let a = (i as f64).to_radians();
                let (s, co) = (a.sin() * r, a.cos() * r);
                c + match k {
                    0 => DVec3::new(0.0, co, s),
                    1 => DVec3::new(co, 0.0, s),
                    _ => DVec3::new(co, s, 0.0),
                }
            })
            .collect()
    }

    /// The ring nearest to the pointer, the parameter (degrees) of the point of it nearest to the pointer, and the distance (pixels).
    fn free_nearest(
        &self,
        c: DVec3,
        r: f64,
        rect: Rect,
        p: Pos2,
        only: Option<usize>,
    ) -> Option<(usize, f64, f32)> {
        let mut best: Option<(usize, f64, f32)> = None;
        for k in (0..3).filter(|k| only.is_none_or(|o| o == *k)) {
            for (i, q) in Self::free_ring(c, r, k).iter().enumerate() {
                if let Some(s) = self.project(q.as_vec3(), rect) {
                    let d = s.distance(p);
                    if best.is_none_or(|b| d < b.2) {
                        best = Some((k, i as f64, d));
                    }
                }
            }
        }
        best
    }

    fn ring_radius(&self) -> f64 {
        (self.camera.distance * 0.16) as f64
    }

    fn plane_free_input(&mut self, resp: &egui::Response, pointer: Option<Pos2>, rect: Rect) {
        let ctx = resp.ctx.clone();
        let r = self.ring_radius();
        let Some(fe) = self.plane_at.as_ref().and_then(|p| p.free.as_ref()) else {
            return;
        };
        let c = fe.plane.origin;
        if resp.drag_started_by(PointerButton::Primary) {
            if let Some(p) = pointer {
                if let Some((k, t, d)) = self.free_nearest(c, r, rect, p, None) {
                    if d < 14.0 {
                        let fe = self.plane_at.as_mut().unwrap().free.as_mut().unwrap();
                        fe.drag = Some((k, t, fe.plane));
                    }
                }
            }
        }
        let drag = self.plane_at.as_ref().unwrap().free.as_ref().unwrap().drag;
        if let (Some((k, t0, start)), Some(p), true) =
            (drag, pointer, resp.dragged_by(PointerButton::Primary))
        {
            if let Some((_, t, _)) = self.free_nearest(c, r, rect, p, Some(k)) {
                let mut delta = t - t0;
                if delta > 180.0 {
                    delta -= 360.0;
                } else if delta < -180.0 {
                    delta += 360.0;
                }
                let fe = self.plane_at.as_mut().unwrap().free.as_mut().unwrap();
                let axis = [DVec3::X, DVec3::Y, DVec3::Z][k];
                fe.plane = turned(&start, axis, in_steps(delta, fe.step));
                self.layers_changed = true;
            }
        }
        if resp.drag_stopped() {
            if let Some(fe) = self.plane_at.as_mut().and_then(|p| p.free.as_mut()) {
                fe.drag = None;
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Enter)) {
            self.confirm_free();
        } else if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.end_plane_at();
            self.set_status("Cancelled", false);
        }
    }

    fn confirm_free(&mut self) {
        let Some(fe) = self.plane_at.as_ref().and_then(|p| p.free.as_ref()) else {
            return;
        };
        let plane = fe.plane;
        let anchor = fe.anchor.map(|(c, e)| {
            (
                c,
                e,
                Some(PlaneKind::Free {
                    normal: plane.normal,
                    x_dir: plane.x_dir,
                }),
            )
        });
        self.start_sketch_on(plane, anchor);
    }

    /// A new sketch on `plane` (the empty sketch that is open moves to it); laid on the end of a curve when `anchor` says so.
    pub(super) fn start_sketch_on(
        &mut self,
        plane: PlaneDef,
        anchor: Option<(CurveId, CurveEnd, Option<PlaneKind>)>,
    ) {
        self.end_plane_at();
        self.cancel_pull();
        self.plane_pick = false;
        self.selection.clear();
        let mut sm = SketchMode::new(plane, None, None);
        sm.anchor =
            anchor.and_then(|(curve, end, kind)| kind.map(|plane| SketchAnchor { curve, end, plane }));
        self.sk = Some(sm);
        self.turn_to_sketch();
        self.layers_changed = true;
        self.set_status("Sketch on the chosen plane", false);
    }

    /// The panel of the free plane: the angles to XY, XZ and YZ (typed numbers are taken with Enter), the step of the turning, the buttons.
    pub(super) fn plane_free_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        let Some(fe) = self.plane_at.as_ref().and_then(|p| p.free.as_ref()) else {
            return;
        };
        let angles = angles_to_base(&fe.plane);
        let mut step = fe.step;
        let mut fields = fe.fields.clone();
        let mut key = fe.key;
        if key.iter().zip(angles).any(|(a, b)| !(a - b).abs().lt(&1e-9)) {
            key = angles;
            fields = angles.map(|a| format!("{a:.1}"));
        }
        let (mut ok, mut cancel) = (false, false);
        let mut typed: Option<(usize, f64)> = None;
        let mut docks = std::mem::take(&mut self.docks);
        docks.panel(
            ctx,
            "plane_free",
            Pos2::new(rect.max.x - 250.0, rect.min.y + 10.0),
            None,
            220.0,
            &tr("Free plane"),
            false,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Step"));
                    for s in [1.0, 5.0, 10.0, 15.0] {
                        ui.selectable_value(&mut step, s, format!("{s}°"));
                    }
                });
                for (i, name) in ["XY", "XZ", "YZ"].into_iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(trf("Angle to {}", &[name.to_string()]));
                        let r = ui.add(egui::TextEdit::singleline(&mut fields[i]).desired_width(52.0));
                        ui.label("°");
                        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            if let Some(v) = parse_num(&fields[i]) {
                                typed = Some((i, v));
                            }
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button(tr("Choose")).clicked() {
                        ok = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            },
        );
        self.docks = docks;
        if let Some(fe) = self.plane_at.as_mut().and_then(|p| p.free.as_mut()) {
            fe.step = step;
            fe.fields = fields;
            fe.key = key;
            if let Some((i, v)) = typed {
                fe.plane = with_angle_to_base(&fe.plane, i, v.clamp(0.0, 90.0));
                fe.key = [f64::NAN; 3];
                self.layers_changed = true;
            }
        }
        if ok {
            self.confirm_free();
        }
        if cancel {
            self.end_plane_at();
            self.set_status("Cancelled", false);
        }
    }

    /// The preview of the plane under the pointer (a sheet with its outline and the point) and the rings of the free plane.
    pub(super) fn plane_at_paint(&self, ui: &egui::Ui, rect: Rect) {
        let Some(pa) = &self.plane_at else { return };
        let painter = ui.painter().with_clip_rect(rect);
        let col = crate::theme::rgb(255, 190, 60);
        let sheet = |plane: &PlaneDef, half: f64, alpha: u8| {
            let (u, v) = (plane.x_dir, plane.y_dir());
            let corners: Option<Vec<Pos2>> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .iter()
                .map(|(a, b)| self.project((plane.origin + u * a * half + v * b * half).as_vec3(), rect))
                .collect();
            if let Some(c) = corners {
                painter.add(egui::Shape::convex_polygon(
                    c,
                    Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), alpha),
                    Stroke::new(1.5, col),
                ));
            }
        };
        let half = (self.camera.distance * 0.2) as f64;
        if let Some(fe) = &pa.free {
            sheet(&fe.plane, half, 50);
            let r = self.ring_radius();
            for (k, colour) in [
                (0, crate::theme::rgb(255, 90, 90)),
                (1, crate::theme::rgb(90, 220, 110)),
                (2, crate::theme::rgb(100, 150, 255)),
            ] {
                let pts: Vec<Pos2> = Self::free_ring(fe.plane.origin, r, k)
                    .iter()
                    .filter_map(|q| self.project(q.as_vec3(), rect))
                    .collect();
                let thick = fe.drag.is_some_and(|d| d.0 == k);
                painter.add(egui::Shape::closed_line(
                    pts,
                    Stroke::new(if thick { 4.0 } else { 2.5 }, colour),
                ));
            }
            if let Some(c) = self.project(fe.plane.origin.as_vec3(), rect) {
                painter.circle_filled(c, 4.0, col);
            }
            return;
        }
        if let Some(h) = &pa.hover {
            let prop = &h.props[pa.choice.min(h.props.len() - 1)];
            sheet(&prop.plane, half, 55);
            if let Some(c) = self.project(h.point.as_vec3(), rect) {
                painter.circle_filled(c, 5.0, col);
                painter.text(
                    c + Vec2::new(10.0, -10.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!(
                        "{} ({}/{})",
                        tr(prop.name),
                        pa.choice.min(h.props.len() - 1) + 1,
                        h.props.len()
                    ),
                    egui::FontId::proportional(14.0),
                    col,
                );
            }
        }
    }

    /// The ends of the curves, the start green and the end orange, when a path is being chosen, the tool Sketch plane is active or a curve is selected.
    pub(super) fn paint_curve_ends(&self, ui: &egui::Ui, rect: Rect) {
        let active = self.tool == Tool::Sweep || self.plane_at.is_some();
        let painter = ui.painter().with_clip_rect(rect);
        let (start_col, end_col) = (crate::theme::rgb(70, 215, 110), crate::theme::rgb(255, 125, 50));
        for c in self.doc.curves().iter().filter(|c| c.visible) {
            if !(active || self.curve_sel == Some(c.id)) {
                continue;
            }
            let pts = self.curve_points(c.id);
            for (q, col, label) in [
                (pts.first(), start_col, tr("start")),
                (pts.last(), end_col, tr("end")),
            ] {
                let Some(s) = q.and_then(|q| self.project(*q, rect)) else {
                    continue;
                };
                painter.circle_filled(s, 6.0, col);
                painter.circle_stroke(s, 8.0, Stroke::new(1.5, col));
                painter.text(
                    s + Vec2::new(11.0, -9.0),
                    egui::Align2::LEFT_BOTTOM,
                    label,
                    egui::FontId::proportional(12.0),
                    col,
                );
            }
        }
    }
}
