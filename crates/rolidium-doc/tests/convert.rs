// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A mesh made a solid: the flat faces are merged, the solid is valid and has the volume of the mesh, one Undo brings the mesh back, and the solid is edited like a drawn one.
use rolidium_doc::{Command, Document, NewMesh};
use rolidium_kernel::FaceKind;
use rolidium_mesh::gen;
use std::sync::Arc;

fn doc_with(mesh: rolidium_mesh::TriMesh) -> (Document, u64) {
    let mut d = Document::new();
    d.apply(Command::AddMeshes {
        meshes: vec![NewMesh {
            name: "m".into(),
            mesh: Arc::new(mesh),
            verified: true,
        }],
    })
    .unwrap();
    let id = d.meshes()[0].id;
    (d, id)
}

#[test]
fn a_box_mesh_becomes_a_box_of_six_faces_and_undo_brings_the_mesh_back() {
    let (mut d, id) = doc_with(gen::box_mesh(40.0, 30.0, 20.0, 3));
    d.apply(Command::ConvertMesh {
        body: id,
        tolerance: 0.02,
        round: true,
        keep: false,
    })
    .unwrap();
    assert!(d.meshes().is_empty() && d.bodies().len() == 1);
    let b = &d.bodies()[0];
    assert_eq!(b.solid.face_count(), 6, "the flat faces are merged");
    assert!((b.solid.volume().unwrap() - 24000.0).abs() < 1.0);
    let rep = rolidium_doc::meshes::conversion_report(b.id).unwrap();
    assert_eq!(rep.kinds[0], 6);
    assert!(rep.max_deviation < 0.02 && rep.single == 0);
    d.undo();
    assert!(d.bodies().is_empty() && d.meshes().len() == 1, "one step of Undo");
}

#[test]
fn a_plate_with_a_hole_becomes_a_valid_solid_of_the_same_volume_that_can_be_pulled() {
    let m = gen::plate_with_hole(60.0, 40.0, 10.0, 2.5, 32);
    let v = m.signed_volume();
    let (mut d, id) = doc_with(m);
    d.apply(Command::ConvertMesh {
        body: id,
        tolerance: 0.02,
        round: true,
        keep: true,
    })
    .unwrap();
    assert!(d.meshes()[0].visible == false, "the mesh is kept, hidden");
    let bid = d.bodies()[0].id;
    let s = &d.body(bid).unwrap().solid;
    assert!(s.is_valid());
    assert!((s.volume().unwrap() - v).abs() < v * 1e-3, "{} against {v}", s.volume().unwrap());
    // the top is one flat face: it can be pulled like a drawn one
    let top = s.faces().find(|f| {
        let i = s.face_info(*f).unwrap();
        i.kind == FaceKind::Plane && i.normal.z > 0.99
    });
    let top = top.expect("a top face");
    d.apply(Command::PullFace {
        body: bid,
        face: top,
        distance: 5.0,
        mode: rolidium_doc::PullMode::Auto,
    })
    .unwrap();
    let after = d.body(bid).unwrap().solid.volume().unwrap();
    assert!(after > v + 5.0 * 1500.0, "{after}");
}

#[test]
fn a_ball_of_triangles_becomes_a_valid_solid_of_single_triangles() {
    let m = gen::sphere(20.0, 800);
    let v = m.signed_volume();
    let (mut d, id) = doc_with(m);
    let r = d.apply(Command::ConvertMesh {
        body: id,
        tolerance: 0.02,
        round: false,
        keep: false,
    });
    assert!(r.is_ok(), "{:?}", r.err().map(|e| e.to_string()));
    let b = &d.bodies()[0];
    assert!((b.solid.volume().unwrap() - v).abs() < v * 1e-3);
    let rep = rolidium_doc::meshes::conversion_report(b.id).unwrap();
    assert!(rep.kinds[0] > 20, "a face for every flat piece: {:?}", rep.kinds);
}
