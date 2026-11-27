// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The Shell tool (O): a body made hollow. Click the body, click the faces that should disappear (Ctrl or Shift adds more; none makes a closed hollow body), set the
//! wall thickness and the side the walls grow to. The preview is calculated on a thread of its own and shown in the place of the body; Enter confirms. The shell is a
//! link: its panel is opened again with a click on it in the document list.

use super::*;
use crate::i18n::{tr, trf};
use rolidium_doc::{FaceAnchor, LinkId, LinkKind, ShellParams, ShellSide};
use std::sync::mpsc;

type Preview = Result<(Arc<rolidium_mesh::TriMesh>, Vec<String>), String>;

struct ShellJob {
    key: String,
    rx: mpsc::Receiver<Preview>,
}

#[derive(Default)]
pub struct ShellUi {
    pub body: Option<BodyId>,
    pub open: Vec<FaceAnchor>,
    pub thickness: f64,
    pub side: ShellSide,
    pub round: bool,
    pub heavier: Vec<(FaceAnchor, f64)>,
    /// Clicks choose walls to make thicker or thinner (not openings).
    pub thicken: bool,
    pub walls: Vec<FaceAnchor>,
    pub extra: f64,
    pub editing: Option<LinkId>,
    job: Option<ShellJob>,
    key: String,
    /// The reason the preview could not be made, and the remarks of the calculation.
    pub error: Option<String>,
    pub notes: Vec<String>,
    pub preview_on: bool,
}

/// The mesh of a solid for the preview.
pub(super) fn solid_preview_mesh(s: &rolidium_kernel::Solid) -> Result<Arc<rolidium_mesh::TriMesh>, String> {
    let m = s.display_mesh().map_err(|e| e.to_string())?;
    Ok(Arc::new(rolidium_mesh::TriMesh {
        positions: m.positions.iter().map(|p| [p.x, p.y, p.z]).collect(),
        triangles: m.indices.as_chunks::<3>().0.to_vec(),
    }))
}

impl App {
    pub(super) fn activate_shell(&mut self) {
        let body = self
            .link_sources()
            .first()
            .copied()
            .or_else(|| self.selection.last().map(|s| s.0));
        self.leave_sketch();
        self.tool = Tool::Shell;
        self.drop_mesh_previews();
        self.shell = ShellUi {
            thickness: 2.0,
            ..ShellUi::default()
        };
        self.clear_selection();
        self.layers_changed = true;
        match body {
            Some(b) => self.shell_choose_body(b),
            None => self.set_status("Shell: click the body to make hollow", false),
        }
    }

    /// The body to hollow is chosen: a mesh is refused (it is converted first), a body that is a shell already opens its link.
    fn shell_choose_body(&mut self, b: BodyId) {
        if self.doc.mesh(b).is_some() {
            self.set_status(
                "A mesh has no walls to hollow. Convert it to a solid first: select it and press Convert to solid in its panel",
                true,
            );
            return;
        }
        let Some(body) = self.doc.body(b) else { return };
        if let Some(l) = self
            .doc
            .state()
            .links
            .iter()
            .find(|l| matches!(l.kind, LinkKind::Shell(_)) && l.results.contains(&b))
            .cloned()
        {
            return self.open_shell_link(l.id);
        }
        let (lo, hi) = body.solid.bounding_box().unwrap_or_default();
        let small = (hi - lo).min_element().max(0.1);
        self.shell.body = Some(b);
        self.shell.thickness = ((small / 10.0).clamp(0.5, 3.0) * 2.0).round() / 2.0;
        self.shell.open.clear();
        self.shell.walls.clear();
        self.shell.heavier.clear();
        self.shell.key.clear();
        self.set_status(
            "Click the faces that should disappear (Ctrl or Shift adds more); no face makes a closed hollow body. Enter makes the shell",
            false,
        );
        self.layers_changed = true;
    }

    /// The panel of a shell that exists is opened (a click on it in the list of the document).
    pub(super) fn open_shell_link(&mut self, id: LinkId) {
        let Some(l) = self.doc.state().links.iter().find(|l| l.id == id).cloned() else {
            return;
        };
        let LinkKind::Shell(p) = &l.kind else { return };
        self.leave_sketch();
        self.tool = Tool::Shell;
        self.drop_mesh_previews();
        self.clear_selection();
        self.shell = ShellUi {
            body: Some(p.target),
            open: p.open.clone(),
            thickness: p.thickness,
            side: p.side,
            round: p.round_corners,
            heavier: p.heavier.clone(),
            editing: Some(id),
            ..ShellUi::default()
        };
        self.link_sel = Some(id);
        self.layers_changed = true;
        self.set_status(
            "Shell: change the numbers, click walls to open or thicken them, Enter confirms",
            false,
        );
    }

