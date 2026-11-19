// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Orbit camera for a Z-up world. Pure maths, no GPU.

use glam::{Mat4, Vec2, Vec3, Vec4};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub target: Vec3,
    /// Rotation about Z, radians.
    pub yaw: f32,
    /// Elevation above the XY plane, radians, clamped to just under ±90°.
    pub pitch: f32,
    pub distance: f32,
    pub fov_y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandardView {
    Top,
    Bottom,
    Front,
    Back,
    Left,
    Right,
    Isometric,
}

/// Right-handed view matrix (camera looks down -Z).
fn look_at_rh(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
    let f = (center - eye).normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);
    Mat4::from_cols(
        Vec4::new(s.x, u.x, -f.x, 0.0),
        Vec4::new(s.y, u.y, -f.y, 0.0),
        Vec4::new(s.z, u.z, -f.z, 0.0),
        Vec4::new(-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0),
    )
}

/// Right-handed perspective projection with 0..1 depth (wgpu convention).
fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    Mat4::from_cols(
        Vec4::new(f / aspect, 0.0, 0.0, 0.0),
        Vec4::new(0.0, f, 0.0, 0.0),
        Vec4::new(0.0, 0.0, far / (near - far), -1.0),
        Vec4::new(0.0, 0.0, near * far / (near - far), 0.0),
    )
}

const MAX_PITCH: f32 = 1.5533; // 89 degrees

impl Default for Camera {
    fn default() -> Self {
        let mut c = Camera {
            target: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            distance: 100.0,
            fov_y: 0.8,
        };
        c.set_view(StandardView::Isometric);
        c
    }
}

/// Minor grid spacing (mm) for a camera distance; shared by the grid shader and snapping.
pub fn grid_step(distance: f32) -> f32 {
    10f32.powf((distance / 15.0).max(1e-3).log10().round())
}

/// Minor and major grid spacing (mm) for a user step and the current zoom (`px_per_mm` on screen).
///
/// The user step is used as long as its lines are at least `MIN_MINOR_PX` apart; when zoomed out
/// the spacing grows in 1-2-5 steps so the grid never turns into a solid area. The major lines
/// are every fifth or tenth minor line.
pub fn grid_levels(step: f32, px_per_mm: f32) -> (f32, f32) {
    const MIN_MINOR_PX: f32 = 14.0;
    let step = if step.is_finite() && step > 0.0 { step } else { 1.0 };
    let ppm = px_per_mm.max(1e-6);
    let mut minor = step;
    let mut i = 0;
    while minor * ppm < MIN_MINOR_PX && i < 64 {
        minor *= [2.0, 2.5, 2.0][i % 3];
        i += 1;
    }
    // Major lines every 10th line, except where that would not be a round number (2 -> 5th, 2.5 -> 4th).
    let mant = minor / 10f32.powf(minor.log10().floor());
    let every = if (mant - 2.0).abs() < 0.01 {
        5.0
    } else if (mant - 2.5).abs() < 0.01 {
        4.0
    } else {
        10.0
    };
    (minor, minor * every)
}

