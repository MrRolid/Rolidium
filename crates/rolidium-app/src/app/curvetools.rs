// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Curves: the 3D line, the helix and the Sweep tool, and how curves are shown, picked and kept in the document tree.

use super::*;
use crate::i18n::{parse_num, tr, trf};
use rolidium_doc::{sketch_chain, CurveDef, CurveId, LoftRef, SweepMode, SweepPath};
use rolidium_kernel::HelixDef;

const CURVE_COLOR: [f32; 4] = [0.35, 0.90, 1.0, 1.0];
const WORK_COLOR: [f32; 4] = [1.0, 0.72, 0.25, 1.0];

// ---------------------------------------------------------------- 3D line

#[derive(Default)]
pub struct Line3 {
    /// The "3D" toggle of the Line tool.
    pub on: bool,
    pub pts: Vec<DVec3>,
    pub radius: String,
    /// Locked to the X, Y or Z axis (0, 1, 2).
    pub lock: Option<usize>,
    pub typed: String,
    pub hover: Option<(DVec3, &'static str)>,
}

// ---------------------------------------------------------------- helix

pub struct HelixUi {
    /// Axis: a point on it and its direction.
    pub axis: Option<(DVec3, DVec3)>,
    pub x_dir: DVec3,
    pub diameter: String,
    pub pitch: String,
    pub height: String,
    pub turns: String,
    pub start: String,
    pub cone: String,
    pub right: bool,
    preview_key: String,
    pub preview: Vec<Vec3>,
    pub error: Option<String>,
}

impl Default for HelixUi {
    fn default() -> Self {
        HelixUi {
            axis: None,
            x_dir: DVec3::X,
            diameter: "20".into(),
            pitch: "5".into(),
            height: "25".into(),
            turns: "5".into(),
            start: "0".into(),
            cone: "0".into(),
            right: true,
            preview_key: String::new(),
            preview: Vec::new(),
            error: None,
        }
    }
}

impl HelixUi {
    fn def(&self) -> Option<HelixDef> {
        let (origin, axis) = self.axis?;
        Some(HelixDef {
            origin,
            axis,
            x_dir: self.x_dir,
            radius: parse_num(&self.diameter)? / 2.0,
            pitch: parse_num(&self.pitch)?,
            turns: parse_num(&self.turns)?,
            right: self.right,
            start_deg: parse_num(&self.start).unwrap_or(0.0),
            cone_deg: parse_num(&self.cone).unwrap_or(0.0),
        })
    }
}

// ---------------------------------------------------------------- sweep

pub struct SweepUi {
    pub profile: Option<LoftRef>,
    pub path: Option<SweepPath>,
    pub mode: SweepMode,
    pub rounded: bool,
    pub preview: Vec<Vec3>,
}

impl Default for SweepUi {
    fn default() -> Self {
        SweepUi {
            profile: None,
            path: None,
            mode: SweepMode::Auto,
            rounded: false,
            preview: Vec::new(),
        }
    }
}

/// Polylines of the curves for drawing and picking, rebuilt when the curves of the document change.
#[derive(Default)]
pub struct CurveCache {
    defs: Vec<(CurveId, CurveDef)>,
    pub lines: Vec<(CurveId, Vec<Vec3>)>,
    /// The deflection (mm) the polylines were made with.
    pub tol: f64,
}

/// The deflection of the curve polylines for a pixel of `pixel_mm`: 0.4 of a pixel (like the edges of the bodies), rounded
/// down to a power of two so that the polylines are rebuilt only when the zoom changes by a factor of two.
pub(super) fn curve_tolerance(pixel_mm: f64) -> f64 {
    if !pixel_mm.is_finite() || pixel_mm <= 0.0 {
        return 0.05;
    }
    let t = (pixel_mm * 0.4).clamp(0.002, 0.5);
    2f64.powf(t.log2().floor())
}

fn polyline_of(def: &CurveDef, tol: f64) -> Vec<Vec3> {
    def.path()
        .and_then(|p| p.polyline(tol))
        .map(|v| v.iter().map(|p| p.as_vec3()).collect())
        .unwrap_or_default()
}

fn pairs(pts: &[Vec3]) -> Vec<Vec3> {
    pts.windows(2).flat_map(|w| [w[0], w[1]]).collect()
}

impl App {
    /// The deflection (mm) the curve polylines are made with now.
    pub(super) fn curve_tol(&self) -> f64 {
        if self.curve_cache.tol > 0.0 {
            self.curve_cache.tol
        } else {
            0.05
        }
    }

    pub(super) fn refresh_curve_cache(&mut self) {
        let now: Vec<(CurveId, CurveDef)> = self.doc.curves().iter().map(|c| (c.id, c.def.clone())).collect();
        let wanted = curve_tolerance(self.pixel_mm());
        let tol = self.curve_cache.tol;
        // finer when the view came closer, coarser (to save memory) only when it went much further away
        let retol = tol <= 0.0 || wanted < tol * 0.99 || wanted > tol * 3.9;
        if now != self.curve_cache.defs || retol {
            let t = if retol { wanted } else { tol };
            self.curve_cache.lines = now.iter().map(|(id, d)| (*id, polyline_of(d, t))).collect();
            self.curve_cache.defs = now;
            self.curve_cache.tol = t;
            self.layers_changed = true;
            if retol {
                self.helix.preview_key.clear();
                if self.tool == Tool::Sweep {
                    self.sweep_refresh_preview();
                }
            }
        }
    }

