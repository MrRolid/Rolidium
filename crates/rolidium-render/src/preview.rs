// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Light-weight Pull preview built from an existing display mesh (no kernel call, so it is
//! cheap enough to rebuild on every mouse move).

use crate::PreviewDraw;
use glam::Vec3;
use rolidium_kernel::Mesh;
use std::collections::HashMap;

/// Translucent prism standing on `face` and extending `distance` along `normal`.
pub fn pull_preview(mesh: &Mesh, face: u32, normal: Vec3, distance: f32) -> PreviewDraw {
    let off = normal * distance;
    let color = if distance >= 0.0 {
        [0.30, 0.70, 1.00, 0.32]
    } else {
        [1.00, 0.45, 0.30, 0.32]
    };
    let mut tris = Vec::new();
    let mut lines = Vec::new();
    // Edge use count within the face; edges used once form the boundary.
    let mut edge_use: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
    for (t, tri) in mesh.indices.chunks_exact(3).enumerate() {
        if mesh.tri_face[t] != face {
            continue;
        }
        for &i in tri {
            tris.push(mesh.positions[i as usize] + off);
        }
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            let key = (a.min(b), a.max(b));
            let e = edge_use.entry(key).or_insert((0, 0));
            e.0 += 1;
            e.1 = a; // direction of the (last) use, only meaningful for boundary edges
        }
    }
    let mut corner: HashMap<u32, Vec<Vec3>> = HashMap::new();
    for ((a, b), (count, _)) in &edge_use {
        if *count != 1 {
            continue;
        }
        let (pa, pb) = (mesh.positions[*a as usize], mesh.positions[*b as usize]);
        tris.extend([pa, pb, pb + off, pa, pb + off, pa + off]);
        lines.extend([pa + off, pb + off]);
        corner.entry(*a).or_default().push((pb - pa).normalize_or_zero());
        corner.entry(*b).or_default().push((pa - pb).normalize_or_zero());
    }
    // Vertical lines only at real corners (not along smooth circles).
    for (v, dirs) in corner {
        let sharp = dirs.len() != 2 || dirs[0].dot(dirs[1]) > -0.97;
        if sharp {
            let p = mesh.positions[v as usize];
            lines.extend([p, p + off]);
        }
    }
    PreviewDraw {
        triangles: tris,
        lines,
        color,
        line_color: [color[0], color[1], color[2], 1.0],
        ..Default::default()
    }
}

/// Boundary segments of one face of a display mesh and its sharp corners.
pub fn face_boundary(mesh: &Mesh, face: u32) -> (Vec<(Vec3, Vec3)>, Vec<Vec3>) {
    let mut use_count: HashMap<(u32, u32), u32> = HashMap::new();
    for (t, tri) in mesh.indices.chunks_exact(3).enumerate() {
        if mesh.tri_face[t] != face {
            continue;
        }
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            *use_count.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let mut segs = Vec::new();
    let mut dirs: HashMap<u32, Vec<Vec3>> = HashMap::new();
    for ((a, b), n) in &use_count {
        if *n != 1 {
            continue;
        }
        let (pa, pb) = (mesh.positions[*a as usize], mesh.positions[*b as usize]);
        segs.push((pa, pb));
        dirs.entry(*a).or_default().push((pb - pa).normalize_or_zero());
        dirs.entry(*b).or_default().push((pa - pb).normalize_or_zero());
    }
    let corners = dirs
        .into_iter()
        .filter(|(_, d)| d.len() != 2 || d[0].dot(d[1]) > -0.97)
        .map(|(v, _)| mesh.positions[v as usize])
        .collect();
    (segs, corners)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rolidium_kernel::{DVec3, Solid};

    #[test]
    fn box_face_preview_has_cap_sides_and_four_corners() {
        let s = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
        let m = s.display_mesh().unwrap();
        let top = s
            .faces()
            .find(|f| s.face_info(*f).unwrap().normal.z > 0.99)
            .unwrap();
        let p = pull_preview(&m, top.0, Vec3::Z, 10.0);
        // 2 cap triangles + 4 boundary edges * 2 side triangles
        assert_eq!(p.triangles.len(), 3 * (2 + 8));
        // 4 cap outline segments + 4 vertical corner lines
        assert_eq!(p.lines.len(), 2 * (4 + 4));
        assert!(p
            .triangles
            .iter()
            .all(|v| v.z >= 20.0 - 1e-4 && v.z <= 30.0 + 1e-4));
    }
}
