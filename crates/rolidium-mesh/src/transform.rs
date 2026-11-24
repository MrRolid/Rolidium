// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Moving, turning, mirroring and scaling (also by different factors in x, y and z) a mesh.

use crate::{TriMesh, V3};

fn apply(m: &mut TriMesh, f: impl Fn(V3) -> V3) {
    for p in &mut m.positions {
        let q = f([p[0] as f64, p[1] as f64, p[2] as f64]);
        *p = [q[0] as f32, q[1] as f32, q[2] as f32];
    }
}

pub fn translate(m: &mut TriMesh, d: V3) {
    apply(m, |p| [p[0] + d[0], p[1] + d[1], p[2] + d[2]]);
}

/// Turns the mesh by `angle` (radians) about the axis through `pivot` with the direction `axis`.
pub fn rotate(m: &mut TriMesh, pivot: V3, axis: V3, angle: f64) {
    let k = crate::unit(axis);
    let (s, c) = angle.sin_cos();
    apply(m, |p| {
        let v = crate::sub(p, pivot);
        // Rodrigues
        let r = crate::add(
            crate::add(crate::mul(v, c), crate::mul(crate::cross(k, v), s)),
            crate::mul(k, crate::dot(k, v) * (1.0 - c)),
        );
        crate::add(r, pivot)
    });
}

/// Scales about `centre` by the factors `f` (they may differ). A negative factor mirrors, and the triangles are turned so that the outside stays outside.
pub fn scale(m: &mut TriMesh, centre: V3, f: V3) {
    apply(m, |p| {
        [
            centre[0] + (p[0] - centre[0]) * f[0],
            centre[1] + (p[1] - centre[1]) * f[1],
            centre[2] + (p[2] - centre[2]) * f[2],
        ]
    });
    if f[0] * f[1] * f[2] < 0.0 {
        for t in &mut m.triangles {
            t.swap(1, 2);
        }
    }
}

/// Mirrors in the plane through `origin` with the normal `normal`.
pub fn mirror(m: &mut TriMesh, origin: V3, normal: V3) {
    let n = crate::unit(normal);
    apply(m, |p| {
        let d = crate::dot(crate::sub(p, origin), n);
        crate::sub(p, crate::mul(n, 2.0 * d))
    });
    for t in &mut m.triangles {
        t.swap(1, 2);
    }
}

/// A linear pattern: the mesh and `count - 1` copies moved by `step` one after another, as one mesh.
pub fn linear_pattern(m: &TriMesh, count: usize, step: V3) -> TriMesh {
    let mut out = TriMesh::default();
    for i in 0..count.max(1) {
        let mut c = m.clone();
        translate(&mut c, crate::mul(step, i as f64));
        let base = out.positions.len() as u32;
        out.positions.extend_from_slice(&c.positions);
        out.triangles.extend(
            c.triangles
                .iter()
                .map(|t| [t[0] + base, t[1] + base, t[2] + base]),
        );
    }
    out
}
