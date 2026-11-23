// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A thread (a circle of 1 mm swept along a helix of 10 turns on a cone of 40 mm): the display mesh of every face covers the face and keeps close to
//! it, at the first mesh and at every zoom (the faces of a sweep along a long curve are meshed in short pieces).
use glam::DVec3;
use rolidium_kernel::*;

const PROFILE_D: f64 = 1.0;

fn thread() -> (Solid, Vec<DVec3>) {
    // the cone: 40 mm across at the foot, 20 mm at the top, 30 mm high; the helix lies on it, 10 turns, pitch 3 mm
    let half = ((20.0_f64 - 10.0) / 30.0).atan().to_degrees();
    let h = HelixDef {
        origin: DVec3::ZERO,
        axis: DVec3::Z,
        x_dir: DVec3::X,
        radius: 20.0,
        pitch: 3.0,
        turns: 10.0,
        right: true,
        start_deg: 0.0,
        cone_deg: -half,
    };
    let path = Path::build(PathSource::Helix(h)).unwrap();
    let t = path.start_tangent;
    let x = t.cross(DVec3::Z).normalize();
    let disc = Profile {
        loops: vec![vec![ProfileSegment::Circle {
            center: [0.0, 0.0],
            radius: PROFILE_D / 2.0,
        }]],
    };
    let input = LoftInput::Profile {
        profile: &disc,
        origin: path.start,
        normal: t,
        x_dir: x,
    };
    let s = Solid::sweep(&input, &path, &SweepOptions::default())
        .unwrap()
        .solid;
    (s, path.polyline(0.002).unwrap())
}

/// The distance of `p` from the polyline (searched near the turn at the height of `p`).
fn distance(poly: &[DVec3], p: DVec3) -> f64 {
    poly.iter().map(|q| (*q - p).length()).fold(f64::MAX, f64::min)
}

/// The least coverage of a face by its triangles, and the largest deviation of the middles of the triangles from the tube (the radius of the
/// profile minus the distance from the centre line).
fn measure(s: &Solid, m: &Mesh, poly: &[DVec3]) -> (f64, f64, usize) {
    let n = s.face_count();
    let area = m.area_per_face(n);
    let mut worst_cover = f64::MAX;
    for (i, f) in s.faces().enumerate() {
        let exact = s.face_info(f).unwrap().area;
        if exact > 1e-9 {
            worst_cover = worst_cover.min(area[i] / exact);
        }
    }
    // the middles of a sample of the triangles
    let tris = m.indices.as_chunks::<3>().0;
    let step = (tris.len() / 1500).max(1);
    let mut worst_dev = 0.0_f64;
    for t in tris.iter().step_by(step) {
        let p: Vec<DVec3> = t.iter().map(|i| m.positions[*i as usize].as_dvec3()).collect();
        for mid in [(p[0] + p[1] + p[2]) / 3.0, (p[0] + p[1]) / 2.0] {
            let d = distance(poly, mid);
            worst_dev = worst_dev.max((PROFILE_D / 2.0 - d).abs());
        }
    }
    (worst_cover, worst_dev, tris.len())
}

#[test]
fn the_first_mesh_of_a_thread_on_a_cone_covers_it_and_is_close() {
    let (s, poly) = thread();
    let m = s.display_mesh().unwrap();
    let (cover, dev, tris) = measure(&s, &m, &poly);
    eprintln!(
        "faces {} deflection {}: coverage {cover:.3}, deviation {dev:.4} mm, {tris} triangles",
        s.face_count(),
        m.deflection
    );
    assert!(cover >= 0.99, "coverage {cover}");
    assert!(dev < 0.1 * PROFILE_D, "deviation {dev} mm");
}

#[test]
fn every_zoom_of_a_thread_on_a_cone_covers_it_and_is_close() {
    let (s, poly) = thread();
    let n = s.face_count();
    let mut mesh = s.display_mesh().unwrap();
    let mut worst = (1.0_f64, 0.0_f64);
    for pixel in [0.2, 0.05, 0.02, 0.005, 0.002, 0.0005] {
        // the program asks again until the zoom is met (a pass makes a face a few times finer)
        for pass in 0..6 {
            let t = std::time::Instant::now();
            mesh = s
                .view_mesh_faces(pixel, &vec![true; n], 150_000, Some(&mesh))
                .unwrap();
            let (cover, dev, tris) = measure(&s, &mesh, &poly);
            eprintln!(
                "pixel {pixel} pass {pass}: coverage {cover:.3}, deviation {dev:.4} mm, {tris} triangles, {:.1} s",
                t.elapsed().as_secs_f64()
            );
            worst = (worst.0.min(cover), worst.1.max(dev));
        }
    }
    assert!(worst.0 >= 0.99 && worst.1 < 0.1 * PROFILE_D, "{worst:?}");
}

/// What the program decides after a pass (`remesh.rs`): whether a face still needs a finer (or a coarser) mesh for this pixel.
fn due(s: &Solid, m: &Mesh, n: usize, pixel: f64) -> bool {
    let want = s.view_deflection(pixel);
    let asked = |i: usize| {
        m.face_asked
            .get(i)
            .or_else(|| m.face_deflection.get(i))
            .copied()
            .filter(|d| *d > 0.0)
            .unwrap_or(m.deflection)
    };
    let finer = (0..n).any(|i| asked(i) > want * 1.4);
    let coarser = m.triangle_count() > 40_000 && (0..n).all(|i| asked(i) * 4.0 < want);
    finer || coarser
}

#[test]
fn a_face_that_cannot_be_made_finer_is_not_asked_for_again() {
    let (s, _) = thread();
    let n = s.face_count();
    let mut mesh = s.display_mesh().unwrap();
    for pixel in [0.2, 0.02, 0.002, 0.0002, 0.00002] {
        let mut passes = 0;
        while due(&s, &mesh, n, pixel) {
            passes += 1;
            assert!(passes <= 8, "pixel {pixel}: still due after {passes} passes");
            mesh = s.view_mesh_faces(pixel, &vec![true; n], 150_000, Some(&mesh)).unwrap();
        }
        eprintln!("pixel {pixel}: settled after {passes} pass(es), {} triangles", mesh.triangle_count());
        // at rest nothing is asked for: the same pixel again gives nothing new
        assert!(!due(&s, &mesh, n, pixel));
    }
}
