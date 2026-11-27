// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Selection by a rectangle in the Select tool: a drag with the left button from an empty place. From left to right (a full rectangle) it selects what lies inside the
//! rectangle entirely, from right to left (a dashed rectangle of another colour) also what the rectangle only crosses; Ctrl adds to what is selected. It works for the
//! bodies in the 3D view and for the shapes of a sketch. The camera is turned and moved with the middle and the right button, so the left button is free for this.

use super::*;
use egui::Shape;

/// A rectangle that is being dragged.
#[derive(Debug, Clone, Copy)]
pub(super) struct BoxSel {
    pub start: Pos2,
    pub now: Pos2,
}

impl BoxSel {
    /// From left to right: only what is inside counts.
    pub fn window(&self) -> bool {
        self.now.x >= self.start.x
    }
    pub fn rect(&self) -> Rect {
        Rect::from_two_pos(self.start, self.now)
    }
}

/// Whether the segment `a`-`b` touches the rectangle.
fn segment_hits(a: Pos2, b: Pos2, r: Rect) -> bool {
    if r.contains(a) || r.contains(b) {
        return true;
    }
    // Liang-Barsky
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [
        (-dx, a.x - r.min.x),
        (dx, r.max.x - a.x),
        (-dy, a.y - r.min.y),
        (dy, r.max.y - a.y),
    ] {
        if p.abs() < 1e-9 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

/// Whether the triangle (on the screen) and the rectangle share a point.
fn triangle_hits(t: [Pos2; 3], r: Rect) -> bool {
    let (lo_x, hi_x) = (t[0].x.min(t[1].x).min(t[2].x), t[0].x.max(t[1].x).max(t[2].x));
    let (lo_y, hi_y) = (t[0].y.min(t[1].y).min(t[2].y), t[0].y.max(t[1].y).max(t[2].y));
    if hi_x < r.min.x || lo_x > r.max.x || hi_y < r.min.y || lo_y > r.max.y {
        return false;
    }
    if t.iter().any(|p| r.contains(*p)) {
        return true;
    }
    if (0..3).any(|k| segment_hits(t[k], t[(k + 1) % 3], r)) {
        return true;
    }
    // the rectangle inside the triangle
    let c = r.center();
    let side = |a: Pos2, b: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let (s0, s1, s2) = (side(t[0], t[1]), side(t[1], t[2]), side(t[2], t[0]));
    (s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0) || (s0 <= 0.0 && s1 <= 0.0 && s2 <= 0.0)
}

impl App {
    /// Starts, follows and ends the rectangle. `empty`: the pointer is over nothing that can be picked.
    pub(super) fn box_select_input(&mut self, resp: &egui::Response, rect: Rect, empty: bool) {
        let (ctrl, press) = resp
            .ctx
            .input(|i| (i.modifiers.command || i.modifiers.shift, i.pointer.press_origin()));
        if self.box_sel.is_none()
            && empty
            && resp.drag_started_by(PointerButton::Primary)
            && self.hole_drag.is_none()
            && self.shape_drag.is_none()
            && self.pull.is_none()
        {
            if let Some(p) = press.or(resp.hover_pos()) {
                self.box_sel = Some(BoxSel { start: p, now: p });
            }
        }
        let Some(b) = &mut self.box_sel else { return };
        if let Some(p) = resp.ctx.input(|i| i.pointer.latest_pos()) {
            b.now = p;
        }
        if resp.drag_stopped_by(PointerButton::Primary) {
            let b = *b;
            self.box_sel = None;
            // a click without a drag is no rectangle
            if (b.now - b.start).length() >= 4.0 {
                self.box_select_apply(b, ctrl, rect);
            }
        }
        resp.ctx.request_repaint();
    }

    /// The rectangle on the screen: a full line for what lies inside, a dashed one of another colour for what it crosses.
    pub(super) fn box_select_paint(&self, ui: &egui::Ui) {
        let Some(b) = &self.box_sel else { return };
        let r = b.rect();
        let painter = ui.painter();
        if b.window() {
            let c = Color32::from_rgb(80, 150, 255);
            painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(80, 150, 255, 30));
            painter.rect_stroke(r, 0.0, Stroke::new(1.5, c), egui::StrokeKind::Inside);
        } else {
            let c = Color32::from_rgb(90, 210, 120);
            painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(90, 210, 120, 26));
            let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            painter.add(Shape::dashed_line(&pts, Stroke::new(1.5, c), 6.0, 4.0));
        }
    }

    fn box_select_apply(&mut self, b: BoxSel, add: bool, rect: Rect) {
        let r = Rect::from_min_max(b.rect().min - rect.min.to_vec2(), b.rect().max - rect.min.to_vec2());
        // the points are projected into the viewport (its own origin), so the rectangle is moved there too
        let window = b.window();
        if self.sk.is_some() {
            self.box_select_shapes(r, window, add, rect);
        } else {
            self.box_select_bodies(r, window, add, rect);
        }
        self.layers_changed = true;
    }

    fn box_select_bodies(&mut self, r: Rect, window: bool, add: bool, vrect: Rect) {
        let proj = |app: &App, p: Vec3| app.project(p, vrect).map(|q| q - vrect.min.to_vec2());
        let mut hit: Vec<BodyId> = Vec::new();
        for m in self.meshes.iter() {
            if self.doc.body(m.id).is_some_and(|b| !b.visible) || self.doc.mesh(m.id).is_some_and(|b| !b.visible) {
                continue;
            }
            let pts: Vec<Option<Pos2>> = m.mesh.positions.iter().map(|p| proj(self, *p)).collect();
            let ok = if window {
                // every vertex that is seen lies inside, and at least one is seen
                let seen: Vec<Pos2> = pts.iter().flatten().copied().collect();
                !seen.is_empty() && seen.len() == pts.len() && seen.iter().all(|p| r.contains(*p))
            } else {
                m.mesh.indices.as_chunks::<3>().0.iter().any(|t| {
                    match (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]) {
                        (Some(a), Some(b), Some(c)) => triangle_hits([a, b, c], r),
                        _ => false,
                    }
                })
            };
            if ok {
                hit.push(m.id);
            }
        }
        let mut keep = if add { self.move_sel.clone() } else { Vec::new() };
        if !add {
            self.clear_selection();
        } else {
            self.selection.clear();
            self.region_sel = None;
            self.region_multi.clear();
            self.edge_sel.clear();
            self.entity_sel = None;
            self.entity_multi.clear();
            if let Some(b) = self.body_sel.take() {
                if !keep.contains(&b) {
                    keep.push(b);
                }
            }
        }
        for b in &hit {
            if let Some(i) = keep.iter().position(|x| x == b).filter(|_| add && false) {
                keep.remove(i);
            } else if !keep.contains(b) {
                keep.push(*b);
            }
        }
        self.body_sel = keep.last().copied();
        self.move_sel = keep;
        let n = hit.len();
        self.set_status(
            if n == 0 {
                tr("Nothing in the rectangle")
            } else {
                trf("{} bodies selected", &[self.move_sel.len().to_string()])
            },
            false,
        );
    }

    fn box_select_shapes(&mut self, r: Rect, window: bool, add: bool, vrect: Rect) {
        let mut hit: Vec<(SketchId, EntityId)> = Vec::new();
        for s in self.doc.sketches().iter().filter(|s| s.visible) {
            for (id, e) in s.sketch.entities() {
                let mut pts: Vec<Vec<Option<Pos2>>> = Vec::new();
                for pl in e.polylines() {
                    pts.push(
                        pl.iter()
                            .map(|p| self.project(s.plane.to_3d(*p).as_vec3(), vrect).map(|q| q - vrect.min.to_vec2()))
                            .collect(),
                    );
                }
                let all: Vec<Option<Pos2>> = pts.iter().flatten().copied().collect();
                let ok = if window {
                    !all.is_empty() && all.iter().all(|p| p.is_some_and(|p| r.contains(p)))
                } else {
                    pts.iter().any(|pl| {
                        pl.windows(2)
                            .any(|w| matches!((w[0], w[1]), (Some(a), Some(b)) if segment_hits(a, b, r)))
                    })
                };
                if ok {
                    hit.push((s.id, *id));
                }
            }
        }
        let mut keep = if add { self.entity_multi.clone() } else { Vec::new() };
        if add {
            if let Some(e) = self.entity_sel {
                if !keep.contains(&e) {
                    keep.push(e);
                }
            }
        } else {
            self.clear_selection();
        }
        for h in &hit {
            if !keep.contains(h) {
                keep.push(*h);
            }
        }
        self.entity_sel = keep.last().copied();
        self.entity_multi = keep;
        let n = self.entity_multi.len();
        self.set_status(
            if hit.is_empty() {
                tr("Nothing in the rectangle")
            } else {
                trf("{} shapes selected", &[n.to_string()])
            },
            false,
        );
    }
}
