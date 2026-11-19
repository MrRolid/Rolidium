// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Booleans with bodies made by sweeping: a spring wound on a cylinder (half sunk into it) and a bent tube through a box.
//! The volumes must add up: merge = A + B - common, cut = A - common, common <= min(A, B).

use rolidium_kernel::{
    BooleanOp, DVec3, HelixDef, LoftInput, Path, PathSeg, PathSource, Profile, ProfileSegment, Solid,
    SweepOptions,
};

fn check(a: &Solid, b: &Solid, what: &str) {
    let (va, vb) = (a.volume().unwrap(), b.volume().unwrap());
    let vol = |op| {
        a.boolean_checked(b, op)
            .unwrap_or_else(|e| panic!("{what}: {op:?} failed: {e:?}"))
            .solid
            .volume()
            .unwrap()
    };
    // a cut may fall into pieces (the core inside a tube); their volumes count together
    let c: f64 = a
        .boolean_pieces(b, BooleanOp::Cut)
        .unwrap_or_else(|e| panic!("{what}: cut failed: {e:?}"))
        .iter()
        .map(|p| p.volume().unwrap())
        .sum();
    let (f, i) = (vol(BooleanOp::Fuse), vol(BooleanOp::Common));
    assert!(i > 0.0 && i < va.min(vb), "{what}: common {i} (a {va}, b {vb})");
    assert!(
        (f - (va + vb - i)).abs() < 1.0,
        "{what}: merge {f} != {va} + {vb} - {i}"
    );
    assert!((c - (va - i)).abs() < 1.0, "{what}: cut {c} != {va} - {i}");
}

fn spring(radius: f64, pitch: f64, turns: f64, start: f64, wire: f64, flip: bool) -> Solid {
    let helix = HelixDef {
        origin: DVec3::ZERO,
        axis: DVec3::Z,
        x_dir: DVec3::X,
        radius,
        pitch,
        turns,
        right: true,
        start_deg: start,
        cone_deg: 0.0,
    };
    let path = Path::build(PathSource::Helix(helix)).unwrap();
    let profile = Profile {
        loops: vec![vec![ProfileSegment::Circle {
            center: [radius, 0.0],
            radius: wire / 2.0,
        }]],
    };
    let a = start.to_radians();
    let input = LoftInput::Profile {
        profile: &profile,
        origin: DVec3::ZERO,
        // the application draws on the XZ plane with the normal -Y, against the direction of the path
        normal: DVec3::new(-a.sin(), a.cos(), 0.0) * if flip { -1.0 } else { 1.0 },
        x_dir: DVec3::new(a.cos(), a.sin(), 0.0),
    };
    Solid::sweep(&input, &path, &SweepOptions::default())
        .unwrap()
        .solid
}

#[test]
fn a_spring_wound_on_a_cylinder_combines_with_it_in_every_way() {
    let cyl = Solid::cylinder(10.0, 50.0, DVec3::ZERO, DVec3::Z).unwrap();
    // These settings used to lose the spring or turn it inside out (a face wrapping a whole turn around the cylinder).
    for (pitch, wire, turns, start) in [
        (6.0, 4.0, 5.5, 0.0),
        (6.0, 4.0, 3.0, 0.0),
        (6.0, 4.0, 5.0, 0.0),
        (7.0, 4.0, 3.0, 0.0),
        (8.0, 6.0, 5.0, 0.0),
        (6.0, 3.0, 5.5, 37.0),
        (5.0, 3.0, 4.0, 90.0),
    ] {
        for flip in [false, true] {
            let s = spring(10.0, pitch, turns, start, wire, flip);
            // one closed solid with a positive volume
            assert!(s.is_valid());
            assert!(s.volume().unwrap() > 0.0);
            check(
                &cyl,
                &s,
                &format!("pitch {pitch} wire {wire} turns {turns} start {start} flip {flip}"),
            );
        }
    }
}

#[test]
fn a_bent_tube_through_a_box_combines_with_it_in_every_way() {
    let v = DVec3::new;
    let segs = [
        PathSeg::Line {
            a: v(0.0, 0.0, 0.0),
            b: v(60.0, 0.0, 0.0),
        },
        PathSeg::Arc {
            a: v(60.0, 0.0, 0.0),
            mid: v(88.2843, 11.7157, 0.0),
            b: v(100.0, 40.0, 0.0),
        },
        PathSeg::Line {
            a: v(100.0, 40.0, 0.0),
            b: v(100.0, 100.0, 0.0),
        },
    ];
    let path = Path::build(PathSource::Segments(&segs)).unwrap();
    let ring = |r| ProfileSegment::Circle {
        center: [0.0, 0.0],
        radius: r,
    };
    let profile = Profile {
        loops: vec![vec![ring(10.0)], vec![ring(8.0)]],
    };
    let input = LoftInput::Profile {
        profile: &profile,
        origin: DVec3::ZERO,
        normal: DVec3::X,
        x_dir: DVec3::Y,
    };
    let tube = Solid::sweep(&input, &path, &SweepOptions::default())
        .unwrap()
        .solid;
    assert!(tube.is_valid() && tube.volume().unwrap() > 0.0);
    // a box across the straight part, one around the bend and one across the end
    for (lo, hi) in [
        (v(20.0, -25.0, -25.0), v(40.0, 25.0, 25.0)),
        (v(70.0, -10.0, -30.0), v(110.0, 50.0, 30.0)),
        (v(80.0, 80.0, -30.0), v(120.0, 120.0, 30.0)),
    ] {
        let b = Solid::cuboid(lo, hi).unwrap();
        check(&b, &tube, &format!("box {lo} {hi}"));
        check(&tube, &b, &format!("tube vs box {lo} {hi}"));
    }
}
