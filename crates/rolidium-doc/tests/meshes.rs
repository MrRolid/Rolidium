// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Meshes (STL) as bodies of the document: add, edit, undo, groups, save and open.

use rolidium_doc::file::{load_from_bytes, save_to_bytes, FORMAT_WITH_MESHES};
use rolidium_doc::{Command, Document, ItemId, NewMesh, RepairKind};
use rolidium_kernel::DVec3;
use rolidium_mesh::{gen, regions};
use std::sync::Arc;

fn doc_with(mesh: rolidium_mesh::TriMesh, name: &str) -> (Document, u64) {
    let mut d = Document::new();
    let c = d
        .apply(Command::AddMeshes {
            meshes: vec![NewMesh {
                name: name.into(),
                mesh: Arc::new(mesh),
                verified: true,
            }],
        })
        .unwrap();
    (d, c.touched_body.unwrap())
}

#[test]
fn a_mesh_is_added_renamed_hidden_grouped_and_deleted_with_undo() {
    let (mut d, id) = doc_with(gen::box_mesh(20.0, 20.0, 20.0, 1), "cube");
    assert_eq!(d.meshes().len(), 1);
    d.apply(Command::Rename {
        target: ItemId::Body(id),
        name: "Krychle".into(),
    })
    .unwrap();
    assert_eq!(d.mesh(id).unwrap().name, "Krychle");
    d.apply(Command::SetVisible {
        target: ItemId::Body(id),
        visible: false,
    })
    .unwrap();
    assert!(!d.mesh(id).unwrap().visible);
    d.apply(Command::CreateGroup {
        bodies: vec![id],
        name: "G".into(),
        parent: None,
    })
    .unwrap();
    let g = d.state().groups[0].id;
    d.apply(Command::SetVisible {
        target: ItemId::Group(g),
        visible: true,
    })
    .unwrap();
    assert!(d.mesh(id).unwrap().visible, "the group shows its mesh");
    d.apply(Command::DeleteBody { body: id }).unwrap();
    assert!(d.meshes().is_empty());
    assert!(d.state().groups[0].bodies.is_empty());
    assert!(d.undo());
    assert_eq!(d.meshes().len(), 1);
    assert_eq!(d.state().groups[0].bodies, vec![id]);
}

#[test]
fn pull_a_wall_and_change_a_hole_then_undo_without_copying() {
    let (mut d, id) = doc_with(gen::plate_with_hole(40.0, 30.0, 10.0, 2.5, 48), "plate");
    let before = d.mesh(id).unwrap().mesh.clone();
    let reg = regions::find(&before, 3.0, 4);
    let top = reg
        .planes
        .iter()
        .find(|p| p.normal[2] > 0.99)
        .expect("the top wall")
        .clone();
    let h0 = before.bounding_box().unwrap().1[2];
    d.apply(Command::MeshPull {
        body: id,
        wall: Arc::new(top),
        distance: 5.0,
    })
    .unwrap();
    let after = d.mesh(id).unwrap().mesh.clone();
    assert!((after.bounding_box().unwrap().1[2] - h0 - 5.0).abs() < 1e-4);
    // the snapshot before the edit is the same allocation: Undo is a swap
    assert!(d.undo());
    assert!(Arc::ptr_eq(&d.mesh(id).unwrap().mesh, &before));
    assert!(d.redo());
    // the hole
    let reg = regions::find(&after, 3.0, 4);
    let hole = reg
        .cylinders
        .iter()
        .find(|c| c.is_hole)
        .expect("the hole")
        .clone();
    assert!((hole.radius - 2.5).abs() < 0.05);
    d.apply(Command::MeshHole {
        body: id,
        hole: Arc::new(hole),
        diameter: 6.0,
    })
    .unwrap();
    let reg = regions::find(&d.mesh(id).unwrap().mesh, 3.0, 4);
    let hole = reg.cylinders.iter().find(|c| c.is_hole).unwrap();
    assert!(
        (hole.radius * 2.0 - 6.0).abs() < 0.05,
        "diameter {}",
        hole.radius * 2.0
    );
}

