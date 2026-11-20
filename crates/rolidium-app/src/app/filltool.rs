// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The Fill tool (F): a closed flat loop of edges of a body (the edge of a hole, the inner circle of a pipe), of sketch lines or
//! of a 3D line becomes a region on its plane, and the Pull tool starts with that region.

use super::*;

impl App {
    /// Why Fill cannot be used now (`None` when it can).
    pub(super) fn fill_unavailable(&self) -> Option<&'static str> {
        let lines = self.doc.sketches().iter().any(|s| !s.sketch.is_empty()) || !self.doc.curves().is_empty();
        if self.doc.bodies().is_empty() && !lines {
            Some("There is nothing to fill yet: draw lines or open a model")
        } else {
            None
        }
    }

    pub(super) fn activate_fill(&mut self) {
        if let Some(why) = self.fill_unavailable() {
            self.set_status(why, true);
            return;
        }
        self.leave_sketch();
        self.tool = Tool::Fill;
        self.fill_entities.clear();
        self.layers_changed = true;
        if !self.edge_sel.is_empty() || self.entity_sel.is_some() || self.curve_sel.is_some() {
            if let Some(e) = self.entity_sel {
                self.fill_entities = vec![e];
            }
            self.fill_try();
        } else {
            self.set_status(
                "Fill: click an edge of a hole or opening, or the lines of a shape that is not filled yet. Shift adds more.",
                false,
            );
        }
    }

    /// A click in the view while the tool is active: the edge, sketch line or 3D line under the pointer joins the choice.
    pub(super) fn fill_input(&mut self, resp: &egui::Response, pointer: Option<Pos2>, rect: Rect) {
        if !resp.clicked_by(PointerButton::Primary) {
            return;
        }
        let Some(p) = pointer else { return };
        let add = self.egui_ctx.input(|i| i.modifiers.shift || i.modifiers.command);
        let Some(o) = self.pick_cycled(p, rect, true, true) else {
            return;
        };
        match o {
            Obj::Edge(b, e) => {
                if !add || self.edge_sel.first().is_some_and(|(bb, _)| *bb != b) {
                    self.edge_sel.clear();
                }
                if !self.edge_sel.contains(&(b, e)) {
                    self.edge_sel.push((b, e));
                }
                self.fill_entities.clear();
                self.curve_sel = None;
            }
            Obj::Entity(s, id) => {
                if !add || self.fill_entities.first().is_some_and(|(ss, _)| *ss != s) {
                    self.fill_entities.clear();
                }
                if !self.fill_entities.contains(&(s, id)) {
                    self.fill_entities.push((s, id));
                }
                self.edge_sel.clear();
                self.curve_sel = None;
            }
            Obj::Curve(c) => {
                self.curve_sel = Some(c);
                self.edge_sel.clear();
                self.fill_entities.clear();
            }
        }
        self.layers_changed = true;
        self.fill_try();
    }

    /// The smallest region of a sketch that all the given lines border.
    fn region_bordered_by(&mut self, ents: &[(SketchId, EntityId)]) -> Option<(SketchId, usize)> {
        let s = ents.first()?.0;
        if ents.iter().any(|(ss, _)| *ss != s) {
            return None;
        }
        self.refresh_regions();
        let sr = self.regions.iter().find(|r| r.id == s)?;
        sr.regions
            .iter()
            .enumerate()
            .filter(|(_, r)| ents.iter().all(|(_, e)| r.border_entities.contains(e)))
            .min_by(|a, b| a.1.area().total_cmp(&b.1.area()))
            .map(|(i, _)| (s, i))
    }

    /// Makes the region of what is chosen and goes on with Pull.
    pub(super) fn fill_try(&mut self) {
        if !self.idle() {
            return;
        }
        if let Some(&(body, _)) = self.edge_sel.first() {
            let edges: Vec<u32> = self.edge_sel.iter().filter(|(b, _)| *b == body).map(|(_, e)| *e).collect();
            self.fill_pending = true;
            self.submit(Command::FillLoop { body, edges });
        } else if !self.fill_entities.is_empty() {
            let ents = self.fill_entities.clone();
            match self.region_bordered_by(&ents) {
                Some((s, i)) => self.fill_start_pull(s, i),
                None => self.set_status(
                    "These lines do not form a closed shape, so there is nothing to fill. Close the gaps with the Join ends tool (J), or Shift+click the other lines of the shape.",
                    true,
                ),
            }
        } else if let Some(c) = self.curve_sel {
            self.fill_pending = true;
            self.submit(Command::FillCurve { curve: c });
        }
        if !self.fill_pending && self.status.error {
            // nothing was started: the message above stays
        }
    }

    /// The new sketch exists: choose its (largest) region and start the Pull tool with it.
    pub(super) fn fill_after(&mut self, sketch: SketchId) {
        self.refresh_regions();
        let Some(sr) = self.regions.iter().find(|r| r.id == sketch) else {
            return;
        };
        let Some(i) = sr
            .regions
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.area().total_cmp(&b.1.area()))
            .map(|(i, _)| i)
        else {
            return;
        };
        self.fill_start_pull(sketch, i);
    }

    fn fill_start_pull(&mut self, sketch: SketchId, idx: usize) {
        self.clear_selection();
        self.region_sel = Some((sketch, idx));
        self.tool = Tool::Pull;
        let rect = self.viewport_rect;
        if self.begin_pull(PullKind::Region { sketch, idx }, rect.center(), rect) {
            self.set_status(
                "Filled: drag to pull the region, or type the distance and press Enter (a negative distance goes into the body and closes the hole)",
                false,
            );
        }
        self.layers_changed = true;
    }
}
