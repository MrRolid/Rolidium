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
}

fn vec3s(f: &[f32]) -> Vec<Vec3> {
    f.chunks_exact(3).map(|c| Vec3::new(c[0], c[1], c[2])).collect()
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
        }
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
        for t in self.indices.chunks_exact(3) {
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
        for t in self.indices.chunks_exact(3) {
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
            .chunks_exact(3)
            .map(|t| {
                let a = self.positions[t[0] as usize].as_dvec3();
                let b = self.positions[t[1] as usize].as_dvec3();
                let c = self.positions[t[2] as usize].as_dvec3();
                a.dot(b.cross(c)) / 6.0
            })
            .sum()
    }
}
