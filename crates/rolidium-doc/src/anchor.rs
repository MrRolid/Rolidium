// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A sketch laid on the end of a curve remembers which end it belongs to and how its plane stands there. When the curve changes (the pitch or the
//! turns of a helix, the points of a line), the sketch moves with the end, and what was made from it (a sweep link) is calculated again.

use super::*;

/// Which end of a curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveEnd {
    Start,
    End,
}

/// How the plane of a sketch stands at the end of a curve.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaneKind {
    /// Through the axis of a helix and the point (the plane of the profile of a thread).
    AxisPlane,
    /// Square to the tangent of the curve at the end.
    NormalToTangent,
    /// Square to the axis of a helix.
    NormalToAxis,
    /// Parallel to the base planes through the point.
    ParallelXY,
    ParallelXZ,
    ParallelYZ,
    /// A plane turned freely: the direction stays, the point follows the end.
    Free {
        normal: DVec3,
        x_dir: DVec3,
    },
}

impl PlaneKind {
    /// The name of the proposal (for the status line).
    pub fn name(&self) -> &'static str {
        match self {
            PlaneKind::AxisPlane => "Plane through the axis of the helix and the point",
            PlaneKind::NormalToTangent => "Plane square to the curve",
            PlaneKind::NormalToAxis => "Plane square to the axis of the helix",
            PlaneKind::ParallelXY => "Plane parallel to XY",
            PlaneKind::ParallelXZ => "Plane parallel to XZ",
            PlaneKind::ParallelYZ => "Plane parallel to YZ",
            PlaneKind::Free { .. } => "Free plane",
        }
    }
}

/// The sketch belongs to one end of a curve.
#[derive(Debug, Clone, PartialEq)]
pub struct SketchAnchor {
    pub curve: CurveId,
    pub end: CurveEnd,
    pub plane: PlaneKind,
}

/// The point at an end of a curve, the direction the curve has there, and the axis of a helix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EndFrame {
    pub point: DVec3,
    pub tangent: DVec3,
    /// A helix: a point of its axis and the direction of it.
    pub axis: Option<(DVec3, DVec3)>,
}

impl EndFrame {
    /// The frame of the curve `def` at `end`.
    pub fn of(def: &CurveDef, end: CurveEnd) -> Result<EndFrame, KernelError> {
        let path = def.path()?;
        let (point, tangent) = match end {
            CurveEnd::Start => (path.start, path.start_tangent),
            CurveEnd::End => (path.end, path.end_tangent),
        };
        let axis = match def {
            CurveDef::Helix(h) if h.axis.length() > 1e-9 => Some((h.origin, h.axis.normalize())),
            _ => None,
        };
        Ok(EndFrame {
            point,
            tangent: tangent.normalize_or_zero(),
            axis,
        })
    }

    /// The directions from the point to the axis, if the point is not on it.
    fn radial(&self) -> Option<DVec3> {
        let (o, d) = self.axis?;
        let v = self.point - o;
        let r = v - d * v.dot(d);
        (r.length() > 1e-9).then(|| r.normalize())
    }

    /// The kinds of plane that can be proposed here, in the order they are offered: for the end of a helix the plane through its axis, the plane
    /// square to the curve and the plane square to the axis, for another curve the plane square to it; then always the planes parallel to
    /// XY, XZ and YZ, and last the free plane (given by `free`).
    pub fn proposals(&self, free: PlaneDef) -> Vec<PlaneKind> {
        let mut out = Vec::new();
        if self.axis.is_some() && self.radial().is_some() {
            out.push(PlaneKind::AxisPlane);
        }
        if self.tangent.length() > 0.5 {
            out.push(PlaneKind::NormalToTangent);
        }
        if self.axis.is_some() {
            out.push(PlaneKind::NormalToAxis);
        }
        out.push(PlaneKind::ParallelXY);
        out.push(PlaneKind::ParallelXZ);
        out.push(PlaneKind::ParallelYZ);
        out.push(PlaneKind::Free {
            normal: free.normal,
            x_dir: free.x_dir,
        });
        out
    }

