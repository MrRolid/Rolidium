// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! What the display needs of a body: its mesh and the facts about its faces. Shared by the window and by the
//! command line tools, so that both can be timed. The cost of a change depends on the bodies that changed, not on the
//! size of the document: [`MeshCache`] keeps the entries of bodies whose solid did not change.

use crate::BodyId;
use rolidium_kernel::{DVec3, FaceKind, Mesh, Solid};
use std::collections::HashMap;

/// What the UI needs to know about a face without asking the kernel.
#[derive(Debug, Clone, Copy)]
pub struct FaceMeta {
    pub kind: FaceKind,
    pub normal: DVec3,
    pub origin: DVec3,
    pub area: f64,
    pub centroid: glam::Vec3,
    /// The box around the triangles of the face (for the test whether it is in view).
    pub bounds: (glam::Vec3, glam::Vec3),
    /// Complete hole or pin: its diameter.
    pub cyl_diameter: Option<f64>,
    /// Flat circular face belonging to a hole or pin: (cylindrical face, diameter).
    pub disc: Option<(u32, f64)>,
    /// An end of a plain cone, frustum or cylinder (a solid of three faces): its diameter. The side follows when it changes.
    pub cap: Option<f64>,
}

#[derive(Clone)]
pub struct MeshEntry {
    pub id: BodyId,
    #[allow(dead_code)]
    pub name: String,
    /// Shared: an entry is copied for every operation and for every frame, its triangles are not.
    pub mesh: std::sync::Arc<Mesh>,
    pub faces: std::sync::Arc<Vec<FaceMeta>>,
    pub color: [f32; 3],
    /// Identifies this mesh: a new number whenever the mesh is made, so that the graphics card keeps the buffers of
    /// a mesh that did not change.
    pub stamp: u64,
}

/// A number that was not used before.
pub fn next_stamp() -> u64 {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub type BodyInput = (BodyId, String, Solid, [f32; 3]);

/// Time spent (summed over all threads, nanoseconds) making meshes, and making the facts about the faces: the first is native work that
/// runs without the lock of the kernel, the second asks the kernel face by face, one call at a time (for `rolidium-cli times open`).
pub static MESH_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static FACTS_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The first (rough) display entry of a body.
pub fn mesh_entry(b: &BodyInput) -> Result<MeshEntry, String> {
    use std::sync::atomic::Ordering::Relaxed;
    let t = std::time::Instant::now();
    let mesh = b.2.display_mesh().map_err(|e| e.to_string())?;
    MESH_NS.fetch_add(t.elapsed().as_nanos() as u64, Relaxed);
    let t = std::time::Instant::now();
    let r = entry_of(b, mesh);
    FACTS_NS.fetch_add(t.elapsed().as_nanos() as u64, Relaxed);
    r
}

/// The entry of a body for a given mesh of it.
pub fn entry_of(b: &BodyInput, mesh: Mesh) -> Result<MeshEntry, String> {
    let (id, name, solid, color) = b;
    // the box is found here, on the thread of the worker, and stays with the solid: the window asks for it at every frame
    let _ = solid.bounding_box();
    // a body with faces and no triangle is a mesh that was stopped or failed, not a body to show as empty
    if solid.face_count() > 0 && mesh.triangle_count() == 0 {
        return Err("The display mesh of the body could not be made, so the view was not changed".into());
    }
    let mut sums = vec![(glam::Vec3::ZERO, 0u32); solid.face_count()];
    let mut bounds = vec![(glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN)); solid.face_count()];
    for (t, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
        let f = mesh.tri_face[t] as usize;
        for &i in tri {
            let p = mesh.positions[i as usize];
            sums[f].0 += p;
            sums[f].1 += 1;
            bounds[f].0 = bounds[f].0.min(p);
            bounds[f].1 = bounds[f].1.max(p);
        }
    }
    let mut faces = Vec::with_capacity(sums.len());
    for (i, f) in solid.faces().enumerate() {
        let info = solid.face_info(f).map_err(|e| e.to_string())?;
        let (sum, n) = sums[i];
        faces.push(FaceMeta {
            kind: info.kind,
            normal: info.normal,
            origin: info.origin,
            area: info.area,
            centroid: if n > 0 { sum / n as f32 } else { glam::Vec3::ZERO },
            bounds: bounds[i],
            cyl_diameter: None,
            disc: None,
            cap: None,
        });
    }
    // Hole/pin information for the dimension panel (only cheap candidates are examined).
    for (i, f) in solid.faces().enumerate() {
        match faces[i].kind {
            FaceKind::Cylinder => {
                if let Ok(Some(c)) = solid.cylinder_info(f) {
                    faces[i].cyl_diameter = Some(c.radius * 2.0);
                }
            }
            FaceKind::Plane => {
                if let Ok(Some((cyl, r))) = solid.disc_partner(f) {
                    faces[i].disc = Some((cyl.0, r * 2.0));
                }
            }
            _ => {}
        }
    }
    if let Some(fr) = solid.frustum_info() {
        for (f, r) in [(fr.cap_base, fr.r_base), (fr.cap_top, fr.r_top)] {
            if let Some(m) = faces.get_mut(f.0 as usize) {
                m.cap = Some(2.0 * r);
            }
        }
    }
    Ok(MeshEntry {
        id: *id,
        name: name.clone(),
        mesh: std::sync::Arc::new(mesh),
        faces: std::sync::Arc::new(faces),
        color: *color,
        stamp: next_stamp(),
    })
}

