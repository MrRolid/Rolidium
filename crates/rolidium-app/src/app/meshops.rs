// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The panels "Simplify the mesh" (the target as a share or a number of triangles, sharp edges and rims of holes kept, a preview of the result next to the
//! original with the same view, the deviation, one step of Undo) and "Stretch across a plane" (a plane by a click on a flat face or by an axis and a
//! position, a distance, a preview for a mesh, one step of Undo).

use super::meshtool::fmt_len;
use super::*;
use crate::i18n::{parse_num, tr, trf};
use rolidium_doc::meshes::SimplifyTarget;
use rolidium_mesh::simplify::{self, Simplified};
use rolidium_mesh::TriMesh;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

/// The number the display gives the preview mesh (no body has it).
pub const PREVIEW_ID: BodyId = u64::MAX - 7;

/// A mesh shown in the model area that is not a body of the document: the result of a simplification next to the original, or the stretched mesh in its place.
#[derive(Clone)]
pub struct PreviewMesh {
    pub mesh: Arc<TriMesh>,
    /// The body that is not drawn while the preview is (the stretch is drawn in its place).
    pub hides: Option<BodyId>,
}

struct SimplifyJob {
    body: BodyId,
    src: Arc<TriMesh>,
    stop: Arc<AtomicBool>,
    rx: mpsc::Receiver<Option<Simplified>>,
    started: std::time::Instant,
}

pub struct SimplifyUi {
    pub by_percent: bool,
    pub percent: String,
    pub triangles: String,
    pub keep_features: bool,
    job: Option<SimplifyJob>,
    /// The result that is shown: the mesh it was made from, and the numbers.
    result: Option<(BodyId, Arc<TriMesh>, SimplifySummary)>,
}

#[derive(Clone)]
struct SimplifySummary {
    before: usize,
    after: usize,
    max_dev: f64,
    mean_dev: f64,
    seconds: f64,
    reached: bool,
    reason: String,
}

impl Default for SimplifyUi {
    fn default() -> Self {
        SimplifyUi {
            by_percent: true,
            percent: "50".into(),
            triangles: String::new(),
            keep_features: true,
            job: None,
            result: None,
        }
    }
}

#[derive(Default)]
pub struct StretchUi {
    /// The plane: a point and the normal, and what it is called.
    pub plane: Option<(DVec3, DVec3, String)>,
    pub position: String,
    pub distance: String,
    /// The next click on a flat face is the plane.
    pub pick: bool,
    pub preview: bool,
    /// The last preview was made for these inputs (point, normal, distance, body, mesh address).
    shown: Option<(BodyId, [f64; 7])>,
}

fn axis_vec(i: usize) -> DVec3 {
    [DVec3::X, DVec3::Y, DVec3::Z][i]
}

impl App {
    /// The meshes the display gets beside the bodies of the document: the preview, and without the one the preview replaces.
    pub(super) fn display_meshes(&self) -> Vec<rolidium_doc::display::MeshInput> {
        let mut v = rolidium_doc::display::mesh_inputs(self.doc.state());
        if let Some(p) = &self.preview_mesh {
            if let Some(h) = p.hides {
                v.retain(|m| m.id != h);
            }
            v.push(rolidium_doc::display::MeshInput {
                id: PREVIEW_ID,
                name: "Preview".into(),
                mesh: p.mesh.clone(),
                color: [0.55, 0.72, 0.92],
                regions: None,
            });
        }
        v
    }

    pub(super) fn simplify_probe(&self) -> serde_json::Value {
        serde_json::json!({
            "running": self.simplify.job.is_some(),
            "result": self.simplify.result.as_ref().map(|(b, _, s)| serde_json::json!({"body": b, "before": s.before, "after": s.after, "max_dev": s.max_dev, "reached": s.reached})),
        })
    }

    pub(super) fn simplify_result_for(&self, body: Option<BodyId>) -> bool {
        self.simplify
            .result
            .as_ref()
            .is_some_and(|(b, _, _)| Some(*b) == body)
            || self.simplify.job.is_some()
    }

    pub(super) fn set_preview(&mut self, p: Option<PreviewMesh>) {
        self.preview_mesh = p;
        self.request_meshes();
        self.layers_changed = true;
    }

    /// Forgets the previews without drawing again (a command follows, and the display is made again when it is done).
    fn forget_previews(&mut self) {
        if let Some(j) = self.simplify.job.take() {
            j.stop.store(true, Ordering::Relaxed);
        }
        self.simplify.result = None;
        self.stretch.shown = None;
        self.preview_mesh = None;
        self.layers_changed = true;
    }

