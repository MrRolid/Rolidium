// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The tool bars: the horizontal bar above the view (sketch and drawing tools) and the left column (modelling tools,
//! edit, view and file). Every button stays where it is: one that cannot be used now is greyed out and its tooltip
//! says why. In a narrow window the top bar shows icons only, and what still does not fit goes under "More".

use super::*;
use crate::i18n::{parse_num, tr};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tb {
    Sketch,
    Draw(SkTool),
    MovePlane,
    PlaneAt,
    PlanView,
    Mode3d,
    Helix,
    Line3d,
}

struct Item {
    act: Tb,
    id: &'static str,
    label: &'static str,
    key: &'static str,
    group: u8,
    selected: bool,
    why: Option<&'static str>,
}

const ICON_W: f32 = 38.0;

/// The main button: (icon, label) before and during a sketch.
fn sketch_button(in_sketch: bool) -> (&'static str, &'static str) {
    if in_sketch {
        ("finish", "Finish sketch")
    } else {
        ("sketch", "Sketch")
    }
}

/// Whether the buttons show their labels and how many fit in a row (the rest goes under "More"): everything with labels,
/// else everything as icons, else as many icons as fit beside the fixed part and a "More" button.
fn layout_mode(
    avail: f32,
    total_full: f32,
    total_icons: f32,
    fixed: f32,
    n: usize,
    gap: f32,
) -> (bool, usize) {
    if total_full <= avail {
        (true, n)
    } else if total_icons <= avail {
        (false, n)
    } else {
        let room = avail - fixed - 70.0;
        (false, ((room / (ICON_W + gap)).floor().max(0.0) as usize).min(n))
    }
}

impl App {
    fn toolbar_items(&self) -> Vec<Item> {
        let in_sketch = self.sk.is_some();
        let first = if in_sketch {
            None
        } else {
            Some("Start a sketch first")
        };
        let drawing = self.sk.as_ref().is_some_and(|m| m.drawing.is_some());
        let tool = self.sk.as_ref().map(|m| m.tool);
        // the main button: starts a sketch, and while sketching it finishes it
        let mut items = vec![Item {
            act: Tb::Sketch,
            id: sketch_button(in_sketch).0,
            label: sketch_button(in_sketch).1,
            key: "K",
            group: 0,
            selected: false,
            why: None,
        }];
        for (t, id, label, key) in [
            (SkTool::Line, "line", "Line", "L"),
            (SkTool::Rect, "rect", "Rectangle", "R"),
            (SkTool::Circle, "circle", "Circle", "C"),
            (SkTool::Arc, "arc", "Arc", "A"),
            (SkTool::Axis, "axis", "Auxiliary axis", "X"),
        ] {
            items.push(Item {
                act: Tb::Draw(t),
                id,
                label,
                key,
                group: 1,
                selected: tool == Some(t) || self.quick_tool == Some(t),
                // from the 3D view too: the face to draw on is chosen first
                why: None,
            });
            if t == SkTool::Line {
                // the 3D switch of the Line tool: points are not tied to the sketch plane
                let why = first.or(if tool == Some(SkTool::Line) {
                    None
                } else {
                    Some("Choose the Line tool first")
                });
                items.push(Item {
                    act: Tb::Line3d,
                    id: "line3d",
                    label: "3D line",
                    key: "",
                    group: 1,
                    selected: self.line3.on,
                    why,
                });
            }
        }
        items.push(Item {
            act: Tb::PlaneAt,
            id: "sketch_plane_at",
            label: "Sketch plane",
            key: "N",
            group: 2,
            selected: self.plane_at.is_some(),
            why: if drawing {
                Some("Not available while a shape is being drawn: finish it first")
            } else {
                None
            },
        });
        items.push(Item {
            act: Tb::MovePlane,
            id: "sketch_plane",
            label: "Move sketch plane",
            key: "",
            group: 2,
            selected: self.plane_move.is_some(),
            why: first.or(if drawing {
                Some("Not available while a shape is being drawn: finish it first")
            } else {
                None
            }),
        });
        items.push(Item {
            act: Tb::Helix,
            id: "helix",
            label: "Helix",
            key: "H",
            group: 3,
            selected: self.tool == Tool::Helix && self.sk.is_none(),
            why: None,
        });
        items.push(Item {
            act: Tb::PlanView,
            id: "plan_view",
            label: "Plan view",
            key: "V",
            group: 3,
            selected: false,
            why: first,
        });
        items.push(Item {
            act: Tb::Mode3d,
            id: "mode_3d",
            label: "3D mode",
            key: "D",
            group: 3,
            selected: false,
            why: first,
        });
        items
    }

