// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Dragging a shape of a sketch with the mouse (Select tool, in and out of the sketch mode): a circle or an arc by its centre (it moves) or by its rim
//! (the radius), a line by its middle (it moves) or an end, a rectangle by a corner or a side. While a point moves, the live distances to the two nearest
//! straight edges are shown and can be typed over (see `edgeedit.rs`); the whole drag is one command, so one Undo takes it back.

use super::edgeedit::*;
use super::*;
use rolidium_doc::rolidium_sketch::edgedims::{nearest_edges, EdgeDims, EdgeSnap};
use rolidium_doc::rolidium_sketch::EntityId;

/// What of a shape the pointer holds.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ShapeHandle {
    /// The centre of a circle or an arc: the shape moves.
    Centre,
    /// The rim of a circle or an arc: the radius changes.
    Rim,
    /// The middle of a line: the line moves.
    Mid,
    /// An end of a line (0 start, 1 end).
    End(usize),
    /// A corner of a rectangle (in the order of `Entity::rect_corners`).
    Corner(usize),
    /// A side of a rectangle (the side from corner i to corner i + 1).
    Edge(usize),
}

pub struct ShapeDrag {
    pub sketch: SketchId,
    pub entity: EntityId,
    pub old: Entity,
    pub handle: ShapeHandle,
    pub plane: PlaneDef,
    /// Where the pointer took hold of the plane, and the place of the handle then.
    pub grab: DVec2,
    pub point0: DVec2,
    /// The straight edges the live distances are measured to.
    pub edges: Vec<(DVec2, DVec2)>,
    /// The place the pointer is at in the plane now (kept so that the typed numbers can act without a movement).
    pub raw: DVec2,
    /// The place of the handle now (after the grid, the snaps and the typed numbers), the live distances there, and the shape that results.
    pub at: DVec2,
    pub dims: Option<EdgeDims>,
    pub snap: Option<EdgeSnap>,
    pub new: Entity,
    pub edit: EdgeEdit,
    /// The grid step (0: none).
    pub grid: f64,
    /// The curves and the points of the other shapes: the handle snaps to their crossings, quadrants, centres, end points and nearest points (Shift turns it off).
    pub curves: Vec<crate::snapcurves::Curve>,
    pub points: Vec<(DVec2, rolidium_doc::rolidium_sketch::SnapKind)>,
    /// What the handle is snapped to (a label at the cursor).
    pub label: Option<crate::sketchmode::SnapLabel>,
}

impl ShapeHandle {
    fn moves_a_point(self) -> bool {
        !matches!(self, ShapeHandle::Rim | ShapeHandle::Edge(_))
    }
}

/// The place a handle stands on.
fn handle_point(e: &Entity, h: ShapeHandle) -> Option<DVec2> {
    Some(match (*e, h) {
        (Entity::Circle { center, .. } | Entity::Arc { center, .. }, ShapeHandle::Centre) => center,
        (Entity::Circle { center, radius }, ShapeHandle::Rim) => center + DVec2::new(radius, 0.0),
        (Entity::Arc { .. }, ShapeHandle::Rim) => rim_point(e)?,
        (Entity::Line { a, b } | Entity::Axis { a, b }, ShapeHandle::Mid) => (a + b) * 0.5,
        (Entity::Line { a, b } | Entity::Axis { a, b }, ShapeHandle::End(i)) => {
            if i == 0 {
                a
            } else {
                b
            }
        }
        (Entity::Rect { a, b }, ShapeHandle::Corner(i)) => Entity::rect_corners(a, b)[i % 4],
        (Entity::Rect { a, b }, ShapeHandle::Edge(i)) => {
            let c = Entity::rect_corners(a, b);
            (c[i % 4] + c[(i + 1) % 4]) * 0.5
        }
        _ => return None,
    })
}

