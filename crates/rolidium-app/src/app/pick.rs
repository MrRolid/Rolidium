// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Picking of edges (solids) and shapes (sketches) by distance to the cursor in screen space.

use super::*;
use crate::sketchmode::SnapLabel;
use rolidium_doc::rolidium_sketch::{EntityId, PlaneDef};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Obj {
    Entity(SketchId, EntityId),
    Edge(BodyId, u32),
    Curve(rolidium_doc::CurveId),
}

/// Sorts candidates `(sort key, distance, object, point)` so the one to take first is at the front.
pub(super) fn rank_candidates(found: &mut [(f32, f32, Obj, Vec3)]) {
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
}

/// The index to take for a click at `pointer` among `len` candidates: the first one, and when the same place (within 3 pixels) is
/// clicked again, the one after the previously taken.
pub(super) fn cycle_index(previous: Option<(Pos2, usize)>, pointer: Pos2, len: usize) -> usize {
    match previous {
        Some((p, i)) if len > 0 && (p - pointer).length() <= 3.0 => (i + 1) % len,
        _ => 0,
    }
}

pub(super) fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> (f32, f32) {
    let ab = b - a;
    let l2 = ab.length_sq();
    let t = if l2 < 1e-9 {
        0.0
    } else {
        ((p - a).dot(ab) / l2).clamp(0.0, 1.0)
    };
    let q = a + ab * t;
    ((p - q).length(), t)
}

/// The point where the ray meets a face of a mesh, and the (outward) normal of the surface there.
pub(super) fn ray_mesh_face_hit(
    m: &rolidium_kernel::Mesh,
    face: u32,
    o: Vec3,
    d: Vec3,
) -> Option<(DVec3, DVec3)> {
    let mut best: Option<(f32, DVec3)> = None;
    for (t, tri) in m.indices.as_chunks::<3>().0.iter().enumerate() {
        if m.tri_face.get(t) != Some(&face) {
            continue;
        }
        let p = |i: u32| m.positions[i as usize];
        if let Some(h) = ray_triangle(o, d, p(tri[0]), p(tri[1]), p(tri[2])) {
            if best.is_none_or(|(b, _)| h < b) {
                let n =
                    (m.normals[tri[0] as usize] + m.normals[tri[1] as usize] + m.normals[tri[2] as usize])
                        .normalize_or_zero();
                best = Some((h, n.as_dvec3()));
            }
        }
    }
    best.map(|(h, n)| ((o + d * h).as_dvec3(), n))
}

/// Möller–Trumbore: distance along the ray to the triangle, if hit.
pub(super) fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let t = o - a;
    let u = t.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let dist = e2.dot(q) * inv;
    (dist > 0.0).then_some(dist)
}

impl App {
    /// Nearest edge or sketch shape within a few pixels of the pointer.
    /// `edges`/`entities` select which kinds may be picked.
    pub(super) fn pick_object(&self, pointer: Pos2, rect: Rect, edges: bool, entities: bool) -> Option<Obj> {
        self.pick_object_d(pointer, rect, edges, entities).map(|(o, _)| o)
    }

    /// Like [`Self::pick_object`], also giving the distance to the object in pixels.
    pub(super) fn pick_object_d(
        &self,
        pointer: Pos2,
        rect: Rect,
        edges: bool,
        entities: bool,
    ) -> Option<(Obj, f32)> {
        self.pick_candidates(pointer, rect, edges, entities)
            .into_iter()
            .next()
    }