struct Cached {
    solid: Solid,
    entry: MeshEntry,
}

/// A mesh (STL) body for the display, with its regions when they are known (see `rolidium_mesh::regions`).
#[derive(Clone)]
pub struct MeshInput {
    pub id: BodyId,
    pub name: String,
    pub mesh: std::sync::Arc<rolidium_mesh::TriMesh>,
    pub color: [f32; 3],
    pub regions: Option<std::sync::Arc<rolidium_mesh::regions::Regions>>,
}

/// The index of a "face" of a mesh body: 0 is what no region covers, then the planar walls, then the holes and pins.
pub fn region_face(regions: &rolidium_mesh::regions::Regions, of_triangle: u32) -> u32 {
    use rolidium_mesh::regions::CYL_FLAG;
    if of_triangle == 0 {
        0
    } else if of_triangle & CYL_FLAG != 0 {
        1 + regions.planes.len() as u32 + (of_triangle & !CYL_FLAG) - 1
    } else {
        of_triangle
    }
}

/// The display entry of a mesh body. The vertices are shared by the triangles of one region only (a vertex on the border of two regions is two vertices of the
/// display mesh), so that a region has an identity on the graphics card and can be lit; a mesh without regions is one face. The normals are zero: the
/// shader takes the flat normal of the triangle.
pub fn mesh_body_entry(i: &MeshInput) -> MeshEntry {
    mesh_body_entry_with(i).0
}

