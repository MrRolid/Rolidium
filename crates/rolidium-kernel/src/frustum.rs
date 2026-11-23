// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A solid that is a cone, a frustum or a cylinder and nothing else (two flat circular ends and one conical or cylindrical side): the diameter of an
//! end and the position of an end can be changed, and the side follows. The solid is made again with the new numbers, so the faces are real cones
//! and cylinders. A cone or cylinder that is part of a bigger solid is not touched by this (the other operations treat it).

use crate::{nat as native, DVec3, FaceId, FaceKind, History, KernelError, OpResult, Result, Solid};

/// A solid made of a cone or a cylinder and its two ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrustumInfo {
    /// Unit axis from the first end to the second.
    pub axis: DVec3,
    /// The centre of the first end.
    pub base: DVec3,
    pub height: f64,
    pub r_base: f64,
    pub r_top: f64,
    pub cap_base: FaceId,
    pub cap_top: FaceId,
    pub side: FaceId,
}

impl FrustumInfo {
    /// Change of the radius per mm along the axis (the tangent of the half angle).
    pub fn slope(&self) -> f64 {
        (self.r_top - self.r_base) / self.height
    }
}

impl Solid {
    /// The numbers of the solid when it is a cone, frustum or cylinder with its two flat ends.
    pub fn frustum_info(&self) -> Option<FrustumInfo> {
        if self.face_count() != 3 {
            return None;
        }
        let side = self.faces().find(|f| {
            matches!(
                self.face_info(*f).map(|i| i.kind),
                Ok(FaceKind::Cone | FaceKind::Cylinder)
            )
        })?;
        let si = self.face_info(side).ok()?;
        let (axis, base, height, r_base, r_top) = if si.kind == FaceKind::Cone {
            let c = self.cone_info(side).ok()??;
            (c.axis, c.base, c.height, c.r_base, c.r_top)
        } else {
            let c = self.cylinder_info(side).ok()??;
            (c.axis, c.base, c.height, c.radius, c.radius)
        };
        let mut caps: Vec<(f64, FaceId)> = Vec::new();
        for f in self.faces().filter(|f| *f != side) {
            let i = self.face_info(f).ok()?;
            if i.kind != FaceKind::Plane
                || i.inner_wires != 0
                || i.normal.normalize().cross(axis).length() > 1e-6
            {
                return None;
            }
            caps.push(((i.origin - base).dot(axis), f));
        }
        if caps.len() != 2 {
            return None;
        }
        caps.sort_by(|a, b| a.0.total_cmp(&b.0));
        // the ends lie where the side ends, and are discs of the radii of the side there
        let tol = 1e-5 * (1.0 + height);
        if caps[0].0.abs() > tol || (caps[1].0 - height).abs() > tol {
            return None;
        }
        for (f, r) in [(caps[0].1, r_base), (caps[1].1, r_top)] {
            let area = self.face_info(f).ok()?.area;
            if (area - std::f64::consts::PI * r * r).abs() > 1e-4 * (1.0 + area) {
                return None;
            }
        }
        Some(FrustumInfo {
            axis,
            base,
            height,
            r_base,
            r_top,
            cap_base: caps[0].1,
            cap_top: caps[1].1,
            side,
        })
    }

    /// The solid with the numbers of `f` changed (a cone, or a cylinder when the radii are equal), and where its faces came from.
    fn frustum_rebuilt(
        &self,
        f: &FrustumInfo,
        base: DVec3,
        height: f64,
        r_base: f64,
        r_top: f64,
    ) -> Result<OpResult> {
        let min = 1e-6;
        if !(height.is_finite() && height > min) {
            return Err(KernelError::Invalid(
                "The solid would have no length left, so nothing was changed".into(),
            ));
        }
        if !(r_base.is_finite() && r_top.is_finite() && r_base > min && r_top > min) {
            return Err(KernelError::Invalid(
                "The cone would come to a point or beyond it with this size, so nothing was changed. Use a larger diameter or a shorter length".into(),
            ));
        }
        let solid = if (r_base - r_top).abs() <= 1e-12 * r_base.max(r_top) {
            Solid::cylinder(r_base, height, base, f.axis)?
        } else {
            Solid::from_shape(native::make_cone(
                r_base,
                r_top,
                height,
                base.to_array(),
                f.axis.to_array(),
            )?)
            .validated_body()?
        };
        // the new ends and the new side are the successors of the old ones
        let nf = solid
            .frustum_info()
            .ok_or_else(|| KernelError::Invalid("the cone could not be made".into()))?;
        let history = History {
            links: vec![
                (0, f.cap_base, nf.cap_base),
                (0, f.cap_top, nf.cap_top),
                (0, f.side, nf.side),
            ],
        };
        Ok(OpResult { solid, history })
    }

    /// Changes the diameter of one end of a cone, frustum or cylinder. The other end keeps its diameter and the side follows (the angle changes), or with
    /// `keep_angle` the angle of the cone stays and the other end follows. The length does not change.
    pub fn set_cap_diameter(&self, cap: FaceId, diameter: f64, keep_angle: bool) -> Result<OpResult> {
        if !(diameter.is_finite() && diameter > 1e-6) {
            return Err(KernelError::Invalid(
                "the diameter must be a positive number".into(),
            ));
        }
        let f = self.frustum_info().ok_or_else(|| {
            KernelError::Invalid(
                "This end belongs to a solid that is not a plain cone or cylinder, so its diameter cannot follow the side. Change the diameter of the hole or pin, or make the shape from a sketch".into(),
            )
        })?;
        let r = diameter / 2.0;
        let (r_base, r_top) = if cap == f.cap_base {
            (
                r,
                if keep_angle {
                    f.r_top + (r - f.r_base)
                } else {
                    f.r_top
                },
            )
        } else if cap == f.cap_top {
            (
                if keep_angle {
                    f.r_base + (r - f.r_top)
                } else {
                    f.r_base
                },
                r,
            )
        } else {
            return Err(KernelError::Invalid("this face is not an end of the cone".into()));
        };
        self.frustum_rebuilt(&f, f.base, f.height, r_base, r_top)
    }

    /// Moves an end of a cone, frustum or cylinder along the axis by `distance` (positive: outwards). The cone is extended or shortened with the angle it
    /// has: the radius at the moved end follows the side. The other end stays.
    pub fn pull_frustum_cap(&self, cap: FaceId, distance: f64) -> Result<OpResult> {
        let f = self
            .frustum_info()
            .ok_or_else(|| KernelError::Invalid("this solid is not a plain cone or cylinder".into()))?;
        let slope = f.slope();
        if cap == f.cap_top {
            self.frustum_rebuilt(
                &f,
                f.base,
                f.height + distance,
                f.r_base,
                f.r_top + slope * distance,
            )
        } else if cap == f.cap_base {
            self.frustum_rebuilt(
                &f,
                f.base - f.axis * distance,
                f.height + distance,
                f.r_base - slope * distance,
                f.r_top,
            )
        } else {
            Err(KernelError::Invalid("this face is not an end of the cone".into()))
        }
    }
}
