// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Choosing the sketch plane with the mouse (a flat face, or the tangent plane of a curved face) and naming the plane
//! of a sketch in the view.

use super::*;
use crate::i18n::tr;

/// The plane that a click on the face under the cursor would choose.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct PlaneHover {
    pub body: BodyId,
    pub face: u32,
    pub point: DVec3,
    pub normal: DVec3,
    pub planar: bool,
}

/// "XY", "XZ", "YZ" (with the offset, "XY +20 mm") or "Face".
pub fn plane_label(plane: &PlaneDef, base_plane: &PlaneDef, offset: f64, from_face: bool) -> String {
    let name = if from_face {
        tr("Face")
    } else {
        tr(plane_name(base_plane))
    };
    if offset.abs() > 1e-9 {
        format!(
            "{name} {}{} mm",
            if offset > 0.0 { "+" } else { "-" },
            fmt_num(offset.abs())
        )
    } else {
        let _ = plane;
        name
    }
}

impl App {
    /// While a sketch plane is being chosen: the face under the cursor shows the plane it would give; a click takes it.
    pub(super) fn plane_pick_input(&mut self, resp: &egui::Response, pointer: Option<Pos2>, rect: Rect) {
        self.plane_hover = None;
        if self.quick_tool.is_some() {
            return self.quick_pick_input(resp, pointer, rect);
        }
        let (true, Some(p), Some((b, f))) = (resp.hovered(), pointer, self.hovered_face()) else {
            return;
        };
        let Some(meta) = self.entry(b).and_then(|m| m.faces.get(f as usize)).cloned() else {
            return;
        };
        let aspect = rect.width() / rect.height().max(1.0);
        let (o, d) = self.camera.ray(Self::ndc(p, rect), aspect);
        let hover = if meta.kind == FaceKind::Plane {
            PlaneHover {
                body: b,
                face: f,
                point: meta.centroid.as_dvec3(),
                normal: meta.normal,
                planar: true,
            }
        } else if let Some((pt, n)) = self.ray_face_hit(b, f, o, d) {
            PlaneHover {
                body: b,
                face: f,
                point: pt,
                normal: n,
                planar: false,
            }
        } else {
            return;
        };
        self.plane_hover = Some(hover);
        if resp.clicked_by(PointerButton::Primary) {
            if hover.planar {
                self.selection = vec![(b, f)];
            } else {
                self.tangent_plane = Some((b, hover.point, hover.normal));
            }
            self.plane_hover = None;
            self.enter_sketch_on(PlaneDef::base());
        }
    }

