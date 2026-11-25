// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The scale of bodies and meshes by factors in x, y and z.
use glam::DVec3;
use rolidium_doc::{Command, Document};
use rolidium_kernel::Solid;

#[test]
fn a_box_a_cylinder_and_a_mesh_are_scaled_and_one_undo_brings_them_back() {
    let mut d = Document::new();
    d.apply(Command::AddBox {
        name: "Box".into(),
        min: DVec3::ZERO,
        max: DVec3::new(40.0, 30.0, 20.0),
    })
    .unwrap();
    let id = d.bodies()[0].id;
    // a round of the box is a feature: it must survive the scale
    d.apply(Command::ScaleBodies {
        bodies: vec![id],
        centre: DVec3::ZERO,
        factors: DVec3::new(2.0, 1.0, 1.0),
    })
    .unwrap();
    let (lo, hi) = d.body(id).unwrap().solid.bounding_box().unwrap();
    assert!(
        (hi.x - lo.x - 80.0).abs() < 1e-6 && (hi.y - lo.y - 30.0).abs() < 1e-6,
        "{lo:?} {hi:?}"
    );
    assert!((d.body(id).unwrap().solid.volume().unwrap() - 48000.0).abs() < 1e-3);
    d.undo();
    let (lo, hi) = d.body(id).unwrap().solid.bounding_box().unwrap();
    assert!((hi.x - lo.x - 40.0).abs() < 1e-6);
}

#[test]
fn a_cylinder_scaled_in_one_direction_becomes_an_ellipse_and_stays_a_valid_solid() {
    let mut d = Document::new();
    let c = Solid::cylinder(15.0, 40.0, DVec3::ZERO, DVec3::Z).unwrap();
    let r = c.scaled(DVec3::ZERO, DVec3::new(1.5, 1.0, 1.0));
    match r {
        Ok(r) => {
            let (lo, hi) = r.solid.bounding_box().unwrap();
            assert!((hi.x - lo.x - 45.0).abs() < 1e-3, "{lo:?} {hi:?}");
            let (v1, v0) = (r.solid.volume().unwrap(), c.volume().unwrap());
            assert!((v1 - v0 * 1.5).abs() < v0 * 2e-2, "{v1} against {}", v0 * 1.5);
        }
        Err(e) => panic!("the scale of a cylinder failed: {e}"),
    }
    let _ = &mut d;
    // equal factors keep it a cylinder
    let u = c.scaled(DVec3::ZERO, DVec3::splat(2.0)).unwrap();
    assert!(u.solid.faces().any(|f| u
        .solid
        .cylinder_info(f)
        .ok()
        .flatten()
        .is_some_and(|i| (i.radius - 30.0).abs() < 1e-6)));
}

#[test]
fn a_cylinder_away_from_the_origin_is_scaled_about_a_corner_of_its_box() {
    let c = Solid::cylinder(15.0, 40.0, DVec3::new(100.0, 100.0, 0.0), DVec3::Z).unwrap();
    let r = c
        .scaled(DVec3::new(85.0, 85.0, 0.0), DVec3::new(1.5, 1.0, 1.0))
        .unwrap_or_else(|e| panic!("{e}"));
    let (lo, hi) = r.solid.bounding_box().unwrap();
    assert!((hi.x - lo.x - 45.0).abs() < 1e-2, "{lo:?} {hi:?}");
    assert!(
        (lo.x - 85.0).abs() < 1e-2,
        "the corner of the box about which it was scaled stays: {lo:?}"
    );
}

#[test]
fn a_cylinder_made_by_a_pull_of_a_circle_is_scaled_too() {
    use rolidium_doc::rolidium_sketch::{DVec2, Entity, PlaneDef};
    use rolidium_doc::{PullMode, SketchTarget};
    let mut d = Document::new();
    let c = d
        .apply(Command::AddEntity {
            target: SketchTarget::New {
                plane: PlaneDef::base(),
                body: None,
            },
            entity: Entity::Circle {
                center: DVec2::new(100.0, 100.0),
                radius: 7.5,
            },
        })
        .unwrap();
    let sk = c.created_sketch.unwrap();
    d.apply(Command::PullRegion {
        sketch: sk,
        region: 0,
        distance: 40.0,
        mode: PullMode::New,
    })
    .unwrap();
    // as the program has it after it opened the file
    let bytes = rolidium_doc::file::save_to_bytes(&d, None, "t").unwrap();
    let mut d = Document::from_loaded(rolidium_doc::file::load_from_bytes(&bytes).unwrap());
    let id = d.bodies()[0].id;
    for (centre, f) in [
        (DVec3::new(92.5, 92.5, 0.0), 3.0),
        (DVec3::new(100.0, 100.0, 20.0), 3.0),
        (DVec3::new(92.5, 92.5, 0.0), 2.0),
        (DVec3::new(92.5, 92.5, 0.0), 1.2),
    ] {
        let r = d.apply(Command::ScaleBodies {
            bodies: vec![id],
            centre,
            factors: DVec3::new(f, 1.0, 1.0),
        });
        println!(
            "centre {centre:?} factor {f}: {:?}",
            r.as_ref().err().map(|e| e.to_string())
        );
        if r.is_ok() {
            d.undo();
        }
    }
}

#[test]
fn an_odd_factor_does_not_make_a_cylinder_invalid() {
    let c = Solid::cylinder(7.5, 40.0, DVec3::new(100.0, 100.0, 0.0), DVec3::Z).unwrap();
    // the body of a program that shows it has been meshed
    let _ = c.mesh(0.05, 0.2).unwrap();
    for f in [3.008819491949516, 2.5, 1.7, 3.0, 0.5] {
        let r = c.scaled(DVec3::new(100.02, 100.0, 20.0), DVec3::new(f, 1.0, 1.0));
        println!("factor {f}: {:?}", r.as_ref().err().map(|e| e.to_string()));
    }
}
