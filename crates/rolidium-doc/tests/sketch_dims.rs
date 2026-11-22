// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Changing the size of a shape of a sketch (the numbers of the panel): circle, rectangle, line, arc; one step of Undo; the regions follow;
//! a body that was pulled from the sketch does not change.
use rolidium_doc::rolidium_sketch::{DVec2, Entity, PlaneDef};
use rolidium_doc::{entity_dims, entity_with_dims, Command, Document, PullMode, SketchTarget};
use std::f64::consts::PI;

fn sketch_with(d: &mut Document, e: Entity) -> (u64, u32) {
    let c = d
        .apply(Command::AddEntity {
            target: SketchTarget::New {
                plane: PlaneDef::base(),
                body: None,
            },
            entity: e,
        })
        .unwrap();
    let sk = c.created_sketch.unwrap();
    let ent = d.sketch(sk).unwrap().sketch.entities()[0].0;
    (sk, ent)
}

fn area(d: &Document, sk: u64) -> f64 {
    d.sketch(sk)
        .unwrap()
        .sketch
        .regions()
        .iter()
        .map(|r| r.area())
        .sum()
}

#[test]
fn the_diameter_of_a_circle_changes_with_one_step_of_undo_and_the_region_follows() {
    let mut d = Document::new();
    let (sk, ent) = sketch_with(
        &mut d,
        Entity::Circle {
            center: DVec2::new(3.0, 4.0),
            radius: 5.0,
        },
    );
    let old = *d.sketch(sk).unwrap().sketch.entity(ent).unwrap();
    assert_eq!(entity_dims(&old), vec![10.0, 3.0, 4.0]);
    let new = entity_with_dims(&old, &[Some(20.0), None, None]).unwrap();
    d.apply(Command::ReplaceEntity {
        sketch: sk,
        entity: ent,
        new,
    })
    .unwrap();
    assert!((area(&d, sk) - PI * 100.0).abs() < 1e-6);
    // the centre moves with the other numbers
    let old = *d.sketch(sk).unwrap().sketch.entity(ent).unwrap();
    let new = entity_with_dims(&old, &[Some(20.0), Some(-7.0), Some(2.0)]).unwrap();
    assert_eq!(
        new,
        Entity::Circle {
            center: DVec2::new(-7.0, 2.0),
            radius: 10.0
        }
    );
    d.apply(Command::ReplaceEntity {
        sketch: sk,
        entity: ent,
        new,
    })
    .unwrap();
    // one Undo goes back one change
    assert!(d.undo());
    assert!(
        matches!(d.sketch(sk).unwrap().sketch.entity(ent), Some(Entity::Circle { center, radius }) if *center == DVec2::new(3.0, 4.0) && (*radius - 10.0).abs() < 1e-9)
    );
    assert!(d.undo());
    assert!((area(&d, sk) - PI * 25.0).abs() < 1e-6);
    // nonsense is refused
    assert!(entity_with_dims(&old, &[Some(0.0)]).is_none());
    assert!(entity_with_dims(&old, &[Some(f64::NAN)]).is_none());
}

