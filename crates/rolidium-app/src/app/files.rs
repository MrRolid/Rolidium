// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! New / Open / Save / Save As, unsaved-changes questions, recent files, autosave and recovery,
//! STEP import and export.

use super::*;
use crate::filedialog::{DialogResult, FileDialog, Purpose};
use crate::worker::GpuHandle;
use rolidium_doc::State;

/// What to do once unsaved changes have been dealt with.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    New,
    OpenDialog,
    Open(PathBuf),
    Quit,
}

/// An autosave left behind by a program that did not close properly.
pub struct Recovery {
    pub original: Option<PathBuf>,
    pub age: String,
}

fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("ROLIDIUM_AUTOSAVE_DIR") {
        return PathBuf::from(d);
    }
    let base = crate::dirs::data_base().unwrap_or_else(std::env::temp_dir);
    base.join("rolidium")
}

pub fn autosave_file() -> PathBuf {
    data_dir().join("autosave.rolidium")
}
fn autosave_meta() -> PathBuf {
    data_dir().join("autosave.json")
}

pub fn remove_autosave() {
    let _ = std::fs::remove_file(autosave_file());
    let _ = std::fs::remove_file(autosave_meta());
}

/// An autosave from an earlier run, if there is one.
pub fn find_recovery() -> Option<Recovery> {
    let f = autosave_file();
    let md = std::fs::metadata(&f).ok()?;
    let secs = md
        .modified()
        .ok()
        .and_then(|m| m.elapsed().ok())
        .map_or(0, |d| d.as_secs());
    let age = if secs < 90 {
        trf("{} seconds ago", &[secs.to_string()])
    } else if secs < 5400 {
        trf("{} minutes ago", &[(secs / 60).to_string()])
    } else {
        trf("{} hours ago", &[(secs / 3600).to_string()])
    };
    let original = std::fs::read_to_string(autosave_meta())
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["original"].as_str().map(PathBuf::from));
    Some(Recovery { original, age })
}

pub fn display_name(path: &Option<PathBuf>) -> String {
    path.as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| tr("Untitled"))
}

impl App {
    fn gpu_handle(&self) -> Option<GpuHandle> {
        Some(GpuHandle {
            device: self.render_state.device.clone(),
            queue: self.render_state.queue.clone(),
        })
    }

    /// Runs `p` now, or first asks what to do with unsaved changes.
    pub(super) fn request(&mut self, p: Pending) {
        if !self.idle() && p != Pending::Quit {
            self.set_status("Still working on the previous operation", true);
            return;
        }
        if self.doc.is_dirty() {
            self.confirm = Some(p);
        } else {
            self.perform(p);
        }
    }

    pub(super) fn perform(&mut self, p: Pending) {
        match p {
            Pending::New => {
                self.replace_document(Document::new(), None);
                self.request_meshes();
                self.set_status("New document", false);
            }
            Pending::OpenDialog => {
                self.file_dialog = Some(self.new_file_dialog(Purpose::Open, ""));
            }
            Pending::Open(path) => {
                if self.worker.send(Work::OpenFile { path: path.clone() }) {
                    self.busy = Busy::File;
                    self.set_status(trf("Opening {}…", &[path.display().to_string()]), false);
                }
            }
            Pending::Quit => {
                remove_autosave();
                self.allow_close = true;
                self.quit_now = true;
            }
        }
    }

    fn replace_document(&mut self, doc: Document, path: Option<PathBuf>) {
        self.doc = doc;
        self.file_path = path;
        self.clear_selection();
        self.clear_preview();
        self.clear_edge_preview();
        self.pull = None;
        self.sk = None;
        self.pending = None;
        self.up_to = None;
        self.hover_region = None;
        self.regions_rev = u64::MAX;
        self.layers_changed = true;
        self.fitted = false;
        self.autosave_state = None;
        self.set_meshes(Vec::new());
    }

    pub(super) fn opened(
        &mut self,
        path: PathBuf,
        loaded: rolidium_doc::file::Loaded,
        recovered: Option<Option<PathBuf>>,
    ) {
        let dropped = loaded.dropped_features;
        let doc = Document::from_loaded(loaded);
        let was_recovered = recovered.is_some();
        match recovered {
            None => {
                self.replace_document(doc, Some(path.clone()));
                self.settings.remember(&path.display().to_string());
                if let Some(d) = path.parent() {
                    self.settings.file_dir = d.display().to_string();
                }
                self.settings.save();
            }
            Some(original) => {
                self.replace_document(doc, original);
                self.doc.mark_dirty();
                remove_autosave();
            }
        }
        self.request_meshes();
        if dropped > 0 {
            self.set_status(
                trf(
                    "Opened {}, but {} round(s) or chamfer(s) could not be recognised and are plain faces now",
                    &[display_name(&self.file_path), dropped.to_string()],
                ),
                true,
            );
        } else if was_recovered {
            self.set_status("Unsaved work restored; save it to keep it", false);
        } else {
            self.set_status(trf("Opened {}", &[display_name(&self.file_path)]), false);
        }
    }

