// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Built-in STL export dialog (works without any desktop portal): folder browser, file name,
//! fineness presets with a live summary of the result.

use crate::browser::{Browser, Event};
use crate::i18n::{parse_num, tr, trf};
use crate::settings::Settings;
use crate::worker::StlParams;
use eframe::egui;
use std::path::{Path, PathBuf};

pub const PRESETS: [(&str, f64, f64); 3] = [
    ("Draft", 0.05, 15.0),
    ("Standard", 0.02, 10.0),
    ("Fine", 0.01, 5.0),
];

pub enum DialogAction {
    None,
    Export(PathBuf, StlParams),
    /// The fineness changed: the app should count triangles for these parameters.
    NeedStats(StlParams),
}

pub struct ExportDialog {
    pub open: bool,
    dir: String,
    name: String,
    preset: String,
    dev_text: String,
    ang_text: String,
    ascii: bool,
    /// How many bodies and meshes are selected (the option to export only them is offered when there are some), and whether it is chosen.
    pub selected: usize,
    pub only_selected: bool,
    /// The triangles of the meshes (STL) that go into the file as they are: they are added to the count of the solids.
    pub extra_triangles: usize,
    /// Triangle count for `stats_for`.
    pub stats: Option<(StlParams, usize)>,
    pub stats_requested: Option<StlParams>,
    browser: Option<Browser>,
    size: egui::Vec2,
    confirm: bool,
    chrome: f32,
    /// Height the window adds around its content (title bar, margins), measured.
    overhead: f32,
}

/// Number of straight pieces a circle of `diameter` mm is cut into.
pub fn circle_segments(deviation: f64, angle_deg: f64, diameter: f64) -> usize {
    let r = diameter / 2.0;
    let by_dev = if deviation < r {
        2.0 * (1.0 - deviation / r).acos()
    } else {
        std::f64::consts::PI
    };
    let step = by_dev.min(angle_deg.to_radians()).max(1e-6);
    ((std::f64::consts::TAU / step - 1e-9).ceil() as usize).max(3)
}

pub fn estimate_bytes(triangles: usize, ascii: bool) -> u64 {
    if ascii {
        triangles as u64 * 262 + 30
    } else {
        84 + 50 * triangles as u64
    }
}

