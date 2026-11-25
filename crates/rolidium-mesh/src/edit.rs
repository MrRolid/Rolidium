// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Edits made on the mesh itself: a planar wall moves along its normal and the neighbouring walls stretch (their triangles share the vertices that
//! moved), the vertices of a round hole or pin move radially. Every edit gives a short record of the vertices it moved; applying the record
//! restores them (and gives the record of the way back), so Undo and Redo never copy the mesh.

use crate::regions::{Cylinder, Plane};
use crate::{add, cross, dot, len, mul, sub, TriMesh, V3};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EditError {
    #[error("A neighbouring wall is not square to this wall (a slope or a bevel), so it cannot be moved without bending the mesh. Tick “Move anyway, the neighbouring walls slant” in the panel to allow it")]
    NeighbourNotSquare,
    #[error("The move would turn a neighbouring wall inside out (the wall would be pulled through the opposite one)")]
    Collapses,
    #[error("The mesh is not closed or not consistent: repair it first")]
    NotClean,
    #[error("The move would make the mesh cut through itself")]
    SelfIntersects,
    #[error("The diameter must be a positive number")]
    BadDiameter,
    #[error("The hole is not round enough to change its diameter")]
    NotRound,
}

/// The vertices an edit moved and where they were.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Undo {
    pub moved: Vec<(u32, [f32; 3])>,
}

impl Undo {
    /// Puts the vertices back; the result is the record that takes the edit again.
    pub fn apply(&self, m: &mut TriMesh) -> Undo {
        let mut back = Undo::default();
        for (i, p) in &self.moved {
            back.moved.push((*i, m.positions[*i as usize]));
            m.positions[*i as usize] = *p;
        }
        back
    }
}

/// The vertices of a region, and the triangles outside it that touch one of them.
fn surroundings(m: &TriMesh, region: &[u32]) -> (Vec<u32>, Vec<u32>) {
    let mut is_vertex = vec![false; m.positions.len()];
    let mut verts: Vec<u32> = Vec::new();
    for &t in region {
        for &v in &m.triangles[t as usize] {
            if !is_vertex[v as usize] {
                is_vertex[v as usize] = true;
                verts.push(v);
            }
        }
    }
    let mut inside = vec![false; m.triangles.len()];
    for &t in region {
        inside[t as usize] = true;
    }
    let outside: Vec<u32> = m
        .triangles
        .iter()
        .enumerate()
        .filter(|(t, tri)| !inside[*t] && tri.iter().any(|v| is_vertex[*v as usize]))
        .map(|(t, _)| t as u32)
        .collect();
    (verts, outside)
}

fn check_closed(m: &TriMesh) -> Result<(), EditError> {
    let r = crate::check::check(m);
    if r.is_watertight() {
        Ok(())
    } else {
        Err(EditError::NotClean)
    }
}

/// Moves a planar wall by `distance` along its (outward) normal. The walls that touch it must be square to it; they stretch.
pub fn pull_plane(m: &mut TriMesh, wall: &Plane, distance: f64) -> Result<Undo, EditError> {
    pull_plane_with(m, wall, distance, true)
}

/// The same; `verify` is false when the mesh was checked before (an edit keeps it closed, so a program checks once, not at every edit).
pub fn pull_plane_with(
    m: &mut TriMesh,
    wall: &Plane,
    distance: f64,
    verify: bool,
) -> Result<Undo, EditError> {
    pull_plane_opts(m, wall, distance, verify, false)
}

/// The same; with `tilt` the walls that touch the wall need not be square to it: the vertices of the wall move and the triangles of the neighbours are
/// stretched, so that they slant. The mesh must not cut through itself afterwards, or nothing is done.
pub fn pull_plane_opts(
    m: &mut TriMesh,
    wall: &Plane,
    distance: f64,
    verify: bool,
    tilt: bool,
) -> Result<Undo, EditError> {
    if verify {
        check_closed(m)?;
    }
    let plan = PullPlan::new(m, wall);
    plan.check(m, wall, distance, tilt)?;
    let mut undo = Undo::default();
    for v in &plan.verts {
        undo.moved.push((*v, m.positions[*v as usize]));
        let p = m.vertex(*v);
        m.set_vertex(*v, add(p, mul(wall.normal, distance)));
    }
    Ok(undo)
}

