// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The Text tool (G): text raised from a flat face of a body or engraved into it, on a flat wall of a mesh (Manifold), or into a sketch. A click on the face puts the
//! text there, the panel has the words, the font, the height of the capital letters, the alignment, the turn and the spacing, and how far the letters go. The outlines
//! of the letters are drawn on the face while the panel is open. On a body the text is a link: its panel opens again with a click on it in the list of the document.

use super::*;
use crate::i18n::{tr, trf};
use rolidium_doc::{LinkId, LinkKind, TextAlign, TextParams};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TextTarget {
    Body(BodyId),
    Mesh(BodyId),
    /// A sketch that lies on no body: the letters become a body of their own.
    Sketch(SketchId),
}

pub struct TextUi {
    pub target: Option<TextTarget>,
    pub origin: DVec3,
    pub normal: DVec3,
    pub x_dir: DVec3,
    pub text: String,
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    pub height: f64,
    pub align: TextAlign,
    pub rotation: f64,
    pub spacing: f64,
    pub depth: f64,
    pub engrave: bool,
    pub editing: Option<LinkId>,
    /// A click in the sketch places the text (the tool was chosen inside a sketch).
    pub wait_sketch: bool,
    pub error: Option<String>,
    pub missing: Vec<char>,
    /// The outlines of the letters in space, as pairs of points (for the view).
    pub outline: Vec<Vec3>,
    key: String,
    focus_text: bool,
}

impl Default for TextUi {
    fn default() -> Self {
        TextUi {
            target: None,
            origin: DVec3::ZERO,
            normal: DVec3::Z,
            x_dir: DVec3::X,
            text: "Text".into(),
            family: "sans".into(),
            bold: false,
            italic: false,
            height: 5.0,
            align: TextAlign::Left,
            rotation: 0.0,
            spacing: 0.0,
            depth: 1.0,
            engrave: false,
            editing: None,
            wait_sketch: false,
            error: None,
            missing: Vec::new(),
            outline: Vec::new(),
            key: String::new(),
            focus_text: false,
        }
    }
}

/// The direction of the writing on a face: along X on a face that looks up or down, else the way a person standing in front of the face reads.
pub fn writing_direction(normal: DVec3) -> DVec3 {
    let n = normal.normalize_or_zero();
    if n.z.abs() > 0.9 {
        return DVec3::X;
    }
    let x = DVec3::Z.cross(n).normalize_or_zero();
    if x == DVec3::ZERO {
        DVec3::X
    } else {
        x
    }
}

impl TextUi {
    pub fn params(&self, target: BodyId) -> TextParams {
        TextParams {
            target,
            text: self.text.clone(),
            font: self.family.clone(),
            bold: self.bold,
            italic: self.italic,
            height: self.height,
            align: self.align,
            rotation: self.rotation,
            spacing: self.spacing,
            depth: self.depth,
            engrave: self.engrave,
            origin: self.origin,
            normal: self.normal,
            x_dir: self.x_dir,
        }
    }
}

impl App {
    pub(super) fn activate_text(&mut self) {
        let in_sketch = self.sk.as_ref().is_some_and(|s| s.sketch.is_some());
        let keep = std::mem::take(&mut self.text);
        // the numbers of the last text are kept (the height, the font), the place and the words are new
        self.text = TextUi {
            family: keep.family,
            bold: keep.bold,
            italic: keep.italic,
            height: keep.height,
            align: keep.align,
            spacing: keep.spacing,
            depth: keep.depth,
            engrave: keep.engrave,
            ..TextUi::default()
        };
        self.drop_mesh_previews();
        if in_sketch {
            self.text.wait_sketch = true;
            self.sketch_select_tool();
            self.set_status("Text: click the place in the sketch where the text starts", false);
            return;
        }
        self.leave_sketch();
        self.tool = Tool::Text;
        self.clear_selection();
        self.layers_changed = true;
        // a flat face that is selected is the place
        self.set_status(
            "Text: click a flat face of a body, a flat wall of a mesh, or a sketch to put the text there",
            false,
        );
    }

