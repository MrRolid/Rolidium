// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Triangle meshes (STL) in the window: the import (button, drag and drop, several files at once) with the dialog of the units and the check, the
//! recognition of walls and holes in the background (with progress and Cancel), and the translation of the commands of a face (pull, diameter of a
//! hole) into the commands of a mesh. A mesh is shown, picked and selected like a body: its walls and holes are the "faces" of its entry.

use super::*;
use crate::filedialog::Purpose;
use crate::worker::StlLoaded;
use rolidium_mesh::{check, regions, transform, TriMesh};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;

/// The walls and holes of a mesh are looked for in the background.
pub struct RegionJob {
    pub mesh: Arc<TriMesh>,
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Option<regions::Regions>>,
    started: Instant,
}

impl RegionJob {
    /// How far the search is, 0 to 1.
    pub fn fraction(&self) -> f32 {
        self.progress.load(Ordering::Relaxed) as f32 / 1000.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnitChoice {
    Mm,
    Inch,
    Custom,
}

/// A mesh waiting in the import dialog, as it is in the file.
pub struct ImportItem {
    pub name: String,
    pub mesh: TriMesh,
    pub report: check::Report,
    /// What the last repair did.
    pub note: String,
}

type Repaired = (usize, TriMesh, check::Report, String);

pub struct ImportDialog {
    pub items: Vec<ImportItem>,
    pub units: UnitChoice,
    pub custom: String,
    /// The files that could not be read, with the reason.
    pub failed: Vec<String>,
    /// A repair runs on another thread.
    repairing: Option<mpsc::Receiver<Repaired>>,
}

impl ImportDialog {
    /// Millimetres per unit of the file.
    pub fn factor(&self) -> Option<f64> {
        match self.units {
            UnitChoice::Mm => Some(1.0),
            UnitChoice::Inch => Some(25.4),
            UnitChoice::Custom => parse_num(&self.custom).filter(|v| *v > 0.0 && v.is_finite()),
        }
    }
}

#[derive(Default)]
pub struct MeshUi {
    pub job: Option<RegionJob>,
    /// Meshes whose regions the person stopped looking for (the meshes are kept so that the address stays theirs).
    skip: Vec<Arc<TriMesh>>,
    pub dialog: Option<ImportDialog>,
    /// The regions were found while the program was busy: the display is made again as soon as it is not.
    rebuild: bool,
    /// How many files the person dropped into the window (for the scripted tests).
    pub dropped: u64,
    /// Files dropped while the program was busy: they are read as soon as it is not.
    queued: Vec<PathBuf>,
    /// The scale factors typed for the selected mesh, and whether they are one factor.
    pub scale: [String; 3],
    pub uniform: bool,
    /// The check of a mesh (the mesh it was made for, kept so that the address stays its own) and the check that is running.
    pub report: Option<(Arc<TriMesh>, check::Report)>,
    checking: Option<(Arc<TriMesh>, mpsc::Receiver<check::Report>)>,
}

/// What the check of a mesh says in one line.
pub fn summary_line(r: &check::Report) -> String {
    if r.is_clean() {
        tr("closed and consistent")
    } else {
        let mut v = Vec::new();
        if r.open_edges > 0 {
            v.push(trf("{} open edges", &[r.open_edges.to_string()]));
        }
        if r.inconsistent_edges > 0 {
            v.push(trf("{} flipped edges", &[r.inconsistent_edges.to_string()]));
        }
        if r.non_manifold_edges > 0 {
            v.push(trf("{} non-manifold edges", &[r.non_manifold_edges.to_string()]));
        }
        if r.degenerate > 0 {
            v.push(trf("{} degenerate triangles", &[r.degenerate.to_string()]));
        }
        v.join(", ")
    }
}

fn is_stl(p: &std::path::Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("stl"))
}

impl App {
    // ---- the list and the panel of a mesh ----

