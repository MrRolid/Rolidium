// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Edits made on the mesh itself: a planar wall moves along its normal and the neighbouring walls stretch (their triangles share the vertices that
//! moved), the vertices of a round hole or pin move radially. Every edit gives a short record of the vertices it moved; applying the record
//! restores them (and gives the record of the way back), so Undo and Redo never copy the mesh.

use crate::regions::{Cylinder, Plane};
use crate::{add, cross, dot, len, mul, sub, TriMesh, V3};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EditError {
    #[error("The wall is not cleanly separated: a neighbouring wall is not square to it, so it cannot be moved without bending the mesh")]
    NeighbourNotSquare,
    #[error("The move would turn a neighbouring wall inside out (the wall would be pulled through the opposite one)")]
    Collapses,
    #[error("The mesh is not closed or not consistent: repair it first")]
    NotClean,
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

fn vertex_triangles(m: &TriMesh, verts: &BTreeMap<u32, ()>) -> BTreeMap<u32, Vec<u32>> {
    let mut map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (t, tri) in m.triangles.iter().enumerate() {
        for v in tri {
            if verts.contains_key(v) {
                map.entry(*v).or_default().push(t as u32);
            }
        }
    }
    map
}

/// The triangles outside `region` that touch one of its vertices, and the vertices of the region.
fn surroundings(m: &TriMesh, region: &[u32]) -> (BTreeMap<u32, ()>, Vec<u32>) {
    let mut verts: BTreeMap<u32, ()> = BTreeMap::new();
    for &t in region {
        for &v in &m.triangles[t as usize] {
            verts.insert(v, ());
        }
    }
    let inside: std::collections::HashSet<u32> = region.iter().copied().collect();
    let touching = vertex_triangles(m, &verts);
    let mut outside: Vec<u32> = touching.values().flatten().copied().filter(|t| !inside.contains(t)).collect();
    outside.sort_unstable();
    outside.dedup();
    (verts, outside)
}

fn check_closed(m: &TriMesh) -> Result<(), EditError> {
    let r = crate::check::check(m);
    if r.is_closed() && r.inconsistent_edges == 0 {
        Ok(())
    } else {
        Err(EditError::NotClean)
    }
}

/// Moves a planar wall by `distance` along its (outward) normal. The walls that touch it must be square to it; they stretch.
pub fn pull_plane(m: &mut TriMesh, wall: &Plane, distance: f64) -> Result<Undo, EditError> {
    check_closed(m)?;
    let (verts, outside) = surroundings(m, &wall.triangles);
    let n = wall.normal;
    let normals_before: Vec<V3> = outside.iter().map(|t| m.normal_area(*t as usize).0).collect();
    if normals_before.iter().any(|nn| dot(*nn, n).abs() > 0.12) {
        return Err(EditError::NeighbourNotSquare);
    }
    let mut undo = Undo::default();
    for v in verts.keys() {
        undo.moved.push((*v, m.positions[*v as usize]));
        let p = m.vertex(*v);
        m.set_vertex(*v, add(p, mul(n, distance)));
    }
    // no wall may be turned over by the move
    for (k, t) in outside.iter().enumerate() {
        let after = m.normal_area(*t as usize);
        if after.1 > 0.0 && dot(after.0, normals_before[k]) < 0.9 {
            undo.apply(m);
            return Err(EditError::Collapses);
        }
    }
    Ok(undo)
}

/// Changes the diameter of a round hole or pin: its vertices move radially to the new radius.
pub fn set_diameter(m: &mut TriMesh, cyl: &Cylinder, diameter: f64) -> Result<Undo, EditError> {
    if !(diameter.is_finite() && diameter > 1e-6) {
        return Err(EditError::BadDiameter);
    }
    check_closed(m)?;
    let r = diameter / 2.0;
    let (verts, outside) = surroundings(m, &cyl.triangles);
    let normals_before: Vec<V3> = outside.iter().map(|t| m.normal_area(*t as usize).0).collect();
    let mut undo = Undo::default();
    for v in verts.keys() {
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
