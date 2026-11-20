// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The Join ends tool (J): joins the free ends of the selected lines and arcs of a sketch (the whole sketch when nothing is
//! selected) or of the selected 3D lines with the shortest segments. The dashed preview shows the connections; Enter confirms,
//! Esc cancels. The pairing is `rolidium_sketch::join` (unit tested).

use super::*;
use crate::i18n::parse_num;
use rolidium_doc::rolidium_sketch::join::{plan_join, JoinPlan};

#[derive(Default)]
pub(super) struct JoinUi {
    pub gap_text: String,
    /// The lines chosen by clicking, and the sketch the tool works on when nothing is chosen.
    pub sel: Vec<(SketchId, EntityId)>,
    pub sketch: Option<SketchId>,
}

impl App {
    /// Why Join ends cannot be used now (`None` when it can).
    pub(super) fn join_unavailable(&self) -> Option<&'static str> {
        let sketch_lines = self.doc.sketches().iter().any(|s| {
            s.sketch
                .entities()
                .iter()
                .any(|(_, e)| rolidium_doc::rolidium_sketch::join::entity_ends(e).is_some())
        });
        let curves = self
            .doc
            .curves()
            .iter()
            .any(|c| matches!(c.def, rolidium_doc::CurveDef::Polyline { .. }));
        if sketch_lines || curves {
            None
        } else {
            Some("There are no open lines to join yet: draw lines or a 3D line first")
        }
    }

    pub(super) fn join_active(&self) -> bool {
        self.tool == Tool::Join && self.sk.is_none()
    }

    pub(super) fn activate_join(&mut self) {
        if let Some(why) = self.join_unavailable() {
            self.set_status(why, true);
            return;
        }
        // the sketch that was open (or the selected line's) is the one that is joined when no lines are chosen
        let open = self.sk.as_ref().and_then(|s| s.sketch);
        self.leave_sketch();
        self.tool = Tool::Join;
        self.join.sel = self.entity_sel.into_iter().collect();
        self.join.sketch = open.or(self.entity_sel.map(|(s, _)| s));
        if self.join.gap_text.is_empty() {
            self.join.gap_text = fmt_num(self.settings.join_gap);
        }
        self.layers_changed = true;
        self.set_status(
            "Join ends: select the lines (Shift adds more; nothing selected means the whole sketch), choose the largest gap, Enter confirms",
            false,
        );
    }

    pub(super) fn join_input(&mut self, resp: &egui::Response, pointer: Option<Pos2>, rect: Rect) {
        if !resp.clicked_by(PointerButton::Primary) {
            return;
        }
        let Some(p) = pointer else { return };
        let add = self.egui_ctx.input(|i| i.modifiers.shift || i.modifiers.command);
        match self.pick_cycled(p, rect, false, true) {
            Some(Obj::Entity(s, id)) => {
                if !add || self.join.sel.first().is_some_and(|(ss, _)| *ss != s) {
                    self.join.sel.clear();
                }
                if let Some(i) = self.join.sel.iter().position(|x| *x == (s, id)) {
                    self.join.sel.remove(i);
                } else {
                    self.join.sel.push((s, id));
                }
                self.join.sketch = Some(s);
                self.curve_sel = None;
            }
            Some(Obj::Curve(c)) => {
                self.curve_sel = Some(c);
                self.join.sel.clear();
            }
            _ => {
                if !add {
                    self.join.sel.clear();
                    self.curve_sel = None;
                }
            }
        }
        self.layers_changed = true;
    }

    fn join_gap(&self) -> Option<f64> {
        parse_num(&self.join.gap_text).filter(|g| g.is_finite() && *g > 0.0)
    }

    /// The sketch the tool works on and the lines of it that take part.
    fn join_lines(&self) -> Option<(SketchId, Vec<(rolidium_doc::rolidium_sketch::EntityId, Entity)>)> {
        if self.curve_sel.is_some() && self.join.sel.is_empty() {
            return None;
        }
        let id = self
            .join
            .sel
            .first()
            .map(|(s, _)| *s)
            .or(self.join.sketch)
            .or_else(|| {
                self.doc
                    .sketches()
                    .iter()
                    .rev()
                    .find(|s| !s.sketch.is_empty())
                    .map(|s| s.id)
            })?;
        let sk = self.doc.sketch(id)?;
        let lines = sk
            .sketch
            .entities()
            .iter()
            .filter(|(e, _)| self.join.sel.is_empty() || self.join.sel.iter().any(|(_, x)| x == e))
            .copied()
            .collect();
        Some((id, lines))
    }

    pub(super) fn join_plan(&self) -> Option<(SketchId, JoinPlan)> {
        let gap = self.join_gap()?;
        let (id, lines) = self.join_lines()?;
        Some((id, plan_join(&lines, gap)))
    }

    pub(super) fn join_apply(&mut self) {
        let Some(gap) = self.join_gap() else {
            self.set_status("The largest gap must be a positive number", true);
            return;
        };
        if !self.idle() {
            return;
        }
        self.settings.join_gap = gap;
        self.settings.save();
        let (cmd, joined, free) = if let (Some(c), true) = (self.curve_sel, self.join.sel.is_empty()) {
            (
                Command::JoinCurves {
                    curves: vec![c],
                    max_gap: gap,
                },
                0,
                0,
            )
        } else {
            let Some((id, plan)) = self.join_plan() else {
                self.set_status("There is nothing to join", true);
                return;
            };
            let ents: Vec<_> = self.join.sel.iter().map(|(_, e)| *e).collect();
            (
                Command::JoinEnds {
                    sketch: id,
                    entities: ents,
                    max_gap: gap,
                },
                plan.joined(),
                plan.free.len(),
            )
        };
        // the doc applies it at once (a few lines: no worker needed), as one step of Undo
        match self.doc.apply(cmd) {
            Ok(_) => {
                self.layers_changed = true;
                self.regions_rev = u64::MAX;
                self.join.sel.clear();
                self.set_status(
                    trf(
                        "Joined gaps: {}. Ends that stayed free: {}.",
                        &[joined.to_string(), free.to_string()],
                    ),
                    false,
                );
                self.tool = Tool::Select;
            }
            Err(e) => self.set_status(e.to_string(), true),
        }
    }

    pub(super) fn join_cancel(&mut self) -> bool {
        if self.tool != Tool::Join {
            return false;
        }
        self.join.sel.clear();
        self.tool = Tool::Select;
        self.layers_changed = true;
        true
    }

    pub(super) fn join_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        if !self.join_active() {
            return;
        }
        let plan = self.join_plan();
        let mut apply = false;
        let mut cancel = false;
        let mut docks = std::mem::take(&mut self.docks);
        docks.panel(
            ctx,
            "join",
            Pos2::new(rect.max.x - 260.0, rect.min.y + 10.0),
            None,
            240.0,
            &tr("Join ends"),
            false,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Largest gap"));
                    ui.add(egui::TextEdit::singleline(&mut self.join.gap_text).desired_width(54.0));
                    ui.label("mm");
                });
                if self.join_gap().is_none() {
                    ui.colored_label(Color32::from_rgb(255, 120, 110), tr("The largest gap must be a positive number"));
                }
                match &plan {
                    Some((_, p)) => {
                        ui.label(trf("Joined gaps: {}. Ends that stayed free: {}.", &[p.joined().to_string(), p.free.len().to_string()]));
                    }
                    None => {
                        ui.label(tr("3D lines: the selected line is joined with the others"));
                    }
                }
                ui.label(
                    egui::RichText::new(tr("Selected lines only, or the whole sketch when nothing is selected. Enter confirms, Esc cancels."))
                        .weak()
                        .small(),
                );
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
        if ctx.input(|i| i.key_pressed(Key::Enter)) {
            apply = true;
        }
        if apply {
            self.join_apply();
        } else if cancel || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.join_cancel();
        }
    }

    /// The dashed connections and the ends that stay free, over the view.
    pub(super) fn join_paint(&self, ui: &egui::Ui, rect: Rect) {
        if !self.join_active() {
            return;
        }
        let Some((id, plan)) = self.join_plan() else {
            return;
        };
        let Some(sk) = self.doc.sketch(id) else { return };
        let painter = ui.painter().with_clip_rect(rect);
        let to_screen = |p: DVec2| self.project(sk.plane.to_3d(p).as_vec3(), rect);
        let green = Color32::from_rgb(120, 255, 160);
        for (a, b) in &plan.connections {
            if let (Some(pa), Some(pb)) = (to_screen(*a), to_screen(*b)) {
                let len = (pb - pa).length().max(1.0);
                let dir = (pb - pa) / len;
                let mut t = 0.0;
                while t < len {
                    let e = (t + 5.0).min(len);
                    painter.line_segment([pa + dir * t, pa + dir * e], egui::Stroke::new(2.0, green));
                    t += 9.0;
                }
                painter.circle_stroke(pa, 4.0, egui::Stroke::new(1.5, green));
                painter.circle_stroke(pb, 4.0, egui::Stroke::new(1.5, green));
            }
        }
        for f in &plan.free {
            if let Some(p) = to_screen(*f) {
                painter.circle_stroke(p, 6.0, egui::Stroke::new(2.0, Color32::from_rgb(255, 110, 100)));
            }
        }
    }
}
