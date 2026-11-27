// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A pattern along a curve of any shape: the pieces stand at equal distances along it (by the count or by the pitch), with the distances left free at the ends, evenly over
//! a closed curve, and turn with the tangent when asked; the link is kept in the file and is one step of Undo.
use rolidium_doc::{path_places, Command, Document, LinkKind, PathParams, PatternMode};
use rolidium_kernel::{DVec3, Solid};

fn params(points: Vec<DVec3>, closed: bool) -> PathParams {
    PathParams {
        points,
        closed,
        count: 5,
        pitch: 0.0,
        start: 0.0,
        end: 0.0,
        orient: false,
        source: None,
    }
}

fn line() -> Vec<DVec3> {
    vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(100.0, 0.0, 0.0)]
}

#[test]
fn the_places_by_count_by_pitch_with_ends_free_and_round_a_closed_curve() {
    let by_count = path_places(&params(line(), false)).unwrap();
    assert_eq!(by_count.len(), 5);
    for (i, (p, t)) in by_count.iter().enumerate() {
        assert!((p.x - 25.0 * i as f64).abs() < 1e-9 && p.y == 0.0, "{p:?}");
        assert!((t.x - 1.0).abs() < 1e-9);
    }
    // the pitch: 30 mm fits 4 times (0, 30, 60, 90)
    let mut p = params(line(), false);
    p.pitch = 30.0;
    assert_eq!(path_places(&p).unwrap().len(), 4);
    // 10 mm free at both ends, 3 pieces: 10, 50, 90
    let mut p = params(line(), false);
    (p.count, p.start, p.end) = (3, 10.0, 10.0);
    let v = path_places(&p).unwrap();
    assert_eq!(
        v.iter().map(|(q, _)| q.x.round() as i64).collect::<Vec<_>>(),
        vec![10, 50, 90]
    );
    // a closed square of 40 mm: 8 pieces are 10 mm apart round the whole length (the start point is not repeated)
    let sq = vec![
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(40.0, 0.0, 0.0),
        DVec3::new(40.0, 40.0, 0.0),
        DVec3::new(0.0, 40.0, 0.0),
    ];
    let mut p = params(sq, true);
    p.count = 8;
    let v = path_places(&p).unwrap();
    assert_eq!(v.len(), 8);
    assert!(
        (v[1].0 - DVec3::new(20.0, 0.0, 0.0)).length() < 1e-9,
        "{:?}",
        v[1]
    );
    assert!(
        (v[4].0 - DVec3::new(40.0, 40.0, 0.0)).length() < 1e-9,
        "{:?}",
        v[4]
    );
    // a curve shorter than the free ends, or with no length
    let mut p = params(line(), false);
    (p.start, p.end) = (60.0, 60.0);
    assert!(path_places(&p).is_err());
    assert!(path_places(&params(vec![DVec3::ZERO], false)).is_err());
}

#[test]
fn copies_stand_along_a_line_and_turn_with_the_tangent_of_a_corner_curve_in_one_step() {
    let mut d = Document::new();
    d.apply(Command::AddNamedSolids {
        solids: vec![(
            Solid::cuboid(DVec3::ZERO, DVec3::new(10.0, 4.0, 4.0)).unwrap(),
            "pin".into(),
        )],
    })
    .unwrap();
    let src = d.bodies()[0].id;
    // along X: the copies do not turn
    d.apply(Command::AddLink {
        kind: LinkKind::Path(params(line(), false), PatternMode::New),
        sources: vec![src],
        name: String::new(),
    })
    .unwrap();
    assert_eq!(d.bodies().len(), 5);
    let xs: Vec<f64> = d
        .bodies()
        .iter()
        .map(|b| b.solid.bounding_box().unwrap().0.x)
        .collect();
    for (i, x) in xs.iter().enumerate() {
        assert!((x - 25.0 * i as f64).abs() < 1e-6, "{xs:?}");
    }
    assert!(d.undo());
    assert_eq!(d.bodies().len(), 1, "one step of Undo takes all the copies");
    // an L-shaped curve: along X then along Y; the piece that stands on the second leg is turned a quarter turn when `orient` is set
    let l = vec![
        DVec3::ZERO,
        DVec3::new(50.0, 0.0, 0.0),
        DVec3::new(50.0, 50.0, 0.0),
    ];
    let mut p = params(l, false);
    p.count = 3;
    p.orient = true;
    d.apply(Command::AddLink {
        kind: LinkKind::Path(p, PatternMode::New),
        sources: vec![src],
        name: String::new(),
    })
    .unwrap();
    assert_eq!(d.bodies().len(), 3);
    let last = d.bodies().last().unwrap().solid.bounding_box().unwrap();
    // the last piece is at the end (50, 50) pointing along Y: 4 wide in X and 10 long in Y
    let (lo, hi) = last;
    assert!(
        ((hi.x - lo.x) - 4.0).abs() < 1e-6 && ((hi.y - lo.y) - 10.0).abs() < 1e-6,
        "{lo:?} {hi:?}"
    );
    // the link is kept in a file
    let bytes = rolidium_doc::file::save_to_bytes(&d, None, "test").unwrap();
    let d2 = Document::from_loaded(rolidium_doc::file::load_from_bytes(&bytes).unwrap());
    assert!(
        d2.state()
            .links
            .iter()
            .any(|l| matches!(l.kind, LinkKind::Path(..))),
        "the link comes back"
    );
}