/// The same, with the vertex each extra display vertex is a copy of.
fn mesh_body_entry_with(i: &MeshInput) -> (MeshEntry, Vec<u32>) {
    use glam::Vec3;
    let m = &*i.mesh;
    let n_tri = m.triangles.len();
    let nfaces = i
        .regions
        .as_ref()
        .map_or(1, |r| 1 + r.planes.len() + r.cylinders.len());
    let face_of = |t: usize| -> u32 {
        match &i.regions {
            Some(r) => region_face(r, r.of_triangle.get(t).copied().unwrap_or(0)),
            None => 0,
        }
    };
    let mut positions: Vec<Vec3> = m.positions.iter().map(|p| Vec3::new(p[0], p[1], p[2])).collect();
    let mut indices: Vec<u32> = Vec::with_capacity(n_tri * 3);
    let mut tri_face: Vec<u32> = Vec::with_capacity(n_tri);
    let mut dups: Vec<u32> = Vec::new();
    if nfaces <= 1 {
        for t in &m.triangles {
            indices.extend_from_slice(t);
        }
        tri_face.resize(n_tri, 0);
    } else {
        let mut first_face = vec![u32::MAX; positions.len()];
        let mut extra: HashMap<u64, u32> = HashMap::new();
        for (t, tri) in m.triangles.iter().enumerate() {
            let f = face_of(t);
            tri_face.push(f);
            for &v in tri {
                let slot = &mut first_face[v as usize];
                if *slot == u32::MAX {
                    *slot = f;
                    indices.push(v);
                } else if *slot == f {
                    indices.push(v);
                } else {
                    let key = ((v as u64) << 32) | f as u64;
                    let at = *extra.entry(key).or_insert_with(|| {
                        positions.push(positions[v as usize]);
                        dups.push(v);
                        (positions.len() - 1) as u32
                    });
                    indices.push(at);
                }
            }
        }
    }
    // the facts of the faces
    let mut sums = vec![(Vec3::ZERO, 0u32); nfaces];
    let mut bounds = vec![(Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)); nfaces];
    for (t, tri) in indices.as_chunks::<3>().0.iter().enumerate() {
        let f = tri_face[t] as usize;
        for &v in tri {
            let p = positions[v as usize];
            sums[f].0 += p;
            sums[f].1 += 1;
            bounds[f].0 = bounds[f].0.min(p);
            bounds[f].1 = bounds[f].1.max(p);
        }
    }
    let mut faces: Vec<FaceMeta> = Vec::with_capacity(nfaces);
    let dv = |v: [f64; 3]| DVec3::new(v[0], v[1], v[2]);
    for f in 0..nfaces {
        let (sum, n) = sums[f];
        let mut meta = FaceMeta {
            kind: FaceKind::Other,
            normal: DVec3::ZERO,
            origin: DVec3::ZERO,
            area: 0.0,
            centroid: if n > 0 { sum / n as f32 } else { Vec3::ZERO },
            bounds: bounds[f],
            cyl_diameter: None,
            disc: None,
            cap: None,
        };
        if let Some(r) = &i.regions {
            if f >= 1 && f <= r.planes.len() {
                let p = &r.planes[f - 1];
                meta.kind = FaceKind::Plane;
                meta.normal = dv(p.normal);
                meta.origin = dv(p.normal) * p.offset;
                meta.area = p.area;
            } else if f > r.planes.len() {
                let c = &r.cylinders[f - 1 - r.planes.len()];
                meta.kind = FaceKind::Cylinder;
                meta.normal = dv(c.axis);
                meta.origin = dv(c.point);
                meta.cyl_diameter = Some(c.radius * 2.0);
            }
        }
        faces.push(meta);
    }
    let normals = vec![Vec3::ZERO; positions.len()];
    let mesh = Mesh {
        positions,
        normals,
        indices,
        tri_face,
        edges: Vec::new(),
        deflection: 0.0,
        face_deflection: Vec::new(),
        face_asked: Vec::new(),
    };
    (
        MeshEntry {
            id: i.id,
            name: i.name.clone(),
            mesh: std::sync::Arc::new(mesh),
            faces: std::sync::Arc::new(faces),
            color: i.color,
            stamp: next_stamp(),
        },
        dups,
    )
}