/// A point of the arc (its middle).
fn rim_point(e: &Entity) -> Option<DVec2> {
    if let Entity::Arc {
        center,
        radius,
        start,
        sweep,
    } = *e
    {
        let t = start + sweep * 0.5;
        Some(center + DVec2::new(t.cos(), t.sin()) * radius)
    } else {
        None
    }
}

impl ShapeDrag {
    /// The shape with the handle at `pos` (for a rim: with that radius; for a side: with the side there).
    fn shaped(&self, pos: DVec2) -> Entity {
        let radius_typed = self.edit.values().0.map(|d| d / 2.0).filter(|r| *r > 0.0);
        match (self.old, self.handle) {
            (Entity::Circle { radius, .. }, ShapeHandle::Centre) => Entity::Circle { center: pos, radius },
            (Entity::Circle { center, .. }, ShapeHandle::Rim) => Entity::Circle {
                center,
                radius: radius_typed.unwrap_or_else(|| center.distance(pos)),
            },
            (
                Entity::Arc {
                    radius: _,
                    start,
                    sweep,
                    ..
                },
                ShapeHandle::Centre,
            ) => Entity::Arc {
                center: pos,
                radius: if let Entity::Arc { radius, .. } = self.old {
                    radius
                } else {
                    0.0
                },
                start,
                sweep,
            },
            (
                Entity::Arc {
                    center, start, sweep, ..
                },
                ShapeHandle::Rim,
            ) => Entity::Arc {
                center,
                radius: radius_typed.unwrap_or_else(|| center.distance(pos)),
                start,
                sweep,
            },
            (Entity::Line { a, b }, ShapeHandle::Mid) => {
                let d = pos - (a + b) * 0.5;
                Entity::Line { a: a + d, b: b + d }
            }
            (Entity::Axis { a, b }, ShapeHandle::Mid) => {
                let d = pos - (a + b) * 0.5;
                Entity::Axis { a: a + d, b: b + d }
            }
            (Entity::Line { a, b }, ShapeHandle::End(i)) => {
                if i == 0 {
                    Entity::Line { a: pos, b }
                } else {
                    Entity::Line { a, b: pos }
                }
            }
            (Entity::Axis { a, b }, ShapeHandle::End(i)) => {
                if i == 0 {
                    Entity::Axis { a: pos, b }
                } else {
                    Entity::Axis { a, b: pos }
                }
            }
            (Entity::Rect { a, b }, ShapeHandle::Corner(i)) => {
                let opposite = Entity::rect_corners(a, b)[(i + 2) % 4];
                Entity::Rect { a: opposite, b: pos }
            }
            (Entity::Rect { mut a, mut b }, ShapeHandle::Edge(i)) => {
                match i % 4 {
                    0 => a.y = pos.y,
                    1 => b.x = pos.x,
                    2 => b.y = pos.y,
                    _ => a.x = pos.x,
                }
                Entity::Rect { a, b }
            }
            (e, _) => e,
        }
    }

    /// Takes the place under the pointer: the grid, the snaps (equal distance, middle of an edge) and the typed numbers give the place of the handle.
    pub fn update(&mut self, raw: DVec2, tol: f64, off: bool) {
        self.raw = raw;
        let mut pos = self.point0 + (raw - self.grab);
        self.label = None;
        if self.handle.moves_a_point() && !off {
            if let Some((p, l)) = crate::sketchmode::curve_snap(pos, tol / 8.0, &self.points, &self.curves) {
                pos = p;
                self.label = Some(l);
            }
        }
        if self.label.is_some() {
            // snapped to a point of the other shapes: the grid does not move it away
        } else if self.grid > 0.0 {
            pos = DVec2::new(
                (pos.x / self.grid).round() * self.grid,
                (pos.y / self.grid).round() * self.grid,
            );
        }
        self.snap = None;
        if self.handle.moves_a_point() {
            let dims = nearest_edges(pos, &self.edges);
            if let Some(d) = &dims {
                let (p, k) = d.snap(pos, tol);
                if k.is_some() {
                    (pos, self.snap) = (p, k);
                }
                let (a, b) = self.edit.values();
                if a.is_some() || b.is_some() {
                    pos = d.place(a, b);
                    self.snap = None;
                }
            }
            self.dims = if dims.is_some() {
                nearest_edges(pos, &self.edges)
            } else {
                None
            };
        } else {
            self.dims = None;
        }
        self.at = pos;
        self.new = self.shaped(pos);
    }
}

