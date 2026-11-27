// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Modelling operations on solids. Every operation validates its result.

use crate::{
    finish, nat as native, BooleanOp, DVec3, EdgeId, EdgeKind, FaceId, FaceKind, History, KernelError,
    OpResult, Result, Solid,
};

/// What `Solid::from_surfaces` needs: shared points, the analytic surfaces, the edges (lines, circles, ellipses between points) and the faces (a surface and
/// loops of edges, each `(edge << 1) | reversed`). The parameters are described at `solid_from_surfaces` in `bridge.cpp`.
#[derive(Debug, Clone, Default)]
pub struct SurfaceSpec {
    pub points: Vec<DVec3>,
    pub point_tol: Vec<f64>,
    /// (kind: 0 plane, 1 cylinder, 2 cone, 3 sphere, 4 torus; parameters)
    pub surfaces: Vec<(u32, [f64; 12])>,
    pub edges: Vec<EdgeSpec>,
    pub faces: Vec<FaceSpec>,
}

#[derive(Debug, Clone, Copy)]
pub struct EdgeSpec {
    /// 0 line, 1 circle, 2 ellipse.
    pub kind: u32,
    pub v: [u32; 2],
    pub params: [f64; 12],
    pub tol: f64,
}

#[derive(Debug, Clone, Default)]
pub struct FaceSpec {
    pub surface: u32,
    pub loops: Vec<Vec<u32>>,
    /// The area the face should have (the area of its triangles), to tell a face that was made the wrong way round a periodic surface; 0 when not known.
    pub area: f64,
}

/// One piece of a 2D profile, in plane coordinates.
#[derive(Debug, Clone, PartialEq)]
pub enum ProfileSegment {
    Line {
        a: [f64; 2],
        b: [f64; 2],
    },
    /// Arc through three points.
    Arc {
        a: [f64; 2],
        mid: [f64; 2],
        b: [f64; 2],
    },
    /// Full circle (a closed loop on its own).
    Circle {
        center: [f64; 2],
        radius: f64,
    },
    /// A quadratic Bezier curve (the curves of a TrueType font): one exact curve, not many short lines.
    Quad {
        a: [f64; 2],
        c: [f64; 2],
        b: [f64; 2],
    },
    /// A quadratic B-spline that is smooth (C1) all along: `poles` (the first and the last are the ends) and the distinct `knots` (the first and the last with the
    /// multiplicity 3, the others 1). A run of the curves of a font that continue each other is one such curve, so a side of a letter is one face.
    Spline2 {
        poles: Vec<[f64; 2]>,
        knots: Vec<f64>,
    },
    /// A cubic Bezier curve.
    Cubic {
        a: [f64; 2],
        c1: [f64; 2],
        c2: [f64; 2],
        b: [f64; 2],
    },
}

/// Closed loops of segments. The first loop is the outer boundary, the rest are holes.
#[derive(Debug, Clone, Default)]
pub struct Profile {
    pub loops: Vec<Vec<ProfileSegment>>,
}

impl Profile {
    fn encode(&self) -> Vec<f64> {
        let mut v = Vec::new();
        for l in &self.loops {
            v.push(0.0);
            v.push(l.len() as f64);
            for s in l {
                match s {
                    ProfileSegment::Line { a, b } => v.extend([1.0, a[0], a[1], b[0], b[1]]),
                    ProfileSegment::Arc { a, mid, b } => {
                        v.extend([2.0, a[0], a[1], mid[0], mid[1], b[0], b[1]])
                    }
                    ProfileSegment::Circle { center, radius } => {
                        v.extend([3.0, center[0], center[1], *radius])
                    }
                    ProfileSegment::Quad { a, c, b } => v.extend([4.0, a[0], a[1], c[0], c[1], b[0], b[1]]),
                    ProfileSegment::Spline2 { poles, knots } => {
                        v.extend([6.0, poles.len() as f64]);
                        for p in poles.iter() {
                            v.extend([p[0], p[1]]);
                        }
                        v.extend(knots.iter().copied());
                    }
                    ProfileSegment::Cubic { a, c1, c2, b } => {
                        v.extend([5.0, a[0], a[1], c1[0], c1[1], c2[0], c2[1], b[0], b[1]])
                    }
                }
            }
        }
        v
    }

    /// Rectangle helper.
    pub fn rectangle(w: f64, h: f64) -> Profile {
        let p = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
        Profile {
            loops: vec![(0..4)
                .map(|i| ProfileSegment::Line {
                    a: p[i],
                    b: p[(i + 1) % 4],
                })
                .collect()],
        }
    }
}

impl Solid {
    /// Extrudes a planar profile lying on the plane `(origin, normal, x_dir)` by `distance` along the normal.
    pub fn extrude_profile(
        profile: &Profile,
        origin: DVec3,
        normal: DVec3,
        x_dir: DVec3,
        distance: f64,
    ) -> Result<Solid> {
        if normal.length() < 1e-9 || x_dir.length() < 1e-9 {
            return Err(KernelError::Invalid("plane directions must not be zero".into()));
        }
        let n = normal.normalize();
        let plane = [
            origin.x, origin.y, origin.z, n.x, n.y, n.z, x_dir.x, x_dir.y, x_dir.z,
        ];
        let face = native::make_planar_face(plane, &profile.encode())?;
        let body = native::prism(&face, (n * distance).to_array())?;
        Solid::from_shape(body).validated()
    }

    /// Revolves a planar profile (in the plane `origin`, `normal`, `x_dir`) about an axis through
    /// `axis_point` with direction `axis_dir` by `angle` radians.
    pub fn revolve_profile(
        profile: &Profile,
        origin: DVec3,
        normal: DVec3,
        x_dir: DVec3,
        axis_point: DVec3,
        axis_dir: DVec3,
        angle: f64,
    ) -> Result<Solid> {
        if !(angle.is_finite() && angle.abs() > 1e-9) {
            return Err(KernelError::Invalid("the angle must not be zero".into()));
        }
        if axis_dir.length() < 1e-9 || normal.length() < 1e-9 || x_dir.length() < 1e-9 {
            return Err(KernelError::Invalid(
                "plane and axis directions must not be zero".into(),
            ));
        }
        let n = normal.normalize();
        let plane = [
            origin.x, origin.y, origin.z, n.x, n.y, n.z, x_dir.x, x_dir.y, x_dir.z,
        ];
        let face = native::make_planar_face(plane, &profile.encode())?;
        let body = native::revolve(
            &face,
            axis_point.to_array(),
            axis_dir.normalize().to_array(),
            angle,
        )?;
        Solid::from_shape(body).validated_body()
    }

    /// Revolves a planar face of this solid about an axis; the result is the swept solid only (not yet
    /// combined with this body).
    pub fn revolve_face(
        &self,
        face: FaceId,
        axis_point: DVec3,
        axis_dir: DVec3,
        angle: f64,
    ) -> Result<Solid> {
        if !(angle.is_finite() && angle.abs() > 1e-9) {
            return Err(KernelError::Invalid("the angle must not be zero".into()));
        }
        if axis_dir.length() < 1e-9 {
            return Err(KernelError::Invalid("the axis has no direction".into()));
        }
        let f = native::face_shape(self.raw(), face.0)?;
        let body = native::revolve(&f, axis_point.to_array(), axis_dir.normalize().to_array(), angle)?;
        Solid::from_shape(body).validated_body()
    }

    /// Rotates about the axis through `pivot` with direction `axis` by `angle`, then translates. The
    /// history says where every face went.
    pub fn moved(&self, translation: DVec3, pivot: DVec3, axis: DVec3, angle: f64) -> Result<OpResult> {
        if !translation.is_finite() || !pivot.is_finite() || !axis.is_finite() || !angle.is_finite() {
            return Err(KernelError::Invalid(
                "the movement must be made of finite numbers".into(),
            ));
        }
        finish(native::transform_shape(
            self.raw(),
            pivot.to_array(),
            axis.to_array(),
            angle,
            translation.to_array(),
        )?)
    }

