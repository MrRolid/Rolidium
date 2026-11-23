// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A loft and a sweep as links: they remember their profiles and their path, follow a change of the source (the diameter of a circle of a
//! sketch, a move of a sketch, another pitch of a helix), ask before a hand edit of the result, and are saved and opened with the file.
use rolidium_doc::rolidium_sketch::{DVec2, Entity, PlaneDef};
use rolidium_doc::*;
use rolidium_kernel::{DVec3, HelixDef, Path, PathSource};
use std::f64::consts::PI;

fn circle(d: &mut Document, plane: PlaneDef, r: f64) -> (SketchId, u32) {
    let c = d
        .apply(Command::AddEntity {
            target: SketchTarget::New { plane, body: None },
            entity: Entity::Circle {
                center: DVec2::ZERO,
                radius: r,
            },
        })
        .unwrap();
    let sk = c.created_sketch.unwrap();
    (sk, d.sketch(sk).unwrap().sketch.entities()[0].0)
}

fn at_z(z: f64) -> PlaneDef {
    PlaneDef {
        origin: DVec3::new(0.0, 0.0, z),
        normal: DVec3::Z,
        x_dir: DVec3::X,
    }
}

fn set_radius(d: &mut Document, sk: SketchId, e: u32, r: f64) {
    d.apply(Command::ReplaceEntity {
        sketch: sk,
        entity: e,
        new: Entity::Circle {
            center: DVec2::ZERO,
            radius: r,
        },
    })
    .unwrap();
}

/// A flat face of the body (an end of a loft or of a sweep).
fn flat_face(d: &Document, b: BodyId) -> rolidium_kernel::FaceId {
    let s = &d.body(b).unwrap().solid;
    s.faces()
        .find(|f| s.face_info(*f).unwrap().kind == rolidium_kernel::FaceKind::Plane)
        .unwrap()
}

fn frustum(h: f64, r1: f64, r2: f64) -> f64 {
    PI * h / 3.0 * (r1 * r1 + r1 * r2 + r2 * r2)
}

fn volume(d: &Document, b: BodyId) -> f64 {
    d.body(b).unwrap().solid.volume().unwrap()
}

/// A ruled loft of two circles as a link: (document, link, result body, bottom sketch, its circle, top sketch).
fn loft_doc() -> (Document, LinkId, BodyId, SketchId, u32, SketchId) {
    let mut d = Document::new();
    let (a, ea) = circle(&mut d, at_z(0.0), 10.0);
    let (b, _) = circle(&mut d, at_z(30.0), 5.0);
    let c = d
        .apply(Command::AddLink {
            kind: LinkKind::Loft {
                profiles: vec![
                    LoftRef::Region { sketch: a, region: 0 },
                    LoftRef::Region { sketch: b, region: 0 },
                ],
                ruled: true,
                mode: PatternMode::New,
            },
            sources: Vec::new(),
            name: String::new(),
        })
        .unwrap();
    let body = c.touched_body.unwrap();
    let link = d.state().links[0].id;
    (d, link, body, a, ea, b)
}

#[test]
fn a_loft_follows_the_diameter_of_a_circle_and_the_move_of_a_sketch() {
    let (mut d, link, body, a, ea, b) = loft_doc();
    assert!((volume(&d, body) - frustum(30.0, 10.0, 5.0)).abs() < 1.0);
    assert_eq!(d.state().links[0].name, "Loft 1");
    // the profiles are kept (hidden) so that they can be edited
    assert!(!d.sketch(a).unwrap().visible && !d.sketch(b).unwrap().visible);
    // the diameter of the bottom circle changes: the body is made again (the same body, not a new one)
    set_radius(&mut d, a, ea, 20.0);
    assert_eq!(d.bodies().len(), 1);
    assert_eq!(d.state().links[0].results, vec![body]);
    assert!((volume(&d, body) - frustum(30.0, 20.0, 5.0)).abs() < 1.0);
    assert!(d.state().links[0].error.is_none());
    // one Undo takes back the change of the circle and the body with it
    assert!(d.undo());
    assert!((volume(&d, body) - frustum(30.0, 10.0, 5.0)).abs() < 1.0);
    // the top sketch is moved up
    d.apply(Command::MoveSketch {
        sketch: b,
        translation: DVec3::new(0.0, 0.0, 10.0),
    })
    .unwrap();
    assert!((volume(&d, body) - frustum(40.0, 10.0, 5.0)).abs() < 1.0);
    assert_eq!(d.state().links[0].id, link);
}