impl App {
    /// The straight edges of the face and the straight shapes of the sketches in the plane, without the shape that is dragged.
    fn edges_except(
        &self,
        plane: &PlaneDef,
        body: Option<BodyId>,
        sketch: SketchId,
        skip: EntityId,
    ) -> Vec<(DVec2, DVec2)> {
        let mut out = Vec::new();
        for s in self
            .doc
            .sketches()
            .iter()
            .filter(|s| s.visible && s.plane.same_plane(plane) && s.target == body)
        {
            for (id, e) in s.sketch.entities() {
                if s.id == sketch && *id == skip {
                    continue;
                }
                match *e {
                    Entity::Line { a, b } | Entity::Axis { a, b } => out.push((a, b)),
                    Entity::Rect { a, b } => {
                        let c = Entity::rect_corners(a, b);
                        out.extend((0..4).map(|i| (c[i], c[(i + 1) % 4])));
                    }
                    _ => {}
                }
            }
        }
        // the edges of the faces of the body in the plane
        if let Some(solid) = body.and_then(|b| self.doc.body(b)).map(|b| &b.solid) {
            let n = plane.normal.normalize();
            for f in solid.faces().take(600) {
                let Ok(i) = solid.face_info(f) else { continue };
                if i.kind != FaceKind::Plane
                    || i.normal.normalize().dot(n).abs() < 1.0 - 1e-6
                    || (i.origin - plane.origin).dot(n).abs() > 1e-5
                {
                    continue;
                }
                if let Ok(es) = solid.face_edges(f) {
                    out.extend(
                        es.into_iter()
                            .filter_map(|e| solid.edge_info(e).ok())
                            .filter(|i| i.kind == rolidium_kernel::EdgeKind::Line)
                            .map(|i| (plane.to_2d(i.start), plane.to_2d(i.end))),
                    );
                }
            }
        }
        out
    }

    /// The shape and the handle under the pointer: a centre, a middle, an end or a corner within 9 points, else a rim or a side within 7.
    fn shape_handle_at(&self, p: Pos2, rect: Rect) -> Option<(SketchId, EntityId, ShapeHandle)> {
        let mut best: Option<(u8, f32, SketchId, EntityId, ShapeHandle)> = None;
        let mut consider = |prio: u8, d: f32, s: SketchId, e: EntityId, h: ShapeHandle| {
            if best.as_ref().is_none_or(|b| (prio, d) < (b.0, b.1)) {
                best = Some((prio, d, s, e, h));
            }
        };
        let mut n = 0;
        for sk in self.doc.sketches().iter().filter(|s| s.visible) {
            let to_screen = |q: DVec2| self.project(sk.plane.to_3d(q).as_vec3(), rect);
            for (id, e) in sk.sketch.entities() {
                n += 1;
                if n > 3000 {
                    return best.map(|b| (b.2, b.3, b.4));
                }
                let mut points: Vec<ShapeHandle> = Vec::new();
                let mut rims: Vec<(ShapeHandle, Vec<Vec<DVec2>>)> = Vec::new();
                match *e {
                    Entity::Circle { .. } | Entity::Arc { .. } => {
                        points.push(ShapeHandle::Centre);
                        rims.push((ShapeHandle::Rim, e.polylines()));
                    }
                    Entity::Line { .. } | Entity::Axis { .. } => {
                        points.extend([ShapeHandle::End(0), ShapeHandle::End(1), ShapeHandle::Mid])
                    }
                    Entity::Rect { a, b } => {
                        points.extend((0..4).map(ShapeHandle::Corner));
                        let c = Entity::rect_corners(a, b);
                        rims.extend((0..4).map(|i| (ShapeHandle::Edge(i), vec![vec![c[i], c[(i + 1) % 4]]])));
                    }
                }
                for h in points {
                    if let Some(q) = handle_point(e, h).and_then(to_screen) {
                        let d = q.distance(p);
                        if d <= 9.0 {
                            consider(0, d, sk.id, *id, h);
                        }
                    }
                }
                for (h, lines) in rims {
                    for pl in lines {
                        let pts: Vec<Option<Pos2>> = pl.iter().map(|q| to_screen(*q)).collect();
                        for w in pts.windows(2) {
                            if let (Some(a), Some(b)) = (w[0], w[1]) {
                                let (d, _) = pick::dist_to_segment(p, a, b);
                                if d <= 7.0 {
                                    consider(1, d, sk.id, *id, h);
                                }
                            }
                        }
                    }
                }
            }
        }
        best.map(|b| (b.2, b.3, b.4))
    }