    /// Everything close to the pointer, the one to take first at the front. A curve or a sketch shape wins over a body edge
    /// lying in the same place (it is what the person drew; the edge of a body is always there), so the sort uses the
    /// distance in pixels minus a few pixels for them. Clicking the same place again takes the next one
    /// ([`Self::pick_cycled`]).
    pub(super) fn pick_candidates(
        &self,
        pointer: Pos2,
        rect: Rect,
        edges: bool,
        entities: bool,
    ) -> Vec<(Obj, f32)> {
        const DRAWN_BONUS: f32 = 4.0;
        let near = |pts: &mut dyn Iterator<Item = (Option<Pos2>, Option<Pos2>)>,
                    limit: f32|
         -> Option<(f32, f32, usize)> {
            let mut best: Option<(f32, f32, usize)> = None;
            for (i, (a, b)) in pts.enumerate() {
                if let (Some(a), Some(b)) = (a, b) {
                    let (d, t) = dist_to_segment(pointer, a, b);
                    if d <= limit && best.is_none_or(|(bd, _, _)| d < bd) {
                        best = Some((d, t, i));
                    }
                }
            }
            best
        };
        // (sort key, distance, object, point on a body edge)
        let mut found: Vec<(f32, f32, Obj, Vec3)> = Vec::new();
        if entities {
            for s in self.doc.sketches().iter().filter(|s| s.visible) {
                for (id, e) in s.sketch.entities() {
                    let mut best: Option<f32> = None;
                    for pl in e.polylines() {
                        let pts: Vec<Option<Pos2>> = pl
                            .iter()
                            .map(|p| self.project(s.plane.to_3d(*p).as_vec3(), rect))
                            .collect();
                        let mut it = pts.windows(2).map(|w| (w[0], w[1]));
                        if let Some((d, _, _)) = near(&mut it, 7.0) {
                            best = Some(best.map_or(d, |b: f32| b.min(d)));
                        }
                    }
                    if let Some(d) = best {
                        found.push((d - DRAWN_BONUS, d, Obj::Entity(s.id, *id), Vec3::ZERO));
                    }
                }
            }
            for (id, pts) in &self.curve_cache.lines {
                if !self.doc.curve(*id).is_some_and(|c| c.visible) {
                    continue;
                }
                let proj: Vec<Option<Pos2>> = pts.iter().map(|p| self.project(*p, rect)).collect();
                let mut it = proj.windows(2).map(|w| (w[0], w[1]));
                if let Some((d, _, _)) = near(&mut it, 7.0) {
                    found.push((d - DRAWN_BONUS, d, Obj::Curve(*id), Vec3::ZERO));
                }
            }
        }
        if edges {
            for m in self.meshes.iter() {
                let boxes = self.pick_cache.borrow_mut().edge_boxes(m.stamp, &m.mesh);
                for (e, (lo, hi)) in m.mesh.edges.iter().zip(boxes.iter()) {
                    // an edge whose box is far from the pointer on the screen is not looked at
                    if !self.box_near_pointer(*lo, *hi, pointer, rect, 8.0) {
                        continue;
                    }
                    let proj: Vec<Option<Pos2>> = e.points.iter().map(|p| self.project(*p, rect)).collect();
                    let mut it = proj.windows(2).map(|w| (w[0], w[1]));
                    if let Some((d, t, i)) = near(&mut it, 6.0) {
                        let pt = e.points[i].lerp(e.points[i + 1], t);
                        found.push((d, d, Obj::Edge(m.id, e.edge.0), pt));
                    }
                }
            }
        }
        rank_candidates(&mut found);
        found
            .into_iter()
            .filter(|(_, _, o, p)| !matches!(o, Obj::Edge(..)) || !self.edge_point_hidden(*p, pointer, rect))
            .map(|(_, d, o, _)| (o, d))
            .collect()
    }

    /// The candidate to take for a click: the first one, and when the same place is clicked again, the next one.
    pub(super) fn pick_cycled(
        &mut self,
        pointer: Pos2,
        rect: Rect,
        edges: bool,
        entities: bool,
    ) -> Option<Obj> {
        let c = self.pick_candidates(pointer, rect, edges, entities);
        if c.is_empty() {
            self.pick_cycle = None;
            return None;
        }
        let next = cycle_index(self.pick_cycle, pointer, c.len());
        self.pick_cycle = Some((pointer, next));
        Some(c[next].0)
    }

    /// True if the screen box of the 3D box `lo`..`hi`, widened by `margin` pixels, holds the pointer (always true when a corner is not on the screen).
    fn box_near_pointer(&self, lo: Vec3, hi: Vec3, pointer: Pos2, rect: Rect, margin: f32) -> bool {
        let (mut min, mut max) = (Pos2::new(f32::MAX, f32::MAX), Pos2::new(f32::MIN, f32::MIN));
        for i in 0..8 {
            let c = Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            let Some(p) = self.project(c, rect) else {
                return true;
            };
            min = Pos2::new(min.x.min(p.x), min.y.min(p.y));
            max = Pos2::new(max.x.max(p.x), max.y.max(p.y));
        }
        pointer.x >= min.x - margin
            && pointer.x <= max.x + margin
            && pointer.y >= min.y - margin
            && pointer.y <= max.y + margin
    }

