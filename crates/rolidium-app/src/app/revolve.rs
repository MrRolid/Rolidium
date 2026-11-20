// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Revolve of a sketch region or a flat face about an axis: pick the axis, set the angle.

use super::*;
use crate::i18n::{parse_num, tr};

#[derive(Clone, Debug)]
pub enum RevKind {
    Region { sketch: SketchId, idx: usize },
    Face { body: BodyId, face: u32 },
}

#[derive(Clone, Debug)]
pub struct AxisCand {
    #[allow(dead_code)]
    pub label: &'static str,
    pub p0: Vec3,
    pub p1: Vec3,
    pub point: DVec3,
    pub dir: DVec3,
}

pub struct RevolveState {
    pub kind: RevKind,
    /// The outline of the profile in 3D (for the preview).
    pub outline: Vec<Vec3>,
    pub cands: Vec<AxisCand>,
    pub axis: Option<AxisCand>,
    pub hover: Option<usize>,
    /// Angle from the mouse (degrees) and the text of the angle field.
    pub angle: f64,
    pub text: String,
    pub press: Option<Pos2>,
}

impl RevolveState {
    pub fn angle_deg(&self) -> f64 {
        if self.text.trim().is_empty() {
            self.angle
        } else {
            parse_num(&self.text).unwrap_or(0.0)
        }
    }
}

fn near_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-9)).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