#[test]
fn a_hand_edit_of_a_loft_asks_and_removing_the_link_frees_the_body() {
    let (mut d, link, body, a, ea, _) = loft_doc();
    // a pull of an end face is a change of its sketch (see end_faces.rs); a round of an edge cannot be turned into one: the link is asked about
    let edge = d.body(body).unwrap().solid.edges().next().unwrap().0;
    let r = d.apply(Command::FilletEdges {
        body,
        edges: vec![edge],
        size: 1.0,
        chamfer: false,
    });
    assert!(
        matches!(r, Err(DocError::Derived { link: l, body: b, .. }) if l == link && b == body),
        "{:?}",
        r.map(|_| ())
    );
    d.apply(Command::Unlink { link }).unwrap();
    assert!(d.state().links.is_empty());
    // the body is an ordinary body now, and the sketches no longer change it
    set_radius(&mut d, a, ea, 20.0);
    assert!((volume(&d, body) - frustum(30.0, 10.0, 5.0)).abs() < 1.0);
}

#[test]
fn deleting_a_loft_link_removes_the_body_and_shows_the_sketches_again() {
    let (mut d, link, _, a, _, b) = loft_doc();
    d.apply(Command::DeleteLink { link }).unwrap();
    assert!(d.bodies().is_empty());
    assert!(d.sketch(a).unwrap().visible && d.sketch(b).unwrap().visible);
}

#[test]
fn a_loft_link_is_saved_and_opened() {
    let (d, _, body, a, ea, _) = loft_doc();
    let bytes = file::save_to_bytes(&d, None, "test").unwrap();
    let mut d2 = Document::from_loaded(file::load_from_bytes(&bytes).unwrap());
    assert_eq!(d2.state().links.len(), 1);
    assert!(matches!(
        d2.state().links[0].kind,
        LinkKind::Loft { ruled: true, .. }
    ));
    assert_eq!(d2.state().links[0].results, vec![body]);
    // it still follows the sketch after the file was opened
    set_radius(&mut d2, a, ea, 20.0);
    assert!((volume(&d2, body) - frustum(30.0, 20.0, 5.0)).abs() < 1.0);
}

fn helix(pitch: f64) -> CurveDef {
    CurveDef::Helix(HelixDef {
        origin: DVec3::ZERO,
        axis: DVec3::Z,
        x_dir: DVec3::X,
        radius: 10.0,
        pitch,
        turns: 5.0,
        right: true,
        start_deg: 0.0,
        cone_deg: 0.0,
    })
}

/// A circle of radius 1 across the start of the helix of 5 turns: a spring as a sweep link.
fn spring_doc() -> (Document, CurveId, BodyId, SketchId, u32) {
    let mut d = Document::new();
    let curve = d
        .apply(Command::AddCurve { def: helix(4.0) })
        .unwrap()
        .created_curve
        .unwrap();
    let path = Path::build(PathSource::Helix(match helix(4.0) {
        CurveDef::Helix(h) => h,
        _ => unreachable!(),
    }))
    .unwrap();
    let x = path.start_tangent.cross(DVec3::Z).normalize();
    let (sk, e) = circle(
        &mut d,
        PlaneDef {
            origin: path.start,
            normal: path.start_tangent,
            x_dir: x,
        },
        1.0,
    );
    let c = d
        .apply(Command::AddLink {
            kind: LinkKind::Sweep {
                profile: LoftRef::Region {
                    sketch: sk,
                    region: 0,
                },
                path: SweepPath::Curve(curve),
                rounded: false,
                mode: PatternMode::New,
            },
            sources: Vec::new(),
            name: String::new(),
        })
        .unwrap();
    (d, curve, c.touched_body.unwrap(), sk, e)
}

fn helix_length(pitch: f64) -> f64 {
    5.0 * ((2.0 * PI * 10.0).powi(2) + pitch * pitch).sqrt()
}