    /// A solid made of flat faces: `faces[f]` are the loops of face `f` (the first is its outline, the others its holes) as indices into `points`. The faces are
    /// sewn within `tol` (points are the same points, so this is tiny); every point has the tolerance it needs to lie on its faces; the result is checked like every other (valid, closed, positive volume).
    pub fn from_polygons(
        points: &[DVec3],
        faces: &[Vec<Vec<u32>>],
        point_tol: &[f64],
        fallback: &[Vec<[u32; 3]>],
        tol: f64,
    ) -> Result<Solid> {
        let pts: Vec<f64> = points.iter().flat_map(|p| p.to_array()).collect();
        let (mut verts, mut loops, mut face_offsets) = (Vec::new(), vec![0u32], vec![0u32]);
        let mut loop_count = 0u32;
        for f in faces {
            for l in f {
                verts.extend_from_slice(l);
                loops.push(verts.len() as u32);
                loop_count += 1;
            }
            face_offsets.push(loop_count);
        }
        let (mut fb_vertices, mut fb_offsets) = (Vec::new(), vec![0u32]);
        for f in 0..faces.len() {
            if let Some(tris) = fallback.get(f) {
                for t in tris {
                    fb_vertices.extend_from_slice(t);
                }
            }
            fb_offsets.push((fb_vertices.len() / 3) as u32);
        }
        let shape = native::solid_from_polygons(
            &pts,
            &verts,
            &loops,
            &face_offsets,
            point_tol,
            &fb_vertices,
            &fb_offsets,
            tol,
        )?;
        Solid::from_shape(shape).validated_body()
    }

    /// A solid made of faces on analytic surfaces (planes, cylinders, cones, spheres, tori) with shared edges that are lines, circles or ellipses: a mesh of a
    /// technical part, recognised. The result is checked like every other (valid, closed, positive volume).
    pub fn from_surfaces(spec: &SurfaceSpec, tol: f64) -> Result<Solid> {
        let pts: Vec<f64> = spec.points.iter().flat_map(|p| p.to_array()).collect();
        let surf_kind: Vec<u32> = spec.surfaces.iter().map(|s| s.0).collect();
        let surf_params: Vec<f64> = spec.surfaces.iter().flat_map(|s| s.1).collect();
        let edge_kind: Vec<u32> = spec.edges.iter().map(|e| e.kind).collect();
        let edge_v: Vec<u32> = spec.edges.iter().flat_map(|e| e.v).collect();
        let edge_params: Vec<f64> = spec.edges.iter().flat_map(|e| e.params).collect();
        let edge_tol: Vec<f64> = spec.edges.iter().map(|e| e.tol).collect();
        let (mut loop_edges, mut loop_offsets, mut face_offsets) = (Vec::new(), vec![0u32], vec![0u32]);
        let mut loop_count = 0u32;
        let mut face_surf = Vec::new();
        let mut face_area = Vec::new();
        for f in &spec.faces {
            for l in &f.loops {
                loop_edges.extend_from_slice(l);
                loop_offsets.push(loop_edges.len() as u32);
                loop_count += 1;
            }
            face_offsets.push(loop_count);
            face_surf.push(f.surface);
            face_area.push(f.area);
        }
        let shape = native::solid_from_surfaces(
            &pts,
            &spec.point_tol,
            &surf_kind,
            &surf_params,
            &edge_kind,
            &edge_v,
            &edge_params,
            &edge_tol,
            &loop_edges,
            &loop_offsets,
            &face_offsets,
            &face_surf,
            &face_area,
            tol,
        )?;
        Solid::from_shape(shape).validated_body()
    }

    /// The solid scaled about `centre` by `factors` (positive numbers). Equal factors keep cylinders and spheres exact; different ones turn them into
    /// ellipses (general surfaces), which later edits may not be able to change.
    pub fn scaled(&self, centre: DVec3, factors: DVec3) -> Result<OpResult> {
        if !centre.is_finite() || !factors.is_finite() || factors.min_element() <= 1e-6 {
            return Err(KernelError::Invalid(
                "the scale factors must be positive numbers".into(),
            ));
        }
        finish(native::scale_shape(
            self.raw(),
            centre.to_array(),
            factors.to_array(),
        )?)
    }

    /// The mirror image about the plane through `point` with the normal `normal`. The result is checked like every other result (a valid closed solid
    /// with a positive volume), so a mirroring that turned the solid inside out is refused.
    pub fn mirrored(&self, point: DVec3, normal: DVec3) -> Result<OpResult> {
        if !point.is_finite() || !normal.is_finite() || normal.length() < 1e-9 {
            return Err(KernelError::Invalid(
                "the mirror plane must be made of finite numbers and have a normal".into(),
            ));
        }
        finish(native::mirror_shape(
            self.raw(),
            point.to_array(),
            normal.normalize().to_array(),
        )?)
    }

    /// Several solids as one tool for a boolean (a compound). The solids must not overlap each other: the result of a boolean with a tool that overlaps
    /// itself is not reliable (the caller checks that, and uses one tool after the other when they do).
    pub fn compound(parts: &[&Solid]) -> Result<Solid> {
        let mut it = parts.iter();
        let Some(first) = it.next() else {
            return Err(KernelError::Invalid("there is nothing to put together".into()));
        };
        let mut acc = native::compound_append(first.raw(), first.raw(), true)?;
        for p in it {
            acc = native::compound_append(&acc, p.raw(), false)?;
        }
        Ok(Solid::from_shape(acc))
    }

    pub fn boolean(&self, other: &Solid, op: BooleanOp) -> Result<OpResult> {
        let code = match op {
            BooleanOp::Fuse => 0,
            BooleanOp::Cut => 1,
            BooleanOp::Common => 2,
        };
        // Merge coplanar neighbours so a wall of a box does not stay split after the operation.
        finish(native::boolean_op(self.raw(), other.raw(), code, true)?)
    }