    /// The name of the sketch plane at the edge of its sheet, and the preview of the plane under the cursor.
    pub(super) fn paint_plane_labels(&self, ui: &egui::Ui, rect: Rect) {
        let painter = ui.painter().with_clip_rect(rect);
        let col = crate::theme::rgb(150, 200, 255);
        let font = egui::FontId::proportional(14.0);
        let sheet = |centre: DVec3, u: DVec3, v: DVec3, half: f64| -> Option<[Pos2; 4]> {
            let c = |a: f64, b: f64| self.project((centre + u * a * half + v * b * half).as_vec3(), rect);
            Some([c(-1.0, -1.0)?, c(1.0, -1.0)?, c(1.0, 1.0)?, c(-1.0, 1.0)?])
        };
        let label_at = |quad: &[Pos2; 4], text: String| {
            // at the lowest edge of the sheet on the screen, inside it, so it does not hide what is drawn
            let mut i = 0;
            for k in 1..4 {
                if quad[k].y > quad[i].y {
                    i = k;
                }
            }
            let a = quad[i];
            let b = quad[(i + 1) % 4];
            let c = quad[(i + 3) % 4];
            let along = if (b.y - a.y).abs() < (c.y - a.y).abs() {
                b
            } else {
                c
            };
            let mut at = a.lerp(along, 0.08) + Vec2::new(4.0, -22.0);
            // the label stays inside the view even when the sheet is larger than the window
            at.x =
                at.x.clamp(rect.min.x + 8.0, (rect.max.x - 90.0).max(rect.min.x + 8.0));
            at.y =
                at.y.clamp(rect.min.y + 8.0, (rect.max.y - 24.0).max(rect.min.y + 8.0));
            painter.text(at, egui::Align2::LEFT_TOP, text, font.clone(), col);
        };
        if let Some(sm) = &self.sk {
            let half = (self.camera.distance * 0.45) as f64;
            let c = sm.focus.as_dvec3();
            // the centre of the view, moved onto the plane
            let c = c - sm.plane.normal * (c - sm.plane.origin).dot(sm.plane.normal);
            if let Some(q) = sheet(c, sm.plane.x_dir, sm.plane.y_dir(), half) {
                label_at(
                    &q,
                    plane_label(&sm.plane, &sm.base_plane, sm.offset, sm.body.is_some()),
                );
            }
        }
        if let (true, Some(h)) = (self.plane_pick, self.plane_hover) {
            let pl = PlaneDef::from_face(h.point, h.normal);
            let half = (self.camera.distance * 0.12) as f64;
            if let Some(q) = sheet(h.point, pl.x_dir, pl.y_dir(), half) {
                painter.add(egui::Shape::convex_polygon(
                    q.to_vec(),
                    Color32::from_rgba_unmultiplied(110, 170, 255, 40),
                    Stroke::new(1.5, Color32::from_rgba_unmultiplied(110, 170, 255, 190)),
                ));
                if self.quick_tool.is_some() {
                    // the grid of the sketch that a click would start, and what to do at the cursor
                    let grid = Stroke::new(1.0, Color32::from_rgba_unmultiplied(110, 170, 255, 120));
                    for k in 1..8 {
                        let t = k as f32 / 8.0;
                        painter.line_segment([q[0].lerp(q[1], t), q[3].lerp(q[2], t)], grid);
                        painter.line_segment([q[0].lerp(q[3], t), q[1].lerp(q[2], t)], grid);
                    }
                    if let Some(c) = ui.input(|i| i.pointer.latest_pos()) {
                        painter.text(
                            c + Vec2::new(16.0, -22.0),
                            egui::Align2::LEFT_BOTTOM,
                            tr("Click on the face to draw on"),
                            egui::FontId::proportional(16.0),
                            crate::theme::rgb(255, 230, 140),
                        );
                    }
                } else {
                    label_at(&q, tr("Face"));
                }
            }
        }
    }
}

/// The sketch plane tangent to a curved face: its normal is the surface normal there and its origin the clicked point.
pub fn tangent_plane_def(point: DVec3, normal: DVec3) -> PlaneDef {
    PlaneDef {
        origin: point,
        ..PlaneDef::from_face(point, normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::pick::ray_mesh_face_hit;
    use rolidium_kernel::{FaceKind, Solid};

    #[test]
    fn the_tangent_plane_of_a_cylinder_starts_at_the_clicked_point_and_is_perpendicular_to_the_axis() {
        let c = Solid::cylinder(5.0, 20.0, DVec3::ZERO, DVec3::Z).unwrap();
        let side = c
            .faces()
            .find(|f| c.face_info(*f).unwrap().kind == FaceKind::Cylinder)
            .unwrap();
        let mesh = c.mesh(0.01, 0.1).unwrap();
        // a ray from the side at height 8 and 30 degrees round the axis
        let a = 30f32.to_radians();
        let target = glam::Vec3::new(5.0 * a.cos(), 5.0 * a.sin(), 8.0);
        let origin = target * glam::Vec3::new(4.0, 4.0, 1.0);
        let dir = (target - origin).normalize();
        let (pt, n) = ray_mesh_face_hit(&mesh, side.0, origin, dir).expect("the ray hits the cylinder");
        let plane = tangent_plane_def(pt, n);
        assert!((plane.origin - pt).length() < 1e-9, "origin at the clicked point");
        assert!((pt - target.as_dvec3()).length() < 0.02, "{pt:?}");
        assert!(
            plane.normal.z.abs() < 1e-3,
            "the normal is perpendicular to the axis: {:?}",
            plane.normal
        );
        assert!((plane.normal - DVec3::new(a.cos() as f64, a.sin() as f64, 0.0)).length() < 0.02);
        // the plane's own directions are perpendicular to its normal, so the sketch lies in the tangent plane
        assert!(plane.x_dir.dot(plane.normal).abs() < 1e-9 && plane.y_dir().dot(plane.normal).abs() < 1e-9);
    }
}
