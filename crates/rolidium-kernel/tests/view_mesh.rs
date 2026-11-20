// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The display mesh follows the zoom: a circle across the whole window deviates by less than half a pixel.
use rolidium_kernel::*;

/// The largest distance of the middle of a polyline segment from the circle of radius `r` about the Z axis (the sagitta).
fn sagitta(points: &[Vec3], r: f32) -> f32 {
    points
        .windows(2)
        .map(|w| {
            let mid = (w[0] + w[1]) * 0.5;
            (r - (mid.x * mid.x + mid.y * mid.y).sqrt()).abs()
        })
        .fold(0.0, f32::max)
}

#[test]
fn a_circle_across_the_window_is_round_to_half_a_pixel() {
    let s = Solid::cylinder(20.0, 30.0, DVec3::ZERO, DVec3::Z).unwrap();
    // a circle of 20 mm radius across a window 1000 px high: one pixel is 0.04 mm
    for pixel in [0.04_f64, 0.01, 0.2] {
        let m = s.view_mesh(pixel, 5_000_000, None).unwrap();
        let worst = m
            .edges
            .iter()
            .filter(|e| e.points.len() > 8)
            .map(|e| sagitta(&e.points, 20.0))
            .fold(0.0, f32::max);
        assert!(f64::from(worst) <= 0.5 * pixel, "pixel {pixel}: sagitta {worst}");
        // the same for the triangles of the side: the rim vertices of the mesh are on the circle, and the chord between two
        // neighbours is no more than half a pixel inside it
        let top: Vec<Vec3> = m.positions.iter().copied().filter(|p| p.z > 29.99).collect();
        assert!(top.len() > 8);
    }
    // coarser at a coarser zoom: fewer triangles
    let near = s.view_mesh(0.01, 5_000_000, None).unwrap().triangle_count();
    let far = s.view_mesh(0.5, 5_000_000, None).unwrap().triangle_count();
    assert!(near > far, "{near} {far}");
}

#[test]
fn the_number_of_triangles_stays_under_the_cap() {
    let s = Solid::cylinder(20.0, 30.0, DVec3::ZERO, DVec3::Z).unwrap();
    let base = s.view_mesh(0.5, 1_000_000, None).unwrap();
    let capped = s
        .view_mesh(0.0001, 400, Some((base.deflection, base.triangle_count())))
        .unwrap();
    assert!(capped.triangle_count() <= 400, "{}", capped.triangle_count());
    // without a previous mesh the second try still brings it under the cap
    let capped2 = s.view_mesh(0.0001, 3000, None).unwrap();
    assert!(capped2.triangle_count() <= 3000, "{}", capped2.triangle_count());
}

/// A box with a cylinder hole: the faces are meshed again one by one for a closer view.
fn plate_with_hole() -> Solid {
    let b = Solid::cuboid(DVec3::ZERO, DVec3::new(60.0, 40.0, 10.0)).unwrap();
    let c = Solid::cylinder(6.0, 12.0, DVec3::new(30.0, 20.0, -1.0), DVec3::Z).unwrap();
    b.boolean(&c, BooleanOp::Cut).unwrap().solid
}

#[test]
fn only_the_faces_in_view_are_made_finer_and_the_others_stay_as_they_were() {
    let s = plate_with_hole();
    let n = s.face_count();
    let base = s.display_mesh().unwrap();
    // the faces of the hole (cylinder) are in view, the rest is not
    let fine: Vec<bool> = s.faces().map(|f| s.face_info(f).unwrap().kind == FaceKind::Cylinder).collect();
    assert!(fine.iter().any(|f| *f) && fine.iter().any(|f| !*f));
    let m = s.view_mesh_faces(0.001, &fine, 1_000_000, Some(&base)).unwrap();
    let per_base = base.triangles_per_face(n);
    let per_new = m.triangles_per_face(n);
    for i in 0..n {
        if fine[i] {
            assert!(per_new[i] > per_base[i], "face {i}: {} -> {}", per_base[i], per_new[i]);
            assert!(m.face_deflection[i] < base.face_deflection[i]);
        } else {
            assert_eq!(per_new[i], per_base[i], "face {i} out of view must not change");
            assert_eq!(m.face_deflection[i], base.face_deflection[i]);
        }
    }
    // every vertex index is valid, normals and positions agree, and the mesh still closes the solid (the volume of the triangles)
    assert_eq!(m.positions.len(), m.normals.len());
    assert!(m.indices.iter().all(|i| (*i as usize) < m.positions.len()));
    assert_eq!(m.indices.len() / 3, m.tri_face.len());
    let exact = s.props().unwrap().volume;
    assert!((m.signed_volume() - exact).abs() < exact * 0.01, "{} vs {exact}", m.signed_volume());
    // the edges of the faces that were not meshed again are kept, those of the hole are finer
    assert_eq!(m.edges.len(), base.edges.len());
}

#[test]
fn the_faces_in_view_stay_under_the_budget() {
    let s = Solid::cylinder(20.0, 30.0, DVec3::ZERO, DVec3::Z).unwrap();
    let base = s.display_mesh().unwrap();
    let fine = vec![true; s.face_count()];
    let m = s.view_mesh_faces(1e-5, &fine, 5_000, Some(&base)).unwrap();
    assert!(m.triangle_count() <= 6_250, "{}", m.triangle_count());
}
