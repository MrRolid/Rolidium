// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! "Move sketch plane": an arrow on the plane along its normal; drag it, type a distance, or press U and
//! click a vertex or a face to bring the plane to it.

use super::*;
use crate::i18n::{parse_num, tr};

pub struct PlaneMove {
    /// Offset before the move started (Esc restores it).
    pub start: f64,
    /// Where the drag started (local pixels), the offset then, and the screen direction/scale of the normal.
    drag: Option<(Vec2, f64, Vec2, f32)>,
    pub typed: String,
    /// "Up to": the next click names the target.
    pub waiting_target: bool,
}

const ARROW_PX: f32 = 110.0;

impl App {
    pub(super) fn begin_plane_move(&mut self) {
        let Some(sm) = &self.sk else { return };
        self.plane_move = Some(PlaneMove {
            start: sm.offset,
            drag: None,
            typed: String::new(),
            waiting_target: false,
        });
        // the view turns smoothly into the isometric one, so that the arrow and the shift of the plane can be seen
        // (a click or a drag interrupts the turn, as everywhere)
        if self.settings.auto_turn_view {
            self.animate_standard_view(StandardView::Isometric);
        }
        self.set_status(
            "Move sketch plane: drag the arrow, type a distance and press Enter, or press U and click a vertex or a face",
            false,
        );
    }

    fn plane_arrow(&self, rect: Rect) -> Option<(Pos2, Pos2)> {
        let sm = self.sk.as_ref()?;
        let c = sm.focus;
        let n = sm.base_plane.normal.as_vec3();
        let ppm = rect.height() / (2.0 * self.camera.distance * (self.camera.fov_y * 0.5).tan());
        let tip = c + n * (ARROW_PX / ppm);
        Some((self.project(c, rect)?, self.project(tip, rect)?))
    }

    /// After the move (confirmed or cancelled) the view turns back square to the sketch plane.
    fn turn_back_to_plane(&mut self) {
        if !self.settings.auto_turn_view {
            return;
        }
        if let Some(sm) = &self.sk {
            let to = super::viewanim::plane_view(&self.camera, sm.base_plane.normal.as_vec3());
            self.animate_to(to);
        }
    }

    pub(super) fn cancel_plane_move(&mut self) {
        if let Some(pm) = self.plane_move.take() {
            self.turn_back_to_plane();
            if let Some(sm) = &mut self.sk {
                sm.set_offset(pm.start);
                sm.offset_text = fmt_num(pm.start);
            }
            self.set_status("Cancelled", false);
        }
    }

    pub(super) fn finish_plane_move(&mut self) {
        let Some(pm) = self.plane_move.take() else { return };
        self.turn_back_to_plane();
        let typed = parse_num(&pm.typed);
        if let Some(sm) = &mut self.sk {
            if let Some(v) = typed {
                sm.set_offset(v);
            }
            sm.offset_text = fmt_num(sm.offset);
            // drawing goes to the sketch that lies on the new plane; none yet means a new sketch is made
            sm.sketch = self
                .doc
                .sketches()
                .iter()
                .find(|s| s.target == sm.body && s.plane.same_plane(&sm.plane))
                .map(|s| s.id);
        }
        let v = self.sk.as_ref().map_or(0.0, |s| s.offset);
        self.set_status(
            trf("The sketch plane is {} mm from its base", &[fmt_num(v)]),
            false,
        );
    }