    /// A drag that starts on a handle of a shape of a sketch takes the shape along (Select tool, also in the sketch mode with no drawing tool).
    pub(super) fn shape_drag_input(&mut self, resp: &egui::Response, pointer: Option<Pos2>, rect: Rect) {
        if self.shape_drag.is_none()
            && self.hole_drag.is_none()
            && resp.drag_started_by(PointerButton::Primary)
            && self.idle()
        {
            let press = resp.ctx.input(|i| i.pointer.press_origin()).or(pointer);
            let Some(press) = press else { return };
            let Some((s, e, handle)) = self.shape_handle_at(press, rect) else {
                return;
            };
            let Some(sk) = self.doc.sketch(s) else { return };
            let Some(old) = sk.sketch.entity(e).copied() else {
                return;
            };
            let plane = sk.plane;
            let target = sk.target;
            let aspect = rect.width() / rect.height().max(1.0);
            let (o, d) = self.camera.ray(Self::ndc(press, rect), aspect);
            let Some((grab, _)) = plane.ray_hit(o.as_dvec3(), d.as_dvec3()) else {
                return;
            };
            let Some(point0) = handle_point(&old, handle) else {
                return;
            };
            let edges = self.edges_except(&plane, target, s, e);
            let curves = self.snap_curves_except(Some((s, e)));
            let points = self.snap_points_except(Some((s, e)));
            self.clear_selection();
            self.entity_sel = Some((s, e));
            resp.ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
            let grid = if self.settings.snap {
                self.settings.grid_step
            } else {
                0.0
            };
            self.shape_drag = Some(ShapeDrag {
                sketch: s,
                entity: e,
                old,
                handle,
                plane,
                grab,
                point0,
                edges,
                raw: grab,
                at: point0,
                dims: None,
                snap: None,
                new: old,
                edit: EdgeEdit::default(),
                grid,
                curves,
                points,
                label: None,
            });
            self.set_status(
                "Drag the shape, or type a number (Tab switches, Enter confirms, Esc cancels)",
                false,
            );
            self.layers_changed = true;
        }
        if self.shape_drag.is_none() {
            return;
        }
        if resp.dragged_by(PointerButton::Primary) {
            if let Some(p) = pointer {
                let aspect = rect.width() / rect.height().max(1.0);
                let (o, d) = self.camera.ray(Self::ndc(p, rect), aspect);
                let plane = self.shape_drag.as_ref().map(|h| h.plane);
                if let Some(pl) = plane {
                    let mpp = self.mm_per_px(
                        &pl,
                        self.shape_drag.as_ref().map_or(DVec2::ZERO, |h| h.point0),
                        rect,
                    );
                    let off = resp.ctx.input(|i| i.modifiers.shift);
                    if let (Some(h), Some((hit, _))) =
                        (&mut self.shape_drag, pl.ray_hit(o.as_dvec3(), d.as_dvec3()))
                    {
                        h.update(hit, 8.0 * mpp, off);
                    }
                }
            }
            self.layers_changed = true;
        }
        if resp.drag_stopped_by(PointerButton::Primary) {
            if self.shape_drag.as_ref().is_some_and(|h| h.edit.any_typed()) {
                self.set_status(
                    "Press Enter to put the shape at the typed numbers, Esc to cancel",
                    false,
                );
            } else {
                self.commit_shape_drag();
            }
        }
    }

