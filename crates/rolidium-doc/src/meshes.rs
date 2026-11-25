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

/// Moves a planar wall of a mesh body by `distance` along its normal; with `tilt` the walls next to it may slant. Gives the mesh and the triangles next to the wall
/// that were stretched.
pub(crate) fn pull(
    b: &MeshBody,
    wall: &Plane,
    distance: f64,
    tilt: bool,
) -> Result<(TriMesh, Vec<u32>), crate::DocError> {
    let mut m = (*b.mesh).clone();
    let plan = rolidium_mesh::edit::PullPlan::new(&m, wall);
    rolidium_mesh::edit::pull_plane_opts(&mut m, wall, distance, !b.verified, tilt).map_err(invalid)?;
    Ok((m, plan.outside))
}

/// Sets the diameter of a round hole or pin of a mesh body.
pub(crate) fn hole(b: &MeshBody, cyl: &Cylinder, diameter: f64) -> Result<TriMesh, crate::DocError> {
    let mut m = (*b.mesh).clone();
    rolidium_mesh::edit::set_diameter_with(&mut m, cyl, diameter, !b.verified).map_err(invalid)?;
    Ok(m)
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
/// instead of a new search (seconds for a mesh of millions of triangles, in which no edit could be made meanwhile).
pub(crate) fn carry_regions(old: &Arc<TriMesh>, new: &Arc<TriMesh>, change: Change) {
    let Some(old_regions) = crate::display::regions_of(old) else {
        return;
    };
    let mut r: rolidium_mesh::regions::Regions = (*old_regions).clone();
    match change {
        Change::Wall {
            first_triangle,
            normal,
            distance,
            ref touched,
        } => {
            let Some(p) = r.planes.iter_mut().find(|p| {
                p.triangles.first() == Some(&first_triangle) && rolidium_mesh::dot(p.normal, normal) > 0.999
            }) else {
                return;
            };
            p.offset += distance;
            // the regions next to the wall stretched: their areas (and the extent of a round surface along the wall's direction) are made again
            let mut planes: Vec<usize> = Vec::new();
            let mut cyls: Vec<usize> = Vec::new();
            for t in touched {
                match r.of_triangle.get(*t as usize).copied().unwrap_or(0) {
                    0 => {}
                    x if x >= crate::meshes::CYL_FLAG => {
                        cyls.push((x - crate::meshes::CYL_FLAG) as usize - 1)
                    }
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
            }
            crate::display::keep_regions(new, Arc::new(r));
            return;
        }
        Change::Hole {
            first_triangle,
            diameter,
        } => {
            let Some(c) = r
                .cylinders
                .iter_mut()
                .find(|c| c.triangles.first() == Some(&first_triangle))
            else {
                return;
            };
            c.radius = diameter / 2.0;
        }
    }
    crate::display::keep_regions(new, Arc::new(r));
}

/// What an edit changed in the regions.
#[derive(Clone)]
pub(crate) enum Change {
    Wall {
        first_triangle: u32,
        normal: [f64; 3],
        distance: f64,
        /// The triangles next to the wall that were stretched.
        touched: Vec<u32>,
    },
    Hole {
        first_triangle: u32,
        diameter: f64,
    },
}