    pub(super) fn save_clicked(&mut self, as_new: bool) {
        match (&self.file_path, as_new) {
            (Some(p), false) => {
                let p = p.clone();
                self.save_to(p);
            }
            _ => {
                let name = self
                    .file_path
                    .as_ref()
                    .map(|_| display_name(&self.file_path))
                    .unwrap_or_default();
                self.file_dialog = Some(self.new_file_dialog(Purpose::SaveAs, &name));
            }
        }
    }

    pub(super) fn save_to(&mut self, path: PathBuf) {
        if !self.idle() {
            self.set_status("Still working on the previous operation", true);
            return;
        }
        let gpu = if self.selftest.is_some() {
            None
        } else {
            self.gpu_handle()
        };
        let w = Work::SaveFile {
            doc: Box::new(self.doc.clone()),
            path: path.clone(),
            gpu,
            meshes: self.meshes.clone(),
            autosave: false,
        };
        if self.worker.send(w) {
            self.busy = Busy::File;
            self.set_status(trf("Saving {}…", &[path.display().to_string()]), false);
        }
    }

    pub(super) fn saved(&mut self, path: PathBuf, state: Arc<State>, autosave: bool) {
        if autosave {
            self.autosave_inflight = false;
            self.autosave_state = Some(state);
            return;
        }
        self.doc.mark_saved_state(state);
        self.file_path = Some(path.clone());
        self.settings.remember(&path.display().to_string());
        if let Some(d) = path.parent() {
            self.settings.file_dir = d.display().to_string();
        }
        self.settings.save();
        remove_autosave();
        self.set_status(trf("Saved {}", &[display_name(&self.file_path)]), false);
        if let Some(p) = self.after_save.take() {
            self.perform(p);
        }
    }