pub fn human(bytes: u64) -> String {
    let b = bytes as f64;
    if b < 1024.0 {
        format!("{bytes} B")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else {
        format!("{:.2} MB", b / 1024.0 / 1024.0)
    }
}

impl ExportDialog {
    pub fn new(s: &Settings) -> Self {
        let dir = if s.export_dir.is_empty() || !Path::new(&s.export_dir).is_dir() {
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| std::path::MAIN_SEPARATOR.to_string())
        } else {
            s.export_dir.clone()
        };
        ExportDialog {
            open: false,
            dir,
            name: "model.stl".into(),
            preset: s.preset.clone(),
            dev_text: format!("{}", s.deviation),
            ang_text: format!("{}", s.angle_deg),
            ascii: s.ascii,
            selected: 0,
            only_selected: false,
            extra_triangles: 0,
            stats: None,
            stats_requested: None,
            browser: None,
            size: if s.dialog_w >= 500.0 && s.dialog_h >= 320.0 {
                egui::vec2(s.dialog_w, s.dialog_h)
            } else {
                egui::vec2(860.0, 600.0)
            },
            confirm: false,
            chrome: 330.0,
            overhead: 36.0,
        }
    }

    pub fn params(&self) -> Option<StlParams> {
        let dev: f64 = parse_num(&self.dev_text)?;
        let ang: f64 = parse_num(&self.ang_text)?;
        if (0.001..=10.0).contains(&dev) && (1.0..=90.0).contains(&ang) {
            Some(StlParams {
                deviation: dev,
                angle_deg: ang,
                ascii: self.ascii,
            })
        } else {
            None
        }
    }

    pub fn settings(&self, base: &Settings) -> Settings {
        let p = self.params();
        Settings {
            export_dir: self
                .browser
                .as_ref()
                .map_or_else(|| self.dir.clone(), |b| b.dir.display().to_string()),
            preset: self.preset.clone(),
            deviation: p.map_or(base.deviation, |p| p.deviation),
            angle_deg: p.map_or(base.angle_deg, |p| p.angle_deg),
            ascii: self.ascii,
            ..base.clone()
        }
    }

    pub fn full_path(&self) -> PathBuf {
        let mut name = self.name.trim().to_string();
        if !name.to_lowercase().ends_with(".stl") {
            name.push_str(".stl");
        }
        let dir = self
            .browser
            .as_ref()
            .map_or_else(|| PathBuf::from(&self.dir), |b| b.dir.clone());
        dir.join(name)
    }

    pub fn ui(&mut self, ctx: &egui::Context, busy: bool, recent: &[String]) -> DialogAction {
        if !self.open {
            return DialogAction::None;
        }
        if self.browser.is_none() {
            self.browser = Some(Browser::new(
                ctx,
                &self.dir,
                &["stl"],
                "STL files (*.stl)",
                recent,
            ));
        }
        let mut action = DialogAction::None;
        let mut open = self.open;
        let mut content_h = 0.0_f32;
        let shown = egui::Window::new(tr("Export STL"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(self.size)
            .min_size([620.0, 460.0])
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(tr("Where to save")).strong());
                let (w, list_h) = (
                    (self.size.x - 30.0).max(560.0),
                    (self.size.y - self.overhead - self.chrome).max(110.0),
                );
                ui.set_width(w);
                if let Some(b) = &mut self.browser {
                    if let Event::Picked(n) | Event::Activated(n) = b.ui(ui, ctx, w, list_h) {
                        self.name = n;
                    }
                }
                ui.horizontal(|ui| {
                    ui.label(tr("File name:"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.name).desired_width(ui.available_width() - 10.0),
                    );
                });
                ui.separator();
                ui.label(egui::RichText::new(tr("Fineness")).strong());
                ui.horizontal(|ui| {
                    for (name, dev, ang) in PRESETS {
                        if ui.selectable_label(self.preset == name, tr(name)).clicked() {
                            self.preset = name.into();
                            self.dev_text = format!("{dev}");
                            self.ang_text = format!("{ang}");
                        }
                    }
                    if ui
                        .selectable_label(self.preset == "Custom", tr("Custom"))
                        .clicked()
                    {
                        self.preset = "Custom".into();
                    }
                });
                egui::Grid::new("fineness").num_columns(2).show(ui, |ui| {
                    ui.label(tr("Largest deviation from the exact shape (mm):"));
                    if ui
                        .add(egui::TextEdit::singleline(&mut self.dev_text).desired_width(70.0))
                        .changed()
                    {
                        self.preset = "Custom".into();
                    }
                    ui.end_row();
                    ui.label(tr("Largest angle between neighbouring facets (degrees):"));
                    if ui
                        .add(egui::TextEdit::singleline(&mut self.ang_text).desired_width(70.0))
                        .changed()
                    {
                        self.preset = "Custom".into();
                    }
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Format:"));
                    ui.radio_value(&mut self.ascii, false, tr("Binary (smaller)"));
                    ui.radio_value(&mut self.ascii, true, tr("ASCII (readable)"));
                });
                if self.selected > 0 {
                    let before = self.only_selected;
                    ui.checkbox(
                        &mut self.only_selected,
                        trf(
                            "Only the selected bodies ({} selected)",
                            &[self.selected.to_string()],
                        ),
                    );
                    if before != self.only_selected {
                        self.stats = None;
                        self.stats_requested = None;
                    }
                }
                ui.separator();
                let params = self.params();
                match params {
                    None => {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 120, 110),
                            tr("Deviation must be 0.001 to 10 mm and the angle 1 to 90 degrees."),
                        );
                    }
                    Some(p) => {
                        let n = circle_segments(p.deviation, p.angle_deg, 10.0);
                        ui.label(trf(
                            "A circle of 10 mm diameter is cut into {} pieces.",
                            &[n.to_string()],
                        ));
                        // statistics for the model (computed in the background)
                        let current = self
                            .stats
                            .filter(|(sp, _)| sp.deviation == p.deviation && sp.angle_deg == p.angle_deg);
                        match current {
                            Some((_, tris)) => {
                                ui.label(trf(
                                    "Triangles: {}   Estimated file size: {}",
                                    &[
                                        (tris + self.extra_triangles).to_string(),
                                        human(estimate_bytes(tris + self.extra_triangles, p.ascii)),
                                    ],
                                ));
                            }
                            None => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(tr("Counting triangles…"));
                                });
                                let want = StlParams { ascii: false, ..p };
                                if self.stats_requested != Some(want) {
                                    self.stats_requested = Some(want);
                                    action = DialogAction::NeedStats(want);
                                }
                            }
                        }
                    }
                }
                let path = self.full_path();
                if path.exists() {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 200, 110),
                        tr("This file exists and will be replaced."),
                    );
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let ok = params.is_some() && !self.name.trim().is_empty() && !busy;
                    if ui.add_enabled(ok, egui::Button::new(tr("Export"))).clicked() {
                        if let Some(p) = params {
                            if path.exists() && !self.confirm {
                                self.confirm = true;
                            } else {
                                self.confirm = false;
                                action = DialogAction::Export(path.clone(), p);
                            }
                        }
                    }
                    if self.confirm {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 200, 110),
                            tr("Press Export again to replace the file."),
                        );
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        self.open = false;
                    }
                });
                content_h = ui.min_rect().height();
                self.chrome = (content_h - list_h).clamp(200.0, 700.0);
            });
        if let Some(w) = shown {
            self.size = w.response.rect.size();
            self.overhead = (w.response.rect.height() - content_h).clamp(20.0, 120.0);
        }
        if !open {
            self.open = false;
        }
        if !self.open {
            self.browser = None;
            self.confirm = false;
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_counts_follow_the_settings() {
        let draft = circle_segments(0.05, 15.0, 10.0);
        let standard = circle_segments(0.02, 10.0, 10.0);
        let fine = circle_segments(0.01, 5.0, 10.0);
        assert!(draft < standard && standard < fine, "{draft} {standard} {fine}");
        assert_eq!(draft, 24);
        assert!(standard >= 36);
        assert!(fine >= 72);
        assert!(estimate_bytes(1000, true) > estimate_bytes(1000, false));
        assert_eq!(estimate_bytes(12, false), 84 + 600);
    }
}
