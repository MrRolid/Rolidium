// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A mirror image is a valid solid with the same volume, on the other side of the plane.
use rolidium_kernel::{BooleanOp, DVec3, Solid};

#[test]
fn a_mirrored_body_keeps_its_volume_and_lies_on_the_other_side() {
    // a plate with an off-centre hole: not symmetric, so a wrong mirror would show
    let plate = Solid::cuboid(DVec3::new(10.0, 0.0, 0.0), DVec3::new(50.0, 30.0, 8.0)).unwrap();
    let pin = Solid::cylinder(4.0, 20.0, DVec3::new(20.0, 10.0, -5.0), DVec3::Z).unwrap();
    let body = plate.boolean(&pin, BooleanOp::Cut).unwrap().solid;
    let v = body.volume().unwrap();
    let m = body.mirrored(DVec3::ZERO, DVec3::X).unwrap().solid;
    assert!(m.is_valid());
    assert!((m.volume().unwrap() - v).abs() < 1e-6, "{} vs {v}", m.volume().unwrap());
    let (a, b) = (body.props().unwrap(), m.props().unwrap());
    assert!((b.min.x + a.max.x).abs() < 1e-9 && (b.max.x + a.min.x).abs() < 1e-9, "{:?} {:?}", a.min, b.max);
    assert!((b.min.y - a.min.y).abs() < 1e-9 && (b.max.z - a.max.z).abs() < 1e-9);
    assert!((b.centroid.x + a.centroid.x).abs() < 1e-6 && (b.centroid.y - a.centroid.y).abs() < 1e-6);
    // mirrored twice: the original again
    let back = m.mirrored(DVec3::ZERO, DVec3::X).unwrap().solid;
    assert!((back.volume().unwrap() - v).abs() < 1e-6);
    // an oblique plane through a point
    let o = body.mirrored(DVec3::new(0.0, 15.0, 0.0), DVec3::new(0.0, 1.0, 1.0)).unwrap().solid;
    assert!(o.is_valid() && (o.volume().unwrap() - v).abs() < 1e-6);
    // no plane
    assert!(body.mirrored(DVec3::ZERO, DVec3::ZERO).is_err());
}
