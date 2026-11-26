// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
// the 3 x 3 and per-axis code reads better with indices
#![allow(clippy::needless_range_loop)]
//! Triangle meshes (STL): a welded indexed mesh, import and export, a check with simple repairs, the recognition of planar and cylindrical regions,
//! and edits made directly on the mesh (pull a planar wall, change the diameter of a round hole or pin) that are undone by a short record of the
//! moved vertices, not by a copy of the mesh. No B-rep is made (see D60 in docs/DECISIONS.md).

pub mod check;
pub mod convert;
pub mod edit;
pub mod fit;
pub mod gen;
pub mod holefill;
pub mod planesweep;
pub mod regions;
pub mod serial;
pub mod stl;
pub mod threemf;
pub mod transform;

/// An indexed triangle mesh. Vertices shared by triangles are one vertex (the import welds them), so a wall that moves takes its neighbours along.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriMesh {
    pub positions: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

pub(crate) type V3 = [f64; 3];

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(crate) fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub(crate) fn mul(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn len(a: V3) -> f64 {
    dot(a, a).sqrt()
}
pub(crate) fn unit(a: V3) -> V3 {
    let l = len(a);
    if l < 1e-300 {
        [0.0; 3]
    } else {
        mul(a, 1.0 / l)
    }
}

impl TriMesh {
    pub fn vertex(&self, i: u32) -> V3 {
        let p = self.positions[i as usize];
        [p[0] as f64, p[1] as f64, p[2] as f64]
    }

    pub fn set_vertex(&mut self, i: u32, v: V3) {
        self.positions[i as usize] = [v[0] as f32, v[1] as f32, v[2] as f32];
    }

    /// The corners of triangle `t`.
    pub fn corners(&self, t: usize) -> [V3; 3] {
        let [a, b, c] = self.triangles[t];
        [self.vertex(a), self.vertex(b), self.vertex(c)]
    }

    /// The unit normal of triangle `t` (zero for a degenerate one) and its area.
    pub fn normal_area(&self, t: usize) -> (V3, f64) {
        let [a, b, c] = self.corners(t);
        let n = cross(sub(b, a), sub(c, a));
        let l = len(n);
        if l < 1e-300 {
            ([0.0; 3], 0.0)
        } else {
            (mul(n, 1.0 / l), l * 0.5)
        }
    }

    /// The box of the mesh: (min, max); `None` for an empty mesh.
    pub fn bounding_box(&self) -> Option<(V3, V3)> {
        let mut it = self.positions.iter();
        let f = it.next()?;
        let (mut lo, mut hi) = ([f[0], f[1], f[2]], [f[0], f[1], f[2]]);
        for p in it {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        Some((
            [lo[0] as f64, lo[1] as f64, lo[2] as f64],
            [hi[0] as f64, hi[1] as f64, hi[2] as f64],
        ))
    }

    /// The signed volume (positive for a closed mesh whose triangles run counter-clockwise seen from outside).
    pub fn signed_volume(&self) -> f64 {
        (0..self.triangles.len())
            .map(|t| {
                let [a, b, c] = self.corners(t);
                dot(a, cross(b, c)) / 6.0
            })
            .sum()
    }
}