    /// A boolean whose result is checked against an independent calculation of the same thing. OCCT sometimes gives a wrong
    /// answer without any error for bodies that meet in awkward ways (a swept spring wound on a cylinder: the merge lost the
    /// spring while the cut and the common part were right). The volume of the common part follows from the three
    /// operations (merge = A + B - common, cut = A - common, common = common), so two independent ways must agree; when
    /// they do not, the other ways of getting the result are tried, and when no two agree the operation is refused.
    /// `history` of a result got by an other way is empty (the rounds and chamfers of the target are then not kept).
    pub fn boolean_checked(&self, other: &Solid, op: BooleanOp) -> Result<OpResult> {
        let t0 = std::time::Instant::now();
        let first = self.boolean(other, op);
        if matches!(first, Err(KernelError::Cancelled)) {
            return first;
        }
        let (va, vb) = (self.volume()?, other.volume()?);
        let tol = 1e-5 * (va + vb).max(1e-3) + 1e-6;
        let max_common = va.min(vb) + tol;
        // the volume of the common part that a result implies
        let implied = |v: f64| match op {
            BooleanOp::Fuse => va + vb - v,
            BooleanOp::Cut => va - v,
            BooleanOp::Common => v,
        };
        let plausible = |x: f64| x.is_finite() && x >= -tol && x <= max_common;
        let vol = |r: &Result<OpResult>| r.as_ref().ok().and_then(|r| r.solid.volume().ok());
        let v1 = vol(&first);
        if first.is_err()
            && !matches!(
                first,
                Err(KernelError::SplitResult | KernelError::InvalidResult | KernelError::Native(_))
            )
        {
            return first;
        }
        if first.is_err() {
            // a cut that falls into pieces is the caller's business; bodies that are really apart have nothing to check
            let apart = !self.may_be_within(other, 1e-6)
                || self.distance_to(other).map(|d| d > 1e-6).unwrap_or(false);
            if apart || (op == BooleanOp::Cut && matches!(first, Err(KernelError::SplitResult))) {
                return first;
            }
        }
        // a slow operation is not repeated: only the plausibility of the volume is checked
        if t0.elapsed() > std::time::Duration::from_millis(1500) {
            return match v1 {
                Some(v) if plausible(implied(v)) && op_volume_ok(op, va, vb, v, tol) => first,
                _ => Err(KernelError::InvalidResult),
            };
        }
        // an independent value of the common part
        let x_other: Option<f64> = match op {
            BooleanOp::Common => self
                .boolean_pieces(other, BooleanOp::Cut)
                .ok()
                .map(|p| va - p.iter().filter_map(|s| s.volume().ok()).sum::<f64>()),
            _ => vol(&self.boolean(other, BooleanOp::Common)),
        }
        .filter(|x| plausible(*x));
        let x1 = v1
            .map(implied)
            .filter(|x| plausible(*x) && op_volume_ok(op, va, vb, v1.unwrap_or(0.0), tol));
        let agree = |a: f64, b: f64| (a - b).abs() <= tol;
        if let (Some(a), Some(b)) = (x1, x_other) {
            if agree(a, b) {
                return first;
            }
        }
        if first.is_err() && x_other.is_none() {
            // nothing to compare with and the first way failed: the bodies probably do not meet; the caller explains
            return first;
        }
        if x1.is_some() && x_other.is_none() {
            // no second opinion (the independent operation failed, e.g. the bodies do not overlap): the plausible result stands
            if !matches!(op, BooleanOp::Fuse)
                || v1.is_some_and(|v| v > va.max(vb) + tol || self.fully_contains(other))
            {
                return first;
            }
        }
        // other ways to the same result
        let mut alts: Vec<(Option<f64>, Result<OpResult>)> = Vec::new();
        let cut_pieces = self.boolean(other, BooleanOp::Cut);
        match op {
            BooleanOp::Fuse => {
                if let Ok(c) = &cut_pieces {
                    alts.push((None, c.solid.boolean(other, BooleanOp::Fuse)));
                }
            }
            BooleanOp::Cut => {
                if let Ok(c) = self.boolean(other, BooleanOp::Common) {
                    alts.push((None, self.boolean(&c.solid, BooleanOp::Cut)));
                }
            }
            BooleanOp::Common => {
                if let Ok(c) = &cut_pieces {
                    alts.push((None, self.boolean(&c.solid, BooleanOp::Cut)));
                }
            }
        }
        let mut candidates: Vec<(f64, Option<OpResult>)> = Vec::new();
        if let (Some(x), Ok(r)) = (x1, first) {
            candidates.push((x, Some(r)));
        }
        if let Some(x) = x_other {
            candidates.push((x, None));
        }
        for (_, a) in alts {
            if let Ok(mut r) = a {
                if let Ok(v) = r.solid.volume() {
                    let x = implied(v);
                    if plausible(x) && op_volume_ok(op, va, vb, v, tol) {
                        r.history = History::default();
                        candidates.push((x, Some(r)));
                    }
                }
            }
        }
        // the first pair that agrees wins; its solid is returned (the first one that has a solid)
        for i in 0..candidates.len() {
            for j in i + 1..candidates.len() {
                if agree(candidates[i].0, candidates[j].0) {
                    let pick = if candidates[i].1.is_some() { i } else { j };
                    if candidates[pick].1.is_some() {
                        return Ok(candidates.swap_remove(pick).1.unwrap());
                    }
                }
            }
        }
        Err(KernelError::InvalidResult)
    }

    /// True if every sampled vertex of `other` lies inside or on this solid (used to tell a merge that adds nothing from
    /// one that lost the tool).
    fn fully_contains(&self, other: &Solid) -> bool {
        let Ok(m) = other.display_mesh() else { return false };
        let step = (m.positions.len() / 40).max(1);
        m.positions
            .iter()
            .step_by(step)
            .all(|p| !native::point_outside(&self.shape, p.as_dvec3().to_array()))
    }

    /// Like [`Self::boolean`], but a result in several pieces is returned as several solids.
    pub fn boolean_pieces(&self, other: &Solid, op: BooleanOp) -> Result<Vec<Solid>> {
        let code = match op {
            BooleanOp::Fuse => 0,
            BooleanOp::Cut => 1,
            BooleanOp::Common => 2,
        };
        let r = native::boolean_op(self.raw(), other.raw(), code, true)?;
        let n = native::solid_count(&r.shape);
        if n <= 1 {
            return Ok(vec![Solid::from_shape(r.shape).validated()?]);
        }
        let mut out = Vec::new();
        for i in 0..n {
            out.push(Solid::from_shape(native::solid_at(&r.shape, i)?).validated()?);
        }
        Ok(out)
    }

    #[doc(hidden)]
    pub fn boolean_unify_opt(&self, other: &Solid, op: BooleanOp, unify: bool) -> Result<OpResult> {
        let code = match op {
            BooleanOp::Fuse => 0,
            BooleanOp::Cut => 1,
            BooleanOp::Common => 2,
        };
        finish(native::boolean_op(self.raw(), other.raw(), code, unify)?)
    }

    pub fn fillet(&self, edges: &[EdgeId], radius: f64) -> Result<OpResult> {
        let chain = self.smooth_chain(edges, crate::blend::CHAIN_ANGLE_DEG);
        let ids: Vec<u32> = chain.iter().map(|e| e.0).collect();
        let r = finish(native::fillet_edges(self.raw(), &ids, radius)?)?;
        self.check_blend(&r.solid, radius, self.edges_length(&chain), "round")?;
        Ok(r)
    }

    /// All edges a round or chamfer of `edges` would really treat: OCCT always follows the chain of
    /// edges that continue each other tangentially, and offers no switch to stop that.
    pub fn blend_chain(&self, edges: &[EdgeId], chamfer: bool) -> Result<Vec<EdgeId>> {
        // our own chain first (it also follows the pieces of a loft outline), then what the kernel adds
        let own = self.smooth_chain(edges, crate::blend::CHAIN_ANGLE_DEG);
        let ids: Vec<u32> = own.iter().map(|e| e.0).collect();
        Ok(native::blend_chain(self.raw(), &ids, chamfer)?
            .into_iter()
            .map(EdgeId)
            .collect())
    }

    /// Rounds several edges in one operation, each with its own radius.
    pub fn fillet_radii(&self, edges: &[(EdgeId, f64)]) -> Result<OpResult> {
        let all = self.chain_map(edges);
        let ids: Vec<u32> = all.iter().map(|e| e.0 .0).collect();
        let radii: Vec<f64> = all.iter().map(|e| e.1).collect();
        let r = finish(native::fillet_edges_radii(self.raw(), &ids, &radii)?)?;
        let size = radii.iter().copied().fold(0.0, f64::max);
        let list: Vec<EdgeId> = all.iter().map(|e| e.0).collect();
        self.check_blend(&r.solid, size, self.edges_length(&list), "round")?;
        Ok(r)
    }

    /// Chamfer with two distances: `distance` is measured on the reference face of each edge, `distance * tan(angle)`
    /// on the other one. The reference face is the one of the two that comes first in a fixed geometric order
    /// (or second with `flip`), so it can be found again after other operations.
    pub fn chamfer_angled(
        &self,
        edges: &[EdgeId],
        distance: f64,
        angle_deg: f64,
        flip: bool,
    ) -> Result<OpResult> {
        if !(angle_deg.is_finite() && angle_deg > 0.5 && angle_deg < 89.5) {
            return Err(KernelError::Invalid(
                "the chamfer angle must be between 1 and 89 degrees".into(),
            ));
        }
        let d2 = distance * angle_deg.to_radians().tan();
        let mut ids = Vec::new();
        let mut faces = Vec::new();
        // the selected edges decide the reference face; the rest of the chain follows the face it shares with them
        let chain = self.smooth_chain(edges, crate::blend::CHAIN_ANGLE_DEG);
        let mut ordered: Vec<EdgeId> = edges.to_vec();
        ordered.extend(chain.iter().filter(|c| !edges.contains(c)));
        let mut refs: Vec<FaceId> = Vec::new();
        for e in &ordered {
            let mut owners: Vec<FaceId> = self
                .faces()
                .filter(|f| self.face_edges(*f).map(|v| v.contains(e)).unwrap_or(false))
                .collect();
            if owners.len() != 2 {
                return Err(KernelError::Invalid("an edge does not have two faces".into()));
            }
            let key = |f: &FaceId| {
                let i = self
                    .face_info(*f)
                    .map(|i| (i.normal, i.origin))
                    .unwrap_or((DVec3::ZERO, DVec3::ZERO));
                [i.0.x, i.0.y, i.0.z, i.1.x, i.1.y, i.1.z].map(|v| (v * 1e6).round() as i64)
            };
            owners.sort_by_key(key);
            let reference = if edges.contains(e) {
                owners[usize::from(flip)]
            } else {
                owners
                    .iter()
                    .copied()
                    .find(|f| refs.contains(f))
                    .unwrap_or(owners[usize::from(flip)])
            };
            if !refs.contains(&reference) {
                refs.push(reference);
            }
            ids.push(e.0);
            faces.push(reference.0);
        }
        let r = finish(native::chamfer_edges_asym(
            self.raw(),
            &ids,
            &faces,
            &vec![distance; faces.len()],
            &vec![d2; faces.len()],
        )?)?;
        self.check_blend(&r.solid, distance.max(d2), self.edges_length(&ordered), "chamfer")?;
        Ok(r)
    }

