// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A hole or a pin as a feature of a body: the round faces that belong together (the cylinder of a hole and the cylinders and cones that share its
//! axis: a counterbore, a countersink, a chamfer at the rim) are read from the body and a *tool* is made of them, a solid of the same shape that
//! fills the hole (or is the pin). The tool is what a pattern or a mirror repeats: cut out of the body again at every place for a hole, fused
//! into the body for a pin. A feature that cannot be read this way (a hole with a thread, a slot, a hole whose faces are not whole cylinders) is
//! refused with the reason.

use crate::{nat as native, DVec3, FaceId, FaceKind, KernelError, Result, Solid};

/// What the faces of a hole or a pin make.
#[derive(Debug, Clone)]
pub struct FeatureTool {
    /// The solid of the hole (or of the pin), a little longer at the end where the material is not (a hole), or where the body is (a pin), so that the
    /// boolean operation is clean.
    pub solid: Solid,
    /// A hole (material outside, cut out) or a pin (material inside, fused).
    pub is_hole: bool,
    pub axis: DVec3,
    /// A point of the axis at the lower end of the feature.
    pub base: DVec3,
    pub radius: f64,
    /// The length of the feature along its axis (without what the tool reaches out).
    pub length: f64,
    /// The faces of the body that make the feature.
    pub faces: Vec<FaceId>,
}

/// The flat face a hole or a pin stands in, and its straight edges: what the distances of the feature to the edges are measured to.
#[derive(Debug, Clone)]
pub struct FeatureRim {
    pub face: FaceId,
    /// Outward normal of the face.
    pub normal: DVec3,
    /// A point of the face.
    pub origin: DVec3,
    /// Where the axis of the feature meets the plane of the face.
    pub centre: DVec3,
    /// The straight edges of the face (pairs of end points).
    pub edges: Vec<(DVec3, DVec3)>,
}

/// How far the tool reaches out of a hole, or into the body for a pin (mm).
const OVERSHOOT: f64 = 0.5;

/// One round face of the feature with its extent along the axis.
#[derive(Debug, Clone, Copy)]
struct Piece {
    z_lo: f64,
    z_hi: f64,
    r_lo: f64,
    r_hi: f64,
}

impl Solid {
    /// A cone (or frustum) of the radii `r_base` and `r_top` and the length `height` along `axis` from `origin`.
    pub fn from_cone(r_base: f64, r_top: f64, height: f64, origin: DVec3, axis: DVec3) -> Result<Solid> {
        if axis.length() < 1e-9 {
            return Err(KernelError::Invalid(
                "the axis of the cone must not be zero".into(),
            ));
        }
        Solid::from_shape(native::make_cone(
            r_base,
            r_top,
            height,
            origin.to_array(),
            axis.normalize().to_array(),
        )?)
        .validated_body()
    }

    fn on_axis(&self, p: DVec3, base: DVec3, axis: DVec3) -> bool {
        let v = p - base;
        (v - axis * v.dot(axis)).length() < 1e-6 * (1.0 + v.length())
    }

    /// The piece of a face if it is a whole cylinder or a cone round the axis (`base`, `axis`); `None` otherwise.
    fn round_piece(&self, face: FaceId, base: DVec3, axis: DVec3, hole: bool) -> Result<Option<Piece>> {
        if let Some(c) = self.cylinder_info(face)? {
            if c.axis.cross(axis).length() > 1e-6 || !self.on_axis(c.base, base, axis) || c.is_hole != hole {
                return Ok(None);
            }
            let z0 = (c.base - base).dot(axis);
            let z1 = z0 + c.height * c.axis.dot(axis).signum();
            return Ok(Some(Piece {
                z_lo: z0.min(z1),
                z_hi: z0.max(z1),
                r_lo: c.radius,
                r_hi: c.radius,
            }));
        }
        if let Some(k) = self.cone_info(face)? {
            if k.axis.cross(axis).length() > 1e-6 || !self.on_axis(k.base, base, axis) {
                return Ok(None);
            }
            // the cone's own axis may point against `axis`: its lower end is then the upper one
            let along = k.axis.dot(axis) > 0.0;
            let z0 = (k.base - base).dot(axis);
            return Ok(Some(if along {
                Piece {
                    z_lo: z0,
                    z_hi: z0 + k.height,
                    r_lo: k.r_base,
                    r_hi: k.r_top,
                }
            } else {
                Piece {
                    z_lo: z0 - k.height,
                    z_hi: z0,
                    r_lo: k.r_top,
                    r_hi: k.r_base,
                }
            }));
        }
        Ok(None)
    }

