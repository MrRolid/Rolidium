// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The end faces of a loft are the sketches of its profiles: another diameter, a pull and a move of an end change the sketch, the link stays (no question about
//! removing it), one Undo takes the change back, and the file keeps it.
use rolidium_doc::rolidium_sketch::{DVec2, Entity, PlaneDef};
use rolidium_doc::*;
use rolidium_kernel::{DVec3, FaceId, FaceKind};
use std::f64::consts::PI;

fn at_z(z: f64) -> PlaneDef {
    PlaneDef {
        origin: DVec3::new(0.0, 0.0, z),
        normal: DVec3::Z,
        x_dir: DVec3::X,
    }
}

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

/// A frustum of two circles (20 and 10 mm in radius, 30 mm apart) as a loft link: (document, body, bottom sketch and circle, top sketch and circle).
fn frustum_doc() -> (Document, BodyId, (SketchId, u32), (SketchId, u32)) {
    let mut d = Document::new();
    let a = circle(&mut d, at_z(0.0), 20.0);
    let b = circle(&mut d, at_z(30.0), 10.0);
    let c = d
        .apply(Command::AddLink {
            kind: LinkKind::Loft {
                profiles: vec![
                    LoftRef::Region { sketch: a.0, region: 0 },
                    LoftRef::Region { sketch: b.0, region: 0 },
                ],
                ruled: true,
                mode: PatternMode::New,
            },
            sources: Vec::new(),
            name: String::new(),
        })
        .unwrap();
    (d, c.touched_body.unwrap(), a, b)
}

fn frustum(h: f64, r1: f64, r2: f64) -> f64 {
    PI * h / 3.0 * (r1 * r1 + r1 * r2 + r2 * r2)
}

fn volume(d: &Document, b: BodyId) -> f64 {
    d.body(b).unwrap().solid.volume().unwrap()
}

fn face_of_kind(d: &Document, b: BodyId, kind: FaceKind, z: Option<f64>) -> FaceId {
    let s = &d.body(b).unwrap().solid;
    s.faces()
        .find(|f| {
            let i = s.face_info(*f).unwrap();
            i.kind == kind && z.is_none_or(|z| (i.origin.z - z).abs() < 1e-6)
        })
        .unwrap()
}

#[test]
fn a_loft_of_two_coaxial_circles_has_a_conical_side_and_its_ends_know_their_sketches() {
    let (d, body, a, b) = frustum_doc();
    let s = &d.body(body).unwrap().solid;
    assert_eq!(s.face_count(), 3);
    face_of_kind(&d, body, FaceKind::Cone, None);
    let top = face_of_kind(&d, body, FaceKind::Plane, Some(30.0));
    let ef = d.end_face(body, top).expect("the top is the end of the loft");
    assert_eq!((ef.sketch, ef.kind), (b.0, EndKind::Loft));
    let (entity, _, r) = d.end_circle(&ef).expect("a circle");
    assert_eq!((entity, r), (b.1, 10.0));
    let bottom = face_of_kind(&d, body, FaceKind::Plane, Some(0.0));
    assert_eq!(d.end_face(body, bottom).unwrap().sketch, a.0);
    // the side is not an end
    assert!(d.end_face(body, face_of_kind(&d, body, FaceKind::Cone, None)).is_none());
}

#[test]
fn the_diameter_of_an_end_is_the_circle_of_the_sketch_and_one_undo_takes_it_back() {
    let (mut d, body, _, b) = frustum_doc();
    let v0 = volume(&d, body);
    d.apply(Command::ReplaceEntity {
        sketch: b.0,
        entity: b.1,
        new: Entity::Circle {
            center: DVec2::ZERO,
            radius: 16.0,
        },
    })
    .unwrap();
    assert_eq!(d.state().links.len(), 1, "the link stays");
    assert!((volume(&d, body) - frustum(30.0, 20.0, 16.0)).abs() < 1e-3);
    assert!(d.undo());
    assert!((volume(&d, body) - v0).abs() < 1e-6);
}

