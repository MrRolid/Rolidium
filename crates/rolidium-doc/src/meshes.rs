// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Triangle meshes (STL) as bodies of the document. A mesh is not a B-rep body (see D60): it is kept as the welded triangle mesh of `rolidium-mesh`, shown by
//! the same renderer and picked through the same hierarchy of boxes. The mesh is shared (`Arc`), so a snapshot of the document for Undo costs nothing; an edit
//! copies the mesh once (the vertices are moved in the copy) and the snapshot before it stays as it was.

use crate::BodyId;
pub(crate) use rolidium_mesh::regions::CYL_FLAG;
use rolidium_mesh::regions::{Cylinder, Plane};
use rolidium_mesh::TriMesh;
use std::sync::Arc;

/// The colour a new mesh gets (a little warmer than the grey of a B-rep body, so that the two kinds are told apart in the view).
pub const MESH_COLOR: [f32; 3] = [0.80, 0.75, 0.66];

/// A mesh of the document. Its number comes from the same series as the numbers of the bodies, so a group, a selection and a name use it like a body's.
#[derive(Debug, Clone)]
pub struct MeshBody {
    pub id: BodyId,
    pub name: String,
    pub mesh: Arc<TriMesh>,
    pub color: [f32; 3],
    pub visible: bool,
    /// The mesh was checked and is closed and consistent: the edits of a wall or a hole may be made without a new check (they keep it so).
    pub verified: bool,
}

/// A mesh to be added to the document (an import).
#[derive(Debug, Clone)]
pub struct NewMesh {
    pub name: String,
    pub mesh: Arc<TriMesh>,
    pub verified: bool,
}

/// What a repair of a mesh does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairKind {
    /// Equal vertices (within a tiny distance) become one.
    pub merge: bool,
    /// The triangles are turned to agree on what is outside.
    pub orient: bool,
}

/// Moves a planar wall of a mesh body by `distance` along its normal; with `tilt` the walls next to it may slant. Gives the mesh and the vertices that moved.
pub(crate) fn pull(
    b: &MeshBody,
    wall: &Plane,
    distance: f64,
    tilt: bool,
) -> Result<(TriMesh, Vec<u32>), crate::DocError> {
    let mut m = (*b.mesh).clone();
    let undo =
        rolidium_mesh::edit::pull_plane_opts(&mut m, wall, distance, !b.verified, tilt).map_err(invalid)?;
    Ok((m, undo.moved.iter().map(|x| x.0).collect()))
}

/// Sets the diameter of a round hole or pin of a mesh body. Gives the mesh and the vertices that moved.
pub(crate) fn hole(
    b: &MeshBody,
    cyl: &Cylinder,
    diameter: f64,
) -> Result<(TriMesh, Vec<u32>), crate::DocError> {
    let mut m = (*b.mesh).clone();
    let undo = rolidium_mesh::edit::set_diameter_with(&mut m, cyl, diameter, !b.verified).map_err(invalid)?;
    Ok((m, undo.moved.iter().map(|x| x.0).collect()))
}

fn invalid(e: rolidium_mesh::edit::EditError) -> crate::DocError {
    crate::DocError::Kernel(rolidium_kernel::KernelError::Invalid(e.to_string()))
}

/// The repair of a mesh; the mesh and what was done ("3 vertices merged, 12 triangles turned").
pub(crate) fn repair(b: &MeshBody, kind: RepairKind) -> (TriMesh, String) {
    use rolidium_mesh::check;
    let mut m = (*b.mesh).clone();
    let merged = if kind.merge {
        let tol = check::merge_tolerance(&m);
        check::merge_vertices(&mut m, tol)
    } else {
        0
    };
    let turned = if kind.orient {
        check::orient_normals(&mut m)
    } else {
        0
    };
    (m, format!("{merged} vertices merged, {turned} triangles turned"))
}

/// The triangles of the visible meshes of a document as one soup (for the STL export of the whole document or of some bodies).
pub fn merged(meshes: &[&TriMesh]) -> TriMesh {
    let mut out = TriMesh::default();
    for m in meshes {
        let base = out.positions.len() as u32;
        out.positions.extend_from_slice(&m.positions);
        out.triangles.extend(
            m.triangles
                .iter()
                .map(|t| [t[0] + base, t[1] + base, t[2] + base]),
        );
    }
    out
}