    /// The panel of a text that exists is opened (a click on it in the list of the document).
    pub(super) fn open_text_link(&mut self, id: LinkId) {
        let Some(l) = self.doc.state().links.iter().find(|l| l.id == id).cloned() else {
            return;
        };
        let LinkKind::Text(p) = &l.kind else { return };
        self.leave_sketch();
        self.tool = Tool::Text;
        self.drop_mesh_previews();
        self.clear_selection();
        self.text = TextUi {
            target: Some(TextTarget::Body(p.target)),
            origin: p.origin,
            normal: p.normal,
            x_dir: p.x_dir,
            text: p.text.clone(),
            family: p.font.clone(),
            bold: p.bold,
            italic: p.italic,
            height: p.height,
            align: p.align,
            rotation: p.rotation,
            spacing: p.spacing,
            depth: p.depth,
            engrave: p.engrave,
            editing: Some(id),
            focus_text: true,
            ..TextUi::default()
        };
        self.link_sel = Some(id);
        self.text.key.clear();
        self.layers_changed = true;
        self.set_status("Text: change the words or the numbers, click another face to move it, Apply confirms", false);
    }

    /// A click on a flat face puts the text there.
    pub(super) fn text_input(&mut self, resp: &egui::Response, pointer: Option<Pos2>, rect: Rect, hovered_face: Option<(BodyId, u32)>) {
        if !resp.clicked_by(PointerButton::Primary) {
            return;
        }
        let (Some(p), Some((b, f))) = (pointer, hovered_face) else {
            return;
        };
        let Some(meta) = self.entry(b).and_then(|m| m.faces.get(f as usize)).cloned() else {
            return;
        };
        if meta.kind != FaceKind::Plane {
            self.set_status("The text goes on a flat face; this face is curved", true);
            return;
        }
        let aspect = rect.width() / rect.height().max(1.0);
        let (o, d) = self.camera.ray(Self::ndc(p, rect), aspect);
        let (o, d) = (o.as_dvec3(), d.as_dvec3());
        let n = meta.normal.normalize_or_zero();
        let denom = d.dot(n);
        let c = meta.centroid.as_dvec3();
        let hit = if denom.abs() > 1e-9 {
            o + d * ((c - o).dot(n) / denom)
        } else {
            c
        };
        let target = if self.doc.mesh(b).is_some() {
            TextTarget::Mesh(b)
        } else {
            TextTarget::Body(b)
        };
        // another body: this is a new text (a text that is being changed stays on its own body)
        if self.text.editing.is_some() && self.text.target != Some(target) {
            self.set_status("A text stays on its own body; remove it and make a new one on this one", true);
            return;
        }
        self.text.target = Some(target);
        self.text.origin = hit;
        self.text.normal = n;
        self.text.x_dir = writing_direction(n);
        self.text.focus_text = true;
        self.text.key.clear();
        self.layers_changed = true;
    }

    /// A click in a sketch while the tool waits for the place.
    pub(super) fn sketch_text_click(&mut self) -> bool {
        if !self.text.wait_sketch {
            return false;
        }
        let Some(sm) = &self.sk else { return false };
        let (Some(sk), Some(snap)) = (sm.sketch, sm.snap) else {
            return false;
        };
        let plane = sm.plane;
        let target = self
            .doc
            .sketch(sk)
            .and_then(|s| s.target)
            .filter(|b| self.doc.body(*b).is_some())
            .map(TextTarget::Body)
            .unwrap_or(TextTarget::Sketch(sk));
        self.text.target = Some(target);
        self.text.origin = plane.to_3d(snap.pos);
        self.text.normal = plane.normal.normalize();
        self.text.x_dir = plane.x_dir;
        self.text.focus_text = true;
        self.text.key.clear();
        self.layers_changed = true;
        true
    }

    /// The outlines of the letters are laid out when the numbers change.
    fn text_refresh(&mut self) {
        let Some(t) = self.text.target else { return };
        let b = match t {
            TextTarget::Body(b) | TextTarget::Mesh(b) => b,
            TextTarget::Sketch(s) => s,
        };
        let p = self.text.params(b);
        let key = format!("{p:?}");
        if key == self.text.key {
            return;
        }
        self.text.key = key;
        self.layers_changed = true;
        self.text.outline.clear();
        self.text.error = None;
        self.text.missing.clear();
        if p.height <= 0.0 || p.text.trim().is_empty() {
            self.text.error = Some(if p.text.trim().is_empty() {
                "Type the text".to_string()
            } else {
                "The height of the letters must be a positive number".to_string()
            });
            return;
        }
        let n = p.normal.normalize_or_zero();
        let x = (p.x_dir - n * p.x_dir.dot(n)).normalize_or_zero();
        let y = n.cross(x);
        match rolidium_text::layout(&p.text, &p.style()) {
            Ok(l) => {
                self.text.missing = l.missing.clone();
                let lift = n * 0.03;
                for c in &l.contours {
                    for i in 0..c.len() {
                        let (a, b2) = (c[i], c[(i + 1) % c.len()]);
                        let pa = p.origin + x * a.x + y * a.y + lift;
                        let pb = p.origin + x * b2.x + y * b2.y + lift;
                        self.text.outline.push(pa.as_vec3());
                        self.text.outline.push(pb.as_vec3());
                    }
                }
            }
            Err(e) => self.text.error = Some(e.to_string()),
        }
    }