    pub(super) fn curve_points(&self, id: CurveId) -> &[Vec3] {
        self.curve_cache
            .lines
            .iter()
            .find(|(c, _)| *c == id)
            .map_or(&[][..], |(_, p)| &p[..])
    }

    /// Curves, the 3D line being drawn, the helix and sweep previews.
    pub(super) fn curve_layers(&self) -> Vec<PreviewDraw> {
        let mut out = Vec::new();
        let mut thick = Vec::new();
        for c in self.doc.curves().iter().filter(|c| c.visible) {
            thick.extend(pairs(self.curve_points(c.id)));
        }
        if !thick.is_empty() {
            out.push(PreviewDraw {
                thick,
                thick_color: CURVE_COLOR,
                ..Default::default()
            });
        }
        if let Some(id) = self.curve_sel {
            let t = pairs(self.curve_points(id));
            if !t.is_empty() {
                out.push(PreviewDraw {
                    thick: t,
                    thick_color: WORK_COLOR,
                    ..Default::default()
                });
            }
        }
        if let Some(Obj::Curve(id)) = self.hover_obj {
            out.push(PreviewDraw {
                thick: pairs(self.curve_points(id)),
                thick_color: [1.0, 0.85, 0.5, 1.0],
                ..Default::default()
            });
        }
        if self.line3_active() && !self.line3.pts.is_empty() {
            let mut p: Vec<Vec3> = self.line3.pts.iter().map(|p| p.as_vec3()).collect();
            if let Some((h, _)) = self.line3.hover {
                p.push(h.as_vec3());
            }
            out.push(PreviewDraw {
                thick: pairs(&p),
                thick_color: WORK_COLOR,
                ..Default::default()
            });
        }
        if self.tool == Tool::Helix && !self.helix.preview.is_empty() {
            out.push(PreviewDraw {
                thick: pairs(&self.helix.preview),
                thick_color: WORK_COLOR,
                ..Default::default()
            });
        }
        if self.tool == Tool::Sweep {
            if let Some(p) = &self.sweep.profile {
                let mut t = Vec::new();
                for w in self.loft_outline(p).windows(2) {
                    t.extend([w[0], w[1]]);
                }
                out.push(PreviewDraw {
                    thick: t,
                    thick_color: WORK_COLOR,
                    ..Default::default()
                });
            }
            if let Some(path) = &self.sweep.path {
                out.push(PreviewDraw {
                    thick: pairs(&self.sweep_path_points(path)),
                    thick_color: [0.45, 0.85, 1.0, 1.0],
                    ..Default::default()
                });
            }
            if !self.sweep.preview.is_empty() {
                out.push(PreviewDraw {
                    triangles: self.sweep.preview.clone(),
                    color: [0.30, 0.70, 1.00, 0.30],
                    ..Default::default()
                });
            }
            // the chain a click would take
            if let Some(o) = self.hover_obj {
                if self.sweep.profile.is_some() {
                    out.push(PreviewDraw {
                        thick: pairs(&self.chain_points(o)),
                        thick_color: [0.85, 0.5, 1.0, 1.0],
                        ..Default::default()
                    });
                }
            }
        }
        out
    }

    // ------------------------------------------------------------ 3D line

    pub(super) fn line3_active(&self) -> bool {
        self.line3.on && self.sk.as_ref().is_some_and(|m| m.tool == SkTool::Line)
    }

