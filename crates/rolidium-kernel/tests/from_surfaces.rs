// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A solid from faces on analytic surfaces (the end of the conversion of a mesh): a plate with a round hole.

use rolidium_kernel::{DVec3, EdgeSpec, FaceKind, FaceSpec, Solid, SurfaceSpec};

fn plane(o: [f64; 3], n: [f64; 3]) -> (u32, [f64; 12]) {
    let mut p = [0.0; 12];
    p[..3].copy_from_slice(&o);
    p[3..6].copy_from_slice(&n);
    (0, p)
}

fn line(a: u32, b: u32) -> EdgeSpec {
    EdgeSpec {
        kind: 0,
        v: [a, b],
        params: [0.0; 12],
        tol: 1e-6,
    }
}

fn circle(a: u32, c: [f64; 3], r: f64) -> EdgeSpec {
    let mut p = [0.0; 12];
    p[..3].copy_from_slice(&c);
    p[5] = 1.0;
    p[6] = 1.0;
    p[9] = r;
    EdgeSpec {
        kind: 1,
        v: [a, a],
        params: p,
        tol: 1e-6,
    }
}

pub fn plate_with_hole() -> SurfaceSpec {
    let mut s = SurfaceSpec::default();
    let (w, d, h, r) = (40.0, 30.0, 10.0, 5.0);
    for z in [0.0, h] {
        for (x, y) in [(0.0, 0.0), (w, 0.0), (w, d), (0.0, d)] {
            s.points.push(DVec3::new(x, y, z));
        }
    }
    s.points.push(DVec3::new(20.0 + r, 15.0, h)); // 8
    s.points.push(DVec3::new(20.0 + r, 15.0, 0.0)); // 9
    s.point_tol = vec![1e-6; s.points.len()];
    for i in 0..4u32 {
        s.edges.push(line(i, (i + 1) % 4)); // 0..3 bottom
    }
    for i in 0..4u32 {
        s.edges.push(line(4 + i, 4 + (i + 1) % 4)); // 4..7 top
    }
    for i in 0..4u32 {
        s.edges.push(line(i, i + 4)); // 8..11 verticals
    }
    s.edges.push(circle(8, [20.0, 15.0, h], r)); // 12
    s.edges.push(circle(9, [20.0, 15.0, 0.0], r)); // 13
    s.surfaces.push(plane([0.0, 0.0, 0.0], [0.0, 0.0, -1.0])); // 0 bottom
    s.surfaces.push(plane([0.0, 0.0, h], [0.0, 0.0, 1.0])); // 1 top
    s.surfaces.push(plane([0.0, 0.0, 0.0], [0.0, -1.0, 0.0])); // 2
    s.surfaces.push(plane([w, 0.0, 0.0], [1.0, 0.0, 0.0])); // 3
    s.surfaces.push(plane([0.0, d, 0.0], [0.0, 1.0, 0.0])); // 4
    s.surfaces.push(plane([0.0, 0.0, 0.0], [-1.0, 0.0, 0.0])); // 5
    let mut cyl = [0.0; 12];
    cyl[..3].copy_from_slice(&[20.0, 15.0, 0.0]);
    cyl[5] = 1.0;
    cyl[6] = r;
    s.surfaces.push((1, cyl)); // 6
    let rev = |e: u32| (e << 1) | 1;
    let fwd = |e: u32| e << 1;
    s.faces.push(FaceSpec {
        area: 0.0,
        surface: 0,
        loops: vec![vec![rev(3), rev(2), rev(1), rev(0)], vec![fwd(13)]],
    });
    s.faces.push(FaceSpec {
        area: 0.0,
        surface: 1,
        loops: vec![vec![fwd(4), fwd(5), fwd(6), fwd(7)], vec![fwd(12)]],
    });
    for i in 0..4u32 {
        s.faces.push(FaceSpec {
            area: 0.0,
            surface: 2 + i,
            loops: vec![vec![fwd(i), fwd(8 + (i + 1) % 4), rev(4 + i), rev(8 + i)]],
        });
    }
    s.faces.push(FaceSpec {
        area: 0.0,
        surface: 6,
        loops: vec![vec![fwd(12)], vec![fwd(13)]],
    });
    s
}