    pub(super) fn text_layers(&self) -> Vec<PreviewDraw> {
        let mut out = Vec::new();
        if self.text.target.is_none() || self.text.outline.is_empty() {
            return out;
        }
        out.push(PreviewDraw {
            thick: self.text.outline.clone(),
            thick_color: [1.0, 0.72, 0.2, 1.0],
            ..Default::default()
        });
        // a cross where the text starts
        let o = self.text.origin.as_vec3();
        let n = self.text.normal.normalize_or_zero().as_vec3() * 0.05;
        let x = self.text.x_dir.normalize_or_zero().as_vec3() * 1.2;
        let y = self.text.normal.normalize_or_zero().cross(self.text.x_dir.normalize_or_zero()).as_vec3() * 1.2;
        out.push(PreviewDraw {
            thick: vec![o - x + n, o + x + n, o - y + n, o + y + n],
            thick_color: [0.4, 0.9, 1.0, 1.0],
            ..Default::default()
        });
        out
    }

    pub(super) fn text_close(&mut self) {
        self.text = TextUi::default();
        self.drop_mesh_previews();
        if self.tool == Tool::Text {
            self.tool = Tool::Select;
        }
        self.layers_changed = true;
    }

    pub(super) fn text_apply(&mut self) {
        let Some(t) = self.text.target else {
            self.set_status("Click the place for the text first", true);
            return;
        };
        if let Some(e) = &self.text.error {
            let e = e.clone();
            self.set_status(e, true);
            return;
        }
        let editing = self.text.editing;
        let cmd = match t {
            TextTarget::Body(b) => {
                let kind = LinkKind::Text(self.text.params(b));
                match editing {
                    Some(link) => Command::EditLink {
                        link,
                        kind: Some(kind),
                        name: None,
                    },
                    None => Command::AddLink {
                        kind,
                        sources: Vec::new(),
                        name: String::new(),
                    },
                }
            }
            TextTarget::Mesh(b) => Command::MeshText {
                body: b,
                params: self.text.params(b),
            },
            TextTarget::Sketch(s) => {
                // letters that stand on no body: a body of their own, raised from the plane of the sketch
                let mut p = self.text.params(s);
                p.engrave = false;
                match rolidium_doc::text_tool(&p) {
                    Ok(solid) => Command::AddNamedSolids {
                        solids: vec![(solid, self.text.text.lines().next().unwrap_or("Text").trim().to_string())],
                    },
                    Err(e) => {
                        self.set_status(e.to_string(), true);
                        return;
                    }
                }
            }
        };
        self.submit(cmd);
        self.text_close();
        self.leave_sketch();
        self.set_status("Computing…", false);
    }