    fn shell_params(&self) -> Option<ShellParams> {
        let b = self.shell.body?;
        Some(ShellParams {
            target: b,
            open: self.shell.open.clone(),
            thickness: self.shell.thickness,
            side: self.shell.side,
            round_corners: self.shell.round,
            heavier: self.shell.heavier.clone(),
        })
    }

    /// The face of the body as it was before the shell that a click on a face of the body now means: the same face when the shell has not changed it (the
    /// outer wall of a shell that grew inwards), else the face offset by the thickness.
    fn shell_anchor_of_click(&self, b: BodyId, f: u32) -> Option<FaceAnchor> {
        let body = self.doc.body(b)?;
        let a = FaceAnchor::of(&body.solid, rolidium_kernel::FaceId(f)).ok()?;
        let Some(id) = self.shell.editing else {
            return Some(a);
        };
        let base = self.doc.state().links.iter().find(|l| l.id == id)?.base.clone()?;
        if let Some(face) = a.find(&base) {
            return FaceAnchor::of(&base, face).ok();
        }
        if a.kind == rolidium_kernel::FaceKind::Plane {
            // the other side of a wall: the plane of the original face is a thickness away
            for sign in [-1.0, 1.0] {
                let mut c = a.clone();
                c.normal = -a.normal;
                c.origin = a.origin + a.normal * (self.shell.thickness * sign);
                if let Some(face) = c.find(&base) {
                    return FaceAnchor::of(&base, face).ok();
                }
            }
        }
        None
    }

    /// The faces chosen are tinted in the view.
    fn shell_sync_selection(&mut self) {
        let Some(b) = self.shell.body else { return };
        let Some(body) = self.doc.body(b) else { return };
        let mut v: Vec<(BodyId, u32)> = Vec::new();
        for a in self.shell.open.iter().chain(self.shell.walls.iter()) {
            if let Some(f) = a.find(&body.solid) {
                v.push((b, f.0));
            }
        }
        if self.selection != v {
            self.selection = v;
            self.sync_sel_order();
            self.layers_changed = true;
        }
    }

    pub(super) fn shell_input(&mut self, resp: &egui::Response, hovered_face: Option<(BodyId, u32)>) {
        if !resp.clicked_by(PointerButton::Primary) {
            return;
        }
        let add = resp.ctx.input(|i| i.modifiers.command || i.modifiers.shift);
        let Some((b, f)) = hovered_face else { return };
        let Some(body) = self.shell.body else {
            return self.shell_choose_body(b);
        };
        if b != body {
            if !add {
                self.shell_choose_body(b);
            }
            return;
        }
        let Some(a) = self.shell_anchor_of_click(b, f) else {
            self.set_status(
                "That face does not come from a wall of the body the shell started from",
                true,
            );
            return;
        };
        let thicken = self.shell.thicken;
        let list = if thicken {
            &mut self.shell.walls
        } else {
            &mut self.shell.open
        };
        let was = list.iter().position(|x| *x == a);
        if add {
            match was {
                Some(i) => {
                    list.remove(i);
                }
                None => list.push(a),
            }
        } else {
            let only_it = was.is_some() && list.len() == 1;
            list.clear();
            if !only_it {
                list.push(a);
            }
        }
        self.shell.key.clear();
        self.shell_sync_selection();
    }

