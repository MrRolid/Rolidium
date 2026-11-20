// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Smooth display: the display mesh follows the zoom. When the view has been still for a moment and the meshes were made for a
//! much coarser (or much finer) pixel than the one now, new meshes are made in the background (`Work::Remesh`); the result
//! replaces the old ones when the model has not changed meanwhile. Only the display changes: the kernel, the measurements and the
//! STL export do not use these meshes.

use super::*;
use rolidium_kernel::VIEW_PIXEL_FRACTION;

/// At most this many triangles for one body, and for all bodies together (so that a big model does not slow the display down).
pub(super) const MAX_TRIANGLES_PER_BODY: usize = 1_500_000;
pub(super) const MAX_TRIANGLES_TOTAL: usize = 3_000_000;
/// The view must be still this long before the meshes are made again (inertia).
const QUIET_SECONDS: f32 = 0.35;
/// New meshes when the pixel is this much smaller than the one the meshes were made for (finer) ...
const FINER_FACTOR: f64 = 1.4;
/// ... or this much larger (coarser: saves triangles after zooming out).
const COARSER_FACTOR: f64 = 4.0;

#[derive(Default)]
pub(super) struct RemeshState {
    last_view: Option<(Vec3, f32, f32, f32)>,
    moved_at: Option<Instant>,
    ticket: u64,
    inflight: Option<(u64, u64, f64)>,
    /// The meshes (by address) and the pixel size they were made for.
    key: usize,
    pixel: f64,
}

impl App {
    /// The size of one screen pixel in millimetres at the point the camera looks at.
    pub(super) fn pixel_mm(&self) -> f64 {
        let h = (self.viewport_rect.height() * self.egui_ctx.pixels_per_point()).max(1.0) as f64;
        2.0 * self.camera.distance as f64 * (self.camera.fov_y as f64 * 0.5).tan() / h
    }

    /// The pixel size the present meshes were made for, from their deflection (the smallest over the bodies).
    fn built_pixel(&self) -> f64 {
        self.meshes
            .iter()
            .map(|m| m.mesh.deflection)
            .filter(|d| *d > 0.0)
            .fold(f64::INFINITY, f64::min)
            / VIEW_PIXEL_FRACTION
    }

    /// Once a frame: starts the remeshing when it is due.
    pub(super) fn remesh_tick(&mut self, ctx: &egui::Context) {
        if self.selftest.is_some() || self.bench.is_some() || self.meshes.is_empty() {
            return;
        }
        let v = (self.camera.target, self.camera.yaw, self.camera.pitch, self.camera.distance);
        if self.remesh.last_view != Some(v) {
            self.remesh.last_view = Some(v);
            self.remesh.moved_at = Some(Instant::now());
        }
        let key = Arc::as_ptr(&self.meshes) as usize;
        if key != self.remesh.key {
            self.remesh.key = key;
            self.remesh.pixel = self.built_pixel();
        }
        let need = self.pixel_mm();
        if !need.is_finite() || need <= 0.0 || !self.remesh.pixel.is_finite() {
            return;
        }
        let due = self.remesh.pixel > need * FINER_FACTOR || need > self.remesh.pixel * COARSER_FACTOR;
        if !due || self.remesh.inflight.is_some() || !self.idle() || self.pull.is_some() || self.view_anim.is_some() {
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
        let bodies: Vec<_> = self
            .doc
            .bodies()
            .iter()
            .filter(|b| b.visible)
            .map(|b| {
                let prev = self
                    .meshes
                    .iter()
                    .find(|m| m.id == b.id)
                    .map(|m| (m.mesh.deflection, m.mesh.triangle_count()));
                ((b.id, b.name.clone(), b.solid.clone(), b.color), prev)
            })
            .collect();
        if bodies.is_empty() {
            return;
        }
        let n = bodies.len().max(1);
        self.remesh.ticket += 1;
        let ticket = self.remesh.ticket;
        let sent = self.worker.send(Work::Remesh {
            ticket,
            bodies,
            pixel_mm: need,
            max_triangles: MAX_TRIANGLES_PER_BODY.min(MAX_TRIANGLES_TOTAL / n),
        });
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

    /// The new meshes arrived: they replace the old ones unless the model changed meanwhile.
    pub(super) fn remesh_done(&mut self, ticket: u64, r: Result<Vec<MeshEntry>, String>) {
        let Some((t, rev, need)) = self.remesh.inflight else { return };
        if t != ticket {
            return;
        }
        self.remesh.inflight = None;
        let Ok(entries) = r else { return };
        if rev != self.doc.revision() || !self.idle() {
            return;
        }
        let tris: usize = entries.iter().map(|m| m.mesh.triangle_count()).sum();
        self.set_meshes(entries);
        self.remesh.key = Arc::as_ptr(&self.meshes) as usize;
        self.remesh.pixel = need;
        log(&format!("display meshes made again for {need:.4} mm per pixel: {tris} triangles"));
    }

    /// For the self-test: the meshes made at once for the present zoom.
    pub(super) fn remesh_now(&mut self) {
        let need = self.pixel_mm();
        let entries: Vec<MeshEntry> = self
            .doc
            .bodies()
            .iter()
            .filter(|b| b.visible)
            .filter_map(|b| {
                let m = b.solid.view_mesh(need, MAX_TRIANGLES_PER_BODY, None).ok()?;
                let mut e = self.meshes.iter().find(|e| e.id == b.id)?.clone();
                e.mesh = m;
                Some(e)
            })
            .collect();
        self.set_meshes(entries);
    }
}