    /// True if a body surface lies between the camera and the point. The triangles of every body are searched through a hierarchy of boxes
    /// that is made once per mesh on another thread; while it is not ready nothing is taken for hidden.
    fn edge_point_hidden(&self, point: Vec3, _pointer: Pos2, _rect: Rect) -> bool {
        let eye = self.camera.eye();
        let to = point - eye;
        let dist = to.length();
        if dist < 1e-9 {
            return false;
        }
        let dir = to / dist;
        let eps = dist * 1e-3 + 0.05;
        for m in self.meshes.iter() {
            let ready = self.pick_cache.borrow_mut().bvh(m.stamp, &m.mesh);
            if let Some(slot) = ready {
                if slot.get().is_some_and(|b| b.hit_before(eye, dir, dist - eps)) {
                    return true;
                }
            }
        }
        false
    }

    /// Edges (besides `edges`) that a round or chamfer of `edges` also treats: the kernel always follows
    /// edges that continue each other tangentially. Cached per document revision.
    /// Do the edges lie where another, separate body touches this one (a joint of two bodies that are not combined)?
    pub(super) fn edges_at_body_joint(&self, body: BodyId, edges: &[u32]) -> bool {
        let key = (self.doc.revision(), body, edges.to_vec());
        if let Some((k, v)) = &*self.joint_cache.borrow() {
            if *k == key {
                return *v;
            }
        }
        let others: Vec<&rolidium_doc::Body> = self
            .doc
            .bodies()
            .iter()
            .filter(|b| b.id != body && b.visible)
            .collect();
        let found = !others.is_empty()
            && edges.iter().any(|e| {
                let Some(pl) = self.edge_polyline(body, *e) else {
                    return false;
                };
                let mid = pl[pl.len() / 2].as_dvec3();
                let Ok(probe) =
                    rolidium_kernel::Solid::cuboid(mid - DVec3::splat(1e-3), mid + DVec3::splat(1e-3))
                else {
                    return false;
                };
                others
                    .iter()
                    .any(|o| o.solid.distance_to(&probe).is_ok_and(|d| d < 2e-3))
            });
        *self.joint_cache.borrow_mut() = Some((key, found));
        found
    }

    pub(super) fn chain_extra(&self, body: BodyId, edges: &[u32]) -> Vec<u32> {
        let key = (self.doc.revision(), body, edges.to_vec(), self.edge_chamfer);
        if let Some(v) = self.chain_cache.borrow().get(&key) {
            return v.clone();
        }
        let ids: Vec<rolidium_kernel::EdgeId> = edges.iter().map(|e| rolidium_kernel::EdgeId(*e)).collect();
        let combined = self.doc.body(body).is_some_and(|b| {
            let inputs: Vec<rolidium_kernel::BlendInput> = b
                .features
                .iter()
                .map(|f| rolidium_kernel::BlendInput {
                    faces: f.faces.clone(),
                    size: f.size,
                    chamfer: f.kind == rolidium_doc::BlendKind::Chamfer,
                })
                .collect();
            b.solid
                .chain_touches_blends(&inputs, &ids, self.edge_chamfer)
                .unwrap_or(false)
        });
        // When the chain runs through existing rounds they are redone together with the new one, so the
        // edges beyond them stay sharp: nothing extra to warn about.
        let extra: Vec<u32> = self
            .doc
            .body(body)
            .filter(|_| !combined)
            .and_then(|b| b.solid.blend_chain(&ids, self.edge_chamfer).ok())
            .map(|v| {
                v.into_iter()
                    .map(|e| e.0)
                    .filter(|e| !edges.contains(e))
                    .collect()
            })
            .unwrap_or_default();
        let mut c = self.chain_cache.borrow_mut();
        if c.len() > 256 {
            c.clear();
        }
        c.insert(key, extra.clone());
        extra
    }