#[test]
fn a_refused_edit_leaves_the_mesh_as_it_was() {
    let (mut d, id) = doc_with(gen::sphere(10.0, 2000), "ball");
    let before = d.mesh(id).unwrap().mesh.clone();
    let plane = regions::Plane {
        triangles: vec![0],
        normal: [0.0, 0.0, 1.0],
        offset: 0.0,
        area: 1.0,
    };
    let r = d.apply(Command::MeshPull {
        body: id,
        wall: Arc::new(plane),
        distance: 100.0,
    });
    assert!(r.is_err());
    assert!(Arc::ptr_eq(&d.mesh(id).unwrap().mesh, &before));
}

#[test]
fn move_mirror_scale_pattern_and_repair() {
    let (mut d, id) = doc_with(gen::box_mesh(10.0, 20.0, 30.0, 1), "box");
    d.apply(Command::Move {
        bodies: vec![id],
        translation: DVec3::new(100.0, 0.0, 0.0),
        pivot: DVec3::ZERO,
        axis: DVec3::Z,
        angle: 0.0,
        copy: false,
    })
    .unwrap();
    assert!((d.mesh(id).unwrap().mesh.bounding_box().unwrap().0[0] - 100.0).abs() < 1e-4);
    d.apply(Command::MeshScale {
        body: id,
        centre: DVec3::new(100.0, 0.0, 0.0),
        factors: DVec3::new(2.0, 1.0, 0.5),
    })
    .unwrap();
    let (lo, hi) = d.mesh(id).unwrap().mesh.bounding_box().unwrap();
    assert!((hi[0] - lo[0] - 20.0).abs() < 1e-4 && (hi[2] - lo[2] - 15.0).abs() < 1e-4);
    d.apply(Command::Mirror {
        bodies: vec![id],
        origin: DVec3::ZERO,
        normal: DVec3::X,
        keep: true,
        merge: false,
    })
    .unwrap();
    assert_eq!(d.meshes().len(), 2);
    assert!(d.mesh(id).unwrap().mesh.signed_volume() > 0.0);
    assert!(
        d.meshes()[1].mesh.signed_volume() > 0.0,
        "the mirror image is turned inside out again"
    );
    d.apply(Command::MeshPattern {
        body: id,
        count: 3,
        step: DVec3::new(0.0, 50.0, 0.0),
    })
    .unwrap();
    assert_eq!(d.meshes().len(), 4);
    let c = d
        .apply(Command::MeshRepair {
            body: id,
            kind: RepairKind {
                merge: true,
                orient: true,
            },
        })
        .unwrap();
    assert!(c.notice.unwrap().contains("vertices merged"));
}

#[test]
fn the_file_with_a_mesh_is_format_2_and_comes_back_with_everything() {
    let (mut d, id) = doc_with(gen::sphere(10.0, 20_000), "ball");
    d.apply(Command::AddBox {
        name: "B".into(),
        min: DVec3::ZERO,
        max: DVec3::splat(5.0),
    })
    .unwrap();
    d.apply(Command::Rename {
        target: ItemId::Body(id),
        name: "Koule".into(),
    })
    .unwrap();
    d.apply(Command::CreateGroup {
        bodies: vec![id],
        name: "G".into(),
        parent: None,
    })
    .unwrap();
    let bytes = save_to_bytes(&d, None, "test").unwrap();
    let l = load_from_bytes(&bytes).unwrap();
    assert_eq!(l.state.meshes.len(), 1);
    assert_eq!(l.state.meshes[0].name, "Koule");
    assert_eq!(*l.state.meshes[0].mesh, *d.mesh(id).unwrap().mesh);
    assert_eq!(l.state.groups[0].bodies, vec![id]);
    let m: serde_json::Value = {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut z.by_name("manifest.json").unwrap(), &mut s).unwrap();
        serde_json::from_str(&s).unwrap()
    };
    assert_eq!(m["format_version"], FORMAT_WITH_MESHES);
}

#[test]
fn a_file_without_a_mesh_stays_format_1_so_that_older_programs_open_it() {
    let mut d = Document::new();
    d.apply(Command::AddBox {
        name: "B".into(),
        min: DVec3::ZERO,
        max: DVec3::splat(5.0),
    })
    .unwrap();
    let bytes = save_to_bytes(&d, None, "test").unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let mut s = String::new();
    std::io::Read::read_to_string(&mut z.by_name("manifest.json").unwrap(), &mut s).unwrap();
    let m: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(m["format_version"], 1);
}