#[test]
fn pulling_an_end_moves_its_sketch_along_the_normal_without_asking_about_the_link() {
    let (mut d, body, a, b) = frustum_doc();
    let top = face_of_kind(&d, body, FaceKind::Plane, Some(30.0));
    d.apply(Command::PullFace {
        body,
        face: top,
        distance: 5.0,
        mode: PullMode::Auto,
    })
    .expect("no question");
    assert_eq!(d.state().links.len(), 1);
    assert!((d.sketch(b.0).unwrap().plane.origin.z - 35.0).abs() < 1e-9);
    assert!((volume(&d, body) - frustum(35.0, 20.0, 10.0)).abs() < 1e-3);
    // the bottom end is pulled outwards (downwards): its sketch goes down
    let bottom = face_of_kind(&d, body, FaceKind::Plane, Some(0.0));
    d.apply(Command::PullFace {
        body,
        face: bottom,
        distance: 4.0,
        mode: PullMode::Auto,
    })
    .unwrap();
    assert!((d.sketch(a.0).unwrap().plane.origin.z + 4.0).abs() < 1e-9);
    assert!((volume(&d, body) - frustum(39.0, 20.0, 10.0)).abs() < 1e-3);
    // one Undo for each
    assert!(d.undo());
    assert!((volume(&d, body) - frustum(35.0, 20.0, 10.0)).abs() < 1e-3);
    assert!(d.undo());
    assert!((volume(&d, body) - frustum(30.0, 20.0, 10.0)).abs() < 1e-3);
}

#[test]
fn moving_an_end_sideways_makes_a_slanting_frustum_and_the_file_keeps_it() {
    let (mut d, body, _, b) = frustum_doc();
    d.apply(Command::MoveSketch {
        sketch: b.0,
        translation: DVec3::new(12.0, 0.0, 0.0),
    })
    .unwrap();
    assert_eq!(d.state().links.len(), 1);
    // the same volume as the straight frustum (the slanting one is a cone cut by parallel planes: Cavalieri)
    assert!((volume(&d, body) - frustum(30.0, 20.0, 10.0)).abs() < 1e-2);
    let (lo, hi) = d.body(body).unwrap().solid.bounding_box().unwrap();
    assert!(hi.x > 21.0 && lo.x < -19.0 && (hi.z - 30.0).abs() < 1e-5);
    assert_eq!(
        d.body(body).unwrap().solid.faces().filter(|f| d.body(body).unwrap().solid.face_info(*f).unwrap().kind == FaceKind::Cone).count(),
        0,
        "a slanting frustum is not a cone of revolution"
    );
    // saved and opened again: the sketch and the body are as they were, and the end still follows
    let bytes = file::save_to_bytes(&d, None, "t").unwrap();
    let mut e = Document::from_loaded(file::load_from_bytes(&bytes).unwrap());
    assert_eq!(e.state().links.len(), 1);
    let top = face_of_kind(&e, body, FaceKind::Plane, Some(30.0));
    e.apply(Command::PullFace {
        body,
        face: top,
        distance: 3.0,
        mode: PullMode::Auto,
    })
    .unwrap();
    assert!((volume(&e, body) - frustum(33.0, 20.0, 10.0)).abs() < 1e-2);
    assert!(d.undo());
    assert!((volume(&d, body) - frustum(30.0, 20.0, 10.0)).abs() < 1e-3);
}

#[test]
fn what_cannot_be_turned_into_a_source_still_asks() {
    let (mut d, body, _, _) = frustum_doc();
    let side = face_of_kind(&d, body, FaceKind::Cone, None);
    // pulling the side, rounding an edge: not a change of a sketch
    let edge = d.body(body).unwrap().solid.edges().next().unwrap().0;
    assert!(matches!(
        d.apply(Command::FilletEdges {
            body,
            edges: vec![edge],
            size: 1.0,
            chamfer: false
        }),
        Err(DocError::Derived { .. })
    ));
    assert!(d.apply(Command::SetCylinderDiameter { body, face: side, diameter: 5.0 }).is_err());
}

#[test]
fn after_the_link_is_removed_the_body_is_an_ordinary_cone_whose_ends_can_be_edited() {
    let (mut d, body, _, _) = frustum_doc();
    let link = d.state().links[0].id;
    d.apply(Command::Unlink { link }).unwrap();
    let s = &d.body(body).unwrap().solid;
    assert!(s.frustum_info().is_some());
    let top = face_of_kind(&d, body, FaceKind::Plane, Some(30.0));
    d.apply(Command::SetCapDiameter { body, face: top, diameter: 32.0, keep_angle: false }).unwrap();
    assert!((volume(&d, body) - frustum(30.0, 20.0, 16.0)).abs() < 1e-3);
    // the top end pulled up: the cone is extended with its angle
    let top = face_of_kind(&d, body, FaceKind::Plane, Some(30.0));
    d.apply(Command::PullFace { body, face: top, distance: 10.0, mode: PullMode::Auto }).unwrap();
    let f = d.body(body).unwrap().solid.frustum_info().unwrap();
    assert!((f.height - 40.0).abs() < 1e-6 && (f.r_top - (16.0 - 10.0 * 4.0 / 30.0)).abs() < 1e-6);
    // one Undo each
    assert!(d.undo() && d.undo());
    assert!((volume(&d, body) - frustum(30.0, 20.0, 10.0)).abs() < 1e-3);
}