    pub(super) fn plane_move_input(&mut self, resp: &egui::Response, pointer: Option<Pos2>, rect: Rect) {
        let ctx = resp.ctx.clone();
        // waiting for the target of "up to"
        if self.plane_move.as_ref().is_some_and(|p| p.waiting_target) {
            if resp.clicked_by(PointerButton::Primary) {
                if let Some(p) = pointer {
                    if let (Some(t), Some(sm)) = (self.up_to_target(p, rect), self.sk.as_ref()) {
                        let (o, n) = (sm.base_plane.origin, sm.base_plane.normal);
                        let raw = snap::up_to_distance(o, n, t);
                        let planes: Vec<DVec3> = self
                            .meshes
                            .iter()
                            .flat_map(|m| m.faces.iter())
                            .filter(|f| f.kind == FaceKind::Plane && f.normal.dot(n).abs() > 0.9999)
                            .map(|f| f.origin)
                            .collect();
                        let v = snap::align_to_planes(raw, o, n, planes);
                        if let Some(sm) = &mut self.sk {
                            sm.set_offset(v);
                            sm.offset_text = fmt_num(v);
                        }
                        if let Some(pm) = &mut self.plane_move {
                            pm.waiting_target = false;
                        }
                        self.finish_plane_move();
                    } else {
                        self.set_status("Click on the model: a face, an edge or a vertex", false);
                    }
                }
            }
            return;
        }
        let shift = ctx.input(|i| i.modifiers.shift);
        let (step, grid_on) = (self.settings.grid_step, self.settings.snap);
        if resp.drag_started_by(PointerButton::Primary) {
            if let (Some(press), Some((a, b))) =
                (ctx.input(|i| i.pointer.press_origin()), self.plane_arrow(rect))
            {
                let ab = b - a;
                let t = ((press - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                if (press - (a + ab * t)).length() <= 12.0 {
                    let local = press - rect.min;
                    let dir = ab.normalized();
                    let ppm = ab.length() / ARROW_PX * ((self.camera.distance * 0.0) + 1.0);
                    let off = self.sk.as_ref().map_or(0.0, |s| s.offset);
                    // pixels per mm along the arrow: its length on screen divided by its length in mm
                    let mm_len = ARROW_PX
                        / (rect.height() / (2.0 * self.camera.distance * (self.camera.fov_y * 0.5).tan()));
                    let px_per_mm = if mm_len > 1e-6 { ab.length() / mm_len } else { ppm };
                    if let Some(pm) = &mut self.plane_move {
                        pm.drag = Some((Vec2::new(local.x, local.y), off, dir, px_per_mm));
                        pm.typed.clear();
                    }
                }
            }
        }
        if resp.dragged_by(PointerButton::Primary) {
            if let (Some(p), Some(pm)) = (pointer, &self.plane_move) {
                if let Some((start, off0, dir, ppm)) = pm.drag {
                    let moved = Vec2::new(p.x - rect.min.x, p.y - rect.min.y) - start;
                    let mm = (moved.dot(dir) / ppm.max(1e-3)) as f64;
                    let raw = off0 + mm;
                    let v = if shift {
                        (raw * 100.0).round() / 100.0
                    } else if grid_on && step > 0.0 {
                        (raw / step).round() * step
                    } else {
                        (raw * 100.0).round() / 100.0
                    };
                    if let Some(sm) = &mut self.sk {
                        sm.set_offset(v);
                        sm.offset_text = fmt_num(v);
                    }
                }
            }
        }
        if resp.drag_stopped_by(PointerButton::Primary) {
            if let Some(pm) = &mut self.plane_move {
                if pm.drag.take().is_some() {
                    self.finish_plane_move();
                }
            }
        }
    }

    /// The arrow, its distance label and the hint, drawn over the viewport.
    pub(super) fn plane_move_paint(&self, ui: &egui::Ui, rect: Rect) {
        let Some(pm) = &self.plane_move else { return };
        let Some((a, b)) = self.plane_arrow(rect) else {
            return;
        };
        let painter = ui.painter().with_clip_rect(rect);
        let col = crate::theme::rgb(255, 200, 80);
        painter.line_segment([a, b], Stroke::new(4.0, col));
        let dir = (b - a).normalized();
        let n = Vec2::new(-dir.y, dir.x);
        painter.add(egui::Shape::convex_polygon(
            vec![b + dir * 12.0, b - dir * 3.0 + n * 8.0, b - dir * 3.0 - n * 8.0],
            col,
            Stroke::NONE,
        ));
        let off = self.sk.as_ref().map_or(0.0, |s| s.offset);
        let text = if pm.typed.is_empty() {
            format!("{} mm", fmt_signed(off))
        } else {
            format!("{} mm", pm.typed)
        };
        painter.text(
            b + Vec2::new(14.0, -10.0),
            egui::Align2::LEFT_BOTTOM,
            text,
            egui::FontId::proportional(16.0),
            col,
        );
        if pm.waiting_target {
            painter.text(
                a + Vec2::new(14.0, 14.0),
                egui::Align2::LEFT_TOP,
                tr("click a vertex, an edge or a face"),
                egui::FontId::proportional(13.0),
                col,
            );
        }
    }
}
