// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The two ways of looking inside a model that change only the picture, never the model: the **X-ray** view (Q: every body is half transparent with all its edges)
//! and the **section** (Y: a plane cuts the picture, the cut is filled with colour). They are drawn by the graphics (a clip plane in the shader), work with every tool
//! and are used by the Shell tool to show the thickness of a wall.

use super::*;
use crate::i18n::{tr, trf};

/// The section: a plane across the model.
#[derive(Clone, Debug)]
pub struct SectionUi {
    pub on: bool,
    /// 0, 1, 2: the X, Y or Z axis; 3: the plane of a face that was clicked.
    pub axis: usize,
    pub normal: DVec3,
    pub pos: f64,
    pub flip: bool,
    /// A click on a flat face takes its plane.
    pub pick: bool,
    /// The range of the position (the box of the model along the axis).
    pub lo: f64,
    pub hi: f64,
    /// The section was turned on by a tool (the Shell), not by the person: the tool turns it off again.
    pub by_tool: bool,
}

impl Default for SectionUi {
    fn default() -> Self {
        SectionUi {
            on: false,
            axis: 1,
            normal: DVec3::Y,
            pos: 0.0,
            flip: false,
            pick: false,
            lo: -50.0,
            hi: 50.0,
            by_tool: false,
        }
    }
}

impl SectionUi {
    pub fn axis_normal(&self) -> DVec3 {
        match self.axis {
            0 => DVec3::X,
            1 => DVec3::Y,
            2 => DVec3::Z,
            _ => self.normal,
        }
    }

    /// The plane for the graphics: what lies where `n . p + d` is negative is cut away.
    pub fn plane(&self) -> [f32; 4] {
        let n = self.axis_normal();
        let s = if self.flip { -1.0 } else { 1.0 };
        [(n.x * s) as f32, (n.y * s) as f32, (n.z * s) as f32, (-self.pos * s) as f32]
    }
}

impl App {
    /// The section plane for the frame, `None` when it is off.
    pub(super) fn view_clip(&self) -> Option<[f32; 4]> {
        self.section.on.then(|| self.section.plane())
    }

    fn model_box(&self) -> Option<(DVec3, DVec3)> {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        scene_bounds(&self.meshes, &mut lo, &mut hi);
        (lo.x <= hi.x).then(|| (lo.as_dvec3(), hi.as_dvec3()))
    }

    /// The range of the position along the axis, and the middle of it as the start.
    fn section_range(&mut self) {
        if let Some((lo, hi)) = self.model_box() {
            let n = self.section.axis_normal();
            let corners = [lo.x, hi.x].into_iter().flat_map(|x| {
                [lo.y, hi.y]
                    .into_iter()
                    .flat_map(move |y| [lo.z, hi.z].into_iter().map(move |z| DVec3::new(x, y, z)))
            });
            let (mut a, mut b) = (f64::MAX, f64::MIN);
            for c in corners {
                a = a.min(c.dot(n));
                b = b.max(c.dot(n));
            }
            self.section.lo = a;
            self.section.hi = b;
        }
    }

    pub(super) fn toggle_xray(&mut self) {
        self.xray = !self.xray;
        self.layers_changed = true;
        self.set_status(if self.xray { "X-ray view: every body is half transparent (Q turns it off)" } else { "Solid view" }, false);
    }

    pub(super) fn toggle_section(&mut self) {
        self.section.on = !self.section.on;
        self.section.by_tool = false;
        if self.section.on {
            self.section_range();
            if self.section.pos < self.section.lo || self.section.pos > self.section.hi || self.section.pos == 0.0 {
                self.section.pos = (self.section.lo + self.section.hi) / 2.0;
            }
            self.set_status("Section: choose the plane in the panel (X, Y, Z or a click on a flat face); Y turns it off", false);
        } else {
            self.section.pick = false;
        }
        self.pick_dirty = true;
        self.layers_changed = true;
    }

    /// The Shell tool shows the thickness of the walls: a section through the middle of the body, looking at the cut, unless the person has one already.
    pub(super) fn section_for_tool(&mut self, on: bool, centre: Option<DVec3>) {
        if on {
            if self.section.on || self.xray {
                return;
            }
            // the axis that points most at the camera: the cut is seen from the front, the half near the camera is cut away
            let fwd = (self.camera.target - self.camera.eye()).normalize_or_zero();
            let f = fwd.abs();
            let axis = if f.x >= f.y && f.x >= f.z {
                0
            } else if f.y >= f.z {
                1
            } else {
                2
            };
            self.section.axis = axis;
            self.section.flip = fwd[axis] < 0.0;
            self.section_range();
            self.section.pos = centre.map_or((self.section.lo + self.section.hi) / 2.0, |c| c[axis]);
            self.section.on = true;
            self.section.by_tool = true;
        } else if self.section.by_tool {
            self.section.on = false;
            self.section.by_tool = false;
        } else {
            return;
        }
        self.pick_dirty = true;
        self.layers_changed = true;
    }

