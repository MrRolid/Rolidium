// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Smooth display: the display mesh follows the zoom. When the view has been still for a moment and the meshes were made for a
//! much coarser (or much finer) pixel than the one now, new meshes are made in the background (`Work::Remesh`); the result
//! replaces the old ones when the model has not changed meanwhile. Only the display changes: the kernel, the measurements and the
//! STL export do not use these meshes.

use super::*;
use crate::worker::RemeshBody;
use rolidium_kernel::VIEW_PIXEL_FRACTION;

/// At most this many triangles for one body, and for all bodies together: the background work must stay within about a second and
/// a half for a part like a spring, and the display must stay fast for a big model.
pub(super) const MAX_TRIANGLES_PER_BODY: usize = 150_000;
pub(super) const MAX_TRIANGLES_TOTAL: usize = 600_000;
/// Even a small body in view gets this much (a hole in it must be round).
const MIN_TRIANGLES_PER_BODY: usize = 6_000;
/// The bodies made finer in one go.
const MAX_BODIES_PER_BATCH: usize = 48;
/// The view must be still this long before the meshes are made again (inertia).
const QUIET_SECONDS: f32 = 0.35;
/// New meshes when a face in view is this much coarser than the zoom asks for (finer) ...
const FINER_FACTOR: f64 = 1.4;
/// ... or when everything in view is this much finer than needed (coarser: saves triangles after zooming out).
const COARSER_FACTOR: f64 = 4.0;
/// Coarser meshes are only worth making for a mesh with more triangles than this.
const COARSER_MIN_TRIANGLES: usize = 40_000;

#[derive(Default)]
pub(super) struct RemeshState {
    last_view: Option<(Vec3, f32, f32, f32)>,
    moved_at: Option<Instant>,
    ticket: u64,
    inflight: Option<(u64, u64, f64)>,
}

/// The faces of `e` whose box is (at least partly) in view for the view-projection `m`.
pub(super) fn faces_in_view(e: &MeshEntry, m: &glam::Mat4) -> Vec<bool> {
    e.faces
        .iter()
        .map(|f| {
            let (lo, hi) = f.bounds;
            if lo.x > hi.x {
                return true; // no triangles known: leave it to the safe side
            }
            // outside when all eight corners are on the outer side of one of the planes of the view
            let corners: Vec<glam::Vec4> = (0..8)
                .map(|i| {
                    let p = Vec3::new(
                        if i & 1 == 0 { lo.x } else { hi.x },
                        if i & 2 == 0 { lo.y } else { hi.y },
                        if i & 4 == 0 { lo.z } else { hi.z },
                    );
                    *m * p.extend(1.0)
                })
                .collect();
            let all = |test: &dyn Fn(&glam::Vec4) -> bool| corners.iter().all(test);
            !(all(&|c| c.x > c.w)
                || all(&|c| c.x < -c.w)
                || all(&|c| c.y > c.w)
                || all(&|c| c.y < -c.w)
                || all(&|c| c.z > c.w)
                || all(&|c| c.w <= 0.0))
        })
        .collect()
}

impl App {
    /// The size of one screen pixel in millimetres at the point the camera looks at.
    pub(super) fn pixel_mm(&self) -> f64 {
        let h = (self.viewport_rect.height() * self.egui_ctx.pixels_per_point()).max(1.0) as f64;
        2.0 * self.camera.distance as f64 * (self.camera.fov_y as f64 * 0.5).tan() / h
    }

    fn view_matrix(&self) -> glam::Mat4 {
        let r = self.viewport_rect;
        self.camera.view_proj(r.width() / r.height().max(1.0))
    }

    /// The size of one screen pixel in millimetres at `p` (a perspective view: further away, a pixel is wider).
    fn pixel_mm_at(&self, p: Vec3) -> f64 {
        let h = (self.viewport_rect.height() * self.egui_ctx.pixels_per_point()).max(1.0) as f64;
        let d = (p - self.camera.eye()).length().max(self.camera.distance * 0.02) as f64;
        2.0 * d * (self.camera.fov_y as f64 * 0.5).tan() / h
    }