    /// Aids for placing a sketch point: points of 3D features projected into the plane, mirrored helper
    /// points, guide lines, extensions, crossings, parallel/perpendicular. `base` is the plain snap.
    pub(super) fn apply_aids(
        &mut self,
        base: Snap,
        raw: DVec2,
        mpp: f64,
        pointer: Option<Pos2>,
        rect: Rect,
        points: &[(DVec2, SnapKind)],
    ) -> Snap {
        use crate::sketchguides::{guided_snap, GuideInput};
        let Some(sm) = &self.sk else { return base };
        let plane = sm.plane;
        // lines and reference points of the sketch and the face
        let mut segments: Vec<(DVec2, DVec2)> = sm.outline.clone();
        let mut centres: Vec<DVec2> = Vec::new();
        for s in self
            .doc
            .sketches()
            .iter()
            .filter(|s| s.visible && s.plane.same_plane(&plane) && s.target == sm.body)
        {
            for (_, e) in s.sketch.entities() {
                match *e {
                    Entity::Line { a, b } | Entity::Axis { a, b } => segments.push((a, b)),
                    Entity::Rect { a, b } => {
                        let c = Entity::rect_corners(a, b);
                        for i in 0..4 {
                            segments.push((c[i], c[(i + 1) % 4]));
                        }
                        centres.push((a + b) * 0.5);
                    }
                    Entity::Circle { center, .. } | Entity::Arc { center, .. } => centres.push(center),
                }
            }
        }
        let mut refs: Vec<DVec2> = points.iter().map(|(p, _)| *p).collect();
        refs.extend(sm.corners.iter().copied());
        // projections of the vertices of all bodies
        let mut n3 = 0;
        for m in self.meshes.iter() {
            for e in &m.mesh.edges {
                for p in [e.points.first(), e.points.last()].into_iter().flatten() {
                    if n3 < 4000 {
                        refs.push(plane.to_2d(p.as_dvec3()));
                        n3 += 1;
                    }
                }
            }
        }
        // mirrored helper points of a rectangular face
        let helpers = match crate::sketchguides::symmetry_axes(&sm.outline) {
            Some((c, fx, fy)) => crate::sketchguides::mirrored_over(&centres, c, fx, fy),
            None => Vec::new(),
        };
        let line_start = sm
            .drawing
            .as_ref()
            .filter(|_| sm.tool == SkTool::Line)
            .map(|d| d.start);
        let mut result = base;
        let mut guides = Vec::new();
        let mut real3d = None;
        let keep = matches!(
            base.label,
            SnapLabel::Endpoint | SnapLabel::Midpoint | SnapLabel::Center | SnapLabel::FaceVertex
        );
        if let Some(g) = guided_snap(&GuideInput {
            raw,
            mm_per_px: mpp,
            refs: &refs,
            segments: &segments,
            line_start,
            helpers: &helpers,
        }) {
            if !keep {
                result = Snap {
                    pos: g.pos,
                    label: g.label,
                    horizontal: false,
                    vertical: false,
                };
                guides = g.guides;
            }
        }
        // an arc that continues a line or an arc tangentially
        if let (SkTool::Arc, Some(d)) = (sm.tool, sm.drawing.as_ref()) {
            if let Some(end) = sm.arc_end {
                let ents: Vec<Entity> = self
                    .doc
                    .sketches()
                    .iter()
                    .filter(|s| s.visible && s.plane.same_plane(&plane) && s.target == sm.body)
                    .flat_map(|s| s.sketch.entities().iter().map(|(_, e)| *e))
                    .collect();
                for t in crate::sketchguides::tangent_dirs(d.start, &ents) {
                    if let Some(m) = crate::sketchguides::tangent_arc_mid(d.start, t, end) {
                        if m.distance(raw) <= 12.0 * mpp {
                            result = Snap {
                                pos: m,
                                label: SnapLabel::Tangent,
                                horizontal: false,
                                vertical: false,
                            };
                            guides.clear();
                        }
                    }
                }
            }
        }
        // a 3D feature under the cursor wins over everything
        if let Some((p2, label, p3)) = pointer.and_then(|p| self.feature3d_snap(p, rect, &plane)) {
            result = Snap {
                pos: p2,
                label,
                horizontal: false,
                vertical: false,
            };
            real3d = Some(p3);
            guides.clear();
        }
        // reference of the distance fields: the last point snapped to, else the nearest face vertex
        let is_point = matches!(
            result.label,
            SnapLabel::Endpoint
                | SnapLabel::Midpoint
                | SnapLabel::Center
                | SnapLabel::FaceVertex
                | SnapLabel::Vertex3D
                | SnapLabel::Mid3D
                | SnapLabel::Center3D
        );
        if let Some(sm) = &mut self.sk {
            sm.helpers = helpers;
            sm.guides = guides;
            sm.real3d = real3d;
            if is_point {
                sm.ref_point = Some(result.pos);
            } else if sm.ref_point.is_none() {
                sm.ref_point = sm
                    .corners
                    .iter()
                    .min_by(|a, b| a.distance(raw).total_cmp(&b.distance(raw)))
                    .copied();
            }
        }
        result
    }

