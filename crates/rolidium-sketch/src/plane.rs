// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
use glam::{DVec2, DVec3};

/// Where a sketch lives in 3D: origin, outward normal and the direction of the sketch X axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaneDef {
    pub origin: DVec3,
    pub normal: DVec3,
    pub x_dir: DVec3,
}

impl PlaneDef {
    /// The world XY plane.
    pub fn base() -> Self {
        PlaneDef {
            origin: DVec3::ZERO,
            normal: DVec3::Z,
            x_dir: DVec3::X,
        }
    }

    /// The world XZ plane, seen from the front (x to the right, y of the sketch is world Z).
    pub fn xz() -> Self {
        PlaneDef {
            origin: DVec3::ZERO,
            normal: -DVec3::Y,
            x_dir: DVec3::X,
        }
    }

    /// The world YZ plane, seen from the right (x of the sketch is world Y, y is world Z).
    pub fn yz() -> Self {
        PlaneDef {
            origin: DVec3::ZERO,
            normal: DVec3::X,
            x_dir: DVec3::Y,
        }
    }

    /// Plane of a flat face. The sketch origin is the point of the plane nearest to the world origin and the
    /// axes follow the world axes projected into the plane (X if possible, otherwise Y).
    pub fn from_face(point_on_plane: DVec3, outward_normal: DVec3) -> Self {
        let n = outward_normal.normalize();
        let proj = |v: DVec3| v - n * v.dot(n);
        let mut x = proj(DVec3::X);
        if x.length() < 1e-6 {
            x = proj(DVec3::Y);
        }
        PlaneDef {
            origin: n * n.dot(point_on_plane),
            normal: n,
            x_dir: x.normalize(),
        }
    }

    pub fn y_dir(&self) -> DVec3 {
        self.normal.cross(self.x_dir).normalize()
    }

    pub fn to_3d(&self, p: DVec2) -> DVec3 {
        self.origin + self.x_dir * p.x + self.y_dir() * p.y
    }

    pub fn to_2d(&self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.x_dir), d.dot(self.y_dir()))
    }

    /// Intersection of a ray with the plane (in sketch coordinates), if it hits in front of the origin.
    pub fn ray_hit(&self, origin: DVec3, dir: DVec3) -> Option<(DVec2, f64)> {
        let denom = dir.dot(self.normal);
        if denom.abs() < 1e-9 {
            return None;
        }
        let t = (self.origin - origin).dot(self.normal) / denom;
        if t < 0.0 {
            return None;
        }
        Some((self.to_2d(origin + dir * t), t))
    }

    pub fn same_plane(&self, other: &PlaneDef) -> bool {
        self.normal.dot(other.normal) > 1.0 - 1e-9
            && (self.origin - other.origin).dot(self.normal).abs() < 1e-6
    }
}