    /// The rows of the meshes in the list of bodies.
    pub(super) fn mesh_tree_rows(
        &mut self,
        ui: &mut egui::Ui,
        marked: &std::collections::HashSet<BodyId>,
        scroll_to: Option<BodyId>,
        tree_cmd: &mut Option<(Command, &'static str)>,
        clicked: &mut Option<BodyId>,
    ) {
        let meshes: Vec<rolidium_doc::MeshBody> = self.doc.meshes().to_vec();
        for m in &meshes {
            ui.horizontal(|ui| {
                let eye = icons::eye_button(ui, m.visible);
                self.probe_widget(&format!("mesh_eye_{}", m.id), eye.rect);
                if eye.clicked() {
                    *tree_cmd = Some((
                        Command::SetVisible {
                            target: ItemId::Body(m.id),
                            visible: !m.visible,
                        },
                        if m.visible { "Hidden" } else { "Shown" },
                    ));
                }
                let n = m.mesh.triangles.len().to_string();
                let t = if m.visible {
                    trf("▦ {}  ({} triangles)", &[m.name.clone(), n])
                } else {
                    format!("▦ {}  ({})", m.name, tr("hidden"))
                };
                let r = ui.selectable_label(marked.contains(&m.id), t);
                let r = r.interact(egui::Sense::click_and_drag());
                self.probe_widget(&format!("mesh_row_{}", m.id), r.rect);
                if r.drag_started() {
                    r.dnd_set_drag_payload(linktool::Dnd::Body(m.id));
                }
                let r = r.on_hover_text(tr("A mesh (STL): a skin of triangles, not a solid"));
                if scroll_to == Some(m.id) {
                    r.scroll_to_me(Some(egui::Align::Center));
                }
                if r.clicked() {
                    *clicked = Some(m.id);
                }
                if r.double_clicked() {
                    self.rename = Some((ItemId::Body(m.id), m.name.clone()));
                }
                r.context_menu(|ui| {
                    if ui.button(tr("Delete")).clicked() {
                        *tree_cmd = Some((
                            Command::DeleteBody { body: m.id },
                            "Deleted (Ctrl+Z brings it back)",
                        ));
                        ui.close();
                    }
                    if ui.button(tr(if m.visible { "Hide" } else { "Show" })).clicked() {
                        *tree_cmd = Some((
                            Command::SetVisible {
                                target: ItemId::Body(m.id),
                                visible: !m.visible,
                            },
                            if m.visible { "Hidden" } else { "Shown" },
                        ));
                        ui.close();
                    }
                    if ui.button(tr("Rename")).clicked() {
                        self.rename = Some((ItemId::Body(m.id), m.name.clone()));
                        ui.close();
                    }
                });
            });
        }
    }

    /// The panel of the selected mesh: what it is, the check and its repairs, the scale.
    pub(super) fn mesh_selection_ui(&mut self, ui: &mut egui::Ui, id: BodyId) {
        let Some(mb) = self.doc.mesh(id).cloned() else {
            return;
        };
        // the check, made in the background when it is asked for
        if let Some((m, rx)) = &self.mesh_ui.checking {
            match rx.try_recv() {
                Ok(r) => {
                    let m = m.clone();
                    self.mesh_ui.report = Some((m, r));
                    self.mesh_ui.checking = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.mesh_ui.checking = None,
                Err(mpsc::TryRecvError::Empty) => ui
                    .ctx()
                    .request_repaint_after(std::time::Duration::from_millis(80)),
            }
        }
        ui.label(trf(
            "Mesh: {} (Delete removes it)",
            std::slice::from_ref(&mb.name),
        ));
        ui.weak(tr("F2 or a double click on the name renames it"));
        ui.label(trf("{} triangles", &[mb.mesh.triangles.len().to_string()]));
        if let Some((lo, hi)) = mb.mesh.bounding_box() {
            ui.label(trf(
                "Size {} × {} × {} mm",
                &[
                    fmt_len(hi[0] - lo[0]),
                    fmt_len(hi[1] - lo[1]),
                    fmt_len(hi[2] - lo[2]),
                ],
            ));
        }
        // the check
        let report = self
            .mesh_ui
            .report
            .as_ref()
            .filter(|(m, _)| Arc::ptr_eq(m, &mb.mesh))
            .map(|(_, r)| r.clone());
        match &report {
            Some(r) => {
                let line = summary_line(r);
                ui.colored_label(
                    if r.is_clean() {
                        Color32::from_rgb(90, 180, 110)
                    } else {
                        Color32::from_rgb(230, 170, 60)
                    },
                    line,
                );
            }
            None => {
                let busy = self.mesh_ui.checking.is_some();
                let r = ui.add_enabled(!busy, egui::Button::new(tr("Check the mesh")));
                self.probe_widget("mesh_check", r.rect);
                if r.clicked() {
                    let (tx, rx) = mpsc::channel();
                    let m = mb.mesh.clone();
                    let (m2, ctx) = (mb.mesh.clone(), ui.ctx().clone());
                    std::thread::spawn(move || {
                        let _ = tx.send(check::check(&m2));
                        ctx.request_repaint();
                    });
                    self.mesh_ui.checking = Some((m, rx));
                }
                if busy {
                    ui.spinner();
                }
            }
        }
        let clean = report.as_ref().is_some_and(|r| r.is_clean());
        ui.add_enabled_ui(self.idle() && !clean, |ui| {
            ui.horizontal_wrapped(|ui| {
                let r = ui.button(tr("Merge vertices"));
                self.probe_widget("mesh_merge", r.rect);
                if r.clicked() {
                    self.submit(Command::MeshRepair {
                        body: id,
                        kind: rolidium_doc::RepairKind {
                            merge: true,
                            orient: false,
                        },
                    });
                }
                let r = ui.button(tr("Turn triangles outwards"));
                self.probe_widget("mesh_orient", r.rect);
                if r.clicked() {
                    self.submit(Command::MeshRepair {
                        body: id,
                        kind: rolidium_doc::RepairKind {
                            merge: false,
                            orient: true,
                        },
                    });
                }
            });
        });
        // the scale
        ui.add_space(4.0);
        ui.label(tr("Scale (factors)"));
        let mut apply = false;
        ui.horizontal(|ui| {
            for (k, name) in ["x", "y", "z"].iter().enumerate() {
                if self.mesh_ui.scale[k].is_empty() {
                    self.mesh_ui.scale[k] = "1".into();
                }
                ui.label(*name);
                let before = self.mesh_ui.scale[k].clone();
                let r = ui.add(egui::TextEdit::singleline(&mut self.mesh_ui.scale[k]).desired_width(46.0));
                self.probe_widget(&format!("mesh_scale_{name}"), r.rect);
                if self.mesh_ui.scale[k] != before && self.mesh_ui.uniform {
                    let v = self.mesh_ui.scale[k].clone();
                    self.mesh_ui.scale = [v.clone(), v.clone(), v];
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    apply = true;
                }
            }
        });
        ui.horizontal(|ui| {
            let r = ui.checkbox(&mut self.mesh_ui.uniform, tr("the same in all directions"));
            self.probe_widget("mesh_scale_uniform", r.rect);
            let r = ui.add_enabled(self.idle(), egui::Button::new(tr("Scale")));
            self.probe_widget("mesh_scale_apply", r.rect);
            if r.clicked() {
                apply = true;
            }
        });
        if apply {
            let f: Vec<Option<f64>> = self.mesh_ui.scale.iter().map(|t| parse_num(t)).collect();
            if let [Some(x), Some(y), Some(z)] = f[..] {
                if let Some((lo, hi)) = mb.mesh.bounding_box() {
                    let c = DVec3::new(
                        (lo[0] + hi[0]) / 2.0,
                        (lo[1] + hi[1]) / 2.0,
                        (lo[2] + hi[2]) / 2.0,
                    );
                    self.submit(Command::MeshScale {
                        body: id,
                        centre: c,
                        factors: DVec3::new(x, y, z),
                    });
                }
            } else {
                self.set_status("The scale factors must be numbers", true);
            }
        }
        ui.weak(tr(
            "M moves and turns the mesh, B mirrors it, T makes copies of it",
        ));
    }

    // ---- reading and the dialog ----

    pub(super) fn import_stl_dialog(&mut self) {
        self.file_dialog = Some(self.new_file_dialog(Purpose::ImportStl, ""));
    }

    /// Reads STL files in the background; the dialog of the units follows.
    pub(super) fn start_stl_read(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        if !self.idle() {
            self.mesh_ui.queued.extend(paths);
            self.set_status("The STL files are read when the running operation is done", false);
            return;
        }
        if self.worker.send(Work::ReadStl { paths }) {
            self.busy = Busy::File;
            self.set_status("Reading the STL file…", false);
        }
    }

    /// Files dropped into the window: the STL files are imported, the rest is said to be skipped. A scripted test names its files in
    /// the file `ROLIDIUM_DROP` (the same way: the real drop of the window system cannot be made by a script).
    pub(super) fn handle_drops(&mut self, ctx: &egui::Context) {
        let mut paths: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if self.probe.is_some() {
            if std::env::var_os("ROLIDIUM_DROP_POLL").is_some() {
                // a test that counts the frames the program draws by itself (the probe does not wake it) names the files this way too: look now and then
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
            if let Some(f) = std::env::var_os("ROLIDIUM_DROP") {
                if let Ok(text) = std::fs::read_to_string(&f) {
                    let _ = std::fs::remove_file(&f);
                    paths.extend(
                        text.lines()
                            .map(str::trim)
                            .filter(|l| !l.is_empty())
                            .map(PathBuf::from),
                    );
                }
            }
        }
        if paths.is_empty() {
            return;
        }
        self.mesh_ui.dropped += paths.len() as u64;
        let (stl, other): (Vec<PathBuf>, Vec<PathBuf>) = paths.into_iter().partition(|p| is_stl(p));
        if !other.is_empty() && stl.is_empty() {
            self.set_status("Only STL files can be dropped into the window", true);
            return;
        }
        self.start_stl_read(stl);
    }

    /// The files were read: the dialog shows them.
    pub(super) fn stl_read(&mut self, loaded: Vec<Result<StlLoaded, String>>) {
        let mut items = Vec::new();
        let mut failed = Vec::new();
        for l in loaded {
            match l {
                Ok(s) => items.push(ImportItem {
                    name: s.name,
                    mesh: s.mesh,
                    report: s.report,
                    note: String::new(),
                }),
                Err(e) => failed.push(e),
            }
        }
        if items.is_empty() {
            self.set_status(failed.first().cloned().unwrap_or_default(), true);
            return;
        }
        self.set_status("", false);
        self.mesh_ui.dialog = Some(ImportDialog {
            items,
            units: UnitChoice::Mm,
            custom: "1".into(),
            failed,
            repairing: None,
        });
    }

    fn import_done(&mut self, d: ImportDialog) {
        let Some(f) = d.factor() else { return };
        let meshes: Vec<rolidium_doc::NewMesh> = d
            .items
            .into_iter()
            .map(|mut i| {
                if (f - 1.0).abs() > 1e-12 {
                    transform::scale(&mut i.mesh, [0.0; 3], [f, f, f]);
                }
                rolidium_doc::NewMesh {
                    name: i.name,
                    verified: i.report.is_closed() && i.report.inconsistent_edges == 0,
                    mesh: Arc::new(i.mesh),
                }
            })
            .collect();
        let n = meshes.len();
        self.pending = None;
        self.submit(Command::AddMeshes { meshes });
        self.fitted = false;
        self.set_status(trf("Imported {} mesh(es) from STL", &[n.to_string()]), false);
    }

    /// The dialog after a read: the size, the units, the check and the repairs.
    pub(super) fn import_dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.mesh_ui.dialog.take() else {
            return;
        };
        // a repair that was running is done
        if let Some(rx) = &d.repairing {
            match rx.try_recv() {
                Ok((i, mesh, report, note)) => {
                    if let Some(it) = d.items.get_mut(i) {
                        it.mesh = mesh;
                        it.report = report;
                        it.note = note;
                    }
                    d.repairing = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => d.repairing = None,
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50));
                }
            }
        }
        let mut action: Option<bool> = None; // Some(true) import, Some(false) cancel
        let mut repair: Option<(usize, bool, bool)> = None;
        let factor = d.factor();
        let working = d.repairing.is_some();
        let win = egui::Window::new(tr("Import STL"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(430.0);
                for e in &d.failed {
                    ui.colored_label(Color32::from_rgb(220, 90, 70), e);
                }
                for (k, it) in d.items.iter().enumerate() {
                    ui.separator();
                    ui.label(egui::RichText::new(&it.name).strong());
                    let size = it.mesh.bounding_box().map(|(lo, hi)| {
                        let f = factor.unwrap_or(1.0);
                        [(hi[0] - lo[0]) * f, (hi[1] - lo[1]) * f, (hi[2] - lo[2]) * f]
                    });
                    ui.label(trf("{} triangles", &[it.mesh.triangles.len().to_string()]));
                    if let Some(s) = size {
                        ui.label(trf(
                            "Size {} × {} × {} mm",
                            &[fmt_len(s[0]), fmt_len(s[1]), fmt_len(s[2])],
                        ));
                    }
                    let clean = it.report.is_clean();
                    let line = summary_line(&it.report);
                    if clean {
                        ui.colored_label(Color32::from_rgb(90, 180, 110), line);
                    } else {
                        ui.colored_label(Color32::from_rgb(230, 170, 60), line);
                        ui.horizontal(|ui| {
                            let r = ui.add_enabled(!working, egui::Button::new(tr("Merge vertices")));
                            self.probe_widget(&format!("stl_merge_{k}"), r.rect);
                            if r.clicked() {
                                repair = Some((k, true, false));
                            }
                            let r =
                                ui.add_enabled(!working, egui::Button::new(tr("Turn triangles outwards")));
                            self.probe_widget(&format!("stl_orient_{k}"), r.rect);
                            if r.clicked() {
                                repair = Some((k, false, true));
                            }
                        });
                    }
                    if !it.note.is_empty() {
                        ui.weak(&it.note);
                    }
                }
                ui.separator();
                ui.label(tr("Units of the file"));
                ui.horizontal(|ui| {
                    for (u, label, id) in [
                        (UnitChoice::Mm, tr("millimetres"), "stl_units_mm"),
                        (UnitChoice::Inch, tr("inches"), "stl_units_inch"),
                        (UnitChoice::Custom, tr("custom scale"), "stl_units_custom"),
                    ] {
                        let r = ui.radio_value(&mut d.units, u, label);
                        self.probe_widget(id, r.rect);
                    }
                });
                if d.units == UnitChoice::Custom {
                    ui.horizontal(|ui| {
                        ui.label(tr("1 unit of the file is"));
                        let r = ui.add(egui::TextEdit::singleline(&mut d.custom).desired_width(70.0));
                        self.probe_widget("stl_custom_field", r.rect);
                        ui.label("mm");
                    });
                    if factor.is_none() {
                        ui.colored_label(
                            Color32::from_rgb(220, 90, 70),
                            tr("The scale must be a positive number"),
                        );
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let ok = ui.add_enabled(factor.is_some() && !working, egui::Button::new(tr("Import")));
                    self.probe_widget("stl_import_ok", ok.rect);
                    if ok.clicked() {
                        action = Some(true);
                    }
                    let no = ui.button(tr("Cancel"));
                    self.probe_widget("stl_import_cancel", no.rect);
                    if no.clicked() {
                        action = Some(false);
                    }
                    if working {
                        ui.spinner();
                    }
                });
            });
        if let Some(w) = win {
            self.probe_widget("stl_dialog", w.response.rect);
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            action = Some(false);
        }
        if let Some((k, merge, orient)) = repair {
            if let Some(it) = d.items.get(k) {
                let mut mesh = it.mesh.clone();
                let (tx, rx) = mpsc::channel();
                d.repairing = Some(rx);
                let ctx2 = ctx.clone();
                std::thread::spawn(move || {
                    let size = mesh
                        .bounding_box()
                        .map(|(lo, hi)| rolidium_mesh::len(rolidium_mesh::sub(hi, lo)))
                        .unwrap_or(0.0);
                    let merged = if merge {
                        check::merge_vertices(&mut mesh, (size * 1e-6).max(1e-6))
                    } else {
                        0
                    };
                    let turned = if orient {
                        check::orient_normals(&mut mesh)
                    } else {
                        0
                    };
                    let report = check::check(&mesh);
                    let note = format!("{merged} vertices merged, {turned} triangles turned");
                    let _ = tx.send((k, mesh, report, note));
                    ctx2.request_repaint();
                });
            }
        }
        match action {
            Some(true) => self.import_done(d),
            Some(false) => self.set_status("Import cancelled", false),
            None => self.mesh_ui.dialog = Some(d),
        }
    }