    /// Whether the body of `e` needs a new mesh; the faces in view are returned with the pixel size of the body: the one at its
    /// nearest face in view (a body far behind the one looked at needs no fine mesh for the zoom of the centre), and its size
    /// on the screen in pixels (the weight for the share of the triangle budget).
    fn needs_remesh(&self, e: &MeshEntry, vp: &glam::Mat4) -> Option<(Vec<bool>, f64, f32)> {
        let fine = faces_in_view(e, vp);
        if !fine.iter().any(|f| *f) {
            return None;
        }
        // the nearest point of the faces in view and the box of them
        let eye = self.camera.eye();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let mut nearest = f32::MAX;
        for (f, _) in e.faces.iter().zip(&fine).filter(|(_, v)| **v) {
            if f.bounds.0.x > f.bounds.1.x {
                continue;
            }
            lo = lo.min(f.bounds.0);
            hi = hi.max(f.bounds.1);
            nearest = nearest.min(eye.clamp(f.bounds.0, f.bounds.1).distance(eye));
        }
        if nearest == f32::MAX {
            return None;
        }
        let need = self.pixel_mm_at(eye + (lo + hi - eye * 2.0).normalize_or_zero() * nearest.max(1e-3));
        // what the kernel will really ask for at this pixel (it limits the deflection at both ends: asking more would never be met)
        let want = self
            .doc
            .body(e.id)
            .map_or(need * VIEW_PIXEL_FRACTION, |b| b.solid.view_deflection(need));
        let asked = |i: usize| {
            e.mesh
                .face_asked
                .get(i)
                .or_else(|| e.mesh.face_deflection.get(i))
                .copied()
                .filter(|d| *d > 0.0)
                .unwrap_or(e.mesh.deflection)
        };
        let finer = fine
            .iter()
            .enumerate()
            .any(|(i, f)| *f && asked(i) > want * FINER_FACTOR);
        let coarser = e.mesh.triangle_count() > COARSER_MIN_TRIANGLES
            && fine
                .iter()
                .enumerate()
                .filter(|(_, f)| **f)
                .all(|(i, _)| asked(i) * COARSER_FACTOR < want);
        // the size on the screen: the box of the faces in view over the pixel
        let pixels = ((hi - lo).length() as f64 / need).clamp(8.0, 4000.0) as f32;
        (finer || coarser).then_some((fine, need, pixels))
    }

    /// The bodies that need a finer (or coarser) mesh for the present view, the biggest on the screen first: each with its own
    /// pixel size and its share of the triangle budget. The second value is false when nothing is due.
    pub(super) fn remesh_batch(&self) -> (Vec<RemeshBody>, bool) {
        let vp = self.view_matrix();
        let mut due: Vec<(&MeshEntry, Vec<bool>, f64, f32)> = self
            .meshes
            .iter()
            .filter(|e| self.doc.body(e.id).is_some_and(|b| b.visible))
            .filter_map(|e| self.needs_remesh(e, &vp).map(|(f, n, w)| (e, f, n, w)))
            .collect();
        let any = !due.is_empty();
        // the bodies that are biggest on the screen first, a batch at a time (the rest follows when this one is done)
        due.sort_by(|a, b| b.3.total_cmp(&a.3));
        due.truncate(MAX_BODIES_PER_BATCH);
        // the budget of triangles goes to the bodies by their size on the screen
        let weights: f32 = due.iter().map(|d| d.3).sum::<f32>().max(1.0);
        let bodies: Vec<RemeshBody> = due
            .into_iter()
            .filter_map(|(e, fine, need_b, w)| {
                let b = self.doc.body(e.id)?;
                let share = (MAX_TRIANGLES_TOTAL as f32 * w / weights) as usize;
                Some(RemeshBody {
                    body: (b.id, b.name.clone(), b.solid.clone(), b.color),
                    base: e.mesh.clone(),
                    fine,
                    pixel_mm: need_b,
                    max_triangles: share.clamp(MIN_TRIANGLES_PER_BODY, MAX_TRIANGLES_PER_BODY),
                })
            })
            .collect();
        (bodies, any)
    }