    pub(super) fn cancel_shape_drag(&mut self) {
        if self.shape_drag.take().is_some() {
            self.layers_changed = true;
            self.set_status("Cancelled", false);
        }
    }

    pub(super) fn commit_shape_drag(&mut self) {
        let Some(h) = self.shape_drag.take() else { return };
        self.layers_changed = true;
        if h.new == h.old {
            self.set_status("The shape stays where it is", false);
            return;
        }
        if let Err(e) = h.new.validate() {
            self.set_status(tr(&e.to_string()), true);
            return;
        }
        self.submit(Command::ReplaceEntity {
            sketch: h.sketch,
            entity: h.entity,
            new: h.new,
        });
    }

    /// The shape at the place it is dragged to.
    pub(super) fn shape_drag_layers(&self) -> Vec<PreviewDraw> {
        let Some(h) = &self.shape_drag else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        for pl in h.new.polylines() {
            for w in pl.windows(2) {
                lines.push((h.plane.to_3d(w[0]) + h.plane.normal.normalize() * 0.02).as_vec3());
                lines.push((h.plane.to_3d(w[1]) + h.plane.normal.normalize() * 0.02).as_vec3());
            }
        }
        vec![PreviewDraw {
            thick: lines,
            thick_color: [1.0, 0.85, 0.35, 1.0],
            ..Default::default()
        }]
    }

    pub(super) fn shape_drag_paint(&self, ui: &egui::Ui, rect: Rect) {
        let Some(h) = &self.shape_drag else { return };
        if let Some(l) = h.label {
            if let Some(p) = self.project(h.plane.to_3d(h.at).as_vec3(), rect) {
                let painter = ui.painter().with_clip_rect(rect);
                let col = crate::theme::rgb(120, 255, 160);
                painter.circle_stroke(p, 7.0, Stroke::new(2.0, col));
                painter.text(
                    p + Vec2::new(14.0, 10.0),
                    egui::Align2::LEFT_TOP,
                    tr(l.text()),
                    egui::FontId::proportional(13.0),
                    col,
                );
            }
        }
        if let Some(d) = &h.dims {
            self.paint_edge_dims(ui, rect, &h.plane, h.at, d, &h.edit, h.snap);
        } else if h.handle == ShapeHandle::Rim {
            let radius = match h.new {
                Entity::Circle { radius, .. } | Entity::Arc { radius, .. } => radius,
                _ => 0.0,
            };
            if let Some(p) = self.project(h.plane.to_3d(h.at).as_vec3(), rect) {
                let typed = !h.edit.text[0].is_empty();
                let txt = if typed {
                    h.edit.text[0].clone()
                } else {
                    format!("{:.2}", radius * 2.0)
                };
                let painter = ui.painter().with_clip_rect(rect);
                let r = Rect::from_center_size(p + Vec2::new(24.0, -14.0), Vec2::new(70.0, 19.0));
                painter.rect_filled(r, 3.0, Color32::from_black_alpha(215));
                painter.rect_stroke(
                    r,
                    3.0,
                    Stroke::new(1.5, crate::theme::rgb(255, 215, 90)),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("⌀ {txt}"),
                    egui::FontId::proportional(13.0),
                    Color32::from_gray(230),
                );
            }
        }
    }
}