    pub fn chamfer(&self, edges: &[EdgeId], distance: f64) -> Result<OpResult> {
        let chain = self.smooth_chain(edges, crate::blend::CHAIN_ANGLE_DEG);
        let ids: Vec<u32> = chain.iter().map(|e| e.0).collect();
        let r = finish(native::chamfer_edges(self.raw(), &ids, distance)?)?;
        self.check_blend(&r.solid, distance, self.edges_length(&chain), "chamfer")?;
        Ok(r)
    }

    /// Removes faces (such as the faces of a round or chamfer) and extends their neighbours so the
    /// gap closes. Used to restore a sharp edge.
    pub fn remove_faces(&self, faces: &[FaceId]) -> Result<OpResult> {
        let ids: Vec<u32> = faces.iter().map(|f| f.0).collect();
        finish(native::defeature_faces(self.raw(), &ids)?)
    }

    /// A hollow body: the `open` faces are taken away and the walls are `thickness` thick (`inwards`: the outside stays as it is; otherwise the walls grow outwards
    /// from the outside). With no open face the body is closed and hollow. `round_corners` rounds the corners that stick out instead of mitring them.
    pub fn shell(
        &self,
        open: &[FaceId],
        thickness: f64,
        inwards: bool,
        round_corners: bool,
    ) -> Result<OpResult> {
        if !(thickness.is_finite() && thickness > 1e-6) {
            return Err(KernelError::Invalid(
                "the wall thickness must be a positive number".into(),
            ));
        }
        let ids: Vec<u32> = open.iter().map(|f| f.0).collect();
        let signed = if inwards { -thickness } else { thickness };
        finish(native::shell_solid(self.raw(), &ids, signed, round_corners)?)
    }

    /// A hollow body whose walls lie half inside and half outside the surface of the body (`thickness` in all).
    pub fn shell_centre(&self, open: &[FaceId], thickness: f64, round_corners: bool) -> Result<OpResult> {
        let inner = self.shell(open, thickness / 2.0, true, round_corners)?;
        let outer = self.shell(open, thickness / 2.0, false, round_corners)?;
        let r = inner.solid.boolean_checked(&outer.solid, BooleanOp::Fuse)?;
        let u = r.solid.unify()?;
        Ok(OpResult {
            solid: u.solid,
            history: History::default(),
        })
    }

    /// Changes the size of an existing round or chamfer made of `faces`: the faces are removed
    /// (restoring the sharp edges) and, unless `size` is zero, the sharp edges are rounded or
    /// chamfered again with the new size. On failure the solid is untouched.
    pub fn edit_blend(&self, faces: &[FaceId], size: f64, chamfer: bool) -> Result<OpResult> {
        self.edit_blend_angled(faces, size, chamfer, 45.0, false)
    }

    /// Like [`Self::edit_blend`]; a chamfer with an angle other than 45 degrees is made with two distances.
    pub fn edit_blend_angled(
        &self,
        faces: &[FaceId],
        size: f64,
        chamfer: bool,
        angle_deg: f64,
        flip: bool,
    ) -> Result<OpResult> {
        if faces.is_empty() {
            return Err(KernelError::Invalid(
                "there is no round or chamfer to change".into(),
            ));
        }
        if !size.is_finite() || size < 0.0 {
            return Err(KernelError::Invalid(
                "the size must be zero or a positive number".into(),
            ));
        }
        // Which faces touch the blend, and which of those pairs are separated by it.
        let edges_of: Vec<Vec<EdgeId>> = self.faces().map(|f| self.face_edges(f)).collect::<Result<_>>()?;
        let shares = |a: FaceId, b: FaceId| {
            edges_of[a.0 as usize]
                .iter()
                .any(|e| edges_of[b.0 as usize].contains(e))
        };
        let mut neighbours: Vec<FaceId> = Vec::new();
        for g in self.faces().filter(|g| !faces.contains(g)) {
            if faces.iter().any(|f| shares(*f, g)) {
                neighbours.push(g);
            }
        }
        let separated: Vec<(FaceId, FaceId)> = neighbours
            .iter()
            .enumerate()
            .flat_map(|(i, a)| neighbours[i + 1..].iter().map(move |b| (*a, *b)))
            .filter(|(a, b)| !shares(*a, *b))
            .collect();
        let before = self.volume()?;
        let open = self.remove_faces(faces).map_err(|e| match e {
            KernelError::Native(m) => KernelError::Invalid(format!(
                "The round or chamfer cannot be changed here: {}",
                m.rsplit(": ").next().unwrap_or(&m)
            )),
            e => e,
        })?;
        // Removing a round adds material, removing a concave one takes it away; either way it must change.
        if (open.solid.volume()? - before).abs() < 1e-9 {
            return Err(KernelError::Invalid(
                "The round or chamfer could not be removed cleanly, so it was not changed".into(),
            ));
        }
        if size < 1e-9 {
            return Ok(open);
        }
        // Edges that appeared where two separated neighbours now meet.
        let sharp = &open.solid;
        let sharp_edges_of: Vec<Vec<EdgeId>> = sharp
            .faces()
            .map(|f| sharp.face_edges(f))
            .collect::<Result<_>>()?;
        let mut edges: Vec<EdgeId> = Vec::new();
        for (a, b) in &separated {
            for fa in open.history.successors(0, *a) {
                for fb in open.history.successors(0, *b) {
                    if fa == fb {
                        continue;
                    }
                    for e in &sharp_edges_of[fa.0 as usize] {
                        if sharp_edges_of[fb.0 as usize].contains(e) && !edges.contains(e) {
                            edges.push(*e);
                        }
                    }
                }
            }
        }
        if edges.is_empty() {
            return Err(KernelError::Invalid(
                "The sharp edge could not be found again, so the round or chamfer was not changed".into(),
            ));
        }
        let blended = if chamfer && (angle_deg - 45.0).abs() > 1e-9 {
            sharp.chamfer_angled(&edges, size, angle_deg, flip)
        } else if chamfer {
            sharp.chamfer(&edges, size)
        } else {
            sharp.fillet(&edges, size)
        }
        .map_err(|e| match e {
            KernelError::Native(_) if chamfer => KernelError::Invalid(format!(
                "The chamfer of {size} mm does not fit here (it is too large for the neighbouring faces)"
            )),
            KernelError::Native(_) => KernelError::Invalid(format!(
                "The radius of {size} mm does not fit here (it is too large for the neighbouring faces)"
            )),
            e => e,
        })?;
        Ok(OpResult {
            history: open.history.compose(&blended.history),
            solid: blended.solid,
        })
    }