#[test]
fn a_pattern_remembers_its_curve_and_the_pieces_move_when_the_curve_changes() {
    use rolidium_doc::rolidium_sketch::{DVec2, Entity, PlaneDef};
    use rolidium_doc::{SketchTarget, SweepPath};
    let mut d = Document::new();
    d.apply(Command::AddNamedSolids {
        solids: vec![(
            Solid::cuboid(DVec3::new(18.0, -2.0, -2.0), DVec3::new(22.0, 2.0, 2.0)).unwrap(),
            "pin".into(),
        )],
    })
    .unwrap();
    let src = d.bodies()[0].id;
    let plane = PlaneDef {
        origin: DVec3::ZERO,
        normal: DVec3::Z,
        x_dir: DVec3::X,
    };
    let c = d
        .apply(Command::AddEntity {
            target: SketchTarget::New { plane, body: None },
            entity: Entity::Circle {
                center: DVec2::ZERO,
                radius: 20.0,
            },
        })
        .unwrap();
    let sk = c.created_sketch.unwrap();
    let e = d.sketch(sk).unwrap().sketch.entities()[0].0;
    let (pts, closed) = d
        .path_polyline(&SweepPath::Sketch {
            sketch: sk,
            entities: vec![e],
        })
        .unwrap();
    let mut p = params(pts, closed);
    p.count = 4;
    p.source = Some(SweepPath::Sketch {
        sketch: sk,
        entities: vec![e],
    });
    d.apply(Command::AddLink {
        kind: LinkKind::Path(p, PatternMode::New),
        sources: vec![src],
        name: String::new(),
    })
    .unwrap();
    // the distance of two neighbouring pieces: the side of a square on the circle (radius r: r * sqrt 2)
    let side = |d: &Document| {
        let c = |i: usize| {
            let (lo, hi) = d.bodies()[i].solid.bounding_box().unwrap();
            (lo + hi) * 0.5
        };
        (c(1) - c(2)).length()
    };
    assert!(
        (side(&d) - 20.0 * 2f64.sqrt()).abs() < 0.3,
        "the pieces stand on the circle of 20: {}",
        side(&d)
    );
    // the circle is made bigger: the pieces move with it
    d.apply(Command::ReplaceEntity {
        sketch: sk,
        entity: e,
        new: Entity::Circle {
            center: DVec2::ZERO,
            radius: 35.0,
        },
    })
    .unwrap();
    assert!(
        (side(&d) - 35.0 * 2f64.sqrt()).abs() < 0.3,
        "the pieces follow the curve: {}",
        side(&d)
    );
    // the file keeps the curve
    let bytes = rolidium_doc::file::save_to_bytes(&d, None, "test").unwrap();
    let d2 = Document::from_loaded(rolidium_doc::file::load_from_bytes(&bytes).unwrap());
    let l = d2
        .state()
        .links
        .iter()
        .find(|l| matches!(l.kind, LinkKind::Path(..)))
        .unwrap();
    match &l.kind {
        LinkKind::Path(p, _) => assert!(p.source.is_some(), "the curve comes back"),
        _ => unreachable!(),
    }
}
