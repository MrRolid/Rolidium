// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The faces of a loft are of the types they geometrically are: two coaxial circles give a cone or a cylinder, two similar polygons flat sides.
use rolidium_kernel::*;

fn circle(r: f64, cx: f64) -> Profile {
    Profile {
        loops: vec![vec![ProfileSegment::Circle {
            center: [cx, 0.0],
            radius: r,
        }]],
    }
}

fn rect(w: f64, h: f64) -> Profile {
    let (a, b) = (w / 2.0, h / 2.0);
    let p = [[-a, -b], [a, -b], [a, b], [-a, b]];
    Profile {
        loops: vec![(0..4)
            .map(|i| ProfileSegment::Line {
                a: p[i],
                b: p[(i + 1) % 4],
            })
            .collect()],
    }
}

fn at(p: &Profile, z: f64) -> LoftInput<'_> {
    LoftInput::Profile {
        profile: p,
        origin: DVec3::new(0.0, 0.0, z),
        normal: DVec3::Z,
        x_dir: DVec3::X,
    }
}

fn kinds(s: &Solid) -> Vec<FaceKind> {
    s.faces().map(|f| s.face_info(f).unwrap().kind).collect()
}

fn count(s: &Solid, k: FaceKind) -> usize {
    kinds(s).iter().filter(|x| **x == k).count()
}

#[test]
fn two_coaxial_circles_make_a_cone_or_a_cylinder() {
    let (a, b) = (circle(20.0, 0.0), circle(10.0, 0.0));
    let cone = Solid::loft(&[at(&a, 0.0), at(&b, 30.0)], false).unwrap();
    assert_eq!((cone.face_count(), count(&cone, FaceKind::Plane), count(&cone, FaceKind::Cone)), (3, 2, 1));
    let v = std::f64::consts::PI * 30.0 / 3.0 * (400.0 + 200.0 + 100.0);
    assert!((cone.volume().unwrap() - v).abs() < 1e-6 * v);
    // the same with the smooth loft and the other way round (the larger circle on top)
    let up = Solid::loft(&[at(&b, 0.0), at(&a, 30.0)], true).unwrap();
    assert_eq!(count(&up, FaceKind::Cone), 1);
    let cyl = Solid::loft(&[at(&a, 0.0), at(&a, 30.0)], false).unwrap();
    assert_eq!((cyl.face_count(), count(&cyl, FaceKind::Cylinder)), (3, 1));
    // the cone has a measurable cone: the information a cone gets for editing
    let side = cone.faces().find(|f| cone.face_info(*f).unwrap().kind == FaceKind::Cone).unwrap();
    let ci = cone.cone_info(side).unwrap().unwrap();
    assert!((ci.r_base - 20.0).abs() < 1e-6 && (ci.r_top - 10.0).abs() < 1e-6 && (ci.height - 30.0).abs() < 1e-6, "{ci:?}");
    // a STEP file keeps the types
    let back = Solid::read_step(&cone.write_step().unwrap()).unwrap();
    assert_eq!(count(&back, FaceKind::Cone), 1);
}

#[test]
fn circles_that_are_not_coaxial_stay_freeform() {
    let (a, b) = (circle(20.0, 0.0), circle(10.0, 8.0));
    let oblique = Solid::loft(&[at(&a, 0.0), at(&b, 30.0)], false).unwrap();
    assert!(oblique.is_valid());
    assert_eq!(count(&oblique, FaceKind::Cone), 0, "an oblique frustum is not a cone");
    assert_eq!(oblique.face_count(), 3);
    let (lo, hi) = oblique.bounding_box().unwrap();
    assert!(hi.x >= 20.0 - 1e-6 && lo.z.abs() < 1e-5 && (hi.z - 30.0).abs() < 1e-5, "{lo:?} {hi:?}");
}

#[test]
fn similar_rectangles_make_flat_sides() {
    let (a, b) = (rect(40.0, 30.0), rect(20.0, 15.0));
    let s = Solid::loft(&[at(&a, 0.0), at(&b, 25.0)], false).unwrap();
    assert_eq!((s.face_count(), count(&s, FaceKind::Plane)), (6, 6));
    assert!(s.is_valid());
    // a prism, and a wedge (two sides parallel, the other two not similar): flat as well when the sides are parallel in pairs
    let prism = Solid::loft(&[at(&a, 0.0), at(&a, 25.0)], false).unwrap();
    assert_eq!(count(&prism, FaceKind::Plane), 6);
    let (lo, hi) = s.bounding_box().unwrap();
    assert!((hi - lo - DVec3::new(40.0, 30.0, 25.0)).length() < 1e-9);
}
