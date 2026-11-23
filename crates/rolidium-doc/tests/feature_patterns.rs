// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Holes and pins as features that a pattern or a mirror repeats in the same body (a hidden tool is made from the round faces), the second piece
//! given by a point, the corners and the grid of a flat face, patterns round an axis anywhere, a pattern that keeps the orientation. Volumes are worked
//! out by hand.
use rolidium_doc::*;
use rolidium_kernel::{BooleanOp, DVec3, FaceId, FaceKind, Solid};
use std::f64::consts::PI;

fn add_solid(d: &mut Document, name: &str, s: Solid) -> BodyId {
    d.apply(Command::AddNamedSolids {
        solids: vec![(s, name.into())],
    })
    .unwrap();
    d.bodies().last().unwrap().id
}

fn volume(d: &Document, id: BodyId) -> f64 {
    d.body(id).unwrap().solid.volume().unwrap()
}

/// A plate (0..w, 0..h, 0..t) with one through hole of radius r at (x, y).
fn plate_with_hole(w: f64, h: f64, t: f64, x: f64, y: f64, r: f64) -> Solid {
    let plate = Solid::cuboid(DVec3::ZERO, DVec3::new(w, h, t)).unwrap();
    let hole = Solid::cylinder(r, t + 2.0, DVec3::new(x, y, -1.0), DVec3::Z).unwrap();
    plate.boolean(&hole, BooleanOp::Cut).unwrap().solid
}

fn hole_face(s: &Solid) -> FaceId {
    s.faces()
        .find(|f| s.cylinder_info(*f).unwrap().is_some_and(|c| c.is_hole))
        .unwrap()
}

fn top_face(s: &Solid) -> FaceId {
    s.faces()
        .find(|f| {
            let i = s.face_info(*f).unwrap();
            i.kind == FaceKind::Plane && i.normal.z > 0.99
        })
        .unwrap()
}

fn repeat(d: &mut Document, body: BodyId, face: FaceId, kind: LinkKind) -> Result<(), DocError> {
    d.apply(Command::AddFeatureLink {
        body,
        face,
        kind,
        name: String::new(),
        rect: None,
    })
    .map(|_| ())
}

fn linear(count: u32, spacing: f64, dir: DVec3, second: Option<Direction2>) -> LinkKind {
    LinkKind::Linear(
        LinearParams {
            axis: Axis {
                origin: DVec3::ZERO,
                dir,
            },
            count,
            spacing,
            by_total: false,
            second,
        },
        PatternMode::New,
    )
}

fn hole_volume(r: f64, t: f64) -> f64 {
    PI * r * r * t
}

#[test]
fn a_hole_is_repeated_round_an_axis_that_is_not_at_the_origin() {
    // 6 holes round the axis of a pin at (40, 40); the original hole is at (60, 40), the axis anywhere
    let mut d = Document::new();
    let plate = add_solid(
        &mut d,
        "Plate",
        plate_with_hole(80.0, 80.0, 10.0, 60.0, 40.0, 3.0),
    );
    let face = hole_face(&d.body(plate).unwrap().solid);
    let v0 = volume(&d, plate);
    let kind = LinkKind::Circular(
        CircularParams {
            axis: Axis {
                origin: DVec3::new(40.0, 40.0, 0.0),
                dir: DVec3::Z,
            },
            count: 6,
            angle_deg: 0.0,
            full: true,
            keep_orientation: false,
        },
        PatternMode::New,
    );
    repeat(&mut d, plate, face, kind).unwrap();
    // five new holes (the original is one of the six); the tool body is hidden
    assert!(
        (v0 - volume(&d, plate) - 5.0 * hole_volume(3.0, 10.0)).abs() < 1e-3,
        "{} {}",
        v0,
        volume(&d, plate)
    );
    assert!(d
        .bodies()
        .iter()
        .any(|b| b.name.starts_with("Hole tool") && !b.visible));
    // the holes lie on a circle of 20 mm round (40, 40): the section at the middle of the plate
    let s = &d.body(plate).unwrap().solid;
    for k in 0..6 {
        let a = k as f64 * PI / 3.0;
        let p = DVec3::new(40.0 + 20.0 * a.cos(), 40.0 + 20.0 * a.sin(), 5.0);
        assert!(!s.contains_point(p).unwrap(), "no hole at {k}");
    }
    // everything stays a link: the number of holes is changed afterwards
    let link = d.state().links[0].id;
    let mut kind = d.state().links[0].kind.clone();
    if let LinkKind::Circular(c, _) = &mut kind {
        c.count = 3;
    }
    d.apply(Command::EditLink {
        link,
        kind: Some(kind),
        name: None,
    })
    .unwrap();
    assert!(
        (v0 - volume(&d, plate) - 2.0 * hole_volume(3.0, 10.0)).abs() < 1e-3,
        "{}",
        volume(&d, plate)
    );
}

