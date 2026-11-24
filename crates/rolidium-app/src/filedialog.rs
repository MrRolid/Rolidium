// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Built-in file dialog (no desktop portal needed) for Open, Save As, Import STEP and Export STEP. The folder
//! browsing itself is `browser::Browser`; this adds the file name, the overwrite question and the buttons.

use crate::browser::{Browser, Event};
use crate::i18n::tr;
use eframe::egui;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose {
    Open,
    SaveAs,
    ImportStep,
    ImportStl,
    ExportStep,
}

impl Purpose {
    pub fn title(self) -> &'static str {
        match self {
            Purpose::Open => "Open",
            Purpose::SaveAs => "Save As",
            Purpose::ImportStep => "Import STEP",
            Purpose::ImportStl => "Import STL",
            Purpose::ExportStep => "Export STEP",
        }
    }
    pub fn saving(self) -> bool {
        matches!(self, Purpose::SaveAs | Purpose::ExportStep)
    }
    /// File name extensions that are listed; the first one is added when saving.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Purpose::Open | Purpose::SaveAs => &["rolidium"],
            Purpose::ImportStep | Purpose::ExportStep => &["step", "stp"],
            Purpose::ImportStl => &["stl"],
        }
    }
    pub fn type_label(self) -> &'static str {
        match self {
            Purpose::Open | Purpose::SaveAs => "Rolidium documents (*.rolidium)",
            Purpose::ImportStep | Purpose::ExportStep => "STEP files (*.step, *.stp)",
            Purpose::ImportStl => "STL files (*.stl)",
        }
    }
    /// Documents are remembered separately from exported and imported exchange files.
    pub fn is_export(self) -> bool {
        matches!(self, Purpose::ExportStep)
    }
}

/// `name` with the default extension of `purpose` added when saving and none of the known ones is there.
pub fn with_extension(purpose: Purpose, name: &str) -> String {
    let name = name.trim().to_string();
    let has = crate::browser::listing::has_extension(
        &name,
        &purpose
            .extensions()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
    );
    if purpose.saving() && !has && !name.is_empty() {
        format!("{name}.{}", purpose.extensions()[0])
    } else {
        name
    }
}

pub struct FileDialog {
    pub purpose: Purpose,
    browser: Browser,
    name: String,
    first: bool,
    confirm: Option<PathBuf>,
    size: egui::Vec2,
    /// Height of everything in the window except the lists (measured, so the window keeps its size).
    chrome: f32,
    /// Height the window adds around its content (title bar, margins), measured.
    overhead: f32,
}

pub enum DialogResult {
    None,
    Cancel,
    Chosen(PathBuf),
}

impl FileDialog {
    pub fn new(
        ctx: &egui::Context,
        purpose: Purpose,
        start_dir: &str,
        name: &str,
        recent: &[String],
        size: (f32, f32),
    ) -> Self {
        let size = if size.0 >= 500.0 && size.1 >= 320.0 {
            egui::vec2(size.0, size.1)
        } else {
            egui::vec2(860.0, 560.0)
        };
        FileDialog {
            purpose,
            browser: Browser::new(ctx, start_dir, purpose.extensions(), purpose.type_label(), recent),
            name: name.to_string(),
            first: true,
            confirm: None,
            size,
            chrome: 135.0,
            overhead: 36.0,
        }
    }

    pub fn dir(&self) -> String {
        self.browser.dir.display().to_string()
    }

    pub fn size(&self) -> (f32, f32) {
        (self.size.x, self.size.y)
    }

    /// The path the current input stands for (with the default extension added when saving).
    pub fn full_path(&self) -> PathBuf {
        self.browser.dir.join(with_extension(self.purpose, &self.name))
    }