/// What a pull of a wall touches, found once: the vertices that move and the triangles around them. The plan answers "can it be done by this much"
/// without changing the mesh, so a program can ask while the person drags.
#[derive(Debug, Clone)]
pub struct PullPlan {
    /// The vertices of the wall (sorted).
    pub verts: Vec<u32>,
    /// The triangles outside the wall that touch it.
    pub outside: Vec<u32>,
    before: Vec<V3>,
    /// Which vertices of the mesh are those of the wall.
    flag: Vec<bool>,
}

impl PullPlan {
    pub fn new(m: &TriMesh, wall: &Plane) -> PullPlan {
        let (mut verts, outside) = surroundings(m, &wall.triangles);
        verts.sort_unstable();
        let before = outside.iter().map(|t| m.normal_area(*t as usize).0).collect();
        let mut flag = vec![false; m.positions.len()];
        for v in &verts {
            flag[*v as usize] = true;
        }
        PullPlan {
            verts,
            outside,
            before,
            flag,
        }
    }

    /// Every neighbouring wall is square to the wall: it can be pulled without bending anything.
    pub fn neighbours_square(&self, wall: &Plane) -> bool {
        self.before.iter().all(|nn| dot(*nn, wall.normal).abs() <= 0.12)
    }

    /// Whether the pull by `distance` can be made, and if not, why.
    pub fn check(&self, m: &TriMesh, wall: &Plane, distance: f64, tilt: bool) -> Result<(), EditError> {
        if !tilt && !self.neighbours_square(wall) {
            return Err(EditError::NeighbourNotSquare);
        }
        let d = mul(wall.normal, distance);
        let moved = |v: u32| self.flag[v as usize];
        let pos = |v: u32| {
            let p = m.vertex(v);
            if moved(v) {
                add(p, d)
            } else {
                p
            }
        };
        // no wall may be turned over by the move
        let keep = if tilt { 0.1 } else { 0.9 };
        for (k, t) in self.outside.iter().enumerate() {
            let tri = m.triangles[*t as usize];
            let (a, b, c) = (pos(tri[0]), pos(tri[1]), pos(tri[2]));
            let nn = cross(sub(b, a), sub(c, a));
            let area2 = len(nn);
            if area2 > 1e-12 && dot(mul(nn, 1.0 / area2), self.before[k]) < keep {
                return Err(EditError::Collapses);
            }
        }
        if tilt && self.intersects(m, wall, d) {
            return Err(EditError::SelfIntersects);
        }
        Ok(())
    }