/// The display of a mesh after an edit that moved some vertices (a pull of a wall, the diameter of a hole), from the display before it: only the
/// moved vertices and the facts of the faces the edit touched are made again. The walls and holes are the same ones, in the same order.
fn patched_entry(old: &CachedMesh, i: &MeshInput, hint: &EditHint) -> Option<(MeshEntry, Vec<u32>)> {
    use glam::Vec3;
    let regions = i.regions.as_ref()?;
    let n_orig = i.mesh.positions.len();
    if old.entry.mesh.positions.len() != n_orig + old.dups.len()
        || old.entry.faces.len() != 1 + regions.planes.len() + regions.cylinders.len()
    {
        return None;
    }
    let mut mesh: Mesh = (*old.entry.mesh).clone();
    let at = |v: u32| {
        let p = i.mesh.positions[v as usize];
        Vec3::new(p[0], p[1], p[2])
    };
    let mut moved = vec![false; n_orig];
    for &v in &hint.moved {
        moved[v as usize] = true;
        mesh.positions[v as usize] = at(v);
    }
    for (k, &o) in old.dups.iter().enumerate() {
        if moved[o as usize] {
            mesh.positions[n_orig + k] = at(o);
        }
    }
    // the facts of the faces that changed: the box and the centre from the triangles of the face, the rest from the regions
    let mut faces: Vec<FaceMeta> = (*old.entry.faces).clone();
    let mut want = vec![false; faces.len()];
    for f in &hint.faces {
        if let Some(w) = want.get_mut(*f as usize) {
            *w = true;
        }
    }
    let mut sums = vec![(Vec3::ZERO, 0u32); faces.len()];
    let mut bounds = vec![(Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)); faces.len()];
    for (t, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
        let f = mesh.tri_face[t] as usize;
        if !want[f] {
            continue;
        }
        for &v in tri {
            let p = mesh.positions[v as usize];
            sums[f].0 += p;
            sums[f].1 += 1;
            bounds[f].0 = bounds[f].0.min(p);
            bounds[f].1 = bounds[f].1.max(p);
        }
    }
    let dv = |v: [f64; 3]| DVec3::new(v[0], v[1], v[2]);
    for f in 0..faces.len() {
        if !want[f] {
            continue;
        }
        if sums[f].1 > 0 {
            faces[f].centroid = sums[f].0 / sums[f].1 as f32;
            faces[f].bounds = bounds[f];
        }
        if f >= 1 && f <= regions.planes.len() {
            let p = &regions.planes[f - 1];
            faces[f].origin = dv(p.normal) * p.offset;
            faces[f].area = p.area;
        } else if f > regions.planes.len() {
            let c = &regions.cylinders[f - 1 - regions.planes.len()];
            faces[f].cyl_diameter = Some(c.radius * 2.0);
            faces[f].origin = dv(c.point);
        }
    }
    Some((
        MeshEntry {
            id: i.id,
            name: i.name.clone(),
            mesh: std::sync::Arc::new(mesh),
            faces: std::sync::Arc::new(faces),
            color: i.color,
            stamp: next_stamp(),
        },
        (*old.dups).clone(),
    ))
}

struct CachedMesh {
    /// Kept so that the address below stays the address of this mesh.
    mesh: std::sync::Arc<rolidium_mesh::TriMesh>,
    regions: Option<std::sync::Arc<rolidium_mesh::regions::Regions>>,
    entry: MeshEntry,
    /// The vertex of the mesh that each extra vertex of the display mesh is a copy of (a vertex on the border of two regions is two display vertices).
    dups: std::sync::Arc<Vec<u32>>,
}

/// How many versions of the display of one mesh are kept (an Undo and a Redo find theirs again, without making them).
const MESH_VERSIONS: usize = 3;

/// What an edit of a mesh moved: the display of the mesh before it can be patched instead of made again.
#[derive(Clone)]
pub struct EditHint {
    pub old: std::sync::Arc<rolidium_mesh::TriMesh>,
    /// The vertices that moved.
    pub moved: Vec<u32>,
    /// The faces (walls and holes of the entry) whose facts changed.
    pub faces: Vec<u32>,
}

static EDITS: std::sync::Mutex<Vec<(std::sync::Arc<rolidium_mesh::TriMesh>, EditHint)>> =
    std::sync::Mutex::new(Vec::new());

/// Remembers what an edit moved, by the mesh it made.
pub fn keep_edit(new: &std::sync::Arc<rolidium_mesh::TriMesh>, hint: EditHint) {
    let mut v = EDITS.lock().unwrap_or_else(|e| e.into_inner());
    v.push((new.clone(), hint));
    while v.len() > 4 {
        v.remove(0);
    }
}

