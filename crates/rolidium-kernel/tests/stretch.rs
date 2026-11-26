// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Stretching a solid across a plane: a box grows through its straight walls and its holes and rounds keep their size.
use rolidium_kernel::{BooleanOp, DVec3, Solid};

#[test]
fn a_box_with_a_hole_on_each_side_grows_and_the_holes_keep_their_size() {
    let mut s = Solid::cuboid(DVec3::new(0.0, 0.0, 0.0), DVec3::new(40.0, 20.0, 10.0)).unwrap();
    for x in [8.0, 32.0] {
        s = s
            .boolean(
                &Solid::cylinder(3.0, 14.0, DVec3::new(x, 10.0, -2.0), DVec3::Z).unwrap(),
                BooleanOp::Cut,
            )
            .unwrap()
            .solid;
    }
    let v0 = s.volume().unwrap();
    let r = s
        .stretched(DVec3::new(20.0, 0.0, 0.0), DVec3::X, 15.0)
        .expect("stretch");
    assert!(r.is_valid());
    let (lo, hi) = r.bounding_box().unwrap();
    assert!((hi.x - lo.x - 55.0).abs() < 1e-6, "{lo} {hi}");
    // the cross-section (20 x 10) times 15 is added; the holes are not in the cross-section
    assert!(
        (r.volume().unwrap() - (v0 + 20.0 * 10.0 * 15.0)).abs() < 1e-3,
        "{}",
        r.volume().unwrap()
    );
    let holes = r
        .faces()
        .filter(|f| r.face_info(*f).unwrap().kind == rolidium_kernel::FaceKind::Cylinder)
        .count();
    assert_eq!(holes, 2);
    // the second hole moved by 15
    let cyl: Vec<DVec3> = r
        .faces()
        .filter(|f| r.face_info(*f).unwrap().kind == rolidium_kernel::FaceKind::Cylinder)
        .map(|f| r.face_info(f).unwrap().origin)
        .collect();
    assert!(
        cyl.iter().any(|o| (o.x - 47.0).abs() < 1e-6) && cyl.iter().any(|o| (o.x - 8.0).abs() < 1e-6),
        "{cyl:?}"
    );
}

#[test]
fn a_box_with_rounded_corners_keeps_its_rounds_when_stretched() {
    let b = Solid::cuboid(DVec3::new(0.0, 0.0, 0.0), DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let vertical: Vec<_> = b
        .edges()
        .filter(|e| {
            let i = b.edge_info(*e).unwrap();
            (i.start.x - i.end.x).abs() < 1e-9 && (i.start.y - i.end.y).abs() < 1e-9
        })
        .collect();
    let s = b.fillet(&vertical, 4.0).unwrap().solid;
    let v0 = s.volume().unwrap();
    let r = s.stretched(DVec3::new(20.0, 0.0, 0.0), DVec3::X, 10.0).unwrap();
    assert!(r.is_valid());
    let area = v0 / 20.0 / 1.0; // the cross-section square to x: 30 x 20 (the rounds are along z, so it is a plain rectangle)
    let _ = area;
    assert!((r.volume().unwrap() - (v0 + 30.0 * 20.0 * 10.0)).abs() < 1e-3);
    let rounds = r
        .faces()
        .filter(|f| r.face_info(*f).unwrap().kind == rolidium_kernel::FaceKind::Cylinder)
        .count();
    assert_eq!(rounds, 4);
}
