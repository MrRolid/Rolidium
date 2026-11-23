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

/// The display entries of the bodies, made again only for the bodies whose solid is not the one they were made for.
#[derive(Default)]
pub struct MeshCache {
    map: HashMap<BodyId, Cached>,
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