#[test]
fn a_sweep_along_a_helix_follows_the_pitch_and_the_circle() {
    let (mut d, curve, body, sk, e) = spring_doc();
    let v = |p: f64, r: f64| PI * r * r * helix_length(p);
    assert!((volume(&d, body) - v(4.0, 1.0)).abs() < 0.01 * v(4.0, 1.0));
    assert_eq!(d.state().links[0].name, "Sweep 1");
    // another pitch of the helix
    d.apply(Command::EditCurve {
        curve,
        def: helix(7.0),
    })
    .unwrap();
    assert_eq!(d.bodies().len(), 1);
    assert!(
        d.state().links[0].error.is_none(),
        "{:?}",
        d.state().links[0].error
    );
    assert!((volume(&d, body) - v(7.0, 1.0)).abs() < 0.01 * v(7.0, 1.0));
    // another diameter of the wire
    set_radius(&mut d, sk, e, 1.5);
    assert!((volume(&d, body) - v(7.0, 1.5)).abs() < 0.01 * v(7.0, 1.5));
    // a hand edit of the spring asks; the link can be removed
    let f = flat_face(&d, body);
    assert!(matches!(
        d.apply(Command::PullFace {
            body,
            face: f,
            distance: 0.5,
            mode: PullMode::Auto
        }),
        Err(DocError::Derived { .. })
    ));
}

#[test]
fn a_sweep_link_is_saved_and_opened() {
    let (d, curve, body, _, _) = spring_doc();
    let bytes = file::save_to_bytes(&d, None, "test").unwrap();
    let mut d2 = Document::from_loaded(file::load_from_bytes(&bytes).unwrap());
    assert_eq!(d2.state().links.len(), 1);
    assert!(matches!(
        d2.state().links[0].kind,
        LinkKind::Sweep { path: SweepPath::Curve(c), .. } if c == curve
    ));
    d2.apply(Command::EditCurve {
        curve,
        def: helix(6.0),
    })
    .unwrap();
    let v = PI * helix_length(6.0);
    assert!((volume(&d2, body) - v).abs() < 0.01 * v);
}

#[test]
fn a_change_that_cannot_be_calculated_keeps_the_last_body_and_says_why() {
    let (mut d, curve, body, _, _) = spring_doc();
    let before = volume(&d, body);
    // a helix with no turns cannot be swept along; the command is refused, so the link is untouched
    let bad = CurveDef::Helix(HelixDef {
        turns: 0.0,
        ..match helix(4.0) {
            CurveDef::Helix(h) => h,
            _ => unreachable!(),
        }
    });
    let r = d.apply(Command::EditCurve { curve, def: bad });
    if r.is_ok() {
        assert!(d.state().links[0].error.is_some());
    }
    assert!((volume(&d, body) - before).abs() < 1e-6);
}

#[test]
fn a_loft_that_adds_to_a_body_follows_its_profiles_and_delete_gives_the_body_back() {
    let mut d = Document::new();
    d.apply(Command::AddBox {
        name: "Box".into(),
        min: DVec3::ZERO,
        max: DVec3::new(40.0, 40.0, 20.0),
    })
    .unwrap();
    let target = d.bodies()[0].id;
    let plane = |z| PlaneDef {
        origin: DVec3::new(20.0, 20.0, z),
        normal: DVec3::Z,
        x_dir: DVec3::X,
    };
    let (a, ea) = circle(&mut d, plane(20.0), 8.0);
    let (b, _) = circle(&mut d, plane(40.0), 4.0);
    let box_volume = 40.0 * 40.0 * 20.0;
    let link = d
        .apply(Command::AddLink {
            kind: LinkKind::Loft {
                profiles: vec![
                    LoftRef::Region { sketch: a, region: 0 },
                    LoftRef::Region { sketch: b, region: 0 },
                ],
                ruled: true,
                mode: PatternMode::Add(target),
            },
            sources: Vec::new(),
            name: String::new(),
        })
        .unwrap();
    let _ = link;
    assert_eq!(d.bodies().len(), 1);
    assert!((volume(&d, target) - box_volume - frustum(20.0, 8.0, 4.0)).abs() < 1.0);
    // the circle gets larger
    d.apply(Command::ReplaceEntity {
        sketch: a,
        entity: ea,
        new: Entity::Circle {
            center: DVec2::ZERO,
            radius: 12.0,
        },
    })
    .unwrap();
    assert!((volume(&d, target) - box_volume - frustum(20.0, 12.0, 4.0)).abs() < 1.0);
    let id = d.state().links[0].id;
    d.apply(Command::DeleteLink { link: id }).unwrap();
    assert!((volume(&d, target) - box_volume).abs() < 1e-6);
}