    /// Pairs of faces that are not neighbours but were separated by the blend `faces`.
    fn separated_pairs(&self, faces: &[FaceId]) -> Result<Vec<(FaceId, FaceId)>> {
        let edges_of: Vec<Vec<EdgeId>> = self.faces().map(|f| self.face_edges(f)).collect::<Result<_>>()?;
        let shares = |a: FaceId, b: FaceId| {
            edges_of[a.0 as usize]
                .iter()
                .any(|e| edges_of[b.0 as usize].contains(e))
        };
        let neighbours: Vec<FaceId> = self
            .faces()
            .filter(|g| !faces.contains(g) && faces.iter().any(|f| shares(*f, *g)))
            .collect();
        Ok(neighbours
            .iter()
            .enumerate()
            .flat_map(|(i, a)| neighbours[i + 1..].iter().map(move |b| (*a, *b)))
            .filter(|(a, b)| !shares(*a, *b))
            .collect())
    }

    /// Edges shared by two faces of this solid.
    fn shared_edges(&self, a: FaceId, b: FaceId) -> Result<Vec<EdgeId>> {
        let eb = self.face_edges(b)?;
        Ok(self
            .face_edges(a)?
            .into_iter()
            .filter(|e| eb.contains(e))
            .collect())
    }

    /// True if [`Self::blend_with_neighbours`] would redo existing blends (the tangent chain of `edges`
    /// runs through them). Cheap: no solid is built.
    pub fn chain_touches_blends(
        &self,
        existing: &[BlendInput],
        edges: &[EdgeId],
        chamfer: bool,
    ) -> Result<bool> {
        let chain = self.blend_chain(edges, chamfer)?;
        for f in existing.iter().filter(|f| f.chamfer == chamfer) {
            for face in &f.faces {
                if self.face_edges(*face)?.iter().any(|e| chain.contains(e)) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Rounds or chamfers `new_edges` together with the existing blends that their tangent chain runs
    /// through: those are removed first and all edges are blended again in one operation, each
    /// group with its own size, so the chain does not drag unselected edges along.
    ///
    /// `Ok(None)`: the chain touches no existing blend (nothing to do differently). `Err`: the
    /// combination did not work; the caller falls back to the plain operation.
    pub fn blend_with_neighbours(
        &self,
        existing: &[BlendInput],
        new_edges: &[EdgeId],
        size: f64,
        chamfer: bool,
    ) -> Result<Option<BlendCombo>> {
        let chain = self.blend_chain(new_edges, chamfer)?;
        let mut absorbed: Vec<usize> = Vec::new();
        for (i, f) in existing.iter().enumerate() {
            let mut touches = false;
            for face in &f.faces {
                if self.face_edges(*face)?.iter().any(|e| chain.contains(e)) {
                    touches = true;
                }
            }
            if touches {
                absorbed.push(i);
            }
        }
        if absorbed.is_empty() {
            return Ok(None);
        }
        if absorbed.iter().any(|i| existing[*i].chamfer != chamfer) {
            return Ok(None);
        }
        let fail = |m: &str| KernelError::Invalid(m.to_string());
        // faces next to each new edge, as pairs
        let mut new_pairs: Vec<(FaceId, FaceId)> = Vec::new();
        for e in new_edges {
            let owners: Vec<FaceId> = self
                .faces()
                .filter(|f| self.face_edges(*f).map(|v| v.contains(e)).unwrap_or(false))
                .collect();
            if owners.len() != 2 {
                return Err(fail("an edge does not have two faces"));
            }
            new_pairs.push((owners[0], owners[1]));
        }
        let map_pairs = |pairs: &[(FaceId, FaceId)], h: &History| -> Vec<(FaceId, FaceId)> {
            let mut out = Vec::new();
            for (a, b) in pairs {
                for fa in h.successors(0, *a) {
                    for fb in h.successors(0, *b) {
                        out.push((fa, fb));
                    }
                }
            }
            out
        };
        let map_faces = |faces: &[FaceId], h: &History| -> Vec<FaceId> {
            let mut v: Vec<FaceId> = faces.iter().flat_map(|f| h.successors(0, *f)).collect();
            v.sort();
            v.dedup();
            v
        };
        let mut cur = self.clone();
        let mut hist = History {
            links: self.faces().map(|f| (0, f, f)).collect(),
        };
        let mut feat_faces: Vec<Vec<FaceId>> = absorbed.iter().map(|i| existing[*i].faces.clone()).collect();
        let mut feat_pairs: Vec<Vec<(FaceId, FaceId)>> = vec![Vec::new(); absorbed.len()];
        for k in 0..absorbed.len() {
            let before = cur.volume()?;
            let sep = cur.separated_pairs(&feat_faces[k])?;
            let open = cur.remove_faces(&feat_faces[k])?;
            if (open.solid.volume()? - before).abs() < 1e-9 {
                return Err(fail("a neighbouring round could not be removed cleanly"));
            }
            let mut pairs_k = Vec::new();
            for (fa, fb) in map_pairs(&sep, &open.history) {
                if fa != fb && !open.solid.shared_edges(fa, fb)?.is_empty() {
                    pairs_k.push((fa, fb));
                }
            }
            for j in 0..absorbed.len() {
                if j != k {
                    feat_pairs[j] = map_pairs(&feat_pairs[j], &open.history);
                }
                if j > k {
                    feat_faces[j] = map_faces(&feat_faces[j], &open.history);
                }
            }
            new_pairs = map_pairs(&new_pairs, &open.history);
            feat_pairs[k] = pairs_k;
            hist = hist.compose(&open.history);
            cur = open.solid;
        }
        let edges_of = |pairs: &[(FaceId, FaceId)]| -> Result<Vec<EdgeId>> {
            let mut out: Vec<EdgeId> = Vec::new();
            for (a, b) in pairs {
                if a == b {
                    continue;
                }
                for e in cur.shared_edges(*a, *b)? {
                    if !out.contains(&e) {
                        out.push(e);
                    }
                }
            }
            Ok(out)
        };
        let new_e = edges_of(&new_pairs)?;
        if new_e.len() < new_edges.len() {
            return Err(fail(
                "the selected edge could not be found after removing the neighbouring rounds",
            ));
        }
        let mut groups_edges: Vec<(Option<usize>, Vec<EdgeId>, f64)> = Vec::new();
        for (k, i) in absorbed.iter().enumerate() {
            let e = edges_of(&feat_pairs[k])?;
            if e.is_empty() {
                return Err(fail("a neighbouring round could not be found again"));
            }
            groups_edges.push((Some(*i), e, existing[*i].size));
        }
        groups_edges.push((None, new_e, size));
        let all: Vec<(EdgeId, f64)> = groups_edges
            .iter()
            .flat_map(|(_, es, sz)| es.iter().map(move |e| (*e, *sz)))
            .collect();
        let res = if chamfer {
            if all.iter().any(|(_, s)| (s - size).abs() > 1e-9) {
                return Err(fail("chamfers with different distances cannot be combined"));
            }
            let ids: Vec<EdgeId> = all.iter().map(|(e, _)| *e).collect();
            cur.chamfer(&ids, size)?
        } else {
            cur.fillet_radii(&all)?
        };
        let mut groups = Vec::new();
        for (existing_idx, es, _) in &groups_edges {
            let mut faces: Vec<FaceId> = es
                .iter()
                .flat_map(|e| res.history.successors(2, FaceId(e.0)))
                .collect();
            faces.sort();
            faces.dedup();
            if faces.is_empty() {
                return Err(fail("the new round could not be recognised"));
            }
            groups.push(BlendGroup {
                existing: *existing_idx,
                faces,
            });
        }
        Ok(Some(BlendCombo {
            result: OpResult {
                history: hist.compose(&res.history),
                solid: res.solid,
            },
            groups,
        }))
    }

    /// Stretches the solid across a plane: the part on the side the normal points to moves by `distance` along the normal and the gap is filled with the
    /// cross-section, swept the same way. A box grows through its straight walls, and the holes, rounds and pockets on each side keep their size.
    /// A negative distance takes out a layer of the solid (the cross-section must be the same along it).
    pub fn stretched(&self, point: DVec3, normal: DVec3, distance: f64) -> Result<Solid> {
        if !(point.is_finite() && normal.is_finite() && normal.length() > 1e-9 && distance.is_finite())
            || distance.abs() < 1e-6
        {
            return Err(KernelError::Invalid(
                "the plane and the distance must be finite numbers, the distance not zero".into(),
            ));
        }
        let n = normal.normalize();
        let (lo, hi) = self.bounding_box()?;
        let size = ((hi - lo).length() * 3.0).max(1.0) + distance.abs();
        let x = if n.cross(DVec3::Z).length() > 1e-6 {
            n.cross(DVec3::Z).normalize()
        } else {
            DVec3::X
        };
        let q = size;
        let c = [[-q, -q], [q, -q], [q, q], [-q, q]];
        let half = Profile {
            loops: vec![(0..4)
                .map(|i| ProfileSegment::Line {
                    a: c[i],
                    b: c[(i + 1) % 4],
                })
                .collect()],
        };
        let upper = Solid::extrude_profile(&half, point, n, x, size)?;
        let dbg = std::env::var_os("ROLIDIUM_DEBUG_STRETCH").is_some();
        let lower = self
            .boolean(&upper, BooleanOp::Cut)
            .map_err(|e| {
                if dbg {
                    eprintln!("stretch: lower {e}");
                }
                e
            })?
            .solid;
        let part = self
            .boolean(&upper, BooleanOp::Common)
            .map_err(|e| {
                if dbg {
                    eprintln!("stretch: upper {e}");
                }
                e
            })?
            .solid;
        // the face of the lower part that lies in the plane is pulled by the distance
        let mut cur = lower;
        let offset = point.dot(n);
        loop {
            let found = cur.faces().find(|f| {
                cur.face_info(*f)
                    .map(|i| {
                        i.kind == FaceKind::Plane
                            && i.normal.dot(n) > 0.9999
                            && (i.origin.dot(n) - offset).abs() < 1e-6 * (1.0 + offset.abs())
                    })
                    .unwrap_or(false)
            });
            let Some(f) = found else { break };
            cur = cur
                .pull_face_plain(f, distance)
                .map_err(|e| {
                    if dbg {
                        eprintln!("stretch: pull {e}");
                    }
                    e
                })?
                .solid;
            // a pulled face is now at the new place; the next plane face found is another piece of the cross-section
            let moved = offset + distance;
            if !cur.faces().any(|g| {
                cur.face_info(g)
                    .map(|i| {
                        i.kind == FaceKind::Plane
                            && i.normal.dot(n) > 0.9999
                            && (i.origin.dot(n) - offset).abs() < 1e-6 * (1.0 + offset.abs())
                    })
                    .unwrap_or(false)
            }) {
                let _ = moved;
                break;
            }
        }
        let shifted = part
            .moved(n * distance, DVec3::ZERO, DVec3::Z, 0.0)
            .map_err(|e| {
                if dbg {
                    eprintln!("stretch: move {e}");
                }
                e
            })?
            .solid;
        let joined = cur.boolean(&shifted, BooleanOp::Fuse).map_err(|e| {
            if dbg {
                eprintln!("stretch: fuse {e}");
            }
            e
        })?;
        let r = joined.solid.unify()?.solid;
        let before = self.volume()?;
        let after = r.volume()?;
        let (lo_v, hi_v) = (before.min(after), before.max(after));
        let _ = (lo_v, hi_v);
        Ok(r)
    }

    /// Merges neighbouring faces that lie on the same surface.
    pub fn unify(&self) -> Result<OpResult> {
        finish(native::unify_same_domain(self.raw())?)
    }

    /// Moves a planar face along its outward normal by `distance` (negative removes material).
    ///
    /// First version: the outline of the face is extruded and fused or cut. Known limitation:
    /// neighbouring faces that are not perpendicular to the pulled face are not extended or
    /// trimmed; a true face offset is planned.
    pub fn pull_face(&self, face: FaceId, distance: f64) -> Result<OpResult> {
        let info = self.face_info(face)?;
        if info.kind == FaceKind::Plane && distance.is_finite() && distance.abs() >= 1e-6 {
            // an end of a plain cone or cylinder: the cone is extended or shortened with its angle (a prism on the end would not)
            if let Some(fr) = self.frustum_info() {
                if face == fr.cap_base || face == fr.cap_top {
                    return self.pull_frustum_cap(face, distance);
                }
            }
            let touching = self.blend_map().touching(face);
            if !touching.is_empty() {
                return self.pull_face_with_blends(face, distance, &touching);
            }
        }
        self.pull_face_plain(face, distance)
    }

    /// [`Self::pull_face`] without looking at rounds and chamfers next to the face.
    pub fn pull_face_plain(&self, face: FaceId, distance: f64) -> Result<OpResult> {
        let info = self.face_info(face)?;
        if info.kind != FaceKind::Plane {
            return Err(KernelError::NotPlanar);
        }
        if !distance.is_finite() {
            return Err(KernelError::Invalid("distance must be a finite number".into()));
        }
        if distance.abs() < 1e-6 {
            return Err(KernelError::ZeroDistance);
        }
        let face_shape = native::face_shape(self.raw(), face.0)?;
        let n = info.normal.normalize();
        let tool = crate::timing::stage("prism of the face", || -> Result<Solid> {
            Ok(Solid::from_shape(native::prism(
                &face_shape,
                (n * distance).to_array(),
            )?))
        })?;
        let before = crate::timing::stage("volume", || self.volume())?;
        let op = if distance > 0.0 {
            BooleanOp::Fuse
        } else {
            BooleanOp::Cut
        };
        let res = self.boolean(&tool, op)?;
        let after = crate::timing::stage("volume", || res.solid.volume())?;
        let ok = match op {
            BooleanOp::Fuse => after >= before - 1e-6,
            _ => after <= before + 1e-6,
        };
        if !ok || (after - before).abs() < 1e-9 {
            return Err(KernelError::Invalid(
                "the pull did not change the solid as expected".into(),
            ));
        }
        Ok(res)
    }

    /// Cross-section with a plane, as 2D-agnostic 3D segments.
    pub fn section(&self, origin: DVec3, normal: DVec3, tolerance: f64) -> Result<Vec<SectionCurve>> {
        let raw = native::section_plane(
            self.raw(),
            origin.to_array(),
            normal.normalize().to_array(),
            tolerance,
        )?;
        let mut out = Vec::new();
        let mut i = 0;
        let p = |i: usize| DVec3::new(raw[i], raw[i + 1], raw[i + 2]);
        while i < raw.len() {
            match raw[i] as i32 {
                1 => {
                    out.push(SectionCurve::Line {
                        a: p(i + 1),
                        b: p(i + 4),
                    });
                    i += 7;
                }
                2 => {
                    out.push(SectionCurve::Arc {
                        a: p(i + 1),
                        mid: p(i + 4),
                        b: p(i + 7),
                    });
                    i += 10;
                }
                3 => {
                    out.push(SectionCurve::Circle {
                        center: p(i + 1),
                        normal: p(i + 4),
                        radius: raw[i + 7],
                    });
                    i += 8;
                }
                _ => return Err(KernelError::Invalid("corrupt section data".into())),
            }
        }
        Ok(out)
    }

    /// Sews separate faces (each given as a one-face solid-less shape source) into a solid.
    pub fn sew_faces_of(sources: &[(&Solid, FaceId)], tolerance: f64) -> Result<Solid> {
        let mut sewer = native::new_sewer(tolerance);
        for (s, f) in sources {
            let face = native::face_shape(s.raw(), f.0)?;
            native::sewer_add(sewer.pin_mut(), &face)?;
        }
        Solid::from_shape(native::sewer_finish(sewer.pin_mut())?).validated()
    }
}

/// A profile of a loft: a flat sketch profile or a flat face of a solid.
pub enum LoftInput<'a> {
    Profile {
        profile: &'a Profile,
        origin: DVec3,
        normal: DVec3,
        x_dir: DVec3,
    },
    Face {
        solid: &'a Solid,
        face: FaceId,
    },
    /// A closed flat loop of one edge (for example the circular end of a tube).
    Edge {
        solid: &'a Solid,
        edge: EdgeId,
    },
}

impl Solid {
    /// The flat face a loft or sweep input stands for.
    pub(crate) fn profile_face(i: &LoftInput) -> Result<cxx::SharedPtr<native::Shape>> {
        Ok(match i {
            LoftInput::Profile {
                profile,
                origin,
                normal,
                x_dir,
            } => {
                if normal.length() < 1e-9 || x_dir.length() < 1e-9 {
                    return Err(KernelError::Invalid("plane directions must not be zero".into()));
                }
                let n = normal.normalize();
                let plane = [
                    origin.x, origin.y, origin.z, n.x, n.y, n.z, x_dir.x, x_dir.y, x_dir.z,
                ];
                native::make_planar_face(plane, &profile.encode())?
            }
            LoftInput::Face { solid, face } => native::face_shape(solid.raw(), face.0)?,
            LoftInput::Edge { solid, edge } => native::edge_face(solid.raw(), edge.0).map_err(|e| {
                KernelError::Invalid(format!(
                    "This edge cannot be a loft profile: {}",
                    e.what().rsplit(": ").next().unwrap_or("")
                ))
            })?,
        })
    }

    /// The frame of a profile given by a plane: origin, unit normal, unit x and y directions.
    fn profile_frame<'a>(i: &'a LoftInput<'a>) -> Option<(DVec3, DVec3, DVec3, DVec3, &'a Profile)> {
        let LoftInput::Profile {
            profile,
            origin,
            normal,
            x_dir,
        } = i
        else {
            return None;
        };
        if normal.length() < 1e-9 || x_dir.length() < 1e-9 {
            return None;
        }
        let n = normal.normalize();
        let x = (*x_dir - n * x_dir.dot(n)).normalize_or_zero();
        if x.length() < 0.5 {
            return None;
        }
        Some((*origin, n, x, n.cross(x), profile))
    }

    /// Two coaxial circles in parallel planes are a cone or a cylinder; two polygons whose sides are parallel in pairs (similar and equally
    /// turned) have flat sides. `None` for anything else (the general loft is used).
    fn loft_analytic(a: &LoftInput, b: &LoftInput) -> Option<Solid> {
        let (oa, na, xa, ya, pa) = Solid::profile_frame(a)?;
        let (ob, nb, xb, yb, pb) = Solid::profile_frame(b)?;
        if pa.loops.len() != 1 || pb.loops.len() != 1 || na.cross(nb).length() > 1e-9 {
            return None;
        }
        let (la, lb) = (&pa.loops[0], &pb.loops[0]);
        let at = |o: DVec3, x: DVec3, y: DVec3, p: [f64; 2]| o + x * p[0] + y * p[1];
        if let (
            [ProfileSegment::Circle {
                center: ca,
                radius: ra,
            }],
            [ProfileSegment::Circle {
                center: cb,
                radius: rb,
            }],
        ) = (la.as_slice(), lb.as_slice())
        {
            let (ca, cb, ra, rb) = (*ca, *cb, *ra, *rb);
            let (c1, c2) = (at(oa, xa, ya, ca), at(ob, xb, yb, cb));
            let d = c2 - c1;
            let h = d.dot(na);
            if h.abs() < 1e-9 || d.cross(na).length() > 1e-9 * (1.0 + d.length()) || ra <= 0.0 || rb <= 0.0 {
                return None;
            }
            let axis = na * h.signum();
            let solid = if (ra - rb).abs() <= 1e-12 * ra.max(rb) {
                Solid::cylinder(ra, h.abs(), c1, axis)
            } else {
                native::make_cone(ra, rb, h.abs(), c1.to_array(), axis.to_array())
                    .map_err(KernelError::from)
                    .and_then(|s| Solid::from_shape(s).validated_body())
            };
            return solid.ok();
        }
        // polygons
        let corners = |l: &Vec<ProfileSegment>, o: DVec3, x: DVec3, y: DVec3| -> Option<Vec<DVec3>> {
            let mut v = Vec::new();
            for s in l {
                match s {
                    ProfileSegment::Line { a, .. } => v.push(at(o, x, y, *a)),
                    _ => return None,
                }
            }
            (v.len() >= 3).then_some(v)
        };
        let pts_a = corners(la, oa, xa, ya)?;
        let pts_b = corners(lb, ob, xb, yb)?;
        if pts_a.len() != pts_b.len() {
            return None;
        }
        let n = pts_a.len();
        // the sides run the same way (parallel, not opposite) and every side face is flat
        let len: f64 = pts_a
            .iter()
            .zip(&pts_b)
            .map(|(p, q)| (*p - *q).length())
            .fold(0.0, f64::max)
            .max(1.0);
        for i in 0..n {
            let (ea, eb) = (pts_a[(i + 1) % n] - pts_a[i], pts_b[(i + 1) % n] - pts_b[i]);
            if ea.length() < 1e-9
                || eb.length() < 1e-9
                || ea.cross(eb).length() > 1e-9 * ea.length() * eb.length()
                || ea.dot(eb) <= 0.0
            {
                return None;
            }
            // the four corners lie in one plane
            let q = [pts_a[i], pts_a[(i + 1) % n], pts_b[(i + 1) % n], pts_b[i]];
            let nrm = ea.cross(pts_b[i] - pts_a[i]);
            if nrm.length() < 1e-9 || (q[2] - q[0]).dot(nrm.normalize()).abs() > 1e-9 * len {
                return None;
            }
        }
        let face = |q: &[DVec3]| -> Option<cxx::SharedPtr<native::Shape>> {
            let nrm = (q[1] - q[0]).cross(q[q.len() - 1] - q[0]);
            let nrm = if nrm.length() < 1e-12 {
                (q[1] - q[0]).cross(q[2] - q[0])
            } else {
                nrm
            };
            if nrm.length() < 1e-12 {
                return None;
            }
            let nn = nrm.normalize();
            let x = (q[1] - q[0]).normalize();
            let y = nn.cross(x);
            let two = |p: DVec3| [(p - q[0]).dot(x), (p - q[0]).dot(y)];
            let segs: Vec<ProfileSegment> = (0..q.len())
                .map(|k| ProfileSegment::Line {
                    a: two(q[k]),
                    b: two(q[(k + 1) % q.len()]),
                })
                .collect();
            let plane = [q[0].x, q[0].y, q[0].z, nn.x, nn.y, nn.z, x.x, x.y, x.z];
            native::make_planar_face(plane, &Profile { loops: vec![segs] }.encode()).ok()
        };
        let mut sewer = native::new_sewer(1e-7);
        let mut add = |f: Option<cxx::SharedPtr<native::Shape>>| -> Option<()> {
            native::sewer_add(sewer.pin_mut(), &*f?).ok()
        };
        add(face(&pts_a))?;
        add(face(&pts_b))?;
        for i in 0..n {
            add(face(&[
                pts_a[i],
                pts_a[(i + 1) % n],
                pts_b[(i + 1) % n],
                pts_b[i],
            ]))?;
        }
        let shape = native::sewer_finish(sewer.pin_mut()).ok()?;
        Solid::from_shape(shape).validated_body().ok()
    }

    /// Joins two or more flat profiles (in the given order) into a solid. Holes of the profiles are
    /// lofted separately and subtracted; all profiles must have the same number of holes.
    pub fn loft(inputs: &[LoftInput], ruled: bool) -> Result<Solid> {
        if inputs.len() < 2 {
            return Err(KernelError::Invalid("a loft needs at least two profiles".into()));
        }
        // two coaxial circles give a real cone or cylinder, two similar polygons give flat sides: the faces are of the types they are
        // (a cone can be edited like a cone, and a STEP file holds them as such), not freeform surfaces that look the same
        if inputs.len() == 2 {
            if let Some(s) = Solid::loft_analytic(&inputs[0], &inputs[1]) {
                return Ok(s);
            }
        }
        let mut l = native::new_loft(ruled);
        for i in inputs {
            let face = Solid::profile_face(i)?;
            native::loft_add(l.pin_mut(), &face).map_err(|e| {
                let m = e.what().to_string();
                if m.contains("same number of holes") {
                    KernelError::Invalid("All profiles must have the same number of holes".into())
                } else {
                    KernelError::Native(m)
                }
            })?;
        }
        let shape = native::loft_finish(l.pin_mut())?;
        Solid::from_shape(shape).validated_body()
    }
}

/// An existing round or chamfer, for [`Solid::blend_with_neighbours`].
#[derive(Debug, Clone)]
pub struct BlendInput {
    pub faces: Vec<FaceId>,
    pub size: f64,
    pub chamfer: bool,
}

/// Faces of one blend in the result: of existing blend number `existing`, or the new one (`None`).
#[derive(Debug, Clone)]
pub struct BlendGroup {
    pub existing: Option<usize>,
    pub faces: Vec<FaceId>,
}

#[derive(Debug, Clone)]
pub struct BlendCombo {
    pub result: OpResult,
    pub groups: Vec<BlendGroup>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SectionCurve {
    Line {
        a: DVec3,
        b: DVec3,
    },
    Arc {
        a: DVec3,
        mid: DVec3,
        b: DVec3,
    },
    Circle {
        center: DVec3,
        normal: DVec3,
        radius: f64,
    },
}

/// A complete (360 degree) cylindrical face: a hole or a pin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CylinderInfo {
    pub axis: DVec3,
    /// Point on the axis at the lower end of the face.
    pub base: DVec3,
    pub radius: f64,
    pub height: f64,
    /// True for a hole (material outside), false for a pin/boss (material inside).
    pub is_hole: bool,
}

/// A conical face: where the helix on it starts and how it opens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConeInfo {
    /// Unit axis, turned so that it points up (+Z) when it is not horizontal.
    pub axis: DVec3,
    /// Point on the axis at the lower end of the face (along `axis`).
    pub base: DVec3,
    pub height: f64,
    /// Radius at the lower end and at the upper end.
    pub r_base: f64,
    pub r_top: f64,
    /// Half angle in degrees, positive when the radius grows along `axis`, negative when it shrinks.
    pub half_angle_deg: f64,
}