fn edit_of(new: &std::sync::Arc<rolidium_mesh::TriMesh>) -> Option<EditHint> {
    let v = EDITS.lock().unwrap_or_else(|e| e.into_inner());
    v.iter()
        .find(|(m, _)| std::sync::Arc::ptr_eq(m, new))
        .map(|x| x.1.clone())
}

/// The display entries of the bodies, made again only for the bodies whose solid is not the one they were made for.
#[derive(Default)]
pub struct MeshCache {
    map: HashMap<BodyId, Cached>,
    meshes: HashMap<BodyId, Vec<CachedMesh>>,
    /// How many entries were made (not taken from the cache) by the last call of `entries`.
    pub last_made: usize,
    /// Several cores for the entries to make (the default); off only to measure the single thread.
    pub serial: bool,
}

impl MeshCache {
    /// An empty cache that works on one thread (to measure the single thread).
    pub fn serial() -> Self {
        MeshCache {
            serial: true,
            ..Default::default()
        }
    }

    /// Entries for `bodies` in their order; bodies without triangles are left out. The entries of unchanged solids come from
    /// the cache (their name and colour are refreshed).
    pub fn entries(&mut self, bodies: &[BodyInput]) -> Result<Vec<MeshEntry>, String> {
        // the entries to make
        let todo: Vec<&BodyInput> = bodies
            .iter()
            .filter(|b| {
                self.map
                    .get(&b.0)
                    .is_none_or(|c| c.solid.shape_id() != b.2.shape_id())
            })
            .collect();
        self.last_made = todo.len();
        let made = if self.serial {
            todo.iter().map(|b| mesh_entry(b)).collect::<Vec<_>>()
        } else {
            rolidium_kernel::par::map_groups(&todo, |b| &b.2, |b| mesh_entry(b))
        }
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
        for (b, entry) in todo.iter().zip(made) {
            self.map.insert(
                b.0,
                Cached {
                    solid: b.2.clone(),
                    entry,
                },
            );
        }
        let mut out = Vec::with_capacity(bodies.len());
        for b in bodies {
            let Some(c) = self.map.get_mut(&b.0) else { continue };
            c.entry.name.clone_from(&b.1);
            c.entry.color = b.3;
            if !c.entry.mesh.indices.is_empty() {
                out.push(c.entry.clone());
            }
        }
        // forget the bodies that are gone
        if self.map.len() > bodies.len() {
            let ids: std::collections::HashSet<BodyId> = bodies.iter().map(|b| b.0).collect();
            self.map.retain(|id, _| ids.contains(id));
        }
        Ok(out)
    }

