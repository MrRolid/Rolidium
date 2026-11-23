// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A helix made on a cylindrical or conical face keeps the height of that face: a change of the turns changes the pitch, nothing leaves the face.
//! The limit is saved with the curve.
use rolidium_doc::*;
use rolidium_kernel::{DVec3, HelixDef};

fn helix(turns: f64, pitch: f64) -> CurveDef {
    CurveDef::Helix(HelixDef {
        origin: DVec3::ZERO,
        axis: DVec3::Z,
        x_dir: DVec3::X,
        radius: 10.0,
        pitch,
        turns,
        right: true,
        start_deg: 0.0,
        cone_deg: 0.0,
    })
}

#[test]
fn a_helix_of_a_face_may_not_be_higher_than_the_face() {
    let mut d = Document::new();
    let c = d
        .apply(Command::AddCurveOnFace {
            def: helix(10.0, 3.0),
            limit_height: 30.0,
        })
        .unwrap()
        .created_curve
        .unwrap();
    assert_eq!(d.curve(c).unwrap().limit_height, Some(30.0));
    // more turns with the same height (the pitch follows): allowed
    d.apply(Command::EditCurve {
        curve: c,
        def: helix(60.0, 0.5),
    })
    .unwrap();
    // the turns changed alone (the pitch stays): the helix would leave the face
    let e = d
        .apply(Command::EditCurve {
            curve: c,
            def: helix(300.0, 0.5),
        })
        .unwrap_err()
        .to_string();
    assert!(e.contains("leave the face") && e.contains("30"), "{e}");
    // nothing changed by the refused edit
    let CurveDef::Helix(h) = &d.curve(c).unwrap().def else {
        panic!()
    };
    assert_eq!(h.turns, 60.0);
    // a helix that is higher than its face cannot be made at all
    assert!(d
        .apply(Command::AddCurveOnFace {
            def: helix(20.0, 3.0),
            limit_height: 30.0
        })
        .is_err());
}

#[test]
fn a_free_helix_has_no_height_limit_but_the_general_one() {
    let mut d = Document::new();
    let c = d
        .apply(Command::AddCurve {
            def: helix(10.0, 3.0),
        })
        .unwrap()
        .created_curve
        .unwrap();
    assert_eq!(d.curve(c).unwrap().limit_height, None);
    d.apply(Command::EditCurve {
        curve: c,
        def: helix(100.0, 3.0),
    })
    .unwrap();
    assert!(d
        .apply(Command::EditCurve {
            curve: c,
            def: helix(5000.0, 3.0)
        })
        .is_err());
}

#[test]
fn the_limit_of_a_helix_of_a_face_is_saved_and_opened() {
    let mut d = Document::new();
    d.apply(Command::AddCurveOnFace {
        def: helix(10.0, 3.0),
        limit_height: 30.0,
    })
    .unwrap();
    d.apply(Command::AddCurve { def: helix(4.0, 1.0) }).unwrap();
    let bytes = file::save_to_bytes(&d, None, "test").unwrap();
    let d2 = Document::from_loaded(file::load_from_bytes(&bytes).unwrap());
    let l: Vec<Option<f64>> = d2.curves().iter().map(|c| c.limit_height).collect();
    assert_eq!(l, vec![Some(30.0), None]);
}
