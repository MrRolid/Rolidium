// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Zooming in never takes a face away: a finer mesh of a face replaces the coarse one only when it is complete.
use glam::DVec3;
use rolidium_kernel::*;

fn area_per_face(m: &Mesh, n: usize) -> Vec<f64> {
    let mut a = vec![0.0; n];
    for (t, tri) in m.indices.as_chunks::<3>().0.iter().enumerate() {
        let p: Vec<DVec3> = tri.iter().map(|i| m.positions[*i as usize].as_dvec3()).collect();
        let f = m.tri_face[t] as usize;
        if f < n {
            a[f] += 0.5 * (p[1] - p[0]).cross(p[2] - p[0]).length();
        }
    }
    a
}

/// A circle of radius 1 swept along a helix of radius 10, pitch 4, with `turns` turns.
fn spring(turns: f64) -> Solid {
    let h = HelixDef {
        origin: DVec3::ZERO,
        axis: DVec3::Z,
        x_dir: DVec3::X,
        radius: 10.0,
        pitch: 4.0,
        turns,
        right: true,
        start_deg: 0.0,
        cone_deg: 0.0,
    };
    let path = Path::build(PathSource::Helix(h)).unwrap();
    let t = path.start_tangent;
    let x = t.cross(DVec3::Z).normalize();
    let disc = Profile {
        loops: vec![vec![ProfileSegment::Circle {
            center: [0.0, 0.0],
            radius: 1.0,
        }]],
    };
    let input = LoftInput::Profile {
        profile: &disc,
        origin: path.start,
        normal: t,
        x_dir: x,
    };
    Solid::sweep(&input, &path, &SweepOptions::default())
        .unwrap()
        .solid
}

#[test]
fn zooming_into_a_swept_helix_keeps_every_face() {
    for turns in [9.0] {
        let s = spring(turns);
        eprintln!("turns {turns}: {} faces", s.face_count());
        let n = s.face_count();
        let base = s.display_mesh().unwrap();
        let base_area = area_per_face(&base, n);
        let base_tris = base.triangles_per_face(n);
        assert!(
            base_tris.iter().all(|t| *t > 0),
            "the coarse mesh has an empty face"
        );
        for pixel in [0.05, 0.005, 0.0005] {
            // every face in view, only the even ones, only the first face
            for pick in 0..4 {
                let fine: Vec<bool> = (0..n)
                    .map(|i| match pick {
                        0 => true,
                        1 => i % 2 == 0,
                        3 => i % 3 != 0,
                        _ => i == 0,
                    })
                    .collect();
                for budget in [6_000, 150_000] {
                    let m = s.view_mesh_faces(pixel, &fine, budget, Some(&base)).unwrap();
                    let tris = m.triangles_per_face(n);
                    let area = area_per_face(&m, n);
                    for i in 0..n {
                        assert!(tris[i] > 0, "turns {turns} budget {budget} pixel {pixel} pick {pick}: face {i} has no triangles");
                        // a finer mesh has about the same area (a polygon inside the surface), never much less
                        assert!(
                            area[i] >= base_area[i] * 0.9,
                            "turns {turns} budget {budget} pixel {pixel} pick {pick}: face {i} area {} -> {}",
                            base_area[i],
                            area[i]
                        );
                    }
                    assert!(m.triangle_count() > 0);
                }
            }
        }
    }
}