    // ---- walls and holes, in the background ----

    /// Once a frame: collects the result of the search for walls and holes, and starts one for a mesh that has none.
    pub(super) fn mesh_tick(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.mesh_ui.job {
            match job.rx.try_recv() {
                Ok(Some(r)) => {
                    let mesh = job.mesh.clone();
                    let secs = job.started.elapsed().as_secs_f32();
                    self.mesh_ui.job = None;
                    rolidium_doc::display::keep_regions(&mesh, Arc::new(r));
                    log(&format!("regions of a mesh found in {secs:.2} s"));
                    self.mesh_ui.rebuild = true;
                }
                Ok(None) => {
                    self.mesh_ui.skip.push(job.mesh.clone());
                    self.mesh_ui.job = None;
                    self.set_status(
                        "Stopped: the walls and holes of the mesh are not shown (the mesh can still be moved, turned and scaled)",
                        false,
                    );
                }
                Err(mpsc::TryRecvError::Disconnected) => self.mesh_ui.job = None,
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100))
                }
            }
        }
        if self.idle() && !self.mesh_ui.queued.is_empty() && self.mesh_ui.dialog.is_none() {
            let paths = std::mem::take(&mut self.mesh_ui.queued);
            self.start_stl_read(paths);
        }
        if self.mesh_ui.rebuild && self.idle() {
            self.mesh_ui.rebuild = false;
            self.request_meshes();
        }
        if self.mesh_ui.job.is_none() && self.idle() && !self.mesh_ui.rebuild {
            let todo = self
                .doc
                .meshes()
                .iter()
                .filter(|m| m.visible)
                .find(|m| {
                    rolidium_doc::display::regions_of(&m.mesh).is_none()
                        && !self.mesh_ui.skip.iter().any(|s| Arc::ptr_eq(s, &m.mesh))
                })
                .map(|m| m.mesh.clone());
            if let Some(mesh) = todo {
                let progress = Arc::new(AtomicU32::new(0));
                let cancel = Arc::new(AtomicBool::new(false));
                let (tx, rx) = mpsc::channel();
                let (m2, p2, c2, ctx2) = (mesh.clone(), progress.clone(), cancel.clone(), ctx.clone());
                // for the scripted tests: the search is made slower by this many ms at every report, so that its progress and Cancel can be tried reliably
                let slow = std::env::var("ROLIDIUM_TEST_REGIONS_DELAY_MS")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok());
                let spawned = std::thread::Builder::new()
                    .name("mesh-regions".into())
                    .spawn(move || {
                        let r = regions::find_with(&m2, 3.0, 4, &mut |f| {
                            if let Some(ms) = slow {
                                std::thread::sleep(std::time::Duration::from_millis(ms));
                            }
                            p2.store((f * 1000.0) as u32, Ordering::Relaxed);
                            !c2.load(Ordering::Relaxed)
                        });
                        let _ = tx.send(r);
                        ctx2.request_repaint();
                    });
                if spawned.is_ok() {
                    self.mesh_ui.job = Some(RegionJob {
                        mesh,
                        progress,
                        cancel,
                        rx,
                        started: Instant::now(),
                    });
                }
            }
        }
    }

    /// The little window with the progress of the search for walls and holes, and its Cancel button.
    #[allow(deprecated)]
    pub(super) fn mesh_overlay(&mut self, ctx: &egui::Context, rect: Rect) {
        let Some(job) = &self.mesh_ui.job else { return };
        if job.started.elapsed() < std::time::Duration::from_millis(250) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        let frac = job.fraction();
        let mut cancel = false;
        let mut at = None;
        egui::Area::new(egui::Id::new("mesh_regions_overlay"))
            .fixed_pos(Pos2::new(rect.center().x - 170.0, rect.min.y + 52.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(trf(
                            "Looking for walls and holes… {} %",
                            &[format!("{:.0}", frac * 100.0)],
                        ));
                        ui.add(egui::ProgressBar::new(frac).desired_width(90.0));
                        let r = ui.button(tr("Cancel"));
                        at = Some(r.rect);
                        if r.clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if let Some(r) = at {
            self.probe_widget("mesh_job_cancel", r);
        }
        if cancel {
            log("the search for walls and holes is stopped by the person");
            if let Some(j) = &self.mesh_ui.job {
                j.cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    // ---- commands of a face become commands of a mesh ----

    /// The wall or the hole of a mesh that a "face" of its entry is, as the search found it.
    fn mesh_region(&self, body: BodyId, face: u32) -> Result<MeshRegion, String> {
        let mb = self
            .doc
            .mesh(body)
            .ok_or_else(|| tr("The body no longer exists"))?;
        let Some(r) = rolidium_doc::display::regions_of(&mb.mesh) else {
            return Err(tr("The walls and holes of the mesh are still being looked for"));
        };
        let f = face as usize;
        if f >= 1 && f <= r.planes.len() {
            Ok(MeshRegion::Wall(Arc::new(r.planes[f - 1].clone())))
        } else if f > r.planes.len() && f - 1 - r.planes.len() < r.cylinders.len() {
            Ok(MeshRegion::Hole(Arc::new(
                r.cylinders[f - 1 - r.planes.len()].clone(),
            )))
        } else {
            Err(tr("This part of the mesh is not a flat wall or a round hole"))
        }
    }

    /// A command that names a face or a body of a mesh: what the mesh does for it (a pull of a wall, the diameter of a hole), or why it cannot.
    pub(super) fn mesh_translate(&self, command: Command) -> Result<Command, String> {
        let is_mesh = |b: &BodyId| self.doc.mesh(*b).is_some();
        match &command {
            Command::PullFace { body, face, distance, mode } if is_mesh(body) => {
                match self.mesh_region(*body, face.0)? {
                    MeshRegion::Wall(wall) => Ok(Command::MeshPull {
                        body: *body,
                        wall,
                        distance: mode.apply(*distance),
                    }),
                    MeshRegion::Hole(_) => Err(tr("Only a flat wall of a mesh can be pulled")),
                }
            }
            Command::SetCylinderDiameter { body, face, diameter } if is_mesh(body) => {
                match self.mesh_region(*body, face.0)? {
                    MeshRegion::Hole(hole) => Ok(Command::MeshHole {
                        body: *body,
                        hole,
                        diameter: *diameter,
                    }),
                    MeshRegion::Wall(_) => Err(tr("This wall is not a round hole or pin")),
                }
            }
            Command::FilletEdges { body, .. }
            | Command::ChamferAngled { body, .. }
            | Command::EditBlend { body, .. }
            | Command::EditChamfer { body, .. }
            | Command::SetCapDiameter { body, .. }
            | Command::MoveFeature { body, .. }
            | Command::AddFeatureLink { body, .. }
            | Command::RevolveFace { body, .. }
            | Command::SetMaterial { body, .. }
                if is_mesh(body) =>
            {
                Err(tr("A mesh cannot be rounded or changed like a solid yet. What works on a mesh: pull a flat wall, the diameter of a hole, move, turn, mirror, scale and copies"))
            }
            Command::Combine { target, tools, .. } if is_mesh(target) || tools.iter().any(is_mesh) => {
                Err(tr("A mesh cannot be joined with a solid or cut by one yet (a mesh is not a solid)"))
            }
            Command::AddLink { sources, kind, .. }
                if sources.iter().any(is_mesh)
                    && !(sources.iter().all(is_mesh)
                        && matches!(
                            kind,
                            rolidium_doc::LinkKind::Linear(..)
                                | rolidium_doc::LinkKind::Circular(..)
                                | rolidium_doc::LinkKind::Mirror(_)
                        )) =>
            {
                Err(tr("A mesh and a solid cannot be patterned together; select only meshes or only solids"))
            }
            _ => Ok(command),
        }
    }

    /// True if the round "face" of a mesh is a pin (its wall faces outwards), not a hole.
    pub(super) fn mesh_hole_is_pin(&self, body: BodyId, face: u32) -> bool {
        match self.mesh_region(body, face) {
            Ok(MeshRegion::Hole(c)) => !c.is_hole,
            _ => false,
        }
    }

    /// The "face" 0 of a mesh is what no wall or hole covers.
    pub(super) fn is_mesh_rest(&self, body: BodyId, face: u32) -> bool {
        face == 0 && self.doc.mesh(body).is_some()
    }
}

pub enum MeshRegion {
    Wall(Arc<regions::Plane>),
    Hole(Arc<regions::Cylinder>),
}

fn fmt_len(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
