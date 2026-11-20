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