impl Camera {
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + self.distance * Vec3::new(cp * cy, cp * sy, sp)
    }

    pub fn set_view(&mut self, v: StandardView) {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
        let (yaw, pitch) = match v {
            StandardView::Top => (-FRAC_PI_2, MAX_PITCH),
            StandardView::Bottom => (-FRAC_PI_2, -MAX_PITCH),
            StandardView::Front => (-FRAC_PI_2, 0.0),
            StandardView::Back => (FRAC_PI_2, 0.0),
            StandardView::Left => (PI, 0.0),
            StandardView::Right => (0.0, 0.0),
            StandardView::Isometric => (-FRAC_PI_4 * 1.5, 0.6155),
        };
        self.yaw = yaw;
        self.pitch = pitch;
    }

    pub fn near_far(&self) -> (f32, f32) {
        ((self.distance * 0.02).max(0.01), self.distance * 200.0 + 1000.0)
    }

    pub fn view(&self) -> Mat4 {
        look_at_rh(self.eye(), self.target, Vec3::Z)
    }

    pub fn projection(&self, aspect: f32) -> Mat4 {
        let (n, f) = self.near_far();
        perspective_rh(self.fov_y, aspect.max(1e-3), n, f)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * self.view()
    }

    /// Rotates by a mouse drag in pixels.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.008;
        self.pitch = (self.pitch + dy * 0.008).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Pans by a mouse drag in pixels (the point under the cursor follows it).
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_height: f32) {
        let per_px = 2.0 * self.distance * (self.fov_y * 0.5).tan() / viewport_height.max(1.0);
        let v = self.view().inverse();
        let right = v.x_axis.truncate();
        let up = v.y_axis.truncate();
        self.target += (-right * dx + up * dy) * per_px;
    }

    /// Ray through a point in normalised device coordinates (x, y in -1..1, y up).
    pub fn ray(&self, ndc: Vec2, aspect: f32) -> (Vec3, Vec3) {
        let inv = self.view_proj(aspect).inverse();
        let a = inv * Vec4::new(ndc.x, ndc.y, 0.0, 1.0);
        let b = inv * Vec4::new(ndc.x, ndc.y, 1.0, 1.0);
        let (a, b) = (a.truncate() / a.w, b.truncate() / b.w);
        (a, (b - a).normalize())
    }

    fn plane_point(&self, ndc: Vec2, aspect: f32) -> Vec3 {
        let (o, d) = self.ray(ndc, aspect);
        let n = (self.target - self.eye()).normalize();
        let t = (self.target - o).dot(n) / d.dot(n);
        o + d * t
    }

    /// Zooms by `factor` (>1 zooms in) keeping the point under the cursor fixed.
    pub fn zoom_at(&mut self, ndc: Vec2, aspect: f32, factor: f32) {
        let before = self.plane_point(ndc, aspect);
        self.distance = (self.distance / factor.max(1e-3)).clamp(0.05, 1.0e6);
        let after = self.plane_point(ndc, aspect);
        self.target += before - after;
    }

    /// Frames a bounding box.
    pub fn fit(&mut self, min: Vec3, max: Vec3) {
        self.target = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(1.0);
        self.distance = radius / (self.fov_y * 0.5).sin() * 1.1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_point_under_cursor() {
        let mut c = Camera::default();
        let ndc = Vec2::new(0.4, -0.3);
        let (o, d) = c.ray(ndc, 1.5);
        let p = o + d * 80.0;
        let before = c.view_proj(1.5).project_point3(p);
        c.zoom_at(ndc, 1.5, 1.7);
        let after = c.view_proj(1.5).project_point3(c.plane_point(ndc, 1.5));
        // The point on the target plane under the cursor stays under the cursor.
        assert!((after.truncate() - ndc).length() < 1e-4);
        let _ = before;
    }

    #[test]
    fn pitch_is_clamped() {
        let mut c = Camera::default();
        c.orbit(0.0, 1.0e6);
        assert!(c.pitch <= MAX_PITCH);
        c.orbit(0.0, -1.0e6);
        assert!(c.pitch >= -MAX_PITCH);
    }

    #[test]
    fn top_view_looks_down() {
        let mut c = Camera::default();
        c.set_view(StandardView::Top);
        assert!(c.eye().z > c.distance * 0.99);
    }
}

#[cfg(test)]
mod grid_tests {
    use super::grid_levels;

    #[test]
    fn user_step_is_kept_when_lines_are_far_apart_enough() {
        assert_eq!(grid_levels(1.0, 20.0), (1.0, 10.0));
        let (m, _) = grid_levels(0.25, 100.0);
        assert!((m - 0.25).abs() < 1e-6);
    }

    #[test]
    fn fine_step_never_becomes_a_solid_area_when_zoomed_out() {
        for step in [0.1_f32, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0] {
            for ppm in [0.01_f32, 0.1, 1.0, 5.0, 50.0] {
                let (minor, major) = grid_levels(step, ppm);
                assert!(minor * ppm >= 14.0 - 1e-3, "{step} {ppm}: {minor}");
                assert!(major > minor);
                assert!(minor >= step);
            }
        }
    }
}