    fn neighbours_of(&self, face: FaceId) -> Result<Vec<FaceId>> {
        let edges = self.face_edges(face)?;
        let mut out = Vec::new();
        for f in self.faces().filter(|f| *f != face) {
            if self.face_edges(f)?.iter().any(|e| edges.contains(e)) {
                out.push(f);
            }
        }
        Ok(out)
    }

    /// Reads the hole or the pin that the round face `face` belongs to and makes its tool (see the module).
    pub fn feature_tool(&self, face: FaceId) -> Result<FeatureTool> {
        self.feature_tool_with(face, OVERSHOOT)
    }

    /// The same with the tool reaching out `overshoot` mm (0: the tool is exactly the hole, or the pin).
    pub fn feature_tool_with(&self, face: FaceId, overshoot: f64) -> Result<FeatureTool> {
        let not = |why: &str| KernelError::Invalid(why.to_string());
        let c = self.cylinder_info(face)?.ok_or_else(|| {
            not("This is not a whole round hole or pin. Click the cylindrical face of a hole or of a pin, or repeat the whole body")
        })?;
        let axis = c.axis.normalize();
        let base = c.base;
        let hole = c.is_hole;
        let mut members: Vec<(FaceId, Piece)> = vec![(
            face,
            self.round_piece(face, base, axis, hole)?
                .ok_or_else(|| not("The round face could not be read"))?,
        )];
        // the faces round the same axis that touch the feature, directly or over a flat shoulder (a counterbore)
        let mut changed = true;
        while changed {
            changed = false;
            let current: Vec<FaceId> = members.iter().map(|m| m.0).collect();
            for m in &current {
                for n in self.neighbours_of(*m)? {
                    if members.iter().any(|x| x.0 == n) {
                        continue;
                    }
                    if let Some(p) = self.round_piece(n, base, axis, hole)? {
                        members.push((n, p));
                        changed = true;
                    } else if self.face_info(n)?.kind == FaceKind::Plane {
                        // a flat face square to the axis that touches two round faces of the feature is a shoulder, and what lies beyond it belongs
                        let fi = self.face_info(n)?;
                        if fi.normal.normalize().cross(axis).length() > 1e-6 {
                            continue;
                        }
                        for q in self.neighbours_of(n)? {
                            if members.iter().any(|x| x.0 == q) {
                                continue;
                            }
                            if let Some(p) = self.round_piece(q, base, axis, hole)? {
                                members.push((q, p));
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        let z_lo = members.iter().map(|m| m.1.z_lo).fold(f64::MAX, f64::min);
        let z_hi = members.iter().map(|m| m.1.z_hi).fold(f64::MIN, f64::max);
        if z_hi - z_lo <= 1e-6 || !(z_hi - z_lo).is_finite() {
            return Err(not("The feature has no length"));
        }
        // the ends: where the material is not (a hole) or is (a pin) just beyond the end, the tool reaches out by OVERSHOOT
        let beyond = |z: f64, dir: f64| base + axis * (z + dir * 0.01);
        let (mut ext_lo, mut ext_hi) = (false, false);
        for (z, dir, flag) in [(z_lo, -1.0, &mut ext_lo), (z_hi, 1.0, &mut ext_hi)] {
            let inside = self.contains_point(beyond(z, dir))?;
            *flag = if hole { !inside } else { inside };
        }
        let mut solid: Option<Solid> = None;
        for (_, p) in &members {
            let mut piece = *p;
            let mut z0 = piece.z_lo;
            let mut h = piece.z_hi - piece.z_lo;
            if h < 1e-9 {
                continue;
            }
            // an end piece reaches further: the radius at that end is kept (a cylinder) or continued along the cone
            let slope = (piece.r_hi - piece.r_lo) / h;
            if ext_lo && (piece.z_lo - z_lo).abs() < 1e-6 {
                z0 -= overshoot;
                h += overshoot;
                piece.r_lo -= slope * overshoot;
            }
            if ext_hi && (piece.z_hi - z_hi).abs() < 1e-6 {
                h += overshoot;
                piece.r_hi += slope * overshoot;
            }
            if piece.r_lo <= 1e-6 || piece.r_hi <= 1e-6 {
                return Err(not(
                    "The feature comes to a point (a drilled cone); repeat the whole body instead",
                ));
            }
            let origin = base + axis * z0;
            let s = if (piece.r_lo - piece.r_hi).abs() <= 1e-9 * piece.r_lo.max(piece.r_hi) {
                Solid::cylinder(piece.r_lo, h, origin, axis)?
            } else {
                Solid::from_shape(native::make_cone(
                    piece.r_lo,
                    piece.r_hi,
                    h,
                    origin.to_array(),
                    axis.to_array(),
                )?)
                .validated_body()?
            };
            solid = Some(match solid {
                None => s,
                Some(acc) => acc.boolean(&s, crate::BooleanOp::Fuse)?.solid,
            });
        }
        let solid = solid.ok_or_else(|| not("The feature could not be made into a tool"))?;
        Ok(FeatureTool {
            solid,
            is_hole: hole,
            axis,
            base: base + axis * z_lo,
            radius: c.radius,
            length: z_hi - z_lo,
            faces: members.iter().map(|m| m.0).collect(),
        })
    }

    /// The flat face (the biggest one square to the axis) next to the round faces of a hole or a pin, with its straight edges. `None` when there is none.
    pub fn feature_rim(&self, face: FaceId) -> Result<Option<FeatureRim>> {
        let ft = self.feature_tool_with(face, 0.0)?;
        let axis = ft.axis;
        let mut best: Option<(f64, FaceId, DVec3, DVec3)> = None;
        for m in &ft.faces {
            for n in self.neighbours_of(*m)? {
                if ft.faces.contains(&n) {
                    continue;
                }
                let fi = self.face_info(n)?;
                if fi.kind != FaceKind::Plane || fi.normal.normalize().cross(axis).length() > 1e-6 {
                    continue;
                }
                if best.as_ref().is_none_or(|b| fi.area > b.0) {
                    best = Some((fi.area, n, fi.normal.normalize(), fi.origin));
                }
            }
        }
        let Some((_, f, normal, origin)) = best else {
            return Ok(None);
        };
        let centre = ft.base + axis * ((origin - ft.base).dot(axis));
        let edges = self
            .face_edges(f)?
            .into_iter()
            .filter_map(|e| self.edge_info(e).ok())
            .filter(|i| i.kind == crate::EdgeKind::Line)
            .map(|i| (i.start, i.end))
            .collect();
        Ok(Some(FeatureRim {
            face: f,
            normal,
            origin,
            centre,
            edges,
        }))
    }

    /// The hole of the round face `face` filled (it is gone), or the pin taken off. A counterbore or a countersink goes with it (the whole feature).
    pub fn remove_feature(&self, face: FaceId) -> Result<crate::OpResult> {
        let exact = self.feature_tool_with(face, 0.0)?;
        self.boolean_checked(
            &exact.solid,
            if exact.is_hole {
                crate::BooleanOp::Fuse
            } else {
                crate::BooleanOp::Cut
            },
        )
    }

    /// The hole or the pin of the round face `face` moved by `delta`: the hole is filled and cut out again at the new place (the pin is taken off and
    /// fused on again). The flat part of `delta` along the axis changes the depth of a blind hole, and that is allowed.
    pub fn move_feature(&self, face: FaceId, delta: DVec3) -> Result<Solid> {
        let exact = self.feature_tool_with(face, 0.0)?;
        let over = self.feature_tool(face)?;
        let restored = self
            .boolean_checked(
                &exact.solid,
                if exact.is_hole {
                    crate::BooleanOp::Fuse
                } else {
                    crate::BooleanOp::Cut
                },
            )?
            .solid;
        let moved = over.solid.moved(delta, DVec3::ZERO, DVec3::Z, 0.0)?.solid;
        if exact.is_hole {
            let common = restored
                .boolean(&moved, crate::BooleanOp::Common)
                .ok()
                .and_then(|r| r.solid.volume().ok())
                .unwrap_or(0.0);
            if common < 1e-9 {
                return Err(KernelError::Invalid("The hole would lie outside the body".into()));
            }
            Ok(restored.boolean_checked(&moved, crate::BooleanOp::Cut)?.solid)
        } else {
            if restored.distance_to(&moved).map(|d| d > 1e-6).unwrap_or(false) {
                return Err(KernelError::Invalid("The pin would not touch the body".into()));
            }
            Ok(restored.boolean_checked(&moved, crate::BooleanOp::Fuse)?.solid)
        }
    }
}