    /// 3D points that the cursor snaps to: vertices, edge midpoints, circle centres, points of sketches and curves.
    fn line3_candidates(&self) -> Vec<(DVec3, &'static str)> {
        let mut v: Vec<(DVec3, &'static str)> = Vec::new();
        for m in self.meshes.iter() {
            for e in &m.mesh.edges {
                let (Some(a), Some(b)) = (e.points.first(), e.points.last()) else {
                    continue;
                };
                v.push((a.as_dvec3(), "vertex (3D)"));
                v.push((b.as_dvec3(), "vertex (3D)"));
                if e.points.len() > 2 {
                    if (*a - *b).length() < 1e-3 {
                        let c: Vec3 = e.points.iter().copied().sum::<Vec3>() / e.points.len() as f32;
                        v.push((c.as_dvec3(), "circle centre (3D)"));
                    } else {
                        v.push((e.points[e.points.len() / 2].as_dvec3(), "edge midpoint (3D)"));
                    }
                } else {
                    v.push(((*a + *b).as_dvec3() * 0.5, "edge midpoint (3D)"));
                }
            }
        }
        for s in self.doc.sketches().iter().filter(|s| s.visible) {
            for (_, e) in s.sketch.entities() {
                match *e {
                    rolidium_doc::rolidium_sketch::Entity::Circle { center, .. }
                    | rolidium_doc::rolidium_sketch::Entity::Arc { center, .. } => {
                        v.push((s.plane.to_3d(center), "center"))
                    }
                    _ => {}
                }
                for pl in e.polylines() {
                    if let (Some(a), Some(b)) = (pl.first(), pl.last()) {
                        v.push((s.plane.to_3d(*a), "endpoint"));
                        v.push((s.plane.to_3d(*b), "endpoint"));
                    }
                }
            }
        }
        for (_, pts) in &self.curve_cache.lines {
            if let (Some(a), Some(b)) = (pts.first(), pts.last()) {
                v.push((a.as_dvec3(), "endpoint"));
                v.push((b.as_dvec3(), "endpoint"));
            }
        }
        v.extend(self.line3.pts.iter().map(|p| (*p, "endpoint")));
        v
    }

    /// The point under the cursor: a snap, else on the locked axis, else in the drawing plane.
    fn line3_point(
        &self,
        pointer: Pos2,
        rect: Rect,
        ray: Option<(Vec3, Vec3)>,
    ) -> Option<(DVec3, &'static str)> {
        let mut best: Option<(f32, DVec3, &'static str)> = None;
        for (p, label) in self.line3_candidates() {
            if let Some(sp) = self.project(p.as_vec3(), rect) {
                let d = (sp - pointer).length();
                if d <= 12.0 && best.is_none_or(|(bd, _, _)| d < bd) {
                    best = Some((d, p, label));
                }
            }
        }
        if let Some((_, p, l)) = best {
            // a locked axis wins over a snap that is off the axis
            if self.line3.lock.is_none() {
                return Some((p, l));
            }
        }
        let (o, d) = ray?;
        if let (Some(axis), Some(last)) = (self.line3.lock, self.line3.pts.last()) {
            // the point of the axis line through the last point that is closest to the view ray
            let dir = [DVec3::X, DVec3::Y, DVec3::Z][axis];
            let (o, d) = (o.as_dvec3(), d.as_dvec3().normalize());
            let w0 = *last - o;
            let (a, b, c) = (dir.dot(dir), dir.dot(d), d.dot(d));
            let (dd, e) = (dir.dot(w0), d.dot(w0));
            let den = a * c - b * b;
            if den.abs() > 1e-9 {
                let t = (b * e - c * dd) / den;
                return Some((*last + dir * t, "axis"));
            }
            return None;
        }
        let plane = self.sk.as_ref()?.plane;
        let (hit, _) = plane.ray_hit(o.as_dvec3(), d.as_dvec3())?;
        Some((plane.to_3d(hit), "plane"))
    }

    pub(super) fn line3_input(
        &mut self,
        resp: &egui::Response,
        pointer: Option<Pos2>,
        ray: Option<(Vec3, Vec3)>,
        rect: Rect,
    ) {
        self.line3.hover = match (resp.hovered(), pointer) {
            (true, Some(p)) => self.line3_point(p, rect, ray),
            _ => None,
        };
        if resp.clicked_by(PointerButton::Primary) {
            if let Some((p, _)) = self.line3.hover {
                if self.line3.pts.last().is_none_or(|l| (*l - p).length() > 1e-6) {
                    self.line3.pts.push(p);
                    self.line3.typed.clear();
                }
            }
        }
        if resp.double_clicked_by(PointerButton::Primary) {
            self.line3_finish();
        }
        if resp.clicked_by(PointerButton::Secondary) {
            self.line3_cancel();
        }
        self.layers_changed = true;
    }

    /// Typed digits while a 3D line is drawn: the length of the next segment.
    pub(super) fn line3_typed(&mut self, ch: char) -> bool {
        if !self.line3_active() || self.line3.pts.is_empty() {
            return false;
        }
        let ch = if ch == ',' { '.' } else { ch };
        if ch.is_ascii_digit() || ch == '.' || (ch == '-' && self.line3.typed.is_empty()) {
            self.line3.typed.push(ch);
            return true;
        }
        false
    }

    pub(super) fn line3_key(&mut self, key: Key) -> bool {
        if !self.line3_active() {
            return false;
        }
        match key {
            Key::X | Key::Y | Key::Z => {
                let a = match key {
                    Key::X => 0,
                    Key::Y => 1,
                    _ => 2,
                };
                self.line3.lock = if self.line3.lock == Some(a) { None } else { Some(a) };
                true
            }
            Key::Enter => {
                if !self.line3.typed.is_empty() {
                    self.line3_apply_length();
                } else {
                    self.line3_finish();
                }
                true
            }
            Key::Escape => {
                self.line3_cancel();
                true
            }
            Key::Backspace => {
                if self.line3.typed.pop().is_none() {
                    self.line3.pts.pop();
                }
                true
            }
            _ => false,
        }
    }

    /// Adds the next point at the typed distance, along the locked axis or towards the cursor.
    fn line3_apply_length(&mut self) {
        let (Some(len), Some(last)) = (parse_num(&self.line3.typed), self.line3.pts.last().copied()) else {
            self.line3.typed.clear();
            return;
        };
        let dir = match (self.line3.lock, self.line3.hover) {
            (Some(a), Some((h, _))) => {
                let axis = [DVec3::X, DVec3::Y, DVec3::Z][a];
                if (h - last).dot(axis) < 0.0 {
                    -axis
                } else {
                    axis
                }
            }
            (None, Some((h, _))) if (h - last).length() > 1e-9 => (h - last).normalize(),
            (Some(a), None) => [DVec3::X, DVec3::Y, DVec3::Z][a],
            _ => DVec3::X,
        };
        self.line3.pts.push(last + dir * len);
        self.line3.typed.clear();
    }

    pub(super) fn line3_cancel(&mut self) {
        self.line3.pts.clear();
        self.line3.typed.clear();
        self.line3.lock = None;
        self.line3.hover = None;
        self.layers_changed = true;
    }

    pub(super) fn line3_finish(&mut self) {
        if self.line3.pts.len() < 2 {
            self.set_status("A 3D line needs at least two points", true);
            return;
        }
        let radius = parse_num(&self.line3.radius).unwrap_or(0.0).max(0.0);
        let def = CurveDef::Polyline {
            points: self.line3.pts.clone(),
            radius,
        };
        if let Err(e) = def.path() {
            self.set_status(e.to_string(), true);
            return;
        }
        self.submit(Command::AddCurve { def });
        self.line3_cancel();
        self.set_status("Computing…", false);
    }

    pub(super) fn line3_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        if !self.line3_active() {
            return;
        }
        let mut finish = false;
        let mut cancel = false;
        let mut docks = std::mem::take(&mut self.docks);
        docks.panel(ctx, "line3", Pos2::new(rect.max.x - 250.0, rect.min.y + 10.0), None, 230.0, &tr("3D line"), false, |ui| {
            ui.label(trf("Points: {}", &[self.line3.pts.len().to_string()]));
            ui.horizontal(|ui| {
                ui.label(tr("Corner radius"));
                ui.add(egui::TextEdit::singleline(&mut self.line3.radius).desired_width(50.0).hint_text("0"));
                ui.label("mm");
            });
            ui.horizontal(|ui| {
                for (i, n) in ["X", "Y", "Z"].iter().enumerate() {
                    if ui.selectable_label(self.line3.lock == Some(i), *n).clicked() {
                        self.line3.lock = if self.line3.lock == Some(i) { None } else { Some(i) };
                    }
                }
                ui.label(tr("lock the direction"));
            });
            if !self.line3.typed.is_empty() {
                ui.label(trf("Length: {} mm (Enter)", std::slice::from_ref(&self.line3.typed)));
            }
            ui.label(egui::RichText::new(tr("Click points; X, Y, Z lock a direction, type a length and press Enter; Enter or double click finishes.")).weak().small());
            ui.horizontal(|ui| {
                if ui.add_enabled(self.line3.pts.len() >= 2, egui::Button::new(tr("Finish"))).clicked() {
                    finish = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
        self.docks = docks;
        if finish {
            self.line3_finish();
        }
        if cancel {
            self.line3_cancel();
        }
    }

    /// The marker and the name of the snap under the cursor, over the view.
    pub(super) fn line3_paint(&self, ui: &egui::Ui, rect: Rect) {
        if !self.line3_active() {
            return;
        }
        let Some((p, label)) = self.line3.hover else {
            return;
        };
        let Some(sp) = self.project(p.as_vec3(), rect) else {
            return;
        };
        let painter = ui.painter().with_clip_rect(rect);
        painter.circle_stroke(sp, 6.0, egui::Stroke::new(2.0, crate::theme::rgb(120, 255, 160)));
        painter.text(
            sp + Vec2::new(10.0, 8.0),
            egui::Align2::LEFT_TOP,
            tr(label),
            egui::FontId::proportional(12.0),
            crate::theme::rgb(120, 255, 160),
        );
        if let (Some(a), Some(l)) = (self.line3.lock, self.line3.pts.last()) {
            let name = ["X", "Y", "Z"][a];
            if let Some(lp) = self.project(l.as_vec3(), rect) {
                painter.text(
                    lp + Vec2::new(8.0, -16.0),
                    egui::Align2::LEFT_BOTTOM,
                    name,
                    egui::FontId::proportional(13.0),
                    crate::theme::rgb(255, 220, 120),
                );
            }
        }
    }

    // ------------------------------------------------------------ helix

    pub(super) fn activate_helix(&mut self) {
        self.leave_sketch();
        self.tool = Tool::Helix;
        self.helix = HelixUi::default();
        self.set_status("Helix: click a cylindrical face, a straight edge or line for the axis, or a point for a vertical axis", false);
        self.layers_changed = true;
    }

    /// A click chooses the axis.
    pub(super) fn helix_input(&mut self, resp: &egui::Response, pointer: Option<Pos2>, rect: Rect) {
        if !resp.clicked_by(PointerButton::Primary) {
            return;
        }
        let Some(p) = pointer else { return };
        let aspect = rect.width() / rect.height().max(1.0);
        let ray = Some(self.camera.ray(Self::ndc(p, rect), aspect));
        // the axis, and what a round face gives: (diameter at the start, height, half angle of the cone)
        type RoundFace = (f64, f64, f64);
        let mut axis: Option<(DVec3, DVec3, Option<RoundFace>)> = None;
        if let Some(obj) = self.pick_object(p, rect, true, true) {
            match obj {
                Obj::Edge(b, e) => {
                    if let Some(pl) = self.edge_polyline(b, e) {
                        if let (Some(a), Some(z)) = (pl.first(), pl.last()) {
                            if pl.len() == 2
                                || (0..pl.len().saturating_sub(2)).all(|i| {
                                    ((pl[i + 1] - pl[i]).normalize().dot((*z - *a).normalize())).abs()
                                        > 0.9999
                                })
                            {
                                axis = Some((a.as_dvec3(), (*z - *a).as_dvec3().normalize(), None));
                            }
                        }
                    }
                }
                Obj::Entity(s, id) => {
                    if let Some(sd) = self.doc.sketch(s) {
                        if let Some(rolidium_doc::rolidium_sketch::Entity::Line { a, b }) =
                            sd.sketch.entity(id)
                        {
                            let (a3, b3) = (sd.plane.to_3d(*a), sd.plane.to_3d(*b));
                            axis = Some((a3, (b3 - a3).normalize(), None));
                        }
                    }
                }
                Obj::Curve(_) => {}
            }
        }
        if axis.is_none() {
            if let Some((b, f)) = self.hovered_face() {
                if let Some(info) = self
                    .doc
                    .body(b)
                    .and_then(|bd| bd.solid.cylinder_info(FaceId(f)).ok().flatten())
                {
                    axis = Some((info.base, info.axis, Some((info.radius * 2.0, info.height, 0.0))));
                } else if let Some(info) = self
                    .doc
                    .body(b)
                    .and_then(|bd| bd.solid.cone_info(FaceId(f)).ok().flatten())
                {
                    // a conical face: the axis of the cone, the start at its lower rim, the height of the face, the diameter and the angle of the cone there
                    axis = Some((
                        info.base,
                        info.axis,
                        Some((info.r_base * 2.0, info.height, info.half_angle_deg)),
                    ));
                }
            }
        }
        if axis.is_none() {
            if let Some((o, d)) = ray {
                // a vertical axis through the point of the base plane under the cursor
                let (o, d) = (o.as_dvec3(), d.as_dvec3());
                if d.z.abs() > 1e-9 {
                    let t = -o.z / d.z;
                    if t > 0.0 {
                        let hit = o + d * t;
                        axis = Some((DVec3::new(hit.x, hit.y, 0.0), DVec3::Z, None));
                    }
                }
            }
        }
        let Some((origin, dir, diameter)) = axis else {
            return;
        };
        self.helix.axis = Some((origin, dir));
        let any = if dir.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        self.helix.x_dir = dir.cross(any.cross(dir)).normalize();
        if let Some((d, height, cone)) = diameter {
            self.helix.diameter = fmt_num(d);
            self.helix.cone = fmt_num(cone);
            self.helix.start = "0".into();
            // the helix runs along the whole face: the height of the face, and the turns that fit it
            self.helix.height = fmt_num(height);
            if let Some(p) = parse_num(&self.helix.pitch).filter(|p| p.abs() > 1e-9) {
                self.helix.turns = fmt_num(height / p.abs());
            }
        }
        self.helix.preview_key.clear();
        self.layers_changed = true;
    }

    fn helix_refresh_preview(&mut self) {
        let key = format!(
            "{}|{:?}|{}|{}|{}|{}|{}|{}|{}",
            self.curve_cache.tol,
            self.helix.axis.map(|(o, a)| (o.to_array(), a.to_array())),
            self.helix.diameter,
            self.helix.pitch,
            self.helix.turns,
            self.helix.start,
            self.helix.cone,
            self.helix.right,
            self.helix.x_dir
        );
        if key == self.helix.preview_key {
            return;
        }
        self.helix.preview_key = key;
        self.helix.error = None;
        self.helix.preview.clear();
        match self.helix.def() {
            Some(def) => match CurveDef::Helix(def).path() {
                Ok(p) => {
                    self.helix.preview = p
                        .polyline(self.curve_tol())
                        .map(|v| v.iter().map(|p| p.as_vec3()).collect())
                        .unwrap_or_default()
                }
                Err(e) => self.helix.error = Some(e.to_string()),
            },
            None => {
                self.helix.error = Some(tr(
                    "Enter numbers for the diameter, the pitch and the height or the number of turns",
                ))
            }
        }
        self.layers_changed = true;
    }

    pub(super) fn helix_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        if self.tool != Tool::Helix {
            return;
        }
        self.helix_refresh_preview();
        let mut create = false;
        let mut cancel = false;
        let mut docks = std::mem::take(&mut self.docks);
        docks.panel(ctx, "helix", Pos2::new(rect.max.x - 250.0, rect.min.y + 10.0), None, 230.0, &tr("Helix"), false, |ui| {
            if self.helix.axis.is_none() {
                ui.label(tr("Click the axis: a cylindrical face, a straight edge or line, or a point for a vertical axis"));
            }
            let pitch = parse_num(&self.helix.pitch).filter(|p| p.abs() > 1e-9);
            let field = |ui: &mut egui::Ui, label: &str, text: &mut String, unit: &str| -> bool {
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label(label);
                    changed = ui.add(egui::TextEdit::singleline(text).desired_width(60.0)).changed();
                    ui.label(unit);
                });
                changed
            };
            let mut h = std::mem::take(&mut self.helix);
            field(ui, &tr("Diameter"), &mut h.diameter, "mm");
            if field(ui, &tr("Pitch"), &mut h.pitch, "mm") {
                if let (Some(p), Some(t)) = (parse_num(&h.pitch).filter(|p| p.abs() > 1e-9), parse_num(&h.turns)) {
                    h.height = fmt_num(t * p.abs());
                }
            }
            if field(ui, &tr("Height"), &mut h.height, "mm") {
                if let (Some(p), Some(ht)) = (pitch, parse_num(&h.height)) {
                    h.turns = fmt_num(ht / p.abs());
                }
            }
            if field(ui, &tr("Turns"), &mut h.turns, "") {
                if let (Some(p), Some(t)) = (pitch, parse_num(&h.turns)) {
                    h.height = fmt_num(t * p.abs());
                }
            }
            ui.horizontal(|ui| {
                ui.selectable_value(&mut h.right, true, tr("Right-handed"));
                ui.selectable_value(&mut h.right, false, tr("Left-handed"));
            });
            field(ui, &tr("Start angle"), &mut h.start, "°");
            field(ui, &tr("Cone angle"), &mut h.cone, "°");
            if let Some(e) = &h.error {
                ui.colored_label(crate::theme::rgb(255, 120, 110), tr(e));
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(h.axis.is_some() && h.error.is_none(), egui::Button::new(tr("Create"))).clicked() {
                    create = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
            });
            self.helix = h;
        });
        self.docks = docks;
        if create {
            if let Some(def) = self.helix.def() {
                self.submit(Command::AddCurve {
                    def: CurveDef::Helix(def),
                });
                self.tool = Tool::Select;
                self.helix.preview.clear();
                self.set_status("Computing…", false);
            }
        }
        if cancel {
            self.tool = Tool::Select;
            self.helix.preview.clear();
            self.layers_changed = true;
        }
    }

    // ------------------------------------------------------------ sweep

    pub(super) fn activate_sweep(&mut self) {
        self.leave_sketch();
        self.tool = Tool::Sweep;
        self.sweep = SweepUi::default();
        // a profile chosen beforehand: a selected sketch region or flat face
        if let Some((s, i)) = self.region_sel {
            self.sweep.profile = Some(LoftRef::Region { sketch: s, region: i });
        } else if let [(b, f)] = self.selection[..] {
            if self
                .entry(b)
                .and_then(|m| m.faces.get(f as usize))
                .is_some_and(|m| m.kind == FaceKind::Plane)
            {
                self.sweep.profile = Some(LoftRef::Face {
                    body: b,
                    face: FaceId(f),
                });
            }
        }
        if self.sweep.profile.is_some() {
            self.clear_selection();
        }
        // a path chosen beforehand: the selected curve
        if let Some(c) = self.curve_sel {
            self.sweep.path = Some(SweepPath::Curve(c));
        }
        self.sweep_refresh_preview();
        self.set_status(
            if self.sweep.profile.is_none() {
                "Sweep: click the profile (a sketch region or a flat face), then the path"
            } else {
                "Sweep: click the path (a curve, a chain of sketch lines or a chain of edges)"
            },
            false,
        );
        self.layers_changed = true;
    }

    /// The points of a path in order, for drawing.
    pub(super) fn sweep_path_points(&self, path: &SweepPath) -> Vec<Vec3> {
        let to_vec = |p: Result<rolidium_kernel::Path, rolidium_kernel::KernelError>| -> Vec<Vec3> {
            p.and_then(|p| p.polyline(self.curve_tol()))
                .map(|v| v.iter().map(|p| p.as_vec3()).collect())
                .unwrap_or_default()
        };
        match path {
            SweepPath::Curve(id) => self.curve_points(*id).to_vec(),
            SweepPath::Sketch { sketch, entities } => {
                let Some(sd) = self.doc.sketch(*sketch) else {
                    return Vec::new();
                };
                match rolidium_doc::sketch_path_segments(&sd.plane, &sd.sketch, entities) {
                    Ok(segs) => to_vec(rolidium_kernel::Path::build(
                        rolidium_kernel::PathSource::Segments(&segs),
                    )),
                    Err(_) => Vec::new(),
                }
            }
            SweepPath::Edges { body, edges } => {
                let Some(b) = self.doc.body(*body) else {
                    return Vec::new();
                };
                let ids: Vec<rolidium_kernel::EdgeId> =
                    edges.iter().map(|e| rolidium_kernel::EdgeId(*e)).collect();
                to_vec(rolidium_kernel::Path::build(rolidium_kernel::PathSource::Edges {
                    solid: &b.solid,
                    edges: &ids,
                }))
            }
        }
    }

    /// What clicking `o` would choose as a path: the whole chain it belongs to.
    fn path_of(&self, o: Obj) -> Option<SweepPath> {
        match o {
            Obj::Curve(id) => Some(SweepPath::Curve(id)),
            Obj::Entity(s, id) => {
                let sd = self.doc.sketch(s)?;
                Some(SweepPath::Sketch {
                    sketch: s,
                    entities: sketch_chain(&sd.sketch, id),
                })
            }
            Obj::Edge(b, e) => {
                let body = self.doc.body(b)?;
                let chain = body
                    .solid
                    .smooth_chain(&[rolidium_kernel::EdgeId(e)], rolidium_kernel::CHAIN_ANGLE_DEG);
                Some(SweepPath::Edges {
                    body: b,
                    edges: chain.iter().map(|c| c.0).collect(),
                })
            }
        }
    }

    fn chain_points(&self, o: Obj) -> Vec<Vec3> {
        match self.path_of(o) {
            Some(SweepPath::Edges { body, edges }) => edges
                .iter()
                .filter_map(|e| self.edge_polyline(body, *e))
                .flat_map(pairs)
                .collect(),
            Some(SweepPath::Sketch { sketch, entities }) => {
                let Some(sd) = self.doc.sketch(sketch) else {
                    return Vec::new();
                };
                entities
                    .iter()
                    .filter_map(|id| sd.sketch.entity(*id))
                    .flat_map(|e| e.polylines())
                    .flat_map(|pl| {
                        pairs(
                            &pl.iter()
                                .map(|p| sd.plane.to_3d(*p).as_vec3())
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect()
            }
            Some(SweepPath::Curve(id)) => pairs(self.curve_points(id)),
            None => Vec::new(),
        }
    }

    pub(super) fn sweep_input(
        &mut self,
        resp: &egui::Response,
        pointer: Option<Pos2>,
        rect: Rect,
        hovered_face: Option<(BodyId, u32)>,
    ) {
        if !resp.clicked_by(PointerButton::Primary) {
            return;
        }
        let Some(p) = pointer else { return };
        // the profile first, then the path; a click on the profile again takes it back
        if self.sweep.profile.is_none() {
            // a region is taken even when the pointer is near its outline (which would otherwise count as a sketch shape)
            let aspect = rect.width() / rect.height().max(1.0);
            let (ro, rd) = self.camera.ray(Self::ndc(p, rect), aspect);
            let region = self
                .hover_region
                .or_else(|| self.pick_region(ro, rd, hovered_face));
            let pick = if let Some((s, i)) = region {
                Some(LoftRef::Region { sketch: s, region: i })
            } else {
                hovered_face.and_then(|(b, f)| {
                    self.entry(b)
                        .and_then(|m| m.faces.get(f as usize))
                        .is_some_and(|m| m.kind == FaceKind::Plane)
                        .then_some(LoftRef::Face {
                            body: b,
                            face: FaceId(f),
                        })
                })
            };
            if let Some(pr) = pick {
                self.sweep.profile = Some(pr);
                self.sweep_refresh_preview();
                self.set_status(
                    "Sweep: now click the path (a curve, a chain of sketch lines or a chain of edges)",
                    false,
                );
            }
        } else if let Some(o) = self.pick_cycled(p, rect, true, true) {
            self.sweep.path = self.path_of(o);
            self.sweep_refresh_preview();
        }
        self.layers_changed = true;
    }

    /// A cheap translucent preview: the outline of the profile carried along the path.
    pub(super) fn sweep_refresh_preview(&mut self) {
        self.sweep.preview.clear();
        let (Some(profile), Some(path)) = (self.sweep.profile, self.sweep.path.clone()) else {
            return;
        };
        let outline = self.loft_outline(&profile);
        let pts = self.sweep_path_points(&path);
        if outline.len() < 3 || pts.len() < 2 {
            return;
        }
        let outline = super::lofttool::resample_loop(&outline, 96);
        // the profile is carried to the nearer end of the path, then along it
        let nearer = |a: Vec3, b: Vec3, q: Vec3| {
            if (a - q).length() <= (b - q).length() {
                (a, false)
            } else {
                (b, true)
            }
        };
        let centre = outline.iter().copied().sum::<Vec3>() / outline.len() as f32;
        let (_, reversed) = nearer(pts[0], pts[pts.len() - 1], centre);
        let mut path = pts;
        if reversed {
            path.reverse();
        }
        // nearly every point of the path (the path is already as fine as the zoom needs), at most 600 frames
        let step = (path.len() / 600).max(1);
        let mut frames: Vec<Vec<Vec3>> = Vec::new();
        let mut q = glam::Quat::IDENTITY;
        let mut prev_t = (path[1] - path[0]).normalize_or_zero();
        let start = path[0];
        let mut i = 0;
        while i < path.len() {
            let t = if i + 1 < path.len() {
                (path[i + 1] - path[i]).normalize_or_zero()
            } else {
                prev_t
            };
            if t.length() > 0.5 && prev_t.length() > 0.5 {
                q = glam::Quat::from_rotation_arc(prev_t, t) * q;
                prev_t = t;
            }
            frames.push(
                outline
                    .iter()
                    .map(|p| path[i] + q * (*p - centre) + (centre - start) * 0.0)
                    .collect(),
            );
            i += step;
        }
        for w in frames.windows(2) {
            let n = w[0].len();
            for k in 0..n {
                let j = (k + 1) % n;
                self.sweep
                    .preview
                    .extend([w[0][k], w[0][j], w[1][j], w[0][k], w[1][j], w[1][k]]);
            }
        }
    }

    pub(super) fn apply_sweep(&mut self) {
        let (Some(profile), Some(path)) = (self.sweep.profile, self.sweep.path.clone()) else {
            self.set_status("Choose the profile and the path first", true);
            return;
        };
        if !self.idle() {
            return;
        }
        self.pending = None;
        self.submit(Command::Sweep {
            profile,
            path,
            mode: self.sweep.mode,
            rounded: self.sweep.rounded,
        });
        self.sweep.preview.clear();
        self.layers_changed = true;
        self.set_status("Computing…", false);
    }

    pub(super) fn sweep_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        if self.tool != Tool::Sweep {
            return;
        }
        let mut apply = false;
        let mut clear = false;
        let mut changed = false;
        let mut docks = std::mem::take(&mut self.docks);
        docks.panel(
            ctx,
            "sweep",
            Pos2::new(rect.max.x - 250.0, rect.min.y + 10.0),
            None,
            230.0,
            &tr("Sweep"),
            false,
            |ui| {
                let profile = match self.sweep.profile {
                    None => tr("click the profile"),
                    Some(LoftRef::Region { sketch, .. }) => {
                        self.doc.sketch(sketch).map_or("?".into(), |s| s.name.clone())
                    }
                    Some(LoftRef::Face { body, .. }) => {
                        self.doc.body(body).map_or("?".into(), |b| b.name.clone())
                    }
                    Some(LoftRef::EdgeLoop { body, .. }) => self
                        .doc
                        .body(body)
                        .map_or("?".into(), |b| format!("{} ({})", b.name, tr("edge"))),
                };
                ui.horizontal(|ui| {
                    ui.colored_label(crate::theme::rgb(255, 184, 64), tr("Profile"));
                    ui.label(profile);
                });
                let path = match &self.sweep.path {
                    None => tr("click the path"),
                    Some(SweepPath::Curve(id)) => self.doc.curve(*id).map_or("?".into(), |c| c.name.clone()),
                    Some(SweepPath::Sketch { sketch, entities }) => trf(
                        "{} ({} shapes)",
                        &[
                            self.doc.sketch(*sketch).map_or("?".into(), |s| s.name.clone()),
                            entities.len().to_string(),
                        ],
                    ),
                    Some(SweepPath::Edges { edges, .. }) => trf("{} edges", &[edges.len().to_string()]),
                };
                ui.horizontal(|ui| {
                    ui.colored_label(crate::theme::rgb(110, 220, 255), tr("Path"));
                    ui.label(path);
                });
                ui.horizontal(|ui| {
                    for (m, name) in [
                        (SweepMode::Auto, "Auto"),
                        (SweepMode::NewBody, "New body"),
                        (SweepMode::Add, "Add"),
                        (SweepMode::Cut, "Cut"),
                    ] {
                        ui.selectable_value(&mut self.sweep.mode, m, tr(name));
                    }
                });
                if ui
                    .checkbox(&mut self.sweep.rounded, tr("Rounded corners"))
                    .changed()
                {
                    changed = true;
                }
                ui.horizontal(|ui| {
                    let ready = self.sweep.profile.is_some() && self.sweep.path.is_some();
                    if ui.add_enabled(ready, egui::Button::new(tr("Apply"))).clicked() {
                        apply = true;
                    }
                    if ui.button(tr("Clear")).clicked() {
                        clear = true;
                    }
                });
            },
        );
        self.docks = docks;
        let _ = changed;
        if apply {
            self.apply_sweep();
        }
        if clear {
            self.sweep = SweepUi::default();
            self.layers_changed = true;
        }
    }
}

#[cfg(test)]
mod tolerance_tests {
    use super::curve_tolerance;

    #[test]
    fn the_curve_tolerance_follows_the_pixel_size_in_powers_of_two() {
        // 0.4 of a pixel, rounded down to a power of two
        let t = curve_tolerance(0.1);
        assert!(t <= 0.04 && t > 0.02, "{t}");
        assert_eq!(curve_tolerance(0.1), curve_tolerance(0.12));
        assert!(curve_tolerance(0.01) < curve_tolerance(0.1));
        // limits
        assert!(curve_tolerance(1e-9) >= 0.001);
        assert!(curve_tolerance(1e6) <= 0.5);
        assert_eq!(curve_tolerance(f64::NAN), 0.05);
    }
}