    /// Once a frame: starts the remeshing when it is due.
    pub(super) fn remesh_tick(&mut self, ctx: &egui::Context) {
        if self.selftest.is_some() || self.bench.is_some() || self.meshes.is_empty() {
            return;
        }
        let v = (
            self.camera.target,
            self.camera.yaw,
            self.camera.pitch,
            self.camera.distance,
        );
        if self.remesh.last_view != Some(v) {
            self.remesh.last_view = Some(v);
            self.remesh.moved_at = Some(Instant::now());
        }
        let need = self.pixel_mm();
        if !need.is_finite() || need <= 0.0 {
            return;
        }
        if self.remesh.inflight.is_some() || !self.idle() || self.pull.is_some() || self.view_anim.is_some() {
            return;
        }
        let (bodies, any) = self.remesh_batch();
        if !any {
            return;
        }
        let quiet = self
            .remesh
            .moved_at
            .map_or(f32::MAX, |t| t.elapsed().as_secs_f32());
        if quiet < QUIET_SECONDS || ctx.input(|i| i.pointer.any_down()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
            return;
        }
        if bodies.is_empty() {
            return;
        }
        self.remesh.ticket += 1;
        let ticket = self.remesh.ticket;
        let sent = self.worker.send(Work::Remesh { ticket, bodies });
        if sent {
            self.remesh.inflight = Some((ticket, self.doc.revision(), need));
        }
    }

    /// An operation is about to start: display meshes for the zoom that are being made are stopped, the operation goes first.
    pub(super) fn cancel_remesh(&mut self) {
        if self.remesh.inflight.take().is_some() {
            rolidium_kernel::request_cancel();
        }
    }

    /// The new meshes arrived: they replace the old ones of the same bodies unless the model changed meanwhile.
    pub(super) fn remesh_done(&mut self, ticket: u64, r: Result<Vec<MeshEntry>, String>) {
        let Some((t, rev, need)) = self.remesh.inflight else {
            return;
        };
        if t != ticket {
            return;
        }
        self.remesh.inflight = None;
        let Ok(entries) = r else { return };
        if rev != self.doc.revision() || !self.idle() {
            return;
        }
        let tris: usize = entries.iter().map(|m| m.mesh.triangle_count()).sum();
        let mut all: Vec<MeshEntry> = self.meshes.iter().cloned().collect();
        for e in entries {
            match all.iter_mut().find(|o| o.id == e.id) {
                Some(slot) => *slot = e,
                None => all.push(e),
            }
        }
        self.set_meshes(all);
        log(&format!(
            "display meshes made again for {need:.4} mm per pixel: {tris} triangles"
        ));
    }

    /// For the self-test: the meshes made at once for the present zoom, by the same choice of bodies and budget as the program uses.
    pub(super) fn remesh_now(&mut self) {
        // a pass makes the faces at most a few times finer (the kernel limits it), so a deep zoom is a few passes, as in the program
        for _ in 0..10 {
            let (bodies, _) = self.remesh_batch();
            if bodies.is_empty() {
                break;
            }
            let made = crate::worker::remesh_entries(&bodies);
            let mut all: Vec<MeshEntry> = self.meshes.iter().cloned().collect();
            for e in made.into_iter().flatten() {
                if let Some(slot) = all.iter_mut().find(|o| o.id == e.id) {
                    *slot = e;
                }
            }
            self.set_meshes(all);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rolidium_kernel::{DVec3, FaceKind};

    fn face(lo: Vec3, hi: Vec3) -> crate::worker::FaceMeta {
        crate::worker::FaceMeta {
            kind: FaceKind::Plane,
            normal: DVec3::Z,
            origin: DVec3::ZERO,
            area: 1.0,
            centroid: (lo + hi) * 0.5,
            bounds: (lo, hi),
            cyl_diameter: None,
            disc: None,
            cap: None,
        }
    }

    #[test]
    fn faces_outside_the_view_are_not_made_finer() {
        // a camera looking at the origin from 100 mm above; the faces: one in the middle, one far to the side, one behind the camera
        let cam = rolidium_render::Camera {
            target: Vec3::ZERO,
            distance: 100.0,
            pitch: 1.5,
            ..Default::default()
        };
        let vp = cam.view_proj(1.5);
        let e = MeshEntry {
            id: 1,
            name: String::new(),
            mesh: Default::default(),
            faces: std::sync::Arc::new(vec![
                face(Vec3::splat(-5.0), Vec3::splat(5.0)),
                face(Vec3::new(500.0, 0.0, 0.0), Vec3::new(510.0, 5.0, 5.0)),
                face(Vec3::new(0.0, 0.0, 200.0), Vec3::new(5.0, 5.0, 210.0)),
                // a big face that only partly overlaps the view still counts
                face(Vec3::new(-1000.0, -2.0, -2.0), Vec3::new(1000.0, 2.0, 2.0)),
            ]),
            color: [1.0; 3],
            stamp: 0,
        };
        assert_eq!(faces_in_view(&e, &vp), vec![true, false, false, true]);
    }
}
