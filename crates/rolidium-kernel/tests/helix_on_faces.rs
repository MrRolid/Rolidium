// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A helix made from a click on a conical or a cylindrical face lies on that face: at the start and at the end (and everywhere between)
//! its distance from the axis is the radius of the face at that height, and its axis is the axis of the face.
use glam::DVec3;
use rolidium_kernel::*;

fn revolved(points: &[[f64; 2]]) -> Solid {
    let n = points.len();
    let loop_: Vec<ProfileSegment> = (0..n)
        .map(|i| ProfileSegment::Line {
            a: points[i],
            b: points[(i + 1) % n],
        })
        .collect();
    let p = Profile { loops: vec![loop_] };
    // the plane XZ: u = x, v = z
    Solid::revolve_profile(
        &p,
        DVec3::ZERO,
        -DVec3::Y,
        DVec3::X,
        DVec3::ZERO,
        DVec3::Z,
        std::f64::consts::TAU,
    )
    .unwrap()
}

fn check_on_axis(
    poly: &[DVec3],
    axis_origin: DVec3,
    axis: DVec3,
    radius_at: &dyn Fn(f64) -> f64,
    what: &str,
) {
    for p in poly {
        let t = (*p - axis_origin).dot(axis);
        let r = (*p - axis_origin - axis * t).length();
        assert!(
            (r - radius_at(t)).abs() < 0.001,
            "{what}: at height {t} the distance from the axis is {r}, the face has {}",
            radius_at(t)
        );
    }
}

fn helix_on(info: &ConeInfo, pitch: f64, right: bool) -> Vec<DVec3> {
    let h = HelixDef {
        origin: info.base,
        axis: info.axis,
        x_dir: DVec3::X,
        radius: info.r_base,
        pitch,
        turns: info.height / pitch.abs(),
        right,
        start_deg: 0.0,
        cone_deg: info.half_angle_deg,
    };
    Path::build(PathSource::Helix(h)).unwrap().polyline(0.01).unwrap()
}

#[test]
fn a_helix_on_a_cone_narrowing_up_and_down_lies_on_it() {
    for (name, pts) in [
        (
            "narrowing up",
            [[0.0, 0.0], [10.0, 0.0], [4.0, 20.0], [0.0, 20.0]],
        ),
        (
            "narrowing down",
            [[0.0, 0.0], [4.0, 0.0], [10.0, 20.0], [0.0, 20.0]],
        ),
    ] {
        let s = revolved(&pts);
        let face = s
            .faces()
            .find(|f| s.face_info(*f).unwrap().kind == FaceKind::Cone)
            .expect("a conical face");
        let info = s.cone_info(face).unwrap().expect("the face is a cone");
        assert!(
            (info.axis - DVec3::Z).length() < 1e-9,
            "{name}: the axis is the axis of the cone"
        );
        assert!(
            info.base.length() < 1e-6,
            "{name}: the start is at the lower rim {:?}",
            info.base
        );
        assert!(
            (info.height - 20.0).abs() < 1e-3,
            "{name}: the height of the face"
        );
        let (rb, rt) = (pts[1][0], pts[2][0]);
        assert!(
            (info.r_base - rb).abs() < 1e-3 && (info.r_top - rt).abs() < 1e-3,
            "{name}: radii {} {}",
            info.r_base,
            info.r_top
        );
        let slope = (rt - rb) / 20.0;
        for (pitch, right) in [(5.0, true), (5.0, false)] {
            let poly = helix_on(&info, pitch, right);
            check_on_axis(&poly, info.base, info.axis, &|t| rb + slope * t, name);
            let first = poly.first().unwrap();
            let last = poly.last().unwrap();
            assert!(
                first.z.abs() < 1e-6 && ((first.truncate().length()) - rb).abs() < 0.001,
                "{name}: the start is on the lower rim"
            );
            assert!(
                (last.z - 20.0).abs() < 0.01,
                "{name}: the end is at the upper rim, z {}",
                last.z
            );
            assert!(
                (last.truncate().length() - rt).abs() < 0.001,
                "{name}: radius at the end {}",
                last.truncate().length()
            );
        }
    }
}

#[test]
fn a_helix_on_a_cylinder_lies_on_it() {
    let s = Solid::cylinder(8.0, 30.0, DVec3::new(5.0, -3.0, 2.0), DVec3::Z).unwrap();
    let face = s
        .faces()
        .find(|f| s.cylinder_info(*f).unwrap().is_some())
        .unwrap();
    let c = s.cylinder_info(face).unwrap().unwrap();
    let h = HelixDef {
        origin: c.base,
        axis: c.axis,
        x_dir: DVec3::X,
        radius: c.radius,
        pitch: 6.0,
        turns: c.height / 6.0,
        right: true,
        start_deg: 0.0,
        cone_deg: 0.0,
    };
    let poly = Path::build(PathSource::Helix(h)).unwrap().polyline(0.01).unwrap();
    check_on_axis(&poly, c.base, c.axis, &|_| 8.0, "cylinder");
    assert!((c.axis - DVec3::Z).length() < 1e-9 && (c.base - DVec3::new(5.0, -3.0, 2.0)).length() < 1e-6);
    assert!((poly.last().unwrap().z - (2.0 + 30.0)).abs() < 0.01);
}
