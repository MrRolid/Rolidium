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

fn peak_rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmHWM")).map(String::from))
        .and_then(|l| l.split_whitespace().nth(1).and_then(|n| n.parse::<u64>().ok()))
        .unwrap_or(0)
        / 1024
}

#[test]
fn zooming_far_into_a_swept_helix_stays_within_memory_and_time() {
    let s = spring(9.0);
    let n = s.face_count();
    let base = s.display_mesh().unwrap();
    let fine = vec![true; n];
    for pixel in [1e-3, 1e-4, 1e-5, 1e-6] {
        let t = std::time::Instant::now();
        let m = s.view_mesh_faces(pixel, &fine, 150_000, Some(&base)).unwrap();
        eprintln!(
            "pixel {pixel}: {} triangles, {:.1} s, peak {} MB",
            m.triangle_count(),
            t.elapsed().as_secs_f64(),
            peak_rss_mb()
        );
        assert!(m.triangle_count() < 400_000);
        assert!(peak_rss_mb() < 3000, "the peak memory is {} MB", peak_rss_mb());
    }
}

#[test]
fn zooming_in_step_by_step_stays_within_memory_and_time() {
    let s = spring(9.0);
    let n = s.face_count();
    let mut mesh = s.display_mesh().unwrap();
    let fine = vec![true; n];
    // each mesh is the base of the next one, as when the person zooms in a few times
    for pixel in [0.1, 0.01, 0.001, 0.0001] {
        let t = std::time::Instant::now();
        mesh = s.view_mesh_faces(pixel, &fine, 150_000, Some(&mesh)).unwrap();
        eprintln!(
            "pixel {pixel}: {} triangles, {:.1} s, peak {} MB",
            mesh.triangle_count(),
            t.elapsed().as_secs_f64(),
            peak_rss_mb()
        );
        assert!(mesh.triangle_count() < 400_000);
        assert!(peak_rss_mb() < 3000, "the peak memory is {} MB", peak_rss_mb());
    }
}

#[test]
fn zooming_far_into_single_faces_stays_within_memory_and_time() {
    let s = spring(9.0);
    let n = s.face_count();
    let base = s.display_mesh().unwrap();
    for i in 0..n {
        let fine: Vec<bool> = (0..n).map(|k| k == i).collect();
        let t = std::time::Instant::now();
        let m = s.view_mesh_faces(1e-5, &fine, 6_000, Some(&base)).unwrap();
        let secs = t.elapsed().as_secs_f64();
        eprintln!(
            "face {i}: {} triangles, {secs:.1} s, peak {} MB",
            m.triangle_count(),
            peak_rss_mb()
        );
        assert!(
            peak_rss_mb() < 3000,
            "face {i}: the peak memory is {} MB",
            peak_rss_mb()
        );
        assert!(secs < 20.0, "face {i}: {secs} s");
    }
}

#[test]
fn a_zoom_after_a_zoom_with_some_faces_in_view_stays_within_memory_and_time() {
    let s = spring(9.0);
    let n = s.face_count();
    let first = s
        .view_mesh_faces(0.0834, &vec![true; n], 150_000, Some(&s.display_mesh().unwrap()))
        .unwrap();
    for skip in 0..n {
        let fine: Vec<bool> = (0..n).map(|k| k >= 5 || k == skip).collect();
        let t = std::time::Instant::now();
        let m = s.view_mesh_faces(2.1e-4, &fine, 150_000, Some(&first)).unwrap();
        let secs = t.elapsed().as_secs_f64();
        eprintln!(
            "mask {skip}: {} triangles, {secs:.1} s, peak {} MB",
            m.triangle_count(),
            peak_rss_mb()
        );
        assert!(
            peak_rss_mb() < 3000 && secs < 20.0,
            "mask {skip}: {secs} s, {} MB",
            peak_rss_mb()
        );
    }
}

#[test]
fn three_zooms_in_a_row_with_fewer_faces_in_view_stay_within_memory_and_time() {
    let s = spring(9.0);
    let n = s.face_count();
    let mut mesh = s.display_mesh().unwrap();
    for (pixel, lo, hi) in [
        (0.0834, 0, 20),
        (2.1e-4, 5, 20),
        (2.1e-5, 10, 20),
        (2.1e-5, 0, 10),
    ] {
        let fine: Vec<bool> = (0..n).map(|k| k >= lo && k < hi).collect();
        let t = std::time::Instant::now();
        mesh = s.view_mesh_faces(pixel, &fine, 150_000, Some(&mesh)).unwrap();
        let secs = t.elapsed().as_secs_f64();
        eprintln!(
            "pixel {pixel} faces {lo}..{hi}: {} triangles, {secs:.1} s, peak {} MB",
            mesh.triangle_count(),
            peak_rss_mb()
        );
        assert!(
            peak_rss_mb() < 3000 && secs < 30.0,
            "{secs} s, {} MB",
            peak_rss_mb()
        );
    }
}