impl Solid {
    /// Describes a conical face (the whole of it or a part): the axis, the lower end, the height and the radii at the ends.
    pub fn cone_info(&self, face: FaceId) -> Result<Option<ConeInfo>> {
        let info = self.face_info(face)?;
        if info.kind != FaceKind::Cone {
            return Ok(None);
        }
        let mut axis = info.normal.normalize();
        if axis.z < -1e-9
            || (axis.z.abs() <= 1e-9 && (axis.x < -1e-9 || (axis.x.abs() <= 1e-9 && axis.y < 0.0)))
        {
            axis = -axis;
        }
        let pts = self.face_outline(face, 0.05)?;
        // the radius at each height from the outline points; the ends are the lowest and the highest points
        let mut lo = (f64::MAX, 0.0);
        let mut hi = (f64::MIN, 0.0);
        for p in pts.iter().flatten() {
            let v = p.as_dvec3() - info.origin;
            let t = v.dot(axis);
            let r = (v - axis * t).length();
            if t < lo.0 {
                lo = (t, r);
            }
            if t > hi.0 {
                hi = (t, r);
            }
        }
        if lo.0 >= hi.0 {
            return Ok(None);
        }
        let height = hi.0 - lo.0;
        let slope = (hi.1 - lo.1) / height;
        Ok(Some(ConeInfo {
            axis,
            base: info.origin + axis * lo.0,
            height,
            r_base: lo.1,
            r_top: hi.1,
            half_angle_deg: slope.atan().to_degrees(),
        }))
    }