#[test]
fn a_hole_with_a_countersink_is_repeated_whole() {
    // a through hole of 3 mm radius with a cone at the top (radius 6 at the surface, 3 at 3 mm deep)
    let plate = Solid::cuboid(DVec3::ZERO, DVec3::new(60.0, 40.0, 10.0)).unwrap();
    let bore = Solid::cylinder(3.0, 12.0, DVec3::new(10.0, 20.0, -1.0), DVec3::Z).unwrap();
    let cone = Solid::from_cone(3.0, 6.0, 3.0, DVec3::new(10.0, 20.0, 7.0), DVec3::Z).unwrap();
    let hole = bore.boolean(&cone, BooleanOp::Fuse).unwrap().solid;
    let s = plate.boolean(&hole, BooleanOp::Cut).unwrap().solid;
    let v_one = plate.volume().unwrap() - s.volume().unwrap();
    let mut d = Document::new();
    let b = add_solid(&mut d, "Plate", s);
    let face = d
        .body(b)
        .unwrap()
        .solid
        .faces()
        .find(|f| d.body(b).unwrap().solid.cylinder_info(*f).unwrap().is_some())
        .unwrap();
    repeat(&mut d, b, face, linear(3, 20.0, DVec3::X, None)).unwrap();
    // two more holes of the same shape (cylinder and countersink together)
    let v = plate_volume(60.0, 40.0, 10.0) - volume(&d, b);
    assert!((v - 3.0 * v_one).abs() < 0.05, "{v} {}", 3.0 * v_one);
}

fn plate_volume(w: f64, h: f64, t: f64) -> f64 {
    w * h * t
}

#[test]
fn a_pattern_along_a_slanting_edge_by_a_point_and_a_grid_4_by_3() {
    // the direction of a pattern is a slanting edge of a turned box: given by two points, the second piece at the second point
    let mut d = Document::new();
    let plate = add_solid(
        &mut d,
        "Plate",
        plate_with_hole(200.0, 120.0, 5.0, 20.0, 20.0, 2.0),
    );
    let face = hole_face(&d.body(plate).unwrap().solid);
    let from = DVec3::new(20.0, 20.0, 2.5);
    let to = from + DVec3::new(30.0, 40.0, 0.0); // 50 mm away, slanting
    let p = linear_through(from, to, 3).unwrap();
    assert!((p.spacing - 50.0).abs() < 1e-9 && (p.axis.dir - DVec3::new(0.6, 0.8, 0.0)).length() < 1e-9);
    let v0 = volume(&d, plate);
    repeat(&mut d, plate, face, LinkKind::Linear(p, PatternMode::New)).unwrap();
    assert!((v0 - volume(&d, plate) - 2.0 * hole_volume(2.0, 5.0)).abs() < 1e-3);
    let s = &d.body(plate).unwrap().solid;
    assert!(
        !s.contains_point(DVec3::new(50.0, 60.0, 2.5)).unwrap(),
        "the second hole"
    );
    // a grid of 4 x 3 from one hole
    let mut g = Document::new();
    let plate = add_solid(
        &mut g,
        "Plate",
        plate_with_hole(200.0, 120.0, 5.0, 20.0, 20.0, 2.0),
    );
    let face = hole_face(&g.body(plate).unwrap().solid);
    let v0 = volume(&g, plate);
    let second = Direction2 {
        axis: Axis {
            origin: DVec3::ZERO,
            dir: DVec3::Y,
        },
        count: 3,
        spacing: 25.0,
        by_total: false,
    };
    repeat(&mut g, plate, face, linear(4, 40.0, DVec3::X, Some(second))).unwrap();
    assert!(
        (v0 - volume(&g, plate) - 11.0 * hole_volume(2.0, 5.0)).abs() < 1e-3,
        "{}",
        v0 - volume(&g, plate)
    );
}