/// After a pull of a wall or a change of a hole the walls and holes are still the same triangles in the same places (the walls next to the pulled one stretch in their own
/// planes), so the regions of the old mesh are made the regions of the new one, with the offset of the pulled wall, the radius of the hole and the areas made again,
/// instead of a new search (seconds for a mesh of millions of triangles, in which no edit could be made meanwhile). `moved` are the vertices that moved.
/// Gives the faces of the display (walls and holes in their order) whose facts changed; also leaves the note the display needs to patch itself.
pub(crate) fn carry_regions(old: &Arc<TriMesh>, new: &Arc<TriMesh>, change: Change, moved: Vec<u32>) {
    let Some(old_regions) = crate::display::regions_of(old) else {
        return;
    };
    let mut r: rolidium_mesh::regions::Regions = (*old_regions).clone();
    let mut changed: Vec<u32> = Vec::new();
    match change {
        Change::Wall {
            first_triangle,
            normal,
            distance,
        } => {
            let Some(i) = r.planes.iter().position(|p| {
                p.triangles.first() == Some(&first_triangle) && rolidium_mesh::dot(p.normal, normal) > 0.999
            }) else {
                return;
            };
            r.planes[i].offset += distance;
            changed.push(1 + i as u32);
        }
        Change::Hole {
            first_triangle,
            diameter,
        } => {
            let Some(i) = r
                .cylinders
                .iter()
                .position(|c| c.triangles.first() == Some(&first_triangle))
            else {
                return;
            };
            r.cylinders[i].radius = diameter / 2.0;
            changed.push(1 + r.planes.len() as u32 + i as u32);
        }
    }
    // the regions next to what moved: the triangles that touch a moved vertex
    let mut is_moved = vec![false; new.positions.len()];
    for v in &moved {
        is_moved[*v as usize] = true;
    }
    let mut planes: Vec<usize> = Vec::new();
    let mut cyls: Vec<usize> = Vec::new();
    let mut rest = false;
    for (t, tri) in new.triangles.iter().enumerate() {
        if !tri.iter().any(|v| is_moved[*v as usize]) {
            continue;
        }
        match r.of_triangle.get(t).copied().unwrap_or(0) {
            0 => rest = true,
            x if x >= CYL_FLAG => cyls.push((x - CYL_FLAG) as usize - 1),
            x => planes.push(x as usize - 1),
        }
    }
    planes.sort_unstable();
    planes.dedup();
    cyls.sort_unstable();
    cyls.dedup();
    for i in planes {
        r.planes[i].area = r.planes[i]
            .triangles
            .iter()
            .map(|t| new.normal_area(*t as usize).1)
            .sum();
        changed.push(1 + i as u32);
    }
    for i in cyls {
        let c = &mut r.cylinders[i];
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for t in &c.triangles {
            for v in new.corners(*t as usize) {
                let z = rolidium_mesh::dot(v, c.axis);
                lo = lo.min(z);
                hi = hi.max(z);
            }
        }
        c.z_range = (lo, hi);
        changed.push(1 + r.planes.len() as u32 + i as u32);
    }
    if rest {
        changed.push(0);
    }
    changed.sort_unstable();
    changed.dedup();
    crate::display::keep_regions(new, Arc::new(r));
    crate::display::keep_edit(
        new,
        crate::display::EditHint {
            old: old.clone(),
            moved,
            faces: changed,
        },
    );
}

/// What an edit changed in the regions.
#[derive(Clone)]
pub(crate) enum Change {
    Wall {
        first_triangle: u32,
        normal: [f64; 3],
        distance: f64,
    },
    Hole {
        first_triangle: u32,
        diameter: f64,
    },
}

/// What a conversion of a mesh to a solid did (shown in the panel of the solid).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConvertReport {
    /// The faces of the solid by kind: planes, cylinders, cones, spheres, tori, others.
    pub kinds: [usize; 6],
    /// Faces that are single triangles (nothing flat was found round them), and the share of the surface they cover.
    pub single: usize,
    pub single_share: f64,
    /// The largest distance of a vertex of the mesh from the plane of its face (mm).
    pub max_deviation: f64,
    pub tolerance: f64,
    pub volume_mesh: f64,
    pub volume_solid: f64,
    pub seconds: f64,
    pub triangles: usize,
    /// The mesh the solid came from is kept (hidden).
    pub kept: bool,
    pub rounded_values: usize,
}

static REPORTS: std::sync::Mutex<Vec<(BodyId, ConvertReport)>> = std::sync::Mutex::new(Vec::new());

pub(crate) fn keep_report(body: BodyId, r: ConvertReport) {
    let mut v = REPORTS.lock().unwrap_or_else(|e| e.into_inner());
    v.retain(|(b, _)| *b != body);
    v.push((body, r));
}

/// What the conversion that made this body did, if it was made by one in this session.
pub fn conversion_report(body: BodyId) -> Option<ConvertReport> {
    let v = REPORTS.lock().unwrap_or_else(|e| e.into_inner());
    v.iter().find(|(b, _)| *b == body).map(|(_, r)| r.clone())
}
