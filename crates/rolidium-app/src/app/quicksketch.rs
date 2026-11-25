// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A quick sketch from the 3D view: choosing a drawing tool (circle, line, rectangle, arc, auxiliary axis) with no sketch open asks for the face to draw on
//! (a flat face of a solid, a flat wall of a mesh or the base plane); the click starts the sketch there and puts the first point of the shape where it
//! was, so that one click does what three steps did. The view is not turned (V does that).

use super::planepick::PlaneHover;
use super::*;
use crate::sketchmode::SkTool;

impl App {
    /// A drawing tool was chosen in the 3D view.
    pub(super) fn begin_quick_sketch(&mut self, t: SkTool) {
        if self.sk.is_some() {
            if let Some(sm) = &mut self.sk {
                sm.set_tool(t);
            }
            return;
        }
        if !self.idle() || self.pull.is_some() || self.move_ui.drag.is_some() {
            return;
        }
        self.tool = Tool::Select;
        self.clear_selection();
        self.quick_tool = Some(t);
        self.plane_pick = true;
        self.plane_hover = None;
        self.set_status(
            "Click on the face to draw on (a flat face, a wall of a mesh or the base plane). Esc cancels",
            false,
        );
    }

    pub(super) fn cancel_quick_sketch(&mut self) {
        self.quick_tool = None;
        self.plane_pick = false;
        self.plane_hover = None;
    }

    /// While the face is being chosen for a quick sketch: a flat face or the base plane under the cursor shows the sketch that a click would start.
    pub(super) fn quick_pick_input(&mut self, resp: &egui::Response, pointer: Option<Pos2>, rect: Rect) {
        let (true, Some(p)) = (resp.hovered(), pointer) else {
            return;
        };
        let aspect = rect.width() / rect.height().max(1.0);
        let (o, d) = self.camera.ray(Self::ndc(p, rect), aspect);
        let (o, d) = (o.as_dvec3(), d.as_dvec3());
        let hit_plane = |origin: DVec3, normal: DVec3| -> Option<DVec3> {
            let den = d.dot(normal);
            if den.abs() < 1e-9 {
                return None;
            }
            let t = (origin - o).dot(normal) / den;
            (t > 0.0).then(|| o + d * t)
        };
        let hover = match self.hovered_face() {
            Some((b, f)) => {
                let meta = self.entry(b).and_then(|m| m.faces.get(f as usize)).cloned();
                match meta {
                    Some(m) if m.kind == FaceKind::Plane => {
                        let n = m.normal.normalize_or_zero();
                        let at = hit_plane(m.origin, n).unwrap_or(m.centroid.as_dvec3());
                        Some(PlaneHover {
                            body: b,
                            face: f,
                            point: at,
                            normal: n,
                            planar: true,
                        })
                    }
                    // a curved face or the part of a mesh that is no wall: nothing to draw on
                    _ => None,
                }
            }
            None => hit_plane(DVec3::ZERO, DVec3::Z).map(|at| PlaneHover {
                body: 0,
                face: u32::MAX,
                point: at,
                normal: DVec3::Z,
                planar: true,
            }),
        };
        let Some(h) = hover else { return };
        self.plane_hover = Some(h);
        if resp.clicked_by(PointerButton::Primary) {
            self.plane_hover = None;
            if h.face == u32::MAX {
                self.selection.clear();
            } else {
                self.selection = vec![(h.body, h.face)];
            }
            self.enter_sketch_on(PlaneDef::base());
        }
    }
}