#[test]
fn four_corners_of_a_plate_100_by_60_with_a_hole_5_mm_from_the_edges() {
    let mut d = Document::new();
    let plate = add_solid(&mut d, "Plate", plate_with_hole(100.0, 60.0, 4.0, 5.0, 5.0, 1.5));
    let s = d.body(plate).unwrap().solid.clone();
    let (hole, top) = (hole_face(&s), top_face(&s));
    let v0 = volume(&d, plate);
    d.apply(Command::AddFeatureLink {
        body: plate,
        face: hole,
        kind: linear(1, 0.0, DVec3::X, None),
        name: String::new(),
        rect: Some((top, RectFill::Corners)),
    })
    .unwrap();
    assert!(
        (v0 - volume(&d, plate) - 3.0 * hole_volume(1.5, 4.0)).abs() < 1e-3,
        "{}",
        v0 - volume(&d, plate)
    );
    let s = &d.body(plate).unwrap().solid;
    for (x, y) in [(5.0, 5.0), (95.0, 5.0), (5.0, 55.0), (95.0, 55.0)] {
        assert!(
            !s.contains_point(DVec3::new(x, y, 2.0)).unwrap(),
            "no hole at ({x}, {y})"
        );
    }
    assert!(s.contains_point(DVec3::new(50.0, 30.0, 2.0)).unwrap());
    // a hole that is not in a corner: the distances to the two nearest edges are kept (symmetric): 12 mm from the left, 8 from the bottom
    let mut e = Document::new();
    let plate = add_solid(&mut e, "Plate", plate_with_hole(100.0, 60.0, 4.0, 12.0, 8.0, 1.5));
    let s = e.body(plate).unwrap().solid.clone();
    e.apply(Command::AddFeatureLink {
        body: plate,
        face: hole_face(&s),
        kind: linear(1, 0.0, DVec3::X, None),
        name: String::new(),
        rect: Some((top_face(&s), RectFill::Corners)),
    })
    .unwrap();
    let s = &e.body(plate).unwrap().solid;
    for (x, y) in [(12.0, 8.0), (88.0, 8.0), (12.0, 52.0), (88.0, 52.0)] {
        assert!(
            !s.contains_point(DVec3::new(x, y, 2.0)).unwrap(),
            "no hole at ({x}, {y})"
        );
    }
    // a grid: 3 columns x 2 rows with a margin of 5 from the far edges, from the hole in the corner
    let mut g = Document::new();
    let plate = add_solid(&mut g, "Plate", plate_with_hole(100.0, 60.0, 4.0, 5.0, 5.0, 1.5));
    let s = g.body(plate).unwrap().solid.clone();
    g.apply(Command::AddFeatureLink {
        body: plate,
        face: hole_face(&s),
        kind: linear(1, 0.0, DVec3::X, None),
        name: String::new(),
        rect: Some((
            top_face(&s),
            RectFill::Grid {
                columns: 3,
                rows: 2,
                margin: 5.0,
            },
        )),
    })
    .unwrap();
    let s = &g.body(plate).unwrap().solid;
    for (x, y) in [
        (5.0, 5.0),
        (50.0, 5.0),
        (95.0, 5.0),
        (5.0, 55.0),
        (50.0, 55.0),
        (95.0, 55.0),
    ] {
        assert!(
            !s.contains_point(DVec3::new(x, y, 2.0)).unwrap(),
            "no hole at ({x}, {y})"
        );
    }
}

#[test]
fn a_hole_is_mirrored_in_the_same_body_and_a_group_is_mirrored_with_its_bodies() {
    let mut d = Document::new();
    let plate = add_solid(
        &mut d,
        "Plate",
        plate_with_hole(100.0, 40.0, 5.0, 20.0, 20.0, 3.0),
    );
    let face = hole_face(&d.body(plate).unwrap().solid);
    let v0 = volume(&d, plate);
    // the plane x = 50 (a base plane with an offset)
    let kind = LinkKind::Mirror(MirrorParams {
        origin: DVec3::new(50.0, 0.0, 0.0),
        normal: DVec3::X,
        into: PatternMode::New,
    });
    repeat(&mut d, plate, face, kind).unwrap();
    assert!((v0 - volume(&d, plate) - hole_volume(3.0, 5.0)).abs() < 1e-3);
    assert!(!d
        .body(plate)
        .unwrap()
        .solid
        .contains_point(DVec3::new(80.0, 20.0, 2.0))
        .unwrap());
    // two bodies of a group, mirrored together
    let mut g = Document::new();
    let a = add_solid(
        &mut g,
        "A",
        Solid::cuboid(DVec3::new(10.0, 0.0, 0.0), DVec3::new(20.0, 10.0, 10.0)).unwrap(),
    );
    let b = add_solid(
        &mut g,
        "B",
        Solid::cuboid(DVec3::new(30.0, 0.0, 0.0), DVec3::new(40.0, 10.0, 10.0)).unwrap(),
    );
    g.apply(Command::CreateGroup {
        bodies: vec![a, b],
        name: "Pair".into(),
        parent: None,
    })
    .unwrap();
    g.apply(Command::Mirror {
        bodies: vec![a, b],
        origin: DVec3::ZERO,
        normal: DVec3::X,
        keep: true,
        merge: false,
    })
    .unwrap();
    assert_eq!(g.bodies().len(), 4);
    let xs: Vec<f64> = g
        .bodies()
        .iter()
        .map(|b| b.solid.props().unwrap().centroid.x)
        .collect();
    assert!(
        xs.iter().any(|x| (x + 15.0).abs() < 1e-6) && xs.iter().any(|x| (x + 35.0).abs() < 1e-6),
        "{xs:?}"
    );
}