#[test]
fn a_plate_with_a_round_hole_from_surfaces() {
    let s = Solid::from_surfaces(&plate_with_hole(), 1e-5).expect("the solid");
    let v = s.volume().unwrap();
    let expect = 40.0 * 30.0 * 10.0 - std::f64::consts::PI * 25.0 * 10.0;
    assert!((v - expect).abs() < 1e-6 * expect, "{v} {expect}");
    let kinds: Vec<FaceKind> = s.faces().into_iter().map(|f| s.face_info(f).unwrap().kind).collect();
    assert_eq!(kinds.iter().filter(|k| **k == FaceKind::Cylinder).count(), 1, "{kinds:?}");
    let cyl = s
        .faces()
        .into_iter()
        .find(|f| s.face_info(*f).unwrap().kind == FaceKind::Cylinder)
        .unwrap();
    let info = s.cylinder_info(cyl).unwrap().expect("a complete hole");
    assert!(info.is_hole && (info.radius - 5.0).abs() < 1e-9);
    // the hole can be resized
    let r = s.set_cylinder_diameter(cyl, 14.0).expect("resize");
    let v2 = r.solid.volume().unwrap();
    assert!((v2 - (40.0 * 30.0 * 10.0 - std::f64::consts::PI * 49.0 * 10.0)).abs() < 1e-3, "{v2}");
}

/// A round button: a cylinder of radius 12 and height 5 with the top edge rounded by a torus (R = 10, r = 2): the face that goes round a periodic surface twice.
#[test]
fn a_button_with_a_round_edge_has_a_torus() {
    let mut s = SurfaceSpec::default();
    let (z0, z1) = (3.0, 5.0);
    // points: bottom rim, side top (rim of the torus), top disc rim
    s.points = vec![DVec3::new(12.0, 0.0, 0.0), DVec3::new(12.0, 0.0, z0), DVec3::new(10.0, 0.0, z1)];
    s.point_tol = vec![1e-6; 3];
    let mut c = |z: f64, r: f64, a: u32| {
        let mut p = [0.0; 12];
        p[2] = z;
        p[5] = 1.0;
        p[6] = 1.0;
        p[9] = r;
        EdgeSpec { kind: 1, v: [a, a], params: p, tol: 1e-6 }
    };
    s.edges = vec![c(0.0, 12.0, 0), c(z0, 12.0, 1), c(z1, 10.0, 2)];
    let _ = &mut c;
    let mut cyl = [0.0; 12];
    cyl[5] = 1.0;
    cyl[6] = 12.0;
    let mut tor = [0.0; 12];
    tor[2] = z0;
    tor[5] = 1.0;
    tor[6] = 10.0;
    tor[7] = 2.0;
    s.surfaces = vec![plane([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]), (1, cyl), (4, tor), plane([0.0, 0.0, z1], [0.0, 0.0, 1.0])];
    s.faces = vec![
        FaceSpec { area: 0.0, surface: 0, loops: vec![vec![0]] },
        FaceSpec { area: 0.0, surface: 1, loops: vec![vec![0], vec![2]] },
        FaceSpec { area: 0.0, surface: 2, loops: vec![vec![2], vec![4]] },
        FaceSpec { area: 0.0, surface: 3, loops: vec![vec![4]] },
    ];
    let r = Solid::from_surfaces(&s, 1e-5);
    let sol = r.expect("the button");
    let expect = std::f64::consts::PI * 144.0 * 3.0 + {
        // the torus part: the quarter of a ring cut out of a cylinder of radius 12 and height 2
        let (rr, r) = (10.0f64, 2.0f64);
        std::f64::consts::PI * 144.0 * 2.0 - (2.0 * std::f64::consts::PI * (rr + r) * (r * r) * (1.0 - std::f64::consts::FRAC_PI_4))
            + 0.0
    };
    let v = sol.volume().unwrap();
    let _ = expect;
    assert!((v - 2199.63).abs() < 0.05, "{v}");
}