    #[allow(deprecated)]
    pub(super) fn text_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        if self.tool != Tool::Text && !self.text.wait_sketch {
            return;
        }
        self.text_refresh();
        let mut docks = std::mem::take(&mut self.docks);
        let mut u = std::mem::take(&mut self.text);
        let target_name = u.target.map(|t| match t {
            TextTarget::Body(b) => self.doc.body(b).map(|x| x.name.clone()).unwrap_or_default(),
            TextTarget::Mesh(b) => self.doc.mesh(b).map(|x| format!("{} ({})", x.name, tr("mesh"))).unwrap_or_default(),
            TextTarget::Sketch(_) => tr("a new body"),
        });
        let mut apply = false;
        let mut cancel = false;
        let mut remove = false;
        let mut unlink = false;
        let editing = u.editing.is_some();
        let probe_w: std::cell::RefCell<Vec<(&'static str, Rect)>> = std::cell::RefCell::new(Vec::new());
        let title = if editing { tr("Text (a link)") } else { tr("Text") };
        docks.panel(ctx, "text", Pos2::new(rect.max.x - 290.0, rect.min.y + 10.0), None, 270.0, &title, false, |ui| {
            if u.target.is_none() {
                ui.label(
                    egui::RichText::new(tr("Click the place for the text"))
                        .size(16.0)
                        .color(crate::theme::rgb(255, 200, 100)),
                );
                ui.colored_label(
                    crate::theme::rgb(255, 200, 100),
                    tr("A flat face of a body, a flat wall of a mesh, or a point in a sketch"),
                );
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(crate::theme::rgb(242, 89, 242), tr("On"));
                ui.label(target_name.clone().unwrap_or_default());
            });
            let id = egui::Id::new("text_field");
            let r = ui.add(
                egui::TextEdit::multiline(&mut u.text)
                    .id(id)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            probe_w.borrow_mut().push(("text_field", r.rect));
            if std::mem::take(&mut u.focus_text) {
                ui.memory_mut(|m| m.request_focus(id));
            }
            ui.horizontal(|ui| {
                ui.label(tr("Font"));
                let cur = rolidium_text::family(&u.family).map(|f| f.name).unwrap_or("?");
                egui::ComboBox::from_id_salt("text_font")
                    .selected_text(cur)
                    .show_ui(ui, |ui| {
                        for f in rolidium_text::FAMILIES.iter() {
                            ui.selectable_value(&mut u.family, f.id.to_string(), format!("{} ({})", f.name, tr(f.note)));
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut u.bold, tr("Bold"));
                ui.checkbox(&mut u.italic, tr("Italic"));
            });
            ui.horizontal(|ui| {
                ui.label(tr("Height of capitals"));
                let r = ui.add(egui::DragValue::new(&mut u.height).speed(0.1).range(0.2..=500.0).suffix(" mm"));
                probe_w.borrow_mut().push(("text_height", r.rect));
            });
            ui.horizontal(|ui| {
                ui.label(tr("Alignment"));
                for (a, l) in [(TextAlign::Left, tr("Left")), (TextAlign::Centre, tr("Centre")), (TextAlign::Right, tr("Right"))] {
                    ui.selectable_value(&mut u.align, a, l);
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("Turn"));
                ui.add(egui::DragValue::new(&mut u.rotation).speed(1.0).suffix("°"));
                ui.label(tr("Letter spacing"));
                ui.add(egui::DragValue::new(&mut u.spacing).speed(0.05).suffix(" mm"));
            });
            ui.separator();
            ui.horizontal(|ui| {
                let r1 = ui.selectable_label(!u.engrave, tr("Raised"));
                let r2 = ui.selectable_label(u.engrave, tr("Engraved"));
                probe_w.borrow_mut().push(("text_raised", r1.rect));
                probe_w.borrow_mut().push(("text_engraved", r2.rect));
                if r1.clicked() {
                    u.engrave = false;
                }
                if r2.clicked() {
                    u.engrave = true;
                }
                ui.label(tr("by"));
                let r = ui.add(egui::DragValue::new(&mut u.depth).speed(0.05).range(0.05..=200.0).suffix(" mm"));
                probe_w.borrow_mut().push(("text_depth", r.rect));
            });
            if !u.missing.is_empty() {
                ui.colored_label(
                    crate::theme::rgb(230, 170, 90),
                    trf("The font has no letter for: {}", &[u.missing.iter().collect::<String>()]),
                );
            }
            if let Some(e) = &u.error {
                ui.colored_label(crate::theme::rgb(255, 120, 120), tr(e));
            }
            ui.weak(tr("Click another face to move the text there"));
            ui.horizontal(|ui| {
                let r = ui.button(tr("Apply"));
                probe_w.borrow_mut().push(("text_apply", r.rect));
                if r.clicked() {
                    apply = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
            });
            if editing {
                ui.horizontal(|ui| {
                    if ui.button(tr("Remove the text")).clicked() {
                        remove = true;
                    }
                    if ui.button(tr("Keep as it is (unlink)")).clicked() {
                        unlink = true;
                    }
                });
            }
        });
        self.docks = docks;
        for (n, r) in probe_w.into_inner() {
            self.probe_widget(n, r);
        }
        let editing_id = u.editing;
        self.text = u;
        if cancel {
            self.text_close();
            self.leave_sketch();
        } else if remove {
            if let Some(link) = editing_id {
                self.submit(Command::DeleteLink { link });
                self.text_close();
            }
        } else if unlink {
            if let Some(link) = editing_id {
                self.submit(Command::Unlink { link });
                self.text_close();
            }
        } else if apply {
            self.text_apply();
        }
    }
}
