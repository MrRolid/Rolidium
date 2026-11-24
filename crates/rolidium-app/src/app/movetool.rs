// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The Move tool: a manipulator at the centre of the selected bodies (three arrows, three rings),
//! a cheap box preview while dragging and a panel for exact values.

use super::*;
use crate::i18n::{parse_num, tr};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Handle {
    Arrow(usize),
    Ring(usize),
}

impl MoveDrag {
    pub fn push_typed(&mut self, ch: char) {
        if ch == '-' && !self.typed.is_empty() {
            return;
        }
        self.typed.push(ch);
    }
    pub fn pop_typed(&mut self) {
        self.typed.pop();
    }
}

pub struct MoveDrag {
    handle: Handle,
    start: Vec2,
    /// Screen direction (unit) of the axis and pixels per mm along it (arrows).
    axis_dir: Vec2,
    px_per_mm: f32,
    /// Angle of the pointer around the centre at the start (rings), screen space.
    start_angle: f32,
    sign: f32,
    pub value: f64,
    pub typed: String,
}

#[derive(Default)]
pub struct MoveUi {
    pub drag: Option<MoveDrag>,
    pub hover: Option<Handle>,
    pub fields: [String; 4],
    pub axis: usize,
    pub copy: bool,
}

const AXES: [Vec3; 3] = [Vec3::X, Vec3::Y, Vec3::Z];
const ARROW_PX: f32 = 120.0;
const RING_PX: f32 = 80.0;
const COLORS: [Color32; 3] = [
    Color32::from_rgb(230, 80, 80),
    Color32::from_rgb(90, 200, 90),
    Color32::from_rgb(90, 140, 240),
];

/// The two unit vectors spanning the plane perpendicular to axis `i`.
fn ring_basis(i: usize) -> (Vec3, Vec3) {
    match i {
        0 => (Vec3::Y, Vec3::Z),
        1 => (Vec3::Z, Vec3::X),
        _ => (Vec3::X, Vec3::Y),
    }
}

impl App {
    pub(super) fn move_toggle(&mut self, id: BodyId) {
        if let Some(i) = self.move_sel.iter().position(|b| *b == id) {
            self.move_sel.remove(i);
        } else {
            self.move_sel.push(id);
        }
    }

    /// Bounding box of the selected bodies.
    pub(super) fn move_bbox(&self) -> Option<(Vec3, Vec3)> {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let mut any = false;
        // an end face of a loft: the box of that face (the gizmo stands on it)
        if let Some((b, sk)) = self.move_end {
            if let Some(m) = self.entry(b) {
                for (i, meta) in m.faces.iter().enumerate() {
                    if self
                        .doc
                        .end_face(b, FaceId(i as u32))
                        .is_some_and(|e| e.sketch == sk)
                    {
                        lo = lo.min(meta.bounds.0);
                        hi = hi.max(meta.bounds.1);
                        any = true;
                    }
                }
            }
            if any {
                return Some((lo, hi));
            }
        }
        for m in self.meshes.iter().filter(|m| self.move_sel.contains(&m.id)) {
            for p in &m.mesh.positions {
                lo = lo.min(*p);
                hi = hi.max(*p);
                any = true;
            }
        }
        for id in &self.move_curves {
            for p in self.curve_points(*id) {
                lo = lo.min(*p);
                hi = hi.max(*p);
                any = true;
            }
        }
        any.then_some((lo, hi))
    }

    fn move_center(&self) -> Option<Vec3> {
        self.move_bbox().map(|(a, b)| (a + b) * 0.5)
    }

    fn px_per_mm0(&self, rect: Rect) -> f32 {
        rect.height() / (2.0 * self.camera.distance * (self.camera.fov_y * 0.5).tan())
    }

    /// Screen points (absolute) of a ring around axis `i`.
    fn ring_points(&self, c: Vec3, i: usize, rect: Rect) -> Vec<Pos2> {
        let r = RING_PX / self.px_per_mm0(rect);
        let (u, v) = ring_basis(i);
        (0..=48)
            .filter_map(|k| {
                let a = k as f32 / 48.0 * std::f32::consts::TAU;
                self.project(c + (u * a.cos() + v * a.sin()) * r, rect)
            })
            .collect()
    }