    /// Describes a full cylindrical face; `None` for partial cylinders (fillets) and other faces.
    pub fn cylinder_info(&self, face: FaceId) -> Result<Option<CylinderInfo>> {
        let info = self.face_info(face)?;
        if info.kind != FaceKind::Cylinder {
            return Ok(None);
        }
        let axis = info.normal.normalize();
        let pts = self.face_outline(face, 0.05)?;
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for p in pts.iter().flatten() {
            let t = (p.as_dvec3() - info.origin).dot(axis);
            lo = lo.min(t);
            hi = hi.max(t);
        }
        if lo >= hi {
            return Ok(None);
        }
        let height = hi - lo;
        let full =
            (info.area - std::f64::consts::TAU * info.radius * height).abs() < 1e-4 * info.area.max(1.0);
        if !full {
            return Ok(None);
        }
        let base = info.origin + axis * lo;
        let mid = base + axis * (height * 0.5);
        let is_hole = !self.contains_point(mid)?;
        Ok(Some(CylinderInfo {
            axis,
            base,
            radius: info.radius,
            height,
            is_hole,
        }))
    }

    /// For a flat circular face (a disc without holes) bounded by one circle that is also the edge of a
    /// full cylindrical face: that cylindrical face and the disc radius.
    pub fn disc_partner(&self, face: FaceId) -> Result<Option<(FaceId, f64)>> {
        let info = self.face_info(face)?;
        if info.kind != FaceKind::Plane || info.inner_wires != 0 {
            return Ok(None);
        }
        let edges = self.face_edges(face)?;
        if edges.len() != 1 {
            return Ok(None);
        }
        let e = self.edge_info(edges[0])?;
        if e.kind != EdgeKind::Circle {
            return Ok(None);
        }
        for f in self.faces() {
            if f != face && self.face_edges(f)?.contains(&edges[0]) && self.cylinder_info(f)?.is_some() {
                return Ok(Some((f, e.radius)));
            }
        }
        Ok(None)
    }