    /// Autosave: only when something changed since the last one, never while another job runs.
    pub(super) fn autosave_tick(&mut self) {
        let minutes = self.settings.autosave_minutes;
        if minutes == 0 || self.selftest.is_some() || self.bench.is_some() || self.autosave_inflight {
            return;
        }
        if !self.idle() || !self.doc.is_dirty() {
            return;
        }
        let interval = std::env::var("ROLIDIUM_AUTOSAVE_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(minutes as u64 * 60);
        if self.last_autosave.elapsed().as_secs() < interval {
            return;
        }
        if self
            .autosave_state
            .as_ref()
            .is_some_and(|s| Arc::ptr_eq(s, self.doc.state()))
        {
            return;
        }
        self.last_autosave = Instant::now();
        let _ = std::fs::create_dir_all(data_dir());
        let meta =
            serde_json::json!({ "original": self.file_path.as_ref().map(|p| p.display().to_string()) });
        let _ = std::fs::write(autosave_meta(), meta.to_string());
        let w = Work::SaveFile {
            doc: Box::new(self.doc.clone()),
            path: autosave_file(),
            gpu: None,
            meshes: self.meshes.clone(),
            autosave: true,
        };
        if self.worker.send(w) {
            self.autosave_inflight = true;
        }
    }

    pub(super) fn import_step_dialog(&mut self) {
        self.file_dialog = Some(self.new_file_dialog(Purpose::ImportStep, ""));
    }

    pub(super) fn export_step_dialog(&mut self) {
        if self.visible_solids().is_empty() {
            self.set_status("Nothing to export", true);
            return;
        }
        self.file_dialog = Some(self.new_file_dialog(Purpose::ExportStep, "model.step"));
    }

    /// The imported solids arrived: add them as bodies.
    pub(super) fn step_read(&mut self, solids: Vec<rolidium_kernel::Solid>, skipped: Vec<String>) {
        let (n, k) = (solids.len(), skipped.len());
        for s in &skipped {
            log(&format!("STEP import skipped: {s}"));
        }
        if n == 0 {
            self.set_status(
                "The STEP file contains no valid solid, nothing was imported",
                true,
            );
            return;
        }
        self.pending = None;
        self.submit(Command::AddSolids { solids });
        self.fitted = false;
        if k > 0 {
            self.set_status(
                trf(
                    "Imported {} body(ies); {} could not be used as a valid solid and were skipped",
                    &[n.to_string(), k.to_string()],
                ),
                true,
            );
        } else {
            self.set_status(
                trf("Imported {} body(ies) from the STEP file", &[n.to_string()]),
                false,
            );
        }
    }

    /// A file dialog that starts where the last one of its kind ended (documents and exports are remembered apart).
    fn new_file_dialog(&self, purpose: Purpose, name: &str) -> FileDialog {
        let dir = if purpose.is_export() {
            &self.settings.export_dir
        } else {
            &self.settings.file_dir
        };
        FileDialog::new(
            &self.egui_ctx,
            purpose,
            dir,
            name,
            &self.settings.recent_dirs,
            (self.settings.dialog_w, self.settings.dialog_h),
        )
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        // the file dialog
        if let Some(d) = &mut self.file_dialog {
            let purpose = d.purpose;
            let dir = d.dir();
            let size = d.size();
            let result = d.ui(ctx);
            if !matches!(result, DialogResult::None) {
                self.settings.dialog_w = size.0;
                self.settings.dialog_h = size.1;
            }
            match result {
                DialogResult::None => {}
                DialogResult::Cancel => {
                    self.file_dialog = None;
                    self.settings.save();
                }
                DialogResult::Chosen(path) => {
                    self.file_dialog = None;
                    if purpose.is_export() {
                        self.settings.export_dir = dir.clone();
                    } else {
                        self.settings.file_dir = dir.clone();
                    }
                    self.settings.remember_dir(&dir);
                    self.settings.save();
                    match purpose {
                        Purpose::Open => self.perform(Pending::Open(path)),
                        Purpose::SaveAs => self.save_to(path),
                        Purpose::ImportStep => {
                            if self.worker.send(Work::ReadStep { path }) {
                                self.busy = Busy::File;
                                self.set_status("Reading the STEP file…", false);
                            }
                        }
                        Purpose::ExportStep => {
                            let solids = self.visible_solids();
                            if self.worker.send(Work::ExportStep { solids, path }) {
                                self.busy = Busy::File;
                                self.set_status("Writing the STEP file…", false);
                            }
                        }
                    }
                }
            }
        }
        self.confirm_window(ctx);
        self.recovery_window(ctx);
        // closing the window
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.doc.is_dirty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm = Some(Pending::Quit);
            } else {
                remove_autosave();
            }
        }
        if self.quit_now {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // window title
        let title = format!(
            "{}{} - Rolidium (Alpha)",
            display_name(&self.file_path),
            if self.doc.is_dirty() { " *" } else { "" }
        );
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    #[allow(deprecated)]
    fn confirm_window(&mut self, ctx: &egui::Context) {
        let Some(p) = self.confirm.clone() else { return };
        let mut choice: Option<u8> = None;
        egui::Window::new(tr("Unsaved changes"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(trf("Save the changes to {}?", &[display_name(&self.file_path)]));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button(tr("Save")).clicked() {
                        choice = Some(0);
                    }
                    if ui.button(tr("Discard")).clicked() {
                        choice = Some(1);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        choice = Some(2);
                    }
                });
            });
        match choice {
            Some(0) => {
                self.confirm = None;
                if self.file_path.is_some() {
                    self.after_save = Some(p);
                    self.save_clicked(false);
                } else {
                    // no name yet: ask for one, then continue
                    self.after_save = Some(p);
                    self.save_clicked(true);
                }
            }
            Some(1) => {
                self.confirm = None;
                self.perform(p);
            }
            Some(_) => {
                self.confirm = None;
                self.after_save = None;
            }
            None => {}
        }
    }

    #[allow(deprecated)]
    fn recovery_window(&mut self, ctx: &egui::Context) {
        let Some(r) = &self.recovery else { return };
        let (age, original) = (r.age.clone(), r.original.clone());
        let mut choice: Option<bool> = None;
        egui::Window::new(tr("Recover unsaved work"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(tr("Rolidium was not closed properly the last time."));
                ui.label(trf("There is an automatic copy of your work from {}.", &[age]));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button(tr("Restore")).clicked() {
                        choice = Some(true);
                    }
                    if ui.button(tr("Discard")).clicked() {
                        choice = Some(false);
                    }
                });
            });
        match choice {
            Some(true) => {
                self.recovery = None;
                self.recovering = Some(original);
                if self.worker.send(Work::OpenFile {
                    path: autosave_file(),
                }) {
                    self.busy = Busy::File;
                }
            }
            Some(false) => {
                self.recovery = None;
                remove_autosave();
            }
            None => {}
        }
    }
}