    #[allow(clippy::needless_range_loop)]
    fn handle_at(&self, p: Pos2, rect: Rect) -> Option<Handle> {
        let c = self.move_center()?;
        let cs = self.project(c, rect)?;
        let len = ARROW_PX / self.px_per_mm0(rect);
        let mut best: Option<(f32, Handle)> = None;
        for i in 0..3 {
            if let Some(tip) = self.project(c + AXES[i] * len, rect) {
                let ab = tip - cs;
                let t = ((p - cs).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                let d = (p - (cs + ab * t)).length();
                if d <= 8.0 && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, Handle::Arrow(i)));
                }
            }
            let pts = self.ring_points(c, i, rect);
            for w in pts.windows(2) {
                let ab = w[1] - w[0];
                let t = ((p - w[0]).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                let d = (p - (w[0] + ab * t)).length();
                if d <= 6.0 && best.is_none_or(|(bd, _)| d < bd - 1.0) {
                    best = Some((d, Handle::Ring(i)));
                }
            }
        }
        best.map(|b| b.1)
    }

    /// Translation, axis and angle that the drag (or the typed value) stands for.
    pub(super) fn move_motion(&self) -> (DVec3, usize, f64) {
        let Some(d) = &self.move_ui.drag else {
            return (DVec3::ZERO, 2, 0.0);
        };
        let v = if d.typed.is_empty() {
            d.value
        } else {
            match d.handle {
                Handle::Arrow(_) => parse_num(&d.typed).unwrap_or(0.0),
                Handle::Ring(_) => parse_num(&d.typed).unwrap_or(0.0).to_radians(),
            }
        };
        match d.handle {
            Handle::Arrow(i) => (AXES[i].as_dvec3() * v, i, 0.0),
            Handle::Ring(i) => (DVec3::ZERO, i, v),
        }
    }

    pub(super) fn move_input(
        &mut self,
        resp: &egui::Response,
        pointer: Option<Pos2>,
        rect: Rect,
        hovered_face: Option<(BodyId, u32)>,
    ) {
        let ctx = resp.ctx.clone();
        let local_rect = Rect::from_min_size(Pos2::ZERO, rect.size());
        let hover = pointer
            .filter(|_| resp.hovered() && self.move_ui.drag.is_none())
            .and_then(|p| self.handle_at(p, rect));
        if hover != self.move_ui.hover {
            self.move_ui.hover = hover;
        }
        if resp.drag_started_by(PointerButton::Primary) && self.move_ui.drag.is_none() && self.idle() {
            let press = ctx.input(|i| i.pointer.press_origin());
            if let (Some(p), Some(c)) = (press.and_then(|p| self.handle_at(p, rect)), self.move_center()) {
                let start = p;
                let _ = start;
                let press = press.unwrap();
                let local = press - rect.min;
                let handle = p;
                let (axis_dir, px_per_mm) = match handle {
                    Handle::Arrow(i) => self.axis_mapping(c, AXES[i], local_rect),
                    Handle::Ring(_) => (Vec2::X, 1.0),
                };
                let cs = self.project(c, rect).unwrap_or(rect.center());
                let start_angle = (press - cs).angle();
                let eye_dot = match handle {
                    Handle::Ring(i) => AXES[i].dot(self.camera.eye() - c),
                    _ => 1.0,
                };
                self.move_ui.drag = Some(MoveDrag {
                    handle,
                    start: Vec2::new(local.x, local.y),
                    axis_dir,
                    px_per_mm,
                    start_angle,
                    sign: if eye_dot >= 0.0 { 1.0 } else { -1.0 },
                    value: 0.0,
                    typed: String::new(),
                });
            }
        }
        if resp.dragged_by(PointerButton::Primary) {
            let shift = ctx.input(|i| i.modifiers.shift);
            let (step, grid_on) = (self.settings.grid_step, self.settings.snap);
            let centre = self.move_center().and_then(|c| self.project(c, rect));
            if let (Some(d), Some(p)) = (&mut self.move_ui.drag, pointer) {
                if d.typed.is_empty() {
                    match d.handle {
                        Handle::Arrow(_) => {
                            let moved = Vec2::new(p.x - rect.min.x, p.y - rect.min.y) - d.start;
                            let mm = (moved.dot(d.axis_dir) / d.px_per_mm.max(1e-3)) as f64;
                            d.value = if shift {
                                (mm * 100.0).round() / 100.0
                            } else if grid_on && step > 0.0 {
                                (mm / step).round() * step
                            } else {
                                (mm * 100.0).round() / 100.0
                            };
                        }
                        Handle::Ring(_) => {
                            if let Some(cs) = centre {
                                let mut da = ((p - cs).angle() - d.start_angle) * d.sign;
                                da = (da + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                                    - std::f32::consts::PI;
                                let deg = (-da).to_degrees() as f64;
                                d.value = if shift {
                                    (deg * 10.0).round() / 10.0
                                } else {
                                    (deg / 5.0).round() * 5.0
                                }
                                .to_radians();
                            }
                        }
                    }
                }
            }
            self.layers_changed = true;
        }
        if resp.drag_stopped_by(PointerButton::Primary) {
            if let Some(d) = &self.move_ui.drag {
                if d.typed.is_empty() {
                    self.commit_move();
                } else {
                    self.set_status("Press Enter to apply the typed value, Esc to cancel", false);
                }
            }
        }
        // selecting bodies
        if resp.clicked_by(PointerButton::Primary) && self.move_ui.drag.is_none() {
            let add = ctx.input(|i| i.modifiers.command || i.modifiers.shift);
            let on_handle = pointer.and_then(|p| self.handle_at(p, rect)).is_some();
            if !on_handle {
                self.move_end = None;
                // an end face of a loft is moved by moving the sketch of its profile: the body follows
                if let (Some((b, f)), false) = (hovered_face, add) {
                    if let Some(ef) = self
                        .doc
                        .end_face(b, FaceId(f))
                        .filter(|e| e.kind == rolidium_doc::EndKind::Loft)
                    {
                        self.move_end = Some((b, ef.sketch));
                        self.move_sel = vec![b];
                        self.move_curves.clear();
                        self.body_sel = None;
                        self.layers_changed = true;
                        self.set_status(
                            "End face: the sketch of its profile moves in any direction and the body follows. Turning it is not possible here",
                            false,
                        );
                        return;
                    }
                }
                // a curve under the pointer is moved like a body
                if let Some(Obj::Curve(c)) = self.hover_obj {
                    if add && !self.move_curves.contains(&c) {
                        self.move_curves.push(c);
                    } else if !add {
                        self.move_curves = vec![c];
                        self.move_sel.clear();
                    }
                } else {
                    match hovered_face {
                        Some((b, _)) if add => self.move_toggle(b),
                        Some((b, _)) => {
                            self.move_sel = vec![b];
                            self.move_curves.clear();
                        }
                        None if !add => {
                            self.move_sel.clear();
                            self.move_curves.clear();
                        }
                        None => {}
                    }
                }
                self.body_sel = None;
                self.layers_changed = true;
            }
        }
        let _ = local_rect;
    }

    pub(super) fn cancel_move(&mut self) {
        if self.move_ui.drag.take().is_some() {
            self.layers_changed = true;
            self.set_status("Move cancelled", false);
        }
    }

    /// Runs the move that the drag (or the typed value) stands for.
    pub(super) fn commit_move(&mut self) {
        let (t, axis, angle) = self.move_motion();
        self.move_ui.drag = None;
        self.layers_changed = true;
        if t.length() < 1e-9 && angle.abs() < 1e-9 {
            self.set_status("Nothing to move", false);
            return;
        }
        self.submit_move(t, axis, angle);
    }

    pub(super) fn submit_move(&mut self, t: DVec3, axis: usize, angle: f64) {
        let Some(c) = self.move_center() else { return };
        if !self.idle() {
            self.set_status("Still working on the previous operation", true);
            return;
        }
        self.keep_move_sel = self.move_sel.clone();
        self.pending = None;
        if let Some((_, sketch)) = self.move_end {
            if angle.abs() > 1e-9 {
                self.set_status(
                    "An end face cannot be turned here: turn the sketch of its profile in the sketch mode",
                    true,
                );
                return;
            }
            self.submit(Command::MoveSketch {
                sketch,
                translation: t,
            });
            self.set_status("Computing…", false);
            return;
        }
        if self.move_sel.is_empty() && !self.move_curves.is_empty() {
            self.submit(Command::MoveCurves {
                curves: self.move_curves.clone(),
                translation: t,
                pivot: c.as_dvec3(),
                axis: AXES[axis].as_dvec3(),
                angle,
            });
            self.set_status("Computing…", false);
            return;
        }
        self.submit(Command::Move {
            bodies: self.move_sel.clone(),
            translation: t,
            pivot: c.as_dvec3(),
            axis: AXES[axis].as_dvec3(),
            angle,
            copy: self.move_ui.copy,
        });
        self.set_status("Computing…", false);
    }

    /// The box of the selection, moved by the current drag: the cheap preview.
    pub(super) fn move_preview_lines(&self) -> Vec<Vec3> {
        let Some((lo, hi)) = self.move_bbox() else {
            return Vec::new();
        };
        let (t, axis, angle) = self.move_motion();
        let c = (lo + hi) * 0.5;
        let q = glam::Quat::from_axis_angle(AXES[axis], angle as f32);
        let tf = |p: Vec3| c + q * (p - c) + t.as_vec3();
        let corner = |i: usize| {
            Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            )
        };
        let mut out = Vec::new();
        for i in 0..8usize {
            for b in [1usize, 2, 4] {
                if i & b == 0 {
                    out.push(tf(corner(i)));
                    out.push(tf(corner(i | b)));
                }
            }
        }
        out
    }

    pub(super) fn move_paint(&self, ui: &egui::Ui, rect: Rect) {
        if self.tool != Tool::Move {
            return;
        }
        let Some(c) = self.move_center() else { return };
        let Some(cs) = self.project(c, rect) else { return };
        let painter = ui.painter().with_clip_rect(rect);
        let len = ARROW_PX / self.px_per_mm0(rect);
        let active = self
            .move_ui
            .drag
            .as_ref()
            .map(|d| d.handle)
            .or(self.move_ui.hover);
        for i in 0..3 {
            let on = active == Some(Handle::Arrow(i));
            let col = if on {
                crate::theme::rgb(255, 235, 120)
            } else {
                COLORS[i]
            };
            if let Some(tip) = self.project(c + AXES[i] * len, rect) {
                painter.line_segment([cs, tip], Stroke::new(if on { 4.0 } else { 2.5 }, col));
                let dir = (tip - cs).normalized();
                let n = Vec2::new(-dir.y, dir.x);
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        tip + dir * 10.0,
                        tip - dir * 4.0 + n * 6.0,
                        tip - dir * 4.0 - n * 6.0,
                    ],
                    col,
                    Stroke::NONE,
                ));
            }
            let on = active == Some(Handle::Ring(i));
            let col = if on {
                crate::theme::rgb(255, 235, 120)
            } else {
                COLORS[i].gamma_multiply(0.85)
            };
            let pts = self.ring_points(c, i, rect);
            if pts.len() > 2 {
                painter.add(egui::Shape::line(
                    pts,
                    Stroke::new(if on { 3.5 } else { 1.8 }, col),
                ));
            }
        }
        if let Some(d) = &self.move_ui.drag {
            let text = match d.handle {
                Handle::Arrow(_) => format!(
                    "{} mm",
                    if d.typed.is_empty() {
                        fmt_signed(d.value)
                    } else {
                        d.typed.clone()
                    }
                ),
                Handle::Ring(_) => format!(
                    "{}°",
                    if d.typed.is_empty() {
                        fmt_signed(d.value.to_degrees())
                    } else {
                        d.typed.clone()
                    }
                ),
            };
            painter.text(
                cs + Vec2::new(18.0, -26.0),
                egui::Align2::LEFT_BOTTOM,
                text,
                egui::FontId::proportional(16.0),
                crate::theme::rgb(255, 230, 140),
            );
        }
    }

    #[allow(deprecated)]
    pub(super) fn move_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        if self.tool != Tool::Move || (self.move_sel.is_empty() && self.move_curves.is_empty()) {
            return;
        }
        let mut apply = false;
        let mut docks = std::mem::take(&mut self.docks);
        let dock = docks.panel(
            ctx,
            "move",
            Pos2::new(rect.max.x - 250.0, rect.min.y + 10.0),
            if docks.is_moved("move") {
                self.move_sel
                    .first()
                    .and_then(|b| self.body_center_screen(*b, rect))
            } else {
                None
            },
            230.0,
            &tr("Move"),
            false,
            |ui| {
                for (i, name) in ["X", "Y", "Z"].iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(*name);
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut self.move_ui.fields[i])
                                .desired_width(70.0)
                                .hint_text("0"),
                        );
                        ui.label("mm");
                        self.probe_widget(&format!("move_field_{i}"), r.rect);
                        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            apply = true;
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(tr("Angle"));
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.move_ui.fields[3])
                            .desired_width(60.0)
                            .hint_text("0"),
                    );
                    ui.label("°");
                    self.probe_widget("move_field_3", r.rect);
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        apply = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Axis"));
                    for (i, n) in ["X", "Y", "Z"].iter().enumerate() {
                        let r = ui.selectable_value(&mut self.move_ui.axis, i, *n);
                        self.probe_widget(&format!("move_axis_{i}"), r.rect);
                    }
                });
                let r = ui
                    .checkbox(&mut self.move_ui.copy, tr("Copy"))
                    .on_hover_text(tr("Move a copy and leave the original where it is"));
                self.probe_widget("move_copy", r.rect);
                let r = ui.button(tr("Apply"));
                self.probe_widget("move_apply", r.rect);
                if r.clicked() {
                    apply = true;
                }
            },
        );
        self.docks = docks;
        let _ = dock;
        if apply {
            let f = |i: usize| {
                let t = self.move_ui.fields[i].trim();
                if t.is_empty() {
                    Some(0.0)
                } else {
                    parse_num(t)
                }
            };
            match (f(0), f(1), f(2), f(3)) {
                (Some(x), Some(y), Some(z), Some(a)) => {
                    let (t, angle) = (DVec3::new(x, y, z), a.to_radians());
                    if t.length() < 1e-9 && angle.abs() < 1e-9 {
                        self.set_status("Nothing to move", false);
                    } else {
                        let axis = self.move_ui.axis;
                        self.submit_move(t, axis, angle);
                    }
                }
                _ => self.set_status("Please enter a number", true),
            }
        }
    }
}