    fn choose(&mut self) -> DialogResult {
        let path = self.full_path();
        if self.name.trim().is_empty() {
            return DialogResult::None;
        }
        if self.purpose.saving() {
            if path.is_dir() {
                return DialogResult::None;
            }
            if path.exists() {
                self.confirm = Some(path);
                return DialogResult::None;
            }
        } else if !path.is_file() {
            return DialogResult::None;
        }
        DialogResult::Chosen(path)
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> DialogResult {
        let mut result = DialogResult::None;
        let mut open = true;
        let saving = self.purpose.saving();
        let mut content_h = 0.0;
        let win = egui::Window::new(tr(self.purpose.title()))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(self.size)
            .min_size([620.0, 380.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                // the content follows the remembered window size, so the window does not grow by itself
                let (w, list_h) = (
                    (self.size.x - 30.0).max(560.0),
                    (self.size.y - self.overhead - self.chrome).max(120.0),
                );
                ui.set_width(w);
                let ev = self.browser.ui(ui, ctx, w, list_h);
                match ev {
                    Event::Picked(n) => self.name = n,
                    Event::Activated(n) => {
                        self.name = n;
                        result = self.choose();
                    }
                    Event::None => {}
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(tr("File name:"));
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.name).desired_width(ui.available_width() - 10.0),
                    );
                    if self.first && saving {
                        r.request_focus();
                    }
                    self.first = false;
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        result = self.choose();
                    }
                });
                let path = self.full_path();
                if saving && path.exists() && !path.is_dir() {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 200, 110),
                        tr("This file exists and will be replaced."),
                    );
                } else if !saving && !self.name.trim().is_empty() && !path.is_file() {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 200, 110),
                        tr("This file does not exist."),
                    );
                } else {
                    ui.label("");
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let label = tr(if saving { "Save" } else { "Open" });
                    let ok = !self.name.trim().is_empty() && (saving || path.is_file());
                    if ui.add_enabled(ok, egui::Button::new(label)).clicked() {
                        result = self.choose();
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        result = DialogResult::Cancel;
                    }
                });
                content_h = ui.min_rect().height();
                self.chrome = (content_h - list_h).clamp(120.0, 400.0);
            });
        if let Some(w) = win {
            self.size = w.response.rect.size();
            self.overhead = (w.response.rect.height() - content_h).clamp(20.0, 120.0);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.confirm.is_none() {
            result = DialogResult::Cancel;
        }
        if let Some(p) = self.confirm.clone() {
            let mut answer: Option<bool> = None;
            egui::Window::new(tr("Replace the file?"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    ui.label(crate::i18n::trf(
                        "\"{}\" already exists. Do you want to replace it?",
                        &[name],
                    ));
                    ui.horizontal(|ui| {
                        if ui.button(tr("Replace")).clicked() {
                            answer = Some(true);
                        }
                        if ui.button(tr("Cancel")).clicked() {
                            answer = Some(false);
                        }
                    });
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        answer = Some(false);
                    }
                });
            match answer {
                Some(true) => {
                    self.confirm = None;
                    result = DialogResult::Chosen(p);
                }
                Some(false) => self.confirm = None,
                None => {}
            }
        }
        if !open {
            result = DialogResult::Cancel;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_is_added_when_saving_only() {
        assert_eq!(with_extension(Purpose::SaveAs, "part"), "part.rolidium");
        assert_eq!(
            with_extension(Purpose::SaveAs, " part.ROLIDIUM "),
            "part.ROLIDIUM"
        );
        assert_eq!(with_extension(Purpose::Open, "part"), "part");
        assert_eq!(with_extension(Purpose::ExportStep, "x"), "x.step");
        assert_eq!(with_extension(Purpose::ExportStep, "x.stp"), "x.stp");
        assert_eq!(with_extension(Purpose::SaveAs, ""), "");
    }

    #[test]
    fn saving_over_an_existing_file_asks_first() {
        let base = std::env::temp_dir().join(format!("rolidium-fd-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("part.rolidium"), "x").unwrap();
        let ctx = egui::Context::default();
        let mut d = FileDialog::new(
            &ctx,
            Purpose::SaveAs,
            &base.display().to_string(),
            "part",
            &[],
            (0.0, 0.0),
        );
        assert!(
            matches!(d.choose(), DialogResult::None),
            "an existing file must be confirmed first"
        );
        assert_eq!(d.confirm.as_deref(), Some(base.join("part.rolidium").as_path()));
        d.confirm = None;
        d.name = "fresh".into();
        assert!(matches!(d.choose(), DialogResult::Chosen(p) if p == base.join("fresh.rolidium")));
        let mut o = FileDialog::new(
            &ctx,
            Purpose::Open,
            &base.display().to_string(),
            "nothing.rolidium",
            &[],
            (0.0, 0.0),
        );
        assert!(
            matches!(o.choose(), DialogResult::None),
            "opening a file that does not exist does nothing"
        );
        o.name = "part.rolidium".into();
        assert!(matches!(o.choose(), DialogResult::Chosen(_)));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn exports_and_documents_use_separate_remembered_folders() {
        assert!(Purpose::ExportStep.is_export());
        assert!(
            !Purpose::SaveAs.is_export() && !Purpose::Open.is_export() && !Purpose::ImportStep.is_export()
        );
    }
}