    fn toolbar_click(&mut self, act: Tb) {
        match act {
            Tb::Sketch => {
                if self.sk.is_some() && !self.plane_pick {
                    self.leave_sketch();
                } else {
                    self.enter_sketch();
                }
            }
            Tb::Draw(t) => {
                if let Some(sm) = &mut self.sk {
                    sm.set_tool(t);
                } else {
                    self.begin_quick_sketch(t);
                }
            }
            Tb::MovePlane => self.begin_plane_move(),
            Tb::PlaneAt => self.begin_plane_at(),
            Tb::PlanView => self.view_normal_to_sketch(),
            Tb::Mode3d => self.leave_sketch(),
            Tb::Helix => self.activate_helix(),
            Tb::Line3d => {
                self.line3.on = !self.line3.on;
                if !self.line3.on {
                    self.line3_cancel();
                }
            }
        }
    }

    /// The offset of the sketch plane: a field that is always there (greyed out outside a sketch).
    fn offset_field(&mut self, ui: &mut egui::Ui, with_label: bool) {
        let in_sketch = self.sk.is_some();
        let drawing = self.sk.as_ref().is_some_and(|m| m.drawing.is_some());
        let why = if !in_sketch {
            Some("Start a sketch first")
        } else if drawing {
            Some("Not available while a shape is being drawn: finish it first")
        } else {
            None
        };
        let mut scratch = String::from("0");
        let resp = ui
            .add_enabled_ui(why.is_none(), |ui| {
                ui.horizontal(|ui| {
                    if with_label {
                        ui.label(tr("Offset"));
                    }
                    let mut dirty = false;
                    {
                        let text = match &mut self.sk {
                            Some(sm) => &mut sm.offset_text,
                            None => &mut scratch,
                        };
                        let r = ui.add(egui::TextEdit::singleline(text).desired_width(46.0));
                        if r.changed() || r.lost_focus() {
                            dirty = true;
                        }
                    }
                    if dirty {
                        if let Some(sm) = &mut self.sk {
                            if let Some(v) = parse_num(&sm.offset_text) {
                                if (v - sm.offset).abs() > 1e-12 {
                                    sm.set_offset(v);
                                    // drawing now goes into the sketch that lies on the new plane, if any
                                    sm.sketch = self
                                        .doc
                                        .sketches()
                                        .iter()
                                        .find(|s| s.target == sm.body && s.plane.same_plane(&sm.plane))
                                        .map(|s| s.id);
                                }
                            }
                        }
                    }
                    ui.label("mm");
                })
                .response
            })
            .response;
        match why {
            Some(w) => {
                resp.on_disabled_hover_text(tr(w));
            }
            None => {
                resp.on_hover_text(tr(
                    "Distance of the sketch plane from where it started; positive is along its normal",
                ));
            }
        }
    }

