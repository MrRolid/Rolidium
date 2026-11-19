// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Synthetic scene for `--bench-viewport`.

use crate::worker::{FaceMeta, MeshEntry};
use rolidium_kernel::{DVec3, FaceKind, Mesh};

/// A wavy sheet with about one million triangles, as one body with one face.
pub fn million_triangle_entry() -> MeshEntry {
    let n = 708usize; // n*n*2 = 1 002 528 triangles
    let mut m = Mesh::default();
    for j in 0..=n {
        for i in 0..=n {
            let (x, y) = (i as f32 / n as f32 * 200.0, j as f32 / n as f32 * 200.0);
            m.positions.push(glam::Vec3::new(
                x,
                y,
                10.0 * ((x * 0.07).sin() * (y * 0.05).cos()),
            ));
            m.normals.push(
                glam::Vec3::new(
                    -0.7 * (x * 0.07).cos() * (y * 0.05).cos(),
                    0.5 * (x * 0.07).sin() * (y * 0.05).sin(),
                    1.0,
                )
                .normalize(),
            );
        }
    }
    let row = (n + 1) as u32;
    for j in 0..n as u32 {
        for i in 0..n as u32 {
            let a = j * row + i;
            m.indices.extend([a, a + 1, a + row + 1, a, a + row + 1, a + row]);
            m.tri_face.extend([0, 0]);
        }
    }
    let face = FaceMeta {
        kind: FaceKind::Other,
        normal: DVec3::Z,
        origin: DVec3::ZERO,
        area: 0.0,
        centroid: glam::Vec3::new(100.0, 100.0, 0.0),
        cyl_diameter: None,
        disc: None,
    };
    MeshEntry {
        id: 1,
        name: "Benchmark sheet".into(),
        mesh: m,
        faces: vec![face],
        color: [0.7, 0.72, 0.8],
    }
}