    /// The plane of `kind` at this end (`None` when it does not exist here, for example the plane through the axis for a point on the axis).
    pub fn plane(&self, kind: &PlaneKind) -> Option<PlaneDef> {
        let p = self.point;
        let along = |n: DVec3, x: Option<DVec3>| {
            let n = n.normalize_or_zero();
            if n.length() < 0.5 {
                return None;
            }
            let x = x
                .map(|x| x - n * x.dot(n))
                .filter(|x| x.length() > 1e-9)
                .or_else(|| {
                    let c = n.cross(DVec3::Z);
                    (c.length() > 1e-6).then_some(c)
                })
                .unwrap_or(DVec3::X);
            Some(PlaneDef {
                origin: p,
                normal: n,
                x_dir: x.normalize(),
            })
        };
        match kind {
            PlaneKind::AxisPlane => {
                let (_, d) = self.axis?;
                let r = self.radial()?;
                along(d.cross(r), Some(r))
            }
            PlaneKind::NormalToTangent => along(self.tangent, None),
            PlaneKind::NormalToAxis => {
                let (_, d) = self.axis?;
                along(d, self.radial())
            }
            PlaneKind::ParallelXY => Some(PlaneDef {
                origin: p,
                ..PlaneDef::base()
            }),
            PlaneKind::ParallelXZ => Some(PlaneDef {
                origin: p,
                ..PlaneDef::xz()
            }),
            PlaneKind::ParallelYZ => Some(PlaneDef {
                origin: p,
                ..PlaneDef::yz()
            }),
            PlaneKind::Free { normal, x_dir } => along(*normal, Some(*x_dir)),
        }
    }
}

/// The plane `plane` turned about the world axis `axis` (through its origin) by `deg` degrees.
pub fn turned(plane: &PlaneDef, axis: DVec3, deg: f64) -> PlaneDef {
    let q = glam::DQuat::from_axis_angle(axis.normalize_or_zero(), deg.to_radians());
    PlaneDef {
        origin: plane.origin,
        normal: (q * plane.normal).normalize(),
        x_dir: (q * plane.x_dir).normalize(),
    }
}

/// `deg` rounded to a multiple of `step` degrees (a drag turns a plane in steps).
pub fn in_steps(deg: f64, step: f64) -> f64 {
    if step <= 0.0 {
        deg
    } else {
        (deg / step).round() * step
    }
}

/// The angles (degrees, 0 to 90) between a plane and the planes XY, XZ and YZ.
pub fn angles_to_base(plane: &PlaneDef) -> [f64; 3] {
    let n = plane.normal.normalize();
    [n.z.abs(), n.y.abs(), n.x.abs()].map(|c| c.clamp(0.0, 1.0).acos().to_degrees())
}

/// Turns `plane` so that its angle to the base plane `which` (0 = XY, 1 = XZ, 2 = YZ) is `deg`, about the line where the plane meets that base
/// plane (the other angles follow).
pub fn with_angle_to_base(plane: &PlaneDef, which: usize, deg: f64) -> PlaneDef {
    let base = [DVec3::Z, DVec3::Y, DVec3::X][which.min(2)];
    let n = plane.normal.normalize();
    // the normal on the side of the base normal: the angle between the planes is the angle between these normals
    let side = if n.dot(base) < 0.0 { -n } else { n };
    let now = side.dot(base).clamp(-1.0, 1.0).acos();
    let axis = base.cross(side);
    let axis = if axis.length() < 1e-9 { plane.x_dir } else { axis };
    turned(plane, axis, deg - now.to_degrees())
}

/// Puts the plane of every anchored sketch where its curve end is now (the sketch moves with the end).
pub(crate) fn refresh(st: &mut State) {
    for i in 0..st.sketches.len() {
        let Some(a) = st.sketches[i].anchor.clone() else {
            continue;
        };
        let Some(def) = st.curves.iter().find(|c| c.id == a.curve).map(|c| c.def.clone()) else {
            continue;
        };
        let Ok(frame) = EndFrame::of(&def, a.end) else {
            continue;
        };
        if let Some(plane) = frame.plane(&a.plane) {
            if plane != st.sketches[i].plane {
                st.sketches[i].plane = plane;
            }
        }
    }
}