    pub(super) fn top_toolbar(&mut self, ui: &mut egui::Ui) {
        let items = self.toolbar_items();
        let avail = ui.available_width();
        let gap = ui.spacing().item_spacing.x;
        let font = egui::TextStyle::Button.resolve(ui.style());
        let text_w = |t: &str| {
            ui.painter()
                .layout_no_wrap(t.to_string(), font.clone(), Color32::WHITE)
                .size()
                .x
        };
        let full_w: Vec<f32> = items
            .iter()
            .map(|i| {
                30.0 + 8.0
                    + text_w(&format!(
                        "{}{}",
                        tr(i.label),
                        if i.key.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", i.key)
                        }
                    ))
                    + 22.0
            })
            .collect();
        let offset_full = text_w(&tr("Offset")) + 46.0 + 30.0 + 3.0 * gap;
        let offset_compact = 46.0 + 30.0 + 2.0 * gap;
        let seps = 3.0 * (gap + 8.0);
        let total_full: f32 =
            (full_w.iter().sum::<f32>() + items.len() as f32 * gap + offset_full + seps) + 12.0 + 40.0;
        let total_icons: f32 =
            items.len() as f32 * (ICON_W + gap) + (icons::MAIN_ICON_W - ICON_W) + offset_compact + seps;
        let (full, shown) = layout_mode(
            avail,
            total_full,
            total_icons,
            offset_compact + seps,
            items.len(),
            gap,
        );
        let mut clicked: Option<Tb> = None;
        ui.horizontal_centered(|ui| {
            let mut last_group = 0u8;
            for (i, item) in items.iter().enumerate().take(shown) {
                if item.group != last_group {
                    ui.separator();
                    last_group = item.group;
                }
                if item.key.is_empty() {
                    icons::stateful();
                }
                let r = if item.act == Tb::Sketch {
                    icons::main_button(ui, &self.help, item.id, &tr(item.label), item.key, full)
                } else if full {
                    // the shortcut is on the help card, not on the button, so the labels fit a 1280 px window
                    icons::tool_button_key(
                        ui,
                        &self.help,
                        item.id,
                        &tr(item.label),
                        item.key,
                        item.selected,
                        item.why,
                    )
                } else {
                    icons::icon_button_why(
                        ui,
                        &self.help,
                        item.id,
                        &tr(item.label),
                        item.selected,
                        item.why,
                        ICON_W,
                        item.key,
                    )
                };
                if r.clicked() {
                    clicked = Some(item.act);
                }
                if item.act == Tb::MovePlane {
                    self.offset_field(ui, full);
                }
                let _ = i;
            }
            if shown < items.len() {
                ui.separator();
                ui.menu_button(tr("More"), |ui| {
                    for item in &items[shown..] {
                        let label = format!(
                            "{}{}",
                            tr(item.label),
                            if item.key.is_empty() {
                                String::new()
                            } else {
                                format!(" ({})", item.key)
                            }
                        );
                        let b = egui::Button::image_and_text(icons::icon(item.id, 20.0), label)
                            .selected(item.selected);
                        let r = ui.add_enabled(item.why.is_none(), b);
                        let r = match item.why {
                            Some(w) => r.on_disabled_hover_text(tr(w)),
                            None => r,
                        };
                        if r.clicked() {
                            clicked = Some(item.act);
                            ui.close();
                        }
                    }
                });
                if !items[..shown].iter().any(|i| i.act == Tb::MovePlane) {
                    self.offset_field(ui, false);
                }
            }
        });
        if let Some(a) = clicked {
            self.toolbar_click(a);
        }
    }

    fn flat_face_count(&self) -> usize {
        self.meshes
            .iter()
            .map(|m| m.faces.iter().filter(|f| f.kind == FaceKind::Plane).count())
            .sum()
    }

    /// The left column: modelling tools, edges, edit, view, file, settings.
    pub(super) fn left_column(&mut self, ui: &mut egui::Ui) {
        let idle = self.idle();
        let busy: Option<&'static str> = if idle {
            None
        } else {
            Some("Wait until the running operation finishes")
        };
        let bodies = self.doc.bodies().len() + self.doc.meshes().len();
        let regions: usize = self.regions.iter().map(|r| r.regions.len()).sum();
        ui.add_space(4.0);
        // tight rows: the whole column must fit a window 720 px high
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(egui::RichText::new(tr("Tools")).strong());
        let profiles = regions + self.flat_face_count();
        for (tool, id, name, key, why) in [
            (Tool::Select, "select", "Select", "S", None),
            (
                Tool::Pull,
                "pull",
                "Pull",
                "P",
                if bodies == 0 && regions == 0 {
                    Some("Nothing to pull yet: draw a sketch or open a model")
                } else {
                    None
                },
            ),
            (
                Tool::Move,
                "move",
                "Move",
                "M",
                if bodies == 0 {
                    Some("There is no body to move")
                } else {
                    None
                },
            ),
            (
                Tool::Combine,
                "combine",
                "Combine",
                "I",
                if bodies < 2 {
                    Some("Combine needs at least two bodies")
                } else {
                    None
                },
            ),
            (
                Tool::Loft,
                "loft",
                "Loft",
                "L",
                if profiles < 2 {
                    Some("Loft needs at least two profiles: sketch regions or flat faces")
                } else {
                    None
                },
            ),
            (
                Tool::Sweep,
                "sweep",
                "Sweep",
                "W",
                if profiles == 0 {
                    Some("Sweep needs a profile (a sketch region or a flat face) and a path")
                } else {
                    None
                },
            ),
            (
                Tool::Measure,
                "measure",
                "Measure",
                "E",
                self.measure_unavailable(),
            ),
        ] {
            // in a sketch the Select button is the sketch's own selecting (no drawing tool), it does not leave the sketch
            let in_sketch_select = tool == Tool::Select && self.sk.is_some();
            let sel = if in_sketch_select {
                self.sk.as_ref().is_some_and(|m| m.tool == SkTool::None)
            } else {
                self.tool == tool && self.sk.is_none()
            };
            let r = icons::tool_button_why(ui, &self.help, id, &format!("{} ({key})", tr(name)), sel, why);
            if tool == Tool::Pull {
                self.pull_btn_rect = r.rect;
            }
            if r.clicked() && in_sketch_select {
                self.sketch_select_tool();
            } else if r.clicked() {
                self.tool = tool;
                self.leave_sketch();
                if tool == Tool::Loft {
                    self.activate_loft();
                }
                if tool == Tool::Sweep {
                    self.activate_sweep();
                }
                if tool == Tool::Measure {
                    self.activate_measure();
                }
            }
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            icons::stateful();
            let r = icons::icon_button_why(
                ui,
                &self.help,
                "fill",
                &format!("{} (F)", tr("Fill")),
                self.tool == Tool::Fill,
                busy.or(self.fill_unavailable()),
                34.0,
                "F",
            );
            if r.clicked() {
                self.activate_fill();
            }
            icons::stateful();
            let r = icons::icon_button_why(
                ui,
                &self.help,
                "join",
                &format!("{} (J)", tr("Join ends")),
                self.tool == Tool::Join,
                busy.or(self.join_unavailable()),
                34.0,
                "J",
            );
            if r.clicked() {
                self.activate_join();
            }
            for (mode, id, name, key) in [
                (super::linktool::LinkMode::Linear, "pattern", "Pattern", "T"),
                (super::linktool::LinkMode::Mirror, "mirror", "Mirror", "B"),
            ] {
                icons::stateful();
                let tool = if mode == super::linktool::LinkMode::Mirror {
                    Tool::Mirror
                } else {
                    Tool::Pattern
                };
                let why = busy.or(
                    if self.doc.bodies().is_empty()
                        && self.doc.meshes().is_empty()
                        && !self.sk.as_ref().is_some_and(|m| m.sketch.is_some())
                    {
                        Some("There is no body to repeat or mirror")
                    } else {
                        None
                    },
                );
                let r = icons::icon_button_why(
                    ui,
                    &self.help,
                    id,
                    &format!("{} ({key})", tr(name)),
                    self.tool == tool && self.sk.is_none(),
                    why,
                    34.0,
                    key,
                );
                if r.clicked() {
                    self.activate_link_tool(mode);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            icons::stateful();
            let why = busy.or(if self.doc.bodies().is_empty() && self.doc.meshes().is_empty() {
                Some("There is no body to make hollow")
            } else {
                None
            });
            let r = icons::icon_button_why(
                ui,
                &self.help,
                "shell",
                &format!("{} (O)", tr("Shell")),
                self.tool == Tool::Shell && self.sk.is_none(),
                why,
                34.0,
                "O",
            );
            if r.clicked() {
                self.activate_shell();
            }
            icons::stateful();
            let r = icons::icon_button_why(
                ui,
                &self.help,
                "text",
                &format!("{} (G)", tr("Text")),
                self.tool == Tool::Text && self.sk.is_none(),
                busy,
                34.0,
                "G",
            );
            if r.clicked() {
                self.activate_text();
            }
        });
        let undo_why = busy.or(if self.doc.can_undo() {
            None
        } else {
            Some("There is nothing to undo")
        });
        let redo_why = busy.or(if self.doc.can_redo() {
            None
        } else {
            Some("There is nothing to redo")
        });
        // Edges and Edit side by side: the left column must fit a window 720 px high
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.label(egui::RichText::new(tr("Edges")).strong());
            ui.add_space(34.0 * 2.0 + 3.0 - ui.min_rect().width().min(70.0) + 4.0);
            ui.label(egui::RichText::new(tr("Edit")).strong());
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            if icons::icon_button_why(
                ui,
                &self.help,
                "round",
                &tr("Round"),
                !self.edge_chamfer,
                None,
                34.0,
                "",
            )
            .clicked()
            {
                self.edge_chamfer = false;
            }
            if icons::icon_button_why(
                ui,
                &self.help,
                "chamfer",
                &tr("Chamfer"),
                self.edge_chamfer,
                None,
                34.0,
                "",
            )
            .clicked()
            {
                self.edge_chamfer = true;
            }
            ui.add_space(6.0);
            icons::stateful();
            if icons::icon_button_why(ui, &self.help, "undo", &tr("Undo"), false, undo_why, 34.0, "")
                .clicked()
            {
                self.undo();
            }
            icons::stateful();
            if icons::icon_button_why(ui, &self.help, "redo", &tr("Redo"), false, redo_why, 34.0, "")
                .clicked()
            {
                self.redo();
            }
        });
        // the size of a round or a chamfer can be set before an edge is clicked (the panel next to the edge starts with it and can change it)
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.label(if self.edge_chamfer { tr("Chamfer") } else { tr("Round") });
            let r = ui.add(egui::TextEdit::singleline(&mut self.edge_size_text).desired_width(40.0));
            self.probe_widget("round_size", r.rect);
            ui.label("mm");
        });
        ui.separator();
        ui.label(egui::RichText::new(tr("View")).strong());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for (id, name, v) in [
                ("view_top", "Top", StandardView::Top),
                ("view_front", "Front", StandardView::Front),
                ("view_right", "Right", StandardView::Right),
                ("view_iso", "Isometric", StandardView::Isometric),
            ] {
                if icons::icon_button_why(ui, &self.help, id, &tr(name), false, None, 34.0, "").clicked() {
                    self.animate_standard_view(v);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            icons::stateful();
            if icons::icon_button_why(ui, &self.help, "xray", &format!("{} (Q)", tr("X-ray")), self.xray, None, 34.0, "Q").clicked() {
                self.toggle_xray();
            }
            icons::stateful();
            let r = icons::icon_button_why(ui, &self.help, "section", &format!("{} (Y)", tr("Section")), self.section.on, None, 34.0, "Y");
            self.probe_widget("section_button", r.rect);
            if r.clicked() {
                self.toggle_section();
            }
        });
        if icons::tool_button_why(ui, &self.help, "fit_all", &tr("Fit all"), false, None).clicked() {
            self.fit_all();
        }
    }

    /// The File block under the scrolling part of the left column: framed, with icons of `size` px and `per_row` buttons in a row.
    pub(super) fn left_file_block(&mut self, ui: &mut egui::Ui, size: f32, per_row: usize) {
        let idle = self.idle();
        let busy: Option<&'static str> = if idle {
            None
        } else {
            Some("Wait until the running operation finishes")
        };
        let bodies = self.doc.bodies().len() + self.doc.meshes().len();
        let nothing = if bodies == 0 {
            Some("There is nothing to export")
        } else {
            None
        };
        let dark = ui.visuals().dark_mode;
        egui::Frame::new()
            .stroke(Stroke::new(1.5, icons::file_frame_color(dark)))
            .corner_radius(6.0)
            .inner_margin(5.0)
            .show(ui, |ui| {
                ui.label(egui::RichText::new(tr("File")).strong());
                let items: [(&str, &str, Option<&'static str>, u8); 8] = [
                    ("file_new", "New", busy, 0),
                    ("file_open", "Open", busy, 1),
                    ("file_save", "Save", busy, 2),
                    ("file_save_as", "Save As", busy, 3),
                    ("import_step", "Import STEP", busy, 4),
                    ("import_stl", "Import STL or 3MF", busy, 7),
                    ("export_step", "Export STEP", busy.or(nothing), 5),
                    ("export_stl", "Export STL", busy.or(nothing), 6),
                ];
                let mut clicked = None;
                ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
                for row in items.chunks(per_row) {
                    ui.horizontal(|ui| {
                        for (id, name, why, n) in row {
                            if icons::file_button(ui, &self.help, id, &tr(name), *why, size).clicked() {
                                clicked = Some(*n);
                            }
                        }
                    });
                }
                match clicked {
                    Some(0) => self.request(Pending::New),
                    Some(1) => self.request(Pending::OpenDialog),
                    Some(2) => self.save_clicked(false),
                    Some(3) => self.save_clicked(true),
                    Some(4) => self.import_step_dialog(),
                    Some(5) => self.export_step_dialog(),
                    Some(6) => self.export_stl(),
                    Some(7) => self.import_stl_dialog(),
                    _ => {}
                }
                ui.add_enabled_ui(!self.settings.recent.is_empty() && idle, |ui| {
                    ui.menu_button(tr("Recent files"), |ui| {
                        let recent = self.settings.recent.clone();
                        for r in recent {
                            let name = std::path::Path::new(&r)
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| r.clone());
                            if ui.button(name).on_hover_text(&r).clicked() {
                                self.request(Pending::Open(PathBuf::from(r)));
                                ui.close();
                            }
                        }
                    });
                });
            });
    }

    /// Settings and About at the bottom of the left column.
    pub(super) fn left_bottom(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        let row = ui.horizontal(|ui| {
            if icons::icon_button_why(
                ui,
                &self.help,
                "settings",
                &tr("Settings"),
                self.settings_open,
                None,
                62.0,
                "",
            )
            .clicked()
            {
                self.settings_open = !self.settings_open;
            }
            if icons::icon_button_why(
                ui,
                &self.help,
                "about",
                &tr("About"),
                self.about_open,
                None,
                62.0,
                "",
            )
            .clicked()
            {
                self.about_open = !self.about_open;
                self.about_scroll = false;
            }
        });
        self.left_bottom_rect = row.response.rect;
    }
}