    /// The entries of the mesh bodies in their order: the entry of a mesh that has not changed (and has the same regions) is the one made before.
    pub fn mesh_entries(&mut self, inputs: &[MeshInput]) -> Vec<MeshEntry> {
        let mut out = Vec::with_capacity(inputs.len());
        for i in inputs {
            let versions = self.meshes.entry(i.id).or_default();
            let same = |c: &CachedMesh| {
                std::sync::Arc::ptr_eq(&c.mesh, &i.mesh)
                    && match (&c.regions, &i.regions) {
                        (None, None) => true,
                        (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
                        _ => false,
                    }
            };
            // the display of this very mesh (an Undo or a Redo brought it back) is the one made before
            if let Some(k) = versions.iter().position(same) {
                let c = versions.remove(k);
                versions.push(c);
            } else {
                // an edit that moved some vertices of the mesh the last display was made for: patched, not made again
                let patched = edit_of(&i.mesh).and_then(|h| {
                    versions
                        .iter()
                        .rev()
                        .find(|c| std::sync::Arc::ptr_eq(&c.mesh, &h.old))
                        .and_then(|c| patched_entry(c, i, &h))
                });
                let (entry, dups) = patched.unwrap_or_else(|| mesh_body_entry_with(i));
                versions.push(CachedMesh {
                    mesh: i.mesh.clone(),
                    regions: i.regions.clone(),
                    entry,
                    dups: std::sync::Arc::new(dups),
                });
                while versions.len() > MESH_VERSIONS {
                    versions.remove(0);
                }
            }
            let c = versions.last_mut().unwrap();
            c.entry.name.clone_from(&i.name);
            c.entry.color = i.color;
            out.push(c.entry.clone());
        }
        if self.meshes.len() > inputs.len() {
            let ids: std::collections::HashSet<BodyId> = inputs.iter().map(|b| b.id).collect();
            self.meshes.retain(|id, _| ids.contains(id));
        }
        out
    }

    /// The entries made by a zoom refinement replace the cached ones of the same solid.
    pub fn replace(&mut self, entry: &MeshEntry, solid: &Solid) {
        self.map.insert(
            entry.id,
            Cached {
                solid: solid.clone(),
                entry: entry.clone(),
            },
        );
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.meshes.clear();
    }
}

/// A planar face of a body that can be pulled a little and an edge that can be rounded a little: something small to edit
/// (for the measurements of the operations). Tried on the document itself, so what is found works.
pub fn small_edit_targets(doc: &crate::Document, body: BodyId) -> Option<(rolidium_kernel::FaceId, u32)> {
    let solid = &doc.body(body)?.solid;
    let face = solid
        .faces()
        .filter_map(|f| solid.face_info(f).ok().map(|i| (f, i)))
        .filter(|(_, i)| i.kind == FaceKind::Plane)
        .max_by(|a, b| a.1.area.total_cmp(&b.1.area))?
        .0;
    let edge = solid.edges().map(|e| e.0).find(|e| {
        doc.plan(crate::Command::FilletEdges {
            body,
            edges: vec![*e],
            size: 0.1,
            chamfer: false,
        })
        .run()
        .is_ok()
    })?;
    Some((face, edge))
}

/// The regions found for meshes, kept by the address of the mesh they belong to (the mesh is kept too, so that the address stays its own). A state
/// that Undo brings back finds its regions here; the older ones are forgotten.
static REGIONS: std::sync::Mutex<
    Vec<(
        std::sync::Arc<rolidium_mesh::TriMesh>,
        std::sync::Arc<rolidium_mesh::regions::Regions>,
    )>,
> = std::sync::Mutex::new(Vec::new());

/// How many meshes with regions are kept.
const REGIONS_KEPT: usize = 6;

/// The regions found for this mesh, if they are known.
pub fn regions_of(
    mesh: &std::sync::Arc<rolidium_mesh::TriMesh>,
) -> Option<std::sync::Arc<rolidium_mesh::regions::Regions>> {
    let mut v = REGIONS.lock().unwrap_or_else(|e| e.into_inner());
    let i = v.iter().position(|(m, _)| std::sync::Arc::ptr_eq(m, mesh))?;
    let hit = v.remove(i);
    let r = hit.1.clone();
    v.push(hit);
    Some(r)
}

/// Remembers the regions of a mesh.
pub fn keep_regions(
    mesh: &std::sync::Arc<rolidium_mesh::TriMesh>,
    regions: std::sync::Arc<rolidium_mesh::regions::Regions>,
) {
    let mut v = REGIONS.lock().unwrap_or_else(|e| e.into_inner());
    v.retain(|(m, _)| !std::sync::Arc::ptr_eq(m, mesh));
    v.push((mesh.clone(), regions));
    while v.len() > REGIONS_KEPT {
        v.remove(0);
    }
}

/// The visible meshes of a state for the display, each with the regions that are known.
pub fn mesh_inputs(state: &crate::State) -> Vec<MeshInput> {
    state
        .meshes
        .iter()
        .filter(|m| m.visible)
        .map(|m| MeshInput {
            id: m.id,
            name: m.name.clone(),
            mesh: m.mesh.clone(),
            color: m.color,
            regions: regions_of(&m.mesh),
        })
        .collect()
}
