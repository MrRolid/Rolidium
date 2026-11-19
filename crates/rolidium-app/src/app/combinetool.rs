// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The Combine tool: pick a target body and one or more tool bodies, then merge, cut or intersect.

use super::*;
use crate::i18n::tr;
use rolidium_doc::CombineOp;

pub struct CombineUi {
    pub target: Option<BodyId>,
    pub tools: Vec<BodyId>,
    pub op: CombineOp,
    pub keep: bool,
}

impl Default for CombineUi {
    fn default() -> Self {
        CombineUi {
            target: None,
            tools: Vec::new(),
            op: CombineOp::Merge,
            keep: false,
        }
    }
}

impl App {
    /// All edges of a body as line pairs (for highlighting a whole body).
    pub(super) fn body_edge_lines(&self, body: BodyId) -> Vec<Vec3> {
        self.entry(body)
            .map(|m| {
                m.mesh
                    .edges
                    .iter()
                    .flat_map(|e| e.points.windows(2).flat_map(|w| [w[0], w[1]]).collect::<Vec<_>>())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn combine_input(&mut self, resp: &egui::Response) {
        let hover = if resp.hovered() {
            self.hovered_face().map(|(b, _)| b)
        } else {
            None
        };
        if hover != self.hover_body {
            self.hover_body = hover;
            self.layers_changed = true;
        }
        if resp.clicked_by(PointerButton::Primary) {
            if let Some(b) = hover {
                let c = &mut self.combine;
                if c.target.is_none() {
                    c.target = Some(b);
                } else if c.target == Some(b) {
                    c.target = None;
                } else if let Some(i) = c.tools.iter().position(|t| *t == b) {
                    c.tools.remove(i);
                } else {
                    c.tools.push(b);
                }
                self.layers_changed = true;
            }
        }
    }

    pub(super) fn apply_combine(&mut self) {
        let (Some(target), false) = (self.combine.target, self.combine.tools.is_empty()) else {
            self.set_status("Click the target body, then the bodies to combine with it", true);
            return;
        };
        if !self.idle() {
            return;
        }
        self.pending = None;
        self.submit(Command::Combine {
            target,
            tools: self.combine.tools.clone(),
            op: self.combine.op,
            keep_tools: self.combine.keep,
        });
        self.combine.target = None;
        self.combine.tools.clear();
        self.layers_changed = true;
        self.set_status("Computing…", false);
    }

    pub(super) fn combine_layers(&self) -> Vec<PreviewDraw> {
        let mut out = Vec::new();
        if let Some(b) = self.hover_body {
            out.push(PreviewDraw {
                thick: self.body_edge_lines(b),
                thick_color: [1.0, 0.72, 0.25, 1.0],
                ..Default::default()
            });
        }
        if self.tool != Tool::Combine {
            return out;
        }
        if let Some(t) = self.combine.target {
            out.push(PreviewDraw {
                thick: self.body_edge_lines(t),
                thick_color: [0.25, 0.85, 1.0, 1.0],
                ..Default::default()
            });
        }
        let tools: Vec<Vec3> = self
            .combine
            .tools
            .iter()
            .flat_map(|t| self.body_edge_lines(*t))
            .collect();
        if !tools.is_empty() {
            out.push(PreviewDraw {
                thick: tools,
                thick_color: [0.95, 0.35, 0.95, 1.0],
                ..Default::default()
            });
        }
        out
    }

    #[allow(deprecated)]
    pub(super) fn combine_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        if self.tool != Tool::Combine {
            return;
        }
        let mut apply = false;
        let mut cancel = false;
        let mut docks = std::mem::take(&mut self.docks);
        let dock = docks.panel(
            ctx,
            "combine",
            Pos2::new(rect.max.x - 250.0, rect.min.y + 10.0),
            if docks.is_moved("combine") {
                self.combine.target.and_then(|b| self.body_center_screen(b, rect))
            } else {
                None
            },
            230.0,
            &tr("Combine"),
            false,
            |ui| {
                let t = match self.combine.target.and_then(|b| self.doc.body(b)) {
                    Some(b) => b.name.clone(),
                    None => tr("click the target body"),
                };
                ui.horizontal(|ui| {
                    ui.colored_label(Color32::from_rgb(64, 217, 255), tr("Target"));
                    ui.label(t);
                });
                let names: Vec<String> = self
                    .combine
                    .tools
                    .iter()
                    .filter_map(|b| self.doc.body(*b).map(|b| b.name.clone()))
                    .collect();
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(Color32::from_rgb(242, 89, 242), tr("Tools"));
                    ui.label(if names.is_empty() {
                        tr("click the other bodies")
                    } else {
                        names.join(", ")
                    });
                });
                ui.horizontal(|ui| {
                    for (o, n) in [
                        (CombineOp::Merge, "Merge"),
                        (CombineOp::Cut, "Cut"),
                        (CombineOp::Intersect, "Intersect"),
                    ] {
                        ui.selectable_value(&mut self.combine.op, o, tr(n));
                    }
                });
                ui.add_enabled(
                    self.combine.op != CombineOp::Merge,
                    egui::Checkbox::new(&mut self.combine.keep, tr("Keep tools")),
                )
                .on_hover_text(tr("The subtracted or intersected bodies stay in the document"));
                ui.horizontal(|ui| {
                    if ui.button(tr("Apply")).clicked() {
                        apply = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            },
        );
        self.docks = docks;
        let _ = dock;
        if apply {
            self.apply_combine();
        }
        if cancel {
            self.combine.target = None;
            self.combine.tools.clear();
            self.layers_changed = true;
        }
    }
}
