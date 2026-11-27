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
        tilt: false,
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
        tilt: false,
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
                split: false,
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

#[test]
fn the_walls_and_holes_after_a_pull_and_a_hole_edit_need_no_new_search() {
    use rolidium_doc::display::{keep_regions, regions_of};
    let (mut d, id) = doc_with(gen::plate_with_hole(60.0, 40.0, 10.0, 2.5, 48), "plate");
    let m0 = d.mesh(id).unwrap().mesh.clone();
    keep_regions(&m0, Arc::new(regions::find(&m0, 3.0, 4)));
    let top = regions_of(&m0)
        .unwrap()
        .planes
        .iter()
        .find(|p| p.normal[2] > 0.99)
        .unwrap()
        .clone();
    d.apply(Command::MeshPull {
        body: id,
        wall: Arc::new(top),
        distance: 5.0,
        tilt: false,
    })
    .unwrap();
    let m1 = d.mesh(id).unwrap().mesh.clone();
    let carried = regions_of(&m1).expect("the regions came with the edit");
    let fresh = regions::find(&m1, 3.0, 4);
    assert_eq!(carried.planes.len(), fresh.planes.len());
    for f in &fresh.planes {
        let c = carried
            .planes
            .iter()
            .find(|c| {
                c.triangles.first() == f.triangles.first()
                    || (c.normal == f.normal && c.triangles.len() == f.triangles.len())
            })
            .expect("the same wall");
        assert!(
            (c.offset - f.offset).abs() < 1e-3,
            "offset {} against {}",
            c.offset,
            f.offset
        );
        assert!(
            (c.area - f.area).abs() < 1e-2,
            "area {} against {}",
            c.area,
            f.area
        );
    }
    let hole = carried.cylinders[0].clone();
    d.apply(Command::MeshHole {
        body: id,
        hole: Arc::new(hole),
        diameter: 7.0,
    })
    .unwrap();
    let m2 = d.mesh(id).unwrap().mesh.clone();
    let carried = regions_of(&m2).expect("and again");
    assert!((carried.cylinders[0].radius - 3.5).abs() < 1e-9);
    // the regions of the state before the edit are still known: Undo needs no search
    assert!(d.undo() && d.undo());
    assert!(regions_of(&d.mesh(id).unwrap().mesh).is_some());
}

#[test]
fn a_pull_with_slanting_neighbours_is_refused_until_it_is_allowed_and_undo_restores_it() {
    use rolidium_mesh::{gen, regions};
    let mut d = Document::new();
    let m = Arc::new(gen::frustum(40.0, 20.0, 20.0));
    d.apply(Command::AddMeshes {
        meshes: vec![NewMesh {
            name: "frustum".into(),
            mesh: m.clone(),
            verified: true,
        }],
    })
    .unwrap();
    let id = d.meshes()[0].id;
    let reg = regions::find(&m, 3.0, 2);
    let top = Arc::new(reg.planes.iter().find(|p| p.normal[2] > 0.99).unwrap().clone());
    let refused = d.apply(Command::MeshPull {
        body: id,
        wall: top.clone(),
        distance: 5.0,
        tilt: false,
    });
    assert!(refused.unwrap_err().to_string().contains("not square"));
    d.apply(Command::MeshPull {
        body: id,
        wall: top.clone(),
        distance: 5.0,
        tilt: true,
    })
    .unwrap();
    let (lo, hi) = d.mesh(id).unwrap().mesh.bounding_box().unwrap();
    assert!((hi[2] - lo[2] - 25.0).abs() < 1e-4);
    d.undo();
    assert!(
        Arc::ptr_eq(&d.mesh(id).unwrap().mesh, &m),
        "Undo gives the very mesh back"
    );
}

#[test]
fn the_display_of_an_edited_mesh_is_patched_and_equals_the_one_made_from_nothing() {
    use rolidium_doc::display::{keep_regions, mesh_body_entry, mesh_inputs, MeshCache};
    use rolidium_mesh::{gen, regions};
    let m = Arc::new(gen::plate_with_holes(4, 3, 10.0, 4.0, 3.0, 24));
    let reg = regions::find(&m, 3.0, 4);
    let top = reg.planes.iter().find(|p| p.normal[2] > 0.99).unwrap().clone();
    let hole = reg.cylinders[3].clone();
    keep_regions(&m, Arc::new(reg));
    let mut d = Document::new();
    d.apply(Command::AddMeshes {
        meshes: vec![NewMesh {
            name: "plate".into(),
            mesh: m.clone(),
            verified: true,
        }],
    })
    .unwrap();
    let id = d.meshes()[0].id;
    let mut cache = MeshCache::default();
    let first = cache.mesh_entries(&mesh_inputs(d.state()));
    for step in 0..2 {
        if step == 0 {
            d.apply(Command::MeshPull {
                body: id,
                wall: Arc::new(top.clone()),
                distance: 2.0,
                tilt: false,
            })
            .unwrap();
        } else {
            d.apply(Command::MeshHole {
                body: id,
                hole: Arc::new(hole.clone()),
                diameter: 7.0,
            })
            .unwrap();
        }
        let inputs = mesh_inputs(d.state());
        let full = mesh_body_entry(&inputs[0]);
        let patched = cache.mesh_entries(&inputs);
        assert_eq!(
            patched[0].mesh.positions, full.mesh.positions,
            "step {step}: the moved vertices"
        );
        assert_eq!(patched[0].mesh.indices, full.mesh.indices);
        for (a, b) in patched[0].faces.iter().zip(full.faces.iter()) {
            assert!(
                (a.centroid - b.centroid).length() < 1e-3 && (a.origin - b.origin).length() < 1e-6,
                "step {step}: the facts of a face"
            );
            assert!((a.bounds.0 - b.bounds.0).length() < 1e-3 && (a.bounds.1 - b.bounds.1).length() < 1e-3);
        }
        assert_ne!(patched[0].stamp, first[0].stamp);
    }
    d.undo();
    d.undo();
    let back = cache.mesh_entries(&mesh_inputs(d.state()));
    assert_eq!(back[0].stamp, first[0].stamp, "Undo finds the display it had");
}