    /// Drops the preview of both panels (the selection changed, the command was made).
    pub(super) fn drop_mesh_previews(&mut self) {
        if let Some(j) = self.simplify.job.take() {
            j.stop.store(true, Ordering::Relaxed);
        }
        self.simplify.result = None;
        self.stretch.shown = None;
        if self.preview_mesh.is_some() {
            self.set_preview(None);
        }
    }

    // ---- simplify -------------------------------------------------------------------------------------------------------------------------------------

    fn simplify_target(&self, triangles: usize) -> Option<SimplifyTarget> {
        if self.simplify.by_percent {
            parse_num(&self.simplify.percent)
                .filter(|p| (0.01..=100.0).contains(p))
                .map(SimplifyTarget::Percent)
        } else {
            parse_num(&self.simplify.triangles)
                .filter(|n| *n >= 4.0 && *n <= triangles as f64)
                .map(|n| SimplifyTarget::Triangles(n as usize))
        }
    }

    fn poll_simplify(&mut self, ctx: &egui::Context) {
        let Some(j) = &self.simplify.job else { return };
        match j.rx.try_recv() {
            Ok(Some(r)) => {
                let (body, src, started) = (j.body, j.src.clone(), j.started);
                self.simplify.job = None;
                let summary = SimplifySummary {
                    before: r.triangles_before,
                    after: r.triangles_after,
                    max_dev: r.max_deviation,
                    mean_dev: r.mean_deviation,
                    seconds: started.elapsed().as_secs_f64(),
                    reached: r.reached,
                    reason: r.reason.clone(),
                };
                // the result stands next to the original: the same view turns and zooms both
                let mut shown = r.mesh;
                if let Some((lo, hi)) = src.bounding_box() {
                    let dx = ((hi[0] - lo[0]) * 1.25).max(1.0) as f32;
                    for p in &mut shown.positions {
                        p[0] += dx;
                    }
                }
                self.simplify.result = Some((body, src, summary));
                self.set_preview(Some(PreviewMesh {
                    mesh: Arc::new(shown),
                    hides: None,
                }));
                self.set_status("Original on the left, the simplified mesh on the right", false);
            }
            Ok(None) | Err(mpsc::TryRecvError::Disconnected) => {
                self.simplify.job = None;
            }
            Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(80)),
        }
    }

    fn simplify_ui(&mut self, ui: &mut egui::Ui, id: BodyId) {
        let Some(mb) = self.doc.mesh(id).cloned() else {
            return;
        };
        self.poll_simplify(ui.ctx());
        if self
            .simplify
            .result
            .as_ref()
            .is_some_and(|(b, src, _)| *b != id || !Arc::ptr_eq(src, &mb.mesh))
        {
            self.simplify.result = None;
            if self.preview_mesh.as_ref().is_some_and(|p| p.hides.is_none()) {
                self.set_preview(None);
            }
        }
        ui.add_space(6.0);
        let h = egui::CollapsingHeader::new(egui::RichText::new(tr("Simplify the mesh")).strong())
            .id_salt("simplify_header")
            .default_open(false)
            .show(ui, |ui| self.simplify_body(ui, id, &mb));
        self.probe_widget("simplify_header", h.header_response.rect);
    }

    fn simplify_body(&mut self, ui: &mut egui::Ui, id: BodyId, mb: &rolidium_doc::meshes::MeshBody) {
        let n = mb.mesh.triangles.len();
        ui.horizontal(|ui| {
            ui.radio_value(
                &mut self.simplify.by_percent,
                true,
                tr("Percent of the triangles"),
            );
            ui.radio_value(&mut self.simplify.by_percent, false, tr("Number of triangles"));
        });
        ui.horizontal(|ui| {
            if self.simplify.by_percent {
                let r = ui.add(egui::TextEdit::singleline(&mut self.simplify.percent).desired_width(50.0));
                self.probe_widget("simplify_value", r.rect);
                ui.label("%");
            } else {
                if self.simplify.triangles.is_empty() {
                    self.simplify.triangles = (n / 2).max(4).to_string();
                }
                let r = ui.add(egui::TextEdit::singleline(&mut self.simplify.triangles).desired_width(80.0));
                self.probe_widget("simplify_value", r.rect);
            }
        });
        let r = ui.checkbox(
            &mut self.simplify.keep_features,
            tr("Keep sharp edges and the rims of holes"),
        );
        self.probe_widget("simplify_keep", r.rect);
        let target = self.simplify_target(n);
        if target.is_none() {
            ui.colored_label(
                Color32::from_rgb(220, 90, 70),
                tr(if self.simplify.by_percent {
                    "The share must be a number from 0.01 to 100"
                } else {
                    "The number must be from 4 to the number of triangles of the mesh"
                }),
            );
        }
        let running = self.simplify.job.is_some();
        ui.horizontal(|ui| {
            let r = ui.add_enabled(
                target.is_some() && !running && self.idle(),
                egui::Button::new(tr("Show the result")),
            );
            self.probe_widget("simplify_preview", r.rect);
            if r.clicked() {
                if let Some(t) = target {
                    self.start_simplify(id, mb.mesh.clone(), t);
                }
            }
            if running {
                ui.spinner();
                if ui.button(tr("Stop")).clicked() {
                    if let Some(j) = self.simplify.job.take() {
                        j.stop.store(true, Ordering::Relaxed);
                    }
                }
            }
        });
        if let Some((_, _, s)) = self.simplify.result.clone() {
            ui.label(trf(
                "{} triangles instead of {} ({} %)",
                &[
                    s.after.to_string(),
                    s.before.to_string(),
                    format!("{:.1}", 100.0 * s.after as f64 / s.before.max(1) as f64),
                ],
            ));
            ui.label(trf(
                "Deviation from the original: largest {} mm, mean {} mm",
                &[format!("{:.4}", s.max_dev), format!("{:.5}", s.mean_dev)],
            ));
            ui.weak(trf("Made in {} s", &[format!("{:.2}", s.seconds)]));
            if !s.reached {
                ui.colored_label(
                    Color32::from_rgb(230, 170, 60),
                    trf("The target was not reached: {}", std::slice::from_ref(&s.reason)),
                );
            }
            ui.horizontal(|ui| {
                let r = ui.add_enabled(
                    self.idle() && s.after < s.before,
                    egui::Button::new(tr("Apply the simplification")),
                );
                self.probe_widget("simplify_apply", r.rect);
                if r.clicked() {
                    if let Some(t) = target {
                        self.forget_previews();
                        self.mesh_ui.keep_body = Some(id);
                        self.submit(Command::MeshSimplify {
                            body: id,
                            target: t,
                            keep_features: self.simplify.keep_features,
                        });
                        self.set_status("Simplifying the mesh… (Esc stops it)", false);
                    }
                }
                if ui.button(tr("Drop the preview")).clicked() {
                    self.drop_mesh_previews();
                }
            });
        }
    }

    fn start_simplify(&mut self, body: BodyId, mesh: Arc<TriMesh>, target: SimplifyTarget) {
        self.drop_mesh_previews();
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (stop2, mesh2) = (stop.clone(), mesh.clone());
        let o = simplify::Options {
            target: match target {
                SimplifyTarget::Percent(p) => simplify::Target::Percent(p),
                SimplifyTarget::Triangles(n) => simplify::Target::Triangles(n),
            },
            keep_features: self.simplify.keep_features,
            ..Default::default()
        };
        std::thread::spawn(move || {
            let r = simplify::simplify(&mesh2, &o, &|| stop2.load(Ordering::Relaxed));
            let _ = tx.send(r);
        });
        self.simplify.job = Some(SimplifyJob {
            body,
            src: mesh,
            stop,
            rx,
            started: std::time::Instant::now(),
        });
        self.set_status("Simplifying the mesh for the preview…", false);
    }

    // ---- stretch --------------------------------------------------------------------------------------------------------------------------------------

    /// The box of a body (solid or mesh).
    fn body_box(&self, id: BodyId) -> Option<(DVec3, DVec3)> {
        if let Some(m) = self.doc.mesh(id) {
            let (lo, hi) = m.mesh.bounding_box()?;
            return Some((DVec3::from_array(lo), DVec3::from_array(hi)));
        }
        self.doc.body(id)?.solid.bounding_box().ok()
    }

    /// A click on a flat face while the plane is asked for.
    pub(super) fn stretch_pick_click(&mut self, resp: &egui::Response) -> bool {
        if !self.stretch.pick {
            return false;
        }
        if resp.clicked_by(PointerButton::Primary) {
            if let Some((b, f)) = self.hovered_face() {
                if let Some(m) = self.entry(b).and_then(|e| e.faces.get(f as usize)) {
                    if m.kind == FaceKind::Plane {
                        self.stretch.plane = Some((m.origin, m.normal, tr("a face")));
                        self.stretch.position.clear();
                        self.stretch.pick = false;
                        self.stretch.shown = None;
                        return true;
                    }
                }
            }
            self.set_status("Click a flat face of a body for the plane", true);
        }
        true
    }

    fn stretch_inputs(&self) -> Option<(DVec3, DVec3, f64)> {
        let (p, n, _) = self.stretch.plane.clone()?;
        let d = parse_num(&self.stretch.distance).filter(|d| d.is_finite() && d.abs() >= 1e-6)?;
        let pos = parse_num(&self.stretch.position);
        // an axis plane: the typed position moves it along the normal
        let point = match pos {
            Some(v) => p + n * (v - p.dot(n)),
            None => p,
        };
        Some((point, n, d))
    }

    pub(super) fn stretch_ui(&mut self, ui: &mut egui::Ui, id: BodyId) {
        let is_mesh = self.doc.mesh(id).is_some();
        let Some((lo, hi)) = self.body_box(id) else { return };
        ui.add_space(6.0);
        let h = egui::CollapsingHeader::new(egui::RichText::new(tr("Stretch across a plane")).strong())
            .id_salt("stretch_header")
            .default_open(false)
            .show(ui, |ui| self.stretch_body(ui, id, is_mesh, lo, hi));
        self.probe_widget("stretch_header", h.header_response.rect);
    }

    fn stretch_body(&mut self, ui: &mut egui::Ui, id: BodyId, is_mesh: bool, lo: DVec3, hi: DVec3) {
        ui.weak(tr(
            "The part beyond the plane moves and the gap is filled with the cross-section: walls grow, rounds, holes and pockets keep their size",
        ));
        ui.horizontal(|ui| {
            for (i, name) in ["X", "Y", "Z"].iter().enumerate() {
                let r = ui.button(trf("Plane {}", &[name.to_string()]));
                self.probe_widget(&format!("stretch_axis_{i}"), r.rect);
                if r.clicked() {
                    let mid = (lo + hi) * 0.5;
                    let n = axis_vec(i);
                    self.stretch.plane = Some((mid, n, (*name).to_string()));
                    self.stretch.position = fmt_len(mid.dot(n));
                    self.stretch.pick = false;
                    self.stretch.shown = None;
                }
            }
            let r = ui.selectable_label(self.stretch.pick, tr("Click a flat face"));
            self.probe_widget("stretch_pick", r.rect);
            if r.clicked() {
                self.stretch.pick = !self.stretch.pick;
                if self.stretch.pick {
                    self.set_status("Click a flat face of a body for the plane", false);
                }
            }
        });
        if let Some((p, n, label)) = self.stretch.plane.clone() {
            ui.label(trf(
                "Plane: {}, normal {} {} {}, through {} {} {}",
                &[
                    label,
                    format!("{:.3}", n.x),
                    format!("{:.3}", n.y),
                    format!("{:.3}", n.z),
                    fmt_len(p.x),
                    fmt_len(p.y),
                    fmt_len(p.z),
                ],
            ));
            ui.horizontal(|ui| {
                ui.label(tr("Position along the normal (mm)"));
                if self.stretch.position.is_empty() {
                    self.stretch.position = fmt_len(p.dot(n));
                }
                let r = ui.add(egui::TextEdit::singleline(&mut self.stretch.position).desired_width(60.0));
                self.probe_widget("stretch_position", r.rect);
            });
            ui.horizontal(|ui| {
                ui.label(tr("Stretch by (mm)"));
                let r = ui.add(egui::TextEdit::singleline(&mut self.stretch.distance).desired_width(60.0));
                self.probe_widget("stretch_distance", r.rect);
            });
            {
                let r = ui.checkbox(&mut self.stretch.preview, tr("Show the result"));
                self.probe_widget("stretch_preview", r.rect);
                if !self.stretch.preview && self.preview_mesh.as_ref().is_some_and(|p| p.hides == Some(id)) {
                    self.set_preview(None);
                    self.stretch.shown = None;
                }
            }
            let inputs = self.stretch_inputs();
            let r = ui.add_enabled(inputs.is_some() && self.idle(), egui::Button::new(tr("Stretch")));
            self.probe_widget("stretch_apply", r.rect);
            if r.clicked() {
                if let Some((point, normal, distance)) = inputs {
                    self.forget_previews();
                    self.mesh_ui.keep_body = Some(id);
                    self.submit(Command::StretchInPlane {
                        body: id,
                        point,
                        normal,
                        distance,
                    });
                    self.set_status("Stretching… (Esc stops it)", false);
                }
            } else if self.stretch.preview {
                if is_mesh {
                    self.stretch_preview(id, inputs);
                } else {
                    self.stretch_preview_solid(id, inputs);
                }
            }
        }
    }

    /// The stretched mesh is drawn in place of the original (a mesh of up to 600 000 triangles is stretched in a few tenths of a second).
    fn stretch_preview(&mut self, id: BodyId, inputs: Option<(DVec3, DVec3, f64)>) {
        let Some((point, normal, distance)) = inputs else {
            return;
        };
        let key = [point.x, point.y, point.z, normal.x, normal.y, normal.z, distance];
        if self.stretch.shown == Some((id, key)) {
            return;
        }
        self.stretch.shown = Some((id, key));
        let Some(mb) = self.doc.mesh(id) else { return };
        if mb.mesh.triangles.len() > 600_000 {
            self.set_status(
                "The mesh is too big for a preview; the stretch itself is made at Stretch",
                false,
            );
            return;
        }
        match rolidium_mesh::planesweep::sweep_in_plane(
            &mb.mesh,
            point.to_array(),
            normal.to_array(),
            distance,
        ) {
            Ok(r) => {
                let text = trf(
                    "Stretched: the cross-section has {} loops",
                    &[r.loops.to_string()],
                );
                self.set_preview(Some(PreviewMesh {
                    mesh: Arc::new(r.mesh),
                    hides: Some(id),
                }));
                self.set_status(text, false);
            }
            Err(e) => {
                self.set_preview(None);
                self.set_status(e, true);
            }
        }
    }

    /// The stretched solid is drawn in place of the original (made by the kernel: a body of up to 150 faces, a few tenths of a second).
    fn stretch_preview_solid(&mut self, id: BodyId, inputs: Option<(DVec3, DVec3, f64)>) {
        let Some((point, normal, distance)) = inputs else {
            return;
        };
        let key = [point.x, point.y, point.z, normal.x, normal.y, normal.z, distance];
        if self.stretch.shown == Some((id, key)) {
            return;
        }
        self.stretch.shown = Some((id, key));
        let Some(solid) = self.doc.body(id).map(|b| b.solid.clone()) else {
            return;
        };
        if solid.face_count() > 150 {
            self.set_preview(None);
            self.set_status(
                "The body has too many faces for a preview; the stretch itself is made at Stretch",
                false,
            );
            return;
        }
        let made = solid
            .stretched(point, normal, distance)
            .and_then(|s| s.display_mesh());
        match made {
            Ok(m) => {
                let mesh = TriMesh {
                    positions: m.positions.iter().map(|p| [p.x, p.y, p.z]).collect(),
                    triangles: m.indices.as_chunks::<3>().0.to_vec(),
                };
                self.set_preview(Some(PreviewMesh {
                    mesh: Arc::new(mesh),
                    hides: Some(id),
                }));
                self.set_status("Stretched: this is how the body will look", false);
            }
            Err(e) => {
                self.set_preview(None);
                self.set_status(e.to_string(), true);
            }
        }
    }

    /// The plane of the stretch in the model area: a frame round the body and an arrow of the distance.
    pub(super) fn stretch_layers(&self) -> Vec<PreviewDraw> {
        let (Some(id), Some((p, n, _))) = (self.body_sel, self.stretch.plane.clone()) else {
            return Vec::new();
        };
        let Some((lo, hi)) = self.body_box(id) else {
            return Vec::new();
        };
        let pos = parse_num(&self.stretch.position);
        let p = match pos {
            Some(v) => p + n * (v - p.dot(n)),
            None => p,
        };
        let r = ((hi - lo).length() * 0.6).max(1.0);
        let helper = if n.cross(DVec3::Z).length() > 1e-6 {
            DVec3::Z
        } else {
            DVec3::X
        };
        let u = n.cross(helper).normalize() * r;
        let v = n.cross(u).normalize() * r;
        let c = ((lo + hi) * 0.5) - n * (((lo + hi) * 0.5 - p).dot(n));
        let corner = |a: f64, b: f64| (c + u * a + v * b).as_vec3();
        let pts = [
            corner(-1.0, -1.0),
            corner(1.0, -1.0),
            corner(1.0, 1.0),
            corner(-1.0, 1.0),
        ];
        let mut thick = Vec::new();
        for i in 0..4 {
            thick.push(pts[i]);
            thick.push(pts[(i + 1) % 4]);
        }
        if let Some(d) = parse_num(&self.stretch.distance) {
            thick.push(c.as_vec3());
            thick.push((c + n * d).as_vec3());
        }
        vec![PreviewDraw {
            thick,
            thick_color: [0.25, 0.85, 1.0, 1.0],
            ..Default::default()
        }]
    }

    /// Both panels of a selected body.
    pub(super) fn mesh_ops_ui(&mut self, ui: &mut egui::Ui, id: BodyId) {
        if self.stretch.plane.is_some() || self.stretch.pick {
            // the plane stays while the same body is selected
        }
        if self.doc.mesh(id).is_some() {
            self.simplify_ui(ui, id);
        }
        self.stretch_ui(ui, id);
    }
}