#[test]
fn the_size_of_a_rectangle_and_the_length_and_angle_of_a_line_change() {
    let mut d = Document::new();
    let (sk, ent) = sketch_with(
        &mut d,
        Entity::Rect {
            a: DVec2::new(0.0, 0.0),
            b: DVec2::new(10.0, 5.0),
        },
    );
    let old = *d.sketch(sk).unwrap().sketch.entity(ent).unwrap();
    assert_eq!(entity_dims(&old), vec![10.0, 5.0]);
    let new = entity_with_dims(&old, &[Some(30.0), Some(4.0)]).unwrap();
    d.apply(Command::ReplaceEntity {
        sketch: sk,
        entity: ent,
        new,
    })
    .unwrap();
    assert!((area(&d, sk) - 120.0).abs() < 1e-9);
    assert!(d.undo());
    assert!((area(&d, sk) - 50.0).abs() < 1e-9);
    // a rectangle drawn towards the negative side keeps its side
    let r = Entity::Rect {
        a: DVec2::new(0.0, 0.0),
        b: DVec2::new(-10.0, -5.0),
    };
    assert_eq!(
        entity_with_dims(&r, &[Some(6.0), Some(2.0)]).unwrap(),
        Entity::Rect {
            a: DVec2::ZERO,
            b: DVec2::new(-6.0, -2.0)
        }
    );
    // a line: the length and the angle
    let l = Entity::Line {
        a: DVec2::new(1.0, 1.0),
        b: DVec2::new(4.0, 5.0),
    };
    let v = entity_dims(&l);
    assert!((v[0] - 5.0).abs() < 1e-9 && (v[1] - (4.0f64).atan2(3.0).to_degrees()).abs() < 1e-9);
    let n = entity_with_dims(&l, &[Some(10.0), Some(90.0)]).unwrap();
    match n {
        Entity::Line { a, b } => {
            assert_eq!(a, DVec2::new(1.0, 1.0));
            assert!((b - DVec2::new(1.0, 11.0)).length() < 1e-9, "{b:?}");
        }
        _ => unreachable!(),
    }
    let n = entity_with_dims(&l, &[Some(10.0), None]).unwrap();
    match n {
        Entity::Line { a, b } => assert!(((b - a).length() - 10.0).abs() < 1e-9),
        _ => unreachable!(),
    }
}

#[test]
fn the_radius_and_the_angle_of_an_arc_change() {
    // a quarter circle of radius 10 from angle 0
    let arc = Entity::Arc {
        center: DVec2::ZERO,
        radius: 10.0,
        start: 0.0,
        sweep: PI / 2.0,
    };
    let v = entity_dims(&arc);
    assert!((v[0] - 10.0).abs() < 1e-9 && (v[1] - 90.0).abs() < 1e-9);
    let n = entity_with_dims(&arc, &[None, Some(180.0)]).unwrap();
    assert!(
        matches!(n, Entity::Arc { center, radius, start, sweep } if center == DVec2::ZERO && radius == 10.0 && start == 0.0 && (sweep - PI).abs() < 1e-9)
    );
    // only the radius: the ends stay where they are (the chord is the same)
    let n = entity_with_dims(&arc, &[Some(20.0), None]).unwrap();
    match n {
        Entity::Arc { radius, .. } => assert!((radius - 20.0).abs() < 1e-9),
        _ => unreachable!(),
    }
    assert!(entity_with_dims(&arc, &[None, Some(0.0)]).is_none());
    assert!(entity_with_dims(&arc, &[None, Some(400.0)]).is_none());
}

#[test]
fn a_body_pulled_from_the_sketch_does_not_change_when_the_sketch_does() {
    let mut d = Document::new();
    let (sk, first) = sketch_with(
        &mut d,
        Entity::Circle {
            center: DVec2::ZERO,
            radius: 5.0,
        },
    );
    d.apply(Command::AddEntity {
        target: SketchTarget::Existing(sk),
        entity: Entity::Circle {
            center: DVec2::new(50.0, 0.0),
            radius: 5.0,
        },
    })
    .unwrap();
    // pull the first circle: it makes a body (and leaves the sketch with the other circle)
    let region = d
        .sketch(sk)
        .unwrap()
        .sketch
        .regions()
        .iter()
        .position(|r| r.contains(DVec2::ZERO))
        .unwrap_or(0);
    d.apply(Command::PullRegion {
        sketch: sk,
        region,
        distance: 10.0,
        mode: PullMode::Auto,
    })
    .unwrap();
    let v0 = d.bodies()[0].solid.volume().unwrap();
    assert!((v0 - PI * 25.0 * 10.0).abs() < 1e-6, "{v0}");
    let other = d
        .sketch(sk)
        .expect("the sketch keeps the other circle")
        .sketch
        .entities()[0]
        .0;
    let _ = first;
    let old = *d.sketch(sk).unwrap().sketch.entity(other).unwrap();
    let new = entity_with_dims(&old, &[Some(30.0), None, None]).unwrap();
    d.apply(Command::ReplaceEntity {
        sketch: sk,
        entity: other,
        new,
    })
    .unwrap();
    assert!((area(&d, sk) - PI * 225.0).abs() < 1e-6, "the sketch follows");
    assert!(
        (d.bodies()[0].solid.volume().unwrap() - v0).abs() < 1e-9,
        "the body does not (direct modelling)"
    );
}