/// The File block's icon size and buttons per row for the given height of the whole column and the height of its scrolling part:
/// the largest icons that leave room for the whole part above them, else the small ones (the part above then scrolls).
pub(super) fn file_block_layout(column_h: f32, middle_h: f32) -> (f32, usize) {
    for (size, per_row) in [(38.0, 3), (32.0, 3)] {
        if middle_h + file_block_height(size, per_row) + LEFT_BOTTOM_H <= column_h {
            return (size, per_row);
        }
    }
    (22.0, 4)
}

/// The height of the File block (rows of buttons, title, recent files, frame).
pub(super) fn file_block_height(size: f32, per_row: usize) -> f32 {
    let rows = 7usize.div_ceil(per_row) as f32;
    rows * (size + 8.0 + 3.0) + 58.0
}

/// The height of the Settings / About row with its spacing.
pub(super) const LEFT_BOTTOM_H: f32 = 46.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sketch_button_changes_with_the_mode() {
        assert_eq!(sketch_button(false), ("sketch", "Sketch"));
        assert_eq!(sketch_button(true), ("finish", "Finish sketch"));
    }

    #[test]
    fn the_bar_shows_labels_then_icons_then_a_more_menu() {
        // wide: labels; narrower: icons only; narrow: some icons and the rest under "More"
        assert_eq!(layout_mode(1200.0, 900.0, 420.0, 150.0, 8, 8.0), (true, 8));
        assert_eq!(layout_mode(600.0, 900.0, 420.0, 150.0, 8, 8.0), (false, 8));
        let (labels, shown) = layout_mode(380.0, 900.0, 420.0, 150.0, 8, 8.0);
        assert!(!labels && (3..8).contains(&shown), "{shown}");
        // never negative, never more than there are buttons
        assert_eq!(layout_mode(50.0, 900.0, 420.0, 150.0, 8, 8.0), (false, 0));
        assert_eq!(layout_mode(5000.0, 9000.0, 8000.0, 150.0, 8, 8.0), (false, 8));
    }
}