    /// Frame by frame: the preview is asked for when the numbers changed and shown when it arrives.
    fn shell_tick(&mut self) {
        let Some(body) = self.shell.body else {
            return;
        };
        let Some(p) = self.shell_params() else { return };
        let want = format!("{p:?}|{}|{:?}", self.doc.revision(), self.shell.editing);
        if let Some(j) = &self.shell.job {
            match j.rx.try_recv() {
                Ok(r) => {
                    let key = j.key.clone();
                    self.shell.job = None;
                    if key == want {
                        match r {
                            Ok((mesh, notes)) => {
                                self.shell.error = None;
                                self.shell.notes = notes;
                                self.shell.preview_on = true;
                                self.set_preview(Some(meshops::PreviewMesh {
                                    mesh,
                                    hides: Some(body),
                                }));
                            }
                            Err(e) => {
                                self.shell.error = Some(e);
                                self.shell.notes.clear();
                                self.shell.preview_on = false;
                                self.set_preview(None);
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    self.egui_ctx
                        .request_repaint_after(std::time::Duration::from_millis(100));
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.shell.job = None;
                }
            }
        }
        if self.shell.job.is_some() || self.shell.key == want {
            return;
        }
        self.shell.key = want.clone();
        let st = self.doc.state().clone();
        let editing = self.shell.editing;
        let (tx, rx) = mpsc::channel();
        let kind = LinkKind::Shell(p);
        let _ = std::thread::Builder::new().stack_size(128 << 20).spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                rolidium_doc::trial_link(&st, kind, editing)
                    .map_err(|e| e.to_string())
                    .and_then(|(solid, notes)| solid_preview_mesh(&solid).map(|m| (m, notes)))
            }))
            .unwrap_or_else(|_| Err("The shell could not be calculated".to_string()));
            let _ = tx.send(r);
        });
        self.shell.job = Some(ShellJob { key: want, rx });
        self.egui_ctx
            .request_repaint_after(std::time::Duration::from_millis(100));
    }

    pub(super) fn shell_close(&mut self) {
        self.shell = ShellUi::default();
        self.drop_mesh_previews();
        self.clear_selection();
        if self.tool == Tool::Shell {
            self.tool = Tool::Select;
        }
        self.layers_changed = true;
    }

    pub(super) fn shell_apply(&mut self) {
        let Some(p) = self.shell_params() else {
            self.set_status("Click the body to make hollow first", true);
            return;
        };
        if let Some(e) = &self.shell.error {
            let e = e.clone();
            self.set_status(e, true);
            return;
        }
        let cmd = match self.shell.editing {
            Some(link) => Command::EditLink {
                link,
                kind: Some(LinkKind::Shell(p)),
                name: None,
            },
            None => Command::AddLink {
                kind: LinkKind::Shell(p),
                sources: Vec::new(),
                name: String::new(),
            },
        };
        self.submit(cmd);
        self.shell_close();
    }

    #[allow(deprecated)]
    pub(super) fn shell_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        if self.tool != Tool::Shell {
            return;
        }
        self.shell_sync_selection();
        self.shell_tick();
        let mut docks = std::mem::take(&mut self.docks);
        let mut ui_state = std::mem::take(&mut self.shell);
        let name = ui_state
            .body
            .and_then(|b| self.doc.body(b).map(|x| x.name.clone()))
            .unwrap_or_default();
        let mut apply = false;
        let mut cancel = false;
        let mut remove = false;
        let mut unlink = false;
        let busy = ui_state.job.is_some();
        let editing = ui_state.editing.is_some();
        let probe_w: std::cell::RefCell<Vec<(&'static str, Rect)>> = std::cell::RefCell::new(Vec::new());
        let title = if editing {
            tr("Shell (a link)")
        } else {
            tr("Shell")
        };
        docks.panel(ctx, "shell", Pos2::new(rect.max.x - 290.0, rect.min.y + 10.0), None, 270.0, &title, false, |ui| {
            if ui_state.body.is_none() {
                ui.label(
                    egui::RichText::new(tr("Click the body to make hollow"))
                        .size(16.0)
                        .color(crate::theme::rgb(255, 200, 100)),
                );
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(crate::theme::rgb(242, 89, 242), tr("Body"));
                ui.label(&name);
            });
            ui.horizontal(|ui| {
                ui.label(tr("Wall thickness"));
                let r = ui.add(
                    egui::DragValue::new(&mut ui_state.thickness)
                        .speed(0.1)
                        .range(0.05..=500.0)
                        .suffix(" mm"),
                );
                probe_w.borrow_mut().push(("shell_thickness", r.rect));
            });
            ui.horizontal(|ui| {
                ui.label(tr("The walls grow"));
                for (side, label, id) in [
                    (ShellSide::Inwards, tr("Inwards"), "shell_in"),
                    (ShellSide::Outwards, tr("Outwards"), "shell_out"),
                    (ShellSide::Centre, tr("Both ways"), "shell_centre"),
                ] {
                    let r = ui.selectable_label(ui_state.side == side, label);
                    probe_w.borrow_mut().push((id, r.rect));
                    if r.clicked() {
                        ui_state.side = side;
                    }
                }
            });
            ui.checkbox(&mut ui_state.round, tr("Round the corners that stick out"));
            ui.separator();
            if !ui_state.thicken {
                ui.colored_label(
                    crate::theme::rgb(255, 200, 100),
                    if ui_state.open.is_empty() {
                        tr("No opening: a closed hollow body. Click a face to open it (Ctrl or Shift adds more)")
                    } else {
                        trf("{} face(s) open", &[ui_state.open.len().to_string()])
                    },
                );
                let mut gone = None;
                for (i, a) in ui_state.open.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(face_name(a));
                        if ui.small_button(tr("Close")).clicked() {
                            gone = Some(i);
                        }
                    });
                }
                if let Some(i) = gone {
                    ui_state.open.remove(i);
                }
            }
            ui.separator();
            let r = ui.selectable_label(ui_state.thicken, tr("Thicken walls"));
            probe_w.borrow_mut().push(("shell_thicken", r.rect));
            if r.clicked() {
                ui_state.thicken = !ui_state.thicken;
            }
            if ui_state.thicken {
                ui.colored_label(
                    crate::theme::rgb(120, 220, 255),
                    trf("Click the walls to change ({} chosen)", &[ui_state.walls.len().to_string()]),
                );
                ui.horizontal(|ui| {
                    ui.label(tr("Thicker (negative: thinner) by"));
                    ui.add(egui::DragValue::new(&mut ui_state.extra).speed(0.1).suffix(" mm"));
                });
                if ui
                    .add_enabled(!ui_state.walls.is_empty() && ui_state.extra.abs() > 1e-9, egui::Button::new(tr("Apply to the chosen walls")))
                    .clicked()
                {
                    for w in std::mem::take(&mut ui_state.walls) {
                        ui_state.heavier.retain(|(a, _)| *a != w);
                        ui_state.heavier.push((w, ui_state.extra));
                    }
                }
            }
            let mut gone = None;
            for (i, (a, e)) in ui_state.heavier.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}: {:+} mm", face_name(a), (e * 100.0).round() / 100.0));
                    if ui.small_button(tr("Remove")).clicked() {
                        gone = Some(i);
                    }
                });
            }
            if let Some(i) = gone {
                ui_state.heavier.remove(i);
            }
            ui.separator();
            if busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr("Calculating the preview…"));
                });
            }
            if let Some(e) = &ui_state.error {
                ui.colored_label(crate::theme::rgb(255, 120, 120), e.as_str());
                ui.weak(tr(
                    "A wall may be thicker than the body allows, or a small feature may be thinner than the wall: make the wall thinner, or make the feature smaller",
                ));
            }
            for n in &ui_state.notes {
                ui.colored_label(crate::theme::rgb(200, 200, 130), tr(n));
            }
            ui.horizontal(|ui| {
                let r = ui.button(tr("Apply"));
                probe_w.borrow_mut().push(("shell_apply", r.rect));
                if r.clicked() {
                    apply = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
            });
            if editing {
                ui.horizontal(|ui| {
                    if ui.button(tr("Remove the shell")).clicked() {
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
        let editing_id = ui_state.editing;
        self.shell = ui_state;
        if cancel {
            self.shell_close();
        } else if remove {
            if let Some(link) = editing_id {
                self.submit(Command::DeleteLink { link });
                self.shell_close();
            }
        } else if unlink {
            if let Some(link) = editing_id {
                self.submit(Command::Unlink { link });
                self.shell_close();
            }
        } else if apply {
            self.shell_apply();
        }
    }
}

/// A face as the person knows it: "flat face facing +Z", "round face, diameter 8 mm".
pub(super) fn face_name(a: &FaceAnchor) -> String {
    use rolidium_kernel::FaceKind;
    match a.kind {
        FaceKind::Plane => {
            let n = a.normal.normalize_or_zero();
            let axis = [
                ("+X", DVec3::X),
                ("-X", -DVec3::X),
                ("+Y", DVec3::Y),
                ("-Y", -DVec3::Y),
                ("+Z", DVec3::Z),
                ("-Z", -DVec3::Z),
            ]
            .iter()
            .find(|(_, v)| n.dot(*v) > 0.999)
            .map(|(l, _)| l.to_string());
            match axis {
                Some(l) => trf("Flat face facing {}", &[l]),
                None => tr("Flat face (slanted)"),
            }
        }
        FaceKind::Cylinder => trf("Round face, diameter {} mm", &[fmt_num(a.radius * 2.0)]),
        _ => tr("Curved face"),
    }
}