    /// The vertex, edge midpoint, circle centre or point on an edge (at grid steps) of any body that the
    /// cursor is over in the 3D view, projected into the sketch plane.
    pub(super) fn feature3d_snap(
        &self,
        pointer: Pos2,
        rect: Rect,
        plane: &PlaneDef,
    ) -> Option<(DVec2, SnapLabel, Vec3)> {
        let mut best: Option<(f32, SnapLabel, Vec3)> = None;
        let mut on_edge: Option<(f32, Vec3)> = None;
        let step = self.settings.grid_step as f32;
        for m in self.meshes.iter() {
            for e in &m.mesh.edges {
                let pts = &e.points;
                if pts.len() < 2 {
                    continue;
                }
                let mut cands: Vec<(Vec3, SnapLabel)> = vec![
                    (pts[0], SnapLabel::Vertex3D),
                    (pts[pts.len() - 1], SnapLabel::Vertex3D),
                ];
                // arc length position of the middle
                let total: f32 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
                let mut acc = 0.0;
                let mut mid = pts[0];
                for w in pts.windows(2) {
                    let l = (w[1] - w[0]).length();
                    if acc + l >= total * 0.5 {
                        mid = w[0].lerp(w[1], ((total * 0.5 - acc) / l.max(1e-9)).clamp(0.0, 1.0));
                        break;
                    }
                    acc += l;
                }
                cands.push((mid, SnapLabel::Mid3D));
                // circular edge: its centre
                if pts.len() >= 8 {
                    let n = pts.len();
                    let closed = (pts[0] - pts[n - 1]).length() < 1e-3;
                    let idx = if closed {
                        [0, n / 3, 2 * n / 3]
                    } else {
                        [0, n / 2, n - 1]
                    };
                    let f = |p: Vec3| [p.x as f64, p.y as f64, p.z as f64];
                    if let Some(c) =
                        crate::sketchguides::circle_centre(f(pts[idx[0]]), f(pts[idx[1]]), f(pts[idx[2]]))
                    {
                        let c = Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32);
                        let r0 = (pts[idx[0]] - c).length();
                        if r0 > 1e-3
                            && pts
                                .iter()
                                .all(|p| ((*p - c).length() - r0).abs() < r0 * 0.01 + 1e-3)
                        {
                            cands.push((c, SnapLabel::Center3D));
                        }
                    }
                }
                for (p, label) in cands {
                    if let Some(sp) = self.project(p, rect) {
                        let d = (sp - pointer).length();
                        if d <= 10.0 && best.is_none_or(|(bd, _, _)| d < bd) {
                            best = Some((d, label, p));
                        }
                    }
                }
                // a point on the edge, at grid steps from its start
                let mut s = 0.0;
                for w in pts.windows(2) {
                    let (Some(a), Some(b)) = (self.project(w[0], rect), self.project(w[1], rect)) else {
                        s += (w[1] - w[0]).length();
                        continue;
                    };
                    let (d, t) = dist_to_segment(pointer, a, b);
                    if d <= 6.0 && on_edge.is_none_or(|(bd, _)| d < bd) {
                        let seg = (w[1] - w[0]).length();
                        let along = s + seg * t;
                        let q = if step > 0.0 {
                            ((along / step).round() * step).clamp(0.0, total)
                        } else {
                            along
                        };
                        // walk back to the point at arc length q
                        let mut acc = 0.0;
                        let mut p = pts[0];
                        for ww in pts.windows(2) {
                            let l = (ww[1] - ww[0]).length();
                            if acc + l >= q {
                                p = ww[0].lerp(ww[1], ((q - acc) / l.max(1e-9)).clamp(0.0, 1.0));
                                break;
                            }
                            acc += l;
                            p = ww[1];
                        }
                        on_edge = Some((d, p));
                    }
                    s += (w[1] - w[0]).length();
                }
            }
        }
        let (label, p3) = match (best, on_edge) {
            (Some((_, l, p)), _) => (l, p),
            (None, Some((_, p))) => (SnapLabel::Edge3D, p),
            _ => return None,
        };
        if self.point_hidden(p3) {
            return None;
        }
        // points in the sketch plane itself are covered by the ordinary snaps
        if (p3.as_dvec3() - plane.origin).dot(plane.normal).abs() < 1e-4 {
            return None;
        }
        Some((plane.to_2d(p3.as_dvec3()), label, p3))
    }

    pub(super) fn point_hidden(&self, p: Vec3) -> bool {
        self.edge_point_hidden(p, Pos2::ZERO, Rect::NOTHING)
    }

    /// Where a ray meets the triangles of one face (nearest hit).
    pub(super) fn ray_face_point(&self, body: BodyId, face: u32, o: Vec3, d: Vec3) -> Option<DVec3> {
        let m = self.entry(body)?;
        let mut best: Option<f32> = None;
        for (t, tri) in m.mesh.indices.as_chunks::<3>().0.iter().enumerate() {
            if m.mesh.tri_face.get(t) != Some(&face) {
                continue;
            }
            let p = |i: u32| m.mesh.positions[i as usize];
            if let Some(h) = ray_triangle(o, d, p(tri[0]), p(tri[1]), p(tri[2])) {
                if best.is_none_or(|b| h < b) {
                    best = Some(h);
                }
            }
        }
        best.map(|h| (o + d * h).as_dvec3())
    }

    /// The point where the ray meets a face, and the (outward) normal of the surface there.
    pub(super) fn ray_face_hit(&self, body: BodyId, face: u32, o: Vec3, d: Vec3) -> Option<(DVec3, DVec3)> {
        ray_mesh_face_hit(&self.entry(body)?.mesh, face, o, d)
    }

    pub(super) fn edge_polyline(&self, body: BodyId, edge: u32) -> Option<&[Vec3]> {
        self.entry(body)?
            .mesh
            .edges
            .iter()
            .find(|e| e.edge.0 == edge)
            .map(|e| &e.points[..])
    }

    pub(super) fn clear_selection(&mut self) {
        self.selection.clear();
        self.region_sel = None;
        self.entity_sel = None;
        self.edge_sel.clear();
        self.body_sel = None;
        self.sketch_sel = None;
        self.curve_sel = None;
        self.move_curves.clear();
        self.region_multi.clear();
        self.sel_order.clear();
        self.move_sel.clear();
        self.layers_changed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAWN_BONUS: f32 = 4.0;

    #[test]
    fn a_drawn_curve_wins_over_a_body_edge_in_the_same_place() {
        // the curve is 3 px from the pointer and the edge 2 px: the curve still comes first (bonus of a few pixels)
        let mut found = vec![
            (2.0, 2.0, Obj::Edge(1, 5), Vec3::ZERO),
            (3.0 - DRAWN_BONUS, 3.0, Obj::Curve(7), Vec3::ZERO),
        ];
        rank_candidates(&mut found);
        assert_eq!(found[0].2, Obj::Curve(7));
        assert_eq!(found[1].2, Obj::Edge(1, 5));
        // an edge that is clearly closer than the curve (curve 7 px away) wins
        let mut found = vec![
            (7.0 - DRAWN_BONUS, 7.0, Obj::Curve(7), Vec3::ZERO),
            (1.0, 1.0, Obj::Edge(1, 5), Vec3::ZERO),
        ];
        rank_candidates(&mut found);
        assert_eq!(found[0].2, Obj::Edge(1, 5));
    }

    #[test]
    fn repeated_clicks_cycle_through_the_candidates() {
        let at = Pos2::new(100.0, 100.0);
        let mut prev = None;
        let mut taken = Vec::new();
        for _ in 0..5 {
            let i = cycle_index(prev, at, 3);
            taken.push(i);
            prev = Some((at, i));
        }
        assert_eq!(taken, vec![0, 1, 2, 0, 1]);
        // a click somewhere else starts again at the first candidate
        assert_eq!(cycle_index(prev, Pos2::new(200.0, 100.0), 3), 0);
        // a click within 3 pixels counts as the same place
        assert_eq!(cycle_index(Some((at, 0)), Pos2::new(102.0, 101.0), 3), 1);
        // nothing to cycle through
        assert_eq!(cycle_index(Some((at, 0)), at, 0), 0);
    }
}