#[test]
fn a_pattern_can_keep_the_orientation_of_the_original() {
    let mut d = Document::new();
    // a box 10 x 4 x 4 at x 20..30 round the Z axis through the origin, 4 pieces
    let b = add_solid(
        &mut d,
        "Bar",
        Solid::cuboid(DVec3::new(20.0, -2.0, 0.0), DVec3::new(30.0, 2.0, 4.0)).unwrap(),
    );
    let k = |keep| {
        LinkKind::Circular(
            CircularParams {
                axis: Axis {
                    origin: DVec3::ZERO,
                    dir: DVec3::Z,
                },
                count: 4,
                angle_deg: 0.0,
                full: true,
                keep_orientation: keep,
            },
            PatternMode::New,
        )
    };
    d.apply(Command::AddLink {
        kind: k(true),
        sources: vec![b],
        name: String::new(),
    })
    .unwrap();
    // the copy at 90 degrees stands at y = 25 and is still 10 long in x (not turned)
    let copy = d
        .bodies()
        .iter()
        .find(|x| (x.solid.props().unwrap().centroid.y - 25.0).abs() < 1e-6)
        .unwrap();
    let (lo, hi) = copy.solid.bounding_box().unwrap();
    assert!(
        (hi.x - lo.x - 10.0).abs() < 1e-6 && (hi.y - lo.y - 4.0).abs() < 1e-6,
        "{lo:?} {hi:?}"
    );
    // turned with the axis it would be 4 wide in x and 10 long in y
    let mut t = Document::new();
    let b = add_solid(
        &mut t,
        "Bar",
        Solid::cuboid(DVec3::new(20.0, -2.0, 0.0), DVec3::new(30.0, 2.0, 4.0)).unwrap(),
    );
    t.apply(Command::AddLink {
        kind: k(false),
        sources: vec![b],
        name: String::new(),
    })
    .unwrap();
    let copy = t
        .bodies()
        .iter()
        .find(|x| (x.solid.props().unwrap().centroid.y - 25.0).abs() < 1e-6)
        .unwrap();
    let (lo, hi) = copy.solid.bounding_box().unwrap();
    assert!(
        (hi.x - lo.x - 4.0).abs() < 1e-6 && (hi.y - lo.y - 10.0).abs() < 1e-6,
        "{lo:?} {hi:?}"
    );
}

#[test]
fn a_feature_links_are_saved_and_opened_and_a_feature_that_is_not_round_is_refused() {
    let mut d = Document::new();
    let plate = add_solid(&mut d, "Plate", plate_with_hole(80.0, 40.0, 5.0, 10.0, 20.0, 3.0));
    let face = hole_face(&d.body(plate).unwrap().solid);
    repeat(&mut d, plate, face, linear(3, 20.0, DVec3::X, None)).unwrap();
    let v = volume(&d, plate);
    let bytes = file::save_to_bytes(&d, None, "test").unwrap();
    let mut d2 = Document::from_loaded(file::load_from_bytes(&bytes).unwrap());
    assert_eq!(d2.state().links.len(), 1);
    assert!((volume(&d2, plate) - v).abs() < 1e-6);
    // the number is changed after the file was opened
    let link = d2.state().links[0].id;
    let mut kind = d2.state().links[0].kind.clone();
    if let LinkKind::Linear(p, _) = &mut kind {
        p.count = 2;
    }
    d2.apply(Command::EditLink {
        link,
        kind: Some(kind),
        name: None,
    })
    .unwrap();
    assert!(volume(&d2, plate) > v);
    // a flat face is not a hole: the answer says what to click
    let flat = top_face(&d2.body(plate).unwrap().solid);
    let e = d2.apply(Command::AddFeatureLink {
        body: plate,
        face: flat,
        kind: linear(2, 10.0, DVec3::X, None),
        name: String::new(),
        rect: None,
    });
    assert!(e.unwrap_err().to_string().contains("whole round hole"));
}