impl App {
    /// Starts "Revolve" for the selected region or flat face: collects the axes that can be clicked.
    pub(super) fn begin_revolve(&mut self) {
        if !self.idle() {
            return;
        }
        let (kind, origin, normal, outline, sketch_lines) = if let Some((s, i)) = self.region_sel {
            let Some(sd) = self.doc.sketch(s) else { return };
            let Some(reg) = self
                .regions
                .iter()
                .find(|r| r.id == s)
                .and_then(|r| r.regions.get(i))
            else {
                return;
            };
            let outline: Vec<Vec3> = reg
                .outer
                .polyline(0.1)
                .iter()
                .map(|p| sd.plane.to_3d(*p).as_vec3())
                .collect();
            let mut lines: Vec<(Vec3, Vec3)> = Vec::new();
            for (_, e) in sd.sketch.entities() {
                match *e {
                    Entity::Line { a, b } => {
                        lines.push((sd.plane.to_3d(a).as_vec3(), sd.plane.to_3d(b).as_vec3()))
                    }
                    Entity::Rect { a, b } => {
                        let c = Entity::rect_corners(a, b);
                        for k in 0..4 {
                            lines.push((
                                sd.plane.to_3d(c[k]).as_vec3(),
                                sd.plane.to_3d(c[(k + 1) % 4]).as_vec3(),
                            ));
                        }
                    }
                    Entity::Circle { .. } | Entity::Arc { .. } => {}
                }
            }
            (
                RevKind::Region { sketch: s, idx: i },
                sd.plane.origin,
                sd.plane.normal,
                outline,
                lines,
            )
        } else if let [(b, f)] = self.selection[..] {
            let Some(meta) = self.entry(b).and_then(|m| m.faces.get(f as usize)).copied() else {
                return;
            };
            if meta.kind != FaceKind::Plane {
                self.set_status("Revolve works on flat faces and sketch regions", true);
                return;
            }
            let outline: Vec<Vec3> = self
                .doc
                .body(b)
                .and_then(|bd| bd.solid.face_outline(FaceId(f), 0.1).ok())
                .and_then(|l| l.into_iter().next())
                .unwrap_or_default();
            (
                RevKind::Face { body: b, face: f },
                meta.origin,
                meta.normal,
                outline,
                Vec::new(),
            )
        } else {
            self.set_status("Select a flat face or a sketch region first", true);
            return;
        };
        let n = normal.normalize();
        let in_plane = |p: DVec3| (p - origin).dot(n).abs() < 1e-4;
        let mut cands: Vec<AxisCand> = Vec::new();
        let mk = |label: &'static str, a: Vec3, b: Vec3| AxisCand {
            label,
            p0: a,
            p1: b,
            point: a.as_dvec3(),
            dir: (b - a).as_dvec3(),
        };
        for (l, a, b) in [
            ("X axis", Vec3::X, -Vec3::X),
            ("Y axis", Vec3::Y, -Vec3::Y),
            ("Z axis", Vec3::Z, -Vec3::Z),
        ] {
            let len = 400.0;
            if in_plane(DVec3::ZERO) && a.as_dvec3().dot(n).abs() < 1e-6 {
                cands.push(mk(l, b * len, a * len));
            }
        }
        for (a, b) in sketch_lines {
            if in_plane(a.as_dvec3()) && in_plane(b.as_dvec3()) && (b - a).length() > 1e-6 {
                cands.push(mk("sketch line", a, b));
            }
        }
        for m in self.meshes.iter() {
            for e in &m.mesh.edges {
                let (Some(a), Some(b)) = (e.points.first(), e.points.last()) else {
                    continue;
                };
                let straight = e.points.len() == 2
                    || e.points.iter().all(|p| {
                        let ab = (*b - *a).normalize_or_zero();
                        ((*p - *a) - ab * (*p - *a).dot(ab)).length() < 1e-3
                    });
                if straight && (*b - *a).length() > 1e-6 && in_plane(a.as_dvec3()) && in_plane(b.as_dvec3()) {
                    cands.push(mk("edge", *a, *b));
                }
            }
        }
        self.pull = None;
        self.clear_preview();
        self.revolve = Some(RevolveState {
            kind,
            outline,
            cands,
            axis: None,
            hover: None,
            angle: 0.0,
            text: String::new(),
            press: None,
        });
        self.layers_changed = true;
        self.set_status(
            "Revolve: click the axis (a sketch line, a straight edge or a coordinate axis)",
            false,
        );
    }

    pub(super) fn cancel_revolve(&mut self) {
        if self.revolve.take().is_some() {
            self.clear_preview();
            self.layers_changed = true;
            self.set_status("Cancelled", false);
        }
    }

    pub(super) fn revolve_input(&mut self, resp: &egui::Response, pointer: Option<Pos2>, rect: Rect) {
        let ctx = resp.ctx.clone();
        let shift = ctx.input(|i| i.modifiers.shift);
        let Some(rv) = &self.revolve else { return };
        if rv.axis.is_none() {
            let hover = pointer.filter(|_| resp.hovered()).and_then(|p| {
                let mut best: Option<(f32, usize)> = None;
                for (i, c) in rv.cands.iter().enumerate() {
                    if let (Some(a), Some(b)) = (self.project(c.p0, rect), self.project(c.p1, rect)) {
                        let d = near_segment(p, a, b);
                        if d <= 8.0 && best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, i));
                        }
                    }
                }
                best.map(|b| b.1)
            });
            if hover != rv.hover {
                self.layers_changed = true;
            }
            let rv = self.revolve.as_mut().unwrap();
            rv.hover = hover;
            if resp.clicked_by(PointerButton::Primary) {
                if let Some(i) = hover {
                    rv.axis = Some(rv.cands[i].clone());
                    rv.hover = None;
                    self.layers_changed = true;
                    self.update_revolve_preview();
                    self.set_status(
                        "Drag to set the angle (5° steps, Shift: free) or type it; Enter applies",
                        false,
                    );
                } else {
                    self.set_status("Click on one of the highlighted axes", false);
                }
            }
            return;
        }
        // angle by dragging
        if resp.drag_started_by(PointerButton::Primary) {
            let press = ctx.input(|i| i.pointer.press_origin());
            if let Some(rv) = &mut self.revolve {
                rv.press = press;
            }
        }
        if resp.dragged_by(PointerButton::Primary) {
            if let (Some(rv), Some(p)) = (&mut self.revolve, pointer) {
                if let Some(s) = rv.press {
                    let d = p - s;
                    let raw = ((d.x - d.y) * 0.5) as f64;
                    rv.angle = if shift {
                        (raw * 10.0).round() / 10.0
                    } else {
                        (raw / 5.0).round() * 5.0
                    };
                    rv.angle = rv.angle.clamp(-360.0, 360.0);
                    rv.text.clear();
                }
            }
            self.update_revolve_preview();
        }
        if resp.drag_stopped_by(PointerButton::Primary)
            && self.revolve.as_ref().is_some_and(|r| r.angle.abs() > 1e-9)
        {
            self.commit_revolve();
        }
    }

    /// A translucent ribbon of the profile outline swept through the angle.
    pub(super) fn update_revolve_preview(&mut self) {
        let Some(rv) = &self.revolve else { return };
        let Some(axis) = &rv.axis else { return };
        let angle = rv.angle_deg().to_radians() as f32;
        if rv.outline.len() < 2 || angle.abs() < 1e-6 {
            self.clear_preview();
            return;
        }
        let dir = axis.dir.as_vec3().normalize();
        let pt = axis.point.as_vec3();
        let steps = ((angle.abs().to_degrees() / 5.0).ceil() as usize).clamp(1, 72);
        let rot = |p: Vec3, a: f32| pt + glam::Quat::from_axis_angle(dir, a) * (p - pt);
        let mut tris = Vec::new();
        let mut lines = Vec::new();
        for k in 0..steps {
            let (a0, a1) = (
                angle * k as f32 / steps as f32,
                angle * (k + 1) as f32 / steps as f32,
            );
            for w in rv.outline.windows(2) {
                let (p, q) = (w[0], w[1]);
                tris.extend([
                    rot(p, a0),
                    rot(q, a0),
                    rot(q, a1),
                    rot(p, a0),
                    rot(q, a1),
                    rot(p, a1),
                ]);
            }
            if k == steps - 1 {
                for w in rv.outline.windows(2) {
                    lines.extend([rot(w[0], a1), rot(w[1], a1)]);
                }
            }
        }
        self.preview = Some(PreviewDraw {
            triangles: tris,
            lines,
            color: [0.35, 0.65, 1.0, 0.35],
            line_color: [1.0, 0.85, 0.35, 1.0],
            ..Default::default()
        });
        self.preview_changed = true;
    }

    pub(super) fn commit_revolve(&mut self) {
        let Some(rv) = self.revolve.take() else { return };
        self.clear_preview();
        self.layers_changed = true;
        let Some(axis) = rv.axis.clone() else { return };
        let angle = rv.angle_deg().to_radians();
        if angle.abs() < 1e-6 {
            self.set_status("Nothing to revolve (angle zero)", false);
            return;
        }
        if !self.idle() {
            self.set_status("Still working on the previous operation", true);
            return;
        }
        self.pending = None;
        let mode = self.pull_mode;
        match rv.kind {
            RevKind::Region { sketch, idx } => self.submit(Command::Revolve {
                sketch,
                region: idx,
                axis_point: axis.point,
                axis_dir: axis.dir,
                angle,
                mode,
            }),
            RevKind::Face { body, face } => self.submit(Command::RevolveFace {
                body,
                face: FaceId(face),
                axis_point: axis.point,
                axis_dir: axis.dir,
                angle,
                mode,
            }),
        }
        self.set_status("Computing…", false);
    }

    /// Thick lines of the axes that can be clicked and of the chosen one.
    pub(super) fn revolve_layer(&self) -> Option<PreviewDraw> {
        let rv = self.revolve.as_ref()?;
        let mut thick = Vec::new();
        if let Some(a) = &rv.axis {
            thick.extend([a.p0, a.p1]);
        } else if let Some(i) = rv.hover {
            thick.extend([rv.cands[i].p0, rv.cands[i].p1]);
        }
        let mut lines = Vec::new();
        if rv.axis.is_none() {
            for c in &rv.cands {
                lines.extend([c.p0, c.p1]);
            }
        }
        Some(PreviewDraw {
            lines,
            line_color: [0.9, 0.55, 1.0, 1.0],
            thick,
            thick_color: [1.0, 0.72, 0.25, 1.0],
            ..Default::default()
        })
    }

    #[allow(deprecated)]
    pub(super) fn revolve_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        let Some(rv) = &mut self.revolve else { return };
        let mut act: Option<u8> = None;
        let mut changed = false;
        let picked = rv.axis.is_some();
        let mut docks = std::mem::take(&mut self.docks);
        let dock = docks.panel(
            ctx,
            "revolve",
            Pos2::new(rect.max.x - 250.0, rect.min.y + 10.0),
            None,
            230.0,
            &tr("Revolve"),
            false,
            |ui| {
                if !picked {
                    ui.label(tr(
                        "Click the axis: a sketch line, a straight edge or a coordinate axis",
                    ));
                } else {
                    ui.horizontal(|ui| {
                        ui.label(tr("Angle"));
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut rv.text)
                                .desired_width(60.0)
                                .hint_text(format!("{}", rv.angle)),
                        );
                        if r.changed() {
                            changed = true;
                        }
                        ui.label("°");
                        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            act = Some(0);
                        }
                        if ui.button("360").clicked() {
                            rv.text = "360".into();
                            changed = true;
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(tr("Mode"));
                    for (m, name) in [
                        (PullMode::Auto, "Auto"),
                        (PullMode::New, "New body"),
                        (PullMode::Add, "Add"),
                        (PullMode::Cut, "Cut"),
                    ] {
                        ui.selectable_value(&mut self.pull_mode, m, tr(name));
                    }
                });
                ui.horizontal(|ui| {
                    if picked && ui.button(tr("Apply")).clicked() {
                        act = Some(0);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        act = Some(1);
                    }
                });
            },
        );
        self.docks = docks;
        let _ = dock;
        if changed {
            self.update_revolve_preview();
        }
        match act {
            Some(0) => self.commit_revolve(),
            Some(_) => self.cancel_revolve(),
            None => {}
        }
    }
}