    /// Rebuilds a hole or pin with a new diameter. Fails (leaving the solid untouched) if the result is not valid.
    pub fn set_cylinder_diameter(&self, face: FaceId, diameter: f64) -> Result<OpResult> {
        if !(diameter.is_finite() && diameter > 1e-6) {
            return Err(KernelError::Invalid(
                "the diameter must be a positive number".into(),
            ));
        }
        let c = self
            .cylinder_info(face)?
            .ok_or_else(|| KernelError::Invalid("only a complete hole or pin can be resized".into()))?;
        let new_r = diameter / 2.0;
        if (new_r - c.radius).abs() < 1e-9 {
            return Err(KernelError::ZeroDistance);
        }
        let before = self.volume()?;
        let cyl = |r: f64| Solid::cylinder(r, c.height, c.base, c.axis);
        let x = if c.axis.cross(DVec3::Z).length() > 1e-6 {
            c.axis.cross(DVec3::Z)
        } else {
            DVec3::X
        };
        let circle = |r: f64| ProfileSegment::Circle {
            center: [0.0, 0.0],
            radius: r,
        };
        let res = if c.is_hole {
            if new_r < c.radius {
                let filled = self.boolean(&cyl(c.radius)?, BooleanOp::Fuse)?;
                let cut = filled.solid.boolean(&cyl(new_r)?, BooleanOp::Cut)?;
                OpResult {
                    history: filled.history.compose(&cut.history),
                    solid: cut.solid,
                }
            } else {
                self.boolean(&cyl(new_r)?, BooleanOp::Cut)?
            }
        } else if new_r > c.radius {
            self.boolean(&cyl(new_r)?, BooleanOp::Fuse)?
        } else {
            let tube = Profile {
                loops: vec![vec![circle(c.radius)], vec![circle(new_r)]],
            };
            let tool = Solid::extrude_profile(&tube, c.base, c.axis, x, c.height)?;
            self.boolean(&tool, BooleanOp::Cut)?
        };
        let after = res.solid.volume()?;
        let grew = new_r > c.radius;
        let ok = if c.is_hole == grew {
            after < before
        } else {
            after > before
        };
        if !ok {
            return Err(KernelError::Invalid(
                "the resized shape does not fit the model".into(),
            ));
        }
        Ok(res)
    }
}

/// The volume of a result lies where the operation allows it: a merge between the larger input and the sum, a cut between
/// zero and the target, a common part between zero and the smaller input.
pub fn op_volume_ok(op: BooleanOp, target: f64, tool: f64, result: f64, tol: f64) -> bool {
    if !result.is_finite() {
        return false;
    }
    match op {
        BooleanOp::Fuse => result >= target.max(tool) - tol && result <= target + tool + tol,
        BooleanOp::Cut => result > 0.0 && result <= target + tol,
        BooleanOp::Common => result > 0.0 && result <= target.min(tool) + tol,
    }
}