    /// After the move, does a triangle that moved or stretched cut through one that did not?
    fn intersects(&self, m: &TriMesh, wall: &Plane, d: V3) -> bool {
        let n = m.triangles.len();
        let mut changed = vec![false; n];
        for t in wall.triangles.iter().chain(&self.outside) {
            changed[*t as usize] = true;
        }
        let moved = |v: u32| self.flag[v as usize];
        let pos = |v: u32| {
            let p = m.vertex(v);
            if moved(v) {
                add(p, d)
            } else {
                p
            }
        };
        let after = |t: usize| {
            let tri = m.triangles[t];
            [pos(tri[0]), pos(tri[1]), pos(tri[2])]
        };
        let boxed = |c: &[V3; 3]| {
            let mut lo = c[0];
            let mut hi = c[0];
            for p in c {
                for i in 0..3 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
            (lo, hi)
        };
        // the box the changed triangles occupy, before and after
        let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
        for t in (0..n).filter(|t| changed[*t]) {
            let before = m.corners(t);
            for c in [before, after(t)] {
                let (l, h) = boxed(&c);
                for i in 0..3 {
                    lo[i] = lo[i].min(l[i]);
                    hi[i] = hi[i].max(h[i]);
                }
            }
        }
        // the triangles that stay and lie in that box, in a grid
        let mut still: Vec<u32> = Vec::new();
        for t in (0..n).filter(|t| !changed[*t]) {
            let (l, h) = boxed(&m.corners(t));
            if (0..3).all(|i| h[i] >= lo[i] && l[i] <= hi[i]) {
                still.push(t as u32);
            }
        }
        if still.is_empty() {
            return false;
        }
        let side = ((still.len() as f64).cbrt().ceil() as usize).clamp(1, 64);
        let ext = [
            (hi[0] - lo[0]).max(1e-9),
            (hi[1] - lo[1]).max(1e-9),
            (hi[2] - lo[2]).max(1e-9),
        ];
        let cell_of = |p: V3| -> [usize; 3] {
            let mut c = [0usize; 3];
            for i in 0..3 {
                c[i] = (((p[i] - lo[i]) / ext[i] * side as f64).floor().max(0.0) as usize).min(side - 1);
            }
            c
        };
        let mut grid: Vec<Vec<u32>> = vec![Vec::new(); side * side * side];
        for &t in &still {
            let (l, h) = boxed(&m.corners(t as usize));
            let (a, b) = (cell_of(l), cell_of(h));
            for x in a[0]..=b[0] {
                for y in a[1]..=b[1] {
                    for z in a[2]..=b[2] {
                        grid[(x * side + y) * side + z].push(t);
                    }
                }
            }
        }
        let mut seen = vec![0u32; n];
        let mut stamp = 0u32;
        for t in (0..n).filter(|t| changed[*t]) {
            let c = after(t);
            let (l, h) = boxed(&c);
            if (0..3).any(|i| h[i] < lo[i] || l[i] > hi[i]) {
                continue;
            }
            stamp += 1;
            let (a, b) = (cell_of(l), cell_of(h));
            let own = m.triangles[t];
            for x in a[0]..=b[0] {
                for y in a[1]..=b[1] {
                    for z in a[2]..=b[2] {
                        for &o in &grid[(x * side + y) * side + z] {
                            if seen[o as usize] == stamp {
                                continue;
                            }
                            seen[o as usize] = stamp;
                            let other = m.triangles[o as usize];
                            if other.iter().any(|v| own.contains(v)) {
                                continue;
                            }
                            if triangles_cross(&c, &m.corners(o as usize)) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }
}

/// Do two triangles cut through each other (an edge of one goes through the face of the other)?
fn triangles_cross(a: &[V3; 3], b: &[V3; 3]) -> bool {
    let pierces = |e0: V3, e1: V3, t: &[V3; 3]| -> bool {
        // Moeller-Trumbore for the segment e0..e1, the ends left out a little
        let dir = sub(e1, e0);
        let (e1v, e2v) = (sub(t[1], t[0]), sub(t[2], t[0]));
        let p = cross(dir, e2v);
        let det = dot(e1v, p);
        if det.abs() < 1e-14 * (len(e1v) * len(e2v) * len(dir)).max(1e-300) {
            return false;
        }
        let inv = 1.0 / det;
        let tv = sub(e0, t[0]);
        let u = dot(tv, p) * inv;
        let q = cross(tv, e1v);
        let v = dot(dir, q) * inv;
        let s = dot(e2v, q) * inv;
        let eps = 1e-6;
        u > eps && v > eps && u + v < 1.0 - eps && s > eps && s < 1.0 - eps
    };
    for k in 0..3 {
        if pierces(a[k], a[(k + 1) % 3], b) || pierces(b[k], b[(k + 1) % 3], a) {
            return true;
        }
    }
    false
}

/// Changes the diameter of a round hole or pin: its vertices move radially to the new radius.
pub fn set_diameter(m: &mut TriMesh, cyl: &Cylinder, diameter: f64) -> Result<Undo, EditError> {
    set_diameter_with(m, cyl, diameter, true)
}

pub fn set_diameter_with(
    m: &mut TriMesh,
    cyl: &Cylinder,
    diameter: f64,
    verify: bool,
) -> Result<Undo, EditError> {
    if !(diameter.is_finite() && diameter > 1e-6) {
        return Err(EditError::BadDiameter);
    }
    if verify {
        check_closed(m)?;
    }
    let r = diameter / 2.0;
    let (verts, outside) = surroundings(m, &cyl.triangles);
    let normals_before: Vec<V3> = outside.iter().map(|t| m.normal_area(*t as usize).0).collect();
    let mut undo = Undo::default();
    for v in &verts {
        let p = m.vertex(*v);
        let rel = sub(p, cyl.point);
        let along = dot(rel, cyl.axis);
        let radial = sub(rel, mul(cyl.axis, along));
        let rl = len(radial);
        if rl < 1e-9 || (rl - cyl.radius).abs() > cyl.radius * 0.1 + 1e-6 {
            // a vertex of the patch that is not on the circle (an edge of a neighbour wall): the hole is not clean
            undo.apply(m);
            return Err(EditError::NotRound);
        }
        undo.moved.push((*v, m.positions[*v as usize]));
        m.set_vertex(*v, add(add(cyl.point, mul(cyl.axis, along)), mul(radial, r / rl)));
    }
    for (k, t) in outside.iter().enumerate() {
        let after = m.normal_area(*t as usize);
        if after.1 > 0.0 && dot(after.0, normals_before[k]) < 0.5 {
            undo.apply(m);
            return Err(EditError::Collapses);
        }
    }
    let _ = cross([0.0; 3], [0.0; 3]);
    Ok(undo)
}
