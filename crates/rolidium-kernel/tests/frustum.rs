// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The ends of a cone, a frustum and a cylinder: a new diameter and a pull along the axis make the side follow.
use rolidium_kernel::*;
use std::f64::consts::PI;

fn frustum_volume(r1: f64, r2: f64, h: f64) -> f64 {
    PI * h / 3.0 * (r1 * r1 + r1 * r2 + r2 * r2)
}

fn circle(r: f64) -> Profile {
    Profile {
        loops: vec![vec![ProfileSegment::Circle {
            center: [0.0, 0.0],
            radius: r,
        }]],
    }
}

fn frustum(r1: f64, r2: f64, h: f64) -> Solid {
    let (a, b) = (circle(r1), circle(r2));
    fn at(p: &Profile, z: f64) -> LoftInput<'_> {
        LoftInput::Profile {
            profile: p,
            origin: DVec3::new(0.0, 0.0, z),
            normal: DVec3::Z,
            x_dir: DVec3::X,
        }
    }
    Solid::loft(&[at(&a, 0.0), at(&b, h)], false).unwrap()
}

fn info(s: &Solid) -> FrustumInfo {
    s.frustum_info().expect("a plain cone")
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6 * (1.0 + a.abs()), "{a} != {b}");
}

#[test]
fn the_diameter_of_one_end_changes_and_the_other_end_stays() {
    let s = frustum(20.0, 10.0, 30.0);
    let f = info(&s);
    // the top: 32 mm across
    let r = s.set_cap_diameter(f.cap_top, 32.0, false).unwrap();
    let n = info(&r.solid);
    assert_eq!(r.solid.face_count(), 3);
    assert_eq!(r.solid.face_info(n.side).unwrap().kind, FaceKind::Cone);
    close(n.r_base, 20.0);
    close(n.r_top, 16.0);
    close(n.height, 30.0);
    close(r.solid.volume().unwrap(), frustum_volume(20.0, 16.0, 30.0));
    // the bottom
    let r2 = s.set_cap_diameter(f.cap_base, 50.0, false).unwrap();
    let n2 = info(&r2.solid);
    close(n2.r_base, 25.0);
    close(n2.r_top, 10.0);
    // the history says where the faces went
    assert_eq!(r2.history.successors(0, f.cap_top), vec![n2.cap_top]);
}

#[test]
fn with_keep_angle_the_other_end_follows() {
    let s = frustum(20.0, 10.0, 30.0);
    let f = info(&s);
    let slope = f.slope();
    let r = s.set_cap_diameter(f.cap_top, 32.0, true).unwrap();
    let n = info(&r.solid);
    close(n.r_top, 16.0);
    close(n.r_base, 26.0); // the difference of the radii stays, so does the angle
    close(n.slope(), slope);
    // a cylinder with both ends following stays a cylinder
    let c = Solid::cylinder(10.0, 25.0, DVec3::ZERO, DVec3::Z).unwrap();
    let fc = info(&c);
    let r = c.set_cap_diameter(fc.cap_top, 30.0, true).unwrap();
    assert_eq!(
        r.solid.face_info(info(&r.solid).side).unwrap().kind,
        FaceKind::Cylinder
    );
    close(r.solid.volume().unwrap(), PI * 225.0 * 25.0);
    // without keeping the angle one end alone makes a cone of the cylinder
    let r = c.set_cap_diameter(fc.cap_top, 30.0, false).unwrap();
    assert_eq!(
        r.solid.face_info(info(&r.solid).side).unwrap().kind,
        FaceKind::Cone
    );
}

#[test]
fn pulling_an_end_extends_or_shortens_the_cone_with_its_angle() {
    let s = frustum(20.0, 10.0, 30.0);
    let f = info(&s);
    let slope = f.slope(); // -1/3
    let up = s.pull_face(f.cap_top, 6.0).unwrap();
    let n = info(&up.solid);
    close(n.height, 36.0);
    close(n.r_top, 10.0 + slope * 6.0);
    close(n.r_base, 20.0);
    assert_eq!(up.solid.face_info(n.side).unwrap().kind, FaceKind::Cone);
    close(
        up.solid.volume().unwrap(),
        frustum_volume(20.0, 10.0 + slope * 6.0, 36.0),
    );
    // shorter, from the bottom end (the end that is pulled inwards)
    let down = s.pull_face(f.cap_base, -9.0).unwrap();
    let m = info(&down.solid);
    close(m.height, 21.0);
    close(m.r_top, 10.0);
    close(m.r_base, 20.0 + slope * 9.0);
    // it stays where it was: the top end is at z = 30
    let (lo, hi) = down.solid.bounding_box().unwrap();
    assert!(
        (hi.z - 30.0).abs() < 1e-5 && (lo.z - 9.0).abs() < 1e-5,
        "{lo:?} {hi:?}"
    );
    // past the point of the cone, and to nothing: refused with a reason
    let e = s.pull_face(f.cap_top, 40.0).expect_err("past the apex");
    assert!(e.to_string().contains("point"), "{e}");
    assert!(s.pull_face(f.cap_top, -31.0).is_err());
}

#[test]
fn a_cone_from_a_step_file_or_a_revolve_is_edited_the_same_way() {
    let s = frustum(20.0, 10.0, 30.0);
    let back = Solid::read_step(&s.write_step().unwrap()).unwrap();
    let f = info(&back);
    let r = back.set_cap_diameter(f.cap_top, 24.0, false).unwrap();
    close(info(&r.solid).r_top, 12.0);
    // a revolve of a trapezoid
    let p = Profile {
        loops: vec![vec![
            ProfileSegment::Line {
                a: [0.0, 0.0],
                b: [20.0, 0.0],
            },
            ProfileSegment::Line {
                a: [20.0, 0.0],
                b: [10.0, 30.0],
            },
            ProfileSegment::Line {
                a: [10.0, 30.0],
                b: [0.0, 30.0],
            },
            ProfileSegment::Line {
                a: [0.0, 30.0],
                b: [0.0, 0.0],
            },
        ]],
    };
    let rev = Solid::revolve_profile(
        &p,
        DVec3::ZERO,
        -DVec3::Y,
        DVec3::X,
        DVec3::ZERO,
        DVec3::Z,
        std::f64::consts::TAU,
    )
    .unwrap();
    let f = info(&rev);
    let up = rev.pull_face(f.cap_top, 3.0).unwrap();
    close(info(&up.solid).height, 33.0);
}

#[test]
fn a_cone_inside_a_bigger_solid_says_why_it_cannot_follow() {
    let b = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let f = b.faces().next().unwrap();
    assert!(b.frustum_info().is_none());
    let e = b.set_cap_diameter(f, 10.0, false).expect_err("not a cone");
    assert!(e.to_string().contains("not a plain cone"), "{e}");
}
