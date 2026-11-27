// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A hollow body: walls of a given thickness, openings, closed hollow bodies, walls grown outwards.

use rolidium_kernel::{DVec3, FaceId, Solid};

fn top_face(s: &Solid) -> FaceId {
    s.faces()
        .find(|f| {
            let i = s.face_info(*f).unwrap();
            i.normal.z > 0.99
        })
        .unwrap()
}

#[test]
fn an_open_box_has_walls_of_the_thickness_inside_the_outline() {
    let b = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let r = b.shell(&[top_face(&b)], 2.0, true, false).unwrap();
    let v = r.solid.volume().unwrap();
    // the outside stays 40 x 30 x 20; the cavity is 36 x 26 x 18
    assert!((v - (24000.0 - 36.0 * 26.0 * 18.0)).abs() < 1.0, "volume {v}");
    let (lo, hi) = r.solid.bounding_box().unwrap();
    assert!((hi - lo - DVec3::new(40.0, 30.0, 20.0)).length() < 1e-6);
}

#[test]
fn walls_grown_outwards_keep_the_inside() {
    let b = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let r = b.shell(&[top_face(&b)], 2.0, false, false).unwrap();
    let (lo, hi) = r.solid.bounding_box().unwrap();
    // 44 x 34 x 22 (the top is open, so it grows by 2 at the bottom only)
    assert!(
        (hi - lo - DVec3::new(44.0, 34.0, 22.0)).length() < 1e-6,
        "{lo:?} {hi:?}"
    );
    assert!(!r.solid.contains_point(DVec3::new(20.0, 15.0, 10.0)).unwrap());
}

#[test]
fn a_closed_hollow_body_has_a_cavity() {
    let b = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let r = b.shell(&[], 2.0, true, false).unwrap();
    let v = r.solid.volume().unwrap();
    assert!((v - (24000.0 - 36.0 * 26.0 * 16.0)).abs() < 1.0, "volume {v}");
    assert!(!r.solid.contains_point(DVec3::new(20.0, 15.0, 10.0)).unwrap());
    assert!(r.solid.contains_point(DVec3::new(1.0, 1.0, 1.0)).unwrap());
}

#[test]
fn a_wall_thicker_than_the_body_is_refused() {
    let b = Solid::cuboid(DVec3::ZERO, DVec3::new(10.0, 10.0, 10.0)).unwrap();
    assert!(b.shell(&[top_face(&b)], 6.0, true, false).is_err());
}

#[test]
fn walls_on_both_sides_of_the_surface() {
    let b = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let r = b.shell_centre(&[top_face(&b)], 2.0, false).unwrap();
    let (lo, hi) = r.solid.bounding_box().unwrap();
    // one millimetre outside, one inside; the top stays where it was
    assert!(
        (hi - lo - DVec3::new(42.0, 32.0, 21.0)).length() < 1e-6,
        "{lo:?} {hi:?}"
    );
    let v = r.solid.volume().unwrap();
    // outer 42 x 32 x 21 minus the cavity 38 x 28 x 19 (open at the top)
    assert!(
        (v - (42.0 * 32.0 * 21.0 - 38.0 * 28.0 * 19.0)).abs() < 2.0,
        "volume {v}"
    );
}