    /// A click on a flat face while the section waits for a face: its plane is the section.
    pub(super) fn section_pick_click(&mut self, resp: &egui::Response, hovered_face: Option<(BodyId, u32)>) -> bool {
        if !(self.section.on && self.section.pick) || !resp.clicked_by(PointerButton::Primary) {
            return false;
        }
        let Some((b, f)) = hovered_face else { return true };
        let Some(meta) = self.entry(b).and_then(|m| m.faces.get(f as usize)).cloned() else {
            return true;
        };
        if meta.kind != FaceKind::Plane {
            self.set_status("The section takes the plane of a flat face; this face is curved", true);
            return true;
        }
        let n = meta.normal.normalize_or_zero();
        self.section.axis = 3;
        self.section.normal = n;
        self.section.pos = n.dot(meta.centroid.as_dvec3());
        // the part in front of the face is cut away: the body behind it stays
        self.section.flip = true;
        self.section.pick = false;
        self.section_range();
        self.pick_dirty = true;
        self.layers_changed = true;
        true
    }

    #[allow(deprecated)]
    pub(super) fn section_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        if !self.section.on {
            return;
        }
        let mut docks = std::mem::take(&mut self.docks);
        let mut s = self.section.clone();
        let mut close = false;
        let probe_w: std::cell::RefCell<Vec<(&'static str, Rect)>> = std::cell::RefCell::new(Vec::new());
        docks.panel(ctx, "section", Pos2::new(rect.min.x + 12.0, rect.max.y - 190.0), None, 260.0, &tr("Section"), false, |ui| {
            ui.horizontal(|ui| {
                for (i, n) in ["X", "Y", "Z"].into_iter().enumerate() {
                    let r = ui.selectable_label(s.axis == i, n);
                    probe_w.borrow_mut().push((["section_x", "section_y", "section_z"][i], r.rect));
                    if r.clicked() {
                        s.axis = i;
                    }
                }
                let r = ui.selectable_label(s.axis == 3 || s.pick, tr("A face"));
                probe_w.borrow_mut().push(("section_face", r.rect));
                if r.clicked() {
                    s.pick = !s.pick;
                }
            });
            if s.pick {
                ui.colored_label(crate::theme::rgb(255, 200, 100), tr("Click a flat face"));
            }
            ui.horizontal(|ui| {
                ui.label(tr("Position"));
                let r = ui.add(egui::DragValue::new(&mut s.pos).speed(0.5).suffix(" mm"));
                probe_w.borrow_mut().push(("section_pos", r.rect));
            });
            let (lo, hi) = (s.lo, s.hi.max(s.lo + 1e-6));
            ui.add(egui::Slider::new(&mut s.pos, lo..=hi).show_value(false));
            ui.horizontal(|ui| {
                let r = ui.checkbox(&mut s.flip, tr("The other side"));
                probe_w.borrow_mut().push(("section_flip", r.rect));
                if ui.button(tr("Turn off")).clicked() {
                    close = true;
                }
            });
            ui.weak(tr("Only the picture is cut; the model is not changed"));
        });
        self.docks = docks;
        for (n, r) in probe_w.into_inner() {
            self.probe_widget(n, r);
        }
        let axis_changed = s.axis != self.section.axis;
        let changed = s.axis != self.section.axis
            || s.pos != self.section.pos
            || s.flip != self.section.flip
            || s.pick != self.section.pick;
        self.section = s;
        if axis_changed {
            self.section_range();
            self.section.pos = (self.section.lo + self.section.hi) / 2.0;
        }
        if close {
            self.section.on = false;
            self.section.pick = false;
            self.section.by_tool = false;
        }
        if changed || close {
            self.pick_dirty = true;
            self.layers_changed = true;
        }
    }

    /// The text of the status line for the section: where the plane is.
    #[allow(dead_code)]
    pub(super) fn section_text(&self) -> String {
        trf("Section at {} mm", &[fmt_num(self.section.pos)])
    }
}
