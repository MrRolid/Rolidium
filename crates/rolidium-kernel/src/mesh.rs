// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
use crate::nat::MeshData;
use glam::Vec3;
use std::io::Write;

/// One topological edge as a polyline.
#[derive(Debug, Clone)]
pub struct EdgePolyline {
    pub edge: crate::EdgeId,
    pub points: Vec<Vec3>,
    /// The two faces continue each other smoothly here (or it is the seam of one face): drawn faintly.
    pub smooth: bool,
}

/// Triangulated solid. Triangles carry the index of the B-rep face they came from.
#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub indices: Vec<u32>,
    /// One face index per triangle.
    pub tri_face: Vec<u32>,
    pub edges: Vec<EdgePolyline>,
    /// The largest distance (mm) of the triangles and of the edge polylines from the exact shape that this mesh was made for;
    /// 0 when not known.
    pub deflection: f64,
    /// The linear deflection (mm) each B-rep face was asked for (0 when not known); the faces outside the view are coarser.
    pub face_deflection: Vec<f64>,
    /// The deflection the display wanted for each face when it was last meshed (what the program compares with the zoom to decide whether
    /// a face needs a new mesh). It differs from `face_deflection` where a limit (the budget of triangles, the step of one pass) held the
    /// mesh back; empty when not known (then `face_deflection` is used).
    pub face_asked: Vec<f64>,
}

fn vec3s(f: &[f32]) -> Vec<Vec3> {
    f.as_chunks::<3>()
        .0
        .iter()
        .map(|c| Vec3::new(c[0], c[1], c[2]))
        .collect()
}

impl Mesh {
    pub(crate) fn from_native(m: MeshData) -> Mesh {
        let pts = vec3s(&m.edge_points);
        let edges = m
            .edge_ids
            .iter()
            .enumerate()
            .map(|(i, &id)| EdgePolyline {
                edge: crate::EdgeId(id),
                points: pts[m.edge_offsets[i] as usize..m.edge_offsets[i + 1] as usize].to_vec(),
                smooth: m.edge_smooth.get(i).copied().unwrap_or(0) == 1,
            })
            .collect();
        Mesh {
            positions: vec3s(&m.positions),
            normals: vec3s(&m.normals),
            indices: m.indices,
            tri_face: m.tri_face,
            edges,
            deflection: 0.0,
            face_deflection: Vec::new(),
            face_asked: Vec::new(),
        }
    }

    /// The number of triangles of each of `faces` B-rep faces.
    pub fn triangles_per_face(&self, faces: usize) -> Vec<usize> {
        let mut n = vec![0usize; faces];
        for f in &self.tri_face {
            if let Some(c) = n.get_mut(*f as usize) {
                *c += 1;
            }
        }
        n
    }

    /// The area of the triangles of every face (`faces` entries).
    pub fn area_per_face(&self, faces: usize) -> Vec<f64> {
        let mut a = vec![0.0f64; faces];
        for (t, tri) in self.indices.as_chunks::<3>().0.iter().enumerate() {
            let Some(slot) = a.get_mut(self.tri_face[t] as usize) else {
                continue;
            };
            let p = |i: u32| self.positions[i as usize].as_dvec3();
            *slot += 0.5 * (p(tri[1]) - p(tri[0])).cross(p(tri[2]) - p(tri[0])).length();
        }
        a
    }

    /// This mesh with the faces marked in `mask` (and the edges in `part`) taken from `part`, a mesh of those faces only.
    pub fn merged_with(&self, part: &Mesh, mask: &[u8]) -> Mesh {
        let mut out = Mesh {
            positions: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
            tri_face: Vec::new(),
            edges: Vec::new(),
            deflection: self.deflection,
            face_deflection: Vec::new(),
            face_asked: Vec::new(),
        };
        // the triangles of the faces that are kept (their vertices are copied as they are used)
        let mut remap: Vec<u32> = vec![u32::MAX; self.positions.len()];
        for (t, tri) in self.indices.as_chunks::<3>().0.iter().enumerate() {
            let f = self.tri_face[t] as usize;
            if mask.get(f) == Some(&1) {
                continue;
            }
            for &i in tri {
                let slot = &mut remap[i as usize];
                if *slot == u32::MAX {
                    *slot = out.positions.len() as u32;
                    out.positions.push(self.positions[i as usize]);
                    out.normals.push(self.normals[i as usize]);
                }
                out.indices.push(*slot);
            }
            out.tri_face.push(f as u32);
        }
        let base = out.positions.len() as u32;
        out.positions.extend_from_slice(&part.positions);
        out.normals.extend_from_slice(&part.normals);
        out.indices.extend(part.indices.iter().map(|i| i + base));
        out.tri_face.extend_from_slice(&part.tri_face);
        // the edges: the new polylines replace the old ones with the same edge
        let new_ids: std::collections::HashSet<u32> = part.edges.iter().map(|e| e.edge.0).collect();
        out.edges = self
            .edges
            .iter()
            .filter(|e| !new_ids.contains(&e.edge.0))
            .cloned()
            .collect();
        out.edges.extend(part.edges.iter().cloned());
        out
    }

    /// Adds the triangles of `other` (used to write several bodies into one STL file).
    pub fn append(&mut self, other: &Mesh) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.indices.extend(other.indices.iter().map(|i| i + base));
        self.tri_face.extend_from_slice(&other.tri_face);
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Binary STL (80-byte header, little-endian). Facet normals are computed from the triangle.
    pub fn write_stl_binary<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        let mut header = [0u8; 80];
        let tag = b"Rolidium binary STL";
        header[..tag.len()].copy_from_slice(tag);
        w.write_all(&header)?;
        w.write_all(&(self.triangle_count() as u32).to_le_bytes())?;
        for t in self.indices.as_chunks::<3>().0 {
            let a = self.positions[t[0] as usize];
            let b = self.positions[t[1] as usize];
            let c = self.positions[t[2] as usize];
            let n = (b - a).cross(c - a).normalize_or_zero();
            for v in [n, a, b, c] {
                for x in v.to_array() {
                    w.write_all(&x.to_le_bytes())?;
                }
            }
            w.write_all(&[0u8, 0u8])?;
        }
        Ok(())
    }

    /// ASCII STL.
    pub fn write_stl_ascii<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        writeln!(w, "solid rolidium")?;
        for t in self.indices.as_chunks::<3>().0 {
            let a = self.positions[t[0] as usize];
            let b = self.positions[t[1] as usize];
            let c = self.positions[t[2] as usize];
            let n = (b - a).cross(c - a).normalize_or_zero();
            writeln!(w, "  facet normal {:e} {:e} {:e}", n.x, n.y, n.z)?;
            writeln!(w, "    outer loop")?;
            for v in [a, b, c] {
                writeln!(w, "      vertex {:e} {:e} {:e}", v.x, v.y, v.z)?;
            }
            writeln!(w, "    endloop")?;
            writeln!(w, "  endfacet")?;
        }
        writeln!(w, "endsolid rolidium")
    }

    /// Signed volume of the triangle mesh (positive for a correctly oriented closed mesh).
    pub fn signed_volume(&self) -> f64 {
        self.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let a = self.positions[t[0] as usize].as_dvec3();
                let b = self.positions[t[1] as usize].as_dvec3();
                let c = self.positions[t[2] as usize].as_dvec3();
                a.dot(b.cross(c)) / 6.0
            })
            .sum()
    }
}
